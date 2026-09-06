use crate::app::components::inline_search::InlineSearch;
use crate::app::components::media_list::WideMediaList;
use crate::app::layout::LayoutMain;
use crate::app::render::arrangements::library as library_arrangement;
use crate::app::render::arrangements::padded_rect;
use crate::app::render::arrangements::wide_hero::{
    self, place_media_list_below, PANE_PAD_X, PANE_PAD_Y,
};
use crate::app::render::components::hero::{wrap_overview_lines, HeroContent};
use crate::app::render::components::hero_model::{Hero, HeroArtwork, HeroArtworkAspect};
use crate::app::render::components::list_rows::LibraryListRenderCtx;
use crate::app::render::HomeImagePaint;
use crate::app::render::{render_pill_bar, render_placeholder, PillBar};
use crate::app::{palette, App, PanelMode, SeriesDetail};
use mbv_core::api::EmbyItem;
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::text::Span;
use ratatui::widgets::{Block, Paragraph, Wrap};
use ratatui::Frame;

/// Minimum visible-row floor for the embedded episode `WideMediaList` box
/// (task 4.2d): a season pill row plus at least this many episode rows, inset
/// by the same recessed-box padding the overview box uses. The box grows past
/// this to fit every episode; `place_media_list_below` then clamps it to the
/// pane's bottom edge, and the canonical control scrolls whatever still
/// overflows.
const EPISODE_LIST_VISIBLE_ROWS: u16 = 6;

/// Blank rows between the overview box and the episode media-list box below
/// it (the same one-row gap convention as task 4.2b's stacked-artwork/title
/// gap).
const MEDIA_LIST_GAP_ROWS: u16 = 1;

/// All App-derived data needed to paint the wide TV workspace.
#[derive(Clone)]
pub(in crate::app) struct TvWideRenderCtx {
    pub(in crate::app) list: LibraryListRenderCtx,
    pub(in crate::app) selected_series: Option<EmbyItem>,
    pub(in crate::app) series_detail: Option<SeriesDetail>,
    pub(in crate::app) season_cursor: usize,
    pub(in crate::app) episode_cursor: Option<usize>,
    pub(in crate::app) focused: bool,
    pub(in crate::app) show_letter_pills: bool,
    pub(in crate::app) images_enabled: bool,
    pub(in crate::app) image_loading: bool,
}

impl TvWideRenderCtx {
    pub(in crate::app) fn new(
        list: LibraryListRenderCtx,
        selected_series: Option<EmbyItem>,
        series_detail: Option<SeriesDetail>,
        season_cursor: usize,
        episode_cursor: Option<usize>,
        show_letter_pills: bool,
    ) -> Self {
        Self {
            list,
            selected_series,
            series_detail,
            season_cursor,
            episode_cursor,
            // Framework focus is owned by `TvWorkspaceComponent` and applied
            // from `Attribute::Focus`; content projection never sets it.
            focused: false,
            show_letter_pills,
            images_enabled: true,
            image_loading: true,
        }
    }

    pub(in crate::app) fn with_image_state(
        mut self,
        images_enabled: bool,
        image_loading: bool,
    ) -> Self {
        self.images_enabled = images_enabled;
        self.image_loading = image_loading;
        self
    }

    pub(in crate::app) fn with_local_state(
        mut self,
        cursor: usize,
        scroll: usize,
        season_cursor: usize,
        episode_cursor: Option<usize>,
    ) -> Self {
        self.list = self.list.with_cursor_scroll(cursor, scroll);
        self.season_cursor = season_cursor;
        self.episode_cursor = episode_cursor;
        self
    }

    /// Publish the `tv_wide_*` layout geometry the mounted
    /// `TvWorkspaceComponent` hit-tests (task 5.3d.18d). The legacy
    /// `render_list` wide-TV underpaint is gone; the App frame now only
    /// publishes the hand-off rects before `render_list` runs so input
    /// routing (`App::wide_tv_library_area`) and the shell's render seam
    /// stay correct while the component owns the picture.
    pub(in crate::app) fn publish_geometry(&self, area: Rect, layout: &mut LayoutMain) {
        layout.tv_wide_area = area;
        let Some(panes) = library_arrangement::wide_library_panes(area, PANE_PAD_X, PANE_PAD_Y)
        else {
            return;
        };
        layout.tv_wide_left_area = panes.left_area;
        layout.tv_wide_right_area = panes.right_area;
        layout.left_area = Rect::default();
        let right_pane = wide_hero::wide_hero_browser_pane(panes.right_panel, panes.right_area);
        layout.tv_wide_list_area = padded_rect(right_pane.list_panel, PANE_PAD_X, PANE_PAD_Y);
    }
}

impl App {
    pub(in crate::app::render) fn is_wide_tv_library(&self, lib_idx: usize) -> bool {
        self.libs.get(lib_idx).is_some_and(|lib| {
            lib.library.collection_type == "tvshows"
                && lib.nav_stack.last().is_some_and(|level| {
                    level.items.is_empty()
                        || level.items.iter().all(|item| item.item_type == "Series")
                })
        })
    }

    /// The right panel's content area for the current terminal size and
    /// panel state, paint-free — `None` when the right panel is not visible
    /// (e.g. Queue-only panel mode). Factored out of `wide_tv_library_area`
    /// so every paint-free breakpoint consumer shares one pipeline.
    fn right_panel_lib_area(&self) -> Option<Rect> {
        let chrome = crate::app::render::arrangements::chrome::chrome_geometry(
            crate::app::render::arrangements::chrome::ChromeGeometryInput {
                area: Rect::new(0, 0, self.terminal_width, self.terminal_height),
                panel_mode: self.effective_panel_mode(),
                panel_focus: self.effective_panel_focus(),
                queue_column_width: self.queue_column_width,
                terminal_width: self.terminal_width,
            },
        );
        if !chrome.right_visible {
            return None;
        }
        Some(
            crate::app::render::components::widgets::right_panel_content_area(
                chrome.right_area,
                self.effective_panel_mode() != PanelMode::Both,
            ),
        )
    }

    /// Whether the right panel is in the wide Wide hero breakpoint right
    /// now, derived paint-free from the current terminal size. Replaces the
    /// four `LayoutMain::is_wide_*_active()` paint-inference predicates: the
    /// breakpoint (`wide_hero_presentation`) is the same for every
    /// Wide hero destination, so one predicate serves all of them.
    pub(in crate::app) fn is_right_panel_wide(&self) -> bool {
        self.right_panel_lib_area()
            .is_some_and(|area| wide_hero::wide_hero_presentation(area).is_some())
    }

    /// The finalized library content rect when the wide Wide hero TV
    /// workspace owns `lib_idx`, computed paint-free from the current
    /// terminal size — `None` when the library is not a wide-TV series list
    /// or the breakpoint is narrow. Mirrors the exact gate `render_library`
    /// applies (`is_wide_tv_library` + `wide_hero_presentation` on the
    /// finalized area), so component mount/focus can be routed a frame
    /// earlier than the deleted previous-frame paint signal this predicate
    /// replaced, which used to flash the narrow browser on entry.
    pub(in crate::app) fn wide_tv_library_area(&self, lib_idx: usize) -> Option<Rect> {
        if !self.is_wide_tv_library(lib_idx) {
            return None;
        }
        let lib_area = self.right_panel_lib_area()?;
        wide_hero::wide_hero_presentation(lib_area).map(|_| lib_area)
    }

    pub(in crate::app) fn wide_tv_render_ctx(
        &self,
        lib_idx: usize,
        cursor_scroll: Option<(usize, usize)>,
    ) -> TvWideRenderCtx {
        let list = self.library_list_render_ctx(
            lib_idx,
            cursor_scroll.map_or_else(|| 0, |v| v.0),
            cursor_scroll.map_or_else(|| 0, |v| v.1),
        );
        let selected_series = list
            .selected_item()
            .cloned()
            .filter(|item| item.item_type == "Series");
        let series_detail = selected_series
            .as_ref()
            .and_then(|item| self.series_detail_cache.get(&item.id).cloned());
        TvWideRenderCtx::new(
            list,
            selected_series,
            series_detail,
            0,
            None,
            self.should_show_letter_pills(lib_idx),
        )
    }
}

/// App-free wide TV renderer. The shell builds `TvWideRenderCtx` and the
/// component supplies its local cursor and pane focus through that context.
pub(in crate::app) fn render_wide_tv_with_ctx(
    f: &mut Frame,
    area: Rect,
    ctx: &TvWideRenderCtx,
    layout: &mut LayoutMain,
    media_list: &mut WideMediaList<String>,
    episodes: &mut WideMediaList<String>,
    inline_search: &mut InlineSearch,
) -> (usize, Option<HomeImagePaint>) {
    layout.tv_wide_episode_list_area = Rect::default();
    layout.tv_wide_season_tabs.clear();
    layout.tv_wide_area = area;

    let Some(panes) = library_arrangement::wide_library_panes(area, PANE_PAD_X, PANE_PAD_Y) else {
        return (0, None);
    };
    let right_panel = panes.right_panel;
    let right_area = panes.right_area;
    let episode_focused = ctx.focused && ctx.episode_cursor.is_some();
    let right_focused = ctx.focused && !episode_focused;
    let Some(left_area) = wide_hero::wide_hero_hero_pane(
        f,
        area,
        wide_hero::LeftPaneFocus::Workspace(ctx.focused && ctx.episode_cursor.is_some()),
    ) else {
        return (0, None);
    };
    layout.tv_wide_left_area = left_area;
    layout.tv_wide_right_area = right_area;
    layout.left_area = Rect::default();

    let (selection_rendered, image_paint) = render_tv_series_selection(
        f,
        left_area,
        episode_focused,
        ctx.selected_series.as_ref(),
        ctx.series_detail.as_ref(),
        ctx.season_cursor,
        layout,
        ctx.images_enabled,
        ctx.image_loading,
        episodes,
    );
    if !selection_rendered {
        render_placeholder(f, left_area, " Loading\u{2026}");
    }

    let right_pane = wide_hero::wide_hero_browser_pane(right_panel, right_area);
    if ctx.list.is_search_active() {
        crate::app::render::components::hero::render_search_box(
            f,
            right_pane.pills_area,
            ctx.list.search_query.as_deref().unwrap_or_default(),
            ctx.list.search_loading,
        );
    } else if ctx.show_letter_pills {
        let selected = ctx
            .list
            .letter_filter
            .as_ref()
            .map(|filter| filter.index)
            .unwrap_or(0);
        let labels = crate::app::render::LetterFilter::labels();
        let ids: Vec<usize> = (0..labels.len()).collect();
        layout.selector_tabs = render_pill_bar(
            f,
            right_pane.pills_area,
            PillBar {
                labels: &labels,
                ids: &ids,
                selected_pos: selected,
                prefix: Some(" \u{2318} "),
            },
        );
    }

    let list_panel = right_pane.list_panel;
    let list_area = padded_rect(list_panel, PANE_PAD_X, PANE_PAD_Y);
    layout.tv_wide_list_area = list_area;
    if list_panel.height > 0 {
        f.render_widget(
            Block::default()
                .style(Style::default().bg(palette::resolve_surface_focus(right_focused))),
            list_panel,
        );
    }
    // The canonical rail owns the full panel row: selection markers and
    // selected backgrounds must reach the panel border, while the layout
    // area remains the padded hit/scroll geometry.
    let paint_area = Rect {
        x: list_panel.x,
        width: list_panel.width,
        ..list_area
    };
    wide_hero::wide_hero_browser_border(f, list_panel, right_focused);
    let final_scroll = if inline_search.is_active() {
        // Wide hero Wide passes only the right-rail library-list area
        // (design.md D3); the episode/Hero pane painted above remains
        // visible and the ordinary series rail does not also paint
        // `list_area`.
        let items = inline_search.ordered_items();
        let query = inline_search.query().to_string();
        let loading = inline_search.loading();
        let cursor = inline_search.cursor();
        let scroll_in = inline_search.scroll();
        let new_scroll = crate::app::render::render_inline_search(
            f,
            list_area,
            &query,
            loading,
            items,
            cursor,
            scroll_in,
            right_focused,
            1,
            inline_search.layout_mut(),
        );
        inline_search.set_scroll(new_scroll);
        new_scroll
    } else {
        // Legacy rail parity (`item_cell_spans`): the selected row takes the
        // resting surface so it reads against the focused green panel body.
        let paint = super::media_list::render_wide_media_list(
            f,
            paint_area,
            list_area,
            media_list,
            right_focused,
            palette::list_selected_row_bg(),
        );
        layout.left_item_rows = paint.left_item_rows;
        layout.left_row_map = paint.left_row_map;
        // Same key the component sorts the rail rows by, so
        // `left_sorted_indices` matches the painted order;
        // `sort_by_cached_key` computes each key once.
        let mut order: Vec<usize> = (0..ctx.list.items.len()).collect();
        order.sort_by_cached_key(|&index| {
            crate::app::ui_util::natural_sort_key(crate::app::render::effective_sort_str(
                &ctx.list.items[index],
            ))
        });
        layout.left_sorted_indices = order;
        paint.row_geometry.offset()
    };
    (final_scroll, image_paint)
}

#[allow(clippy::too_many_arguments)]
fn render_tv_series_selection(
    f: &mut Frame,
    area: Rect,
    focused: bool,
    selected_series: Option<&EmbyItem>,
    detail: Option<&SeriesDetail>,
    season_cursor: usize,
    layout: &mut LayoutMain,
    images_enabled: bool,
    image_loading: bool,
    episodes: &mut WideMediaList<String>,
) -> (bool, Option<HomeImagePaint>) {
    let Some(item) = selected_series else {
        return (false, None);
    };

    // Artwork-slot-first layout (design.md D-D): a full-width 16:9 landscape
    // slot above the title/metadata/overview, sized with the same formula
    // `prepare_wide_emby_hero_card` uses for Home's wide Wide hero card.
    let artwork_height = if images_enabled {
        (area.width.saturating_mul(9).saturating_add(31) / 32).max(1)
    } else {
        0
    };
    // Size the box to every episode row, floored at the visible-row minimum;
    // `place_media_list_below` clamps this to the pane bottom and the
    // canonical list scrolls any remainder.
    let media_list_height = (episodes.rows().len() as u16)
        .max(EPISODE_LIST_VISIBLE_ROWS)
        .saturating_add(1) // season pill row
        .saturating_add(PANE_PAD_Y * 2);
    let slots = wide_hero::wide_hero_slots(area, artwork_height, images_enabled);

    let image_paint = slots.artwork.and_then(|artwork_area| {
        match item.artwork_for(HeroArtworkAspect::Landscape) {
            HeroArtwork::Placeholder if images_enabled => {
                super::artwork_placeholder::render_artwork_placeholder(f, artwork_area);
                None
            }
            HeroArtwork::Image { image_types, .. } => Some(HomeImagePaint::Series {
                area: artwork_area,
                item: Box::new(item.clone()),
                show_placeholder: image_loading,
                image_types,
            }),
            _ => None,
        }
    });

    let content_area = slots.overview;
    if content_area.height == 0 {
        return (true, image_paint);
    }

    let title = item.title().to_string();
    let meta = item
        .meta_rows(content_area.width)
        .into_iter()
        .next()
        .map(|spans| {
            spans
                .iter()
                .map(|span| span.content.as_ref())
                .collect::<String>()
        });
    // Title + ordered metadata only here -- the overview gets its own
    // main-content box below (design.md D-C), matching the season/episode
    // detail box already painted the same way further down.
    let result = crate::app::render::components::hero::paint_hero_content(
        f,
        content_area,
        &HeroContent {
            title: Some(title.as_str()),
            meta_line: meta.as_deref(),
            meta_color: palette::TEXT_DETAIL_META,
            show_playing: false,
            unconditional_spacer_after_meta: true,
            lines: &[],
            image: None,
        },
        focused,
    );
    let row = result.next_row;
    let description = item.description();
    let mut overview_bottom = row;
    if let Some(text) = description.filter(|t| !t.is_empty()) {
        let box_content_width = content_area.width.saturating_sub(PANE_PAD_X * 2) as usize;
        let ov_lines = wrap_overview_lines(&text, |_| box_content_width);
        let ov_height = (ov_lines.len() as u16)
            .max(1)
            .saturating_add(PANE_PAD_Y * 2)
            .min(content_area.bottom().saturating_sub(row));
        if ov_height > PANE_PAD_Y * 2 {
            let box_area = Rect::new(
                content_area.x.saturating_sub(PANE_PAD_X),
                row,
                content_area.width.saturating_add(PANE_PAD_X * 2),
                ov_height,
            );
            let (_, ov_content) = wide_hero::wide_hero_hero_content_box(f, box_area);
            let ov_color = if focused {
                palette::TEXT_STRONG
            } else {
                palette::TEXT_MUTED
            };
            f.render_widget(
                Paragraph::new(Span::styled(
                    ov_lines.join(" "),
                    Style::default().fg(ov_color),
                ))
                .wrap(Wrap { trim: true }),
                ov_content,
            );
            overview_bottom = row + ov_height;
        }
    }
    // The episode media-list box is a separate, fixed-height recessed box
    // placed one blank row below the overview box's real painted bottom
    // edge (task 4.2d's regression fix), not pre-reserved by
    // `wide_hero_slots` -- overview height is text-length-dependent and
    // only known after the overview is painted.
    let Some(media_list_area) = place_media_list_below(
        content_area,
        overview_bottom,
        MEDIA_LIST_GAP_ROWS,
        media_list_height,
    ) else {
        return (true, image_paint);
    };
    let Some(detail) = detail else {
        let (_, content) = wide_hero::wide_hero_hero_content_box(f, media_list_area);
        render_placeholder(f, content, " Loading\u{2026}");
        return (true, image_paint);
    };
    let Some(season) = detail.seasons.get(season_cursor) else {
        return (true, image_paint);
    };
    let (detail_panel, detail_area) = wide_hero::wide_hero_hero_content_box(f, media_list_area);
    if focused {
        f.render_widget(
            Block::default().style(Style::default().bg(palette::SURFACE_ACCENT_SOFT)),
            detail_panel,
        );
    }
    if detail_area.height == 0 || detail_area.width == 0 {
        return (true, image_paint);
    }
    let labels: Vec<String> = detail
        .seasons
        .iter()
        .map(|season| season.display_name())
        .collect();
    let ids: Vec<usize> = (0..labels.len()).collect();
    // Season pills stay parent-owned chrome (design.md D-D): painted here,
    // never absorbed into `wide_hero_slots`/`WideMediaList`.
    layout.tv_wide_season_tabs = render_pill_bar(
        f,
        Rect::new(detail_area.x, detail_area.y, detail_area.width, 1),
        PillBar {
            labels: &labels,
            ids: &ids,
            selected_pos: season_cursor,
            prefix: Some(" Series: "),
        },
    );
    let episode_list_area = Rect {
        y: detail_area.y.saturating_add(1),
        height: detail_area.height.saturating_sub(1),
        ..detail_area
    };
    if episode_list_area.height == 0 {
        return (true, image_paint);
    }
    if episodes.is_empty() {
        render_placeholder(
            f,
            Rect::new(
                episode_list_area.x,
                episode_list_area.y,
                episode_list_area.width,
                1,
            ),
            if detail.episodes.contains_key(&season.id) {
                " (no episodes)"
            } else {
                " Loading\u{2026}"
            },
        );
        return (true, image_paint);
    }
    layout.tv_wide_episode_list_area = episode_list_area;
    let paint_area = Rect {
        x: detail_panel.x,
        width: detail_panel.width,
        ..episode_list_area
    };
    super::media_list::render_wide_media_list(
        f,
        paint_area,
        episode_list_area,
        episodes,
        focused,
        palette::list_selected_row_bg(),
    );
    (true, image_paint)
}

#[cfg(test)]
#[path = "tv_wide_tests.rs"]
mod tests;
