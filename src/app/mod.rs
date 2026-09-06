mod action;
mod actions;
mod actions_navigation;
mod app_audiobookshelf_service_completion;
mod app_emby_service_completion;
mod app_struct;
mod audio_subtitle_actions;
mod audiobookshelf_book_modal_actions;
mod audiobookshelf_browse_actions;
mod audiobookshelf_podcast_modal_actions;
mod audiobookshelf_service_actions;
mod bootstrap;
mod browse_level_actions;
mod cast_actions;
mod cast_reattach;
mod cast_status_actions;
pub mod components;
mod construct;
mod consume_quit_actions;
mod context_menu_actions;
mod cw_library_tab_actions;
mod daemon_restart;
mod emby_service_actions;
mod feed_actions;
mod feed_parse;
mod feed_parse_date;
mod feed_tab_actions;
mod feeds_manage_actions;
mod home_actions;
pub(crate) mod images;
mod input;
mod input_browse_dispatch;
mod input_confirm_keys;
mod input_lib_keys;
mod input_playlist_keys;
mod input_queue_keys;
mod input_resolver;
mod input_search_sidebar_keys;
mod key_policy;
pub(crate) mod layout;
mod lib_cursor_actions;
mod lib_event_actions;
mod lib_event_actions_reconcile;
mod library_browse_actions;
mod library_column_width;
mod library_load_actions;
mod library_position_state;
mod library_route;
mod library_search_actions;
mod mouse_gestures;
mod music_actions;
mod music_grouping;
mod notify_actions;
pub(crate) mod palette;
mod panel_focus_state;
mod panel_targets;
mod playback_target;
mod playback_target_cast;
mod playback_target_local;
mod playback_target_remote;
mod player_event;
mod queue_actions;
mod queue_column_width;
mod queue_scope;
mod remote_slot_state;
pub mod render;
mod render_cadence;
mod resize;
mod router;
mod run_loop_drains;
mod run_loop_events;
mod search_sidebar;
mod selection_modal_actions;
mod series_modal_actions;
mod service_startup;
mod services_settings;
mod session_command_actions;
mod session_connect;
mod session_switch;
mod settings;
mod shared_sync;
mod shell_draw;
mod shuffle_folder_actions;
mod types_audiobookshelf_browse;
mod types_browse;
mod types_cast;
mod types_confirm;
mod types_context_menu;
mod types_daemon_lost;
mod types_events;
mod types_feed;
mod types_feed_tab;
mod types_feeds_manage;
mod types_library_tab;
mod types_overlay;
mod types_playback;
mod types_player_tab;
mod types_selection_modal;
mod types_settings;
mod types_sidebar;
mod types_tab_selection;
pub(crate) mod ui_util;
mod visualizer;
mod visualizer_worker;
mod ws_event_actions;

pub use self::app_struct::App;
mod shell;
mod shell_audiobookshelf_book;
mod shell_audiobookshelf_podcast;
mod shell_browser;
mod shell_destination_mounts;
mod shell_feeds;
mod shell_feeds_manage;
mod shell_home;
mod shell_home_content;
mod shell_inline_search;
mod shell_library;
mod shell_modal_actions;
mod shell_music_workspace;
mod shell_overlays;
mod shell_playback;
mod shell_playlists;
mod shell_queue;
mod shell_root;
mod shell_settings;
mod shell_tv_workspace;
pub use self::shell::Model;
mod app_init;
use self::app_init::AppInit;
use self::bootstrap::{bootstrap_local_daemon_queue, bootstrap_unified_queue};
use self::notify_actions::ToastSeverity;
use self::resize::spawn_resize_worker;
use self::types_browse::{
    restore_library_position, AlbumIndexState, AlbumPathPart, AlbumSearchEntry, BrowseLevel,
    SeriesDetail,
};
use self::types_confirm::{ConfirmAction, ConfirmModal};
#[cfg(test)]
use self::types_context_menu::LibraryRoutePopup;
use self::types_context_menu::{
    ContextAction, ContextMenuAnchor, ContextMenuEntry, LibraryRouteStage, MultiSelectKind,
};
use self::types_daemon_lost::DaemonLostModal;
use self::types_events::{LibEvent, ReconciliationCommand, SessionEvent};
use self::types_feed::{
    FeedHomeVideoGroup, FeedHomeVideoState, IdleFeed, SavePlaylistDialog, SavePlaylistStage,
};
use self::types_library_tab::LibraryTab;
use self::types_playback::{
    CastPlaybackTarget, HomeLatestSource, LocalPlaybackTarget, PendingQueueAction, PlaybackState,
    PlaybackTarget, QueueCursorPush, QueueScope, QueueScopeResolution, RemotePlaybackTarget,
    RemoteReanchorPopup, RemoteSlotState, SuspendedLocalSession, UndoEntry,
};
use self::types_player_tab::PlayerTab;
use self::types_selection_modal::{
    SelectionModal, SelectionModalFilter, SelectionModalListState, SelectionModalRow,
    SelectionModalSource,
};
use self::types_settings::{PanelFocus, PanelMode, SettingKey, SETTING_SECTIONS};
pub(crate) use self::types_sidebar::SidebarId;
use self::types_tab_selection::TabSelection;
use mbv_core::api::EmbyClient;
use std::sync::atomic::{AtomicBool, Ordering};
#[cfg(test)]
use std::sync::{mpsc, Arc, Mutex};
use std::time::{Duration, Instant};

static QUIT_REQUESTED: AtomicBool = AtomicBool::new(false);
// Set only by SIGHUP or stdin POLLHUP (terminal vanished). Never set by q/SIGTERM.
// The watchdog's forced exit arms only on this flag so clean q-quits are never raced.
static TERMINAL_GONE: AtomicBool = AtomicBool::new(false);

#[cfg(test)]
type DirectConnectFn = fn(
    &mbv_core::remote_player::DaemonEndpoint,
) -> Result<
    (
        mbv_core::remote_player::RemotePlayer,
        mpsc::Receiver<PlayerEvent>,
    ),
    String,
>;

#[cfg(test)]
static DIRECT_CONNECT_OVERRIDE: Mutex<Option<DirectConnectFn>> = Mutex::new(None);

// Separate from DIRECT_CONNECT_OVERRIDE above (Sessions-panel "Direct
// Remote" upgrade, keyed off a discovered SessionInfo): this is issue
// #222's lazy daemon-route connect primitive, targeting a statically
// configured DaemonEndpoint with no session discovery. Kept as its own
// override/lock pair so the two connect paths -- and the App state they
// eventually drive (`connected_session_id`/`direct_remote_label` vs. a
// future #223 `active_route`) -- stay independently testable and are
// never conflated, per #223's explicit "must not be conflated" rule.
#[cfg(test)]
static DAEMON_ROUTE_CONNECT_OVERRIDE: Mutex<Option<DirectConnectFn>> = Mutex::new(None);
#[cfg(test)]
static DAEMON_ROUTE_CONNECT_TEST_LOCK: Mutex<()> = Mutex::new(());

// Test seam for live-session-list lookups, mirroring
// DAEMON_ROUTE_CONNECT_OVERRIDE/_TEST_LOCK above: lets tests inject a fake
// session list without a real network call. Shared by
// `try_auto_reconnect`'s `DirectSession` lookup (#236) and the F2
// "Library Routes" device picker (`enter_device_stage`, #256).
#[cfg(test)]
type SessionsLoadFn =
    fn(&mbv_core::api::EmbyClient) -> Result<Vec<mbv_core::api::SessionInfo>, String>;
#[cfg(test)]
static SESSIONS_LOAD_OVERRIDE: Mutex<Option<SessionsLoadFn>> = Mutex::new(None);
#[cfg(test)]
static SESSIONS_LOAD_TEST_LOCK: Mutex<()> = Mutex::new(());

// Test seam for `App::connect_cast_receiver`'s resolve-and-connect step
// (7.3/7.5), mirroring the overrides above: lets tests substitute a fake
// worker instead of a real mDNS browse + `CastClient::connect`, without
// making the reattach/attach-on-selection call sites themselves
// test-aware.
#[cfg(test)]
type CastConnectFn = fn(&str, Duration) -> Result<mpsc::Sender<types_cast::CastJob>, String>;
#[cfg(test)]
static CAST_CONNECT_OVERRIDE: Mutex<Option<CastConnectFn>> = Mutex::new(None);
#[cfg(test)]
static CAST_CONNECT_TEST_LOCK: Mutex<()> = Mutex::new(());

pub(super) const LEFT_WIDTH_DEFAULT: u16 = 40;
pub(super) const LEFT_WIDTH_STEP: u16 = 5;
/// The single wide/narrow breakpoint. Minimum list-pane / Home-pane width at
/// which the view switches to a two-column layout. Every screen and
/// arrangement reads this one constant instead of testing width itself; the
/// library list's column count derives from it (`library_column_count`), not
/// the other way around.
pub(super) const TWO_COLUMN_THRESHOLD: u16 = 82;
/// The narrow-terminal breakpoint below which the Power View uses the
/// two-state "mini view" (`x` toggles library-only <-> queue-only) instead of
/// the three-state both/queue-only/library-only cycle. Independent of and
/// unrelated to `TWO_COLUMN_THRESHOLD` (82), which governs the library
/// panel's internal list-column layout (see design.md).
pub(super) const MINI_VIEW_THRESHOLD: u16 = 80;
/// Left margin for the tab row. The control pill used to live here (hence
/// the old, larger reservation); it now renders in the status bar (see
/// `render_status_bar`) and the tabs are left-aligned flush with the left
/// edge instead.
pub(super) const TABBAR_LEFT_RESERVE: u16 = 0;

extern "C" fn handle_quit_signal(signum: i32) {
    let name = match signum {
        1 => "SIGHUP",
        15 => "SIGTERM",
        _ => "unknown",
    };
    // SAFETY: log::info is not async-signal-safe, but we only reach this
    // from SIGTERM/SIGHUP where the process is about to exit anyway;
    // a worst-case torn write is acceptable for diagnostics.
    eprintln!("mbv: received {name} (signal {signum}), requesting quit");
    QUIT_REQUESTED.store(true, Ordering::Relaxed);
    if signum == 1 {
        // SIGHUP — terminal closed
        TERMINAL_GONE.store(true, Ordering::Relaxed);
    }
}

fn install_signal_handlers() {
    extern "C" {
        fn signal(signum: i32, handler: unsafe extern "C" fn(i32)) -> usize;
    }
    unsafe {
        signal(1, handle_quit_signal); // SIGHUP — terminal closed
        signal(15, handle_quit_signal); // SIGTERM — process termination
    }
}

// Returns true if stdin (fd 0) has POLLHUP — the PTY master was closed.
fn stdin_has_hup() -> bool {
    let mut pfd = libc::pollfd {
        fd: 0,
        events: 0,
        revents: 0,
    };
    unsafe { libc::poll(&mut pfd, 1, 0) > 0 && (pfd.revents & libc::POLLHUP as libc::c_short) != 0 }
}

// Watchdog thread: detects terminal close (SIGHUP or stdin POLLHUP) and
// ensures the mpv window closes and the process exits even when the main event
// loop is wedged in a blocking crossterm epoll call (which SA_RESTART prevents
// SIGHUP from interrupting). Calls player stop directly — bypassing the event
// loop — so the mpv window closes within one wait_event(0.5) tick. The player
// thread then reports stopped to Emby on its own. Force-exits after 15s as a
// backstop for hung Emby HTTP calls.
//
// The forced exit is gated on TERMINAL_GONE (set only by SIGHUP/stdin POLLHUP),
// never on QUIT_REQUESTED alone. A clean q-quit sets QUIT_REQUESTED but not
// TERMINAL_GONE, so the watchdog stops mpv but never races report_stopped.
fn start_quit_watchdog(quit_handle: Option<mbv_core::player::QuitHandle>, quit_timeout: Duration) {
    std::thread::spawn(move || {
        loop {
            std::thread::sleep(Duration::from_millis(50));
            let hup = stdin_has_hup();
            if hup {
                TERMINAL_GONE.store(true, Ordering::Relaxed);
            }
            if TERMINAL_GONE.load(Ordering::Relaxed) || QUIT_REQUESTED.load(Ordering::Relaxed) {
                QUIT_REQUESTED.store(true, Ordering::Relaxed);
                if let Some(ref h) = quit_handle {
                    h.stop_for_shutdown(quit_timeout);
                }
                if TERMINAL_GONE.load(Ordering::Relaxed) {
                    std::thread::sleep(Duration::from_secs(15));
                    std::process::exit(0);
                }
                return; // clean quit — let the main thread finish report_stopped
            }
        }
    });
}

use ratatui::{backend::CrosstermBackend, Terminal};

#[cfg(test)]
use mbv_core::api::EmbyItem;
#[cfg(test)]
use mbv_core::playback_queue::RemoveSlotResult;
#[cfg(test)]
use mbv_core::player::PlayerEvent;

const PAGE_SIZE: usize = 100;
const PREFETCH_AHEAD: usize = 25;
const SESSIONS_PANEL_W: u16 = 40;
const HELP_PANEL_W: u16 = 40;
const SETTINGS_PANEL_W: u16 = 40;
#[cfg(test)]
const PLAYLISTS_PANEL_W: u16 = 40;
const SEARCH_PANEL_W: u16 = 40;
impl App {
    pub(super) fn spawn_search_sidebar_query(&self, client: EmbyClient, query: String) {
        let tx = self.search_tx.clone();
        std::thread::spawn(move || {
            let result = client.search_items(&query, 100);
            let _ = tx.send((query, result));
        });
    }
}

fn init_terminal() -> Result<Terminal<CrosstermBackend<std::io::Stdout>>, Box<dyn std::error::Error>>
{
    crossterm::terminal::enable_raw_mode()?;
    let mut stdout = std::io::stdout();
    crossterm::execute!(stdout, crossterm::terminal::EnterAlternateScreen)?;
    crossterm::execute!(stdout, crossterm::event::EnableMouseCapture)?;
    crossterm::execute!(stdout, crossterm::event::EnableFocusChange)?;
    let _ = crossterm::execute!(
        stdout,
        crossterm::event::PushKeyboardEnhancementFlags(
            crossterm::event::KeyboardEnhancementFlags::DISAMBIGUATE_ESCAPE_CODES
        )
    );
    Ok(Terminal::new(CrosstermBackend::new(stdout))?)
}

fn restore_terminal(
    mut terminal: Terminal<CrosstermBackend<std::io::Stdout>>,
) -> Result<(), Box<dyn std::error::Error>> {
    crossterm::terminal::disable_raw_mode()?;
    let _ = crossterm::execute!(
        terminal.backend_mut(),
        crossterm::event::PopKeyboardEnhancementFlags
    );
    crossterm::execute!(
        terminal.backend_mut(),
        crossterm::event::DisableMouseCapture
    )?;
    crossterm::execute!(terminal.backend_mut(), crossterm::event::DisableFocusChange)?;
    crossterm::execute!(
        terminal.backend_mut(),
        crossterm::terminal::LeaveAlternateScreen
    )?;
    terminal.show_cursor()?;
    Ok(())
}

include!("app_test_modules.rs");
