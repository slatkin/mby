//! [`HitRegions`] — a last-push-wins rectangle registry for **irregular
//! painted chrome only** (design.md D6): pills, Queue scope buttons, transport
//! controls, group selectors, overlay rows.
//!
//! This is not a new shape — it formalises the hand-rolled `Vec<(Rect, T)>`
//! that components like `feeds.rs` (`layout.selector_tabs`) already keep. The
//! two canonical media-list controls do **not** use this; their uniform row
//! flow resolves arithmetically through `resolve_point` (design.md D6).
//!
//! `HitRegions` has no paint hook and no frame lifecycle of its own: the
//! owning component calls [`HitRegions::clear`] and [`HitRegions::push`] in the
//! same code that paints those rects.

use ratatui::layout::{Position, Rect};

/// A last-push-wins map from painted rectangles to a caller-chosen `Tag`.
///
/// On overlap, the most recently pushed rectangle wins (it is painted on top).
#[derive(Debug, Clone, Default)]
pub struct HitRegions<Tag> {
    regions: Vec<(Rect, Tag)>,
}

impl<Tag> HitRegions<Tag> {
    pub fn new() -> Self {
        Self {
            regions: Vec::new(),
        }
    }

    /// Drop every recorded region. Call at the start of the code that repaints
    /// this chrome.
    pub fn clear(&mut self) {
        self.regions.clear();
    }

    /// Record a painted rectangle and the tag it resolves to. Later pushes win
    /// on overlap.
    pub fn push(&mut self, rect: Rect, tag: Tag) {
        self.regions.push((rect, tag));
    }

    /// The tag of the last-pushed region containing `point`, if any. Points on
    /// the right/bottom edge of a rectangle are outside it (ratatui
    /// `Rect::contains` semantics).
    pub fn resolve(&self, point: Position) -> Option<&Tag> {
        self.regions
            .iter()
            .rev()
            .find(|(rect, _)| rect.contains(point))
            .map(|(_, tag)| tag)
    }

    /// Test seam: the recorded rect/tag pairs, so component tests can derive
    /// click coordinates from the same geometry the component resolves.
    #[cfg(test)]
    pub(crate) fn regions(&self) -> &[(Rect, Tag)] {
        &self.regions
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rect(x: u16, y: u16, w: u16, h: u16) -> Rect {
        Rect {
            x,
            y,
            width: w,
            height: h,
        }
    }

    #[test]
    fn empty_resolves_to_none() {
        let regions: HitRegions<u8> = HitRegions::new();
        assert_eq!(regions.resolve(Position { x: 0, y: 0 }), None);
    }

    #[test]
    fn last_push_wins_on_overlap() {
        let mut regions = HitRegions::new();
        regions.push(rect(0, 0, 10, 10), "under");
        regions.push(rect(5, 5, 10, 10), "over");
        assert_eq!(regions.resolve(Position { x: 6, y: 6 }), Some(&"over"));
        // Only the first region covers this point.
        assert_eq!(regions.resolve(Position { x: 1, y: 1 }), Some(&"under"));
    }

    #[test]
    fn out_of_bounds_point_resolves_to_none() {
        let mut regions = HitRegions::new();
        regions.push(rect(2, 2, 4, 4), 1);
        assert_eq!(regions.resolve(Position { x: 100, y: 100 }), None);
        assert_eq!(regions.resolve(Position { x: 0, y: 0 }), None);
    }

    #[test]
    fn point_on_rect_edge() {
        let mut regions = HitRegions::new();
        regions.push(rect(2, 2, 4, 4), 1); // covers x 2..6, y 2..6
                                           // Top-left edge is inside.
        assert_eq!(regions.resolve(Position { x: 2, y: 2 }), Some(&1));
        // Right/bottom edge is outside.
        assert_eq!(regions.resolve(Position { x: 6, y: 4 }), None);
        assert_eq!(regions.resolve(Position { x: 4, y: 6 }), None);
        // Last row/column still inside.
        assert_eq!(regions.resolve(Position { x: 5, y: 5 }), Some(&1));
    }

    #[test]
    fn clear_drops_all_regions() {
        let mut regions = HitRegions::new();
        regions.push(rect(0, 0, 4, 4), 1);
        regions.clear();
        assert_eq!(regions.resolve(Position { x: 1, y: 1 }), None);
    }
}
