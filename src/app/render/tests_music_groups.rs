use super::components::album_detail::album_hero_detail_rows;
use super::components::hero::HERO_BLOCK_EXTRA_ROWS;
use super::test_helpers::*;
use super::*;
use crate::app::shell::Model;
use crate::app::tests::make_item;
use ratatui::backend::TestBackend;
use ratatui::layout::Rect;
use ratatui::Terminal;

/// Narrow grouped Music is painted by the mounted `MusicWorkspaceComponent`
/// now (task 3.8): drive the real `Model::draw_frame` shell path and read the
/// component's own published `LayoutMain` via `mounted_music_layout`.
fn narrow_music_frame(app: App, height: u16) -> (Model, String) {
    let mut model = mounted_model_at(app, 60, height);
    let output = draw_mounted_frame(&mut model, 60, height);
    (model, output)
}

#[test]
fn selectable_artist_headers_are_typed_row_targets() {
    let mut app = make_music_group_app();
    let mut alpha_album2 = make_item("Second Alpha Album", "MusicAlbum");
    alpha_album2.id = "album-1b".into();
    alpha_album2.artist = "Alpha".into();
    alpha_album2.is_folder = true;
    app.libs[0]
        .nav_stack
        .last_mut()
        .unwrap()
        .items
        .push(alpha_album2);
    let mut beta_album = make_item("Beta Album", "MusicAlbum");
    beta_album.id = "album-2".into();
    beta_album.artist = "Beta".into();
    beta_album.is_folder = true;
    app.libs[0]
        .nav_stack
        .last_mut()
        .unwrap()
        .items
        .push(beta_album);

    let (model, out) = narrow_music_frame(app, 20);
    let layout = mounted_music_layout(&model);

    assert!(
        out.contains("Alpha") && out.contains("Beta"),
        "expected both artist headers to render:\n{out}"
    );
    // Artist headers are display-only and must not appear as row targets.
    // Music-group view renders through `render_wide_right_album_browser`
    // (shared with the wide Wide hero layout), which populates
    // `left_row_targets` directly rather than the legacy `left_row_map`.
    assert!(
        layout.left_row_targets.iter().any(|t| t.is_none()),
        "expected a non-album row (artist header) in the row targets"
    );
}

#[test]
fn narrow_grouped_music_replaces_selected_album_row_with_hero_detail() {
    // Task 3.2: the selected album's row is replaced by the Model A hero
    // (title/meta/art), not an inline track table -- see
    // `tests_music_characterization.rs` for the text-level assertion that
    // the track table and action hint no longer render.
    let mut app = make_music_group_app();
    let tracks: Vec<mbv_core::api::EmbyItem> = (0..2)
        .map(|i| {
            let mut track = make_item(&format!("Track {}", i + 1), "Audio");
            track.id = format!("track-{}", i + 1);
            track.index_number = (i + 1) as i64;
            track
        })
        .collect();
    app.album_tracks_cache.insert("album-1".into(), tracks);
    let (model, output) = narrow_music_frame(app, 30);
    let layout = mounted_music_layout(&model);

    assert!(
        output.contains("First Album"),
        "selected album hero must render its title"
    );
    assert_eq!(
        layout
            .left_row_targets
            .iter()
            .filter(|target| { matches!(target, Some(0)) })
            .count(),
        1,
        "the selected album must publish one replacement parent target"
    );
    let hero_marker = layout
        .hero_area
        .y
        .checked_sub(0)
        .and_then(|y| output.lines().nth(y as usize))
        .and_then(|line| {
            line.chars()
                .nth(layout.left_area.x.saturating_sub(2) as usize)
        });
    assert_ne!(
        hero_marker,
        Some('\u{258e}'),
        "the shared replacement plan suppresses the ordinary marker over its hero"
    );
}

#[test]
fn narrow_grouped_music_does_not_repaint_album_hero_with_zero_row_shell() {
    let app = make_music_group_app();
    let (model, output) = narrow_music_frame(app, 30);
    let layout = mounted_music_layout(&model);

    let top_row = output
        .lines()
        .nth(layout.hero_area.y as usize)
        .unwrap_or_default();
    let bottom_row = output
        .lines()
        .nth(layout.hero_area.bottom().saturating_sub(1) as usize)
        .unwrap_or_default();
    assert!(
        top_row.contains('▁'),
        "album hero top border missing: {top_row:?}"
    );
    assert!(
        bottom_row.contains('▔'),
        "album hero bottom border missing: {bottom_row:?}"
    );
}

#[test]
fn narrow_grouped_music_keeps_bottom_hero_fully_visible() {
    let mut app = make_music_group_app();
    for i in 2..=12 {
        let mut album = make_item(&format!("Album {i:02}"), "MusicAlbum");
        album.id = format!("album-{i}");
        album.artist = "Alpha".into();
        app.libs[0].nav_stack.last_mut().unwrap().items.push(album);
    }
    app.image_protocol_enabled = true;
    let albums = app.libs[0].nav_stack.last().unwrap().items.clone();
    let cursor = albums.len() - 1;
    app.libs[0]
        .nav_stack
        .last_mut()
        .unwrap()
        .set_resting_cursor(cursor);
    let expected_height = album_hero_detail_rows(true) + HERO_BLOCK_EXTRA_ROWS as usize;
    let (model, output) = narrow_music_frame(app, 30);
    let layout = mounted_music_layout(&model);
    // The mounted component paints into `app.layout.main.left_area`; its own
    // `layout()` publishes hero/target geometry in the same screen space.
    let list_area = model.app.layout.main.left_area;

    assert_eq!(layout.hero_area.height as usize, expected_height);
    assert!(layout.hero_area.y > list_area.y);
    assert_eq!(layout.hero_area.bottom(), list_area.bottom());
    assert_eq!(layout.selected_item_rect, Some(layout.hero_area));
    let selected_row = layout
        .left_item_rows
        .iter()
        .position(|row| row == &vec![cursor])
        .expect("the selected source row becomes the parent hero row");
    assert_eq!(
        layout
            .left_row_targets
            .iter()
            .filter(|target| { matches!(target, Some(idx) if *idx == cursor) })
            .count(),
        1,
        "the admitted hero publishes exactly one selected parent target"
    );
    let continuation_end = selected_row + expected_height;
    assert!(layout.left_item_rows.len() >= continuation_end);
    assert!(layout.left_item_rows[selected_row + 1..continuation_end]
        .iter()
        .all(Vec::is_empty));
    // Album rows render below the reserved group pill row (task 3.6a), so
    // screen-space row targets are measured from the content area, not the
    // pane top.
    let content_top = super::arrangements::wide_hero::pill_bar_areas(list_area)
        .content_area
        .y;
    let selected_screen_row = layout.hero_area.y.saturating_sub(content_top) as usize;
    let target_end = selected_screen_row + expected_height;
    assert!(layout.left_row_targets.len() >= target_end);
    assert!(layout.left_row_targets[selected_screen_row + 1..target_end]
        .iter()
        .all(Option::is_none));

    let marker_col = list_area.x.saturating_sub(2) as usize;
    for y in layout.hero_area.y..layout.hero_area.bottom() {
        let marker = output
            .lines()
            .nth(y as usize)
            .and_then(|line| line.chars().nth(marker_col));
        assert_ne!(
            marker,
            Some('\u{258e}'),
            "ordinary marker painted over hero at y={y}"
        );
    }
}

#[test]
fn narrow_grouped_music_persists_bottom_hero_scroll() {
    let mut app = make_music_group_app();
    for i in 2..=12 {
        let mut album = make_item(&format!("Album {i:02}"), "MusicAlbum");
        album.id = format!("album-{i}");
        album.artist = "Alpha".into();
        app.libs[0].nav_stack.last_mut().unwrap().items.push(album);
    }
    app.image_protocol_enabled = true;
    let cursor = app.libs[0].nav_stack.last().unwrap().items.len() - 1;
    app.libs[0]
        .nav_stack
        .last_mut()
        .unwrap()
        .set_resting_cursor(cursor);
    let mut model = mounted_model_at(app, 60, 30);
    let _ = draw_mounted_frame(&mut model, 60, 30);

    let stored_scroll = mounted_music_scroll(&model);
    assert!(stored_scroll > 0, "the admitted hero offset must persist");
    {
        let list_area = model.app.layout.main.left_area;
        let layout = mounted_music_layout(&model);
        assert_eq!(layout.selected_item_rect, Some(layout.hero_area));
        assert!(layout.hero_area.bottom() <= list_area.bottom());
    }

    let _ = draw_mounted_frame(&mut model, 60, 30);
    assert_eq!(
        mounted_music_scroll(&model),
        stored_scroll,
        "the computed hero scroll remains persisted on the next render"
    );
}

#[test]
fn short_grouped_music_restores_the_ordinary_selected_album_row() {
    let mut app = make_music_group_app();
    app.image_protocol_enabled = true;
    let expected_height = album_hero_detail_rows(true) + HERO_BLOCK_EXTRA_ROWS as usize;
    // Terminal height == the hero's own row count: after chrome reservation the
    // list area is strictly shorter than the hero needs, so the ordinary
    // selected row must be restored.
    let (model, output) = narrow_music_frame(app, expected_height as u16);
    let layout = mounted_music_layout(&model);

    assert!(output.contains("First Album"));
    assert_eq!(layout.hero_area, Rect::default());
    let selected = layout
        .selected_item_rect
        .expect("the ordinary selected album row remains targetable");
    assert_ne!(selected, layout.hero_area);
    assert!(layout
        .left_row_targets
        .iter()
        .any(|target| matches!(target, Some(0))));
}

#[test]
fn grouped_hero_art_follows_album_focus() {
    let mut album_app = make_music_group_app();
    let mut second = make_item("Second Album", "MusicAlbum");
    second.id = "album-2".into();
    second.artist = "Alpha".into();
    album_app.libs[0]
        .nav_stack
        .last_mut()
        .unwrap()
        .items
        .push(second);
    album_app.libs[0]
        .nav_stack
        .last_mut()
        .unwrap()
        .set_resting_cursor(1);
    album_app.image_protocol_enabled = true;
    // 60x30 so the list below the album hero still shows both albums.
    let (model, out) = narrow_music_frame(album_app, 30);
    assert!(out.contains("First Album"));
    // The hero renders the *selected* album's art (portrait `:P`), never a
    // square collage tile (`:sq`).
    assert!(model.app.card_image_loading.contains("album-2:P"));
    // Narrow grouped Music pre-warms neighbouring album art through the shell;
    // the row painter itself still emits only the selected album's hero art.
    assert!(!model.app.card_image_loading.contains("album-2:sq"));
}

#[test]
fn grouped_music_maps_reordered_non_contiguous_album_source() {
    let mut app = make_music_group_app();
    let mut beta = make_item("Beta Album", "MusicAlbum");
    beta.id = "album-beta".into();
    beta.artist = "Beta".into();
    let mut alpha_other = make_item("Alpha Other", "MusicAlbum");
    alpha_other.id = "album-alpha-other".into();
    alpha_other.artist = "Alpha".into();
    let mut selected = make_item("Selected Album", "MusicAlbum");
    selected.id = "album-selected".into();
    selected.artist = "Alpha".into();
    app.libs[0]
        .nav_stack
        .last_mut()
        .unwrap()
        .items
        .extend([beta, alpha_other, selected]);
    let cursor = 3;
    app.libs[0]
        .nav_stack
        .last_mut()
        .unwrap()
        .set_resting_cursor(cursor);
    let (model, rendered) = narrow_music_frame(app, 30);
    let layout = mounted_music_layout(&model);

    assert!(rendered.contains("Selected Album"));
    assert_eq!(layout.selected_item_rect, Some(layout.hero_area));
    assert_eq!(
        layout
            .left_row_targets
            .iter()
            .filter(|target| { matches!(target, Some(3)) })
            .count(),
        1
    );
    assert_eq!(
        layout
            .left_item_rows
            .iter()
            .filter(|row| row.as_slice() == [3])
            .count(),
        1
    );
}

#[test]
fn wide_music_frame_publishes_identical_geometry_from_publish_and_paint() {
    // The paint path must consume the arrangement returned by
    // `publish_geometry` rather than recomputing it: the pure arrangement
    // math runs once per wide frame and both passes produce the same
    // geometry.
    let app = make_music_group_app();
    let app2 = make_music_group_app();

    let mut publish_layout = LayoutMain::default();
    let mut paint_layout = LayoutMain::default();

    let ctx = app.wide_music_render_ctx(0, None);
    let published = ctx
        .publish_geometry(Rect::new(0, 0, 120, 24), &mut publish_layout)
        .expect("wide area publishes panes");

    let mut terminal = Terminal::new(TestBackend::new(120, 24)).unwrap();
    terminal
        .draw(|f| {
            render_wide_music_group_with_ctx(
                f,
                Rect::new(0, 0, 120, 24),
                &app2.wide_music_render_ctx(0, None),
                &mut paint_layout,
                &mut crate::app::components::media_list::WideMediaList::new(),
                &mut crate::app::components::media_list::WideMediaList::new(),
                &mut crate::app::components::inline_search::InlineSearch::new(),
            );
        })
        .unwrap();

    let (published_panes, published_left) = published;
    assert_eq!(published_panes.left_area, paint_layout.left_area);
    assert_eq!(
        published_panes.right_area,
        paint_layout.wide_music_right_area
    );
    assert_eq!(published_left.hero_area, paint_layout.hero_area);
    assert_eq!(published_left.art_area, paint_layout.wide_music_art_area);
    assert_eq!(publish_layout.wide_music_area, paint_layout.wide_music_area);
    assert_eq!(publish_layout.left_area, paint_layout.left_area);
    assert_eq!(
        publish_layout.wide_music_right_area,
        paint_layout.wide_music_right_area
    );
    assert_eq!(
        publish_layout.wide_music_art_area,
        paint_layout.wide_music_art_area
    );
    assert_eq!(publish_layout.hero_area, paint_layout.hero_area);
}

#[test]
fn wide_music_stacked_layout_reserves_one_blank_row_between_art_and_text() {
    use crate::app::render::arrangements::music::wide_music_left_layout;

    // Narrow left pane forces `stack_metadata` when images are on.
    let left_area = Rect::new(0, 0, 40, 24);

    let stacked = wide_music_left_layout(left_area, true, 5);
    assert!(stacked.stack_metadata, "expected stacked metadata layout");
    assert_eq!(
        stacked.text_area.y,
        stacked.art_area.y + stacked.art_area.height + 1,
        "expected exactly one blank row between art and text when stacked"
    );

    // Same geometry with images off must not stack: no art is reserved at
    // all, so text occupies the full hero area with no gap applied.
    let no_images = wide_music_left_layout(left_area, false, 5);
    assert!(!no_images.stack_metadata);
    assert_eq!(
        no_images.text_area.y, no_images.hero_area.y,
        "expected text to flush against the hero area when images are off"
    );

    // Wide left pane forces side-by-side (not stacked) even with images on:
    // art and text sit next to each other, no vertical gap applies.
    let side_by_side_area = Rect::new(0, 0, 80, 24);
    let side_by_side = wide_music_left_layout(side_by_side_area, true, 5);
    assert!(!side_by_side.stack_metadata, "expected side-by-side layout");
    assert_eq!(
        side_by_side.text_area.y, side_by_side.hero_area.y,
        "expected text to flush against the hero area in side-by-side layout"
    );
}
