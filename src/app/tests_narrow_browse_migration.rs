//! Regression coverage for the narrow browse surfaces, seeded by
//! `migrate-narrow-browse-to-components` (archived) and since grown to cover
//! adjacent feeds/hero work. All tests are live; none are `#[ignore]`d.
//!
//! Groups:
//! - Saved-position restore seam (`LibEvent::RestoreLibraryPosition`): restore
//!   still writes the resting `BrowseLevel` cursor a later content projection
//!   hands the owning component.
//! - Painted-selection movement under `j`/`k` for TV and grouped music.
//! - `*_paints_each_browse_row_once`: the double-paint guard — assert on the
//!   `TestBackend` buffer through the full `Model::draw_frame` path, red only
//!   if both the legacy painter and the component `view` run for one surface.
//! - `feed_home_video_group_*`: shared inline/wide hero placement, scroll, and
//!   frame completeness for the Home feed video group.
//! - `wide_podcast_*`: wide Audiobookshelf podcast body snapshot/paint.

use super::*;
use crate::app::components::BrowserComponent;
use crate::app::shell::Model;
use crate::app::tests::*;
use crate::app::{BrowseLevel, LibraryTab, PanelFocus, TabSelection};
use ratatui::backend::TestBackend;
use ratatui::Terminal;
use tuirealm::event::{Event, Key, KeyEvent, KeyModifiers, MouseEvent, MouseEventKind};

fn saved_level(
    parent_id: &str,
    title: &str,
    focused_item_id: &str,
    item_types: Option<&str>,
) -> crate::config::LibraryPositionLevel {
    crate::config::LibraryPositionLevel {
        parent_id: parent_id.into(),
        title: title.into(),
        focused_item_id: Some(focused_item_id.into()),
        cursor_index: 0,
        item_types: item_types.map(Into::into),
        unplayed_only: false,
        sort_by: "SortName".into(),
        sort_order: "Ascending".into(),
        letter_filter_index: None,
        library_total: None,
    }
}

fn folder_items(prefix: &str, item_type: &str, n: usize) -> Vec<mbv_core::api::EmbyItem> {
    (0..n)
        .map(|i| {
            let mut item = make_item(&format!("{prefix} {i}"), item_type);
            item.id = format!("{prefix}-{i}");
            item.is_folder = true;
            item
        })
        .collect()
}

// ── Characterization: saved-position restore (green now, green after) ─────────

/// Entering a narrow Emby TV library restores its saved series position: the
/// restored top browse level lands its cursor on the saved `focused_item_id`,
/// not index 0.
#[test]
fn narrow_tv_library_restores_saved_series_position() {
    let mut app = make_app_stub();
    app.terminal_width = 60;
    app.terminal_height = 20;
    app.panel_focus = PanelFocus::Queue;
    app.tab = TabSelection::EmbyLibrary(0);

    let mut library = make_item("Shows", "CollectionFolder");
    library.id = "lib-shows".into();
    library.collection_type = "tvshows".into();
    app.libs.push(LibraryTab::new(library));

    let saved = saved_level("lib-shows", "Shows", "Series-3", Some("Series"));
    let position = crate::config::LibraryPosition {
        levels: vec![saved.clone()],
        ..Default::default()
    };
    app.replace_saved_library_position(0, position.clone());

    let level =
        BrowseLevel::from_position_level(&saved, folder_items("Series", "Series", 5), 5, 10);
    app.handle_lib_event(LibEvent::RestoreLibraryPosition {
        lib_idx: 0,
        requested_position: position.clone(),
        position,
        nav_stack: vec![level],
    });

    assert_eq!(
        app.libs[0].nav_stack[0].resting().cursor(),
        3,
        "entering the narrow TV library must restore the saved series (Series-3 at index 3)"
    );
}

/// Entering a narrow Emby grouped-Music library restores its saved album
/// position: the restored album (child) browse level lands its cursor on the
/// saved album `focused_item_id`.
#[test]
fn narrow_grouped_music_library_restores_saved_album_position() {
    let mut app = make_app_stub();
    app.terminal_width = 60;
    app.terminal_height = 20;
    app.panel_focus = PanelFocus::Queue;
    app.tab = TabSelection::EmbyLibrary(0);
    app.music_levels = vec!["group".into(), "album".into()];

    let mut library = make_item("Music", "CollectionFolder");
    library.id = "lib-music".into();
    library.collection_type = "music".into();
    app.libs.push(LibraryTab::new(library));

    let group_level = saved_level("lib-music", "Music", "group-1", None);
    let album_level = saved_level("group-1", "Beta", "album-2", None);
    let position = crate::config::LibraryPosition {
        levels: vec![group_level.clone(), album_level.clone()],
        ..Default::default()
    };
    app.replace_saved_library_position(0, position.clone());

    let groups = folder_items("group", "MusicArtist", 3);
    let albums: Vec<mbv_core::api::EmbyItem> = (0..4)
        .map(|i| {
            let mut album = make_item(&format!("Album {i}"), "MusicAlbum");
            album.id = format!("album-{i}");
            album
        })
        .collect();
    let nav_stack = vec![
        BrowseLevel::from_position_level(&group_level, groups, 3, 10),
        BrowseLevel::from_position_level(&album_level, albums, 4, 10),
    ];
    app.handle_lib_event(LibEvent::RestoreLibraryPosition {
        lib_idx: 0,
        requested_position: position.clone(),
        position,
        nav_stack,
    });

    assert_eq!(
        app.libs[0].nav_stack.len(),
        2,
        "grouped path must survive restore"
    );
    assert_eq!(
        app.libs[0].nav_stack[1].resting().cursor(), 2,
        "entering the narrow grouped Music library must restore the saved album (album-2 at index 2)"
    );
}

// ── Regression markers: red until the named task ─────────────────────────────

fn narrow_backend() -> Terminal<TestBackend> {
    Terminal::new(TestBackend::new(60, 20)).unwrap()
}

fn buffer_text(term: &Terminal<TestBackend>) -> String {
    let buf = term.backend().buffer();
    let area = *buf.area();
    (0..area.height)
        .map(|y| {
            (0..area.width)
                .map(|x| buf[(x, y)].symbol())
                .collect::<String>()
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn draw(model: &mut Model, term: &mut Terminal<TestBackend>) -> String {
    term.draw(|f| model.draw_frame(f, false, false)).unwrap();
    buffer_text(term)
}

/// Feed one key into whatever component currently holds focus and route any
/// emitted `Msg` the way the run loop does. Pre-migration the narrow browse
/// surfaces have no owning component, so focus rests on `UiRoot` and the key
/// is dead — which is exactly what the ignored tests document.
fn press(model: &mut Model, code: Key) {
    let focused = model.application.focus().cloned();
    if let Some(id) = &focused {
        let msg = model
            .application
            .get_component_mut(id)
            .expect("focused component mounted")
            .on(&Event::Keyboard(KeyEvent {
                code,
                modifiers: KeyModifiers::NONE,
            }));
        if let Some(msg) = msg {
            let mut music_resize = false;
            let mut tv_resize = false;
            model.handle_terminal_message(msg, &mut music_resize, &mut tv_resize);
        }
    }
    model.sync_mounted_surfaces();
}

fn tv_shows_app() -> App {
    let mut app = make_app_stub();
    app.terminal_width = 60;
    app.terminal_height = 20;
    app.mini_view_focus = PanelFocus::Library;
    app.tab = TabSelection::EmbyLibrary(0);

    let mut library = make_item("Shows", "CollectionFolder");
    library.id = "lib-shows".into();
    library.collection_type = "tvshows".into();
    library.is_folder = true;

    app.libs.push(LibraryTab {
        nav_stack: vec![BrowseLevel {
            parent_id: "lib-shows".into(),
            title: "Shows".into(),
            items: folder_items("Series", "Series", 5),
            total_count: 5,
            resting: crate::app::types_browse::BrowseResting::new(0, 0),
            item_types: Some("Series".into()),
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

/// Regression 3: narrow TV `j` moves the painted selection. Post-task-3.4 the
/// mounted `BrowserComponent` owns the surface, so the painted selection lives
/// in its own layout (`test_layout`), keyed off its component-local cursor.
#[test]
fn narrow_tv_browse_j_moves_painted_selection() {
    let mut model = Model::new(tv_shows_app());
    model.sync_mounted_surfaces();
    let id = model
        .emby_browser_id
        .clone()
        .expect("narrow TV browser mounted");
    let mut term = narrow_backend();

    // Seed past the selected-Series inline hero (which swallows its own row)
    // so both samples are plain rows.
    model
        .application
        .get_component_mut(&id)
        .unwrap()
        .as_any_mut()
        .downcast_mut::<BrowserComponent>()
        .unwrap()
        .set_cursor_for_test(1);
    draw(&mut model, &mut term);
    let before = model
        .application
        .get_component(&id)
        .unwrap()
        .as_any()
        .downcast_ref::<BrowserComponent>()
        .unwrap()
        .test_layout()
        .selected_item_rect;
    assert!(
        before.is_some(),
        "narrow TV browse must paint a selected row"
    );

    press(&mut model, Key::Char('j'));
    draw(&mut model, &mut term);
    let after = model
        .application
        .get_component(&id)
        .unwrap()
        .as_any()
        .downcast_ref::<BrowserComponent>()
        .unwrap()
        .test_layout()
        .selected_item_rect;

    assert_ne!(
        before, after,
        "j must move the painted selection down the narrow TV series list"
    );
}

/// Regression 4: narrow grouped Music `j` moves the painted selection.
#[test]
fn narrow_grouped_music_j_moves_painted_selection() {
    let mut app = crate::app::render::make_music_group_app();
    app.terminal_width = 60;
    app.terminal_height = 20;
    app.mini_view_focus = PanelFocus::Library;
    let album_level = app.libs[0].nav_stack.last_mut().unwrap();
    for i in 0..3 {
        let mut album = make_item(&format!("Extra Album {i}"), "MusicAlbum");
        album.id = format!("album-extra-{i}");
        album.artist = "Alpha".into();
        album_level.items.push(album);
    }
    album_level.total_count = album_level.items.len();

    let mut model = Model::new(app);
    model.sync_mounted_surfaces();
    let mut term = narrow_backend();

    draw(&mut model, &mut term);
    let before = model.app.layout.main.selected_item_rect;
    assert!(
        before.is_some(),
        "narrow grouped Music must paint a selected album row"
    );

    press(&mut model, Key::Char('j'));
    draw(&mut model, &mut term);
    let after = model.app.layout.main.selected_item_rect;

    assert_ne!(
        before, after,
        "j must move the painted selection down the narrow grouped-album list"
    );
}

/// Regression (task 3.4 template step d): narrow Emby TV paints each visible
/// series/season row exactly once — the mounted `BrowserComponent` is the sole
/// painter now that the legacy `render_list` narrow branch early-returns for
/// `tvshows` too.
#[test]
fn narrow_tv_paints_each_browse_row_once() {
    let mut model = Model::new(tv_shows_app());
    model.sync_mounted_surfaces();
    let mut term = narrow_backend();

    let output = draw(&mut model, &mut term);

    for row in ["Series 0", "Series 1", "Series 2", "Series 3", "Series 4"] {
        assert_eq!(
            output.matches(row).count(),
            1,
            "narrow TV browse row {row:?} must be painted exactly once:\n{output}"
        );
    }
}

fn podcast_app() -> App {
    let mut app = make_app_stub();
    app.terminal_width = 60;
    app.terminal_height = 20;
    app.mini_view_focus = PanelFocus::Library;
    app.tab = TabSelection::EmbyLibrary(0);

    let mut library = make_item("Podcasts", "CollectionFolder");
    library.id = "lib-podcasts".into();
    library.collection_type = "podcasts".into();
    library.is_folder = true;

    app.libs.push(LibraryTab {
        nav_stack: vec![BrowseLevel {
            parent_id: "lib-podcasts".into(),
            title: "Podcasts".into(),
            items: folder_items("Show", "Series", 5),
            total_count: 5,
            resting: crate::app::types_browse::BrowseResting::new(0, 0),
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

/// Regression (task 3.5a template step d): narrow Emby podcast paints each
/// visible show row exactly once — the mounted `BrowserComponent` is the sole
/// painter now that the legacy `render_list` narrow branch early-returns for
/// podcast libraries too.
#[test]
fn narrow_podcast_paints_each_browse_row_once() {
    let mut model = Model::new(podcast_app());
    model.sync_mounted_surfaces();
    let mut term = narrow_backend();

    let output = draw(&mut model, &mut term);

    for row in ["Show 0", "Show 1", "Show 2", "Show 3", "Show 4"] {
        assert_eq!(
            output.matches(row).count(),
            1,
            "narrow podcast browse row {row:?} must be painted exactly once:\n{output}"
        );
    }
}

fn wide_podcast_app() -> App {
    let mut app = podcast_app();
    app.terminal_width = 140;
    app.terminal_height = 40;
    app
}

fn wide_backend() -> Terminal<TestBackend> {
    Terminal::new(TestBackend::new(140, 40)).unwrap()
}

fn feed_home_video_group_app() -> App {
    let mut app = make_app_stub();
    app.terminal_width = 60;
    app.terminal_height = 20;
    app.mini_view_focus = PanelFocus::Library;
    app.tab = TabSelection::EmbyLibrary(0);
    app.config.lock().unwrap().feed_view_libraries = vec!["youtube".into()];
    let mut library = make_item("YouTube", "CollectionFolder");
    library.id = "lib-youtube".into();
    library.collection_type = "homevideos".into();
    library.is_folder = true;
    let mut folder = make_item("Channel A", "Folder");
    folder.id = "folder-a".into();
    folder.is_folder = true;
    let mut first = make_item("Video One", "Movie");
    first.id = "video-one".into();
    first.runtime_ticks = 3_600 * 10_000_000;
    first.genre = "Family".into();
    first.overview = "Distinctive wrapping overview fragment for inline expansion.".into();
    let mut second = make_item("Video Two", "Movie");
    second.id = "video-two".into();
    app.libs.push(LibraryTab {
        nav_stack: vec![BrowseLevel {
            parent_id: "lib-youtube".into(),
            title: "YouTube".into(),
            items: vec![folder.clone()],
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
        }],
        feed_home_video: Some(FeedHomeVideoState {
            all_items: vec![first.clone(), second.clone()],
            groups: vec![FeedHomeVideoGroup {
                folder,
                items: vec![first, second],
            }],
            loading: false,
            ..FeedHomeVideoState::default()
        }),
        ..LibraryTab::new(library)
    });
    app
}

fn feed_snapshot(width: u16, height: u16) -> String {
    let mut app = feed_home_video_group_app();
    app.terminal_width = width;
    app.terminal_height = height;
    let mut model = Model::new(app);
    model.sync_mounted_surfaces();
    let mut term = Terminal::new(TestBackend::new(width, height)).unwrap();
    draw(&mut model, &mut term)
}

fn selected_feed_row_region(output: &str, title: &str) -> String {
    let lines: Vec<_> = output.lines().collect();
    let row = lines
        .iter()
        .position(|line| line.contains('▎') && line.contains(title))
        .unwrap_or_else(|| panic!("selected feed row must be rendered: {output}"));
    lines[row..row + 1].join("\n")
}


#[test]
fn feed_home_video_group_narrow_uses_shared_inline_hero() {
    // The picker routes through `render_narrow_browse_with_ctx` now: a
    // feed-group pill row, then the shared inline-hero replacement for the
    // selected row (framed, meta line inside) - identical to a generic narrow
    // home-video library.
    let output = feed_snapshot(60, 20);
    let lines: Vec<&str> = output.lines().collect();
    assert!(
        output.contains("All") && output.contains("Channel A"),
        "feed-group pills missing:\n{output}"
    );
    // Framed inline hero: a `▁` top rule above and a `▔` bottom rule below,
    // with the selected item's meta line between them.
    let top = lines
        .iter()
        .position(|l| l.trim_start().starts_with('\u{2581}'))
        .expect("inline-hero top rule missing");
    let bottom = lines
        .iter()
        .rposition(|l| l.trim_start().starts_with('\u{2594}'))
        .expect("inline-hero bottom rule missing");
    assert!(top < bottom);
    let framed = lines[top..=bottom].join("\n");
    assert!(framed.contains("Video One") && framed.contains("Family") && framed.contains("1h"));
    assert_eq!(
        output
            .lines()
            .filter(|line| line.contains("Video Two")
                && !line.contains('\u{2581}')
                && !line.contains('\u{2594}'))
            .count(),
        1,
        "Video Two paints once, outside the frame:\n{output}"
    );
}

#[test]
fn feed_home_video_group_wide_uses_wide_hero() {
    // Wide: Wide hero. Selected item's detail (overview + meta) is the
    // left hero card; the right rail is a plain one-column list with the
    // feed-group pills - no inline expansion in the rail.
    let output = feed_snapshot(140, 40);
    assert!(
        output.contains("All") && output.contains("Channel A"),
        "feed-group pills missing:\n{output}"
    );
    assert!(
        output.contains("Distinctive wrapping"),
        "left hero overview missing:\n{output}"
    );
    // Video Two is only ever a rail row (never the selected hero), so it
    // pins single-paint of the rail without the hero-echo of Video One.
    assert_eq!(
        output.lines().filter(|line| line.contains("Video Two")).count(),
        1,
        "Video Two paints once in the rail:\n{output}"
    );
}

#[test]
fn feed_home_video_group_paints_each_row_once() {
    for (width, height) in [(60, 20), (140, 40)] {
        let output = feed_snapshot(width, height);
        let rows = output.lines().filter(|line| {
            line.contains("Video Two") && !line.contains('\u{2581}') && !line.contains('\u{2594}')
        });
        assert_eq!(rows.count(), 1, "feed {width}x{height} paints the row once");
    }
}

#[test]
fn feed_home_video_group_metadata_bearing_hero_keeps_complete_frame() {
    let mut app = feed_home_video_group_app();
    app.terminal_height = 30;
    let overview = "First overview line with enough detail to wrap across the narrow hero.\nSecond overview line remains visible.\nFINAL OVERVIEW LINE";
    let state = app.libs[0].feed_home_video.as_mut().unwrap();
    state.groups[0].items[0].overview = overview.into();
    state.all_items[0].overview = overview.into();
    let mut model = Model::new(app);
    model.sync_mounted_surfaces();
    let mut term = Terminal::new(TestBackend::new(60, 30)).unwrap();
    let output = draw(&mut model, &mut term);
    let lines: Vec<_> = output.lines().collect();
    let top = lines.iter().position(|line| line.trim_start().starts_with('▁')).unwrap();
    let bottom = lines.iter().rposition(|line| line.trim_start().starts_with('▔')).unwrap();
    let final_line = lines.iter().position(|line| line.contains("FINAL OVERVIEW LINE")).unwrap();
    assert!(top < final_line && final_line < bottom, "hero frame clips overview:\n{output}");
}

#[test]
fn feed_home_video_group_browser_scroll_updates_video_scroll() {
    let mut app = feed_home_video_group_app();
    let state = app.libs[0].feed_home_video.as_mut().unwrap();
    for i in 0..30 {
        let mut item = make_item(&format!("Video extra {i}"), "Movie");
        item.id = format!("video-extra-{i}");
        state.groups[0].items.push(item.clone());
        state.all_items.push(item);
    }
    state.groups[0].folder.is_folder = true;
    let mut model = Model::new(app);
    model.sync_mounted_surfaces();
    let id = model.emby_browser_id.clone().expect("feed browser mounted");
    // Use the wide fixed-row control so its one-row-per-item geometry gives a
    // durable maximum without reading the removed narrow `left_item_rows`.
    let mut term = Terminal::new(TestBackend::new(140, 40)).unwrap();
    draw(&mut model, &mut term);
    let (area, total_rows) = {
        let component = model.application.get_component(&id).unwrap();
        let browser = component
            .as_any()
            .downcast_ref::<BrowserComponent>()
            .unwrap();
        let layout = browser.test_layout();
        let total_rows = model.app.libs[0]
            .feed_home_video
            .as_ref()
            .unwrap()
            .selected_len();
        (layout.left_area, total_rows)
    };
    let max_offset = total_rows.saturating_sub(area.height as usize);
    assert!(max_offset > 0, "feed fixture must overflow the mounted control");
    let mut music_resize = false;
    let mut tv_resize = false;
    // Drive the existing typed wheel path through the mounted control: the
    // final event has a raw result of max_offset + 1, but the control emits
    // its clamped offset to the persisted feed-home-video state.
    for _ in 0..=max_offset {
        model
            .application
            .get_component_mut(&id)
            .unwrap()
            .as_any_mut()
            .downcast_mut::<BrowserComponent>()
            .unwrap()
            .reset_mouse_gestures_for_test();
        let msg = model
            .application
            .get_component_mut(&id)
            .unwrap()
            .on(&Event::Mouse(MouseEvent {
                kind: MouseEventKind::ScrollDown,
                column: area.x + 1,
                row: area.y + 1,
                modifiers: KeyModifiers::NONE,
            }))
            .expect("scroll emits typed request");
        model.handle_terminal_message(msg, &mut music_resize, &mut tv_resize);
    }
    assert_ne!(max_offset + 1, max_offset);
    let control_scroll = model
        .application
        .get_component(&id)
        .and_then(|component| component.as_any().downcast_ref::<BrowserComponent>())
        .expect("feed browser remains mounted")
        .scroll();
    assert_eq!(control_scroll, max_offset);
    assert_eq!(
        model.app.libs[0].feed_home_video.as_ref().unwrap().video_scroll,
        max_offset
    );
}

#[test]
fn feed_home_video_group_metadata_free_selected_row_stays_ordinary() {
    let mut app = feed_home_video_group_app();
    let state = app.libs[0].feed_home_video.as_mut().unwrap();
    state.groups[0].items[0].overview.clear();
    state.groups[0].items[0].genre.clear();
    state.groups[0].items[0].runtime_ticks = 0;
    state.all_items[0].overview.clear();
    state.all_items[0].genre.clear();
    state.all_items[0].runtime_ticks = 0;
    let mut model = Model::new(app);
    model.sync_mounted_surfaces();
    let mut term = Terminal::new(TestBackend::new(60, 20)).unwrap();
    let output = draw(&mut model, &mut term);
    let region = selected_feed_row_region(&output, "Video One");
    assert_eq!(region.matches("Video One").count(), 1);
    assert!(region.contains('▎'));
    assert!(!region.contains('▁') && !region.contains('▔'));
}

/// Characterization (task 3.5b template step a): pins the painted WIDE Emby
/// podcast browse surface through the full `Model::draw_frame` path, at a
/// wide+tall size where `wide_hero_presentation` returns `Some`. The
/// pre-change baseline (committed with this test) was BLANK: `render_list`'s
/// hero-presentation early return fired for podcast libraries and returned
/// before anything published `layout.left_area`, and no podcast wide-workspace
/// component exists. This is the post-change buffer: with the podcast disjunct
/// removed from that early return, wide podcast falls through to the
/// `component_owned` block and the mounted `BrowserComponent` composes the
/// generic browse body across the wide area (blank -> browse body, an expected
/// bug-fix diff).
#[test]
fn wide_podcast_surface_snapshot() {
    let mut model = Model::new(wide_podcast_app());
    model.sync_mounted_surfaces();
    let mut term = wide_backend();

    let output = draw(&mut model, &mut term);

    assert_eq!(output, WIDE_PODCAST_SURFACE);
}

const WIDE_PODCAST_SURFACE: &str = "                                                                                                                                            \n                                           HOME  ▐ PODCASTS                                                                                 \n                                                                                                                                            \n                                                                                                                                            \n                                                                                                                                            \n                                                                                                                                            \n                                                                                                                                            \n                                        ▎  Show 0                                           Show 1                                          \n                                           Show 2                                           Show 3                                          \n                                           Show 4                                                                                           \n                                                                                                                                            \n                                                                                                                                            \n                                                                                                                                            \n                                                                                                                                            \n                                                                                                                                            \n                                                                                                                                            \n                                                                                                                                            \n                                                                                                                                            \n                                                                                                                                            \n                                                                                                                                            \n                                                                                                                                            \n                                                                                                                                            \n                                                                                                                                            \n                                                                                                                                            \n                                                                                                                                            \n                                                                                                                                            \n                                                                                                                                            \n     🖧  WOIMS                                                                                                                               \n                                                                                                                                            \n    Add items with p from Home or libr                                                                                                      \n                                                                                                                                            \n                                                                                                                                            \n                                                                                                                                            \n                                                                                                                                            \n                                                                                                                                            \n                                                                                                                                            \n                                                                                                                                            \n     🖭  none                                                                                                                                \n                                                                                                                                            \n                                         🔊  100                                                                                     󰚴  ♥  ";

/// Regression (task 3.5b template step d): the WIDE Emby podcast browse surface
/// paints the generic browse body (mounted `BrowserComponent`, kind `Generic`)
/// across the wide area — it is no longer blank. The shared narrow composer
/// runs wide here (no podcast wide-specific layout, per the task): it reserves
/// a placeholder hero block and lays the show rows out in a multi-column grid,
/// so the earliest rows sit under the hero reservation and `Show 2`..`Show 4`
/// are the visible browse body. Matching the wide generic-collection case
/// (task 3.3 scope note), this shared-composer wide behavior is task 3.8
/// territory, not a 3.5b regression.
#[test]
fn wide_podcast_paints_browse_body() {
    let mut model = Model::new(wide_podcast_app());
    model.sync_mounted_surfaces();
    let mut term = wide_backend();

    let output = draw(&mut model, &mut term);

    for row in ["Show 2", "Show 3", "Show 4"] {
        assert_eq!(
            output.matches(row).count(),
            1,
            "wide podcast browse row {row:?} must be painted exactly once:\n{output}"
        );
    }
}

/// Regression 5: narrow Movies paints each browse row exactly once (currently
/// double-painted by legacy `render_list` + `BrowserComponent::view`).
#[test]
fn narrow_movies_paints_each_browse_row_once() {
    let mut app = crate::app::render::make_movie_app();
    app.terminal_width = 60;
    app.terminal_height = 20;
    app.mini_view_focus = PanelFocus::Library;

    let mut model = Model::new(app);
    model.sync_mounted_surfaces();
    let mut term = narrow_backend();

    let output = draw(&mut model, &mut term);

    assert_eq!(
        output.matches("Second Movie").count(),
        1,
        "narrow Movies browse row must be painted exactly once, not double-painted:\n{output}"
    );
}
