//! Wide hero arrangement: geometry, borders, recessed content blocks,
//! and wrapped-text rendering for the two-pane layout shared by Music,
//! Home, and future screens (design.md decisions 4–6).

use super::padded_rect;
use crate::app::palette;
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Paragraph};
use ratatui::Frame;
use textwrap::wrap;

/// Minimum outer content-area height for the Wide hero arrangement's
/// two-pane split.
pub(in crate::app::render) const WIDE_HERO_MIN_AREA_HEIGHT: u16 = 6;
/// Minimum width either Wide hero pane may shrink to (decision 5's
/// minimum pane width).
const WIDE_HERO_MIN_PANE_WIDTH: u16 = 40;
/// Empty columns separating the Wide hero arrangement's two panes.
const WIDE_HERO_PANE_GAP: u16 = 2;
/// Height of the pill row at the top of the Wide hero arrangement's left
/// (list) pane.
const WIDE_HERO_PILLS_ROW_HEIGHT: u16 = 1;
/// Blank rows below the pill row before the list starts.
const WIDE_HERO_PILLS_GAP_ROWS: u16 = 1;

/// Symmetric interior padding shared by every Wide hero surface's panes
/// (hero content, list panel, recessed boxes). One definition; surfaces that
/// previously carried their own `PANE_PAD_X`/`PANE_PAD_Y` (or `HOME_HERO_PAD_*`)
/// copy now import these.
pub(in crate::app) const PANE_PAD_X: u16 = 2;
pub(in crate::app) const PANE_PAD_Y: u16 = 1;

/// Resolves the only shared responsive decision for hero-bearing browsers and
/// returns the pane geometry when the wide presentation fits. Callers provide
/// content; they do not own a breakpoint or a height threshold.
///
/// This primitive owns the one-row status-bar reserve: both returned panes
/// already exclude the terminal's bottom status row, so every Wide hero
/// screen bottoms out exactly one row above it. Callers must not re-derive
/// that reserve (no extra `saturating_sub(1)` on the panes, no `bottom_pad`
/// on `wide_hero_browser_pane`) — doing so double-subtracts and shifts the
/// screen a second row.
///
/// Geometry is returned by semantic role: `hero` is the larger (~60%)
/// hero/workspace pane on the right, `browser` is the ~40% list pane on the
/// left.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::app) struct WideHeroPanes {
    pub browser: Rect,
    pub hero: Rect,
}

pub(in crate::app) fn wide_hero_presentation(content_area: Rect) -> Option<WideHeroPanes> {
    (content_area.width >= crate::app::TWO_COLUMN_THRESHOLD
        && content_area.height.saturating_sub(1) >= WIDE_HERO_MIN_AREA_HEIGHT)
        .then(|| {
            let (mut browser, mut hero) = wide_hero_split(content_area);
            hero.height = hero.height.saturating_sub(1);
            browser.height = browser.height.saturating_sub(1);
            WideHeroPanes { browser, hero }
        })
}

#[cfg(test)]
#[allow(clippy::items_after_test_module)]
mod tests {
    use super::*;

    /// The primitive owns the one-row status-row reserve (task 5.1, D7):
    /// both returned panes already exclude the terminal's bottom status row,
    /// so callers must not shrink them again.
    #[test]
    fn shared_presentation_reserves_one_status_row_on_both_panes() {
        let area = Rect {
            x: 2,
            y: 4,
            width: crate::app::TWO_COLUMN_THRESHOLD,
            height: 20,
        };
        let WideHeroPanes {
            hero: left,
            browser: right,
        } = wide_hero_presentation(area).expect("wide area");
        assert_eq!(left.height, area.height - 1);
        assert_eq!(right.height, area.height - 1);
        assert_eq!(left.bottom(), area.bottom() - 1);
        assert_eq!(right.bottom(), area.bottom() - 1);
    }

    #[test]
    fn pill_and_right_pane_geometry_saturate_short_areas() {
        let areas = pill_bar_areas(Rect {
            x: 4,
            y: 7,
            width: 10,
            height: 1,
        });
        assert_eq!(areas.pills_area.height, 1);
        assert_eq!(areas.spacer_area.height, 0);
        assert_eq!(areas.content_area.height, 0);

        let right = wide_hero_browser_pane(
            Rect {
                x: 20,
                y: 3,
                width: 10,
                height: 1,
            },
            Rect {
                x: 20,
                y: 3,
                width: 10,
                height: 1,
            },
        );
        assert_eq!(right.list_panel.height, 0);
    }
}

/// Returns `(browser_pane, hero_pane)` for the Wide hero arrangement's
/// horizontal split: a `WIDE_HERO_PANE_GAP`-column gutter between a
/// ~40%-width browser (list) pane on the left and the larger hero pane
/// taking the remainder on the right, each floored at
/// `WIDE_HERO_MIN_PANE_WIDTH`.
pub(in crate::app::render) fn wide_hero_split(content_area: Rect) -> (Rect, Rect) {
    let browser_w = ((content_area.width as u32 * 2 / 5) as u16)
        .max(WIDE_HERO_MIN_PANE_WIDTH)
        .min(
            content_area
                .width
                .saturating_sub(WIDE_HERO_MIN_PANE_WIDTH)
                .saturating_sub(WIDE_HERO_PANE_GAP),
        );
    let hero_w = content_area
        .width
        .saturating_sub(browser_w)
        .saturating_sub(WIDE_HERO_PANE_GAP);
    (
        Rect {
            x: content_area.x,
            y: content_area.y,
            width: browser_w,
            height: content_area.height,
        },
        Rect {
            x: content_area.x + browser_w + WIDE_HERO_PANE_GAP,
            y: content_area.y,
            width: hero_w,
            height: content_area.height,
        },
    )
}

/// The Wide hero arrangement's left (list) pane geometry: a one-row pill
/// bar flush with the pane's top, then the list panel below it (decision
/// 6's "pill row at top of list pane"). `right_panel` is the pane's full
/// rect (its `y`/`height` anchor the pill row and the panel's bottom);
/// `right_area` is the vertically-inset pane used for the pill row's
/// x/width. The pane's own status-row reserve is owned by
/// [`wide_hero_presentation`]; callers must not re-derive it.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::app) struct WideHeroBrowserPane {
    pub pills_area: Rect,
    pub spacer_area: Rect,
    pub list_panel: Rect,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::app) struct PillBarAreas {
    pub pills_area: Rect,
    pub spacer_area: Rect,
    pub content_area: Rect,
}

/// Places the shared one-row pill bar, its one-row parent-background spacer,
/// and the content below them.
pub(in crate::app) fn pill_bar_areas(area: Rect) -> PillBarAreas {
    let reserved = WIDE_HERO_PILLS_ROW_HEIGHT + WIDE_HERO_PILLS_GAP_ROWS;
    PillBarAreas {
        pills_area: Rect {
            height: WIDE_HERO_PILLS_ROW_HEIGHT.min(area.height),
            ..area
        },
        spacer_area: Rect {
            y: area.y.saturating_add(WIDE_HERO_PILLS_ROW_HEIGHT),
            height: WIDE_HERO_PILLS_GAP_ROWS.min(area.height.saturating_sub(1)),
            ..area
        },
        content_area: Rect {
            y: area.y.saturating_add(reserved),
            height: area.height.saturating_sub(reserved),
            ..area
        },
    }
}

pub(in crate::app) fn wide_hero_browser_pane(
    right_panel: Rect,
    right_area: Rect,
) -> WideHeroBrowserPane {
    let areas = pill_bar_areas(Rect {
        x: right_area.x,
        y: right_panel.y,
        width: right_area.width,
        height: right_panel.height,
    });
    WideHeroBrowserPane {
        pills_area: areas.pills_area,
        spacer_area: areas.spacer_area,
        list_panel: areas.content_area,
    }
}

/// The Wide hero left pane's focus resolution (design.md D-B). A closed
/// enum rather than a `bool`: the defect class this primitive exists to
/// prevent is exactly the read-only-versus-workspace confusion (e.g. passing
/// a bare `focused` when the correct value is `focused &&
/// interaction.episode_selection.is_some()`). `ReadOnly` and `Workspace(..)`
/// are two visibly different call shapes, so a reviewer can check the
/// variant rather than the expression.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::app) enum LeftPaneFocus {
    /// The pane is never focusable (Movies/home-videos/Emby-podcasts/
    /// feed-group browser, Home, Feeds): always [`palette::SURFACE_RESTING`].
    ReadOnly,
    /// The pane belongs to a focusable workspace (TV, Music, ABS Books, ABS
    /// Podcasts); `true` when that workspace currently holds focus.
    Workspace(bool),
}

/// Paints the Wide hero right pane: fills the [`wide_hero_presentation`]
/// right pane with the surface [`LeftPaneFocus`] resolves to, and returns the
/// shared content inset (`PANE_PAD_X`, `PANE_PAD_Y`). One owner for fill,
/// extent, inset, and focus resolution (design.md D-A) -- callers must not
/// resize, re-derive, or conditionally skip the fill, and must not apply a
/// destination-specific inset.
///
/// Takes `content_area` rather than a pane rect so a caller has nothing to
/// hand in but the rect the arrangement already consumes -- it cannot supply
/// a mutated right pane rect. `wide_hero_presentation` is pure and cheap, so
/// recomputing it here costs nothing.
pub(in crate::app) fn wide_hero_hero_pane(
    f: &mut Frame,
    content_area: Rect,
    focus: LeftPaneFocus,
) -> Option<Rect> {
    let WideHeroPanes {
        hero: hero_panel, ..
    } = wide_hero_presentation(content_area)?;
    let background = match focus {
        LeftPaneFocus::ReadOnly => palette::SURFACE_RESTING,
        LeftPaneFocus::Workspace(held) => palette::resolve_surface_focus(held),
    };
    f.render_widget(
        Block::default().style(Style::default().bg(background)),
        hero_panel,
    );
    Some(padded_rect(hero_panel, PANE_PAD_X, PANE_PAD_Y))
}

#[cfg(test)]
#[allow(clippy::items_after_test_module)]
mod wide_hero_hero_pane_tests {
    use super::*;
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;

    fn wide_area() -> Rect {
        Rect {
            x: 3,
            y: 2,
            width: crate::app::TWO_COLUMN_THRESHOLD,
            height: WIDE_HERO_MIN_AREA_HEIGHT + 5,
        }
    }

    #[test]
    fn read_only_never_resolves_to_the_focused_surface() {
        let area = wide_area();
        let mut terminal = Terminal::new(TestBackend::new(area.right(), area.bottom())).unwrap();
        let WideHeroPanes {
            hero: left_panel, ..
        } = wide_hero_presentation(area).expect("wide fits");
        terminal
            .draw(|f| {
                let returned =
                    wide_hero_hero_pane(f, area, LeftPaneFocus::ReadOnly).expect("wide fits");
                assert_eq!(returned, padded_rect(left_panel, PANE_PAD_X, PANE_PAD_Y));
            })
            .unwrap();
        let cell = &terminal.backend().buffer()[(left_panel.x, left_panel.y)];
        assert_eq!(cell.bg, palette::SURFACE_RESTING);
        assert_ne!(cell.bg, palette::resolve_surface_focus(true));
    }

    #[test]
    fn workspace_resolves_focus_and_returns_the_shared_inset() {
        let area = wide_area();
        let mut terminal = Terminal::new(TestBackend::new(area.right(), area.bottom())).unwrap();
        let WideHeroPanes {
            hero: left_panel, ..
        } = wide_hero_presentation(area).expect("wide fits");
        let expected = padded_rect(left_panel, PANE_PAD_X, PANE_PAD_Y);
        terminal
            .draw(|f| {
                let returned = wide_hero_hero_pane(f, area, LeftPaneFocus::Workspace(true))
                    .expect("wide fits");
                assert_eq!(returned.x, left_panel.x + PANE_PAD_X);
                assert_eq!(returned.y, left_panel.y + PANE_PAD_Y);
                assert_eq!(returned, expected);
            })
            .unwrap();
        let cell = &terminal.backend().buffer()[(left_panel.x, left_panel.y)];
        assert_eq!(cell.bg, palette::resolve_surface_focus(true));

        let mut terminal = Terminal::new(TestBackend::new(area.right(), area.bottom())).unwrap();
        terminal
            .draw(|f| {
                wide_hero_hero_pane(f, area, LeftPaneFocus::Workspace(false)).expect("wide fits");
            })
            .unwrap();
        let cell = &terminal.backend().buffer()[(left_panel.x, left_panel.y)];
        assert_eq!(cell.bg, palette::resolve_surface_focus(false));
    }

    #[test]
    fn sub_breakpoint_content_area_returns_none_without_painting() {
        let area = Rect {
            x: 0,
            y: 0,
            width: crate::app::TWO_COLUMN_THRESHOLD - 1,
            height: 20,
        };
        let mut terminal = Terminal::new(TestBackend::new(area.right(), area.bottom())).unwrap();
        terminal
            .draw(|f| {
                assert_eq!(wide_hero_hero_pane(f, area, LeftPaneFocus::ReadOnly), None);
            })
            .unwrap();
        let buffer = terminal.backend().buffer();
        for y in 0..area.height {
            for x in 0..area.width {
                assert_eq!(buffer[(x, y)].bg, ratatui::style::Color::Reset);
            }
        }
    }
}

/// Paints the focused browser rail's border using the shared framing primitive:
/// a `▔` top row and a `▁` bottom row, with a focus-resolved background, one
/// row inside `list_panel`'s own top/bottom edge. The rail uses the shared
/// painter's focused-rail framing arm; its fixed window mirrors `hero_block_shell`'s
/// (`offset = 0`, fully visible, padding rows `[1, height - 2]`); this is
/// Wide hero's thin shell entry point, the same role `hero_block_shell`
/// plays for inline presentation.
pub(in crate::app) fn wide_hero_browser_border(f: &mut Frame, list_panel: Rect, focused: bool) {
    if list_panel.height == 0 {
        return;
    }
    let background = palette::resolve_surface_focus(focused);
    for y in list_panel.y..list_panel.bottom() {
        for x in list_panel.x..list_panel.right() {
            let cell = f.buffer_mut().cell_mut((x, y)).expect("panel cell exists");
            cell.set_bg(background);
        }
    }
    crate::app::render::render_selected_block_borders(
        f,
        list_panel,
        0,
        list_panel.height as usize,
        1,
        (list_panel.height as usize).saturating_sub(2),
        crate::app::render::SelectedBlockBorderStyle::FocusedRail { focused },
    );
}

/// Paints the Wide hero arrangement's main content box: a
/// [`palette::SURFACE_BACKDROP`] inset within the Wide hero left pane,
/// present on every Wide hero surface with a kind-dependent payload (the
/// episode listing on TV, the track listing on Music, item description and
/// metadata elsewhere) and one shared padding value (design.md D9, matching
/// the pane inset from D6). Returns both rects so callers can use `panel` for
/// full-bleed row backgrounds and `content` for text layout.
///
/// Shared by Music's track panel and Home's overview block.
pub(in crate::app::render) fn wide_hero_hero_content_box(
    f: &mut Frame,
    area: Rect,
) -> (Rect, Rect) {
    let panel = Rect {
        x: area.x.saturating_add(PANE_PAD_X),
        width: area.width.saturating_sub(PANE_PAD_X * 2),
        ..area
    };
    f.render_widget(
        Block::default().style(Style::default().bg(palette::SURFACE_BACKDROP)),
        panel,
    );
    let content = Rect {
        x: panel.x.saturating_add(PANE_PAD_X),
        y: panel.y.saturating_add(PANE_PAD_Y),
        width: panel.width.saturating_sub(PANE_PAD_X * 2),
        height: panel.height.saturating_sub(PANE_PAD_Y * 2),
    };
    (panel, content)
}

/// Named Rect-only extension points within a Wide hero content rect
/// (design.md D-D): an optional artwork region and the overview text area
/// filling the remainder. Placement only -- no painting, no Service/image
/// effects, no list ownership.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::app) struct WideHeroSlots {
    pub artwork: Option<Rect>,
    pub overview: Rect,
}

/// Slices `content` top-to-bottom into `WideHeroSlots`: an `artwork_height`
/// row artwork slot (omitted when `0`), and the overview slot filling the
/// remainder. Callers place an embedded media list afterward via
/// [`place_media_list_below`].
pub(in crate::app::render) fn wide_hero_slots(
    content: Rect,
    artwork_height: u16,
    images_enabled: bool,
) -> WideHeroSlots {
    let artwork_height = artwork_height.min(content.height);
    let artwork = hero_artwork_slot(
        Rect {
            height: artwork_height,
            ..content
        },
        images_enabled,
    );
    // One blank row between the artwork and the title below it, matching the
    // wide Wide hero card (`prepare_wide_emby_hero_card`, which starts its
    // metadata at `img_area.bottom() + 1`) and every other tab's hero.
    let reserved_artwork_height = artwork.map_or(0, |area| area.height + 1);
    let reserved_artwork_height = reserved_artwork_height.min(content.height);
    let overview = Rect {
        y: content.y.saturating_add(reserved_artwork_height),
        height: content.height.saturating_sub(reserved_artwork_height),
        ..content
    };
    WideHeroSlots { artwork, overview }
}

/// Applies the global image policy to an artwork region. Images-off removes
/// the region entirely so its sibling can use the full content width.
pub(in crate::app::render) fn hero_artwork_slot(area: Rect, images_enabled: bool) -> Option<Rect> {
    (images_enabled && area.width > 0 && area.height > 0).then_some(area)
}

/// Places an embedded media-list box `gap` rows below `overview_bottom` (the
/// caller's already-painted overview content's real bottom row -- not a
/// pre-reserved slot height), sized to `height` rows and clamped to fit
/// within `content`'s bottom edge. Returns `None` when there is no room
/// (same "omitted when no room" convention as [`wide_hero_slots`]).
///
/// Rect-only: no painting, no text measurement -- callers supply the
/// already-measured overview bottom and desired height. Reusable by any
/// Wide hero surface embedding a media list below its overview (TV's
/// episode list; a future Music tracks / Audiobookshelf list).
pub(in crate::app::render) fn place_media_list_below(
    content: Rect,
    overview_bottom: u16,
    gap: u16,
    height: u16,
) -> Option<Rect> {
    let y = overview_bottom.saturating_add(gap);
    if y >= content.bottom() {
        return None;
    }
    let height = height.min(content.bottom().saturating_sub(y));
    (height > 0).then_some(Rect {
        x: content.x,
        y,
        width: content.width,
        height,
    })
}

#[cfg(test)]
#[allow(clippy::items_after_test_module)]
mod wide_hero_slots_tests {
    use super::*;

    fn content() -> Rect {
        Rect {
            x: 1,
            y: 2,
            width: 30,
            height: 20,
        }
    }

    #[test]
    fn splits_artwork_and_overview_slots() {
        let slots = wide_hero_slots(content(), 5, true);
        let artwork = slots.artwork.expect("artwork slot present");
        assert_eq!(artwork.y, content().y);
        assert_eq!(artwork.height, 5);
        assert_eq!(slots.overview.y, artwork.bottom() + 1);
        assert_eq!(slots.overview.bottom(), content().bottom());
    }

    #[test]
    fn omits_absent_artwork_slot() {
        let slots = wide_hero_slots(content(), 0, true);
        assert!(slots.artwork.is_none());
        assert_eq!(slots.overview, content());
    }

    #[test]
    fn images_off_collapses_artwork_and_preserves_full_content_width() {
        let area = content();
        let slots = wide_hero_slots(area, 5, false);
        assert!(slots.artwork.is_none());
        assert_eq!(slots.overview, area);
        assert_eq!(hero_artwork_slot(area, false), None);
    }

    #[test]
    fn place_media_list_below_starts_one_gap_row_after_overview_bottom() {
        let area = content();
        let overview_bottom = area.y + 5;
        let placed =
            place_media_list_below(area, overview_bottom, 1, 6).expect("room for media list");
        assert_eq!(placed.y, overview_bottom + 1);
        assert_eq!(placed.height, 6);
        assert_eq!(placed.x, area.x);
        assert_eq!(placed.width, area.width);
    }

    #[test]
    fn place_media_list_below_clamps_to_content_bottom() {
        let area = content();
        let overview_bottom = area.bottom() - 3;
        let placed =
            place_media_list_below(area, overview_bottom, 1, 6).expect("some room remains");
        assert_eq!(placed.height, 2);
        assert_eq!(placed.bottom(), area.bottom());
    }

    #[test]
    fn place_media_list_below_returns_none_without_room() {
        let area = content();
        let overview_bottom = area.bottom();
        assert!(place_media_list_below(area, overview_bottom, 1, 6).is_none());
    }
}

/// One line of the `Hero` component's Wide hero text block. Unlike
/// inline presentation's single-row, truncated [`super::hero::HeroLine`], Wide hero
/// text wraps across as many rows as it needs (design.md decision 2's
/// "Consequence": text wrapping moves into `Hero`, screens hand over
/// unwrapped strings). Style is screen-chosen (e.g. focus-derived bold),
/// matching how `HeroContent::meta_color` lets an inline browser pick its
/// own colour.
pub(in crate::app::render) struct WrappedHeroLine<'a> {
    pub text: &'a str,
    pub style: Style,
}

/// Paints `lines` wrapped to `area`'s width, top to bottom, stopping at
/// `area`'s bottom edge; empty line text is skipped. Returns the first
/// unpainted row.
pub(in crate::app::render) fn paint_wide_hero_text(
    f: &mut Frame,
    area: Rect,
    lines: &[WrappedHeroLine],
) -> u16 {
    if area.height == 0 || area.width < 3 {
        return area.y;
    }
    let mut row = area.y;
    let wrap_width = (area.width as usize).saturating_sub(1);
    for line in lines {
        if line.text.is_empty() {
            continue;
        }
        for wrapped in wrap(line.text, wrap_width.max(1)) {
            if row >= area.bottom() {
                return row;
            }
            f.render_widget(
                Paragraph::new(Line::from(Span::styled(wrapped.into_owned(), line.style))),
                Rect {
                    x: area.x,
                    y: row,
                    width: area.width,
                    height: 1,
                },
            );
            row += 1;
        }
    }
    row
}
