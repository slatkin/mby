use super::*;
// Characterization coverage stays beside the moved TV component.
use crate::app::components::TvWorkspaceComponent;
use crate::app::layout::LayoutMain;
use crate::app::render::test_helpers::buffer_to_string;
use crate::app::render::HomeImagePaint;
use crate::app::tests::{make_app_stub, make_item};
use crate::app::{BrowseLevel, LibraryTab, SeriesDetail, TabSelection};
use ratatui::backend::TestBackend;
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::widgets::Block;
use ratatui::Terminal;
use std::collections::HashMap;
use tuirealm::component::{AppComponent, Component};

/// Paints the wide TV workspace exactly as the live shell does: draw the
/// legacy `App` base frame (which now only publishes the `tv_wide_*`
/// hand-off geometry, task 5.3d.18d) then render the mounted
/// `TvWorkspaceComponent` over the same area so it owns the picture.
/// Returns the buffer and the component so tests can read both the App
/// pre-pass layout (`AppLayout`) and the component-owned geometry
/// (`tv_wide_episode_list_area`/`tv_wide_season_tabs`).
fn render_tv_workspace(app: &mut App, layout: &mut LayoutMain) -> (String, TvWorkspaceComponent) {
    let backend = TestBackend::new(100, 40);
    let mut term = Terminal::new(backend).unwrap();
    let area = Rect::new(0, 0, 100, 40);
    let mut component = TvWorkspaceComponent::new();
    component.set_content(
        app.wide_tv_render_ctx(0, None)
            .with_image_state(false, false),
    );
    component.set_focused(true);
    term.draw(|f| {
        app.render_library(f, area, layout, None);
        component.view(f, area);
    })
    .unwrap();
    let layout = component.test_layout();
    let buffer = term.backend().buffer();
    assert!(
        (layout.tv_wide_right_area.x..layout.tv_wide_right_area.right()).all(|x| {
            (layout.tv_wide_right_area.y..layout.tv_wide_right_area.bottom())
                .all(|y| buffer[(x, y)].bg != palette::SURFACE_ARTWORK_PLACEHOLDER)
        }),
        "images-off TV hero must not reserve artwork placeholder cells"
    );
    (buffer_to_string(&term), component)
}

fn tv_app() -> App {
    let mut app = make_app_stub();
    app.tab = TabSelection::EmbyLibrary(0);
    let mut library = make_item("Shows", "CollectionFolder");
    library.id = "library".into();
    library.collection_type = "tvshows".into();
    library.is_folder = true;

    let mut series = make_item("The Series", "Series");
    series.id = "series".into();
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
    let mut episodes = HashMap::new();
    episodes.insert("season-1".into(), vec![episode]);
    app.series_detail_cache.insert(
        "series".into(),
        SeriesDetail {
            seasons: vec![season],
            episodes,
        },
    );
    app
}

#[test]
fn is_right_panel_wide_reflects_terminal_size_paint_free() {
    let mut app = make_app_stub();
    app.terminal_width = 150;
    app.terminal_height = 24;
    assert!(app.is_right_panel_wide());

    app.terminal_width = 60;
    app.terminal_height = 24;
    assert!(!app.is_right_panel_wide());
}

#[test]
fn wide_tv_images_off_collapses_artwork_and_uses_full_text_width() {
    let mut app = tv_app();
    let (output, component) = render_tv_workspace(&mut app, &mut LayoutMain::default());
    let layout = component.test_layout();
    assert!(layout.tv_wide_left_area.width > 0);
    assert!(layout.tv_wide_right_area.width > layout.tv_wide_left_area.width / 2);
    assert!(output.contains("The Series"));
    assert!(
        output.contains("Pilot"),
        "TV text must remain visible: {output}"
    );
}

#[test]
fn wide_tv_series_placeholder_paints_the_full_portrait_budget() {
    let mut app = tv_app();
    let item = app.libs[0].nav_stack[0].items[0].clone();
    let mut terminal = Terminal::new(TestBackend::new(30, 20)).unwrap();
    terminal
        .draw(|f| {
            app.paint_home_image(
                f,
                Some(HomeImagePaint::Series {
                    area: Rect::new(2, 2, 18, 12),
                    item: Box::new(item),
                    show_placeholder: true,
                    image_types: &["Primary"],
                }),
            );
        })
        .unwrap();
    let buffer = terminal.backend().buffer();
    for y in 2..14 {
        for x in 2..20 {
            assert_eq!(
                buffer[(x, y)].bg,
                palette::BORDER_UNFOCUSED,
                "unpainted portrait cell at {x},{y}"
            );
        }
    }
}

#[test]
fn wide_tv_persists_series_workspace_and_separate_targets() {
    let mut app = tv_app();
    let mut layout = crate::app::layout::LayoutMain::default();
    let (output, component) = render_tv_workspace(&mut app, &mut layout);

    assert!(layout.tv_wide_right_area.width > 0 && layout.tv_wide_right_area.height > 0);
    assert!(component.test_layout().tv_wide_episode_list_area.height > 0);
    assert!(
        output.contains("Series:"),
        "season tabs are missing: {output}"
    );
    assert!(output.contains("The Series"));
    assert!(output.contains("Pilot"));
    assert!(output.contains("1:00:00"));
}

/// `remove-migrated-surface-underpaint` 3.3 (D4): at the wide Wide hero
/// breakpoint the mounted `TvWorkspaceComponent` owns the picture.
/// `render_library` publishes the `tv_wide_*` geometry hand-off and
/// `render_list` then returns (`src/app/render/components/list.rs:113`)
/// without painting the series hero, season tabs, or episode table.
/// Mirrors the Home precedent
/// `legacy_base_frame_does_not_paint_home_content_before_the_component`.
#[test]
fn wide_tv_legacy_base_frame_publishes_geometry_but_paints_no_workspace() {
    let mut app = tv_app();
    let mut layout = LayoutMain::default();
    let area = Rect::new(0, 0, 100, 30);
    let mut term = Terminal::new(TestBackend::new(100, 30)).unwrap();
    term.draw(|f| {
        app.render_library(f, area, &mut layout, None);
    })
    .unwrap();

    assert!(
        layout.tv_wide_right_area.width > 0 && layout.tv_wide_right_area.height > 0,
        "wide TV geometry hand-off must still be reserved: {:?}",
        layout.tv_wide_right_area
    );
    let output = buffer_to_string(&term);
    assert!(
        !output.contains("Pilot") && !output.contains("The Series"),
        "legacy base frame must not paint the TV workspace at the wide breakpoint: {output:?}"
    );
}

#[test]
fn wide_series_render_keeps_loading_treatment_during_season_fan_out() {
    let mut app = tv_app();
    app.series_detail_cache
        .get_mut("series")
        .unwrap()
        .episodes
        .clear();
    app.series_detail_loading.insert("series".into());
    app.series_season_loading
        .insert(("series".into(), "season-1".into()));

    let (output, _component) = render_tv_workspace(&mut app, &mut LayoutMain::default());

    assert!(output.contains("Loading"), "{output}");
}

#[test]
fn wide_series_with_no_seasons_keeps_the_child_region_blank() {
    let mut app = tv_app();
    app.series_detail_cache
        .get_mut("series")
        .unwrap()
        .seasons
        .clear();
    let mut layout = LayoutMain::default();

    let (output, component) = render_tv_workspace(&mut app, &mut layout);

    assert!(output.contains("The Series"), "{output}");
    assert!(!output.contains("No items available"), "{output}");
    assert!(!output.contains("Empty"), "{output}");
    assert!(component.test_layout().tv_wide_season_tabs.is_empty());
    assert_eq!(component.test_layout().tv_wide_episode_list_area.height, 0);
}

#[test]
fn wide_tv_episode_list_uses_soft_accent_when_focused() {
    // A second episode (task 4.2d) so there is an unselected row: the
    // canonical episode `WideMediaList` paints its own selected-row
    // background (`palette::list_selected_row_bg`) over the cursor row, so
    // the box-level soft accent this test characterizes is now only visible
    // through an unselected row.
    let mut app = tv_app();
    let mut second_episode = make_item("Episode Two", "Episode");
    second_episode.id = "episode-2".into();
    app.series_detail_cache
        .get_mut("series")
        .unwrap()
        .episodes
        .get_mut("season-1")
        .unwrap()
        .push(second_episode);
    let mut component = TvWorkspaceComponent::new();
    component.set_content(
        app.wide_tv_render_ctx(0, None)
            .with_image_state(false, false),
    );
    component.set_focused(true);
    component.on(&tuirealm::event::Event::Keyboard(
        tuirealm::event::KeyEvent {
            code: tuirealm::event::Key::Right,
            modifiers: tuirealm::event::KeyModifiers::NONE,
        },
    ));
    let mut terminal = Terminal::new(TestBackend::new(100, 30)).unwrap();
    terminal.draw(|f| component.view(f, f.area())).unwrap();

    let episode_list_area = component.test_layout().tv_wide_episode_list_area;
    let unselected_row_y = episode_list_area.y.saturating_add(1);
    assert_eq!(
        terminal.backend().buffer()[(
            episode_list_area.x.saturating_sub(PANE_PAD_X),
            unselected_row_y
        )]
            .bg,
        palette::SURFACE_ACCENT_SOFT
    );
}

/// migrate-home-feeds 5.1 (§5 geometry test): the shared Wide hero
/// primitive owns the one-row status-bar reserve, so wide TV's framed series
/// rail paints its `▁` bottom border two rows above `tv_wide_area`'s bottom,
/// leaving exactly one blank row before the status bar. Asserted against the
/// painted buffer so a one-row vertical shift is caught.
#[test]
fn wide_tv_series_rail_leaves_exactly_one_row_above_the_status_bar() {
    let app = tv_app();
    let mut component = TvWorkspaceComponent::new();
    component.set_content(
        app.wide_tv_render_ctx(0, None)
            .with_image_state(false, false),
    );
    component.set_focused(true);
    let area = Rect::new(0, 0, 100, 30);
    let mut terminal = Terminal::new(TestBackend::new(area.width, area.height)).unwrap();
    terminal.draw(|f| component.view(f, area)).unwrap();
    let right = component.test_layout().tv_wide_right_area;
    assert!(right.height > 0, "wide TV right rail must paint");
    crate::app::render::test_helpers::assert_list_pane_reserves_one_row_above_status(
        terminal.backend().buffer(),
        right,
        area.bottom(),
    );
}

/// Library wide view: exactly one of the two panes carries the focus-green
/// background at a time. When the episode (left) pane takes focus the right
/// series rail must drop to `SURFACE_RESTING`, never stay green.
#[test]
fn wide_tv_left_focus_drops_the_right_rail_to_the_resting_surface() {
    let app = tv_app();
    let mut component = TvWorkspaceComponent::new();
    component.set_content(
        app.wide_tv_render_ctx(0, None)
            .with_image_state(false, false),
    );
    component.set_focused(true);
    component.on(&tuirealm::event::Event::Keyboard(
        tuirealm::event::KeyEvent {
            code: tuirealm::event::Key::Right,
            modifiers: tuirealm::event::KeyModifiers::NONE,
        },
    ));
    let mut terminal = Terminal::new(TestBackend::new(100, 30)).unwrap();
    terminal.draw(|f| component.view(f, f.area())).unwrap();

    let rail = component.test_layout().tv_wide_list_area;
    // A row two below the letter heading is panel body, not the selected row.
    assert_eq!(
        terminal.backend().buffer()[(rail.x, rail.y + 2)].bg,
        palette::SURFACE_RESTING,
        "right rail must lose focus-green when the episode pane is focused"
    );
}

#[test]
fn wide_tv_focused_series_browser_uses_focused_surface() {
    fn render(focused: bool) -> (ratatui::buffer::Buffer, LayoutMain) {
        let mut app = tv_app();
        let area = Rect::new(0, 0, 100, 30);
        let mut layout = LayoutMain::default();
        let mut component = TvWorkspaceComponent::new();
        component.set_content(app.wide_tv_render_ctx(0, None));
        component.set_focused(focused);
        let mut terminal = Terminal::new(TestBackend::new(100, 30)).unwrap();
        terminal
            .draw(|f| {
                f.render_widget(
                    Block::default().style(Style::default().bg(palette::SURFACE_BACKDROP)),
                    area,
                );
                app.render_library(f, area, &mut layout, None);
                component.view(f, area);
            })
            .unwrap();
        (terminal.backend().buffer().clone(), layout)
    }

    // `tv_wide_list_area.y` is the letter heading; `y + 1` is the first
    // (selected) series row, `y - 1` the panel top edge.
    let (focused_buffer, focused_layout) = render(true);
    let fla = focused_layout.tv_wide_list_area;
    // Focused rail: panel body is the focused surface, and the selected row
    // takes the resting surface (legacy `item_cell_spans` parity) so it
    // reads against the green panel body.
    assert_eq!(
        focused_buffer[(fla.x.saturating_sub(1), fla.y.saturating_sub(1))].bg,
        palette::resolve_surface_focus(true)
    );
    assert_eq!(
        focused_buffer[(fla.x, fla.y + 1)].bg,
        palette::SURFACE_RESTING
    );
    assert_ne!(
        focused_buffer[(fla.x, fla.y + 1)].bg,
        focused_buffer[(fla.x, fla.y + 2)].bg,
        "selected row must be distinct from the panel body"
    );

    let (unfocused_buffer, unfocused_layout) = render(false);
    let ula = unfocused_layout.tv_wide_list_area;
    // Unfocused rail: panel drops to the resting surface and there is no
    // selection highlight — the selected row is indistinguishable from the
    // body.
    assert_eq!(
        unfocused_buffer[(ula.x.saturating_sub(1), ula.y.saturating_sub(1))].bg,
        palette::resolve_surface_focus(false)
    );
    assert_eq!(
        unfocused_buffer[(ula.x, ula.y + 1)].bg,
        unfocused_buffer[(ula.x, ula.y + 2)].bg,
        "unfocused rail shows no selection highlight"
    );
}
