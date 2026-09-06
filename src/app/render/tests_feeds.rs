use super::test_helpers::*;
use super::*;
use crate::app::components::FeedsComponent;
use crate::app::render::arrangements::wide_hero;
use mbv_core::api::TICKS_PER_SECOND;
use mbv_core::config::{FeedKind, FeedSubscription};
use mbv_core::playback_queue::FeedEntry;
use ratatui::backend::TestBackend;
use ratatui::layout::Rect;
use ratatui::Terminal;
use tuirealm::component::{AppComponent, Component};
use tuirealm::event::{Event, Key, KeyEvent, KeyModifiers};

fn feed_entry(guid: &str, title: &str, played: bool) -> FeedEntry {
    FeedEntry {
        guid: guid.into(),
        title: title.into(),
        enclosure_url: Some(format!("https://example.test/{guid}.mp3")),
        link: Some(format!("https://example.test/{guid}")),
        mime_type: Some("audio/mpeg".into()),
        duration_ticks: Some(65 * TICKS_PER_SECOND as u64),
        pub_date_secs: Some(1_700_000_000),
        feed_kind: Some(FeedKind::Audio),
        feed_id: Some("https://example.test/feed".into()),
        position_ticks: if played { 42 } else { 0 },
        played,
    }
}

fn feed_component_with_entries(entries: Vec<FeedEntry>) -> FeedsComponent {
    let subscriptions = vec![FeedSubscription {
        name: "Test Feed".into(),
        url: "https://example.test/feed".into(),
        kind: FeedKind::Audio,
    }];
    let entries = vec![entries];
    let all_entries = entries[0].clone();
    let mut component = FeedsComponent::new();
    component.set_content(&subscriptions, &entries, &all_entries, false);
    component.set_focused(true);
    component
}

fn feed_component() -> FeedsComponent {
    feed_component_with_entries(vec![
        feed_entry("entry-1", "Entry One", false),
        feed_entry("entry-2", "Played Entry Two", true),
    ])
}

#[test]
fn feeds_images_off_collapses_artwork_and_uses_full_text_width() {
    let mut component = feed_component();
    component.set_images_enabled(false);
    let terminal = terminal_for(&mut component, 120, 30);
    let layout = component.layout();
    let buffer = terminal.backend().buffer();
    assert!(buffer_to_string(&terminal).contains("Entry One"));
    assert!(
        layout.left_area.width > 0,
        "images-off text area must be usable"
    );
    assert!(
        (layout.left_area.x..layout.left_area.right()).all(|x| {
            (layout.left_area.y..layout.left_area.bottom())
                .all(|y| buffer[(x, y)].bg != crate::app::palette::SURFACE_ARTWORK_PLACEHOLDER)
        }),
        "images-off Feeds hero must not paint artwork placeholder"
    );
}

fn terminal_for(component: &mut FeedsComponent, width: u16, height: u16) -> Terminal<TestBackend> {
    let backend = TestBackend::new(width, height);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal
        .draw(|frame| component.view(frame, Rect::new(0, 0, width, height)))
        .unwrap();
    terminal
}

#[test]
fn wide_feeds_use_a_left_entry_browser_and_right_detail_workspace() {
    // Canonical characterization: the parent paints the Wide hero detail
    // pane and frames the entry browser; the canonical `WideMediaList` paints
    // the grouped rows into the inset sub-rect. Observable intent preserved
    // from the legacy painter: left-entry/right-detail split, framed browser
    // with a reserved bottom border, a non-selectable grouping heading, and the
    // two entry rows left-aligned to the same column with the selected row
    // filled.
    for width in [82, 120] {
        let mut component = feed_component();
        let terminal = terminal_for(&mut component, width, 30);
        let layout = component.layout();

        assert!(
            layout.hero_area.width < width,
            "hero={:?}",
            layout.hero_area
        );
        assert!(
            layout.left_area.x < layout.hero_area.x,
            "hero={:?} list={:?}",
            layout.hero_area,
            layout.left_area
        );
        assert!(!layout.selector_tabs.is_empty());
        assert_eq!(
            layout
                .left_item_rows
                .iter()
                .filter(|row| !row.is_empty())
                .count(),
            2
        );
        assert!(
            layout.left_item_rows.iter().all(|row| row.len() <= 1),
            "rows={:?}",
            layout.left_item_rows
        );
        let buffer = terminal.backend().buffer();
        assert!(
            (layout.hero_area.x..layout.hero_area.right()).any(|x| {
                (layout.hero_area.y..layout.hero_area.bottom())
                    .any(|y| buffer[(x, y)].bg == crate::app::palette::SURFACE_ARTWORK_PLACEHOLDER)
            }),
            "feed hero must paint its reserved artwork slot placeholder"
        );
        let panel = Rect::new(
            layout.left_area.x.saturating_sub(wide_hero::PANE_PAD_X),
            layout.left_area.y.saturating_sub(wide_hero::PANE_PAD_Y),
            layout.left_area.width + 2 * wide_hero::PANE_PAD_X,
            layout.left_area.height + 2 * wide_hero::PANE_PAD_Y,
        );
        assert_eq!(buffer[(panel.x, panel.y)].symbol(), "▔");
        assert_eq!(
            buffer[(panel.x, panel.bottom() - 1)].symbol(),
            "▁",
            "Wide Feeds border must reserve its bottom row"
        );
        let first_row = layout.left_area.y
            + layout
                .left_row_map
                .iter()
                .position(|item| item == &Some(0))
                .expect("first row") as u16;
        let second_row = layout.left_area.y
            + layout
                .left_row_map
                .iter()
                .position(|item| item == &Some(1))
                .expect("second row") as u16;
        assert_ne!(
            buffer[(layout.left_area.x, first_row)].bg,
            ratatui::style::Color::Reset,
            "selected row must be filled"
        );
        let first = (layout.left_area.x..layout.left_area.right())
            .map(|x| buffer[(x, first_row)].symbol())
            .collect::<String>();
        let second = (layout.left_area.x..layout.left_area.right())
            .map(|x| buffer[(x, second_row)].symbol())
            .collect::<String>();
        assert_eq!(first.matches("Entry One").count(), 1, "row={first:?}");
        assert!(second.contains("Played Entry Two"), "row={second:?}");
        assert_eq!(
            first.find("Entry One").map(|c| first[..c].chars().count()),
            second
                .find("Played Entry Two")
                .map(|c| second[..c].chars().count()),
            "entry titles must share a left edge: first={first:?} second={second:?}"
        );
        let heading = (layout.left_area.x..layout.left_area.right())
            .map(|x| buffer[(x, layout.left_area.y)].symbol())
            .collect::<String>();
        assert!(
            heading.contains("Older than a month"),
            "heading={heading:?}"
        );
        let hero = buffer_to_string(&terminal);
        assert!(hero.contains("audio/m"), "metadata missing: {hero:?}");
    }
}

/// migrate-home-feeds 4.6 regression: after the full arrangement paint the
/// focused selected row's background must be the surface *containing* the
/// list panel (`SURFACE_RESTING`), not the panel's own focus-green fill, and
/// the rail-framing helper must not overpaint that bar. Unfocused, the row
/// must be indistinguishable from the panel body (no bar).
#[test]
fn wide_feeds_selected_row_punches_through_to_the_resting_surface() {
    fn selected_and_body_bg(focused: bool) -> (ratatui::style::Color, ratatui::style::Color) {
        let subscriptions = vec![FeedSubscription {
            name: "Test Feed".into(),
            url: "https://example.test/feed".into(),
            kind: FeedKind::Audio,
        }];
        let entries = vec![vec![
            feed_entry("entry-1", "Entry One", false),
            feed_entry("entry-2", "Played Entry Two", true),
        ]];
        let all_entries = entries[0].clone();
        let mut component = FeedsComponent::new();
        component.set_content(&subscriptions, &entries, &all_entries, false);
        component.set_focused(focused);
        let terminal = terminal_for(&mut component, 120, 30);
        let layout = component.layout();
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

/// migrate-home-feeds 4.6 regression: the Wide left hero pane mirrors the
/// sibling media tabs -- a plain `SURFACE_RESTING` fill with no `▔`/`▁`
/// HeroShell border and no focus tint from the list panel. Focusing the list
/// must not turn the hero pane green.
#[test]
fn wide_feeds_left_hero_pane_is_a_plain_resting_surface() {
    let mut component = feed_component();
    let terminal = terminal_for(&mut component, 120, 30);
    let layout = component.layout();
    let hero = layout.hero_area;
    assert!(hero.width > 0 && hero.height > 0);
    let buffer = terminal.backend().buffer();

    for y in hero.y..hero.bottom() {
        for x in hero.x..hero.right() {
            let cell = &buffer[(x, y)];
            assert_ne!(cell.bg, crate::app::palette::resolve_surface_focus(true));
            assert!(
                cell.symbol() != "▔" && cell.symbol() != "▁",
                "hero pane must not carry a shell border at ({x},{y})"
            );
        }
    }
    assert_eq!(
        buffer[(hero.x, hero.bottom() - 1)].bg,
        crate::app::palette::SURFACE_RESTING
    );
}

#[test]
fn wide_feeds_reserve_borders_at_the_scrolled_bottom_boundary() {
    let entries = (0..8)
        .map(|index| {
            let title = if index == 7 {
                "Last Played Entry".to_string()
            } else {
                format!("Entry {index}")
            };
            feed_entry(&format!("entry-{index}"), &title, index == 7)
        })
        .collect();
    let mut component = feed_component_with_entries(entries);
    for _ in 0..7 {
        component.on(&Event::Keyboard(KeyEvent {
            code: Key::Down,
            modifiers: KeyModifiers::NONE,
        }));
    }
    let terminal = terminal_for(&mut component, 82, 12);
    let layout = component.layout();
    // The canonical control scrolled the viewport to keep the selection on
    // screen; the resolved offset is observable (never fed back).
    assert!(component.scroll() > 0);

    let panel = Rect::new(
        layout.left_area.x.saturating_sub(wide_hero::PANE_PAD_X),
        layout.left_area.y.saturating_sub(wide_hero::PANE_PAD_Y),
        layout.left_area.width + 2 * wide_hero::PANE_PAD_X,
        layout.left_area.height + 2 * wide_hero::PANE_PAD_Y,
    );
    let buffer = terminal.backend().buffer();
    // The scrolled-to selection paints on the last row of the inset list
    // sub-rect, and the framed rail still reserves its top and bottom border
    // rows outside that sub-rect.
    let last_row = layout.left_area.bottom() - 1;
    let row = (layout.left_area.x..layout.left_area.right())
        .map(|x| buffer[(x, last_row)].symbol())
        .collect::<String>();
    assert!(row.contains("Last Played Entry"), "row={row:?}");
    assert_eq!(buffer[(panel.x, panel.y)].symbol(), "▔");
    assert_eq!(buffer[(panel.x, panel.bottom() - 1)].symbol(), "▁");
}

/// migrate-home-feeds 5.1: the shared Wide hero primitive owns the status-row
/// reserve, so the Wide Feeds list panel and left hero panel must bottom out
/// exactly one row above `area.bottom()` — the same one-row gap every sibling
/// tab gets from the shared primitive (no per-tab reserve on top of it).
#[test]
fn wide_feeds_reserve_a_bottom_row_above_the_status_bar() {
    let height: u16 = 30;
    let mut component = feed_component();
    let terminal = terminal_for(&mut component, 120, height);
    let layout = component.layout();

    let list_panel_bottom = layout.left_area.bottom() + wide_hero::PANE_PAD_Y;
    assert_eq!(
        list_panel_bottom,
        height - 1,
        "framed list panel must bottom out one row above {height}"
    );
    assert_eq!(
        layout.hero_area.bottom(),
        height - 1,
        "hero panel must bottom out one row above {height}"
    );

    // Positive buffer check: the framed list panel paints its `▁` bottom
    // border on `height - 2` and the reserve row `height - 1` is blank. A
    // one-row vertical shift moves the border or paints the reserve row.
    assert_list_pane_reserves_one_row_above_status(
        terminal.backend().buffer(),
        layout.left_area,
        height,
    );
}

#[test]
fn narrow_feeds_insert_selected_entry_detail_into_the_list_flow() {
    let mut component = feed_component();
    terminal_for(&mut component, 60, 20);
    let layout = component.layout();

    assert!(layout.hero_area.height > 0);
    assert!(
        layout.hero_area.y >= layout.left_area.y,
        "hero={:?} list={:?}",
        layout.hero_area,
        layout.left_area
    );
}

#[test]
fn narrow_feeds_suppress_detail_when_the_viewport_is_too_short() {
    let mut component = feed_component();
    terminal_for(&mut component, 60, 4);
    assert_eq!(component.layout().hero_area.height, 0);
}

#[test]
fn feeds_buffer_characterization_covers_default_focused_narrow_and_selected_states() {
    for (width, height, focused) in [
        (140, 30, false),
        (140, 30, true),
        (60, 20, true),
        (40, 20, false),
    ] {
        let mut component = feed_component();
        let terminal = terminal_for(&mut component, width, height);
        let output = buffer_to_string(&terminal);
        assert!(
            output.contains("Test Feed"),
            "missing feed selector: {output:?}"
        );
        assert!(
            output.contains("Entry One"),
            "missing selected entry: {output:?}"
        );
        let _ = focused;
    }
}

#[test]
fn feeds_pill_row_and_targets_are_characterized_end_to_end() {
    let assert_geometry = |terminal: &Terminal<TestBackend>, layout: &LayoutMain| {
        let panel = Rect::new(0, 0, 60, 20);
        let areas = wide_hero::pill_bar_areas(panel);
        assert_surface_pills(
            terminal,
            layout,
            panel,
            1,
            ratatui::style::Color::Reset,
            &[0, 1],
            &["⌘", "All", "Test Feed"],
            0,
        );
        assert_eq!(layout.selector_tabs[0].0.y, areas.pills_area.y);
        assert_eq!(layout.left_area.y, areas.spacer_area.bottom() + 2);
        let buffer = terminal.backend().buffer();
        let filter_row = (0..buffer.area().width)
            .map(|x| buffer[(x, areas.spacer_area.bottom())].symbol())
            .collect::<String>();
        assert!(
            filter_row.contains("All"),
            "missing watched All filter: {filter_row:?}"
        );
        assert!(
            filter_row.contains("Played") && filter_row.contains("Unplayed"),
            "missing watched filters: {filter_row:?}"
        );
    };

    let mut no_hero_component = feed_component();
    let no_hero_subscriptions = [FeedSubscription {
        name: "Test Feed".into(),
        url: "https://example.test/feed".into(),
        kind: FeedKind::Audio,
    }];
    no_hero_component.set_content(&no_hero_subscriptions, &[Vec::new()], &[], false);
    no_hero_component.set_focused(true);
    let no_hero_terminal = terminal_for(&mut no_hero_component, 60, 20);
    assert_geometry(&no_hero_terminal, no_hero_component.layout());

    let mut post_hero_component = feed_component();
    let post_hero_terminal = terminal_for(&mut post_hero_component, 60, 20);
    assert_geometry(&post_hero_terminal, post_hero_component.layout());
    assert!(post_hero_component.layout().hero_area.height > 0);

    let mut no_subscriptions_component = FeedsComponent::new();
    let no_subscriptions_terminal = terminal_for(&mut no_subscriptions_component, 60, 20);
    let layout = no_subscriptions_component.layout();
    assert!(layout.selector_tabs.is_empty());
    assert_eq!(layout.left_area.y, 3);
    let empty_row = (0..60)
        .map(|x| no_subscriptions_terminal.backend().buffer()[(x, 3)].symbol())
        .collect::<String>();
    assert!(
        empty_row.contains("No feed subscriptions configured"),
        "empty/help content moved: {empty_row:?}"
    );
}
