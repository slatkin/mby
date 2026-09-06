use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::widgets::Block;
use ratatui::Frame;

use super::BrowserComponent;
use crate::app::components::component_id::BrowserKind;
use crate::app::palette;
use crate::app::render::{
    padded_rect, prepare_wide_emby_hero_card, render_count_label, render_home_hero_content,
    render_pill_bar, wide_hero_browser_border, wide_hero_browser_pane, wide_hero_hero_pane,
    wide_library_panes, HeroData, LetterFilter, LibraryListRenderCtx, PillBar, PANE_PAD_X,
    PANE_PAD_Y,
};

impl BrowserComponent {
    /// Paints the wide Movies/home-video Wide hero layout: a read-only
    /// shared Emby hero card on the left and the letter-pill/count/search
    /// row plus the one-column list in the right rail. Mirrors the deleted
    /// legacy wide renderer so the picture is unchanged.
    /// Returns the final list scroll (the component owns its cursor/scroll,
    /// so it records it instead of writing the App nav level).
    pub(super) fn render_wide_movies(
        &mut self,
        f: &mut Frame,
        area: Rect,
        ctx: &LibraryListRenderCtx,
    ) -> usize {
        let body_area = area;

        let Some(panes) = wide_library_panes(body_area, PANE_PAD_X, PANE_PAD_Y) else {
            // Defensive structure only: unreachable on canonical Wide paths.
            // `browser/mod.rs` calls `render_wide_movies` solely when
            // `wide_hero_presentation(area).is_some()`, and
            // `wide_library_panes` returns `None` only when that same check
            // fails on the same rect (`body_area == area`). If a degenerate
            // rect ever reaches here, keep a canonical render rather than
            // routing to the legacy painter.
            let paint = crate::app::render::render_wide_media_list(
                f,
                body_area,
                body_area,
                &mut self.wide_list,
                self.focused,
                palette::list_selected_row_bg(),
            );
            self.layout.left_item_rows = paint.left_item_rows;
            self.layout.left_row_map = paint.left_row_map;
            return paint.row_geometry.offset();
        };
        let browser_panel = panes.browser_panel;

        let browser_area = panes.browser_area;
        self.layout.movies_wide_right_area = browser_area;

        // Left pane: read-only shared hero card (not an interactive hero —
        // `layout.hero_area` stays unset so the left pane is outside mouse
        // geometry, mirroring the legacy wide renderer).
        let hero_content =
            wide_hero_hero_pane(f, body_area, crate::app::render::LeftPaneFocus::ReadOnly)
                .expect("wide movies layout has a hero pane");
        let hero_data = ctx
            .selected_item()
            .filter(|item| {
                !item.is_folder
                    && (!matches!(self.kind, BrowserKind::Movies) || item.item_type == "Movie")
            })
            .and_then(|item| {
                prepare_wide_emby_hero_card(item, hero_content, self.images_enabled).map(
                    |(meta_layout, meta_area, img_area)| {
                        HeroData::new(
                            Box::new(item.clone()),
                            meta_area,
                            meta_area,
                            img_area,
                            meta_layout,
                        )
                    },
                )
            });

        // Right rail: pill row + one-column list.
        let right_pane = wide_hero_browser_pane(browser_panel, browser_area);
        let pills_area = right_pane.pills_area;
        let list_panel = right_pane.list_panel;

        if self.narrow_extras.feed_items.is_some() {
            crate::app::render::paint_feed_group_pills_row(
                f,
                pills_area,
                &self.narrow_extras,
                &mut self.layout,
            );
        } else if self.wide_movies_home_video {
            render_count_label(f, pills_area, ctx.total_count);
        } else if self.wide_movies_letter_pills {
            self.render_letter_pills_row(f, pills_area, ctx);
        }

        if list_panel.height > 0 {
            let list_bg = palette::resolve_surface_focus(self.focused);
            f.render_widget(
                Block::default().style(Style::default().bg(list_bg)),
                list_panel,
            );
        }
        // `content` is the inset row/hit geometry; `paint` keeps the full
        // panel width so the selected-row bar and flush marker reach the rail
        // border, inset vertically for the framed border rows.
        let content = padded_rect(list_panel, PANE_PAD_X, PANE_PAD_Y);
        let paint = Rect {
            x: list_panel.x,
            width: list_panel.width,
            ..content
        };

        self.layout.left_area = content;
        // Frame the rail before the row flow: the helper fills the whole
        // panel background, so it must run before `render_wide_media_list`
        // paints the selected-row bar (matches TV / Music ordering).
        wide_hero_browser_border(f, list_panel, self.focused);
        let final_scroll = if self.inline_search.is_active() {
            // Wide hero Wide passes only the right-rail library-list area
            // (design.md D3); the Hero pane painted above remains visible and
            // the ordinary canonical list does not also paint `content`.
            let items = self.inline_search.ordered_items();
            let query = self.inline_search.query().to_string();
            let loading = self.inline_search.loading();
            let cursor = self.inline_search.cursor();
            let scroll_in = self.inline_search.scroll();
            let new_scroll = crate::app::render::render_inline_search(
                f,
                pills_area,
                content,
                &query,
                loading,
                items,
                cursor,
                scroll_in,
                self.focused,
                1,
                self.inline_search.layout_mut(),
            );
            self.inline_search.set_scroll(new_scroll);
            new_scroll
        } else if self.wide_list.is_empty() {
            crate::app::render::components::widgets::render_placeholder(
                f,
                content,
                if ctx.loading {
                    " Loading…"
                } else {
                    " (empty)"
                },
            );
            0
        } else {
            let painted = crate::app::render::render_wide_media_list(
                f,
                paint,
                content,
                &mut self.wide_list,
                self.focused,
                palette::list_selected_row_bg(),
            );
            self.layout.left_item_rows = painted.left_item_rows;
            self.layout.left_row_map = painted.left_row_map;
            let offset = painted.row_geometry.offset();
            // Export the selected-row anchor from the control's exact painted
            // flow; the shell consumes it for context-menu placement.
            self.layout.selected_item_rect = painted.selected_row_rect;
            // Republish the sorted display order the rail was built from so the
            // parent's letter-aware keyboard navigation keeps resolving targets
            // against `self.layout` (mirrors `render_wide_tv_with_ctx`; task
            // 3.5c re-points navigation onto the control itself).
            let grouped =
                !ctx.is_search_active() && (ctx.true_total() >= 50 || ctx.letter_filter.is_some());
            self.layout.left_sorted_indices = if grouped {
                let mut order: Vec<usize> = (0..ctx.items.len()).collect();
                order.sort_by_cached_key(|&index| {
                    crate::app::ui_util::natural_sort_key(crate::app::render::effective_sort_str(
                        &ctx.items[index],
                    ))
                });
                order
            } else {
                (0..ctx.items.len()).collect()
            };
            offset
        };

        // Paint the shared hero text last (after the list); defer the cover
        // image paint to the shell, which owns the image-cache authority.
        if let Some(hero_data) = &hero_data {
            self.image_paint =
                render_home_hero_content(f, hero_data, true, self.focused, self.use_nerd_fonts);
        } else {
            self.image_paint = None;
        }

        final_scroll
    }

    pub(super) fn render_letter_pills_row(
        &mut self,
        f: &mut Frame,
        row_area: Rect,
        ctx: &LibraryListRenderCtx,
    ) {
        if row_area.width == 0 {
            self.layout.selector_tabs = Vec::new();
            return;
        }
        let selected_pos = ctx.letter_filter.as_ref().map(|flt| flt.index).unwrap_or(0);
        let labels = LetterFilter::labels();
        let ids: Vec<usize> = (0..labels.len()).collect();
        self.layout.selector_tabs = render_pill_bar(
            f,
            row_area,
            PillBar {
                labels: &labels,
                ids: &ids,
                selected_pos,
                prefix: Some(" ⌘ "),
            },
        );
    }
}
