use super::chrome::thin_vertical_thumb;
use crate::app::layout::LayoutMain;
use crate::app::{palette, App, TabSelection};
use ratatui::layout::Rect;
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Paragraph};
use ratatui::Frame;
use tui_scrollbar::{GlyphSet, ScrollBar, ScrollLengths};
use unicode_width::UnicodeWidthStr;

// The main UI re-renders frequently while scrolling; prefer a cheaper filter in
// these hot paths to reduce terminal image preparation stalls.
pub(in crate::app) const RENDER_FILTER: ratatui_image::FilterType =
    ratatui_image::FilterType::Triangle;

// Configured music albums need the image worker's child-audio lookup; their
// album containers do not reliably expose usable Primary images.
pub(in crate::app) const MUSIC_ALBUM_IMAGE_TYPES: &[&str] = &["AudioChild"];

/// Columns of empty space between the left and right panels.
pub(in crate::app) const COLUMN_GAP: u16 = 0;

/// Left-edge padding applied once to every tab's content area
/// (Home, library lists, music groups, albums, series, home-video, feed
/// groups) plus the music-group pills row, so all tabs share a consistent
/// gutter. Applied at the single dispatch chokepoint in the main render
/// fn; individual tab renderers add only their own content-level gutters
/// (marker columns, banner indents) relative to this padded edge.
///
/// Detail surfaces that need additional internal alignment can add their own
/// indentation relative to this padded edge.
pub(in crate::app) const TAB_LEFT_PAD: u16 = 2;

pub(in crate::app) fn right_panel_content_area(area: Rect, left_collapsed: bool) -> Rect {
    if left_collapsed {
        Rect {
            x: area.x + 1,
            width: area.width.saturating_sub(2),
            ..area
        }
    } else {
        Rect {
            x: area.x + TAB_LEFT_PAD,
            width: area.width.saturating_sub(TAB_LEFT_PAD.saturating_mul(2)),
            ..area
        }
    }
}

/// The single scrollbar entry point: takes a role (`color`), never
/// hardcodes one. Positions the thumb at the area's own right edge if the
/// area already reaches the frame's right edge, otherwise just outside the
/// area (so a scrollbar never overlaps a panel's own content column).
pub(in crate::app) fn render_right_scrollbar(
    f: &mut Frame,
    area: Rect,
    max_offset: usize,
    offset: usize,
    color: Color,
) {
    let visible = area.height as usize;
    render_right_scrollbar_with_viewport(
        f,
        area,
        max_offset.saturating_add(visible),
        visible,
        offset,
        color,
    );
}

pub(in crate::app) fn render_right_scrollbar_with_viewport(
    f: &mut Frame,
    area: Rect,
    content_length: usize,
    viewport_content_length: usize,
    offset: usize,
    color: Color,
) {
    let x = if area.right() < f.area().right() {
        area.right()
    } else {
        area.x + area.width.saturating_sub(1)
    };
    render_scrollbar_with_viewport_at(
        f,
        area,
        content_length,
        viewport_content_length,
        offset,
        x,
        thin_vertical_thumb(GlyphSet::minimal()),
        color,
    );
}

pub(in crate::app) fn render_scrollbar_with_viewport_at(
    f: &mut Frame,
    area: Rect,
    content_length: usize,
    viewport_content_length: usize,
    offset: usize,
    x: u16,
    glyph_set: GlyphSet,
    scrollbar_color: Color,
) {
    if area.height == 0 || viewport_content_length == 0 || content_length <= viewport_content_length
    {
        return;
    }
    let max_offset = content_length.saturating_sub(viewport_content_length);
    let scrollbar = ScrollBar::vertical(ScrollLengths {
        content_len: content_length,
        viewport_len: viewport_content_length,
    })
    .offset(offset.min(max_offset))
    .glyph_set(glyph_set)
    .track_style(Style::default().fg(scrollbar_color))
    .thumb_style(Style::default().fg(scrollbar_color));
    f.render_widget(
        &scrollbar,
        Rect {
            x,
            width: 1,
            ..area
        },
    );
}

/// Paints a colored background block spanning display rows `[top_pad_abs, bottom_pad_abs]`
/// (absolute/unscrolled indices into the complete display row sequence), clamped to the
/// visible scroll window `[offset, offset+visible)`. The block fills the full row width
/// supplied by `area.x` and `area.width` (interior content can indent itself further).
/// Call before rendering list/row content so the background shows through.
pub(in crate::app) fn render_selected_block_background(
    f: &mut Frame,
    area: Rect,
    offset: usize,
    visible: usize,
    top_pad_abs: usize,
    bottom_pad_abs: usize,
    bg: Color,
) {
    let vis_top = top_pad_abs.max(offset);
    let vis_bot = bottom_pad_abs.min(offset + visible.saturating_sub(1));
    if vis_top <= vis_bot {
        let block_y = area.y + (vis_top - offset) as u16;
        let block_h = (vis_bot - vis_top + 1) as u16;
        f.render_widget(
            Block::default().style(Style::default().bg(bg)),
            Rect {
                x: area.x,
                y: block_y,
                width: area.width,
                height: block_h,
            },
        );
    }
}

/// The declared framing variants for [`render_selected_block_borders`].
/// Differences stay in one painter rather than being forked by callers.
pub(in crate::app) enum SelectedBlockBorderStyle {
    Framed,
    FocusedRail { focused: bool },
}

/// Paints the ▁/▔ border rows on the reserved rows one position outside
/// the colored block's padding rows `[top_pad_abs, bottom_pad_abs]`.
/// The padding rows are inserted with extra detail rule rows for border space.
/// Call *after* the block's own content and scrollbar render, so borders paint on top.
pub(in crate::app) fn render_selected_block_borders(
    f: &mut Frame,
    area: Rect,
    offset: usize,
    visible: usize,
    top_pad_abs: usize,
    bottom_pad_abs: usize,
    style: SelectedBlockBorderStyle,
) {
    let (top_glyph, bottom_glyph, bg) = match style {
        SelectedBlockBorderStyle::Framed => ("\u{2581}", "\u{2594}", None),
        SelectedBlockBorderStyle::FocusedRail { focused } => (
            "\u{2594}",
            "\u{2581}",
            Some(palette::resolve_surface_focus(focused)),
        ),
    };
    let mut border_style = Style::default().fg(palette::PROGRESS_TRACK);
    if let Some(bg) = bg {
        border_style = border_style.bg(bg);
    }
    // Top border: paint one row before the colored block padding
    if let Some(top_border) = top_pad_abs.checked_sub(1) {
        if top_border >= offset && top_border < offset + visible {
            let top_y = area.y + (top_border - offset) as u16;
            f.render_widget(
                Paragraph::new(Line::from(Span::styled(
                    top_glyph.repeat(area.width as usize),
                    border_style,
                ))),
                Rect {
                    x: area.x,
                    y: top_y,
                    width: area.width,
                    height: 1,
                },
            );
        }
    }
    // Bottom border: paint one row after the colored block padding
    let bot_border = bottom_pad_abs + 1;
    if bot_border >= offset && bot_border < offset + visible {
        let bot_y = area.y + (bot_border - offset) as u16;
        f.render_widget(
            Paragraph::new(Line::from(Span::styled(
                bottom_glyph.repeat(area.width as usize),
                border_style,
            ))),
            Rect {
                x: area.x,
                y: bot_y,
                width: area.width,
                height: 1,
            },
        );
    }
}

pub(in crate::app) fn render_queue_panel_frame(f: &mut Frame, area: Rect, focused: bool) -> Rect {
    if area.width == 0 || area.height == 0 {
        return Rect::default();
    }

    let bg = if focused {
        palette::SURFACE_ACCENT_SOFT
    } else {
        palette::SURFACE_BACKDROP
    };
    f.render_widget(Block::default().style(Style::default().bg(bg)), area);

    area
}

/// Style for a pill-selector choice: white text on the green selected
/// surface, muted text on the dark unselected surface. This is the canonical
/// appearance for every interactive pill selector (Home sections, feed
/// groups, music groups, letter filters, and series seasons).
fn selector_pill_style(selected: bool) -> Style {
    if selected {
        Style::default()
            .fg(palette::PILL_SELECTED_FG)
            .bg(palette::PILL_SELECTED_BG)
    } else {
        Style::default().fg(palette::PILL_FG).bg(palette::PILL_BG)
    }
}

/// Draws the shared " {count} items" header (SUBTLE) on the first row of
/// `area` and returns `area` shrunk by that one row, so callers can render
/// their list into the remaining space. Used by the home-video tab to keep
/// the label styling and the one-row consumption identical to other tabs
/// that once shared it (movies/tv show library lists no longer show this
/// row; see `render_list`).
pub(in crate::app) fn render_count_label(f: &mut Frame, area: Rect, count: usize) -> Rect {
    if area.width == 0 || area.height == 0 {
        return area;
    }
    f.render_widget(
        Paragraph::new(Span::styled(
            format!(" {} items", count),
            Style::default().fg(palette::TEXT_SECONDARY),
        )),
        Rect { height: 1, ..area },
    );
    Rect {
        y: area.y + 1,
        height: area.height.saturating_sub(1),
        ..area
    }
}

/// A horizontally-scrolling row of selector pills, shared by every
/// pill selector (Home sections, feed groups, music groups, letter
/// filters, and series seasons) so their appearance,
/// scroll/overflow/selection behavior can't drift apart. Callers
/// pre-truncate `labels`, supply the parallel `ids` recorded as click
/// targets, mark which position is `selected_pos`, and may pass an
/// optional leading `prefix` inset (rendered without the pill shell; it
/// does not alter the pill visual).
pub(in crate::app) struct PillBar<'a> {
    pub labels: &'a [String],
    pub ids: &'a [usize],
    pub selected_pos: usize,
    pub prefix: Option<&'a str>,
}

/// Renders `bar` into `area`, painting the canonical pill-selector row
/// background, drawing joined angled pills with the selected choice kept on
/// screen (with `‹`/`›` chevrons when the pills overflow), and returning the
/// on-screen pill hitboxes as `(rect, id)` pairs for `layout.selector_tabs`.
/// This is the sole renderer for interactive pill selectors; callers do not
/// select appearance variants.
pub(in crate::app) fn render_pill_bar(
    f: &mut Frame,
    area: Rect,
    bar: PillBar,
) -> Vec<(Rect, usize)> {
    // `ids` runs parallel to `labels`; a mismatch would panic on the slice
    // below, so assert the contract up front rather than fail cryptically.
    debug_assert_eq!(
        bar.labels.len(),
        bar.ids.len(),
        "render_pill_bar: labels and ids must be parallel"
    );
    let mut selector_tabs: Vec<(Rect, usize)> = Vec::new();
    if area.width == 0 || area.height == 0 || bar.labels.is_empty() {
        return selector_tabs;
    }
    let area = Rect { height: 1, ..area };
    let n = bar.labels.len();
    let bar_w = area.width as usize;
    let prefix_w = bar.prefix.map(|p| p.width()).unwrap_or(0);
    // Display width of each joined pill is "◢ label ◤" = label width + inner
    // padding (2) + leading/trailing edge glyphs (2).
    let pill_widths: Vec<usize> = bar.labels.iter().map(|l| l.width() + 4).collect();

    // Greedy: how many pills fit starting at `start` within `avail` columns.
    let count_fitting = |start: usize, avail: usize| -> usize {
        let mut used = 0usize;
        let mut count = 0usize;
        for width in pill_widths.iter().skip(start) {
            if used + *width > avail {
                break;
            }
            used += *width;
            count += 1;
        }
        count
    };

    // Advance the scroll window until the selected pill is visible.
    let mut scroll_start = 0usize;
    loop {
        let avail = bar_w
            .saturating_sub(prefix_w)
            .saturating_sub(if scroll_start > 0 { 2 } else { 0 }) // "‹ "
            .saturating_sub(2); // reserve for " ›"
        let cnt = count_fitting(scroll_start, avail);
        if cnt == 0 || scroll_start + cnt > bar.selected_pos {
            break;
        }
        scroll_start += 1;
    }

    let has_left = scroll_start > 0;
    let avail_pills = bar_w
        .saturating_sub(prefix_w)
        .saturating_sub(if has_left { 2 } else { 0 })
        .saturating_sub(2); // reserve for " ›"
    let cnt = count_fitting(scroll_start, avail_pills);
    let scroll_end = (scroll_start + cnt).min(n);
    let has_right = scroll_end < n;

    // The row surface is part of the canonical shell.
    f.render_widget(
        Block::default().style(Style::default().bg(palette::PILL_ROW_BG)),
        area,
    );

    let mut spans: Vec<Span> = Vec::new();
    let mut x_cursor = area.x;
    if let Some(prefix) = bar.prefix {
        if prefix == "  " {
            spans.push(Span::styled(
                "  ",
                Style::default()
                    .fg(palette::STATUS_AVAILABLE)
                    .bg(palette::PILL_ROW_BG),
            ));
        } else {
            spans.push(Span::styled(
                prefix.to_string(),
                Style::default().fg(palette::TEXT_METADATA),
            ));
        }
        x_cursor += prefix_w as u16;
    }
    if has_left {
        let chunk = "\u{2039} ";
        spans.push(Span::styled(
            chunk,
            Style::default().fg(palette::PILL_OVERFLOW_FG),
        ));
        x_cursor += chunk.width() as u16;
    }
    for (offset, (label, &id)) in bar.labels[scroll_start..scroll_end]
        .iter()
        .zip(bar.ids[scroll_start..scroll_end].iter())
        .enumerate()
    {
        let abs_idx = scroll_start + offset;
        let selected = abs_idx == bar.selected_pos;
        let is_last_pill = abs_idx + 1 == n;
        let style = selector_pill_style(selected);
        let pill = format!(" {} ", label);
        let marker_w = "◢◤".width() as u16;
        let pill_w = pill.width() as u16 + marker_w;
        selector_tabs.push((
            Rect {
                x: x_cursor,
                y: area.y,
                width: pill_w,
                height: 1,
            },
            id,
        ));
        spans.push(Span::styled(
            "◢",
            Style::default()
                .fg(if selected {
                    palette::PILL_SELECTED_BG
                } else {
                    palette::PILL_BG
                })
                .bg(if abs_idx == 0 {
                    palette::PILL_ROW_BG
                } else {
                    palette::PILL_BG
                }),
        ));
        spans.push(Span::styled(pill, style));
        spans.push(Span::styled(
            "◤",
            Style::default()
                .fg(if selected {
                    palette::PILL_SELECTED_BG
                } else {
                    palette::PILL_BG
                })
                .bg(if is_last_pill {
                    palette::PILL_ROW_BG
                } else {
                    palette::PILL_BG
                }),
        ));
        x_cursor += pill_w;
    }
    if has_right {
        let chunk = " \u{203a}";
        spans.push(Span::styled(
            chunk,
            Style::default().fg(palette::PILL_OVERFLOW_FG),
        ));
        x_cursor += chunk.width() as u16;
    }

    // Clear the rest of the row with the canonical row background so the
    // surface is continuous across the panel.
    let used_w = x_cursor.saturating_sub(area.x) as usize;
    let remaining = bar_w.saturating_sub(used_w);
    if remaining > 0 {
        spans.push(Span::styled(
            " ".repeat(remaining),
            Style::default().bg(palette::PILL_ROW_BG),
        ));
    }

    f.render_widget(Paragraph::new(Line::from(spans)), area);
    selector_tabs
}

/// Draws a shared empty/loading placeholder message (MUTED) at `area`.
/// Callers pass the exact text (`" (empty)"`, `" Loading…"`, or a
/// context-specific string like `"Indexing music library..."`) so the
/// wording stays local, but the placeholder styling is defined once.
pub(in crate::app) fn render_placeholder(f: &mut Frame, area: Rect, msg: &str) {
    if area.width == 0 || area.height == 0 {
        return;
    }
    f.render_widget(
        Paragraph::new(Span::styled(
            msg.to_string(),
            Style::default().fg(palette::TEXT_MUTED),
        )),
        area,
    );
}

impl App {
    pub(in crate::app) fn render_library(
        &mut self,
        _f: &mut Frame,
        area: Rect,
        layout: &mut LayoutMain,
        cursor_scroll: Option<(usize, usize)>,
    ) {
        // If a music-group library's nav_stack was truncated to just the group
        // level (e.g., stale breadcrumb click), immediately re-push the album level.
        // (Emby-only; done inside the Emby match arm below.)
        // Exhaustive destination dispatch: each Service renders only its own
        // view; there is no default-to-Emby branch. The selected destination
        // was already normalized to a live index by `render_main`.
        match self.tab {
            TabSelection::Home => {
                // Home content is painted by the mounted `HomeComponent`
                // (the shell paints it right after this legacy base frame,
                // reading `home_area` to size it). The legacy frame only
                // reserves the full Home destination area here — it paints
                // no Home rows, pills, hero, or image (task 5.3d, Home
                // legacy underpaint removal).
                layout.home_area = area;
            }
            TabSelection::Feeds => {
                layout.feeds_area = area;
            }
            TabSelection::AudiobookshelfLibrary(_) => {
                // The Book surface is painted by the mounted
                // `AudiobookshelfBookComponent` (task 5.3d.13) and the Podcast
                // surface by the mounted `AudiobookshelfPodcastComponent` (task
                // 5.3d.10, Unit E); the legacy App renderers were removed. This
                // arm only reserves the destination content area the shell
                // reads to place those component overlays.
                let is_book = self.tab.audiobookshelf_index().is_some_and(|index| {
                    matches!(
                        self.audiobookshelf_kind_at(index),
                        Some(
                            crate::app::types_audiobookshelf_browse::AudiobookshelfBrowseKind::Book
                        )
                    )
                });
                // `from_media_type` maps every ABS media type to exactly one of
                // Book | Podcast, so the non-book arm *is* the podcast surface;
                // a kind guard here would be unreachable branch weight.
                if is_book {
                    layout.audiobookshelf_book_area = area;
                } else {
                    layout.audiobookshelf_podcast_area = area;
                }
            }
            TabSelection::EmbyLibrary(lib_idx) => {
                self.ensure_music_group_album_level(lib_idx);
                self.ensure_feed_home_video_group_level(lib_idx);
                if self.is_feed_home_video_group_view(lib_idx) {
                    // BrowserComponent owns feed group presentation at every
                    // width; publish only the full browser area.
                    layout.left_area = area;
                    return;
                }
                {
                    // Music's mounted workspace needs the same-frame geometry
                    // before its view replaces this legacy frame.
                    if self.is_music_group_view(lib_idx)
                        && self.is_viewing_album_folders(lib_idx)
                        && crate::app::render::arrangements::wide_hero::wide_hero_presentation(area)
                            .is_some()
                    {
                        let ctx = self.wide_music_render_ctx(lib_idx, cursor_scroll);
                        ctx.publish_geometry(area, layout);
                    }
                    // Wide TV's mounted `TvWorkspaceComponent` paints the
                    // whole Wide hero workspace itself (task 5.3d.18d);
                    // the legacy wide-TV branch is gone. We only publish the
                    // hand-off `tv_wide_*` rects here before `render_list` so
                    // input routing (`App::wide_tv_library_area`) and the
                    // shell's render seam can locate them.
                    if self.is_wide_tv_library(lib_idx)
                        && crate::app::render::arrangements::wide_hero::wide_hero_presentation(area)
                            .is_some()
                    {
                        let ctx = self.wide_tv_render_ctx(lib_idx, cursor_scroll);
                        ctx.publish_geometry(area, layout);
                    }
                    // BrowserComponent owns the browse body at every width;
                    // reserve only the destination area here.
                    layout.left_area = area;
                }
            }
        }
    }

    /// Resolves the display artist for an album item in the grouped music
    /// views, synchronously (never schedules artist lookups). Priority
    /// order:
    /// 1. `item.artist` (Emby's Album-entity metadata) if non-empty.
    /// 2. `album_artist_cache` entry if non-empty (fetched from the album's
    ///    first few tracks — see `fetch_album_artist` in `images.rs`).
    /// 3. `parse_album_folder_name` heuristic.
    /// 4. Literal "Unknown Artist".
    pub(in crate::app) fn resolve_group_album_artist(
        &self,
        item: &mbv_core::api::EmbyItem,
    ) -> String {
        crate::app::music_grouping::derive_album_artist(
            item,
            self.album_artist_cache.get(&item.id).map(String::as_str),
        )
    }
}
