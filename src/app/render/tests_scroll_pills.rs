use super::components::widgets::{render_pill_bar, PillBar};
use super::test_helpers::*;
use crate::app::palette;
use ratatui::backend::TestBackend;
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::widgets::Block;
use ratatui::Terminal;

#[test]
fn pill_bar_does_not_paint_the_reserved_spacer_row() {
    let labels = vec!["All".to_string(), "A-C".to_string()];
    let ids = vec![0, 1];
    let mut terminal = Terminal::new(TestBackend::new(20, 2)).unwrap();

    terminal
        .draw(|f| {
            let area = Rect::new(0, 0, 20, 2);
            f.render_widget(
                Block::default().style(Style::default().bg(palette::SURFACE_BACKDROP)),
                area,
            );
            render_pill_bar(
                f,
                area,
                PillBar {
                    labels: &labels,
                    ids: &ids,
                    selected_pos: 0,
                    prefix: None,
                },
            );
        })
        .unwrap();

    let buffer = terminal.backend().buffer();
    assert_eq!(buffer[(19, 0)].bg, palette::PILL_ROW_BG);
    for x in 0..20 {
        assert_eq!(buffer[(x, 1)].bg, palette::SURFACE_BACKDROP);
    }
}

#[test]
fn pill_bar_hitboxes_carry_caller_ids_not_display_positions() {
    let labels: Vec<String> = ["Alpha", "Beta", "Gamma"]
        .iter()
        .map(|s| s.to_string())
        .collect();
    let ids = vec![10usize, 11, 12];

    let tabs = render_pill_bar_hitboxes(&labels, &ids, 0, 60);
    assert_eq!(
        tabs.iter().map(|(_, id)| *id).collect::<Vec<_>>(),
        vec![10, 11, 12],
    );
    for pair in tabs.windows(2) {
        assert!(pair[0].0.x + pair[0].0.width <= pair[1].0.x);
    }
}

#[test]
fn pill_bar_scrolls_to_keep_selected_visible_and_maps_its_id() {
    let labels: Vec<String> = (0..6).map(|i| format!("Group{i}")).collect();
    let ids: Vec<usize> = (0..6).map(|i| 20 + i).collect();

    let tabs = render_pill_bar_hitboxes(&labels, &ids, 5, 18);

    assert!(!tabs.is_empty(), "expected at least one visible pill");
    assert!(
        tabs.iter().any(|(_, id)| *id == 25),
        "selected pill's id should be visible after scrolling, got {:?}",
        tabs.iter().map(|(_, id)| *id).collect::<Vec<_>>(),
    );
    assert!(tabs.iter().all(|(_, id)| (20..=25).contains(id)));
    assert!(
        tabs.len() < labels.len(),
        "narrow row should not fit all six pills"
    );
}

#[test]
fn pill_bar_does_not_pin_a_backwards_selection_to_the_trailing_edge() {
    let labels: Vec<String> = (0..8).map(|i| format!("Group{i}")).collect();
    let ids: Vec<usize> = (0..8).map(|i| 20 + i).collect();

    let tabs = render_pill_bar_hitboxes(&labels, &ids, 4, 38);
    let Some(selected) = tabs.iter().position(|(_, id)| *id == 24) else {
        panic!("selected pill should be visible");
    };

    assert!(
        selected > 0,
        "selected pill should have a visible predecessor"
    );
    assert!(
        selected + 1 < tabs.len(),
        "selected pill should have a visible successor"
    );
}
