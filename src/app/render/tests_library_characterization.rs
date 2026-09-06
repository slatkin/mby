use super::test_helpers::{
    draw_mounted_frame, make_movie_app, mounted_browser_layout, mounted_browser_scroll,
    mounted_model_at,
};
use super::*;
use crate::app::tests::make_item;
use crate::app::TabSelection;

#[test]
fn library_buffer_characterization_covers_wide_unfocused_narrow_and_selected_states() {
    // Note: width 120 triggers wide Movies layout, which is now handled by
    // BrowserComponent (5.3d.17a). Narrow Movies is likewise painted by the
    // mounted `BrowserComponent` now (task 3.8), so route through the real
    // `Model::draw_frame` path.
    let states = [(60, 20, 0), (60, 20, 1)];
    for (width, height, cursor) in states {
        let mut app = make_movie_app();
        app.libs[0].nav_stack[0].set_resting_cursor(cursor);
        let mut model = mounted_model_at(app, width, height);
        let output = draw_mounted_frame(&mut model, width, height);
        assert!(
            output.contains("Movie"),
            "library rows missing in {width}x{height}: {output:?}"
        );
    }
}

// Note: movies_pill_row_and_targets_are_characterized_end_to_end deleted.
// It tested the legacy wide Movies layout, which is now handled by
// BrowserComponent (5.3d.17a). Component rendering is tested separately.

#[test]
fn movies_plain_replacement_characterization_covers_bottom_scroll_fallback_and_targets() {
    let mut app = make_movie_app();
    app.libs[0].nav_stack[0].items[1].overview = "The selected movie overview.".into();
    app.libs[0].nav_stack[0].set_resting_cursor(1);
    app.libs[0].nav_stack[0].set_resting_scroll(1);
    let mut model = mounted_model_at(app, 70, 30);
    let output = draw_mounted_frame(&mut model, 70, 30);
    let layout = mounted_browser_layout(&model);

    assert!(
        output.contains("Second Movie"),
        "selected movie is missing:\n{output}"
    );
    assert!(
        layout.hero_area.height > 0,
        "complete selected replacement should fit: hero={:?}\n{output}",
        layout.hero_area
    );
    let selected_rect = layout
        .selected_item_rect
        .expect("selected movie keeps a parent-owned row target");
    assert_eq!(selected_rect.x, layout.hero_area.x);
    assert_eq!(selected_rect.y, layout.hero_area.y);
    assert_eq!(selected_rect.width, layout.hero_area.width);
    assert!(selected_rect.height > 0);
    let hero_lines = output
        .lines()
        .skip(layout.hero_area.y as usize)
        .take(layout.hero_area.height as usize)
        .collect::<String>();
    assert!(
        !hero_lines.contains('▎'),
        "ordinary selection marker leaked into the hero"
    );
    assert_eq!(
        layout
            .left_row_map
            .iter()
            .filter(|target| **target == Some(1))
            .count(),
        1,
        "replacement owns one parent row: {:?}",
        layout.left_row_map
    );
    assert!(
        layout.left_row_map.iter().any(Option::is_none),
        "replacement continuation rows must remain targetless"
    );
    let control_scroll = mounted_browser_scroll(&model);
    assert!(
        control_scroll > 0,
        "mounted control must retain replacement scroll"
    );
    let _ = draw_mounted_frame(&mut model, 70, 30);
    assert_eq!(
        mounted_browser_scroll(&model),
        control_scroll,
        "mounted control scroll persists across redraws"
    );

    let mut cannot_fit = make_movie_app();
    cannot_fit.libs[0].nav_stack[0].items[1].overview = "The selected movie overview.".into();
    cannot_fit.libs[0].nav_stack[0].set_resting_cursor(1);
    let mut fallback_model = mounted_model_at(cannot_fit, 70, 12);
    let fallback = draw_mounted_frame(&mut fallback_model, 70, 12);
    let fallback_layout = mounted_browser_layout(&fallback_model);
    assert!(
        fallback.contains("Second Movie"),
        "ordinary fallback loses the row:\n{fallback}"
    );
    assert_eq!(fallback_layout.hero_area.height, 0);
    assert!(
        fallback_layout.left_row_map.contains(&Some(1)),
        "ordinary fallback restores the selected row"
    );
}

fn tv_letter_grouped_app(scroll: usize) -> App {
    let mut app = make_movie_app();
    app.tab = TabSelection::EmbyLibrary(0);
    app.libs[0].library.collection_type = "tvshows".into();
    let items = (0..55)
        .map(|i| {
            let mut item = make_item(
                &format!("{} Series {i:02}", (b'A' + (i % 26) as u8) as char),
                "Series",
            );
            item.id = format!("series-{i}");
            item.is_folder = true;
            item.overview = "The selected series overview.".into();
            item
        })
        .collect();
    app.libs[0].nav_stack[0].items = items;
    app.libs[0].nav_stack[0].total_count = 55;
    app.libs[0].nav_stack[0].set_resting_cursor(54);
    app.libs[0].nav_stack[0].set_resting_scroll(scroll);
    app.libs[0].library_total = Some(55);
    app
}

#[test]
fn tv_letter_grouped_replacement_characterization_covers_header_fit_and_marker_suppression() {
    let mut model = mounted_model_at(tv_letter_grouped_app(12), 70, 20);
    let output = draw_mounted_frame(&mut model, 70, 20);
    let layout = mounted_browser_layout(&model);

    assert!(
        output.contains("Series 54"),
        "selected series is missing:\n{output}"
    );
    assert!(
        layout.left_row_map.iter().any(Option::is_none),
        "group headers and continuation rows remain targetless"
    );
    assert_eq!(
        layout
            .left_row_map
            .iter()
            .filter(|target| **target == Some(54))
            .count(),
        1,
        "grouped replacement owns one parent row"
    );
    assert!(
        layout.hero_area.height > 0,
        "grouped complete replacement should fit"
    );
    assert!(
        layout.left_row_map.iter().any(Option::is_none),
        "letter headers have no ordinary target"
    );
    let control_scroll = mounted_browser_scroll(&model);
    assert!(
        control_scroll > 0,
        "mounted grouped control must retain scroll"
    );
    let hero_lines = output
        .lines()
        .skip(layout.hero_area.y as usize)
        .take(layout.hero_area.height as usize)
        .collect::<String>();
    assert!(
        !hero_lines.contains('▎'),
        "ordinary marker leaked into the grouped hero"
    );
    let _ = draw_mounted_frame(&mut model, 70, 20);
    assert_eq!(
        mounted_browser_scroll(&model),
        control_scroll,
        "mounted grouped control scroll persists across redraws"
    );

    let mut boundary_model = mounted_model_at(tv_letter_grouped_app(1), 70, 14);
    let boundary_output = draw_mounted_frame(&mut boundary_model, 70, 14);
    let boundary_layout = mounted_browser_layout(&boundary_model);
    assert!(
        boundary_output.contains("Series 54"),
        "header fit boundary hides selected row: hero={:?} map={:?}\n{boundary_output}",
        boundary_layout.hero_area,
        boundary_layout.left_row_map
    );
    assert_eq!(
        boundary_layout.hero_area.height, 0,
        "cannot-fit grouped detail restores ordinary rows"
    );
    assert!(boundary_layout.left_row_map.contains(&Some(54)));
}

#[test]
fn wide_letter_grouped_row_map_indexes_items_without_counting_headings() {
    // Regression: the Wide `left_row_map` used to project source-row indices,
    // so every painted row after a letter heading or spacer was off by the
    // count of those non-item rows. It must instead map each painted row to
    // the control's selectable index, leaving headings/spacers `None`.
    use crate::app::components::browser::{BrowserComponent, BrowserContent};
    use crate::app::components::component_id::BrowserKind;
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;
    use tuirealm::component::Component;

    let items = (0..55)
        .map(|i| {
            let mut item = make_item(
                &format!("{} Movie {i:02}", (b'A' + (i % 26) as u8) as char),
                "Movie",
            );
            item.id = format!("movie-{i}");
            item
        })
        .collect();
    let mut browser = BrowserComponent::new_for_kind(BrowserKind::Movies);
    browser.set_content(BrowserContent::from_items(items));
    browser.set_focused(true);
    browser.apply_position(54, 40);

    let mut terminal = Terminal::new(TestBackend::new(120, 40)).unwrap();
    terminal
        .draw(|frame| browser.view(frame, frame.area()))
        .unwrap();
    let row_map = &browser.test_layout().left_row_map;

    assert!(
        row_map.iter().any(Option::is_none),
        "grouped Wide flow must carry targetless heading/spacer rows: {row_map:?}"
    );
    let selectable: Vec<usize> = row_map.iter().flatten().copied().collect();
    assert!(
        selectable.len() >= 3,
        "expected several painted item rows: {row_map:?}"
    );
    assert!(
        selectable.iter().all(|&index| index < 55),
        "no painted row may target past the last selectable item; source-row \
         projection inflated these by the heading/spacer count: {row_map:?}"
    );
    assert!(
        selectable.windows(2).all(|pair| pair[1] == pair[0] + 1),
        "consecutive painted item rows map to consecutive selectable indices, \
         not source rows inflated by preceding headings/spacers: {row_map:?}"
    );
}

/// migrate-home-feeds 4.6 regression: after the full wide-Movies arrangement
/// paint, the focused selected row's background is the surface *containing*
/// the list panel (`SURFACE_RESTING`), and the rail-framing helper — which
/// now runs before the row flow — must not overpaint that bar. Unfocused,
/// the row must match the panel body (no bar).
#[test]
fn wide_movies_selected_row_punches_through_to_the_resting_surface() {
    use crate::app::components::browser::{BrowserComponent, BrowserContent};
    use crate::app::components::component_id::BrowserKind;
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;
    use tuirealm::component::Component;

    fn selected_and_body_bg(focused: bool) -> (ratatui::style::Color, ratatui::style::Color) {
        let items = (0..10)
            .map(|i| {
                let mut item = make_item(&format!("Movie {i:02}"), "Movie");
                item.id = format!("movie-{i}");
                item
            })
            .collect();
        let mut browser = BrowserComponent::new_for_kind(BrowserKind::Movies);
        browser.set_content(BrowserContent::from_items(items));
        browser.set_focused(focused);
        browser.apply_position(0, 40);
        let mut terminal = Terminal::new(TestBackend::new(120, 40)).unwrap();
        terminal
            .draw(|frame| browser.view(frame, frame.area()))
            .unwrap();
        let layout = browser.test_layout();
        let buffer = terminal.backend().buffer();
        let row_for = |target: usize| {
            layout.left_area.y
                + layout
                    .left_row_map
                    .iter()
                    .position(|item| item == &Some(target))
                    .expect("row present") as u16
        };
        (
            buffer[(layout.left_area.x, row_for(0))].bg,
            buffer[(layout.left_area.x, row_for(1))].bg,
        )
    }

    let (selected, body) = selected_and_body_bg(true);
    assert_eq!(selected, crate::app::palette::SURFACE_RESTING);
    assert_eq!(body, crate::app::palette::resolve_surface_focus(true));
    assert_ne!(selected, body);

    let (selected, body) = selected_and_body_bg(false);
    assert_eq!(selected, body, "unfocused rail shows no selection bar");
}

/// migrate-home-feeds 5.1 (§5 geometry test): the shared Wide hero
/// primitive owns the one-row status-bar reserve, so wide Movies' framed list
/// panel paints its `▁` bottom border two rows above the destination area's
/// bottom, leaving exactly one blank row before the status bar. Asserted
/// against the painted buffer so a one-row vertical shift is caught.
#[test]
fn wide_movies_list_panel_leaves_exactly_one_row_above_the_status_bar() {
    use crate::app::components::browser::{BrowserComponent, BrowserContent};
    use crate::app::components::component_id::BrowserKind;
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;
    use tuirealm::component::Component;

    let items = (0..40)
        .map(|i| {
            let mut item = make_item(&format!("Movie {i:02}"), "Movie");
            item.id = format!("movie-{i}");
            item
        })
        .collect();
    let mut browser = BrowserComponent::new_for_kind(BrowserKind::Movies);
    browser.set_content(BrowserContent::from_items(items));
    browser.set_focused(true);
    browser.apply_position(0, 40);

    let area = ratatui::layout::Rect::new(0, 0, 120, 40);
    let mut terminal = Terminal::new(TestBackend::new(area.width, area.height)).unwrap();
    terminal.draw(|frame| browser.view(frame, area)).unwrap();
    let right = browser.test_layout().movies_wide_right_area;
    assert!(right.height > 0, "wide movies right pane must paint");
    super::test_helpers::assert_list_pane_reserves_one_row_above_status(
        terminal.backend().buffer(),
        right,
        area.bottom(),
    );
}
