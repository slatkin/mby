use crate::app::App;
use mbv_core::playback_queue::QueueItem;
use ratatui::layout::Rect;
use ratatui::text::Span;
use ratatui::Frame;
use textwrap::wrap;

use super::hero_model::{Hero, HeroArtwork};

/// The two-column (wide) hero's original 2-col horizontal padding around
/// the overview text block. The single-column hero has none (flush with
/// the title above it).
pub(in crate::app::render) const WIDE_OVERVIEW_PAD: usize = 2;

/// Pre-wrapped content for an inline item's metadata column, plus the
/// total row count it needs. Computed once (mirroring `compact_banner_layout`'s
/// measure-before-render pattern) so the caller can size the panel to fit
/// before rendering, and so the title and overview are wrapped exactly once
/// per frame rather than once to measure and again to render. Shared by the
/// Emby Keep Watching hero and the generic Audiobookshelf hero -- both are
/// beside-image, inline items and use the same wrap-around-the-image
/// shape.
pub(in crate::app) struct KeepWatchingHeroLayout {
    title_lines: Vec<String>,
    show_name: String,
    /// Overview text lines with a per-line flag: `true` once the line has
    /// wrapped past the image's row extent and reclaims the full hero
    /// width (the image no longer occupies that row), `false` while beside
    /// the image at the narrower meta-column width.
    overview_lines: Vec<(String, bool)>,
    pub(in crate::app::render) height: u16,
}

/// Per-provider metadata for the shared inline meta block: an optional
/// glyph drawn one space after the last title line (Emby's watch-state icon),
/// plus the metadata rows below the subtitle (release date, duration, ...).
pub(in crate::app::render) struct HeroMetaBlock {
    pub title_suffix: Option<Span<'static>>,
    pub meta_rows: Vec<Vec<Span<'static>>>,
}

/// Prepares a wide (Wide hero) selected-Emby hero card from `item`,
/// sized into the given content area (the left pane's inner rect after
/// padding). Returns the data needed to build `HeroData::Emby`, or `None`
/// when the area is too small for a usable card (image and metadata).
/// Shared by Home's wide branch and the wide Movies arrangement so the two
/// render the exact same 16:9-artwork-above-metadata card: image occupies
/// the top of the content area at 16:9, metadata (title, show name,
/// release date, duration, overview) below it. The metadata layout uses the
/// full content width for both narrow and wide wrapping (no wrap-around
/// split — the image sits above text, not beside it), matching Home's wide
/// Wide hero presentation.
pub(in crate::app) fn prepare_wide_emby_hero_card(
    item: &mbv_core::api::EmbyItem,
    content_area: Rect,
    images_enabled: bool,
) -> Option<(KeepWatchingHeroLayout, Rect, Option<Rect>)> {
    let meta_w = content_area.width as usize;
    let meta_layout = App::keep_watching_hero_layout(item, meta_w, meta_w, 0, WIDE_OVERVIEW_PAD);
    if meta_layout.height < 4 {
        return None;
    }
    if !images_enabled {
        let meta_height = meta_layout.height.min(content_area.height);
        return Some((
            meta_layout,
            Rect {
                x: content_area.x,
                y: content_area.y,
                width: content_area.width,
                height: meta_height,
            },
            None,
        ));
    }
    // Preserve the image-on geometry: the image reserves its original
    // 16:9 budget and the metadata keeps its full measured height.
    let image_height = (content_area.width.saturating_mul(9).saturating_add(31) / 32)
        .max(1)
        .min(content_area.height.saturating_sub(meta_layout.height));
    if image_height == 0 {
        return None;
    }
    let img_area = Rect {
        x: content_area.x,
        y: content_area.y,
        width: content_area.width,
        height: image_height,
    };
    let img_area =
        super::super::arrangements::wide_hero::hero_artwork_slot(img_area, images_enabled);
    let meta_y = img_area.map_or(content_area.y, |area| area.bottom() + 1);
    let meta_area = Rect {
        x: content_area.x,
        y: meta_y,
        width: content_area.width,
        height: meta_layout.height,
    };
    Some((meta_layout, meta_area, img_area))
}

pub(in crate::app) struct HeroData {
    item: Box<mbv_core::api::EmbyItem>,
    meta_area: Rect,
    wide_area: Rect,
    img_area: Option<Rect>,
    meta_layout: KeepWatchingHeroLayout,
}

impl HeroData {
    pub(in crate::app) fn new(
        item: Box<mbv_core::api::EmbyItem>,
        meta_area: Rect,
        wide_area: Rect,
        img_area: Option<Rect>,
        meta_layout: KeepWatchingHeroLayout,
    ) -> Self {
        Self {
            item,
            meta_area,
            wide_area,
            img_area,
            meta_layout,
        }
    }

    fn render_content(
        &self,
        f: &mut Frame,
        two_column: bool,
        focused: bool,
        use_nerd_fonts: bool,
    ) -> Option<HomeImagePaint> {
        let meta_block =
            App::keep_watching_hero_meta_block(&self.item, self.meta_area.width, use_nerd_fonts);
        render_hero_layout_meta_content(
            f,
            self.meta_area,
            self.wide_area,
            &self.meta_layout,
            meta_block,
            if two_column {
                WIDE_OVERVIEW_PAD as u16
            } else {
                0
            },
            focused,
            use_nerd_fonts,
            self.item.as_ref(),
        );
        self.img_area.map(|area| HomeImagePaint::Emby {
            area,
            item: self.item.clone(),
            centered: two_column,
        })
    }
}

/// Renders a generic (non-Emby) inline hero's content: title/metadata/
/// overview beside its cover (if any), plus the cover image request (if
/// any) still needing paint. Mirrors [`HeroData::render_content`] for the
/// Emby case, but generic providers (Audiobookshelf, Feeds) use a
/// different `Hero`-trait-driven measurement path that doesn't converge
/// with Emby's `KeepWatchingHeroLayout` preparation, so the two stay
/// separate rather than share one enum/match.
pub(in crate::app) fn render_generic_hero_content(
    f: &mut Frame,
    item: &QueueItem,
    area: Rect,
    focused: bool,
    use_nerd_fonts: bool,
    images_enabled: bool,
) -> Option<HomeImagePaint> {
    let hero: &dyn Hero = item;
    let overview = hero.description().unwrap_or_default();
    let (img_w, layout, image_rows) = beside_image_hero_dims(
        hero.title(),
        hero.subtitle().unwrap_or_default(),
        &overview,
        area.width,
        area.height,
        hero.meta_rows(area.width).len() as u16,
        images_enabled,
    );
    let (meta_area, image_area) =
        beside_image_hero_rects(area, img_w, layout.height, image_rows, images_enabled);
    render_hero_layout_meta_content(
        f,
        meta_area,
        area,
        &layout,
        HeroMetaBlock {
            title_suffix: hero.title_suffix(),
            meta_rows: hero.meta_rows(meta_area.width),
        },
        0,
        focused,
        use_nerd_fonts,
        hero,
    );
    match hero.artwork() {
        HeroArtwork::Image { item_id, .. } if images_enabled => {
            let image = match item {
                QueueItem::Audiobookshelf(_) => HomeImagePaint::AudiobookshelfCover {
                    area: image_area,
                    library_item_id: item_id.to_owned(),
                    show_placeholder: true,
                },
                QueueItem::AudiobookshelfBook(_) => HomeImagePaint::AudiobookshelfBookCover {
                    area: image_area,
                    library_item_id: item_id.to_owned(),
                },
                _ => return None,
            };
            Some(image)
        }
        HeroArtwork::Placeholder if images_enabled => {
            super::artwork_placeholder::render_artwork_placeholder(f, image_area);
            None
        }
        _ => None,
    }
}

/// The image an in-progress Home hero render needs painted, computed
/// without `App` (design D2): the shell on the `HomeComponent`'s behalf
/// fetches/looks up the cached protocol and paints it into `area` using App's
/// image-cache authority right after `view()` returns (task 3.4's confirmed
/// extraction: share orchestration, defer only the pixel paint).
pub(in crate::app) enum HomeImagePaint {
    Emby {
        area: Rect,
        item: Box<mbv_core::api::EmbyItem>,
        centered: bool,
    },
    Series {
        area: Rect,
        item: Box<mbv_core::api::EmbyItem>,
        show_placeholder: bool,
        /// Ordered Emby image-type candidate chain to fetch, so wide TV's
        /// landscape hero can request the `Thumb`-first chain while other
        /// callers keep the narrow inline detail's `&["Primary"]`.
        image_types: &'static [&'static str],
    },
    /// The compact movie/Series detail banner's poster. Painted byte-identically
    /// to the legacy inline `render_compact_detail` block: a dim placeholder
    /// while `show_placeholder`, else the cached protocol rendered straight into
    /// `area` (no `fetch_*` -- the prefetch loop owns fetching, #287).
    CompactBanner {
        area: Rect,
        item: Box<mbv_core::api::EmbyItem>,
        show_placeholder: bool,
    },
    AudiobookshelfCover {
        area: Rect,
        library_item_id: String,
        /// `true` for the narrow beside-image hero (`GenericBeside`), which
        /// always shows the dim placeholder while uncached, matching every
        /// other beside-image hero; `false` for the two-column/text `Generic`
        /// detail block, which renders nothing until the cover is cached (an
        /// existing, preserved difference between the two call sites).
        show_placeholder: bool,
    },
    /// Audiobookshelf book artwork must stay isolated from podcast artwork,
    /// including when both use the same library item ID (book-browsing spec
    /// line 124).
    AudiobookshelfBookCover { area: Rect, library_item_id: String },
}

fn render_hero_layout_meta_content(
    f: &mut Frame,
    area: Rect,
    wide_area: Rect,
    layout: &KeepWatchingHeroLayout,
    meta_block: HeroMetaBlock,
    overview_pad: u16,
    focused: bool,
    use_nerd_fonts: bool,
    hero: &dyn Hero,
) {
    // Preserve the precomputed Nerd Font glyphs; Emby's Hero suffix is the
    // ordinary-Unicode fallback and must not shadow them.
    let title_suffix = if use_nerd_fonts {
        meta_block.title_suffix
    } else {
        hero.title_suffix().or(meta_block.title_suffix)
    };
    super::hero::render_home_hero_meta_block(
        f,
        area,
        wide_area,
        &layout.title_lines,
        hero.subtitle().unwrap_or(&layout.show_name),
        title_suffix,
        meta_block.meta_rows,
        &layout.overview_lines,
        overview_pad,
        focused,
    );
}

/// Renders an Emby Home hero's non-image content (title/meta/overview text)
/// without `App`, returning the cover image (if any) still needing paint for
/// the `HomeComponent` render path (task 3.4's confirmed extraction). See
/// [`render_generic_hero_content`] for the non-Emby equivalent.
pub(in crate::app) fn render_home_hero_content(
    f: &mut Frame,
    hero_data: &HeroData,
    two_column: bool,
    focused: bool,
    use_nerd_fonts: bool,
) -> Option<HomeImagePaint> {
    hero_data.render_content(f, two_column, focused, use_nerd_fonts)
}

/// Beside-image inline dims: image width, the wrap-around text layout,
/// and the image's row count. The single source of this geometry for every
/// inline item with a cover -- Emby Keep Watching and the generic
/// Audiobookshelf hero both call this so their layouts can't drift apart.
pub(in crate::app::render) fn beside_image_hero_dims(
    title: &str,
    show_name: &str,
    overview: &str,
    inner_w: u16,
    max_allowed: u16,
    meta_row_count: u16,
    images_enabled: bool,
) -> (u16, KeepWatchingHeroLayout, u16) {
    let img_w = if images_enabled { inner_w / 2 } else { 0 };
    let meta_w = inner_w.saturating_sub(img_w + 1) as usize;
    let image_rows = (img_w.saturating_mul(9).saturating_add(31) / 32).min(max_allowed);
    let layout = hero_text_layout(
        title,
        show_name,
        overview,
        meta_w,
        inner_w as usize,
        image_rows,
        0,
        meta_row_count,
    );
    (img_w, layout, image_rows)
}

/// Beside-image inline `Rect`s: the metadata column (left) and image
/// column (right), both stretched to the taller of the two so the shorter
/// one's background/border still spans the full row height. The single
/// source of this geometry, shared the same way as [`beside_image_hero_dims`].
pub(in crate::app::render) fn beside_image_hero_rects(
    hero_content: Rect,
    img_w: u16,
    layout_height: u16,
    image_rows: u16,
    images_enabled: bool,
) -> (Rect, Rect) {
    // Clamp to `hero_content.height` -- the panel's actual granted height,
    // which `placement-neutral geometry` can clamp smaller than what `image_rows`/
    // `layout_height` asked for when the terminal doesn't have room for
    // everything requested. Sizing the image/meta `Rect`s from the desired
    // height alone (unclamped) lets them extend past the hero panel's real
    // bottom edge, where the image's overflow gets drawn over by whatever
    // renders below it (pills/list) -- looking like the image is cut off.
    let hero_height = image_rows.max(layout_height).min(hero_content.height);
    if !images_enabled {
        return (hero_content, Rect::default());
    }
    let meta_area = Rect {
        x: hero_content.x,
        y: hero_content.y,
        width: hero_content.width.saturating_sub(img_w + 1),
        height: hero_height,
    };
    let img_area = Rect {
        x: hero_content.x + hero_content.width.saturating_sub(img_w),
        y: hero_content.y,
        width: img_w,
        height: hero_height,
    };
    (meta_area, img_area)
}

/// Wrap-around-the-image text layout shared by every inline item with a
/// beside-text image: title wrap lines, then one row each for the show-name
/// line, the duration/progress line, and the blank separator, then the
/// wrapped overview. The overview wraps around the image: it wraps at
/// `text_w` (the meta column, beside the image) for however many of its rows
/// still fall within `image_rows`, then reclaims the full `wide_w` for any
/// remaining rows once past the image's bottom edge. `overview_pad` is the
/// two-column (wide) hero's original 2-col horizontal padding around the
/// overview block; the single-column hero passes 0 so its overview stays
/// flush with the title above it. `meta_row_count` is the number of reserved
/// metadata rows below the title/show-name (Emby's hero now uses 2: one for
/// the release date, one for the duration; other heroes use 1).
pub(in crate::app::render) fn hero_text_layout(
    title: &str,
    show_name: &str,
    overview: &str,
    text_w: usize,
    wide_w: usize,
    image_rows: u16,
    overview_pad: usize,
    meta_row_count: u16,
) -> KeepWatchingHeroLayout {
    if text_w == 0 {
        return KeepWatchingHeroLayout {
            title_lines: Vec::new(),
            show_name: String::new(),
            overview_lines: Vec::new(),
            height: 0,
        };
    }
    let title_lines: Vec<String> = wrap(title, text_w)
        .into_iter()
        .map(|s| s.into_owned())
        .collect();
    let header_rows = title_lines.len() as u16
        + if show_name.is_empty() { 0 } else { 1 } // show name row (only for episodes)
        + meta_row_count // metadata rows (release date, duration, ...)
        + 1; // blank separator row
    let ov_text_w = text_w.saturating_sub(overview_pad * 2);
    let ov_wide_w = wide_w.saturating_sub(overview_pad * 2);
    let overview_lines: Vec<(String, bool)> = if overview.is_empty() {
        Vec::new()
    } else {
        let narrow_capacity = image_rows.saturating_sub(header_rows) as usize;
        if narrow_capacity == 0 {
            wrap(overview, ov_wide_w.max(1))
                .into_iter()
                .map(|s| (s.into_owned(), true))
                .collect()
        } else {
            let narrow_all: Vec<String> = wrap(overview, ov_text_w)
                .into_iter()
                .map(|s| s.into_owned())
                .collect();
            if narrow_all.len() <= narrow_capacity {
                narrow_all.into_iter().map(|l| (l, false)).collect()
            } else {
                let consumed_words: usize = narrow_all[..narrow_capacity]
                    .iter()
                    .map(|l| l.split_whitespace().count())
                    .sum();
                let remainder: String = overview
                    .split_whitespace()
                    .skip(consumed_words)
                    .collect::<Vec<_>>()
                    .join(" ");
                let mut lines: Vec<(String, bool)> = narrow_all[..narrow_capacity]
                    .iter()
                    .cloned()
                    .map(|l| (l, false))
                    .collect();
                lines.extend(
                    wrap(&remainder, ov_wide_w.max(1))
                        .into_iter()
                        .map(|s| (s.into_owned(), true)),
                );
                lines
            }
        }
    };
    let height = header_rows
        + if overview_lines.is_empty() {
            0
        } else {
            overview_lines.len() as u16
                + 1 // overview lines + bottom pad
                + if overview_pad > 0 {
                    1 // gap row above the Wide hero overview box
                } else {
                    0
                }
        };
    KeepWatchingHeroLayout {
        title_lines,
        show_name: show_name.to_string(),
        overview_lines,
        height,
    }
}

#[cfg(test)]
mod tests {
    use super::prepare_wide_emby_hero_card;
    use crate::app::render::make_movie_app;
    use ratatui::layout::Rect;

    #[test]
    fn wide_emby_hero_without_images_starts_full_width_meta_at_content_start() {
        let item = make_movie_app().libs[0].nav_stack[0].items[0].clone();
        let content = Rect::new(4, 3, 80, 30);
        let (layout, meta, artwork) = prepare_wide_emby_hero_card(&item, content, false).unwrap();

        assert!(artwork.is_none());
        assert_eq!(meta.x, content.x);
        assert_eq!(meta.y, content.y);
        assert_eq!(meta.width, content.width);
        assert_eq!(meta.height, layout.height.min(content.height));
    }

    #[test]
    fn wide_emby_hero_with_images_preserves_artwork_and_meta_dimensions() {
        let item = make_movie_app().libs[0].nav_stack[0].items[0].clone();
        let content = Rect::new(4, 3, 80, 40);
        let (layout, meta, artwork) = prepare_wide_emby_hero_card(&item, content, true).unwrap();
        let expected_image_height = (content.width * 9).div_ceil(32);

        let artwork = artwork.unwrap();
        assert_eq!(
            artwork.height,
            expected_image_height.min(content.height - layout.height)
        );
        assert_eq!(meta.y, artwork.bottom() + 1);
        assert_eq!(meta.width, content.width);
        assert_eq!(meta.height, layout.height);
    }
}
