use super::padded_rect;
use super::wide_hero::{hero_artwork_slot, PANE_PAD_X, PANE_PAD_Y};
use crate::app::render::components::album_art::{INLINE_ALBUM_ART_RESERVED, INLINE_ALBUM_ART_ROWS};
use ratatui::layout::Rect;

pub(in crate::app::render) const MIN_LEFT_HEIGHT_FOR_SEPARATOR: u16 = 6;
pub(in crate::app::render) const MIN_HERO_METADATA_SIDE_WIDTH: u16 = 15;

pub(in crate::app) struct WideMusicLeftLayout {
    pub hero_area: Rect,
    pub track_area: Rect,
    pub art_area: Rect,
    pub text_area: Rect,
    pub stack_metadata: bool,
}

pub(in crate::app::render) fn wide_music_left_layout(
    left_area: Rect,
    images_enabled: bool,
    track_count: usize,
) -> WideMusicLeftLayout {
    let total_h = left_area.height;
    let top_pad = PANE_PAD_Y;
    let hero_content_area = padded_rect(left_area, PANE_PAD_X, top_pad);
    let art_available = images_enabled && hero_content_area.width >= INLINE_ALBUM_ART_RESERVED;
    let side_metadata_width = hero_content_area
        .width
        .saturating_sub(INLINE_ALBUM_ART_RESERVED);
    let stack_metadata = art_available && side_metadata_width < MIN_HERO_METADATA_SIDE_WIDTH;
    let sep = (total_h > MIN_LEFT_HEIGHT_FOR_SEPARATOR) as u16;
    let track_rows = track_count.max(1) as u16;
    let requested_track_h = track_rows.saturating_add(PANE_PAD_Y * 2);
    let hero_ideal = if art_available {
        INLINE_ALBUM_ART_ROWS.saturating_add(if stack_metadata { 4 } else { 0 })
    } else {
        2
    }
    .min(total_h.saturating_sub(top_pad + sep + PANE_PAD_Y * 2));
    let track_h = requested_track_h.min(total_h.saturating_sub(top_pad + hero_ideal + sep));
    let hero_h = hero_ideal.min(total_h.saturating_sub(top_pad + track_h + sep));
    let hero_area = Rect {
        x: hero_content_area.x,
        y: hero_content_area.y,
        width: hero_content_area.width,
        height: hero_h,
    };
    let track_area = Rect {
        x: left_area.x,
        y: left_area.y + top_pad + hero_h + sep,
        width: left_area.width,
        height: track_h,
    };
    let art_area = hero_artwork_slot(
        if art_available && hero_area.width >= INLINE_ALBUM_ART_RESERVED {
            let art_width = if stack_metadata {
                hero_area.width
            } else {
                INLINE_ALBUM_ART_RESERVED
            };
            Rect {
                x: if stack_metadata {
                    hero_area.x
                } else {
                    hero_area.x.saturating_add(
                        hero_area
                            .width
                            .saturating_sub(INLINE_ALBUM_ART_RESERVED)
                            .saturating_add(PANE_PAD_X),
                    )
                },
                y: hero_area.y,
                width: art_width,
                height: if stack_metadata {
                    INLINE_ALBUM_ART_ROWS.min(hero_area.height)
                } else {
                    hero_area.height
                },
            }
        } else {
            Rect::default()
        },
        images_enabled,
    )
    .unwrap_or_default();
    let text_area = if stack_metadata {
        Rect {
            x: hero_area.x,
            y: hero_area
                .y
                .saturating_add(art_area.height)
                .saturating_add(1),
            width: hero_area.width,
            height: hero_area
                .height
                .saturating_sub(art_area.height.saturating_add(1)),
        }
    } else {
        Rect {
            width: hero_area.width.saturating_sub(art_area.width),
            ..hero_area
        }
    };
    WideMusicLeftLayout {
        hero_area,
        track_area,
        art_area,
        text_area,
        stack_metadata,
    }
}
