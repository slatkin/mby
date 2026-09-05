use super::images;
use super::layout;
use super::panel_targets::PanelTarget;
use super::render;
use super::resize::{ResizeRegisterTx, ResizeResponseRx};
use super::types_browse::{AlbumIndexState, SeriesDetail};
use super::types_cast::{CastAttachment, CastEvent};
use super::types_confirm::ConfirmModal;
use super::types_events::{LibEvent, SessionEvent};
use super::types_feed::IdleFeed;
use super::types_feed::SavePlaylistDialog;
use super::types_feed_tab::FeedTabState;
use super::types_library_tab::LibraryTab;
use super::types_playback::{
    PendingQueueAction, PlayheadProjection, PlaylistMutationState, QueueScope,
    RemoteQueueProjection, SuspendedLocalSession, UndoEntry,
};
use super::types_player_tab::PlayerTab;
use super::types_settings::{PanelFocus, PanelMode, SettingsDestination};
use super::types_tab_selection::TabSelection;
use super::visualizer_worker::{PipeWireWorker, StereoSampleWindow};
use super::SidebarId;
use mbv_core::api::EmbyItem;
use mbv_core::playback_queue::QueueSlotId;
use mbv_core::player::{PlayerEvent, PlayerProxy};
use mbv_core::service_runtime::{AudiobookshelfRuntime, EmbyRuntime};
use mbv_core::ws::WsEvent;
use ratatui_image::picker::Picker;
use std::sync::mpsc;
use std::time::Instant;

pub struct App {
    /// General application configuration is independent of the optional Emby
    /// runtime. Feed management reads and mutates this context directly.
    pub(super) config: std::sync::Arc<std::sync::Mutex<crate::config::Config>>,
    pub(super) emby_runtime: EmbyRuntime,
    pub(super) audiobookshelf_runtime: AudiobookshelfRuntime,
    pub(super) emby_startup_rx: Option<super::service_startup::StartupReceiver>,
    pub(super) emby_startup_request: Option<(
        crate::config::Config,
        mbv_core::service_runtime::SetupGeneration,
    )>,
    pub(super) audiobookshelf_startup_rx:
        Option<super::service_startup::AudiobookshelfStartupReceiver>,
    pub(super) audiobookshelf_startup_request: Option<(
        crate::config::Config,
        mbv_core::service_runtime::SetupGeneration,
    )>,
    pub(super) audiobookshelf_catalog_rx:
        Option<super::service_startup::AudiobookshelfCatalogReceiver>,
    pub(super) audiobookshelf_libraries: Vec<mbv_core::audiobookshelf::AudiobookshelfLibrary>,
    /// Most-recent `Newest Episodes` shelf per podcast library (async shelf
    /// fetch, Task 6.2), keyed by library id. `fetch_home()` rebuilds Home's
    /// Audiobookshelf Latest pills from this cache — never a blocking network
    /// call — and the shelf-fetch handler refreshes it (Task 6.3).
    pub(super) audiobookshelf_shelf_cache:
        std::collections::HashMap<String, Vec<mbv_core::playback_queue::QueueItem>>,
    pub(super) audiobookshelf_browse:
        Vec<super::types_audiobookshelf_browse::AudiobookshelfBrowseState>,
    pub(super) audiobookshelf_book_browse:
        Vec<super::types_audiobookshelf_browse::AudiobookshelfBookBrowseState>,
    pub(super) audiobookshelf_test_rx:
        Option<super::service_startup::AudiobookshelfStartupReceiver>,
    pub(super) audiobookshelf_setup_rx:
        Option<std::sync::mpsc::Receiver<super::service_startup::AudiobookshelfSetupCompletion>>,
    pub(super) emby_setup_form: Option<super::services_settings::EmbySetupForm>,
    pub(super) audiobookshelf_setup_form: Option<super::services_settings::AudiobookshelfSetupForm>,
    pub(super) emby_setup_rx: Option<mpsc::Receiver<super::service_startup::SetupCompletion>>,
    pub(super) pending_emby_replacement: Option<super::service_startup::Startup>,
    pub(super) pending_audiobookshelf_replacement:
        Option<super::service_startup::AudiobookshelfPendingReplacement>,
    pub(super) shared_client: Option<mbv_core::shared_client::SharedClient>,
    pub(super) shared_reconnect_rx: Option<
        mpsc::Receiver<
            Result<
                (
                    mbv_core::shared_client::SharedClient,
                    mbv_core::shared_state::SharedSnapshotResponse,
                ),
                String,
            >,
        >,
    >,
    pub(super) player: PlayerProxy,
    /// Handle to the live MPRIS D-Bus registration, if one was started for
    /// this session (`App::new` / `App::new_remote` both start one; test
    /// construction via `build()` does not). `None` in tests so they never
    /// spin up a real D-Bus connection.
    ///
    /// `switch_to_direct_remote` and `restore_local_mode` call
    /// `mpris::rebind` on this whenever they swap `player` between a local
    /// `Player` and a `RemotePlayer` (#175): MPRIS must always publish
    /// whichever one currently owns playback, not whatever was live when
    /// the D-Bus service was first registered.
    pub(super) mpris: Option<crate::mpris::MprisHandle>,
    pub(super) player_rx: mpsc::Receiver<PlayerEvent>,
    pub(super) ws_rx: mpsc::Receiver<WsEvent>,
    pub(super) audiobookshelf_socket_rx:
        mpsc::Receiver<mbv_core::audiobookshelf_socket::SocketEvent>,
    pub(super) audiobookshelf_socket_tx: Option<mpsc::Sender<()>>,
    pub(super) audiobookshelf_socket_generation: Option<mbv_core::service_runtime::SetupGeneration>,
    pub(super) libs: Vec<LibraryTab>,
    pub(super) player_tab: PlayerTab,
    pub(super) remote_player_tab: Option<PlayerTab>,
    pub(super) status: String,
    pub(super) status_expires: Option<Instant>,
    pub(super) status_severity: super::notify_actions::ToastSeverity,
    /// `true` only for instances built via `App::new_remote` (the
    /// `--connect-daemon` / local-daemon-auto-detect thin-client launch
    /// path). Those instances never populate `active_route` or
    /// `connected_session_state` (those are set by runtime library-route
    /// switches / session attaches that only apply to `App::new` instances),
    /// so `teardown`'s auto-reconnect persistence (#236) must skip this flag
    /// entirely rather than compute (and save) a bogus `None` record that
    /// would wipe out a real record saved by a different `App::new` session.
    pub(super) launched_as_remote: bool,
    /// The daemon endpoint for the current player target. `None` means an
    /// in-process player (bare mode). `Some(DaemonEndpoint::Local)` is this
    /// machine's managed local daemon. `Some(Tcp | Unix)` is a different
    /// daemon. Replaces the mutable `is_local_daemon` boolean so every
    /// transition records its source of truth rather than projecting it
    /// down to a bool that must be manually kept in sync.
    pub(super) player_endpoint: Option<mbv_core::remote_player::DaemonEndpoint>,
    /// The one-time, launch-time launch classification: `true` only for
    /// `App::new_remote` instances constructed for the managed local
    /// daemon, and never updated afterward. Kept independent of
    /// `player_endpoint`, which tracks the *current* player target and can
    /// change at runtime. Kept fixed at its construction-time value so
    /// `restore_local_mode` can tell whether this app's baseline (the state
    /// to return to when a route switch is undone) was a genuinely local
    /// in-process player (nothing to do here) or a connection to the local
    /// daemon (which must be reconnected, since there's no suspended local
    /// player to restore in that case).
    pub(super) home_is_local_daemon: bool,
    pub(super) hidden_libraries: Vec<String>,
    pub(super) hidden_latest: Vec<String>,
    /// `Config.library_routes` at startup (#256). Values are resolved
    /// `tcp://host:port` endpoints, read directly with no live-session
    /// lookup -- see `mbv_core::config::resolve_library_route`.
    pub(super) library_routes: std::collections::HashMap<String, String>,
    pub(super) music_levels: Vec<String>,
    pub(super) album_indexes: std::collections::HashMap<String, AlbumIndexState>,
    // Per-frame layout geometry from last render, used for mouse hit-testing.
    // See src/app/layout.rs for the grouping rationale.
    pub(super) layout: layout::AppLayout,
    pub(super) terminal_width: u16,
    pub(super) terminal_height: u16,
    pub(super) last_space_press: Option<Instant>,
    pub(super) last_esc_press: Option<Instant>,
    /// Shell handoff for a modal raised by App-owned effects. The mounted
    /// component owns the modal after the next Model tick.
    pub(super) pending_overlay: Option<super::types_overlay::OverlayRequest>,
    /// Set right before requesting a clean exit on an announced daemon
    /// shutdown (task 7.2); printed once by `run()` after the terminal is
    /// restored, since anything written while still in the alternate screen
    /// would never be visible. `None` on every other exit path.
    pub(super) pending_exit_message: Option<String>,
    pub(super) pending_delete_slot: Option<QueueSlotId>, // marks a delete that was already applied optimistically, so the Stopped handler doesn't re-derive it
    pub(super) pending_queue_removal: Option<(QueueSlotId, bool)>, // deferred removal (slot, is_audio) after TrackChanged index-shifts
    pub(super) queue_undo_stack: Vec<UndoEntry>,
    pub(super) remote_queue_undo_stack: Vec<UndoEntry>,
    pub(super) pending_remote_move_cursor: Option<usize>,
    /// The display cursor a just-issued local queue edit (e.g. remove) wants
    /// the next `UnifiedQueueUpdated` broadcast to land on, since the daemon's
    /// state tracks *playback* position, not the UI selection — see
    /// `remove_from_queue` and `PlayerEvent::UnifiedQueueUpdated`.
    pub(super) pending_queue_edit_cursor: Option<usize>,
    /// Single source of truth for the playback playhead: active scope/slot,
    /// position/runtime, `Confirmed | Predicted(reason)` confidence, and the
    /// one-shot scoped `queue_cursor` push for the next `sync_queue`. Folds in
    /// the former `pending_active_idx` and `queue_cursor_pushed`.
    pub(super) playhead: PlayheadProjection,
    pub(super) next_up_item: Option<EmbyItem>,
    // Main UI scalars.
    // reuses shared self.libs.
    pub(super) panel_focus: PanelFocus,
    // Ephemeral narrow-terminal (< MINI_VIEW_THRESHOLD columns) two-state
    // toggle target: Library ⇄ Queue. Never read from or written to prefs;
    // tracked independently so wide-mode `panel_mode`/`panel_focus` stay
    // untouched while narrow. Defaults to Queue.
    pub(super) mini_view_focus: PanelFocus,
    pub(super) tab: TabSelection, // which left-panel tab is active
    pub(super) queue_column_width: u16,
    pub(super) panel_mode: PanelMode,
    pub(super) library_tab_pending: usize, // restored from prefs; applied once libs have loaded
    pub(super) last_played_item_id: Option<String>,
    pub(super) last_played_completed: bool,
    pub(super) card_image_states: std::collections::HashMap<String, images::CachedImage>,
    pub(super) image_lru: std::collections::VecDeque<String>,
    pub(super) image_cache_size: usize,
    pub(super) card_image_loading: std::collections::HashSet<String>,
    pub(super) last_card_height: u16,
    pub(super) last_card_width: u16,
    pub(super) pending_image_fetches: std::collections::VecDeque<images::ImageFetchReq>,
    pub(super) image_fetches_active: usize,
    pub(super) card_image_tx: mpsc::Sender<(String, Option<image::DynamicImage>)>,
    pub(super) card_image_rx: mpsc::Receiver<(String, Option<image::DynamicImage>)>,
    /// Registers a freshly created per-cache-key `ResizeRequest` receiver
    /// with the resize worker thread (see `spawn_resize_worker`), so the
    /// worker can service many concurrently-alive `ThreadProtocol`s off the
    /// render thread while still routing each `ResizeResponse` back to the
    /// right `card_image_states` entry (#164). `ResizeRequest`/`ResizeResponse`
    /// carry no key of their own — that's why each cache key gets its own
    /// dedicated channel instead of sharing one globally.
    pub(super) resize_register_tx: ResizeRegisterTx,
    /// Completed off-thread resize+encode results, tagged with the
    /// `card_image_states` cache key they belong to. Drained once per
    /// event-loop tick alongside `card_image_rx` (#164).
    pub(super) resize_response_rx: ResizeResponseRx,
    pub(super) image_picker: Option<Picker>,
    pub(super) halfblock_picker: Option<Picker>,
    pub(super) dim_backdrop_active: bool,
    pub(super) image_cache_size_total: usize,
    pub(super) settings_destination: SettingsDestination,
    pub(super) settings_save_at: Option<Instant>,
    pub(super) confirm_logout: bool,
    pub(super) system_notifications: bool,
    pub(super) notif_failed: bool,
    pub(super) notif_action_tx: mpsc::Sender<String>,
    pub(super) notif_action_rx: mpsc::Receiver<String>,
    pub(super) lib_tx: mpsc::Sender<LibEvent>,
    pub(super) lib_rx: mpsc::Receiver<LibEvent>,
    pub(super) search_tx: mpsc::Sender<(String, Result<Vec<EmbyItem>, String>)>,
    pub(super) search_rx: mpsc::Receiver<(String, Result<Vec<EmbyItem>, String>)>,
    /// Whether the global Search sidebar overlay is open. The
    /// `SearchSidebarComponent` owns the sidebar state (query, cursor, scroll,
    /// results, debounce); this flag tells the legacy render/input path the
    /// overlay is active (task 3.2).
    pub(super) sessions: Vec<mbv_core::api::SessionInfo>,
    /// Last cast discovery browse result (8.1), independent of `sessions`'s
    /// own reload cadence -- see `panel_targets::build_panel_targets`.
    pub(super) cast_receivers: Vec<mbv_core::cast_discovery::CastReceiver>,
    /// The F3 panel's merged Emby+Cast target list, rebuilt from `sessions`/
    /// `cast_receivers` by `App::rebuild_panel_targets` (8.1/8.2).
    pub(super) panel_targets: Vec<PanelTarget>,
    pub(super) sessions_loading: bool,
    pub(super) playlists: Vec<EmbyItem>,
    pub(super) playlists_cursor: usize,
    pub(super) playlists_scroll: usize,
    pub(super) playlists_loading: bool,
    pub(super) playlists_open: Option<EmbyItem>, // playlist currently being browsed
    pub(super) playlists_open_items: Vec<EmbyItem>,
    pub(super) playlists_open_cursor: usize,
    pub(super) playlists_open_scroll: usize,
    pub(super) playlists_open_loading: bool,
    pub(super) queue_source: crate::config::QueueSource,
    pub(super) queue_dirty: bool,
    pub(super) pending_queue_action: Option<PendingQueueAction>,
    pub(super) use_nerd_fonts: bool,
    pub(super) indicator_style: render::indicators::IndicatorStyle,
    pub(super) ws_send_tx: Option<mbv_core::ws::WsSender>,
    pub(super) last_keepalive: Instant,
    pub(super) last_capabilities: Instant,
    pub(super) sessions_tx: mpsc::Sender<SessionEvent>,
    pub(super) sessions_rx: mpsc::Receiver<SessionEvent>,
    pub(super) connected_session_id: Option<String>,
    pub(super) connected_session_state: Option<mbv_core::api::SessionInfo>,
    /// Cast attachment, beside `connected_session_id`/`connected_session_state`
    /// above: `None` means no cast target is attached. See `cast_actions.rs`
    /// for attach/detach and `cast_status_actions.rs` for status polling.
    pub(super) cast_attachment: Option<CastAttachment>,
    pub(super) cast_tx: mpsc::Sender<CastEvent>,
    pub(super) cast_rx: mpsc::Receiver<CastEvent>,
    pub(super) last_cast_poll: Instant,
    pub(super) cast_status_loading: bool,
    pub(super) remote_tracker: Option<mbv_core::remote_reconciliation::ReconciliationTracker>,
    pub(super) remote_queue_projection: Option<RemoteQueueProjection>,
    pub(super) remote_queue_lineage: u64,
    pub(super) playlist_mutations: std::collections::HashMap<String, PlaylistMutationState>,
    pub(super) next_playlist_mutation: u64,
    pub(super) session_poll_generation: u64,
    pub(super) direct_remote_connected: bool,
    pub(super) direct_remote_label: Option<String>,
    pub(super) last_session_poll: Instant,
    pub(super) session_miss_count: u8, // consecutive polls that didn't find the connected session
    pub(super) remote_pos_s: i64,      // monotonic position estimate for the connected remote
    pub(super) remote_pos_at: Instant, // when remote_pos_s was last anchored
    pub(super) remote_api_pos_advanced_at: Instant, // last time the API position actually moved forward
    pub(super) remote_stalled_while_paused: bool, // last API poll observed IsPaused=true with no position advance
    pub(super) remote_seek_pending_until: Instant, // suppress poll pos-reconcile after a seek
    pub(super) runtime_zero_since: Option<Instant>, // when runtime_s first became 0 for the current item (fast-poll cap)
    pub(super) suspended_local: Option<SuspendedLocalSession>,
    /// The library route currently driving playback, if any (#223):
    /// `Some(name)` holds the lowercased library name whose configured
    /// daemon is the active player target. `None` means local playback,
    /// or a Sessions-panel direct remote (`connected_session_id` /
    /// `direct_remote_label`) -- a separate concept, never conflated with
    /// this one. Fixed for the life of the current queue: a *new* queue
    /// re-evaluates it (see `apply_route_for_playback`), but enqueuing
    /// into the existing queue must match it or be rejected (see
    /// `enqueue_route_conflict`).
    pub(super) active_route: Option<String>,
    /// Per-item cache of ancestor-lookup library-route resolution for
    /// cross-library aggregate views (Continue Watching/Next Up,
    /// Favorites), keyed by item id. `Some(name)` = resolved to that
    /// library (lowercased); `None` = resolved, no owning library route.
    /// Avoids a repeat `get_ancestors` round-trip for the same item
    /// within a session (#223). Each entry also carries the `Instant` it
    /// was cached at, so a mid-session library reorganization on the
    /// Emby server self-heals after `LIBRARY_ROUTE_CACHE_TTL` instead of
    /// requiring an app restart (#223, post-grilling revision item 5).
    pub(super) library_route_cache: std::collections::HashMap<String, (Option<String>, Instant)>,
    pub(super) force_clear: bool,
    pub(super) tab_scroll: usize,
    pub(super) ui_volume: u8,
    pub(super) pre_mute_volume: Option<u8>,
    pub(super) mute_on: bool,
    pub(super) visualizer_enabled: bool,
    pub(super) visualizer_failed: bool,
    pub(super) visualizer: Option<PipeWireWorker>,
    pub(super) visualizer_window: StereoSampleWindow,
    pub(super) visualizer_glyph: String,
    pub(super) now_playing_throbber_index: usize,
    pub(super) last_throbber_advance: std::time::Instant,
    /// Text and start time of the shared marquee clock, reset whenever the
    /// tracked text changes so a new string always starts its scroll from
    /// the beginning rather than mid-cycle. Used by all marquee callers
    /// (mini-view "On Now", standard title row, idle feed title).
    pub(super) marquee_text: String,
    pub(super) marquee_started_at: std::time::Instant,
    pub(super) last_nav_at: Instant,
    pub(super) last_library_nav_at: Instant,
    /// Set once `library_position_state` has an unflushed in-memory change.
    /// The disk write + shared-document sync are debounced off this rather
    /// than run synchronously on every cursor move -- see
    /// `save_default_library_position`'s doc comment.
    pub(super) library_position_dirty: bool,
    pub(super) library_position_dirty_at: Instant,
    /// Tracks terminal focus and arms a grace window to swallow the
    /// click that merely brings the window into focus.
    ///
    /// State transitions:
    /// - `None`: terminal is unfocused (or never reported focus).
    ///   All mouse button/scroll events are suppressed.
    /// - `Some(Instant)`: terminal is focused; the Instant records
    ///   when `FocusGained` was seen.  A click within `REFOCUS_WINDOW`
    ///   of that Instant is the refocusing click and is suppressed;
    ///   after the window expires the field stays `Some` so
    ///   subsequent clicks dispatch normally until the next
    ///   `FocusLost`.
    pub(super) refocus_at: Option<Instant>,
    pub(super) album_artist_cache: std::collections::HashMap<String, String>,
    pub(super) album_artist_loading: std::collections::HashSet<String>,
    pub(super) pending_album_artist_fetches: std::collections::VecDeque<String>,
    pub(super) album_artist_fetches_active: usize,
    /// Track lists for the album currently highlighted in the
    /// album-folder listing, fetched proactively so the inline album detail
    /// pane (#145) has data without requiring the user to drill in first.
    /// Keyed by album id, mirroring `album_artist_cache`'s never-evicted
    /// lifetime.
    pub(super) album_tracks_cache: std::collections::HashMap<String, Vec<EmbyItem>>,
    pub(super) album_tracks_loading: std::collections::HashSet<String>,
    /// TV series detail cache for inline rendering.
    /// When a Series is selected, we proactively fetch seasons and episodes
    /// so the inline detail pane can render without drilling in.
    pub(super) series_detail_cache: std::collections::HashMap<String, SeriesDetail>,
    pub(super) series_detail_loading: std::collections::HashSet<String>,
    pub(super) series_season_loading: std::collections::HashSet<(String, String)>,
    pub(super) image_protocol: Option<String>,
    pub(super) image_protocol_enabled: bool,
    pub(super) library_position_state: crate::config::LibraryPositionState,
    pub(super) queue_scope: QueueScope,
    pub(super) idle_feed: Option<IdleFeed>,
    pub(super) feed_tab: FeedTabState,
    /// When a seek was issued during Feed playback, the slot_id is stored
    /// here. The next `OutputStarted` clears it and persists the resulting
    /// position. This prevents ordinary output restarts (buffering,
    /// startup) from becoming state writes.
    pub(super) feed_seek_pending_slot: Option<mbv_core::playback_queue::QueueSlotId>,
    #[cfg(test)]
    pub(super) _test_state_dir_guard: Option<crate::config::TestStateDirGuard>,
}

impl App {
    pub(super) fn request_sidebar_open(&mut self, sidebar: SidebarId) {
        self.pending_overlay = Some(super::types_overlay::OverlayRequest::OpenSidebar(sidebar));
    }

    pub(super) fn request_sidebar_dismiss(&mut self, sidebar: SidebarId) {
        self.pending_overlay = Some(super::types_overlay::OverlayRequest::DismissSidebar(
            sidebar,
        ));
    }

    pub(super) fn request_sidebar_toggle(&mut self, sidebar: SidebarId) {
        self.pending_overlay = Some(super::types_overlay::OverlayRequest::ToggleSidebar(sidebar));
    }

    pub(super) fn ask_confirm(&mut self, modal: ConfirmModal) {
        self.pending_overlay = Some(super::types_overlay::OverlayRequest::Confirm(modal));
    }

    pub(super) fn open_save_playlist_dialog(&mut self, dialog: SavePlaylistDialog) {
        self.pending_overlay = Some(super::types_overlay::OverlayRequest::SavePlaylist(dialog));
    }

    pub(super) fn dismiss_confirm(&mut self) {
        self.pending_overlay = Some(super::types_overlay::OverlayRequest::DismissConfirm);
    }

    pub(super) fn dismiss_daemon_lost(&mut self) {
        self.pending_overlay = Some(super::types_overlay::OverlayRequest::DismissDaemonLost);
    }

    pub(super) fn dismiss_remote_reanchor(&mut self) {
        self.pending_overlay = Some(super::types_overlay::OverlayRequest::DismissRemoteReanchor);
    }

    pub(super) fn dismiss_save_playlist(&mut self) {
        self.pending_overlay = Some(super::types_overlay::OverlayRequest::DismissSavePlaylist);
    }
}
