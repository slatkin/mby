use super::{padded_rect, wide_hero};
use ratatui::layout::Rect;

/// The shared padded panes used by wide library presentations.
pub(in crate::app) struct WideLibraryPanes {
    pub hero_panel: Rect,
    pub browser_panel: Rect,
    pub hero_area: Rect,
    pub browser_area: Rect,
}

pub(in crate::app) fn wide_library_panes(
    area: Rect,
    pad_x: u16,
    pad_y: u16,
) -> Option<WideLibraryPanes> {
    let wide_hero::WideHeroPanes {
        hero: hero_panel,
        browser: browser_panel,
    } = wide_hero::wide_hero_presentation(area)?;
    let hero_area = padded_rect(hero_panel, pad_x, pad_y);
    let browser_area = Rect {
        x: browser_panel.x,
        y: browser_panel.y.saturating_add(pad_y),
        width: browser_panel.width,
        height: browser_panel.height.saturating_sub(pad_y * 2),
    };
    Some(WideLibraryPanes {
        hero_panel,
        browser_panel,
        hero_area,
        browser_area,
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
        assert_eq!(panes.hero_area.x, panes.hero_panel.x + 2);
        assert_eq!(panes.browser_area.y, panes.browser_panel.y + 1);
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
