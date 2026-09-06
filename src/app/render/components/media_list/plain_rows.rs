use crate::app::layout::LayoutMain;
use crate::app::library_column_width::{library_cell_width, LIBRARY_COLUMN_GAP};
use crate::app::palette;
use crate::app::render::components::hero::InlineDisplayRow;
use crate::app::render::components::list_rows::{
    focused_or_subtle, item_cell_spans, selected_cell_rect, DisplayRow, InlineReplacementPlan,
    ListRenderCtx,
};
use crate::app::ui_util::*;
use ratatui::style::*;
use ratatui::text::*;
use ratatui::widgets::*;
use ratatui::Frame;
use unicode_width::UnicodeWidthStr;

/// Canonical fixed-row render path for the canonical media-list controls.
/// Semantic theme ownership stays in `theme`/`palette` and `list_rows`.
///
/// Plain list kind of `render_list`: the catch-all covering the Home
/// "Continue Watching" tab, search result sets, small libraries, and
/// non-album music levels, all of which render identically without letter
/// grouping. Returns the scroll offset to persist.
pub(in crate::app) fn render_plain_rows(
    f: &mut Frame,
    ctx: ListRenderCtx,
    layout: &mut LayoutMain,
) -> usize {
    #[cfg(test)]
    super::PLAIN_ROWS_PAINTS.with(|count| count.set(count.get() + 1));
    let ListRenderCtx {
        content_area,
        items,
        cursor,
        stored_scroll,
        cols,
        focused,
        hero_rows,
    } = ctx;
    let n = items.len();
    // Publish the identity display order just like letter-grouped lists publish
    // their sorted order; the fresh frame layout makes this authoritative for
    // cursor and hit testing even when the list has no grouping.
    layout.left_sorted_indices = (0..n).collect();
    let visible = content_area.height as usize;
    let cell_w = library_cell_width(content_area, cols) as usize;

    // Build display rows row-major: item `i` occupies column `i % cols`
    // of row `i / cols`. In one-column mode every row carries exactly
    // one index, so both modes share this single path.
    let display_rows: Vec<DisplayRow> = (0..n)
        .collect::<Vec<_>>()
        .chunks(cols.max(1))
        .map(|item_row| DisplayRow::Item(item_row.to_vec()))
        .collect();
    // `display_cursor` is the index of the *row containing* the cursor
    // item, so the scroll clamp keeps the cursor row on screen.
    let display_cursor = display_rows
        .iter()
        .position(|r| matches!(r, DisplayRow::Item(idxs) if idxs.contains(&cursor)))
        .unwrap_or(0);
    let plan = InlineReplacementPlan::new(
        &display_rows,
        display_cursor,
        cursor,
        hero_rows,
        content_area.height,
        stored_scroll,
    );
    let offset = plan.offset();
    let detail_rows = plan.detail_rows();
    let total_display = plan.total_display_rows();
    let final_offset = offset;

    let show_scrollbar = focused && total_display > visible;

    let list_items: Vec<ListItem> = (offset..total_display)
        .take(visible)
        .map(|display_row| {
            match plan
                .display_row(display_row)
                .expect("display row is within the replacement flow")
            {
                InlineDisplayRow::Replacement => ListItem::new(Line::default()),
                InlineDisplayRow::Source(source_row) => match &display_rows[source_row] {
                    DisplayRow::Spacer | DisplayRow::LetterHeader(_) => {
                        ListItem::new(Line::default())
                    }
                    DisplayRow::Item(idxs) => {
                        // Each item renders into its own cell, truncated to the
                        // cell width; cells are padded to the cell boundary
                        // (+ inter-column gap) so the next cell starts at its
                        // own x offset. Trailing partial rows leave the empty
                        // cells as plain list background.
                        let mut spans: Vec<Span> = Vec::new();
                        for (cell_idx, &idx) in idxs.iter().enumerate() {
                            let item = &items[idx];
                            let selected = idx == cursor;

                            // Compute name and duration as separate strings so they can be styled
                            // independently: name in the normal fg, duration in OVERLAY (no parens).
                            let (item_name, dur_str) = if item.is_folder {
                                let name = if item.item_type == "Folder" && item.total_count > 0 {
                                    format!(
                                        "{} \u{b7} {} items",
                                        item.display_name(),
                                        item.total_count
                                    )
                                } else if item.unplayed_item_count > 0 && item.item_type != "Series"
                                {
                                    format!(
                                        "{} [{}]",
                                        item.display_name(),
                                        item.unplayed_item_count
                                    )
                                } else {
                                    item.display_name()
                                };
                                (name, String::new())
                            } else {
                                let year = if item.production_year > 0 {
                                    format!(" {}", item.production_year)
                                } else {
                                    String::new()
                                };
                                (item.display_name(), year)
                            };

                            // Every cell starts with a 1-column leading
                            // separator (the selected cell's highlight
                            // background, a plain space otherwise), so titles
                            // align across rows.
                            let avail = cell_w.saturating_sub(2);
                            let name_w = avail.saturating_sub(dur_str.width());
                            let (title, dur_str) = if selected && detail_rows > 0 {
                                (String::new(), String::new())
                            } else {
                                (trunc_str(&item_name, name_w), dur_str)
                            };
                            let fg = focused_or_subtle(focused);

                            let pad_to = if cell_idx + 1 == idxs.len() {
                                cell_w
                            } else {
                                cell_w + LIBRARY_COLUMN_GAP as usize
                            };
                            spans.extend(item_cell_spans(title, dur_str, selected, fg, pad_to));
                        }
                        ListItem::new(Line::from(spans))
                    }
                },
            }
        })
        .collect();

    let row_targets = plan.row_targets();
    layout.left_row_map = (offset..total_display)
        .take(visible)
        .enumerate()
        .map(|(visible_row, _)| row_targets[offset + visible_row])
        .collect();
    // Publish the full row structure (parallel to the display rows,
    // empty entries for headers) so column-aware cursor movement and
    // mouse hit-testing can resolve cells between frames.
    layout.left_item_rows = plan.item_rows();

    if let Some(hero_area) = plan.hero_area(content_area) {
        layout.hero_area = hero_area;
        layout.inline_hero_area = layout.hero_area;
    }

    let mut state = ListState::default();
    state.select(Some(display_cursor.saturating_sub(offset)));
    layout.selected_item_rect = selected_cell_rect(
        content_area,
        cursor,
        &layout.left_item_rows,
        offset,
        cols,
        cell_w as u16,
        LIBRARY_COLUMN_GAP,
    );
    if detail_rows > 0 {
        layout.selected_item_rect = Some(layout.hero_area);
    }
    f.render_stateful_widget(
        List::new(list_items).highlight_style(Style::default()),
        content_area,
        &mut state,
    );

    if show_scrollbar {
        let max_off = total_display.saturating_sub(visible);
        crate::app::render::render_right_scrollbar(
            f,
            content_area,
            max_off,
            offset,
            palette::SCROLLBAR,
        );
    }

    if plan.should_draw_selection_markers() {
        crate::app::render::components::list_rows::draw_column_selection_markers(
            f,
            content_area,
            cursor,
            &layout.left_item_rows,
            offset,
        );
    }

    final_offset
}
