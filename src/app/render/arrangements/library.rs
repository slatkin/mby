use super::{padded_rect, wide_hero};
use ratatui::layout::Rect;

/// The shared padded panes used by wide library presentations.
pub(in crate::app) struct WideLibraryPanes {
    pub left_panel: Rect,
    pub right_panel: Rect,
    pub left_area: Rect,
    pub right_area: Rect,
}

pub(in crate::app) fn wide_library_panes(
    area: Rect,
    pad_x: u16,
    pad_y: u16,
) -> Option<WideLibraryPanes> {
    let wide_hero::WideHeroPanes {
        hero: left_panel,
        browser: right_panel,
    } = wide_hero::wide_hero_presentation(area)?;
    let left_area = padded_rect(left_panel, pad_x, pad_y);
    let right_area = Rect {
        x: right_panel.x,
        y: right_panel.y.saturating_add(pad_y),
        width: right_panel.width,
        height: right_panel.height.saturating_sub(pad_y * 2),
    };
    Some(WideLibraryPanes {
        left_panel,
        right_panel,
        left_area,
        right_area,
    })
}

pub(in crate::app::render) fn selected_detail_content_area(
    hero_area: Rect,
    side_padding: u16,
    extra_rows: u16,
) -> Rect {
    Rect {
        x: hero_area.x.saturating_add(side_padding),
        y: hero_area.y.saturating_add(2),
        width: hero_area.width.saturating_sub(side_padding * 2),
        height: hero_area.height.saturating_sub(extra_rows),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::render::arrangements::wide_hero::WIDE_HERO_MIN_AREA_HEIGHT;

    #[test]
    fn wide_library_preserves_breakpoint_and_padding() {
        let area = Rect {
            x: 2,
            y: 3,
            width: crate::app::TWO_COLUMN_THRESHOLD,
            height: WIDE_HERO_MIN_AREA_HEIGHT + 1,
        };
        let panes = wide_library_panes(area, 2, 1).expect("wide area");
        assert_eq!(panes.left_area.x, panes.left_panel.x + 2);
        assert_eq!(panes.right_area.y, panes.right_panel.y + 1);
        assert!(wide_library_panes(
            Rect {
                height: WIDE_HERO_MIN_AREA_HEIGHT,
                ..area
            },
            2,
            1,
        )
        .is_none());
    }

    #[test]
    fn selected_detail_content_saturates_zero_area() {
        assert_eq!(
            selected_detail_content_area(
                Rect {
                    width: 3,
                    height: 1,
                    ..Rect::default()
                },
                2,
                4
            ),
            Rect {
                x: 2,
                y: 2,
                width: 0,
                height: 0
            },
        );
    }
}
