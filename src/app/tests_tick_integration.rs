use std::time::{Duration, Instant};

use ratatui::backend::TestBackend;
use ratatui::layout::Rect;
use ratatui::Terminal;
use tuirealm::application::PollStrategy;
use tuirealm::component::AppComponent;
use tuirealm::event::{
    Event, Key, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
};

use crate::app::components::msg::{ConfirmIntent, PlaybackRequest, ServiceRequest};
use crate::app::components::inline_search::InlineSearchHost;
use crate::app::components::{
    ComponentId, ModalId, Msg, MusicWorkspaceComponent, OverlayId, QueueRequest, SearchPool,
    SearchSidebarComponent, ShellRequest, TerminalObserverEvent, UserEvent,
};
use crate::app::router::RouterOutcome;
use crate::app::shell::apply_router_outcome;
use crate::app::tests::make_app_stub;
use crate::app::tests_tick_harness::TickHarness;
use crate::app::types_confirm::{ConfirmAction, ConfirmModal};
use crate::app::types_overlay::OverlayRequest;
use crate::app::{PanelFocus, PanelMode, SidebarId, TabSelection};

fn key(code: Key) -> Event<UserEvent> {
    Event::Keyboard(KeyEvent {
        code,
        modifiers: KeyModifiers::NONE,
    })
}

fn queue_focused_harness() -> TickHarness {
    let mut app = make_app_stub();
    app.panel_focus = PanelFocus::Queue;
    TickHarness::new(app)
}

pub(super) fn search_component_mut(harness: &mut TickHarness) -> &mut SearchSidebarComponent {
    harness
        .model_mut()
        .application
        .get_component_mut(&ComponentId::Overlay(OverlayId::Search))
        .expect("search sidebar mounted")
        .as_any_mut()
        .downcast_mut::<SearchSidebarComponent>()
        .expect("search sidebar type")
}

fn arm_search_query(harness: &mut TickHarness, query: &str) {
    for c in query.chars() {
        let message = search_component_mut(harness).on(&key(Key::Char(c)));
        assert!(message.is_none(), "typing search chars stays local");
    }
}

/// Phase 1 delivery proof (task 2.7): with Queue focused, a click on the
/// seek-bar row still reaches the unfocused `PlaybackComponent` through its
/// `mouse_sub()` subscription, and the component resolves the column against
/// its own painted `seekbar_area` into a 0.0..=1.0 fraction. No other eligible
/// surface claims the event (D2 exclusivity).
#[test]
fn tick_delivers_seekbar_click_to_unfocused_playback_as_a_fraction() {
    let mut app = make_app_stub();
    app.panel_focus = PanelFocus::Queue;
    app.connected_session_id = Some("session-1".into());
    app.layout.playback.player_area = Rect::new(10, 5, 40, 4);
    let mut harness = TickHarness::new(app);
    harness.model_mut().sync_mounted_surfaces();

    let mut terminal = Terminal::new(TestBackend::new(60, 12)).unwrap();
    terminal
        .draw(|frame| harness.model_mut().render_playback_component(frame))
        .unwrap();

    harness.inject(Event::Mouse(MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: 30,
        row: 5,
        modifiers: KeyModifiers::NONE,
    }));
    let outcome = harness.step();

    assert_eq!(outcome.pre_fold_focus, Some(ComponentId::Queue));
    let seeks: Vec<f64> = outcome
        .raw_messages
        .iter()
        .filter_map(|msg| match msg {
            Msg::Playback(PlaybackRequest::SeekTo(f)) => Some(*f),
            _ => None,
        })
        .collect();
    assert_eq!(seeks.len(), 1, "exactly one surface claims the click");
    assert!((seeks[0] - 0.5).abs() < 1e-6, "column 30 of x10..w40 is 0.5");
}

#[test]
fn tick_delivers_key_to_focused_queue_before_root_observer_once() {
    let mut harness = queue_focused_harness();
    harness.inject(key(Key::Char('[')));

    let outcome = harness.step();

    assert_eq!(outcome.pre_fold_focus, Some(ComponentId::Queue));
    assert!(matches!(outcome.router, RouterOutcome::FallThrough));
    assert_eq!(outcome.raw_messages.len(), 2, "one leaf and one observer");
    assert!(matches!(
        outcome.raw_messages.first(),
        Some(Msg::Queue(QueueRequest::Scope(
            crate::app::QueueScope::Local
        )))
    ));
    assert!(matches!(
        outcome.raw_messages.get(1),
        Some(Msg::TerminalEvent(TerminalObserverEvent::Key(_)))
    ));
    assert_eq!(
        outcome
            .raw_messages
            .iter()
            .filter(|msg| matches!(msg, Msg::Queue(QueueRequest::Scope(_))))
            .count(),
        1
    );
    assert_eq!(
        outcome
            .raw_messages
            .iter()
            .filter(|msg| matches!(msg, Msg::TerminalEvent(TerminalObserverEvent::Key(_))))
            .count(),
        1
    );
    assert_eq!(outcome.messages.len(), 1, "observer key is fold-only");

    harness.inject(key(Key::Char('[')));
    let next = harness.step();
    assert_eq!(next.raw_messages.len(), 2);
    assert_eq!(next.messages.len(), 1);
}

#[test]
fn full_sync_sequence_leaves_focus_on_queue_or_library_destination() {
    let mut queue_harness = queue_focused_harness();
    queue_harness.model_mut().sync_mounted_surfaces();
    assert_eq!(
        queue_harness.model().application.focus(),
        Some(&ComponentId::Queue)
    );

    let mut library_app = crate::app::render::make_movie_app();
    library_app.tab = TabSelection::EmbyLibrary(0);
    library_app.panel_focus = PanelFocus::Library;
    library_app.panel_mode = PanelMode::Both;
    let mut library_harness = TickHarness::new(library_app);
    library_harness.model_mut().sync_mounted_surfaces();
    let child = library_harness
        .model()
        .emby_browser_id
        .clone()
        .expect("movie browser child mounted");
    assert_eq!(library_harness.model().application.focus(), Some(&child));

    let mut stub_app = make_app_stub();
    stub_app.tab = TabSelection::EmbyLibrary(0);
    stub_app.panel_focus = PanelFocus::Library;
    let mut stub_harness = TickHarness::new(stub_app);
    stub_harness.model_mut().sync_mounted_surfaces();
    assert_eq!(
        stub_harness.model().application.focus(),
        Some(&ComponentId::UiRoot)
    );
}

#[test]
fn search_clock_user_event_reaches_mounted_search_component() {
    let mut harness = TickHarness::new(make_app_stub());
    harness.model_mut().mount_sidebar(SidebarId::Search);
    arm_search_query(&mut harness, "ab");
    std::thread::sleep(Duration::from_millis(310));

    harness.inject(Event::User(UserEvent::Clock(Instant::now())));
    let raw_messages = harness
        .model_mut()
        .application
        .tick(tuirealm::application::PollStrategy::Once(
            Duration::from_millis(500),
        ))
        .expect("tick user clock");

    assert!(raw_messages.iter().any(|msg| {
        matches!(
            msg,
            Msg::Service(ServiceRequest::SearchQuery(query)) if query == "ab"
        )
    }));
    let component = search_component_mut(&mut harness);
    assert!(component.debounce_pending.is_none());
    assert!(component.debounce_deadline.is_none());
}

#[test]
fn search_clock_sweep_dispatches_debounce_on_step() {
    let mut harness = TickHarness::new(make_app_stub());
    harness.model_mut().mount_sidebar(SidebarId::Search);
    arm_search_query(&mut harness, "ab");
    assert!(harness
        .model_mut()
        .tick_search_clock(Instant::now())
        .is_none());

    std::thread::sleep(Duration::from_millis(310));
    let outcome = harness.step();

    assert!(outcome.raw_messages.is_empty());
    let component = search_component_mut(&mut harness);
    assert!(component.debounce_pending.is_none());
    assert!(component.debounce_deadline.is_none());
    let _ = ServiceRequest::SearchQuery;
}

/// Mini view keeps `effective_panel_focus` on Queue, so `sync_queue` used to
/// re-activate Queue on the tick after a sidebar mounted, stealing the Esc
/// that would close it. The sync passes must yield focus while an overlay is
/// up.
#[test]
fn esc_closes_a_sidebar_in_mini_view() {
    let mut app = make_app_stub();
    app.terminal_width = 70;
    let mut harness = TickHarness::new(app);
    harness.model_mut().mount_sidebar(SidebarId::Sessions);
    let id = ComponentId::Overlay(OverlayId::Sessions);

    // The sync pass that previously stole focus back to Queue.
    harness.model_mut().sync_mounted_surfaces();
    assert_eq!(harness.model().application.focus(), Some(&id));

    harness.inject(key(Key::Esc));
    let outcome = harness.step();
    let (mut music_resize, mut tv_resize) = (false, false);
    for message in outcome.messages {
        harness
            .model_mut()
            .handle_terminal_message(message, &mut music_resize, &mut tv_resize);
    }
    harness.model_mut().sync_mounted_surfaces();
    assert!(!harness.model().application.mounted(&id));
}

#[test]
fn blocking_confirm_overlay_keeps_focus_and_receives_input() {
    let mut app = make_app_stub();
    app.panel_focus = PanelFocus::Queue;
    app.pending_overlay = Some(OverlayRequest::Confirm(ConfirmModal {
        title: "Clear queue?".into(),
        message: "Remove queued items".into(),
        hint: "[y] Confirm    [Esc] Cancel".into(),
        on_confirm: ConfirmAction::ClearQueue,
    }));
    let mut harness = TickHarness::new(app);

    harness.model_mut().sync_mounted_surfaces();
    let confirm_id = ComponentId::Modal(ModalId::Confirm);
    assert_eq!(harness.model().application.focus(), Some(&confirm_id));

    harness.inject(key(Key::Char('y')));
    let outcome = harness.step();
    assert_eq!(outcome.pre_fold_focus, Some(confirm_id.clone()));
    assert!(matches!(outcome.router, RouterOutcome::FallThrough));
    assert!(matches!(
        outcome.raw_messages.first(),
        Some(Msg::Shell(ShellRequest::ConfirmIntent(
            ConfirmIntent::Accept
        )))
    ));

    harness
        .model_mut()
        .application
        .active(&ComponentId::Queue)
        .expect("activate lower queue for swallow guard");
    harness.inject(key(Key::Char('c')));
    let pre_fold_focus = harness.model().application.focus().cloned();
    let raw_messages = harness
        .model_mut()
        .application
        .tick(PollStrategy::Once(Duration::from_millis(500)))
        .expect("tick lower focused queue");
    let router = harness.model_mut().router_outcome(&raw_messages);
    let messages = apply_router_outcome(raw_messages, pre_fold_focus.as_ref(), &router);
    assert_eq!(pre_fold_focus, Some(ComponentId::Queue));
    assert!(matches!(router, RouterOutcome::Swallow));
    assert!(messages.is_empty());
}

fn wide_music_harness() -> (TickHarness, ComponentId) {
    let mut app = crate::app::render::make_music_group_app();
    app.terminal_width = 160;
    app.terminal_height = 40;
    let mut first = crate::app::tests::make_item("Track One", "Audio");
    first.id = "track-1".into();
    let mut second = crate::app::tests::make_item("Track Two", "Audio");
    second.id = "track-2".into();
    app.album_tracks_cache
        .insert("album-1".into(), vec![first, second]);
    let mut harness = TickHarness::new(app);
    harness.model_mut().sync_mounted_surfaces();
    let id = harness
        .model()
        .music_workspace_id
        .clone()
        .expect("wide Music workspace mounted");
    (harness, id)
}

fn music_track_cursor(harness: &TickHarness, id: &ComponentId) -> Option<usize> {
    harness
        .model()
        .application
        .get_component(id)
        .expect("Music workspace mounted")
        .as_any()
        .downcast_ref::<MusicWorkspaceComponent>()
        .expect("Music workspace type")
        .track_cursor()
}

fn music_workspace<'a>(
    harness: &'a TickHarness,
    id: &ComponentId,
) -> &'a MusicWorkspaceComponent {
    harness
        .model()
        .application
        .get_component(id)
        .expect("Music workspace mounted")
        .as_any()
        .downcast_ref::<MusicWorkspaceComponent>()
        .expect("Music workspace type")
}

fn music_album_cursor(harness: &TickHarness, id: &ComponentId) -> usize {
    music_workspace(harness, id).album_cursor()
}

fn music_selected_album_id(harness: &TickHarness, id: &ComponentId) -> Option<String> {
    music_workspace(harness, id).selected_item().map(|item| item.id)
}

/// A Library → Queue → Library round trip through real `Application::tick()`:
/// the Music workspace cannot navigate while Queue holds focus, navigates
/// immediately when Library focus returns (no click, no content refresh
/// between the focus return and the key), and keeps its private track-pane
/// selection across the whole trip.
#[test]
fn music_library_queue_library_round_trip_keeps_focus_and_pane_state() {
    let (mut harness, id) = wide_music_harness();
    assert_eq!(harness.model().application.focus(), Some(&id));

    // Enter the inline track pane, then move the track cursor: private pane
    // state a blur must not disturb.
    harness.inject(key(Key::Enter));
    harness.step();
    assert_eq!(music_track_cursor(&harness, &id), Some(0));
    harness.inject(key(Key::Down));
    harness.step();
    assert_eq!(music_track_cursor(&harness, &id), Some(1));

    // Panel focus moves to Queue through the production sync order.
    harness.model_mut().app.panel_focus = PanelFocus::Queue;
    harness.model_mut().sync_mounted_surfaces();
    assert_eq!(
        harness.model().application.focus(),
        Some(&ComponentId::Queue)
    );

    // A Music navigation key while blurred does not reach the workspace.
    harness.inject(key(Key::Up));
    let raw = harness
        .model_mut()
        .application
        .tick(PollStrategy::Once(Duration::from_millis(500)))
        .expect("tick blurred music");
    assert!(!raw
        .iter()
        .any(|msg| matches!(msg, Msg::Shell(ShellRequest::MusicTrackActivate))));
    assert_eq!(
        music_track_cursor(&harness, &id),
        Some(1),
        "blurred Music must not navigate its track pane"
    );

    // Library regains focus through the destination pass only — no content
    // push.
    harness.model_mut().app.panel_focus = PanelFocus::Library;
    harness.model_mut().sync_active_destination();
    assert_eq!(harness.model().application.focus(), Some(&id));
    assert_eq!(
        music_track_cursor(&harness, &id),
        Some(1),
        "the private track cursor survives the focus round trip"
    );

    // Keyboard navigation lands immediately, with no click and no content
    // refresh between the focus return and the key.
    harness.inject(key(Key::Up));
    harness
        .model_mut()
        .application
        .tick(PollStrategy::Once(Duration::from_millis(500)))
        .expect("tick refocused music");
    assert_eq!(
        music_track_cursor(&harness, &id),
        Some(0),
        "Music navigates immediately once Library focus returns"
    );
}

/// Ctrl+A on a selected Inline Search result reaches the focused destination
/// through a real `Application::tick()`: the text-entry router gate lets the
/// chord fall through to the workspace, which enqueues the selected result row
/// rather than the ordinary album cursor.
#[test]
fn ctrl_a_on_inline_search_result_enqueues_that_result_through_live_tick() {
    let (mut harness, id) = wide_music_harness();

    // Open Inline Search from the focused Music workspace.
    harness.inject(key(Key::Char('/')));
    harness.step();
    assert!(harness.model().active_inline_search_is_open());

    // Seed a known result row (the shell content push would otherwise supply
    // the library's own albums); the cursor rests on it.
    {
        let workspace = harness
            .model_mut()
            .application
            .get_component_mut(&id)
            .expect("Music workspace mounted")
            .as_any_mut()
            .downcast_mut::<MusicWorkspaceComponent>()
            .expect("Music workspace type");
        let mut result = crate::app::tests::make_item("Result Album", "MusicAlbum");
        result.id = "result-album".into();
        workspace
            .inline_search_mut()
            .set_pool(SearchPool::Items(vec![result]));
    }

    harness.inject(Event::Keyboard(KeyEvent {
        code: Key::Char('a'),
        modifiers: KeyModifiers::CONTROL,
    }));
    let outcome = harness.step();
    assert!(
        outcome.messages.iter().any(|msg| matches!(
            msg,
            Msg::Shell(ShellRequest::EmbyLibraryEnqueue { item }) if item.id == "result-album"
        )),
        "Ctrl+A acts on the selected Inline Search result: {:?}",
        outcome.messages
    );
    assert!(
        !harness.model().active_inline_search_is_open(),
        "launching a result exits search so focus is not trapped in the text entry"
    );
}

fn album_row(id: &str, name: &str) -> mbv_core::api::EmbyItem {
    let mut album = crate::app::tests::make_item(name, "MusicAlbum");
    album.id = id.into();
    album
}

/// Enter on an album result is fully asynchronous: `activate_recursive_album`
/// needs an Emby runtime client (absent in this harness) so it returns `false`,
/// and the synchronous branch must then change nothing -- the search stays open
/// and no track-focus / re-anchor one-shot is armed. (Regression: task 2.1's
/// first cut dismissed the search and armed the flags synchronously, stranding
/// the user in track-selection on the pre-search album when activation failed.)
#[test]
fn enter_on_inline_search_album_result_defers_to_async_activation() {
    let (mut harness, id) = wide_music_harness();

    harness.inject(key(Key::Char('/')));
    harness.step();
    assert!(harness.model().active_inline_search_is_open());

    let album = album_row("album-1", "First Album");
    harness.model_mut().app.album_indexes.insert(
        "lib-music".into(),
        crate::app::AlbumIndexState::Ready(vec![crate::app::AlbumSearchEntry {
            album: album.clone(),
            ancestors: Vec::new(),
            display_label: "First Album".into(),
            search_text: "first album".into(),
        }]),
    );
    {
        let workspace = harness
            .model_mut()
            .application
            .get_component_mut(&id)
            .expect("Music workspace mounted")
            .as_any_mut()
            .downcast_mut::<MusicWorkspaceComponent>()
            .expect("Music workspace type");
        workspace
            .inline_search_mut()
            .set_pool(SearchPool::Albums(vec![crate::app::AlbumSearchEntry {
                album,
                ancestors: Vec::new(),
                display_label: "First Album".into(),
                search_text: "first album".into(),
            }]));
    }

    harness.inject(key(Key::Enter));
    let outcome = harness.step();
    let (mut music_resize, mut tv_resize) = (false, false);
    for message in outcome.messages {
        harness
            .model_mut()
            .handle_terminal_message(message, &mut music_resize, &mut tv_resize);
    }
    harness.model_mut().sync_mounted_surfaces();

    assert!(
        harness.model().active_inline_search_is_open(),
        "a failed recursive activation leaves Inline Search open"
    );
    assert_eq!(
        music_track_cursor(&harness, &id),
        None,
        "a failed recursive activation does not enter track-selection mode"
    );
}

/// The `RecursiveAlbumActivated` lib event is the sole owner of the return to
/// the standard Music presentation: it replaces the nav stack, then re-anchors
/// the workspace onto the activated album at its natural list position and
/// enters track-selection for THAT album -- not for whatever album sat under
/// the pre-search cursor.
#[test]
fn recursive_album_activation_event_reanchors_onto_the_activated_album() {
    let (mut harness, id) = wide_music_harness();

    // Two sibling albums; the pre-search cursor rests on the first.
    assert_eq!(music_album_cursor(&harness, &id), 0);

    // The async worker replaces the whole nav stack with the resolved path;
    // its resting cursor points at the activated album (the second sibling,
    // "album-1", whose tracks the harness has cached).
    let nav_stack = vec![
        crate::app::BrowseLevel {
            parent_id: "lib-music".into(),
            title: "Music".into(),
            items: vec![album_row("group-0", "Alpha")],
            total_count: 1,
            resting: crate::app::types_browse::BrowseResting::new(0, 0),
            item_types: None,
            unplayed_only: false,
            sort_by: "SortName".into(),
            sort_order: "Ascending".into(),
            loading: false,
            all_items: None,
            letter_filter: None,
            music_grouping: None,
        },
        crate::app::BrowseLevel {
            parent_id: "group-0".into(),
            title: "Alpha".into(),
            items: vec![
                album_row("album-0", "Zeroth Album"),
                album_row("album-1", "First Album"),
            ],
            total_count: 2,
            resting: crate::app::types_browse::BrowseResting::new(1, 0),
            item_types: None,
            unplayed_only: false,
            sort_by: "SortName".into(),
            sort_order: "Ascending".into(),
            loading: false,
            all_items: None,
            letter_filter: None,
            music_grouping: None,
        },
    ];

    // Drive the production `LibEvent::RecursiveAlbumActivated` arm
    // (shell_run.rs): App installs the path, then the shell arms the re-anchor
    // + track-focus one-shots and re-projects the workspace.
    harness
        .model_mut()
        .app
        .handle_lib_event(crate::app::LibEvent::RecursiveAlbumActivated {
            library_id: "lib-music".into(),
            nav_stack,
        });
    harness.model_mut().music_track_focus_request =
        Some(crate::app::shell::MusicTrackFocusRequest::Enter { album_id: "album-1".into() });
    harness.model_mut().music_workspace_reanchor = true;
    harness.model_mut().push_music_workspace_content();
    harness.model_mut().sync_mounted_surfaces();

    assert_eq!(
        music_album_cursor(&harness, &id),
        1,
        "the workspace re-anchors onto the activated album's natural position"
    );
    assert_eq!(
        music_selected_album_id(&harness, &id).as_deref(),
        Some("album-1"),
        "the focused album is the activated one, not the pre-search cursor"
    );
    assert_eq!(
        music_track_cursor(&harness, &id),
        Some(0),
        "track-selection mode is entered for the activated album"
    );
}

/// Blocking-overlay focus loss and restoration through live `Application::tick()`:
/// raising a blocking confirm modal moves keyboard delivery off the focused
/// Queue; dismissing it the production way restores Queue focus with no
/// focus-only content projection, and Queue receives keys again immediately.
#[test]
fn blocking_overlay_focus_loss_and_restoration_through_live_tick() {
    let mut harness = queue_focused_harness();
    harness.model_mut().sync_mounted_surfaces();
    assert_eq!(
        harness.model().application.focus(),
        Some(&ComponentId::Queue)
    );

    harness.model_mut().app.pending_overlay = Some(OverlayRequest::Confirm(ConfirmModal {
        title: "Clear queue?".into(),
        message: "Remove queued items".into(),
        hint: "[y] Confirm    [Esc] Cancel".into(),
        on_confirm: ConfirmAction::ClearQueue,
    }));
    harness.model_mut().sync_mounted_surfaces();
    let confirm_id = ComponentId::Modal(ModalId::Confirm);
    assert_eq!(harness.model().application.focus(), Some(&confirm_id));

    // The blocking modal, not Queue, receives keyboard input while it is up.
    harness.inject(key(Key::Char('y')));
    let outcome = harness.step();
    assert_eq!(outcome.pre_fold_focus, Some(confirm_id.clone()));
    assert!(outcome
        .raw_messages
        .iter()
        .any(|msg| matches!(
            msg,
            Msg::Shell(ShellRequest::ConfirmIntent(ConfirmIntent::Accept))
        )));

    // Dismiss the modal the production way; the next sync pass restores focus
    // to the underlying Queue without a focus-only projection.
    let (mut music_resize, mut tv_resize) = (false, false);
    harness.model_mut().handle_terminal_message(
        Msg::Shell(ShellRequest::ConfirmIntent(ConfirmIntent::Accept)),
        &mut music_resize,
        &mut tv_resize,
    );
    harness.model_mut().sync_mounted_surfaces();
    assert!(!harness.model().application.mounted(&confirm_id));
    assert_eq!(
        harness.model().application.focus(),
        Some(&ComponentId::Queue),
        "overlay dismiss restores focus to the underlying Queue"
    );

    harness.inject(key(Key::Char('[')));
    let outcome = harness.step();
    assert_eq!(outcome.pre_fold_focus, Some(ComponentId::Queue));
    assert!(outcome.raw_messages.iter().any(|msg| matches!(
        msg,
        Msg::Queue(QueueRequest::Scope(crate::app::QueueScope::Local))
    )));
}


/// Finding 1: `.` is a selection-dependent chord, so the central router falls
/// it through to the focused `QueueComponent`; the emitted `QueueContextMenu`
/// request is dispatched by the shell into a pending context-menu overlay.
#[test]
fn tick_routes_dot_to_focused_queue_and_opens_the_context_menu() {
    let mut app = make_app_stub();
    app.panel_focus = PanelFocus::Queue;
    app.player_tab.set_queue_items(
        vec![mbv_core::playback_queue::QueueItem::Emby(Box::new(
            crate::app::tests::make_item("queued", "Movie"),
        ))],
        0,
    );
    let mut harness = TickHarness::new(app);
    harness.model_mut().sync_mounted_surfaces();
    assert_eq!(harness.model().application.focus(), Some(&ComponentId::Queue));

    harness.inject(key(Key::Char('.')));
    let outcome = harness.step();

    assert_eq!(outcome.pre_fold_focus, Some(ComponentId::Queue));
    assert!(matches!(outcome.router, RouterOutcome::FallThrough));
    assert!(
        outcome
            .messages
            .iter()
            .any(|m| matches!(m, Msg::Shell(ShellRequest::QueueContextMenu { .. }))),
        "`.` falls through to the focused Queue component"
    );

    let (mut music_resize, mut tv_resize) = (false, false);
    for message in outcome.messages {
        harness
            .model_mut()
            .handle_terminal_message(message, &mut music_resize, &mut tv_resize);
    }
    assert!(
        matches!(
            harness.model().app.pending_overlay,
            Some(OverlayRequest::ContextMenu(_))
        ),
        "dispatching QueueContextMenu opens a context-menu overlay"
    );
}
