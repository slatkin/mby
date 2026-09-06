use ratatui::backend::TestBackend;
use ratatui::layout::Rect;
use ratatui::Terminal;
use tuirealm::event::{Event, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};

use crate::app::components::{
    BrowserComponent, ComponentId, ModalId, Msg, OverlayId, QueueComponent, ShellRequest,
};
use crate::app::tests::{make_app_stub, make_item};
use crate::app::tests_tick_harness::{StepOutcome, TickHarness};
use crate::app::tests_tick_integration::search_component_mut;
use crate::app::types_confirm::{ConfirmAction, ConfirmModal};
use crate::app::types_context_menu::{
    ContextAction, ContextMenu, ContextMenuAnchor, ContextMenuEntry,
};
use crate::app::types_daemon_lost::DaemonLostModal;
use crate::app::types_overlay::OverlayRequest;
use crate::app::types_playback::RemoteReanchorPopup;
use crate::app::{PanelFocus, PanelMode, SidebarId, TabSelection};

// --- Task 5.3: blocking modals suppress mouse activity by eligibility (D2
// rung 1), not by message discarding. A mounted Search sidebar painted with
// results is the underlying surface: if the modal did not hold exclusivity,
// a click on a result row would move its cursor and a click outside its
// frame would emit `DismissSearch`.

/// A harness with a mounted Search sidebar painted with two results.
fn search_sidebar_with_painted_results() -> (TickHarness, Vec<(Rect, usize)>) {
    let mut app = make_app_stub();
    app.layout.main.panel_area = Rect::new(0, 0, 30, 16);
    let mut harness = TickHarness::new(app);
    harness.model_mut().mount_sidebar(SidebarId::Search);
    {
        let component = search_component_mut(&mut harness);
        component.sidebar.query = "clip".into();
        component.sidebar.results = vec![
            make_item("Birthday Clip", "Movie"),
            make_item("Other Clip", "Series"),
        ];
        component.sidebar.list_height = 10;
    }
    let mut terminal = Terminal::new(TestBackend::new(40, 16)).unwrap();
    terminal
        .draw(|frame| harness.model_mut().render_search_overlay(frame))
        .unwrap();
    let rows = search_component_mut(&mut harness)
        .test_results()
        .regions()
        .to_vec();
    assert_eq!(rows.len(), 2, "both search results must be painted");
    (harness, rows)
}

/// Clicking on the second painted result row (outside the modal) must
/// produce no message and leave the sidebar's cursor/scroll/filter
/// untouched; clicking outside the sidebar frame must not emit the
/// `DismissSearch` it would if the sidebar were still eligible.
fn assert_blocking_modal_suppresses_sidebar_clicks(
    harness: &mut TickHarness,
    rows: &[(Rect, usize)],
) {
    let (column, row) = {
        let (rect, _) = rows[1];
        (rect.x, rect.y)
    };
    harness.inject(Event::Mouse(MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column,
        row,
        modifiers: KeyModifiers::NONE,
    }));
    let outcome = harness.step();
    assert!(
        outcome
            .raw_messages
            .iter()
            .all(|msg| matches!(msg, Msg::TerminalEvent(_))),
        "a click outside the blocking modal must produce no underlying \
         message (only the UiRoot observer's NoOp redraw echo may appear)"
    );
    {
        let component = search_component_mut(harness);
        assert_eq!(component.sidebar.cursor, 0, "underlying cursor untouched");
        assert_eq!(component.sidebar.scroll, 0, "underlying scroll untouched");
        assert_eq!(component.sidebar.type_filter, 0);
    }

    // Outside the sidebar's painted frame: an eligible sidebar would emit
    // `DismissSearch` here (its Esc path).
    harness.inject(Event::Mouse(MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: 39,
        row: 15,
        modifiers: KeyModifiers::NONE,
    }));
    let outcome = harness.step();
    assert!(
        outcome
            .raw_messages
            .iter()
            .all(|msg| matches!(msg, Msg::TerminalEvent(_))),
        "the sidebar's dismiss click must not surface beneath a blocking modal"
    );
    assert_eq!(search_component_mut(harness).sidebar.cursor, 0);
}

#[test]
fn tick_blocking_confirm_modal_suppresses_underlying_mouse_activity() {
    let (mut harness, rows) = search_sidebar_with_painted_results();
    let modal_id = ComponentId::Modal(ModalId::Confirm);
    harness.model_mut().app.pending_overlay = Some(OverlayRequest::Confirm(ConfirmModal {
        title: "Clear queue?".into(),
        message: "Remove queued items".into(),
        hint: "[y] Confirm    [Esc] Cancel".into(),
        on_confirm: ConfirmAction::ClearQueue,
    }));
    harness.model_mut().sync_mounted_surfaces();
    assert!(harness.model().application.mounted(&modal_id));
    assert_eq!(
        harness.model().mouse_subscribed,
        std::iter::once(modal_id).collect(),
        "rung 1: only the blocking modal is mouse-eligible"
    );

    assert_blocking_modal_suppresses_sidebar_clicks(&mut harness, &rows);
}

#[test]
fn tick_blocking_daemon_lost_modal_suppresses_underlying_mouse_activity() {
    let (mut harness, rows) = search_sidebar_with_painted_results();
    let modal_id = ComponentId::Modal(ModalId::DaemonLost);
    harness.model_mut().app.pending_overlay =
        Some(OverlayRequest::DaemonLost(DaemonLostModal {
            last_playing_title: Some("Birthday Clip".into()),
            daemon_log_path: "/tmp/mbvd.log".into(),
            restart_error: None,
        }));
    harness.model_mut().sync_mounted_surfaces();
    assert!(harness.model().application.mounted(&modal_id));
    assert_eq!(
        harness.model().mouse_subscribed,
        std::iter::once(modal_id).collect(),
        "rung 1: only the blocking modal is mouse-eligible"
    );

    assert_blocking_modal_suppresses_sidebar_clicks(&mut harness, &rows);
}

#[test]
fn tick_blocking_remote_reanchor_modal_suppresses_underlying_mouse_activity() {
    let (mut harness, rows) = search_sidebar_with_painted_results();
    let modal_id = ComponentId::Modal(ModalId::RemoteReanchor);
    harness.model_mut().app.pending_overlay = Some(OverlayRequest::RemoteReanchor(
        RemoteReanchorPopup {
            targets: vec![(0, "Local".into())],
            cursor: 0,
        },
    ));
    harness.model_mut().sync_mounted_surfaces();
    assert!(harness.model().application.mounted(&modal_id));
    assert_eq!(
        harness.model().mouse_subscribed,
        std::iter::once(modal_id).collect(),
        "rung 1: only the blocking modal is mouse-eligible"
    );

    assert_blocking_modal_suppresses_sidebar_clicks(&mut harness, &rows);
}

// --- Task 6.5: tab-bar click-to-switch. The tab bar is shell-painted chrome
// with no mounted component, so it has no `mouse_sub()` claim; the click is
// resolved by the shell against `layout.tabs_hitmap` via the `MouseClick`
// observer signal, then driven through `set_library_tab` (the same entry
// point keyboard tab-cycling uses).

fn apply_outcome(harness: &mut TickHarness, outcome: StepOutcome) {
    let (mut music_resize, mut tv_resize) = (false, false);
    for message in outcome.messages {
        harness
            .model_mut()
            .handle_terminal_message(message, &mut music_resize, &mut tv_resize);
    }
}

#[test]
fn tab_bar_click_switches_active_tab() {
    let mut app = crate::app::render::make_movie_app();
    app.tab = TabSelection::Home;
    let mut harness = TickHarness::new(app);
    harness.model_mut().sync_mounted_surfaces();

    let mut terminal = Terminal::new(TestBackend::new(80, 24)).unwrap();
    terminal
        .draw(|f| harness.model_mut().draw_frame(f, false, false))
        .unwrap();

    let (rect, tab_pos) = harness
        .model()
        .app
        .layout
        .main
        .tabs_hitmap
        .iter()
        .find(|(_, pos)| *pos == 1)
        .copied()
        .expect("Movies tab painted at position 1");
    assert_eq!(tab_pos, 1);

    harness.inject(Event::Mouse(MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: rect.x,
        row: rect.y,
        modifiers: KeyModifiers::NONE,
    }));
    let outcome = harness.step();
    apply_outcome(&mut harness, outcome);

    assert_eq!(
        harness.model().app.tab,
        TabSelection::EmbyLibrary(0),
        "clicking the Movies tab switches to it, mirroring keyboard tab-cycling"
    );
}

#[test]
fn tab_bar_click_outside_tabs_area_is_noop() {
    let mut app = crate::app::render::make_movie_app();
    app.tab = TabSelection::Home;
    let mut harness = TickHarness::new(app);
    harness.model_mut().sync_mounted_surfaces();

    let mut terminal = Terminal::new(TestBackend::new(80, 24)).unwrap();
    terminal
        .draw(|f| harness.model_mut().draw_frame(f, false, false))
        .unwrap();

    assert!(
        !harness.model().app.layout.tabs_area.contains(
            ratatui::layout::Position { x: 0, y: 0 }
        ),
        "top-left corner must fall outside the tab bar for this assertion to be meaningful"
    );

    harness.inject(Event::Mouse(MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: 0,
        row: 0,
        modifiers: KeyModifiers::NONE,
    }));
    let outcome = harness.step();
    apply_outcome(&mut harness, outcome);

    assert_eq!(
        harness.model().app.tab,
        TabSelection::Home,
        "a click outside tabs_area is a no-op"
    );
}

/// Task 5.4 (D2 rung 2 exclusivity): with the context menu mounted, a wheel
/// over the obscured queue must not reach it. The same wheel reaches the
/// queue and scrolls it while the queue is eligible.
#[test]
fn tick_context_menu_wheel_does_not_mutate_the_obscured_queue() {
    let mut app = make_app_stub();
    app.panel_focus = PanelFocus::Queue;
    app.layout.main.queue_area = Rect::new(0, 0, 40, 10);
    let mut harness = TickHarness::new(app);
    harness.model_mut().sync_mounted_surfaces();

    let wheel = |column, row| {
        Event::Mouse(MouseEvent {
            kind: MouseEventKind::ScrollDown,
            column,
            row,
            modifiers: KeyModifiers::NONE,
        })
    };
    harness.inject(wheel(5, 5));
    let outcome = harness.step();
    assert!(
        outcome
            .raw_messages
            .iter()
            .any(|msg| matches!(msg, Msg::Shell(ShellRequest::QueueScroll { .. }))),
        "the queue responds to the wheel while it is mouse-eligible"
    );

    let menu_id = ComponentId::Overlay(OverlayId::ContextMenu);
    harness.model_mut().app.pending_overlay = Some(OverlayRequest::ContextMenu(ContextMenu {
        anchor: ContextMenuAnchor::SelectedItem(PanelFocus::Queue),
        entries: vec![ContextMenuEntry {
            label: "Play",
            action: Some(ContextAction::Play),
        }],
        cursor: 0,
    }));
    harness.model_mut().sync_mounted_surfaces();
    assert!(harness.model().application.mounted(&menu_id));
    assert_eq!(
        harness.model().mouse_subscribed,
        std::iter::once(menu_id).collect(),
        "only the context menu is mouse-eligible while it is mounted"
    );

    harness.inject(wheel(5, 5));
    let outcome = harness.step();
    assert!(
        outcome
            .raw_messages
            .iter()
            .all(|msg| matches!(msg, Msg::TerminalEvent(_))),
        "the obscured queue must not receive the wheel once the menu is up"
    );
}

// --- Task 7.1: with Queue and the Library destination both visible and no
// overlay mounted, a click on each resolves through the real `tick()` sync
// order to that surface's own message (D2 exclusivity holds with two
// simultaneously eligible surfaces, not just one), and focus follows the
// click via the same `sync_mounted_surfaces` pass a real frame uses.

#[test]
fn simultaneous_queue_and_library_clicks_resolve_to_the_painting_component() {
    let mut app = crate::app::render::make_queue_app(2);
    app.panel_mode = PanelMode::Both;
    app.panel_focus = PanelFocus::Library;
    let mut harness = TickHarness::new(app);
    harness.model_mut().sync_mounted_surfaces();

    let library_child = harness
        .model()
        .emby_browser_id
        .clone()
        .expect("movie browser child mounted");
    let eligible = &harness.model().mouse_subscribed;
    assert!(
        eligible.contains(&ComponentId::Queue) && eligible.contains(&library_child),
        "Queue and the Library destination are simultaneously mouse-eligible with no overlay up: {eligible:?}"
    );
    assert_eq!(harness.model().application.focus(), Some(&library_child));

    let mut terminal = Terminal::new(TestBackend::new(100, 30)).unwrap();
    terminal
        .draw(|f| harness.model_mut().draw_frame(f, false, false))
        .unwrap();

    let (queue_rect, _) = *harness
        .model_mut()
        .application
        .get_component_mut(&ComponentId::Queue)
        .expect("queue mounted")
        .as_any_mut()
        .downcast_mut::<QueueComponent>()
        .expect("queue component type")
        .test_rows()
        .first()
        .expect("queue painted at least one row");

    let library_point = harness
        .model_mut()
        .application
        .get_component_mut(&library_child)
        .expect("library child mounted")
        .as_any_mut()
        .downcast_mut::<BrowserComponent>()
        .expect("browser component type")
        .test_layout()
        .left_area;
    assert!(
        library_point.width > 0 && library_point.height > 0,
        "the Library destination must have painted a non-empty list area"
    );

    let click = |column, row| {
        Event::Mouse(MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column,
            row,
            modifiers: KeyModifiers::NONE,
        })
    };

    // A click inside Queue's painted rows resolves to a Queue-specific
    // message, not a Library one, even though Library currently holds focus.
    harness.inject(click(queue_rect.x, queue_rect.y));
    let outcome = harness.step();
    assert!(
        outcome
            .raw_messages
            .iter()
            .any(|msg| matches!(msg, Msg::Shell(ShellRequest::QueueRowClick { .. }))),
        "a click on Queue's painted row must resolve through Queue"
    );
    assert!(
        outcome
            .raw_messages
            .iter()
            .all(|msg| !matches!(msg, Msg::Shell(ShellRequest::BrowserRowClick { .. }))),
        "the click on Queue must not also resolve through Library"
    );
    apply_outcome(&mut harness, outcome);
    harness.model_mut().sync_mounted_surfaces();
    assert_eq!(
        harness.model().application.focus(),
        Some(&ComponentId::Queue),
        "focus follows the click onto Queue"
    );

    // A click inside Library's painted list resolves to a Library-specific
    // message and focus follows back onto the Library destination.
    harness.inject(click(library_point.x, library_point.y));
    let outcome = harness.step();
    assert!(
        outcome
            .raw_messages
            .iter()
            .any(|msg| matches!(msg, Msg::Shell(ShellRequest::BrowserRowClick { .. }))),
        "a click on Library's painted list must resolve through the Library destination"
    );
    assert!(
        outcome
            .raw_messages
            .iter()
            .all(|msg| !matches!(msg, Msg::Shell(ShellRequest::QueueRowClick { .. }))),
        "the click on Library must not also resolve through Queue"
    );
    apply_outcome(&mut harness, outcome);
    harness.model_mut().sync_mounted_surfaces();
    assert_eq!(
        harness.model().application.focus(),
        Some(&library_child),
        "focus follows the click back onto the Library destination"
    );
}

// --- Task 7.3 (breakpoint half): the Movies destination switches its
// embedded canonical control (`WideMediaList` -> `InlineMediaBrowser`) when a
// resize crosses the wide/narrow breakpoint. A click must resolve against the
// `row_geometry` the CURRENT frame painted, never the rect a prior frame left
// behind. The scroll half of this proof already lives in
// `media_list::tests::resolve_point::wide_resolves_against_a_scrolled_viewport`.

#[test]
fn browser_row_click_resolves_against_the_current_breakpoints_geometry_not_a_stale_one() {
    let mut app = crate::app::render::make_movie_app();
    app.panel_focus = PanelFocus::Library;
    app.panel_mode = PanelMode::LibraryOnly;
    let mut harness = TickHarness::new(app);
    harness.model_mut().sync_mounted_surfaces();

    let library_child = harness
        .model()
        .emby_browser_id
        .clone()
        .expect("movie browser child mounted");

    let browser_test_layout = |harness: &mut TickHarness| {
        harness
            .model_mut()
            .application
            .get_component_mut(&library_child)
            .expect("library child mounted")
            .as_any_mut()
            .downcast_mut::<BrowserComponent>()
            .expect("browser component type")
            .test_layout()
            .left_area
    };

    let click = |column, row| {
        Event::Mouse(MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column,
            row,
            modifiers: KeyModifiers::NONE,
        })
    };

    // Wide breakpoint: paints via `WideMediaList` into the right-hand list
    // pane, well clear of column 0.
    let mut wide_terminal = Terminal::new(TestBackend::new(100, 30)).unwrap();
    wide_terminal
        .draw(|f| harness.model_mut().draw_frame(f, false, false))
        .unwrap();
    let wide_list_area = browser_test_layout(&mut harness);
    assert!(
        wide_list_area.width > 0 && wide_list_area.height > 0,
        "the wide breakpoint must have painted a non-empty list area"
    );

    harness.inject(click(wide_list_area.x, wide_list_area.y));
    let outcome = harness.step();
    assert!(
        outcome
            .raw_messages
            .iter()
            .any(|msg| matches!(msg, Msg::Shell(ShellRequest::BrowserRowClick { .. }))),
        "a click on the wide-painted list row must resolve through the canonical control"
    );
    apply_outcome(&mut harness, outcome);

    // Resize below the two-column threshold: the destination switches to
    // `InlineMediaBrowser`, and its list geometry starts far to the left of
    // where the wide list used to live.
    let mut narrow_terminal = Terminal::new(TestBackend::new(80, 24)).unwrap();
    narrow_terminal
        .draw(|f| harness.model_mut().draw_frame(f, false, false))
        .unwrap();
    let narrow_list_area = browser_test_layout(&mut harness);
    assert!(
        narrow_list_area.width > 0 && narrow_list_area.height > 0,
        "the narrow breakpoint must have painted a non-empty list area"
    );
    assert_ne!(
        wide_list_area, narrow_list_area,
        "the breakpoint change must have actually repainted different list geometry"
    );

    // A click on the OLD wide list's bottom row, now below the bottom edge
    // of the shorter narrow-painted list area (the wide browser pane sits
    // left of the hero and is vertically taller than the narrow list), must
    // not resolve to a row: if resolution consulted stale wide geometry
    // instead of the freshly painted narrow layout, this click would
    // incorrectly still land on a list row.
    let stale_probe_col = wide_list_area.x;
    let stale_probe_row = wide_list_area.bottom() - 1;
    assert!(
        stale_probe_row >= narrow_list_area.bottom()
            && stale_probe_col >= narrow_list_area.x
            && stale_probe_col < narrow_list_area.right(),
        "the old wide list must genuinely extend below the narrow list's new bottom edge at a \
         shared column for this click to be a meaningful stale-geometry probe"
    );
    harness.inject(click(stale_probe_col, stale_probe_row));
    let outcome = harness.step();
    assert!(
        outcome
            .raw_messages
            .iter()
            .all(|msg| !matches!(msg, Msg::Shell(ShellRequest::BrowserRowClick { .. }))),
        "a click at the old wide-list position must not resolve through stale wide geometry \
         after the narrow repaint"
    );
    apply_outcome(&mut harness, outcome);

    // A click inside the NEW narrow list area must resolve through the
    // now-mounted `InlineMediaBrowser`, proving the current geometry (not
    // memory of the old control) is what actually governs resolution.
    harness.inject(click(narrow_list_area.x, narrow_list_area.y));
    let outcome = harness.step();
    assert!(
        outcome
            .raw_messages
            .iter()
            .any(|msg| matches!(msg, Msg::Shell(ShellRequest::BrowserRowClick { .. }))),
        "a click on the narrow-painted list row must resolve through the canonical control"
    );
    apply_outcome(&mut harness, outcome);
}
