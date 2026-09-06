//! The `List` component (design.md "Component catalogue"): row rendering,
//! the shared `SelectionMarker`, and the row/cell padding it composes with,
//! extracted from and shared by movies/TV's list renderers
//! (`list_letter_groups.rs`, `media_list.rs`, both consumers of
//! `item_cell_spans`/`draw_column_selection_markers` below) and reused by
//! the audiobookshelf show grid. `ListRenderCtx`/`DisplayRow` are its row
//! model; `render_right_scrollbar` (`widgets.rs`) is its `Scrollbar`.
//! Screens still call these functions directly and record their own row hit
//! targets on `LayoutMain` rather than getting one back from a single
//! entry point -- unifying that return shape, and folding in grouped
//! Music's structurally different row model, is design.md's phase
//! 8 ("Unified mouse hit targets"), not this extraction phase.

use crate::app::palette;
use ratatui::layout::Rect;
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Paragraph};
use ratatui::Frame;

/// Standard inset for every selected detail block.
pub(in crate::app::render) const SELECTED_BLOCK_SIDE_PADDING: u16 = 2;

/// Returns `palette::TEXT_EMPHASIS` when `focused`, `palette::TEXT_SECONDARY` otherwise.
pub(in crate::app::render) fn focused_or_subtle(focused: bool) -> Color {
    if focused {
        palette::TEXT_EMPHASIS
    } else {
        palette::TEXT_SECONDARY
    }
}

pub(in crate::app::render) enum DisplayRow {
    Spacer,
    LetterHeader(String),
    /// One display row: the item indices occupying it, in column order. In
    /// one-column mode every such row carries exactly one index, so both
    /// modes share a single rendering path with no `cols == 1` branch.
    Item(Vec<usize>),
}

/// The shared selected-row replacement contract for a single-column browser.
/// Callers provide their already-built rows; this plan owns admission,
/// swallowing, flow scroll, fallback, geometry, targets, and marker policy.
pub(in crate::app::render) struct InlineReplacementPlan<'a> {
    display_rows: &'a [DisplayRow],
    selected_row: usize,
    selected_item: usize,
    detail_rows: u16,
    total_display_rows: usize,
    offset: usize,
    detail_screen_row: Option<usize>,
}

impl<'a> InlineReplacementPlan<'a> {
    /// Builds one replacement plan from the surface's display rows. A detail
    /// block is admitted only when `inline_detail_flow` can keep it and one
    /// ordinary browser row visible; otherwise the ordinary row flow wins.
    pub(in crate::app::render) fn new(
        display_rows: &'a [DisplayRow],
        selected_row: usize,
        selected_item: usize,
        desired_detail_rows: u16,
        visible_rows: u16,
        stored_offset: usize,
    ) -> Self {
        let admitted = (selected_row < display_rows.len()).then(|| {
            super::hero::inline_detail_flow(
                selected_row,
                desired_detail_rows,
                visible_rows,
                stored_offset,
            )
        });
        let (detail_rows, offset, detail_screen_row) = match admitted.flatten() {
            Some(flow) => {
                let mut offset = flow.offset;
                if matches!(
                    display_rows.get(offset.saturating_sub(1)),
                    Some(DisplayRow::LetterHeader(_))
                ) && offset > 0
                {
                    let header_offset = offset - 1;
                    let detail_end =
                        selected_row.saturating_sub(header_offset) + desired_detail_rows as usize;
                    if detail_end <= visible_rows as usize {
                        offset = header_offset;
                    }
                }
                (
                    desired_detail_rows,
                    offset,
                    Some(selected_row.saturating_sub(offset)),
                )
            }
            None => {
                if selected_row >= display_rows.len() {
                    let max_offset = display_rows.len().saturating_sub(visible_rows as usize);
                    let offset = stored_offset.min(max_offset);
                    return Self {
                        display_rows,
                        selected_row,
                        selected_item,
                        detail_rows: 0,
                        total_display_rows: display_rows.len(),
                        offset,
                        detail_screen_row: None,
                    };
                }
                let visible_rows = visible_rows as usize;
                let lower_bound = selected_row.saturating_sub(visible_rows.saturating_sub(1));
                let offset = stored_offset.clamp(lower_bound, selected_row);
                (0, offset, None)
            }
        };
        let total_display_rows =
            super::hero::inline_display_row_count(display_rows.len(), selected_row, detail_rows);
        Self {
            display_rows,
            selected_row,
            selected_item,
            detail_rows,
            total_display_rows,
            offset,
            detail_screen_row,
        }
    }

    pub(in crate::app::render) fn detail_rows(&self) -> u16 {
        self.detail_rows
    }

    pub(in crate::app::render) fn offset(&self) -> usize {
        self.offset
    }

    pub(in crate::app::render) fn total_display_rows(&self) -> usize {
        self.total_display_rows
    }

    pub(in crate::app::render) fn hero_area(&self, content_area: Rect) -> Option<Rect> {
        self.detail_screen_row.map(|screen_row| Rect {
            y: content_area.y + screen_row as u16,
            height: self.detail_rows,
            ..content_area
        })
    }

    pub(in crate::app::render) fn display_row(
        &self,
        display_row: usize,
    ) -> Option<super::hero::InlineDisplayRow> {
        if self.selected_row >= self.display_rows.len() {
            return (display_row < self.display_rows.len())
                .then_some(super::hero::InlineDisplayRow::Source(display_row));
        }
        super::hero::inline_display_row(
            self.display_rows.len(),
            self.selected_row,
            self.detail_rows,
            display_row,
        )
    }

    pub(in crate::app::render) fn row_targets(&self) -> Vec<Option<usize>> {
        (0..self.total_display_rows)
            .map(|display_row| match self.display_row(display_row) {
                Some(super::hero::InlineDisplayRow::Replacement) => {
                    (display_row == self.selected_row).then_some(self.selected_item)
                }
                Some(super::hero::InlineDisplayRow::Source(source_row)) => {
                    match &self.display_rows[source_row] {
                        DisplayRow::Item(items) => items.first().copied(),
                        DisplayRow::Spacer | DisplayRow::LetterHeader(_) => None,
                    }
                }
                None => None,
            })
            .collect()
    }

    pub(in crate::app::render) fn item_rows(&self) -> Vec<Vec<usize>> {
        (0..self.total_display_rows)
            .map(|display_row| match self.display_row(display_row) {
                Some(super::hero::InlineDisplayRow::Replacement) => {
                    if display_row == self.selected_row {
                        vec![self.selected_item]
                    } else {
                        Vec::new()
                    }
                }
                Some(super::hero::InlineDisplayRow::Source(source_row)) => {
                    match &self.display_rows[source_row] {
                        DisplayRow::Item(items) => items.clone(),
                        DisplayRow::Spacer | DisplayRow::LetterHeader(_) => Vec::new(),
                    }
                }
                None => Vec::new(),
            })
            .collect()
    }

    pub(in crate::app::render) fn should_draw_selection_markers(&self) -> bool {
        self.detail_rows == 0
    }
}

/// Shared inputs to the per-kind row-rendering bodies of `render_list`
/// (`render_letter_grouped_rows`, `media_list::render_plain_rows`): the
/// prelude values both kinds' bodies read, factored out so each callee takes
/// one struct instead of the same six-plus positional arguments.
pub(in crate::app::render) struct ListRenderCtx<'a> {
    /// The list's scrolling area. Narrow callers replace the active source row
    /// in this same flow.
    pub(in crate::app::render) content_area: Rect,
    pub(in crate::app::render) items: &'a [mbv_core::api::EmbyItem],
    pub(in crate::app::render) cursor: usize,
    pub(in crate::app::render) stored_scroll: usize,
    /// Column count for this frame's list pane width (1 or 2).
    pub(in crate::app::render) cols: usize,
    pub(in crate::app::render) focused: bool,
    pub(in crate::app::render) hero_rows: u16,
}

/// Owned browser-list inputs shared by narrow and wide renderers. The shell
/// builds this once from the active source; renderers no longer choose between
/// search results and the navigation level while painting rows.
#[derive(Clone)]
pub(in crate::app) struct LibraryListRenderCtx {
    pub(in crate::app) items: Vec<mbv_core::api::EmbyItem>,
    pub(in crate::app::render) cursor: usize,
    pub(in crate::app::render) scroll: usize,
    pub(in crate::app) total_count: usize,
    pub(in crate::app) library_total: Option<usize>,
    pub(in crate::app) letter_filter: Option<super::super::LetterFilter>,
    pub(in crate::app) loading: bool,
    pub(in crate::app) search_query: Option<String>,
    pub(in crate::app) search_loading: bool,
    /// The projected surface shows a feed/home-video group-pill row
    /// (`is_feed_home_video_group_view`; migrate-narrow-browse task 2.2). The
    /// focused `BrowserComponent`'s `[`/`]` chord then means group cycling
    /// (`BrowserCycleGroup`) rather than letter-pill cycling.
    pub(in crate::app) group_pills: bool,
}

impl LibraryListRenderCtx {
    pub(in crate::app) fn from_items(
        items: Vec<mbv_core::api::EmbyItem>,
        cursor: usize,
        scroll: usize,
    ) -> Self {
        let total_count = items.len();
        Self {
            items,
            cursor,
            scroll,
            total_count,
            library_total: None,
            letter_filter: None,
            loading: false,
            search_query: None,
            search_loading: false,
            group_pills: false,
        }
    }

    /// Marks this projection as a feed/home-video group picker (task 2.2).
    pub(in crate::app) fn with_group_pills(mut self, group_pills: bool) -> Self {
        self.group_pills = group_pills;
        self
    }

    pub(in crate::app) fn with_loading(mut self, loading: bool) -> Self {
        self.loading = loading;
        self
    }

    pub(in crate::app) fn with_cursor_scroll(mut self, cursor: usize, scroll: usize) -> Self {
        self.cursor = cursor;
        self.scroll = scroll;
        self
    }

    pub(in crate::app) fn with_search(mut self, query: String, loading: bool) -> Self {
        self.search_query = Some(query);
        self.search_loading = loading;
        self
    }

    pub(in crate::app) fn item_count(&self) -> usize {
        self.items.len()
    }

    pub(in crate::app) fn cursor(&self) -> usize {
        self.cursor
    }

    pub(in crate::app) fn scroll(&self) -> usize {
        self.scroll
    }

    pub(in crate::app) fn selected_item(&self) -> Option<&mbv_core::api::EmbyItem> {
        self.items.get(self.cursor)
    }

    pub(in crate::app::render) fn rows(
        &self,
        content_area: Rect,
        cols: usize,
        focused: bool,
        hero_rows: u16,
    ) -> ListRenderCtx<'_> {
        ListRenderCtx {
            content_area,
            items: &self.items,
            cursor: self.cursor,
            stored_scroll: self.scroll,
            cols,
            focused,
            hero_rows,
        }
    }

    pub(in crate::app) fn is_search_active(&self) -> bool {
        self.search_query.is_some()
    }

    pub(in crate::app) fn true_total(&self) -> usize {
        self.library_total.unwrap_or(self.total_count)
    }

    pub(in crate::app) fn has_letter_filter(&self) -> bool {
        self.letter_filter.is_some()
    }
}

/// Builds the title (+ optional duration) spans for one list row, shared by
/// both the letter-grouped and plain-list rendering branches (identical
/// styling logic, only how `title`/`dur_str`/`avail` are computed differs
/// between the two call sites). Every cell starts with a 1-column leading
/// space; the selected cell carries a `palette::SURFACE_RESTING`
/// background, in both one- and two-column mode. The marker glyph itself
/// (the shared `SelectionMarker` component) is drawn separately, at the
/// list's outer edge, by `draw_column_selection_markers`.
pub(in crate::app::render) fn build_list_row_spans(
    title: String,
    dur_str: String,
    selected: bool,
    fg: Color,
) -> Vec<Span<'static>> {
    let mut spans: Vec<Span> = if selected {
        let bg = palette::SURFACE_RESTING;
        let title_style = Style::default().fg(palette::TEXT_FOCUS_ACCENT).bg(bg);
        vec![
            Span::styled(" ", Style::default().bg(bg)),
            Span::styled(title, title_style),
        ]
    } else {
        vec![Span::raw(" "), Span::styled(title, Style::default().fg(fg))]
    };
    if !dur_str.is_empty() {
        let dur_style = if selected {
            Style::default()
                .fg(palette::TEXT_METADATA)
                .bg(palette::SURFACE_RESTING)
        } else {
            Style::default().fg(palette::TEXT_METADATA)
        };
        spans.push(Span::styled(dur_str, dur_style));
    }
    spans
}

/// Builds the padded spans for one item rendered into a `cell_width`-wide
/// cell: the existing marker/title/metadata/truncation logic operating
/// against the narrower cell width. Returns the cell's spans plus trailing
/// padding so the next cell starts at its own x offset; `pad_to` is the
/// total width to fill (cell width, plus the inter-column gap for every
/// cell except the last in its row).
pub(in crate::app::render) fn item_cell_spans(
    title: String,
    dur_str: String,
    selected: bool,
    fg: Color,
    pad_to: usize,
) -> Vec<Span<'static>> {
    let mut spans = build_list_row_spans(title, dur_str, selected, fg);
    let used: usize = spans.iter().map(|s| s.width()).sum();
    let pad = pad_to.saturating_sub(used);
    if pad > 0 {
        let pad_span = if selected {
            Span::styled(
                " ".repeat(pad),
                Style::default().bg(palette::SURFACE_RESTING),
            )
        } else {
            Span::raw(" ".repeat(pad))
        };
        spans.push(pad_span);
    }
    spans
}

/// Horizontal edge a `SelectionMarker` block sits at: the left edge for a
/// single-column list, or a two-column list's left column; the right edge
/// for a two-column list's right column.
pub(in crate::app::render) enum MarkerEdge {
    Left,
    Right,
}

/// The shared `SelectionMarker` component (design.md decision 2): a thin
/// `ACCENT`-role block, directional in two-column mode. `active` selects
/// the accent glyph vs. a blank column so unselected rows keep standard
/// alignment. Returns the styled span every list embeds as its marker;
/// `draw_column_selection_markers` uses the same glyph/color definition to
/// paint the library list's marker at the true outer edge, outside the
/// row's own content area.
pub(in crate::app::render) fn selection_marker(active: bool, edge: MarkerEdge) -> Span<'static> {
    if !active {
        return Span::raw(" ");
    }
    let glyph = match edge {
        MarkerEdge::Left => "\u{258e}",
        MarkerEdge::Right => "\u{1fb87}",
    };
    Span::styled(glyph, Style::default().fg(palette::ACCENT))
}

/// The screen rect of the selected cell in a column-aware list, derived from
/// the same `item_rows`/`row_offset` inputs `draw_column_selection_markers`
/// consumes plus the cell-width/column-gap geometry the renderer already
/// computed. Returns `None` when the cursor isn't on screen (e.g. it sits in
/// a filtered-out bucket).
pub(in crate::app::render) fn selected_cell_rect(
    content_area: Rect,
    cursor: usize,
    item_rows: &[Vec<usize>],
    row_offset: usize,
    cols: usize,
    cell_width: u16,
    column_gap: u16,
) -> Option<Rect> {
    let cursor_row = item_rows.iter().position(|row| row.contains(&cursor))?;
    let row_idx = cursor_row.checked_sub(row_offset)?;
    let col_in_row = item_rows[cursor_row]
        .iter()
        .position(|&idx| idx == cursor)
        .unwrap_or(0);
    let col = col_in_row.min(cols.saturating_sub(1));
    let cell_x = content_area
        .x
        .saturating_add(col as u16 * cell_width.saturating_add(column_gap));
    Some(Rect {
        x: cell_x,
        y: content_area.y + row_idx as u16,
        width: cell_width,
        height: 1,
    })
}

/// Draws the library list's column selection marker after the list has
/// rendered, at the panel's outer edge: the left edge in single-column
/// mode or for a left-column selection, the right edge for a right-column
/// selection (symmetric). The background is extended to cover the gap
/// between the marker and the cell content.
pub(in crate::app::render) fn draw_column_selection_markers(
    f: &mut Frame,
    content_area: Rect,
    cursor: usize,
    item_rows: &[Vec<usize>],
    row_offset: usize,
) {
    draw_column_selection_markers_with_background(
        f,
        content_area,
        cursor,
        item_rows,
        row_offset,
        palette::SURFACE_RESTING,
    );
}

/// Draws selection markers with the selected row's surface. Most catalog
/// lists use the resting selected-row surface; Wide hero Feeds rows use
/// their focus-resolved surface instead.
pub(in crate::app::render) fn draw_column_selection_markers_with_background(
    f: &mut Frame,
    content_area: Rect,
    cursor: usize,
    item_rows: &[Vec<usize>],
    row_offset: usize,
    background: Color,
) {
    let Some(cursor_row) = item_rows.iter().position(|row| row.contains(&cursor)) else {
        return;
    };
    let Some(row_idx) = cursor_row.checked_sub(row_offset) else {
        return;
    };
    let col_in_row = item_rows[cursor_row]
        .iter()
        .position(|&idx| idx == cursor)
        .unwrap_or(0);

    let row_y = content_area.y + row_idx as u16;

    if col_in_row == 0 {
        f.render_widget(
            Block::default().style(Style::default().bg(background)),
            Rect {
                x: content_area.x.saturating_sub(2),
                y: row_y,
                width: 2,
                height: 1,
            },
        );
        f.render_widget(
            Paragraph::new(Line::from(selection_marker(true, MarkerEdge::Left))),
            Rect {
                x: content_area.x.saturating_sub(2),
                y: row_y,
                width: 1,
                height: 1,
            },
        );
    } else {
        f.render_widget(
            Block::default().style(Style::default().bg(background)),
            Rect {
                x: content_area.x + content_area.width,
                y: row_y,
                width: 2,
                height: 1,
            },
        );
        f.render_widget(
            Paragraph::new(Line::from(selection_marker(true, MarkerEdge::Right))),
            Rect {
                x: content_area.x + content_area.width + 1,
                y: row_y,
                width: 1,
                height: 1,
            },
        );
    }
}

#[cfg(test)]
mod selection_marker_tests {
    use super::*;

    // Replaces `home_latest_row.rs`'s deleted `row_unselected_has_no_marker`
    // (design.md decision 2 centralized every list's marker onto this one
    // component, so the "unselected rows carry no marker glyph" guarantee
    // belongs here now, not in a per-screen row painter).
    #[test]
    fn inactive_marker_is_blank() {
        for edge in [MarkerEdge::Left, MarkerEdge::Right] {
            let span = selection_marker(false, edge);
            assert_eq!(span.content.as_ref(), " ");
            assert_eq!(span.style.fg, None);
        }
    }

    #[test]
    fn active_marker_uses_directional_glyph() {
        assert_eq!(
            selection_marker(true, MarkerEdge::Left).content.as_ref(),
            "\u{258e}"
        );
        assert_eq!(
            selection_marker(true, MarkerEdge::Right).content.as_ref(),
            "\u{1fb87}"
        );
    }
}
