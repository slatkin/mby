mod plain_rows;
mod wide;
mod wide_row;

pub(in crate::app) use plain_rows::render_plain_rows;
pub(in crate::app) use wide::{render_inline_media_browser, render_wide_media_list};

// §3.2 one-painter instrumentation: per-frame execution counters for the two
// canonical wide list paint entry points. Tests reset these, render one
// frame, and assert exactly one wide list painter ran for a destination.
#[cfg(test)]
thread_local! {
    pub(in crate::app) static WIDE_MEDIA_LIST_PAINTS: std::cell::Cell<usize> =
        const { std::cell::Cell::new(0) };
    pub(in crate::app) static PLAIN_ROWS_PAINTS: std::cell::Cell<usize> =
        const { std::cell::Cell::new(0) };
    pub(in crate::app) static INLINE_MEDIA_BROWSER_PAINTS: std::cell::Cell<usize> =
        const { std::cell::Cell::new(0) };
}

#[cfg(test)]
mod wide_row_regression_tests {
    use super::wide::render_wide_media_list;
    use super::wide_row_regression_tests_helpers::{item, paint, row_of};
    use crate::app::components::media_list::{MediaKind, WideMediaList};
    use crate::app::palette;
    use ratatui::backend::TestBackend;
    use ratatui::layout::Rect;
    use ratatui::Terminal;

    /// migrate-home-feeds 4.6: the selected row's highlight bar must span the
    /// whole panel width (never just the row text, with or without a duration
    /// string), the edge marker must sit flush at the panel's `x`, and the
    /// title must land at column 2. These broke together when the painter was
    /// handed an already-inset content rect.
    #[test]
    fn selected_row_spans_full_width_with_flush_marker_and_three_col_indent() {
        const PX: u16 = 10;
        const PW: u16 = 40;
        let selected_bg = palette::SURFACE_RESTING;

        for duration in [None, Some("1:05".to_string())] {
            let mut list: WideMediaList<String> = WideMediaList::new();
            list.set_content(vec![
                item("sel", "Selected Entry", duration.clone()),
                item("other", "Other Entry", None),
            ]);

            let mut terminal = Terminal::new(TestBackend::new(60, 6)).unwrap();
            terminal
                .draw(|f| {
                    render_wide_media_list(
                        f,
                        Rect::new(PX, 0, PW, 4),
                        Rect::new(PX, 0, PW, 4),
                        &mut list,
                        true,
                        selected_bg,
                    );
                })
                .unwrap();
            let buf = terminal.backend().buffer();

            assert_eq!(
                buf[(PX, 0)].symbol(),
                "▎",
                "edge marker must be flush at the panel x (duration={duration:?})"
            );
            // Skip the flush marker glyph itself; the title is the next
            // non-blank cell.
            let first_text = (PX + 1..PX + PW)
                .find(|&x| buf[(x, 0)].symbol().trim() != "")
                .map(|x| x - PX);
            assert_eq!(
                first_text,
                Some(2),
                "title text indent must be 2 columns (duration={duration:?})"
            );
            for x in PX..PX + PW {
                assert_eq!(
                    buf[(x, 0)].bg,
                    selected_bg,
                    "selected-row bar must fill column {x} (duration={duration:?})"
                );
            }
            assert_ne!(
                buf[(PX, 1)].bg,
                selected_bg,
                "only the selected row is filled (duration={duration:?})"
            );
        }
    }

    /// Step 4 latent bug: the painter must persist the resolved scroll offset
    /// back into `list` so it survives across frames. Home discarded the old
    /// `usize` return, so its rail always re-scrolled to the top.
    #[test]
    fn painter_persists_resolved_scroll_offset_across_frames() {
        let rect = Rect::new(0, 0, 40, 4);
        let selected_bg = palette::SURFACE_RESTING;
        let mut list: WideMediaList<String> = WideMediaList::new();
        list.set_content(
            (0..12)
                .map(|i| item(&format!("t{i}"), &format!("Entry {i}"), None))
                .collect(),
        );
        list.select_last();

        let first = paint(&mut list, rect, selected_bg);
        let resolved = first.row_geometry.offset();
        assert!(resolved > 0, "a bottom selection must scroll the viewport");
        assert_eq!(
            list.scroll(),
            resolved,
            "painter stores the offset it resolved"
        );

        // Re-render with no further input: the stored offset is reused, not reset.
        let second = paint(&mut list, rect, selected_bg);
        assert_eq!(second.row_geometry.offset(), resolved);
        assert_eq!(list.scroll(), resolved);
    }

    /// canonical-list-duration-kind 1.2: the painter suppresses the duration
    /// slot for `Collection` rows even when one is projected, and paints a
    /// `Media` row's duration right-aligned in `STATUS_AVAILABLE` green.
    #[test]
    fn collection_row_suppresses_projected_duration_media_row_paints_it() {
        let rect = Rect::new(0, 0, 40, 4);
        let selected_bg = palette::SURFACE_RESTING;
        let dur = crate::app::ui_util::list_duration_secs(272); // 4:32
        assert_eq!(dur.as_deref(), Some("4:32"));
        let mut list: WideMediaList<String> = WideMediaList::new();
        list.set_content(vec![
            row_of("coll", "Some Album", dur.clone(), MediaKind::Collection),
            row_of("leaf", "Some Track", dur, MediaKind::Media),
        ]);

        let mut terminal = Terminal::new(TestBackend::new(40, 4)).unwrap();
        terminal
            .draw(|f| {
                render_wide_media_list(f, rect, rect, &mut list, true, selected_bg);
            })
            .unwrap();
        let buf = terminal.backend().buffer();

        let row_text = |y: u16| {
            (0..rect.width)
                .map(|x| buf[(x, y)].symbol().to_string())
                .collect::<String>()
        };
        assert!(
            !row_text(0).contains("4:32"),
            "Collection row must not paint a projected duration: {:?}",
            row_text(0)
        );
        let media = row_text(1);
        assert!(
            media.contains("4:32"),
            "Media row paints its duration: {media:?}"
        );
        assert!(
            media.trim_end().ends_with("4:32"),
            "Media duration is right-aligned: {media:?}"
        );
        let dur_x = rect.width - 4;
        assert_eq!(
            buf[(dur_x, 1)].fg,
            palette::STATUS_AVAILABLE,
            "Media duration is painted green"
        );
    }

    /// The duration right-aligns to 2 columns from the panel edge whether or
    /// not the focused list overflows and reserves a scrollbar column — the
    /// scrollbar must not shift it another column inwards.
    #[test]
    fn duration_right_inset_is_two_columns_with_and_without_scrollbar() {
        let selected_bg = palette::SURFACE_RESTING;
        for (rows_count, focused) in [(3usize, false), (3, true), (12, true)] {
            let rect = Rect::new(0, 0, 40, 4);
            let mut list: WideMediaList<String> = WideMediaList::new();
            list.set_content(
                (0..rows_count)
                    .map(|i| item(&format!("t{i}"), &format!("Entry {i}"), Some("4:32".into())))
                    .collect(),
            );

            let mut terminal = Terminal::new(TestBackend::new(40, 4)).unwrap();
            terminal
                .draw(|f| {
                    render_wide_media_list(f, rect, rect, &mut list, focused, selected_bg);
                })
                .unwrap();
            let buf = terminal.backend().buffer();

            // "4:32" ends 2 columns before the panel's right edge; the two
            // cells before it are the quiet gap.
            let dur_start = rect.width - 2 - 4;
            for (i, ch) in "4:32".chars().enumerate() {
                assert_eq!(
                    buf[(dur_start + i as u16, 0)].symbol(),
                    ch.to_string(),
                    "duration at fixed inset (rows={rows_count}, focused={focused}, i={i})"
                );
            }
            let gap_x = rect.width - 2 - 4 - 1;
            assert_eq!(
                buf[(gap_x, 0)].symbol(),
                " ",
                "quiet gap before duration (rows={rows_count}, focused={focused})"
            );
        }
    }
}

#[cfg(test)]
mod wide_row_regression_tests_helpers {
    use super::wide::MediaListPaint;
    use crate::app::components::media_list::{
        MediaKind, MediaListRow, MediaSemanticState, WideMediaList,
    };
    use ratatui::backend::TestBackend;
    use ratatui::layout::Rect;
    use ratatui::style::Color;
    use ratatui::Terminal;

    pub(super) fn item(
        target: &str,
        primary: &str,
        duration: Option<String>,
    ) -> MediaListRow<String> {
        row_of(target, primary, duration, MediaKind::Media)
    }

    pub(super) fn row_of(
        target: &str,
        primary: &str,
        duration: Option<String>,
        kind: MediaKind,
    ) -> MediaListRow<String> {
        MediaListRow::Item {
            target: target.into(),
            primary: primary.into(),
            trailing: None,
            duration,
            kind,
            semantic_state: MediaSemanticState::Ordinary,
        }
    }

    pub(super) fn paint(
        list: &mut WideMediaList<String>,
        rect: Rect,
        selected_bg: Color,
    ) -> MediaListPaint<String> {
        let mut terminal = Terminal::new(TestBackend::new(80, 20)).unwrap();
        let mut captured = None;
        terminal
            .draw(|f| {
                captured = Some(render_wide_media_list(
                    f,
                    rect,
                    rect,
                    list,
                    true,
                    selected_bg,
                ));
            })
            .unwrap();
        captured.unwrap()
    }

    use super::wide::render_wide_media_list;
}
