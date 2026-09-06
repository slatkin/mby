use crate::app::components::media_list::{InlineMediaBrowser, RowGeometry, WideMediaList};
use crate::app::layout::LayoutMain;
use crate::app::palette;
use crate::app::render::arrangements::{padded_rect, wide_hero};
use crate::app::render::components::hero::{
    paint_hero_content, selected_detail_shell, HeroContent, HERO_BLOCK_EXTRA_ROWS,
};
use crate::app::render::components::list_rows::SELECTED_BLOCK_SIDE_PADDING;
use crate::app::render::components::widgets::{render_pill_bar, render_placeholder, PillBar};
use crate::app::render::render_artwork_placeholder;
use crate::app::render::screens::feeds_model::{feed_entry_meta_line, feed_hero_content_rows};
use crate::app::types_feed_tab::WatchedFilter;
use mbv_core::config::FeedSubscription;
use mbv_core::playback_queue::FeedEntry;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Paragraph};
use ratatui::Frame;

use super::media_list::{render_inline_media_browser, render_wide_media_list};

/// The parent-owned Feeds chrome model: the subscription/group selector pills
/// and the watched-filter selector stay outside the canonical control, which
/// only ever paints the sub-rect below that pill strip.
pub(in crate::app) struct FeedsRenderModel<'a> {
    pub subscriptions: &'a [FeedSubscription],
    pub visible_entries: &'a [FeedEntry],
    pub watched_filter: WatchedFilter,
    pub selected_group: usize,
    pub loading: bool,
    /// The cursor-selected entry, for the parent-owned detail hero (Wide left
    /// pane and Narrow inline replacement block). `None` when nothing is
    /// selectable.
    pub selected_entry: Option<&'a FeedEntry>,
    pub images_enabled: bool,
}

/// Paints the Feeds destination's parent-owned pill strip + watched-filter
/// chrome + Wide hero detail pane, then mounts the active canonical control
/// (`WideMediaList` for Wide hero Wide, `InlineMediaBrowser` for inline
/// Narrow) into the list sub-rect below the pill strip and rebuilds the
/// pre-#638 row-geometry maps from its exported `RowGeometry`. Returns the
/// resolved scroll offset the painter used this frame (observability only; the
/// control owns cursor/scroll and there is no render write-back).
pub(in crate::app) fn render_feeds_content(
    f: &mut Frame,
    area: Rect,
    focused: bool,
    layout: &mut LayoutMain,
    model: FeedsRenderModel<'_>,
    canonical_list: &mut WideMediaList<String>,
    inline_list: &InlineMediaBrowser<String>,
) -> usize {
    if area.height == 0 || area.width == 0 {
        return 0;
    }
    layout.feeds_area = area;
    let subscriptions = model.subscriptions;
    let has_subs = !subscriptions.is_empty();

    // The shared arrangement owns the pill row and spacer. The watched
    // filter remains Feeds chrome immediately below that spacer, with the
    // existing trailing gap before the list.
    let render_selector_content = |f: &mut Frame, pane: Rect| {
        let areas = wide_hero::pill_bar_areas(pane);
        let mut selector_tabs = Vec::new();
        if has_subs && areas.pills_area.height > 0 {
            const MAX_LABEL: usize = 12;
            let labels: Vec<String> = std::iter::once("All".to_string())
                .chain(subscriptions.iter().map(|sub| {
                    if sub.name.len() > MAX_LABEL {
                        format!("{}…", &sub.name[..MAX_LABEL])
                    } else {
                        sub.name.clone()
                    }
                }))
                .collect();
            let ids: Vec<usize> = (0..labels.len()).collect();
            selector_tabs = render_pill_bar(
                f,
                areas.pills_area,
                PillBar {
                    labels: &labels,
                    ids: &ids,
                    selected_pos: model.selected_group,
                    prefix: Some(" ⌘ "),
                },
            );
        }

        let filter_area = Rect {
            y: areas.spacer_area.bottom(),
            height: if has_subs {
                1.min(areas.content_area.height)
            } else {
                0
            },
            ..areas.content_area
        };
        if has_subs && filter_area.height > 0 {
            let filter = model.watched_filter;
            let mut spans = Vec::new();
            for (i, f_variant) in [
                crate::app::types_feed_tab::WatchedFilter::All,
                crate::app::types_feed_tab::WatchedFilter::Watched,
                crate::app::types_feed_tab::WatchedFilter::Unwatched,
            ]
            .iter()
            .enumerate()
            {
                if i > 0 {
                    spans.push(Span::styled(
                        " · ",
                        Style::default().fg(palette::TEXT_MUTED),
                    ));
                }
                let active = *f_variant == filter;
                spans.push(Span::styled(
                    f_variant.label().to_string(),
                    if active {
                        Style::default()
                            .fg(palette::ACCENT)
                            .add_modifier(Modifier::BOLD)
                    } else {
                        Style::default().fg(palette::TEXT_MUTED)
                    },
                ));
            }
            f.render_widget(
                Paragraph::new(Line::from(spans))
                    .style(Style::default().bg(palette::SURFACE_BACKDROP)),
                filter_area,
            );
        }

        let list_y = filter_area
            .y
            .saturating_add(if has_subs { 2 } else { 1 })
            .min(pane.y.saturating_add(pane.height));
        let list_area = Rect {
            y: list_y,
            height: pane.y.saturating_add(pane.height).saturating_sub(list_y),
            ..pane
        };
        (selector_tabs, list_area)
    };

    // The shared arrangement owns the pill row and spacer (and the status-row
    // reserve on both returned panes).
    let wide_panes = wide_hero::wide_hero_presentation(area);
    let selector_pane = wide_panes.map(|panes| panes.browser).unwrap_or(area);
    let (selector_tabs, list_panel) = render_selector_content(f, selector_pane);
    layout.selector_tabs = selector_tabs;
    layout.left_area = list_panel;
    if list_panel.height == 0 {
        return 0;
    }

    // Empty / help states: no canonical control, one-line placeholder.
    if !has_subs {
        render_placeholder(
            f,
            Rect {
                height: 1,
                ..list_panel
            },
            " No feed subscriptions configured",
        );
        return 0;
    }
    if model.visible_entries.is_empty() {
        let msg = if model.loading {
            " Loading…"
        } else {
            " Press r to load feeds"
        };
        render_placeholder(
            f,
            Rect {
                height: 1,
                ..list_panel
            },
            msg,
        );
        return 0;
    }

    let wide = wide_panes.is_some();

    // Wide: paint the parent-owned Wide hero detail pane, then frame the
    // right rail and inset the canonical control inside it so the border can
    // never replace a heading or the last visible entry at a scroll boundary.
    let (list_area, outer_panel) = if let Some(hero_panel) = wide_panes.map(|panes| panes.hero) {
        layout.hero_area = hero_panel;
        // Wide left hero pane: unconditional fill via the shared primitive
        // (D1, persistent pane -- painted even with no selected entry). Feeds
        // is read-only and never focus-green (D3/D8).
        let hero_content_area =
            wide_hero::wide_hero_hero_pane(f, area, wide_hero::LeftPaneFocus::ReadOnly)
                .expect("wide branch already confirmed wide_hero_presentation fits");
        let (_, hero_content_area) = wide_hero::wide_hero_hero_content_box(f, hero_content_area);
        if let Some(entry) = model.selected_entry {
            paint_feed_hero(f, hero_content_area, entry, focused, model.images_enabled);
        }
        f.render_widget(
            Block::default().style(Style::default().bg(palette::resolve_surface_focus(focused))),
            list_panel,
        );
        // `list_area` is the inset content rect (row/hit geometry); the
        // painter is handed a full-width, vertically-inset paint rect below.
        (
            padded_rect(list_panel, wide_hero::PANE_PAD_X, wide_hero::PANE_PAD_Y),
            Some(list_panel),
        )
    } else {
        (list_panel, None)
    };
    layout.left_area = list_area;
    if list_area.height == 0 {
        return 0;
    }

    if wide {
        // Full panel width so the selected-row bar and flush marker reach the
        // rail border; vertically inset so the framed border never overpaints
        // a heading or the last visible entry.
        let paint_rect = Rect {
            x: outer_panel.map_or(list_area.x, |panel| panel.x),
            width: outer_panel.map_or(list_area.width, |panel| panel.width),
            ..list_area
        };
        // Frame the rail before the row flow: the helper fills the whole
        // panel background, so it must run before `render_wide_media_list`
        // paints the selected-row bar (matches TV / Music ordering).
        if let Some(panel) = outer_panel {
            wide_hero::wide_hero_browser_border(f, panel, focused);
        }
        let paint = render_wide_media_list(
            f,
            paint_rect,
            list_area,
            canonical_list,
            focused,
            palette::list_selected_row_bg(),
        );
        layout.selected_item_rect = paint.selected_row_rect;
        rebuild_selectable_maps(layout, &paint.row_geometry, list_area);
        layout.inline_hero_area = Rect::default();
        paint.row_geometry.offset()
    } else {
        let desired_detail_rows =
            feed_hero_content_rows(true).saturating_add(HERO_BLOCK_EXTRA_ROWS) as usize;
        let result = render_inline_media_browser(
            f,
            list_area,
            inline_list,
            desired_detail_rows,
            focused,
            palette::list_selected_row_bg(),
        );
        let geometry = result.row_geometry;
        let offset = geometry.offset();
        rebuild_selectable_maps(layout, &geometry, list_area);
        match result.hero_area {
            Some(hero_area) => {
                layout.hero_area = hero_area;
                layout.inline_hero_area = hero_area;
                layout.selected_item_rect = Some(hero_area);
                if let Some(entry) = model.selected_entry {
                    selected_detail_shell(f, hero_area, hero_area.height, focused);
                    paint_feed_hero(
                        f,
                        Rect {
                            x: hero_area.x + SELECTED_BLOCK_SIDE_PADDING,
                            y: hero_area.y + 2,
                            width: hero_area
                                .width
                                .saturating_sub(2 * SELECTED_BLOCK_SIDE_PADDING),
                            height: hero_area.height.saturating_sub(HERO_BLOCK_EXTRA_ROWS),
                        },
                        entry,
                        focused,
                        model.images_enabled,
                    );
                }
            }
            None => {
                layout.hero_area = Rect::default();
                layout.inline_hero_area = Rect::default();
                layout.selected_item_rect = geometry.selected_row_rect(list_area);
            }
        }
        offset
    }
}

/// Paint the feeds detail hero (title + one metadata line, no artwork) into the
/// already-inset `content` rect. Wide passes the plain pane's padded rect (like
/// the sibling hero painters); Narrow passes the rect inset inside its `▔`/`▁`
/// HeroShell.
fn paint_feed_hero(
    f: &mut Frame,
    content: Rect,
    entry: &FeedEntry,
    focused: bool,
    images_enabled: bool,
) {
    let meta = feed_entry_meta_line(entry);
    let image_width = if images_enabled {
        (content.width / 8).max(1)
    } else {
        0
    };
    let image_height = (image_width.saturating_mul(9).saturating_add(31) / 32)
        .min(content.height)
        .max(1);
    let result = paint_hero_content(
        f,
        content,
        &HeroContent {
            title: Some(entry.title.as_str()),
            meta_line: Some(meta.as_str()),
            meta_color: palette::PLAYBACK_META_FG,
            show_playing: false,
            unconditional_spacer_after_meta: false,
            lines: &[],
            image: images_enabled.then_some(crate::app::render::components::hero::HeroImage {
                actual_w: image_width,
                height: image_height,
            }),
        },
        focused,
    );
    if let Some(image_area) = result.img_rect {
        render_artwork_placeholder(f, image_area);
    }
}

/// Rebuild the pre-#638 mouse-compat maps from the canonical control's
/// exported `RowGeometry`: every painted flow row that carries a selectable
/// target maps to that control's selectable index (which, for Feeds, is the
/// `visible_entries` index); headings, spacers, and replacement continuation
/// rows stay `None`. `left_item_rows` is parallel to the full flow; `left_row_map`
/// is the visible window.
fn rebuild_selectable_maps<T>(layout: &mut LayoutMain, geometry: &RowGeometry<T>, area: Rect) {
    let mut next_selectable = 0usize;
    let per_row: Vec<Option<usize>> = geometry
        .targets()
        .map(|target| {
            target.map(|_| {
                let index = next_selectable;
                next_selectable += 1;
                index
            })
        })
        .collect();
    layout.left_item_rows = per_row
        .iter()
        .map(|slot| slot.map(|index| vec![index]).unwrap_or_default())
        .collect();
    layout.left_row_map = per_row
        .into_iter()
        .skip(geometry.offset())
        .take(area.height as usize)
        .collect();
}
