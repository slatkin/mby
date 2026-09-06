use super::*;
use crate::app::components::{BrowserComponent, Msg};
use crate::app::render::{make_large_movie_library_app, make_movie_app};
use crate::app::tests::{make_app_stub, make_item, make_items};
use crate::app::types_browse::BrowseResting;
use crate::app::{App, BrowseLevel, ContextAction, LibraryTab, PanelMode, TabSelection};
use ratatui::backend::TestBackend;
use ratatui::Terminal;
use tuirealm::event::{Event, Key, KeyEvent, KeyModifiers};

#[path = "shell_browser_group_tests.rs"]
mod group_tests;
#[path = "shell_browser_test_support.rs"]
mod test_support;
use test_support::*;

/// The mounted Movies browser must receive the wide letter-pill projection from
/// the shell. This checks the rendered output, rather than only the shared
/// breakpoint predicate: the pre-projection implementation leaves the row
/// empty even though `BrowserComponent::view` selects the wide layout.
#[test]
fn shell_emby_browser_wide_movies_renders_letter_pills() {
    let _guard = crate::config::TestStateDirGuard::new();
    let mut app = make_large_movie_library_app(1000);
    app.libs[0].nav_stack[0].items = make_items(1000);
    app.libs[0].nav_stack[0].total_count = 1000;
    app.panel_mode = PanelMode::LibraryOnly;
    let mut model = Model::new(app);
    model.sync_emby_browser();
    model.sync_active_destination();

    let backend = TestBackend::new(200, 40);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal
        .draw(|frame| {
            model.app.compose_base_frame(frame, None);
            model.render_emby_browser_component(frame);
        })
        .unwrap();

    let buffer = terminal.backend().buffer();
    let rendered = (0..buffer.area().height)
        .flat_map(|y| (0..buffer.area().width).map(move |x| buffer[(x, y)].symbol()))
        .collect::<String>();
    assert!(
        rendered.contains("A–C"),
        "wide Movies letter-pill row was not rendered: {rendered}"
    );
}

#[test]
fn shell_emby_browser_wide_movies_paints_one_item_per_row() {
    let _guard = crate::config::TestStateDirGuard::new();
    let mut app = make_large_movie_library_app(12);
    app.libs[0].nav_stack[0].items = make_items(12);
    app.libs[0].nav_stack[0].total_count = 12;
    app.panel_mode = PanelMode::LibraryOnly;
    let mut model = Model::new(app);
    model.sync_emby_browser();
    model.sync_active_destination();
    let id = model.emby_browser_id.clone().expect("browser mounted");
    let backend = TestBackend::new(200, 30);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal
        .draw(|frame| {
            model.app.compose_base_frame(frame, None);
            model.render_emby_browser_component(frame);
        })
        .unwrap();
    let rows = browser_component_painted_rows(&model, &id);
    let item_rows: Vec<&Vec<usize>> = rows.iter().filter(|row| !row.is_empty()).collect();
    assert!(
        item_rows.iter().all(|row| row.len() == 1),
        "wide rail painted multiple columns: {item_rows:?}"
    );
    let row_of = |item| {
        item_rows
            .iter()
            .position(|row| row.contains(&item))
            .expect("painted item")
    };
    assert_ne!(row_of(0), row_of(1));
    assert!(matches!(
        drive_browser_key(&mut model, &id, Key::Down, KeyModifiers::NONE),
        Some(Msg::Shell(ShellRequest::BrowserCursorIndex { index: 1 }))
    ));
}

#[test]
fn shell_emby_browser_wide_movies_guards_hero_to_movie_items() {
    let _guard = crate::config::TestStateDirGuard::new();

    let render_left_pane = |selected: usize, non_movie: bool| {
        let mut app = browser_app_with_folder_and_movie();
        if non_movie {
            app.libs[0].nav_stack[0].items[1].item_type = "BoxSet".into();
            app.libs[0].nav_stack[0].items[1].name = "Box Set".into();
        }
        app.libs[0].nav_stack[0].set_resting_cursor(selected);
        app.panel_mode = PanelMode::LibraryOnly;
        let mut model = Model::new(app);
        model.sync_emby_browser();
        model.sync_active_destination();

        let backend = TestBackend::new(200, 30);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal
            .draw(|frame| {
                model.app.compose_base_frame(frame, None);
                model.render_emby_browser_component(frame);
            })
            .unwrap();

        let area = crate::app::render::wide_library_panes(model.app.layout.main.left_area, 2, 1)
            .expect("wide browser panes")
            .hero_area;
        let buffer = terminal.backend().buffer();
        let mut rendered = String::new();
        for y in 0..area.height {
            for x in 0..area.width {
                rendered.push_str(buffer[(area.x + x, area.y + y)].symbol());
            }
        }
        rendered
    };

    let folder_pane = render_left_pane(0, false);
    assert!(
        !folder_pane.contains("Folder A"),
        "folder selection must not paint a wide hero card: {folder_pane}"
    );

    let non_movie_pane = render_left_pane(1, true);
    assert!(
        !non_movie_pane.contains("Box Set"),
        "non-Movie selection must not paint a wide hero card: {non_movie_pane}"
    );

    let movie_pane = render_left_pane(1, false);
    assert!(
        movie_pane.contains("Movie B"),
        "Movie selection must paint the wide hero card: {movie_pane}"
    );
}

/// Task 5.3d, Emby browser effect decoupling: `BrowserComponent` resolves
/// its own selected `EmbyItem` from its component-local cursor over the
/// mirrored content, and the shell routes each typed effect to an `App`
/// handler that acts on the supplied item directly (never by copying the
/// component cursor into a `BrowseLevel.cursor` and re-reading it).
///
/// The regression parks App's nav cursor on the folder at the top of the
/// list while the component selects the playable movie below it — the
/// legacy Enter/Ctrl+P/Ctrl+A/Ctrl+W/'.' arms on the parked folder would
/// navigate into the folder, play the folder, enqueue the folder, toggle
/// the folder, or raise the folder-scoped context menu respectively — and
/// proves each of the five effects acts on the component-selected movie
/// instead. Requests are captured from the mounted component itself, so
/// the emitted payload is the component-resolved item; every assertion is
/// on the effect's outcome (nav-stack depth/cursor, queued item id, the
/// unavailable-Service toast, or the raised menu's actions), never on a
/// hand-set coordinate.
#[test]
fn shell_emby_browser_effects_honor_component_target() {
    let _guard = crate::config::TestStateDirGuard::new();
    let mut model = Model::new(browser_app_with_folder_and_movie());
    model.sync_emby_browser();
    model.sync_active_destination();
    let id = model.emby_browser_id.clone().expect("browser mounted");

    // Drive the component cursor onto the movie (index 1) while App's
    // nav cursor stays parked on the folder (index 0). The movement key
    // now returns the typed rows request (task 5.3d, Emby browser local
    // navigation) — the component cursor still advances in place.
    assert!(matches!(
        drive_browser_key(&mut model, &id, Key::Down, KeyModifiers::NONE),
        Some(Msg::Shell(ShellRequest::BrowserCursorIndex { index: 1 }))
    ));

    // Enter: the component emits BrowserActivate for its own selected
    // movie; routed with App's cursor parked on the folder, the effect
    // activates the supplied movie (cursor jumps to it, nav stack does
    // NOT grow into the folder, and the emby-gated play flashes the
    // unavailable Service) instead of the legacy folder navigation.
    let Some(Msg::Shell(ShellRequest::BrowserActivate { item })) =
        drive_browser_key(&mut model, &id, Key::Enter, KeyModifiers::NONE)
    else {
        panic!("browser Enter must emit BrowserActivate, got no typed request");
    };
    assert_eq!(
        item.id, "movie-b",
        "component must resolve its own selection"
    );
    model.app.libs[0].nav_stack[0].set_resting_cursor(0);
    model.handle_browser_request(ShellRequest::BrowserActivate { item });
    assert_eq!(
        model.app.libs[0].nav_stack.len(),
        1,
        "playable activation must not navigate into the parked folder"
    );
    assert_eq!(
        model.app.libs[0].nav_stack[0].resting().cursor(),
        1,
        "the effect must select the supplied movie, not the parked cursor"
    );
    assert_eq!(model.app.status, "Emby is unavailable");

    // Ctrl+P: non-folder activation of the supplied movie, again with
    // the App cursor re-parked on the folder — same decisive signals as
    // Enter (folder play would have diverted to `play_folder`).
    model.app.status.clear();
    let Some(Msg::Shell(ShellRequest::BrowserPlay { item })) =
        drive_browser_key(&mut model, &id, Key::Char('p'), KeyModifiers::CONTROL)
    else {
        panic!("browser Ctrl+P must emit BrowserPlay, got no typed request");
    };
    assert_eq!(item.id, "movie-b");
    model.app.libs[0].nav_stack[0].set_resting_cursor(0);
    model.handle_browser_request(ShellRequest::BrowserPlay { item });
    assert_eq!(model.app.libs[0].nav_stack.len(), 1);
    assert_eq!(model.app.libs[0].nav_stack[0].resting().cursor(), 1);
    assert_eq!(model.app.status, "Emby is unavailable");

    // Ctrl+A: the supplied movie (not the parked folder) is enqueued.
    model.app.status.clear();
    let Some(Msg::Shell(ShellRequest::BrowserEnqueue { item })) =
        drive_browser_key(&mut model, &id, Key::Char('a'), KeyModifiers::CONTROL)
    else {
        panic!("browser Ctrl+A must emit BrowserEnqueue, got no typed request");
    };
    assert_eq!(item.id, "movie-b");
    model.app.libs[0].nav_stack[0].set_resting_cursor(0);
    model.handle_browser_request(ShellRequest::BrowserEnqueue { item });
    let queued = model.app.player_tab.emby_items();
    assert_eq!(queued.len(), 1);
    assert_eq!(
        queued[0].id, "movie-b",
        "enqueue must queue the supplied movie, not the parked folder"
    );

    // Ctrl+W: the supplied movie is toggled (the emby-gated effect
    // flashes the unavailable Service) even though a legacy arm on the
    // parked folder would skip silently via the folder guard.
    model.app.status.clear();
    let Some(Msg::Shell(ShellRequest::BrowserToggleWatched { item })) =
        drive_browser_key(&mut model, &id, Key::Char('w'), KeyModifiers::CONTROL)
    else {
        panic!("browser Ctrl+W must emit BrowserToggleWatched, got no typed request");
    };
    assert_eq!(item.id, "movie-b");
    model.app.libs[0].nav_stack[0].set_resting_cursor(0);
    model.handle_browser_request(ShellRequest::BrowserToggleWatched { item });
    assert_eq!(
        model.app.status, "Emby is unavailable",
        "watched toggle must act on the supplied movie, not skip on the parked folder"
    );

    // '.': the component emits BrowserContextMenu for its own selected
    // movie; the shell raises the menu for that supplied item via
    // `open_context_menu_for`. Legacy resolution on the parked folder
    // would raise the folder-scoped menu (Play All/Shuffle/Add to Queue),
    // so the menu must offer the generic per-item Play and no folder
    // actions — decisive that the menu targets the component-selected
    // movie, not the parked `BrowseLevel` cursor.
    let Some(Msg::Shell(ShellRequest::BrowserContextMenu { item })) =
        drive_browser_key(&mut model, &id, Key::Char('.'), KeyModifiers::NONE)
    else {
        panic!("browser '.' must emit BrowserContextMenu, got no typed request");
    };
    assert_eq!(item.id, "movie-b");
    model.app.libs[0].nav_stack[0].set_resting_cursor(0);
    model.handle_browser_request(ShellRequest::BrowserContextMenu { item });
    let menu = match model.app.pending_overlay.as_ref() {
        Some(crate::app::types_overlay::OverlayRequest::ContextMenu(menu)) => menu,
        _ => panic!("context menu must open for the supplied movie"),
    };
    let actions: Vec<_> = menu
        .entries
        .iter()
        .filter_map(|e| e.action.clone())
        .collect();
    assert!(
        actions.iter().any(|a| matches!(a, ContextAction::Play)),
        "context menu must offer the generic per-item Play, got: {actions:?}"
    );
    assert!(
        !actions.iter().any(|a| matches!(
            a,
            ContextAction::PlayFolder(_)
                | ContextAction::ShuffleFolder(_)
                | ContextAction::EnqueueFolder(_)
        )),
        "context menu must target the supplied movie, not the parked folder, got: {actions:?}"
    );

    // Ctrl+S: the component emits BrowserShuffle carrying its own
    // selected movie — not the parked folder that a legacy `shuffle_play`
    // on the App cursor would have resolved. The shell's preserved
    // `shuffle_play` tail then takes the non-folder branch (current
    // browse-level parent) for the supplied movie; the emitted payload is
    // decisive that the component-local cursor selected the target.
    model.app.status.clear();
    let Some(Msg::Shell(ShellRequest::BrowserShuffle { item })) =
        drive_browser_key(&mut model, &id, Key::Char('s'), KeyModifiers::CONTROL)
    else {
        panic!("browser Ctrl+S must emit BrowserShuffle, got no typed request");
    };
    assert_eq!(
        item.id, "movie-b",
        "shuffle must carry the component-selected movie, not the parked BrowseLevel.cursor folder"
    );

    // Bare `r` refreshes the active Emby library (task 5.3d, Emby browser
    // refresh): the component emits `BrowserRefresh`, and the shell derives
    // the library index from its own tab state and runs `App::refresh_lib`,
    // which lifts the current nav level's `loading` flag.
    let Some(Msg::Shell(ShellRequest::BrowserRefresh)) =
        drive_browser_key(&mut model, &id, Key::Char('r'), KeyModifiers::NONE)
    else {
        panic!("browser bare r must emit BrowserRefresh, got no typed request");
    };
    model.handle_browser_request(ShellRequest::BrowserRefresh);
    assert!(
        model.app.libs[0].nav_stack[0].loading,
        "refresh must lift the active library nav level's loading flag"
    );

    // Legacy Alt+`r` preserves a bare-refresh, not a rescan: the CONTROL
    // arm is guarded by the CONTROL modifier, Alt does not set it, and the
    // bare `r` arm below it catches Alt+`r` — exactly the legacy
    // `handle_lib_key` ordering.
    let Some(Msg::Shell(ShellRequest::BrowserRefresh)) =
        drive_browser_key(&mut model, &id, Key::Char('r'), KeyModifiers::ALT)
    else {
        panic!("browser Alt+r must still emit BrowserRefresh, got no typed request");
    };

    // Ctrl+`r` raises the Rescan Library confirmation (task 5.3d, Emby
    // browser rescan): the component emits `BrowserRescan`, and the shell
    // raises the same confirm modal (title/message/hint and
    // `ConfirmAction::RescanLibrary(lib_idx)`) the legacy arm raised.
    let Some(Msg::Shell(ShellRequest::BrowserRescan)) =
        drive_browser_key(&mut model, &id, Key::Char('r'), KeyModifiers::CONTROL)
    else {
        panic!("browser Ctrl+r must emit BrowserRescan, got no typed request");
    };
    model.handle_browser_request(ShellRequest::BrowserRescan);
    match model.app.pending_overlay.as_ref() {
        Some(crate::app::types_overlay::OverlayRequest::Confirm(modal)) => {
            assert_eq!(modal.title, " Rescan Library ");
            assert!(matches!(
                modal.on_confirm,
                crate::app::ConfirmAction::RescanLibrary(0)
            ));
            assert_eq!(modal.message, "Rescan 'Movies'?");
        }
        _ => panic!("Ctrl+r must raise the Rescan Library confirmation"),
    }

    // Esc/Backspace use the typed browser-back request (task 5.3d,
    // Emby browser back): with the browser focused, both keys emit a
    // typed `BrowserBack` — not a raw legacy key — and the shell routes
    // it to `App::go_back`, which pops the child level and restores the
    // parent cursor to the folder the child came from. Drive the parent
    // cursor off the folder first so the restoration is observable.
    model.app.libs[0].nav_stack[0].set_resting_cursor(1);
    model.app.libs[0].nav_stack.push(BrowseLevel {
        parent_id: "folder-a".into(),
        title: "Folder A".into(),
        items: vec![],
        total_count: 0,
        resting: BrowseResting::new(0, 0),
        item_types: None,
        unplayed_only: false,
        sort_by: "SortName".into(),
        sort_order: "Ascending".into(),
        loading: false,
        all_items: None,
        letter_filter: None,
        music_grouping: None,
    });
    let Some(Msg::Shell(ShellRequest::BrowserBack)) =
        drive_browser_key(&mut model, &id, Key::Esc, KeyModifiers::NONE)
    else {
        panic!("focused browser Esc must emit BrowserBack, got no typed request");
    };
    model.handle_browser_request(ShellRequest::BrowserBack);
    assert_eq!(
        model.app.libs[0].nav_stack.len(),
        1,
        "BrowserBack must pop the child browse level via go_back"
    );
    assert_eq!(
        model.app.libs[0].nav_stack[0].resting().cursor(),
        0,
        "go_back must restore the parent cursor to the folder the child came from"
    );

    // Backspace routes the same way (the legacy arm matched both keys
    // with no modifier guard).
    let Some(Msg::Shell(ShellRequest::BrowserBack)) =
        drive_browser_key(&mut model, &id, Key::Backspace, KeyModifiers::NONE)
    else {
        panic!("focused browser Backspace must emit BrowserBack, got no typed request");
    };

    // `[`/`]` cycle the letter-range pill row (task 5.3d, Emby browser
    // selector cycling): the focused browser emits a typed
    // `BrowserCycleLetterPill` carrying the delta — never a raw legacy
    // key — and the shell derives the library index from its own tab
    // state and runs `App::cycle_letter_pill`, whose select effect lands
    // on the top browse level. The fixture's Movies library already sits
    // at its top browse level, so capturing a true total is the only
    // missing `should_show_letter_pills` piece.
    model.app.libs[0].library_total = Some(1000);
    let Some(Msg::Shell(ShellRequest::BrowserCycleLetterPill { delta })) =
        drive_browser_key(&mut model, &id, Key::Char(']'), KeyModifiers::NONE)
    else {
        panic!("focused browser ] must emit BrowserCycleLetterPill, got no typed request");
    };
    assert_eq!(delta, 1, "']' must carry +1");
    model.handle_browser_request(ShellRequest::BrowserCycleLetterPill { delta });
    assert_eq!(
        model.app.libs[0].nav_stack[0]
            .letter_filter
            .as_ref()
            .map(|f| f.index),
        Some(1),
        "']' must advance from the default A\u{2013}C pill to the next bucket"
    );

    // `[` cycles back the other way (the default is bucket 0, so this
    // round-trips to it).
    let Some(Msg::Shell(ShellRequest::BrowserCycleLetterPill { delta })) =
        drive_browser_key(&mut model, &id, Key::Char('['), KeyModifiers::NONE)
    else {
        panic!("focused browser [ must emit BrowserCycleLetterPill, got no typed request");
    };
    assert_eq!(delta, -1, "'[' must carry -1");
    model.handle_browser_request(ShellRequest::BrowserCycleLetterPill { delta });
    assert_eq!(
        model.app.libs[0].nav_stack[0]
            .letter_filter
            .as_ref()
            .map(|f| f.index),
        Some(0),
        "'[' must cycle back to the A\u{2013}C pill"
    );

    // Ctrl/Alt brackets are NOT letter-pill cycling: the legacy guard
    // excluded CONTROL and ALT, so those combinations remain unclaimed by
    // the component and are left to the central router.
    assert_eq!(
        drive_browser_key(&mut model, &id, Key::Char('['), KeyModifiers::CONTROL),
        None
    );
    assert_eq!(
        drive_browser_key(&mut model, &id, Key::Char(']'), KeyModifiers::ALT),
        None
    );
}

fn browser_app_with_folder_and_movie() -> App {
    let mut app = make_app_stub();
    app.tab = TabSelection::EmbyLibrary(0);

    let mut library = make_item("Movies", "CollectionFolder");
    library.id = "lib-movies".into();
    library.is_folder = true;
    library.collection_type = "movies".into();

    let mut folder = make_item("Folder A", "CollectionFolder");
    folder.id = "folder-a".into();
    folder.is_folder = true;

    let mut movie = make_item("Movie B", "Movie");
    movie.id = "movie-b".into();

    app.libs.push(LibraryTab {
        nav_stack: vec![BrowseLevel {
            parent_id: "lib-movies".into(),
            title: "Movies".into(),
            items: vec![folder, movie],
            total_count: 2,
            resting: BrowseResting::new(0, 0),
            item_types: None,
            unplayed_only: false,
            sort_by: "SortName".into(),
            sort_order: "Ascending".into(),
            loading: false,
            all_items: None,
            letter_filter: None,
            music_grouping: None,
        }],
        ..LibraryTab::new(library)
    });

    app
}

#[test]
fn shell_mounts_and_syncs_the_generic_emby_browser() {
    let mut model = Model::new(make_movie_app());
    model.sync_emby_browser();
    model.sync_active_destination();
    let id = model.emby_browser_id.clone().expect("browser mounted");
    let message = {
        model
            .application
            .get_component_mut(&id)
            .unwrap()
            .on(&Event::Keyboard(KeyEvent {
                code: Key::Down,
                modifiers: KeyModifiers::NONE,
            }))
    };
    // The focused browser's Down now routes through the typed rows
    // request (task 5.3d, Emby browser local navigation) instead of
    // forwarding the raw legacy key; the shell arm moves the App cursor
    // through `App::move_lib_cursor_rows` the way `handle_lib_key` did.
    let Some(Msg::Shell(ShellRequest::BrowserCursorIndex { index })) = message else {
        panic!("browser movement should emit the typed index request");
    };
    assert_eq!(index, 1, "Down must resolve to item 1");
    model.handle_browser_request(ShellRequest::BrowserCursorIndex { index });
    model.sync_emby_browser();
    model.sync_active_destination();
    assert_eq!(model.app.libs[0].nav_stack[0].resting().cursor(), 1);
    assert!(model
        .application
        .get_component(&id)
        .unwrap()
        .as_any()
        .downcast_ref::<BrowserComponent>()
        .is_some());
}

/// Task 5.3d, Emby browser local navigation through the Model boundary:
/// the focused `BrowserComponent` returns typed `BrowserMoveRows` /
/// `BrowserMoveColumn` / `BrowserJumpCursor` requests in place of the
/// raw legacy key, and the shell derives the active Emby library index
/// from its own tab state and runs the same `App` cursor methods the
/// legacy `handle_lib_key` movement arms call. The App cursor must move
/// through that typed path (never a raw cursor-field write): a
/// two-column painted list strides the App cursor by the column count
/// per row (Down +2), Home/End jump to the first/last item, and
/// Left/Right move within the row; a one-column list keeps Left/Right/
/// h/l unbound (raw key consumed by the component without movement,
/// App cursor unchanged) while the row keys keep their typed
/// stride of one.
#[test]
fn shell_emby_browser_movement_drives_app_cursor_via_typed_requests() {
    let _guard = crate::config::TestStateDirGuard::new();
    let mut app = browser_app_with_flat_movies(10);
    // LibraryOnly hides the queue column so the library pane spans the
    // full window and clears the two-column threshold at render width;
    // the panel mode is a state the app already supports, not hand-set
    // layout rects (the whole frame is painted into a TestBackend).
    app.panel_mode = PanelMode::LibraryOnly;
    let mut model = Model::new(app);
    model.sync_emby_browser();
    model.sync_active_destination();
    let id = model.emby_browser_id.clone().expect("browser mounted");

    // Paint the App and the mounted browser at 150 columns: both derive
    // the same two-column stride from the same painted geometry (the
    // generic library never takes the wide-Movies 1-column rail).
    render_browser_model(&mut model, 150, 40);
    model.sync_emby_browser();
    model.sync_active_destination();

    // Down: the focused component returns `BrowserMoveRows { rows: 1 }`
    // (one display row, in place of the raw key), and the shell runs
    // `App::move_lib_cursor_rows` — its own painted two-column stride
    // lands the App cursor on item 2, exactly like the legacy arm.
    let Some(Msg::Shell(ShellRequest::BrowserCursorIndex { index })) =
        drive_browser_key(&mut model, &id, Key::Down, KeyModifiers::NONE)
    else {
        panic!("focused browser Down must emit BrowserCursorIndex, got no typed request");
    };
    assert_eq!(index, 2, "Down must resolve to item 2");
    let navigation_before = model.app.last_nav_at;
    model.app.library_position_dirty = false;
    model.handle_browser_request(ShellRequest::BrowserCursorIndex { index });
    assert_eq!(
        model.app.libs[0].nav_stack[0].resting().cursor(),
        2,
        "two-column Down must apply the component-resolved index"
    );
    assert!(
        model.app.library_position_dirty,
        "cursor application must persist the library position"
    );
    assert!(
        model.app.last_nav_at > navigation_before,
        "cursor application must mark library navigation"
    );
    assert_eq!(
        model.app.library_position_state.libraries["lib-films"].levels[0].cursor_index, 2,
        "the single cursor application must persist the resolved index"
    );
    let component_cursor = model
        .application
        .get_component(&id)
        .unwrap()
        .as_any()
        .downcast_ref::<BrowserComponent>()
        .unwrap()
        .cursor();
    assert_eq!(
        component_cursor, 2,
        "component cursor remains locally resolved"
    );

    // End/Home jump the App cursor to the last/first item through
    // `App::jump_lib_cursor`.
    let Some(Msg::Shell(ShellRequest::BrowserCursorIndex { index })) =
        drive_browser_key(&mut model, &id, Key::End, KeyModifiers::NONE)
    else {
        panic!("focused browser End must emit BrowserCursorIndex, got no typed request");
    };
    assert_eq!(index, 9, "End must resolve to the last item");
    model.handle_browser_request(ShellRequest::BrowserCursorIndex { index });
    assert_eq!(model.app.libs[0].nav_stack[0].resting().cursor(), 9);
    let Some(Msg::Shell(ShellRequest::BrowserCursorIndex { index })) =
        drive_browser_key(&mut model, &id, Key::Home, KeyModifiers::NONE)
    else {
        panic!("focused browser Home must emit BrowserCursorIndex, got no typed request");
    };
    assert_eq!(index, 0, "Home must resolve to the first item");
    model.handle_browser_request(ShellRequest::BrowserCursorIndex { index });
    assert_eq!(model.app.libs[0].nav_stack[0].resting().cursor(), 0);

    // Right/Left move the App cursor within the row via
    // `App::move_lib_cursor` (the two-column list claims them).
    let Some(Msg::Shell(ShellRequest::BrowserCursorIndex { index })) =
        drive_browser_key(&mut model, &id, Key::Right, KeyModifiers::NONE)
    else {
        panic!(
            "focused two-column browser Right must emit BrowserCursorIndex, got no typed request"
        );
    };
    assert_eq!(index, 1, "Right must resolve to item 1");
    model.handle_browser_request(ShellRequest::BrowserCursorIndex { index });
    assert_eq!(model.app.libs[0].nav_stack[0].resting().cursor(), 1);
    let Some(Msg::Shell(ShellRequest::BrowserCursorIndex { index })) =
        drive_browser_key(&mut model, &id, Key::Char('h'), KeyModifiers::NONE)
    else {
        panic!("focused two-column browser h must emit BrowserCursorIndex, got no typed request");
    };
    assert_eq!(index, 0, "h must resolve to item 0");
    model.handle_browser_request(ShellRequest::BrowserCursorIndex { index });
    assert_eq!(model.app.libs[0].nav_stack[0].resting().cursor(), 0);

    // One-column list: Left/Right/h/l stay unbound locally with no movement
    // request, leaving the App cursor unchanged.
    model.app.panel_mode = PanelMode::Both;
    render_browser_model(&mut model, 100, 40);
    model.sync_emby_browser();
    model.sync_active_destination();
    for key in [Key::Left, Key::Right, Key::Char('h'), Key::Char('l')] {
        assert_eq!(
            drive_browser_key(&mut model, &id, key, KeyModifiers::NONE),
            None,
            "one-column focused {key:?} must stay unclaimed"
        );
    }
    let comp_cursor = model
        .application
        .get_component(&id)
        .unwrap()
        .as_any()
        .downcast_ref::<BrowserComponent>()
        .unwrap()
        .cursor();
    assert_eq!(
        comp_cursor, 0,
        "one-column Left/Right/h/l must not move the component cursor"
    );
    assert_eq!(
        model.app.libs[0].nav_stack[0].resting().cursor(),
        0,
        "one-column Left/Right/h/l must not move the App cursor"
    );
    let Some(Msg::Shell(ShellRequest::BrowserCursorIndex { index })) =
        drive_browser_key(&mut model, &id, Key::Down, KeyModifiers::NONE)
    else {
        panic!(
            "focused one-column browser Down must still emit BrowserCursorIndex, got no typed request"
        );
    };
    assert_eq!(index, 1);
    model.handle_browser_request(ShellRequest::BrowserCursorIndex { index });
    assert_eq!(
        model.app.libs[0].nav_stack[0].resting().cursor(),
        1,
        "one-column Down must stride the App cursor one item"
    );
}

/// Task 3.4a: at narrow TV width (`BrowserComponent`'s only TV mount),
/// `BrowserActivate` on a `Series` item must reopen the season-selection
/// modal via the shared Series-activation gate rather than drill in flat
/// through `select_item`. A non-Series folder item still drills in.
#[test]
fn browser_activate_series_opens_selection_modal_at_narrow_width() {
    let _guard = crate::config::TestStateDirGuard::new();
    let mut model = Model::new(browser_app_with_folder_and_movie());
    model.app.libs[0].library.collection_type = "tvshows".into();
    model.sync_emby_browser();
    model.sync_active_destination();

    let mut series = make_item("Show A", "Series");
    series.id = "series-a".into();
    series.is_folder = true;

    // Pre-fix: this routed through `select_item` and grew the nav stack
    // instead of opening the modal, so `pending_overlay` stayed `None`.
    model.handle_browser_request(ShellRequest::BrowserActivate { item: series });
    match model.app.pending_overlay.as_ref() {
        Some(crate::app::types_overlay::OverlayRequest::SelectionModal(modal)) => {
            match &modal.source {
                crate::app::types_selection_modal::SelectionModalSource::Series { series_id } => {
                    assert_eq!(series_id.as_str(), "series-a");
                }
                _ => panic!("narrow Series activation must open a Series selection modal"),
            }
        }
        _ => panic!("narrow Series activation must open the series selection modal"),
    }
    assert_eq!(
        model.app.libs[0].nav_stack.len(),
        1,
        "narrow Series activation must not drill into the series"
    );

    // A non-Series folder item still drills in unchanged.
    let mut folder = make_item("Folder A", "CollectionFolder");
    folder.id = "folder-a".into();
    folder.is_folder = true;
    model.handle_browser_request(ShellRequest::BrowserActivate { item: folder });
    assert_eq!(model.app.libs[0].nav_stack.len(), 2);
}

#[test]
fn browser_navigation_persists_live_scroll_at_level_boundaries() {
    let _guard = crate::config::TestStateDirGuard::new();
    let mut model = Model::new(browser_app_with_folder_and_movie());
    model.app.libs[0].nav_stack[0].set_resting_scroll(7);
    model.sync_emby_browser();
    model.sync_active_destination();
    let mut folder = make_item("Folder A", "CollectionFolder");
    folder.id = "folder-a".into();
    folder.is_folder = true;

    model.handle_browser_request(ShellRequest::BrowserActivate { item: folder });
    assert_eq!(model.app.libs[0].nav_stack.len(), 2);
    assert_eq!(model.app.libs[0].nav_stack[0].resting().scroll(), 7);
    assert_eq!(
        model.app.library_position_state.libraries["lib-movies"].levels[0].cursor_index,
        0
    );

    model.app.libs[0].nav_stack[1].set_resting_scroll(3);
    model.sync_emby_browser();
    model.sync_active_destination();
    model.handle_browser_request(ShellRequest::BrowserBack);
    assert_eq!(model.app.libs[0].nav_stack.len(), 1);
    assert_eq!(model.app.libs[0].nav_stack[0].resting().scroll(), 7);
}

#[test]
fn teardown_flush_captures_live_browser_scroll_without_navigation() {
    let _guard = crate::config::TestStateDirGuard::new();
    let mut model = Model::new(browser_app_with_folder_and_movie());
    model.app.libs[0].nav_stack[0].set_resting_scroll(6);
    model.sync_emby_browser();
    model.sync_active_destination();
    model.app.libs[0].nav_stack[0].set_resting_scroll(0);

    model.persist_emby_browser_scroll_for_active_library();
    model.app.flush_library_position_now();

    assert_eq!(model.app.libs[0].nav_stack[0].resting().scroll(), 6);
    assert!(!model.app.library_position_dirty);
}
