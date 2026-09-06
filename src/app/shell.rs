use std::time::{Duration, Instant};

use super::action::{playback_command_for_key, Command};
use super::components::msg::AlbumCursorKind;
use super::components::{
    media_list::ViewportAnchor, ComponentId, Msg, OverlayId, PlaybackComponent, ShellRequest,
    TerminalObserverEvent, UiRootComponent, UserEvent,
};
use super::router::{resolve_router_outcome_with_focused, RouterOutcome, RouterSnapshot};
use super::service_startup;
use super::types_feeds_manage::FeedsManagePopup;
use super::types_playback::{HomeContent, HomeLatestSource};
use super::{
    init_terminal, install_signal_handlers, restore_terminal, start_quit_watchdog, QUIT_REQUESTED,
};
use super::{App, IdleFeed, ToastSeverity};
use crossterm::event::KeyCode;
use tuirealm::application::{Application, PollStrategy};
use tuirealm::listener::EventListenerCfg;

#[path = "shell_messages.rs"]
mod shell_messages;
#[path = "shell_run.rs"]
mod shell_run;

/// How often the TuiRealm crossterm listener worker polls the terminal for
/// events. The listener's `poll` blocks for half of this; the worker cycle is
/// this long. Set to 8 ms so event latency matches the legacy loop's fastest
/// cadence (the visualizer's 8 ms poll). The main thread's per-iteration wait
/// is governed separately by the `PollStrategy::Once` timeout below, so this
/// only affects how promptly a buffered event reaches the channel — not the
/// render cadence.
const TERMINAL_LISTENER_INTERVAL: Duration = Duration::from_millis(8);
/// Upper bound on events the listener drains from crossterm in one worker
/// cycle. Generous so a burst (e.g. a mouse drag) is flushed into the channel
/// in one cycle; the main thread still processes at most one per tick via
/// `PollStrategy::Once`, matching the legacy one-event-per-iteration loop.
const TERMINAL_LISTENER_MAX_POLL: usize = 60;

/// One-shot transfer used only when TV changes its active destination at a breakpoint.
#[derive(Clone, Debug)]
pub(super) struct InlineSearchTransfer {
    pub query: String,
    pub selected_id: Option<String>,
    pub selected_type: Option<String>,
    pub row_offset: usize,
}

/// One-shot inline-track-focus transition the shell hands the Music workspace
/// at the next content push. `Enter` is bound to the album it was raised for,
/// so a re-anchor that outruns that album's track fetch can retry on the
/// tracks re-push without ever firing on a different album.
#[derive(Clone, Debug, PartialEq)]
pub(super) enum MusicTrackFocusRequest {
    /// Enter inline track focus for this album (recursive album activation).
    Enter { album_id: String },
    /// Clear inline track focus (saved-position restore).
    Clear,
}

/// Shell model holding the legacy `App` and the TuiRealm `Application`.
pub struct Model {
    pub app: App,
    pub(super) application: Application<ComponentId, Msg, UserEvent>,
    pub(super) emby_browser_id: Option<ComponentId>,
    pub(super) tv_workspace_id: Option<ComponentId>,
    pub(super) music_workspace_id: Option<ComponentId>,
    pub(super) abs_podcast_id: Option<ComponentId>,
    pub(super) abs_book_id: Option<ComponentId>,
    /// Maintained registry of every mounted destination surface component
    /// (`Browser` workspaces and `InlineSearch`). TuiRealm's `Application`
    /// exposes no component enumeration, so stale-discovery for
    /// reconciliation cannot read the view registry; this set mirrors every
    /// destination `mount`/`umount` (tasks 1.2 correction) so
    /// `reconcile_destination_mounts` can find a retired library's component
    /// even when no `*_id` pointer still names it.
    pub(super) mounted_destinations: std::collections::HashSet<ComponentId>,
    /// Components currently carrying the `mouse_sub()` subscription. Owned
    /// solely by `sync_mouse_subscriptions` (ADR 0024 D2): it is the mouse
    /// arbitration table. `tuirealm` 4.1's `Application::unsubscribe` removes
    /// every subscription matching the clause value at once (it cannot target
    /// one component's mouse sub in isolation), so the reconciler wipes and
    /// rebuilds the whole set on any change and this mirror is how it knows
    /// the current state without querying `Application`.
    pub(super) mouse_subscribed: std::collections::HashSet<ComponentId>,
    /// One-shot shell→component request for the mounted Music workspace's
    /// inline track focus, applied at the next `sync_music_workspace` after
    /// the component is mounted/synced (so mount-timing never loses it).
    /// Neither mirrors App state: the component owns the cursor, the shell
    /// only delivers the trigger that used to write the deleted inline
    /// track-focus field.
    pub(super) music_track_focus_request: Option<MusicTrackFocusRequest>,
    /// One-shot shell→component re-anchor trigger for the mounted Music
    /// workspace's album cursor/scroll, consumed at the next
    /// `push_music_workspace_content`. Set at the three navigation events that
    /// legitimately move a shell-owned cursor -- group switch, recursive-album
    /// activation, saved-position restore -- and once after mount. An ordinary
    /// content push never adopts the shell cursor; this is the explicit
    /// re-anchor that replaced the deleted echo-suppression test.
    pub(super) music_workspace_reanchor: bool,
    /// One-shot shell→component re-anchor trigger for the mounted wide TV
    /// workspace's series cursor/scroll, consumed at the next
    /// `push_tv_workspace_content`. Set by the breakpoint hand-off
    /// (`hand_off_tv_breakpoint`, migrate-narrow-browse task 2.3 / D5) when
    /// the active-destination pointer flips from the narrow `BrowserComponent`
    /// to `TvWorkspaceComponent`, so the kept-mounted workspace adopts the
    /// resting position the narrow browser left behind instead of its stale
    /// local cursor.
    pub(super) tv_viewport_anchor: Option<ViewportAnchor<String>>,
    pub(super) inline_search_transfer: Option<InlineSearchTransfer>,
    /// Shell-owned mirror of the feeds-management popup's interaction state
    /// plus its background add-feed channel (task 5.3c). The
    /// `FeedsManageComponent` mirrors `stage`/`cursor`/`feeds`/`pending_add`
    /// from here each tick; the mpsc cannot live in the component.
    pub(super) feeds_manage: Option<FeedsManagePopup>,
    /// Model-owned Home content (task 5.3d): the sole snapshot pushed to
    /// `HomeComponent`; App-internal writers deliver computed snapshots via
    /// lib_tx; `loading` mirrors the deleted `App.home_loading`.
    pub(super) home_content: HomeContent,
    /// Shell-owned semantic Home section preference and one-time restore marker.
    pub(super) home_section_pref_semantic: Option<HomeLatestSource>,
    pub(super) home_section_pending: Option<HomeLatestSource>,
}

/// The ADR 0023 Keyboard Router fold: apply the router's outcome to this
/// tick's message list and return the messages that survive.
///
/// `Application::tick` returns the focused component's message first, then the
/// UiRoot observer's `TerminalEvent`. With `PollStrategy::Once` there is at
/// most one terminal event per tick, so the messages for a key chord are:
///
/// * **UiRoot focused** — only the observer's `TerminalEvent(Key)`. This is
///   the active component's own message; `FallThrough` keeps it, while
///   `Command`/`Swallow` replace it (the command is dispatched by the caller).
/// * **Leaf focused** — the leaf's request (or `None`) plus the observer's
///   `TerminalEvent(Key)`. The router's outcome selects between them:
///   `FallThrough` keeps the leaf's request; `Command`/`Swallow` discard it.
///
/// Non-key observer signals (`Resize`, `FocusGained/Lost`, `NoOp`) always pass
/// through: they are redraw/layout signals, not chords.
pub(super) fn apply_router_outcome(
    messages: Vec<Msg>,
    focused: Option<&ComponentId>,
    router: &RouterOutcome,
) -> Vec<Msg> {
    let observed_key = messages
        .iter()
        .any(|msg| matches!(msg, Msg::TerminalEvent(TerminalObserverEvent::Key(_))));
    let mut out = Vec::with_capacity(messages.len());
    for msg in messages {
        match msg {
            Msg::TerminalEvent(TerminalObserverEvent::Key(_)) => {
                // The observed chord. When UiRoot itself is focused this is
                // the leaf message (the active component's own request) and
                // its survival is decided by the router like any leaf message.
                if focused == Some(&ComponentId::UiRoot) {
                    match router {
                        RouterOutcome::FallThrough => out.push(msg),
                        RouterOutcome::Command(_) | RouterOutcome::Swallow => {}
                    }
                }
                // When a leaf is focused the observer key is only the router's
                // trigger; the fold already applied the outcome to the leaf's
                // own message below.
            }
            Msg::TerminalEvent(_) => out.push(msg),
            leaf => {
                // The focused component's request (or a typed shell request
                // from a subscription). `FallThrough` lets it stand; the
                // router's `Command`/`Swallow` discards it for this tick.
                // With no key observed, nothing was routed and every message
                // stands.
                match (router, observed_key) {
                    (RouterOutcome::FallThrough, _) | (_, false) => out.push(leaf),
                    (RouterOutcome::Command(_) | RouterOutcome::Swallow, true) => {}
                }
            }
        }
    }
    out
}

/// ADR 0024: the mouse fold, applied to a `tick()` message list beside the
/// ADR 0023 keyboard router fold.
///
/// `tuirealm` forwards `Event::Mouse` to the focused component *and* to every
/// subscriber, so a mouse tick can yield several `Msg`s. `sync_mouse_subscriptions`
/// keeps only components painted this frame mouse-eligible, and those paint
/// disjoint rectangles and each emits only for points inside its own geometry.
/// Queue wheel messages are the one intentional exception: crossterm can
/// deliver a same-tick burst, so their deltas are summed and clamped to one
/// row. Other multiple claims remain a geometry defect.
///
/// Keyboard ticks carry a `TerminalObserverEvent::Key` marker (the permanent
/// `UiRoot` observer emits it for every `Event::Keyboard`); those pass through
/// untouched for the keyboard router to resolve.
///
/// Load-bearing invariant: `UiRootComponent` is the *only* component that mounts
/// with a non-mouse (`EventClause::Any`) subscription — every other `.mount(`
/// site passes `vec![]`, and mouse eligibility is added later by
/// `sync_mouse_subscriptions`. The structural mouse-tick detection here (a tick
/// with no `TerminalEvent` marker is a mouse tick) depends on that: if a future
/// component mounts its own non-mouse subscription, a non-mouse tick could reach
/// this fold with no marker and be mistaken for a mouse claim — release builds
/// drop it silently, debug builds trip the `debug_assert!` below (ADR 0024).
pub(super) fn fold_mouse_messages(messages: Vec<Msg>) -> Vec<Msg> {
    let observed_key = messages
        .iter()
        .any(|msg| matches!(msg, Msg::TerminalEvent(TerminalObserverEvent::Key(_))));
    if observed_key {
        return messages;
    }
    let claims = messages
        .iter()
        .filter(|msg| !matches!(msg, Msg::TerminalEvent(_)))
        .filter(|msg| !matches!(msg, Msg::Shell(ShellRequest::QueueScroll { .. })))
        .count();
    debug_assert!(
        claims <= 1,
        "mouse fold: {claims} components claimed one mouse event; eligible \
         surfaces paint disjoint regions (ADR 0024)"
    );
    let queue_delta = messages.iter().filter_map(|msg| match msg {
        Msg::Shell(ShellRequest::QueueScroll { delta }) => Some(*delta),
        _ => None,
    });
    let queue_delta = queue_delta.sum::<i64>().clamp(-1, 1);
    let mut kept_claim = false;
    let mut emitted_queue_scroll = false;
    messages
        .into_iter()
        .filter_map(|msg| match msg {
            Msg::TerminalEvent(_) => Some(msg),
            Msg::Shell(ShellRequest::QueueScroll { .. }) if emitted_queue_scroll => None,
            Msg::Shell(ShellRequest::QueueScroll { .. }) => {
                emitted_queue_scroll = true;
                (queue_delta != 0)
                    .then_some(Msg::Shell(ShellRequest::QueueScroll { delta: queue_delta }))
            }
            _ => {
                let first = !kept_claim;
                kept_claim = true;
                first.then_some(msg)
            }
        })
        .collect()
}

impl Model {
    /// Build the router snapshot and resolve the terminal chord. The router
    /// reads a plain-data snapshot, never component attributes (ADR 0023).
    pub(in crate::app) fn router_outcome(&mut self, messages: &[Msg]) -> RouterOutcome {
        let Some(tui_key) = messages.iter().find_map(|msg| match msg {
            Msg::TerminalEvent(TerminalObserverEvent::Key(key)) => Some(*key),
            _ => None,
        }) else {
            return RouterOutcome::FallThrough;
        };
        let key = super::input_resolver::tuirealm_key_to_crossterm(tui_key);

        let snapshot = RouterSnapshot {
            player_active: self.app.player.status.lock().unwrap().active,
            has_remote_session: self.app.connected_session_id.is_some()
                || self.app.player.is_remote()
                || self.app.is_cast_attached(),
            connected_session_id_present: self.app.connected_session_id.is_some(),
            panel_mode: self.app.effective_panel_mode(),
            panel_focus: self.app.effective_panel_focus(),
            blocking_overlay_open: self.blocking_overlay_active(),
            help_overlay_open: self
                .application
                .mounted(&ComponentId::Overlay(OverlayId::Help)),
            sessions_sidebar_open: self
                .application
                .mounted(&ComponentId::Overlay(OverlayId::Sessions)),
            selection_modal_open: self
                .application
                .mounted(&ComponentId::Overlay(OverlayId::SelectionModal)),
            context_menu_open: self
                .application
                .mounted(&ComponentId::Overlay(OverlayId::ContextMenu)),
            idle_feed_link_available: self.app.idle_feed_link_available(),
            text_entry_focused: matches!(
                self.application.focus(),
                Some(
                    ComponentId::Overlay(OverlayId::Search)
                        | ComponentId::Overlay(OverlayId::Settings)
                )
            ) || self.active_inline_search_is_open(),
            space_double_tap: self
                .app
                .last_space_press
                .is_some_and(|pressed| pressed.elapsed() < Duration::from_millis(300)),
            esc_double_tap: self
                .app
                .last_esc_press
                .is_some_and(|pressed| pressed.elapsed() < Duration::from_millis(300)),
        };

        let outcome = resolve_router_outcome_with_focused(key, &snapshot, self.application.focus());
        // The router arms the double-tap timer on the first eligible Space/Esc
        // press regardless of focus; the second press within the window is
        // claimed by `command_for_policy` when the double-tap snapshot flag is
        // set.
        self.update_double_tap_state(key, &snapshot, &outcome);
        outcome
    }

    /// Keep the existing App-owned double-tap timestamps in sync while the
    /// router owns playback resolution. A first eligible press falls through
    /// to the focused leaf and starts its timer; a second press is claimed by
    /// the router and clears the timer after dispatch is selected.
    fn update_double_tap_state(
        &mut self,
        key: crossterm::event::KeyEvent,
        snapshot: &RouterSnapshot,
        outcome: &RouterOutcome,
    ) {
        let playback = playback_command_for_key(
            super::input_resolver::KeyChord::from_key(key),
            snapshot.player_active,
            snapshot.has_remote_session,
        );
        match (key.code, playback, outcome) {
            (KeyCode::Char(' '), Some(Command::TogglePlayPause), RouterOutcome::FallThrough)
                if !snapshot.space_double_tap =>
            {
                self.app.last_space_press = Some(Instant::now());
            }
            (
                KeyCode::Char(' '),
                Some(Command::TogglePlayPause),
                RouterOutcome::Command(Command::TogglePlayPause),
            ) => self.app.last_space_press = None,
            (KeyCode::Esc, Some(Command::Stop), RouterOutcome::FallThrough)
                if !snapshot.esc_double_tap =>
            {
                self.app.last_esc_press = Some(Instant::now());
            }
            (KeyCode::Esc, Some(Command::Stop), RouterOutcome::Command(Command::Stop)) => {
                self.app.last_esc_press = None;
            }
            // any other (key, playback command, router outcome) triple: no
            // double-tap timer to arm or clear.
            _ => {}
        }
    }

    fn dispatch_router_command(&mut self, command: Command) -> bool {
        match command {
            Command::OpenHelp => {
                self.mount_help();
                false
            }
            command => self.app.dispatch(command),
        }
    }

    /// Construct the model, starting the TuiRealm crossterm listener and
    /// mounting the permanent root observer.
    pub fn new(app: App) -> Self {
        Self::new_with_listener(
            app,
            EventListenerCfg::default()
                .crossterm_input_listener(TERMINAL_LISTENER_INTERVAL, TERMINAL_LISTENER_MAX_POLL),
        )
    }

    pub(in crate::app) fn new_with_listener(
        app: App,
        listener_cfg: EventListenerCfg<UserEvent>,
    ) -> Self {
        let application = Application::init(listener_cfg);
        let home_section = App::load_prefs()["home_section"]
            .as_str()
            .and_then(HomeLatestSource::from_pref_key);
        let mut model = Self {
            app,
            application,
            emby_browser_id: None,
            tv_workspace_id: None,
            music_workspace_id: None,
            abs_podcast_id: None,
            abs_book_id: None,
            mounted_destinations: std::collections::HashSet::new(),
            mouse_subscribed: std::collections::HashSet::new(),
            music_track_focus_request: None,
            music_workspace_reanchor: false,
            tv_viewport_anchor: None,
            inline_search_transfer: None,
            feeds_manage: None,
            home_content: HomeContent::new(),
            home_section_pref_semantic: home_section.clone(),
            home_section_pending: home_section,
        };
        // UiRoot owns overlay z-order and permanently observes terminal events.
        // This is the ONLY mount with a non-mouse subscription; every other
        // `.mount(` passes `vec![]` and gains mouse eligibility only via
        // `sync_mouse_subscriptions`. `fold_mouse_messages` depends on that
        // (ADR 0024) — see its doc comment before adding a subscription here.
        model
            .application
            .mount(
                ComponentId::UiRoot,
                Box::new(UiRootComponent::new()),
                UiRootComponent::subscriptions(),
            )
            .expect("mount UiRoot");
        model
            .application
            .active(&ComponentId::UiRoot)
            .expect("activate UiRoot");
        // Home is mounted for the whole session but never made active: its
        // input stays on the shell path, only its render is component-owned
        model.mount_home();
        model.mount_feeds();
        // Playback is also the stable attribute carrier for precedence gates.
        model
            .application
            .mount(
                ComponentId::Playback,
                Box::new(PlaybackComponent::new()),
                // `vec![]`: no non-mouse subscription (ADR 0024, see
                // `fold_mouse_messages`); mouse eligibility comes from
                // `sync_mouse_subscriptions`.
                vec![],
            )
            .expect("mount Playback");
        model.update_settings_content();
        model
    }
}

fn apply_terminal_observer(
    model: &mut Model,
    event: TerminalObserverEvent,
    music_resize: &mut bool,
    tv_resize: &mut bool,
) {
    match event {
        TerminalObserverEvent::Resize { width, height } => {
            model.app.terminal_width = width;
            model.app.terminal_height = height;
            model.app.force_clear = true;
            model.app.card_image_states.clear();
            model.app.card_image_loading.clear();
            model.push_inline_search_content();
            *music_resize = true;
            *tv_resize = true;
        }
        TerminalObserverEvent::FocusGained => model.app.note_focus_gained(),
        TerminalObserverEvent::FocusLost => model.app.note_focus_lost(),
        // Task 6.5: the tab bar is shell-painted chrome with no mounted
        // component of its own, so it has no `mouse_sub()` claim to resolve
        // through. A click outside `layout.tabs_area` (and therefore outside
        // every published hit target) is a no-op here and falls through to
        // whatever mounted component's own claim the same tick produced.
        TerminalObserverEvent::MouseClick { column, row } => {
            let point = ratatui::layout::Position { x: column, y: row };
            if let Some(tab_pos) = model.app.layout.main.tab_at(point) {
                model.dismiss_active_inline_search();
                model.app.set_library_tab(tab_pos);
            }
        }
        TerminalObserverEvent::Key(_) | TerminalObserverEvent::NoOp => {}
    }
}

#[cfg(test)]
mod mouse_fold_tests {
    use super::*;
    use crate::app::components::msg::PlaybackRequest;

    #[test]
    fn fold_keeps_one_mouse_claim_and_the_observer_signal() {
        let msgs = vec![
            Msg::Playback(PlaybackRequest::TogglePlayPause),
            Msg::TerminalEvent(TerminalObserverEvent::NoOp),
        ];
        assert_eq!(
            fold_mouse_messages(msgs),
            vec![
                Msg::Playback(PlaybackRequest::TogglePlayPause),
                Msg::TerminalEvent(TerminalObserverEvent::NoOp),
            ]
        );
    }

    #[test]
    fn fold_passes_a_keyboard_tick_through_untouched() {
        use tuirealm::event::{Key, KeyEvent, KeyModifiers};
        let key = Msg::TerminalEvent(TerminalObserverEvent::Key(KeyEvent {
            code: Key::Down,
            modifiers: KeyModifiers::NONE,
        }));
        let leaf = Msg::Playback(PlaybackRequest::TogglePlayPause);
        assert_eq!(
            fold_mouse_messages(vec![leaf.clone(), key.clone()]),
            vec![leaf, key]
        );
    }

    #[test]
    fn fold_collapses_a_queue_wheel_burst_to_one_row() {
        let folded = fold_mouse_messages(vec![
            Msg::Shell(ShellRequest::QueueScroll { delta: 1 }),
            Msg::Shell(ShellRequest::QueueScroll { delta: 1 }),
            Msg::Shell(ShellRequest::QueueScroll { delta: -1 }),
            Msg::TerminalEvent(TerminalObserverEvent::NoOp),
        ]);
        assert_eq!(
            folded,
            vec![
                Msg::Shell(ShellRequest::QueueScroll { delta: 1 }),
                Msg::TerminalEvent(TerminalObserverEvent::NoOp),
            ]
        );
    }

    #[test]
    #[should_panic(expected = "claimed one mouse event")]
    fn fold_debug_asserts_on_a_second_claim() {
        let _ = fold_mouse_messages(vec![
            Msg::Playback(PlaybackRequest::TogglePlayPause),
            Msg::Playback(PlaybackRequest::Stop),
        ]);
    }
}

#[cfg(test)]
#[path = "shell_tests.rs"]
mod tests;
