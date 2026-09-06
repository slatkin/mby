use crate::app::components::media_list::{InlineMediaBrowser, WideMediaList};
use crate::app::palette;
use crate::app::render::arrangements::library as library_arrangement;
use crate::app::render::arrangements::padded_rect;
use crate::app::render::arrangements::wide_hero::{self, PANE_PAD_X, PANE_PAD_Y};
use crate::app::render::components::hero::{self, HERO_BLOCK_EXTRA_ROWS};
use crate::app::render::components::home_hero;
use crate::app::render::components::home_hero::{HeroData, HomeImagePaint, KeepWatchingHeroLayout};
use crate::app::render::components::home_pills::{home_pill_labels, render_home_pills};
use crate::app::render::components::list_rows::SELECTED_BLOCK_SIDE_PADDING;
use crate::app::types_playback::HomeLatestSource;
use crate::app::ui_util::*;
use mbv_core::playback_queue::QueueItem;
use ratatui::layout::*;
use ratatui::style::*;
use ratatui::widgets::*;
use ratatui::Frame;

/// Output of [`render_home_content`]: painted geometry the caller owns.
/// `hero_area`/`selected_item_rect` are `None` when this render touched no
/// hero / painted no visible selection.
pub(in crate::app) struct HomeContentOutput {
    pub(in crate::app) pill_targets: Vec<(Rect, usize)>,
    pub(in crate::app) image_paint: Option<HomeImagePaint>,
    pub(in crate::app) hero_area: Option<Rect>,
    pub(in crate::app) left_area: Rect,
    pub(in crate::app) selected_item_rect: Option<Rect>,
    /// The `section` actually rendered, after the invalid-section clamp.
    /// `HomeComponent::view()` writes it back into its own section state.
    pub(in crate::app) resolved_section: usize,
}

/// The QueueItem at flat `cursor` in the continue-watching + latest-sections
/// flat ordering (mirrors `App::home_current_item` without `App`).
fn home_item_at(
    continue_items: &[QueueItem],
    latest: &[(String, HomeLatestSource, Vec<QueueItem>)],
    cursor: usize,
) -> Option<QueueItem> {
    continue_items
        .iter()
        .chain(latest.iter().flat_map(|(_, _, i)| i.iter()))
        .nth(cursor)
        .cloned()
}

/// Paints Home's parent-owned hero + section pills + list-surface chrome
/// without `App` (design D2), then mounts the active canonical control
/// (`canonical_list` for Wide hero Wide, `inline_list` for inline Narrow)
/// into the list area and rebuilds the pre-#638 hit map from its exported row
/// geometry. `section` is the already-resolved selected pill; `cursor` is the
/// component's already-clamped flat cursor (used only to pick the hero item
/// and anchor the replacement block). Only the image pixel paint is deferred
/// to the shell.
#[allow(clippy::too_many_arguments)]
pub(in crate::app) fn render_home_content(
    f: &mut Frame,
    area: Rect,
    focused: bool,
    continue_items: &[QueueItem],
    latest: &[(String, HomeLatestSource, Vec<QueueItem>)],
    section: usize,
    cursor: usize,
    canonical_list: &mut WideMediaList<String>,
    inline_list: &InlineMediaBrowser<String>,
    use_nerd_fonts: bool,
    images_enabled: bool,
) -> HomeContentOutput {
    if area.height == 0 || area.width == 0 {
        return HomeContentOutput {
            pill_targets: Vec::new(),
            image_paint: None,
            hero_area: None,
            left_area: Rect::default(),
            selected_item_rect: None,
            resolved_section: section,
        };
    }

    struct Section {
        section_idx: usize,
        items: Vec<QueueItem>,
    }
    let mut new_sections: Vec<Section> = Vec::new();
    for (idx, (_title, _source, items)) in latest.iter().enumerate() {
        new_sections.push(Section {
            section_idx: idx + 1,
            items: items.clone(),
        });
    }

    // The caller resolves which section is *persisted*; a section that no
    // longer exists (e.g. a provider went away) still falls back to the
    // first available new section here, matching the legacy clamp.
    let section = if section != 0 && !new_sections.iter().any(|s| s.section_idx == section) {
        new_sections.first().map(|s| s.section_idx).unwrap_or(0)
    } else {
        section
    };

    let selected_new = new_sections.iter().find(|s| s.section_idx == section);

    // Same threshold the library list uses to switch to two columns, so
    // Home's hero/list split and the library list cross over together.
    let wide_panes = wide_hero::wide_hero_presentation(area);
    let two_column = wide_panes.is_some();
    // Single-column Home's whole panel (content plus the shared tab
    // gutters) is painted green while focused in `render_main`, before
    // this function runs.
    let narrow_pill_areas = wide_hero::pill_bar_areas(area);
    // Wide (Wide hero) still pre-reserves its own pill row above
    // `content_area` (its pills sit at the top of the right pane, a
    // Wide hero concern, `wide_hero_browser_pane`). Narrow
    // inline presentation no longer pre-reserves anything here: its
    // pill row now lives inside `placement-neutral geometry`'s own `pills_area`,
    // outside the selected replacement, same as every other inline browser
    // (design.md decision 6 -- pill *position* is geometry, not a
    // per-screen declaration).
    let content_area = narrow_pill_areas.content_area;

    let control_empty = if section == 0 {
        continue_items.is_empty()
    } else {
        selected_new.is_none_or(|sec| sec.items.is_empty())
    };

    // --- Home hero panel ----------------------------------------------
    // Shared hero above the selected Home list. It reflects the current
    // flat cursor item whether the active pill is Continue Watching or one
    // of the Newest sections. Emby rows keep the full two-column/hero
    // treatment; non-Emby rows (Audiobookshelf today, Feeds in Part 3) get
    // the generic detail block added in Part 2 (#543).
    let current_item = home_item_at(continue_items, latest, cursor);
    let emby_item = current_item
        .as_ref()
        .and_then(|item| item.as_emby().cloned());
    // Hero data: Emby keeps (item, meta_area, wide_area, img_area,
    // meta_layout) — `wide_area` is where overview lines past the
    // image's bottom edge render at full width; the generic detail
    // block renders into a single content area.
    let hero_data: Option<HeroData>;
    // The non-Emby (Audiobookshelf/Feeds) hero item, sized into `hero_content`
    // — a sibling to `hero_data` rather than a shared variant, since generic
    // providers use a different `Hero`-trait-driven measurement path that
    // doesn't converge with Emby's `KeepWatchingHeroLayout` preparation.
    let mut generic_hero: Option<(QueueItem, Rect)> = None;
    let list_area: Rect;
    // Narrow layout's hero shell (area, row count), painted after the
    // pill-gap fill below rather than inline here: `placement-neutral geometry`
    // shifts the hero up into the blank row above `content_area` when
    // one exists, which is the same row the pill-gap fill owns, so the
    // shell must paint last to win that row rather than be painted over.
    let mut narrow_pills_area: Option<Rect> = None;
    let mut narrow_dims: Option<HeroContentDims> = None;
    let mut narrow_desired_hero_rows: u16 = 0;
    let mut hero_area_out: Option<Rect> = None;

    if two_column {
        // Two-column layout: hero pane and browser list (Wide hero,
        // design.md decision 4/5: the pane split and its minimum pane
        // width are the shared arrangement's, not a Home-local ratio).
        let Some(wide_hero::WideHeroPanes {
            hero: hero_panel,
            browser: right_panel,
        }) = wide_panes
        else {
            unreachable!("wide_panes is present when two_column is true");
        };
        hero_area_out = Some(hero_panel);
        let mut hero_content =
            wide_hero::wide_hero_hero_pane(f, area, wide_hero::LeftPaneFocus::ReadOnly)
                .expect("wide branch already confirmed shared hero presentation fits");
        let hero_col_height = hero_content.height;

        hero_data = match emby_item {
            Some(item) => {
                // Shared wide Wide hero card preparation (design.md
                // decision 1): the exact same 16:9-artwork-above-metadata
                // card the wide Movies arrangement renders, so the two
                // cannot drift in image sizing, metadata order, or
                // overview treatment.
                crate::app::render::components::home_hero::prepare_wide_emby_hero_card(
                    &item,
                    hero_content,
                    images_enabled,
                )
                .map(|(meta_layout, meta_area, img_area)| {
                    HeroData::new(
                        Box::new(item),
                        meta_area,
                        meta_area, // wide_area same as meta_area in Wide hero
                        img_area,
                        meta_layout,
                    )
                })
            }
            None => {
                generic_hero = current_item
                    .filter(|item| item.as_emby().is_none())
                    .map(|item| {
                        // Size the generic hero to its actual content
                        // (title/overview text, plus a cover for
                        // Audiobookshelf) instead of the full column height —
                        // otherwise short items (feeds have no cover at all)
                        // leave a mostly-empty panel. Artwork is top-anchored
                        // with the text by `render_home_latest_detail_content`.
                        let text_w = hero_content.width as usize;
                        // The recessed overview box applies the shared pane
                        // padding twice (panel and content), so measure against
                        // its actual text width.
                        let ov_w = text_w;
                        let text =
                            crate::app::render::components::home_latest_row::home_latest_detail_text(
                                &item, text_w, ov_w,
                            );
                        let rows = if matches!(item, QueueItem::Audiobookshelf(_)) {
                            let image_rows =
                                hero_content.width.saturating_mul(9).saturating_add(31) / 32;
                            text.meta_height + 1 + image_rows
                        } else {
                            text.meta_height
                        };
                        hero_content.height = rows.min(hero_col_height);
                        (item, hero_content)
                    });
                None
            }
        };

        list_area = if hero_data.is_some() || generic_hero.is_some() {
            right_panel
        } else {
            // No hero item: list takes full width
            content_area
        };
    } else {
        // Vertical layout: inline presentation (design.md decision 1),
        // reusing the shared reserved-block geometry and the HeroShell
        // (`▁`/`▔`) border every other inline browser already has
        // (decision 2's "Narrow hero shell is uniform" -- Home was the
        // one screen missing it). The image-beside-metadata content wrap
        // itself is unchanged; it already matches the shared shape.
        let max_allowed = content_area.height.saturating_sub(7);
        let inner_w = content_area
            .width
            .saturating_sub(SELECTED_BLOCK_SIDE_PADDING * 2);

        let dims = if area.width < 24 {
            HeroContentDims::None
        } else {
            // Every inline item with a cover -- Emby and the generic
            // Audiobookshelf hero alike -- gets its image-beside-text
            // dims from the same `beside_image_hero_dims` call, so the
            // two providers' layouts cannot drift apart (image sits
            // beside the metadata column, top-aligned; the overview
            // wraps at the narrower meta width while still beside the
            // image, then at the full hero width once past its bottom
            // edge).
            match emby_item {
                    Some(item) => {
                        let show_name = if item.item_type == "Episode" {
                            item.series_name.clone()
                        } else {
                            String::new()
                        };
                        let overview = if item.overview.is_empty() {
                            String::new()
                        } else {
                            trunc_overview(&item.overview)
                        };
                        let (img_w, meta_layout, image_rows) =
                            crate::app::render::components::home_hero::beside_image_hero_dims(
                                &item.name,
                                &show_name,
                                &overview,
                                inner_w,
                                max_allowed,
                                2, // release-date row + duration row
                                images_enabled,
                            );
                        if meta_layout.height < 4 {
                            HeroContentDims::None
                        } else {
                            HeroContentDims::Emby(Box::new(item), img_w, meta_layout, image_rows)
                        }
                    }
                    None => current_item
                        .filter(|item| item.as_emby().is_none())
                        .map(|item| {
                            // Feeds have no cover to sit beside and stay
                            // text-only at the full hero width.
                            let QueueItem::Audiobookshelf(_) = &item else {
                                let text = crate::app::render::components::home_latest_row::home_latest_detail_text(
                                    &item,
                                    inner_w as usize,
                                    inner_w as usize,
                                );
                                return HeroContentDims::Generic(item, text.meta_height);
                            };
                            let layout = crate::app::render::components::home_latest_row::home_latest_detail_text(
                                &item,
                                inner_w as usize,
                                inner_w as usize,
                            );
                            let image_rows = if images_enabled { inner_w.saturating_mul(9).saturating_add(31) / 32 } else { 0 };
                            HeroContentDims::Generic(
                                item,
                                (layout.meta_height + 1 + image_rows).min(max_allowed),
                            )
                        })
                        .unwrap_or(HeroContentDims::None),
                }
        };
        let content_rows = match &dims {
            HeroContentDims::Emby(_, _, meta_layout, image_rows) => {
                meta_layout.height.max(*image_rows)
            }
            HeroContentDims::Generic(_, rows) => *rows,
            HeroContentDims::None => 0,
        };
        // Size the hero from its content; placement and admission are the
        // canonical `InlineMediaBrowser`'s replacement-flow decision, resolved
        // when the control paints below.
        narrow_desired_hero_rows = if content_rows > 0 {
            content_rows + HERO_BLOCK_EXTRA_ROWS
        } else {
            0
        };
        narrow_dims = Some(dims);
        hero_data = None;
        narrow_pills_area = Some(narrow_pill_areas.pills_area);
        list_area = content_area;
    }

    // Wide hero's right pane: pill row at the pane's top, then the
    // list panel below it (design.md decision 6, shared with Music and
    // audiobooks via `wide_hero::wide_hero_browser_pane`). With no hero item
    // there is no right pane at all -- pills span the full row and the
    // list takes the full width, same as the single-column layout.
    let wide_pill_section = two_column && (hero_data.is_some() || generic_hero.is_some());
    let (pills_area, spacer_area, green_panel_full): (Rect, Rect, Option<Rect>) =
        if wide_pill_section {
            let right_area = padded_rect(list_area, 0, PANE_PAD_Y);
            let right_pane = wide_hero::wide_hero_browser_pane(list_area, right_area);
            (
                right_pane.pills_area,
                right_pane.spacer_area,
                Some(right_pane.list_panel),
            )
        } else if two_column {
            // Wide layout, no hero item: same top-of-area fallback the
            // Wide hero pane would have used.
            let areas = wide_hero::pill_bar_areas(area);
            (areas.pills_area, areas.spacer_area, None)
        } else {
            // Narrow: section pills stay outside the selected detail flow.
            (
                narrow_pills_area.unwrap_or_default(),
                narrow_pill_areas.spacer_area,
                None,
            )
        };
    let labels = home_pill_labels(latest);
    let pill_targets = render_home_pills(f, pills_area, &labels, section);

    let list_area = if let Some(list_panel) = green_panel_full {
        let panel_bg = palette::resolve_surface_focus(focused);
        f.render_widget(
            Block::default().style(Style::default().bg(panel_bg)),
            list_panel,
        );
        // `list_area` is the inset content rect (row/hit geometry); the
        // painter is handed a full-width, vertically-inset paint rect.
        padded_rect(list_panel, PANE_PAD_X, PANE_PAD_Y)
    } else {
        // Narrow Home leaves the `chrome.rs` SURFACE_BACKDROP showing behind
        // its rows (Movies narrow parity); the inline hero shell then reads as
        // a recessed card against it. Reverts the 14fb8435 pane flood.
        list_area
    };
    // The selected row's full-width background fill uses this rect in
    // both layouts — the wide layout's dedicated green panel, or (with
    // no separate panel) `list_area` itself in the single-column
    // layout — so the selected row always gets the same full-row
    // highlight style. `green_panel_full` alone stays `None` in the
    // single-column layout since it also drives the wide panel's
    // top/bottom border rule, which the single-column layout doesn't
    // have.
    // Selected-row highlight colour: the row punches through to the surface
    // containing the list panel, which is a resting surface in both layouts
    // (the wide list panel is focus-green, but its parent container is not).
    let selection_bg = palette::list_selected_row_bg();

    // Keep the row immediately below the Home pill bar free of list text.
    // The wide layout uses the list panel surface; the single-column
    // layout inherits the ordinary library panel surface (no green
    // focus fill -- Home's panel background matches every other
    // inline browser's regardless of focus).
    if spacer_area.y < area.bottom() && spacer_area.width > 0 {
        let panel_bg = palette::SURFACE_BACKDROP;
        f.render_widget(
            Paragraph::new(" ".repeat(spacer_area.width as usize))
                .style(Style::default().bg(panel_bg)),
            spacer_area,
        );
    }

    let left_area = list_area;
    let mut image_paint = None;
    // Two-column: the Wide hero card paints independently of the list flow
    // (its geometry was resolved above, before the pill/list split).
    if two_column {
        if let Some(hero_data) = &hero_data {
            image_paint =
                home_hero::render_home_hero_content(f, hero_data, true, focused, use_nerd_fonts);
        } else if let Some((item, area)) = &generic_hero {
            image_paint = home_hero::render_generic_hero_content(
                f,
                item,
                *area,
                focused,
                use_nerd_fonts,
                images_enabled,
            );
        }
    }

    // Frame the wide rail before the row flow: the helper fills the whole
    // panel background, so it must run before the canonical control paints
    // the selected-row bar (matches TV / Music ordering).
    if let Some(panel) = green_panel_full {
        wide_hero::wide_hero_browser_border(f, panel, focused);
    }

    // Paint the active canonical control into the list area. Row identity for
    // the mouse path comes from the control's own `resolve_point` (#638), not
    // a parent hit map.
    let selected_item_rect = if control_empty {
        crate::app::render::render_placeholder(f, list_area, " (empty)");
        None
    } else if two_column {
        // Full panel width so the selected-row bar and flush marker reach the
        // rail border; `list_area` is already inset vertically and stays the
        // hit/scroll geometry rect.
        let paint_rect = Rect {
            x: green_panel_full.map_or(list_area.x, |panel| panel.x),
            width: green_panel_full.map_or(list_area.width, |panel| panel.width),
            ..list_area
        };
        let paint = super::media_list::render_wide_media_list(
            f,
            paint_rect,
            list_area,
            canonical_list,
            focused,
            selection_bg,
        );
        paint.selected_row_rect
    } else {
        let result = super::media_list::render_inline_media_browser(
            f,
            list_area,
            inline_list,
            narrow_desired_hero_rows as usize,
            focused,
            selection_bg,
        );
        match result.hero_area {
            Some(hero_area) => {
                hero_area_out = Some(hero_area);
                hero::selected_detail_shell(f, hero_area, hero_area.height, focused);
                let hero_content = library_arrangement::selected_detail_content_area(
                    hero_area,
                    SELECTED_BLOCK_SIDE_PADDING,
                    HERO_BLOCK_EXTRA_ROWS,
                );
                match narrow_dims
                    .take()
                    .and_then(|dims| narrow_hero_data(dims, hero_content, images_enabled))
                {
                    Some(NarrowHeroPaint::Emby(hero_data)) => {
                        image_paint = home_hero::render_home_hero_content(
                            f,
                            &hero_data,
                            false,
                            focused,
                            use_nerd_fonts,
                        );
                    }
                    Some(NarrowHeroPaint::Generic(item, area)) => {
                        image_paint = home_hero::render_generic_hero_content(
                            f,
                            &item,
                            area,
                            focused,
                            use_nerd_fonts,
                            images_enabled,
                        );
                    }
                    None => {}
                }
                Some(hero_area)
            }
            None => result.row_geometry.selected_row_rect(list_area),
        }
    };

    HomeContentOutput {
        pill_targets,
        image_paint,
        hero_area: hero_area_out,
        left_area,
        selected_item_rect,
        resolved_section: section,
    }
}

/// Sized hero content for the narrow inline flow, resolved before the control
/// admits (or rejects) the replacement block.
enum HeroContentDims {
    Emby(
        Box<mbv_core::api::EmbyItem>,
        u16,
        KeepWatchingHeroLayout,
        u16,
    ),
    // Feed and Audiobookshelf use the shared stacked detail block;
    // Audiobookshelf artwork is painted above its metadata; Feed stays
    // text-only in the shared renderer.
    Generic(QueueItem, u16),
    None,
}

/// The narrow hero to paint once the canonical control has resolved the
/// on-screen detail-block rect: Emby's `HeroData`, or a generic (non-Emby)
/// item + its content area for [`home_hero::render_generic_hero_content`].
enum NarrowHeroPaint {
    Emby(HeroData),
    Generic(QueueItem, Rect),
}

/// Build the parent-owned narrow hero paint once the canonical control has
/// resolved the on-screen detail-block rect.
fn narrow_hero_data(
    dims: HeroContentDims,
    hero_content: Rect,
    images_enabled: bool,
) -> Option<NarrowHeroPaint> {
    match dims {
        HeroContentDims::Emby(item, img_w, meta_layout, image_rows) => {
            let (meta_area, img_area) =
                crate::app::render::components::home_hero::beside_image_hero_rects(
                    hero_content,
                    img_w,
                    meta_layout.height,
                    image_rows,
                    images_enabled,
                );
            Some(NarrowHeroPaint::Emby(HeroData::new(
                item,
                meta_area,
                hero_content,
                Some(img_area),
                meta_layout,
            )))
        }
        HeroContentDims::Generic(item, _) => Some(NarrowHeroPaint::Generic(item, hero_content)),
        HeroContentDims::None => None,
    }
}
