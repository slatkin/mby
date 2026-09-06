//! Shared Inline Search painter. Destinations supply the pill and result
//! rectangles; this component paints both parts of the presentation.

use ratatui::layout::Rect;
use ratatui::Frame;

use crate::app::layout::LayoutMain;
use crate::app::render::{render_generic_movies_home_video_rows_with_ctx, LibraryListRenderCtx};

/// Paints one embedded Inline Search frame into the supplied pill and result
/// rectangles and returns the result list's scroll offset to persist (mirrors
/// `render_generic_movies_home_video_rows_with_ctx`). `items` is the
/// caller's already-scored, already-ordered result set (design.md D2); this
/// function only places and paints.
#[allow(clippy::too_many_arguments)]
pub(in crate::app) fn render_inline_search(
    f: &mut Frame,
    pill_area: Rect,
    result_area: Rect,
    query: &str,
    loading: bool,
    items: Vec<mbv_core::api::EmbyItem>,
    cursor: usize,
    scroll: usize,
    focused: bool,
    columns: usize,
    layout: &mut LayoutMain,
) -> usize {
    crate::app::render::components::hero::render_search_box(f, pill_area, query, loading);
    let ctx = LibraryListRenderCtx::from_items(items, cursor, scroll)
        .with_search(query.to_string(), loading);
    render_generic_movies_home_video_rows_with_ctx(f, result_area, &ctx, focused, columns, layout)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::tests::make_item;
    use ratatui::{backend::TestBackend, Terminal};

    #[test]
    fn render_inline_search_uses_supplied_one_row_and_result_rects() {
        let mut terminal = Terminal::new(TestBackend::new(40, 8)).unwrap();
        let mut layout = LayoutMain::default();
        let pill_area = Rect {
            x: 3,
            y: 1,
            width: 30,
            height: 1,
        };
        let result_area = Rect {
            x: 4,
            y: 3,
            width: 24,
            height: 5,
        };
        terminal
            .draw(|f| {
                render_inline_search(
                    f,
                    pill_area,
                    result_area,
                    "on",
                    false,
                    vec![make_item("One", "Movie")],
                    0,
                    0,
                    true,
                    1,
                    &mut layout,
                );
            })
            .unwrap();
        let buffer = terminal.backend().buffer();

        let pill: String = (pill_area.left()..pill_area.right())
            .map(|x| buffer.cell((x, pill_area.top())).unwrap().symbol())
            .collect();
        assert!(pill.contains("SEARCH:") && pill.contains("on█"));
        assert!(!pill.contains("┌") && !pill.contains("└"));
        assert_eq!(layout.left_area, result_area);
    }
}
