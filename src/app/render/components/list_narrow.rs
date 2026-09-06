//! Canonical narrow browse composition for generic, Movies, and home-video
//! destinations. `BrowserComponent` owns inputs; this module paints and
//! publishes replacement-flow geometry for its callers.

use super::detail::compact_banner_image_cache_key;
use crate::app::components::browser_narrow::{NarrowBrowseExtras, NarrowInlineHero};
use crate::app::components::media_list::InlineMediaBrowser;
use crate::app::images::series_image_cache_key;
use crate::app::layout::LayoutMain;
use crate::app::library_column_width::library_column_count;
use crate::app::render::arrangements::{library, wide_hero};
use crate::app::render::components::hero::{
    selected_detail_shell, HERO_BLOCK_EXTRA_ROWS, HERO_PLACEHOLDER_ROWS, HERO_TITLE_ROWS,
};
use crate::app::render::components::list_rows::{
    LibraryListRenderCtx, SELECTED_BLOCK_SIDE_PADDING,
};
use crate::app::render::HomeImagePaint;
use crate::app::App;
use ratatui::layout::Rect;
use ratatui::Frame;

/// Full narrow generic/Movies/home-video browse composition
/// (`migrate-narrow-browse-to-components` task 3.3): the count label, letter
/// pill row, the browse row list with an inline movie/series hero reserved in
/// flow, and the empty-state placeholder — the picture the legacy
/// `render_list` narrow branch painted, now owned by `BrowserComponent` via
/// `browser_narrow.rs`. Pure: `layout` is the component's own geometry and
/// the poster image is returned as a `HomeImagePaint` for the shell to
/// execute (no `App`, cache, or fetch).
pub(in crate::app) fn render_narrow_browse_with_ctx(
    f: &mut Frame,
    area: Rect,
    ctx: &LibraryListRenderCtx,
    extras: &NarrowBrowseExtras,
    focused: bool,
    layout: &mut LayoutMain,
    browser: &mut InlineMediaBrowser<usize>,
) -> (usize, Option<HomeImagePaint>) {
    let mut content_area = area;

    // Feed/home-video group pickers share the browser composer, but their
    // cursor and rows live in FeedHomeVideoState rather than BrowseLevel.
    if extras.home_video && extras.feed_items.is_none() && content_area.height > 0 {
        content_area = crate::app::render::render_count_label(f, content_area, ctx.total_count);
        content_area = Rect {
            y: content_area.y + 1,
            height: content_area.height.saturating_sub(1),
            ..content_area
        };
    }

    // Narrow TV season grids keep their own single-column stride
    // (`is_viewing_season_grid`, legacy `list.rs`); every other narrow browse
    // surface derives the column count from the list width.
    let hero_presentation = extras.inline_hero.is_some() || extras.hero_placeholder;
    let cols = if extras.season_grid || hero_presentation {
        1
    } else {
        library_column_count(content_area.width)
    };

    let mut inline_hero_rows: u16 = match &extras.inline_hero {
        Some(NarrowInlineHero::Movie { layout: banner, .. }) if extras.feed_items.is_some() => {
            extras.feed_selected_height.max(1)
        }
        Some(NarrowInlineHero::Movie { layout: banner, .. }) => {
            banner.content_rows_with_title(HERO_TITLE_ROWS.saturating_mul((cols > 1) as u16)) as u16
                + HERO_BLOCK_EXTRA_ROWS
        }
        Some(NarrowInlineHero::Series {
            item,
            images_enabled,
            ..
        }) => {
            crate::app::render::screens::detail_series::series_inline_detail_rows(
                *images_enabled,
                item,
                content_area.width,
                cols > 1,
            ) as u16
                + HERO_BLOCK_EXTRA_ROWS
        }
        None => {
            if extras.hero_placeholder {
                HERO_PLACEHOLDER_ROWS
            } else {
                0
            }
        }
    };
    if !extras.use_shared_replacement_plan {
        inline_hero_rows =
            if inline_hero_rows > HERO_BLOCK_EXTRA_ROWS && inline_hero_rows < content_area.height {
                inline_hero_rows
            } else {
                0
            };
    }

    // Feed/home-video group pickers and letter pickers both use the shared
    // pill-bar geometry; the picker just fills the row with feed-group pills
    // instead of letters (`is_feed_home_video_group_view`). Everything below —
    // rows, inline-hero replacement, hit maps — is the shared path.
    let feed_group_pills = extras.feed_items.is_some();
    let (pills_area, list_area) = if extras.show_letter_pills || feed_group_pills {
        let areas = wide_hero::pill_bar_areas(content_area);
        (areas.pills_area, areas.content_area)
    } else {
        (Rect::default(), content_area)
    };
    if !extras.use_shared_replacement_plan {
        inline_hero_rows =
            if inline_hero_rows > HERO_BLOCK_EXTRA_ROWS && inline_hero_rows < list_area.height {
                inline_hero_rows
            } else {
                0
            };
    }
    if extras.show_letter_pills {
        paint_letter_pills_row(
            f,
            pills_area,
            ctx.letter_filter.as_ref().map(|flt| flt.index).unwrap_or(0),
            layout,
        );
    } else if feed_group_pills {
        paint_feed_group_pills_row(f, pills_area, extras, layout);
    }

    layout.left_area = list_area;
    layout.hero_area = Rect::default();

    if ctx.items.is_empty() {
        crate::app::render::render_placeholder(
            f,
            list_area,
            if ctx.loading { "Loading..." } else { "(empty)" },
        );
        return (0, None);
    }

    let use_letter_groups =
        !ctx.is_search_active() && (ctx.true_total() >= 50 || ctx.letter_filter.is_some());
    // Hero-bearing narrow surfaces use the canonical inline control. The
    // legacy two-column policy remains for non-hero catalogs.
    let final_offset = if hero_presentation {
        // The persistent InlineMediaBrowser is fed by BrowserComponent before
        // this painter runs. Its rows, selection, and scroll are authoritative;
        // this function only paints and exports the control's flow geometry.
        let result = super::media_list::render_inline_media_browser(
            f,
            list_area,
            &*browser,
            inline_hero_rows as usize,
            focused,
            // Legacy `item_cell_spans` parity: selected row on the resting
            // surface, read against the focused panel body.
            crate::app::palette::list_selected_row_bg(),
        );
        layout.hero_area = result.hero_area.unwrap_or_default();
        layout.inline_hero_area = layout.hero_area;
        // Keep this one map as pre-#638 mouse compatibility. It is copied from
        // the painter's replacement flow rather than rebuilt here.
        layout.left_row_map = result
            .row_geometry
            .targets()
            .skip(result.row_geometry.offset())
            .take(list_area.height as usize)
            .map(|target| target.copied())
            .collect();
        layout.selected_item_rect = result.row_geometry.selected_row_rect(list_area);
        result.row_geometry.offset()
    } else {
        let row_ctx = ctx.rows(list_area, cols, focused, inline_hero_rows);
        if use_letter_groups {
            super::list_letter_groups::render_letter_grouped_rows(
                f,
                row_ctx,
                ctx.letter_filter.clone(),
                ctx.true_total(),
                layout,
            )
        } else {
            super::media_list::render_plain_rows(f, row_ctx, layout)
        }
    };

    let mut image_paint = None;
    if layout.hero_area.height > 0 {
        selected_detail_shell(f, layout.hero_area, inline_hero_rows, focused);
        let content_rect = library::selected_detail_content_area(
            layout.hero_area,
            SELECTED_BLOCK_SIDE_PADDING,
            HERO_BLOCK_EXTRA_ROWS,
        );
        image_paint = match &extras.inline_hero {
            Some(NarrowInlineHero::Movie {
                item,
                layout: banner,
            }) => super::detail::render_compact_detail_with_ctx(
                super::detail::CompactDetailCtx {
                    item,
                    layout: banner.clone(),
                },
                f,
                content_rect,
                focused,
                true,
            ),
            Some(NarrowInlineHero::Series {
                item,
                images_enabled,
                image_loading,
            }) => super::detail_series_view::render_series_inline_detail(
                super::detail_series_view::SeriesInlineDetailCtx {
                    item,
                    images_enabled: *images_enabled,
                    image_loading: *image_loading,
                },
                f,
                content_rect,
                focused,
                true,
            ),
            None => None,
        };
    }

    (final_offset, image_paint)
}

/// Feed/home-video group-picker pill row: an "All" pill plus one per feed
/// folder group, selected by `feed_group_cursor`. Mirrors
/// `paint_letter_pills_row` so the picker reuses the shared narrow browse
/// composer instead of a bespoke painter.
pub(in crate::app) fn paint_feed_group_pills_row(
    f: &mut Frame,
    row_area: Rect,
    extras: &NarrowBrowseExtras,
    layout: &mut LayoutMain,
) {
    if row_area.width == 0 {
        layout.selector_tabs = Vec::new();
        return;
    }
    let labels: Vec<String> = std::iter::once("All".to_string())
        .chain(
            extras
                .feed_groups
                .iter()
                .map(|s| crate::app::ui_util::trunc_str(s, 12)),
        )
        .collect();
    let ids: Vec<usize> = (0..labels.len()).collect();
    layout.selector_tabs = crate::app::render::render_pill_bar(
        f,
        row_area,
        crate::app::render::PillBar {
            labels: &labels,
            ids: &ids,
            selected_pos: extras.feed_group_cursor,
            prefix: Some(" \u{2318} "),
        },
    );
}

fn paint_letter_pills_row(
    f: &mut Frame,
    row_area: Rect,
    selected_pos: usize,
    layout: &mut LayoutMain,
) {
    if row_area.width == 0 {
        layout.selector_tabs = Vec::new();
        return;
    }
    let labels = crate::app::render::LetterFilter::labels();
    let ids: Vec<usize> = (0..labels.len()).collect();
    layout.selector_tabs = crate::app::render::render_pill_bar(
        f,
        row_area,
        crate::app::render::PillBar {
            labels: &labels,
            ids: &ids,
            selected_pos,
            prefix: Some(" \u{2318} "),
        },
    );
}

impl App {
    /// Poster-prefetch window for the narrow generic/Movies/home-video and
    /// podcast browsers (#287): pre-warm the Primary images of movies just
    /// ahead of / behind the cursor. Called from `shell_browser.rs` after the
    /// mounted browser has established its authoritative cursor.
    pub(in crate::app) fn fetch_nearby_movie_posters(
        &mut self,
        items: &[mbv_core::api::EmbyItem],
        cursor: usize,
    ) {
        const PREFETCH_AHEAD: usize = 3;
        const PREFETCH_BEHIND: usize = 1;
        let start = cursor.saturating_sub(PREFETCH_BEHIND);
        let end = (cursor + PREFETCH_AHEAD + 1).min(items.len());
        let prefetch: Vec<(String, String, String)> = items[start..end]
            .iter()
            .enumerate()
            .filter(|(i, item)| start + i != cursor && item.item_type == "Movie" && !item.is_folder)
            .map(|(_, item)| {
                (
                    compact_banner_image_cache_key(&item.id),
                    item.id.clone(),
                    item.series_id.clone(),
                )
            })
            .collect();
        if self.images_enabled() {
            for (cache_key, item_id, series_id) in prefetch {
                self.fetch_list_card_image_when_idle(cache_key, item_id, series_id, &["Primary"]);
            }
        }
    }

    /// Shell-resolved extras for the narrow generic/Movies/home-video browse
    /// composer (`migrate-narrow-browse-to-components` task 3.3): the count
    /// label, letter-pill row, and the inline movie/series hero — everything
    /// that needs `App`/image-cache authority, resolved here and pushed to
    /// `BrowserComponent` each frame.
    pub(in crate::app) fn narrow_browse_extras(
        &mut self,
        lib_idx: usize,
        cursor: usize,
    ) -> NarrowBrowseExtras {
        let coll = self.libs[lib_idx].library.collection_type.clone();
        let feed_group_view = self.is_feed_home_video_group_view(lib_idx);
        let home_video = self.is_home_video_view(lib_idx) && !feed_group_view;
        let show_letter_pills = self.should_show_letter_pills(lib_idx);
        let (feed_items, feed_groups, feed_group_cursor) = if feed_group_view {
            let items = self.feed_home_video_selected_items(lib_idx);
            let groups = self.libs[lib_idx]
                .feed_home_video
                .as_ref()
                .map(|s| s.groups.iter().map(|g| g.folder.name.clone()).collect())
                .unwrap_or_default();
            let cursor = self.feed_home_video_selected_group_index(lib_idx);
            (Some(items), groups, cursor)
        } else {
            (None, Vec::new(), 0)
        };
        let use_shared_replacement_plan = matches!(coll.as_str(), "movies" | "tvshows");
        let season_grid = self.is_viewing_season_grid(lib_idx);

        let selected_movie = self.selected_movie_item(lib_idx, cursor).or_else(|| {
            feed_items.as_ref().and_then(|items| {
                let cursor = self.libs[lib_idx]
                    .feed_home_video
                    .as_ref()
                    .map_or(0, |s| s.video_cursor);
                items.get(cursor).cloned()
            })
        });
        let selected_series = if selected_movie.is_none() {
            self.selected_series_item(lib_idx, cursor)
        } else {
            None
        };

        let inline_hero = if let Some(item) = selected_movie {
            let truncate_overview =
                self.is_home_video_view(lib_idx) || self.is_podcast_library(lib_idx);
            let panel_width = self
                .layout
                .main
                .left_area
                .width
                .saturating_sub(2 * SELECTED_BLOCK_SIDE_PADDING);
            let banner =
                self.compact_banner_layout_with_overview(&item, panel_width, truncate_overview);
            Some(NarrowInlineHero::Movie {
                item,
                layout: banner,
            })
        } else if let Some(item) = selected_series {
            let images_enabled = self.images_enabled();
            // Narrow keeps its own `Primary`-chain entry (the chain
            // `detail_series_view` paints); it must never read Wide's
            // Thumb-first entry, whose bytes differ.
            let image_cache_key = series_image_cache_key(&item.id, &["Primary"]);
            let image_loading =
                images_enabled && !self.card_image_states.contains_key(&image_cache_key);
            Some(NarrowInlineHero::Series {
                item,
                images_enabled,
                image_loading,
            })
        } else {
            None
        };

        let feed_selected_height = if feed_group_view {
            match &inline_hero {
                Some(NarrowInlineHero::Movie { layout: banner, .. }) => {
                    let rows = banner.content_rows();
                    if rows == 0 {
                        0
                    } else {
                        banner.content_rows_with_title(1) as u16 + 5
                    }
                }
                _ => 0,
            }
        } else {
            0
        };

        // Every hero-capable browse destination, including folder/channel
        // selections without a resolved leaf hero, uses the inline
        // replacement flow. Non-hero catalogs keep their width-derived grid.
        let hero_placeholder = inline_hero.is_none()
            && crate::app::render::arrangements::wide_hero::wide_hero_presentation(
                self.layout.main.left_area,
            )
            .is_none()
            && matches!(
                coll.as_str(),
                "movies" | "homevideos" | "podcasts" | "tvshows" | "music"
            );

        NarrowBrowseExtras {
            home_video,
            show_letter_pills,
            use_shared_replacement_plan,
            hero_placeholder,
            season_grid,
            feed_items,
            feed_groups,
            feed_group_cursor,
            feed_selected_height,
            inline_hero,
        }
    }
}
