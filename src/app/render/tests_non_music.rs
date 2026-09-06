use super::test_helpers::*;
use super::*;
use crate::app::tests::{make_app_stub, make_item};
use crate::app::{BrowseLevel, LibraryTab, TabSelection};
use ratatui::backend::TestBackend;
use ratatui::Terminal;
use tuirealm::component::Component;

#[test]
fn home_video_library_is_never_album_folders_and_renders_via_original_list_path() {
    let mut model = mounted_model_at(make_home_video_app(), 60, 20);
    let lib_idx = 0;

    assert!(
        !model.app.is_viewing_album_folders(lib_idx),
        "a homevideos library must never satisfy is_viewing_album_folders"
    );
    assert!(model.app.is_home_video_view(lib_idx));

    let out = draw_mounted_frame(&mut model, 60, 20);

    assert!(
        out.contains("Birthday Clip"),
        "expected the mounted BrowserComponent to paint the home-video list:\n{out}"
    );
    assert!(
        model.app.album_tracks_cache.is_empty(),
        "home-video rendering must never touch the album-tracks cache added by #145"
    );
}

#[test]
fn narrow_home_video_selected_item_retains_inline_detail() {
    let mut app = make_home_video_app();
    app.libs[0].nav_stack[0].items[1].overview = "The selected home video overview.".into();
    app.libs[0].nav_stack[0].set_resting_cursor(1);
    let mut model = mounted_model_at(app, 70, 30);
    let output = draw_mounted_frame(&mut model, 70, 30);
    let layout = mounted_browser_layout(&model);

    assert!(
        layout.hero_area.height > 0,
        "selected Home Video detail disappeared"
    );
    assert!(
        output.contains("Vacation Clip"),
        "selected Home Video title is missing:\n{output}"
    );
}

#[test]
fn wide_home_video_uses_a_left_detail_and_right_rail() {
    // Wide Movies / home-video geometry is published by the mounted
    // `BrowserComponent` now (task 3.8): the legacy base frame only reserves
    // `left_area`. Read the right rail off the component's own painted layout.
    let mut model = mounted_model_at(make_home_video_app(), 200, 40);
    let _ = draw_mounted_frame(&mut model, 200, 40);
    let layout = mounted_browser_layout(&model);

    assert!(layout.movies_wide_right_area.width > 0);
    assert!(layout.movies_wide_right_area.height > 0);
}

/// `remove-migrated-surface-underpaint` 3.2 (D4): at the wide Wide hero
/// breakpoint the mounted `BrowserComponent` owns the Movies / home-video
/// picture. Post task 3.8 the legacy `render_library` `EmbyLibrary` arm only
/// reserves the destination `left_area` and paints no row, banner, or hero —
/// the `movies_wide_*` split geometry hand-off is now published by the
/// component itself. Mirrors the Home precedent
/// `legacy_base_frame_does_not_paint_home_content_before_the_component`.
#[test]
fn wide_movies_legacy_base_frame_publishes_geometry_but_paints_no_rows() {
    for (mut app, marker) in [
        (make_movie_app(), "Focused Movie"),
        (make_home_video_app(), "Birthday Clip"),
    ] {
        let mut layout = LayoutMain::default();
        let mut term = ratatui::Terminal::new(ratatui::backend::TestBackend::new(120, 40)).unwrap();
        term.draw(|f| {
            app.render_library(
                f,
                ratatui::layout::Rect::new(0, 0, 120, 40),
                &mut layout,
                None,
            );
        })
        .unwrap();

        assert!(
            layout.left_area.width > 0 && layout.left_area.height > 0,
            "wide movies destination area hand-off must still be reserved: {:?}",
            layout.left_area
        );
        let output = buffer_to_string(&term);
        assert!(
            !output.contains(marker),
            "legacy base frame must not paint browser rows at the wide breakpoint: {output:?}"
        );
    }
}

/// `remove-migrated-surface-underpaint` 3.8 (D4): while the mounted
/// must not underpaint the ordinary browse list. The shell projects
/// returns after publishing `left_area` (`src/app/render/components/list.rs`,
/// just before the `n == 0` branch) without painting a row or hero. Mirrors
/// `wide_movies_legacy_base_frame_publishes_geometry_but_paints_no_rows`.
#[test]
fn wide_emby_podcast_does_not_publish_tv_geometry() {
    let mut app = make_movie_app();
    app.libs[0].library.collection_type = "podcasts".into();
    for item in &mut app.libs[0].nav_stack[0].items {
        item.item_type = "Series".into();
        item.is_folder = true;
    }

    let layout = render_view(&mut app, 200, 40);

    assert_eq!(layout.tv_wide_left_area, ratatui::layout::Rect::default());
    assert_eq!(layout.tv_wide_right_area, ratatui::layout::Rect::default());
}

#[test]
fn podcast_and_home_video_use_inline_when_wide_height_is_unavailable() {
    let mut podcast = make_movie_app();
    podcast.libs[0].library.collection_type = "podcasts".into();
    let podcast_layout = render_view(&mut podcast, 200, 8);
    assert_eq!(podcast_layout.tv_wide_left_area.width, 0);

    let mut home_video = make_home_video_app();
    let home_video_layout = render_view(&mut home_video, 200, 8);
    assert_eq!(home_video_layout.movies_wide_right_area.width, 0);
}

#[test]
fn letter_filter_buckets_match_emby_name_range_bounds() {
    let ac = LetterFilter::for_index(0).unwrap();
    assert_eq!(ac.label, "A\u{2013}C");
    assert_eq!(ac.name_ge, Some("A"));
    assert_eq!(ac.name_lt, Some("D"));

    let vz = LetterFilter::for_index(7).unwrap();
    assert_eq!(vz.label, "V\u{2013}Z");
    assert_eq!(vz.name_ge, Some("V"));
    assert_eq!(vz.name_lt, None, "V–Z has no upper bound");

    let hash = LetterFilter::for_index(8).unwrap();
    assert_eq!(hash.label, "#");
    assert_eq!(hash.name_ge, None, "# has no lower bound");
    assert_eq!(hash.name_lt, Some("A"));

    assert!(LetterFilter::for_index(9).is_none());
    assert_eq!(LetterFilter::count(), 9);
    assert_eq!(LetterFilter::labels().len(), 9);
}

#[test]
fn letter_filter_default_is_the_first_bucket() {
    assert_eq!(
        LetterFilter::default_filter(),
        LetterFilter::for_index(0).unwrap()
    );
}

fn letter_grouped_series_app() -> App {
    let mut app = make_app_stub();
    app.tab = TabSelection::EmbyLibrary(0);

    let mut library = make_item("Shows", "CollectionFolder");
    library.id = "lib-shows".into();
    library.is_folder = true;
    library.collection_type = "tvshows".into();

    let series: Vec<_> = (0..55)
        .map(|i| {
            let letter = (b'A' + (i % 26) as u8) as char;
            let name = format!("{letter}alpha Series {i:02}");
            let mut s = make_item(&name, "Series");
            s.id = format!("series-{i}");
            s
        })
        .collect();

    app.libs.push(LibraryTab {
        nav_stack: vec![BrowseLevel {
            parent_id: "lib-shows".into(),
            title: "Shows".into(),
            items: series,
            total_count: 55,
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
        library_total: Some(55),
        ..LibraryTab::new(library)
    });
    app
}

#[test]
fn tv_series_list_computes_sorted_indices_when_above_threshold() {
    // Narrow letter-grouped TV is painted by the mounted `BrowserComponent`
    // (task 3.8). Its control exports row geometry for the compatibility hit
    // map, not a second sorted-index projection. Use the Wide TV control's
    // published sorted order below as the durable ordering evidence.
    let mut app = letter_grouped_series_app();
    let mut layout = LayoutMain::default();
    let mut terminal = ratatui::Terminal::new(ratatui::backend::TestBackend::new(120, 20)).unwrap();
    // Wide TV is component-owned (task 5.3d.18d): the App frame only
    // publishes the `tv_wide_*` hand-off geometry, and the mounted
    // `TvWorkspaceComponent` paints the surface pills over it, exactly as
    // the live shell does.
    let mut component = crate::app::components::TvWorkspaceComponent::new();
    component.set_content(app.wide_tv_render_ctx(0, None));
    let wide_area = ratatui::layout::Rect::new(0, 0, 120, 20);
    terminal
        .draw(|f| {
            app.render_library(f, wide_area, &mut layout, None);
            component.view(f, wide_area);
        })
        .unwrap();
    let component_layout = component.test_layout();
    let first_idx = component_layout
        .left_sorted_indices
        .first()
        .copied()
        .expect("Wide TV control publishes sorted order for grouped series");
    assert!(
        app.libs[0].nav_stack[0].items[first_idx]
            .name
            .starts_with('A'),
        "first Wide TV sorted item should start with A, got: {}",
        app.libs[0].nav_stack[0].items[first_idx].name,
    );
    assert_surface_pills(
        &terminal,
        component_layout,
        ratatui::layout::Rect {
            y: component_layout.selector_tabs[0].0.y,
            height: component_layout
                .tv_wide_right_area
                .bottom()
                .saturating_sub(component_layout.selector_tabs[0].0.y),
            ..component_layout.tv_wide_right_area
        },
        1,
        ratatui::style::Color::Reset,
        &(0..9).collect::<Vec<_>>(),
        &["⌘", "A–C", "D–F", "G–I", "J–L", "M–O", "P–R", "S–U", "V–Z"],
        0,
    );
}

/// Characterization test for the narrow (single-column) Series inline hero
/// (task 2.1/2.2): renders hero content only (title/meta/overview/image) --
/// no "Series:" season pill/count row, no episode table. The wide
/// (Wide hero) presentation is a non-goal here; see `tv_wide_tests.rs`
/// for its unchanged coverage.
#[test]
fn narrow_series_inline_hero_shows_only_hero_content_no_season_or_episode_list() {
    let mut app = make_app_stub();
    app.tab = TabSelection::EmbyLibrary(0);

    let mut library = make_item("Shows", "CollectionFolder");
    library.id = "library".into();
    library.collection_type = "tvshows".into();
    library.is_folder = true;

    let mut series = make_item("The Series", "Series");
    series.id = "series".into();
    series.overview = "An overview of the series.".into();

    let mut season = make_item("Season 1", "Season");
    season.id = "season-1".into();
    season.index_number = 1;
    let mut episode = make_item("Pilot", "Episode");
    episode.id = "episode".into();
    episode.index_number = 1;
    episode.runtime_ticks = 3600 * mbv_core::api::TICKS_PER_SECOND;

    app.libs.push(LibraryTab {
        nav_stack: vec![BrowseLevel {
            parent_id: "library".into(),
            title: "Shows".into(),
            items: vec![series],
            total_count: 1,
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
        library_total: Some(1),
        ..LibraryTab::new(library)
    });
    let mut episodes = std::collections::HashMap::new();
    episodes.insert("season-1".into(), vec![episode]);
    app.series_detail_cache.insert(
        "series".into(),
        crate::app::SeriesDetail {
            seasons: vec![season],
            episodes,
        },
    );

    // Below `TWO_COLUMN_THRESHOLD` so the narrow single-column presentation
    // renders instead of `render_wide_tv`. Painted by the mounted
    // `BrowserComponent` (task 3.8).
    let mut model = mounted_model_at(app, 70, 30);
    let output = draw_mounted_frame(&mut model, 70, 30);

    assert!(output.contains("The Series"), "{output}");
    assert!(output.contains("An overview"), "{output}");
    assert!(
        !output.contains("Series:"),
        "narrow inline hero must not show the season pill/count row:\n{output}"
    );
    assert!(
        !output.contains("Pilot"),
        "narrow inline hero must not show the episode table:\n{output}"
    );
}

/// migrate-home-feeds 5.1 (§5 geometry test): the shared Wide hero
/// primitive owns the one-row status-bar reserve, so wide Music's framed list
/// panel must paint its `▁` bottom border two rows above `wide_music_area`'s
/// bottom, leaving exactly one blank row between the panel and the status bar.
/// Asserted against the painted buffer — a re-derived layout rect cannot catch
/// a one-row vertical shift.
#[test]
fn wide_music_list_panel_leaves_exactly_one_row_above_the_status_bar() {
    let mut model = mounted_model_at(make_music_group_app(), 200, 40);
    let terminal = draw_mounted_terminal(&mut model, 200, 40);
    let layout = mounted_music_layout(&model);
    let right = layout.wide_music_right_area;
    assert!(right.height > 0, "wide music right pane must paint");
    assert_list_pane_reserves_one_row_above_status(
        terminal.backend().buffer(),
        right,
        layout.wide_music_area.bottom(),
    );
}

/// migrate-home-feeds 5.1 (§5 geometry test): same one-blank-row reserve for
/// the ABS Book tab. Book paints no framed list border at the pane bottom, so
/// this checks the painted buffer directly: the last row before the status bar
/// (`area.bottom() - 1`) is blank across the right pane, and the surname-bucket
/// pill row the component publishes (`geometry.selector_tabs`) is actually
/// painted at that row in the buffer. A one-row downward shift of the pane
/// would paint the reserve row and move the pills off their published row.
#[test]
fn wide_book_panes_leave_exactly_one_row_above_the_status_bar() {
    use crate::app::components::AudiobookshelfBookComponent;
    let area = Rect::new(0, 0, 120, 30);
    let app = make_audiobookshelf_book_app();
    let mut component = AudiobookshelfBookComponent::new();
    if let Some(state) = app.audiobookshelf_book_browse.first() {
        component.set_content(state, app.images_enabled());
        component.set_focused(true);
    }
    let mut terminal = Terminal::new(TestBackend::new(area.width, area.height)).unwrap();
    terminal.draw(|frame| component.view(frame, area)).unwrap();
    let geometry = component.geometry();
    let pill_rect = geometry
        .selector_tabs
        .first()
        .map(|(rect, _)| *rect)
        .expect("book pills painted");
    let buffer = terminal.backend().buffer();

    // The published pill row is really painted there (non-blank glyphs).
    let pill_row: String = (pill_rect.x..pill_rect.right())
        .map(|x| buffer[(x, pill_rect.y)].symbol())
        .collect();
    assert!(
        !pill_row.trim().is_empty(),
        "book pill row must be painted at its published row {}: {pill_row:?}",
        pill_rect.y
    );

    // Exactly one blank row between the pane and the status bar: everything on
    // `area.bottom() - 1` across the right pane is unpainted.
    let reserve_y = area.bottom() - 1;
    for x in pill_rect.x..area.right() {
        assert_eq!(
            buffer[(x, reserve_y)].symbol(),
            " ",
            "book reserve row {reserve_y} must be blank at x={x}"
        );
    }
}
