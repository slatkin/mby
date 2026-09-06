//! [`MouseGestureState`] — per-mounted-parent gesture recognition (ADR 0024,
//! design.md D3).
//!
//! Each mounted destination parent owns one `MouseGestureState`. It consumes
//! raw `tuirealm` [`MouseEvent`]s and emits recognized [`MouseGesture`]s. It is
//! keyed by nothing but that parent's own recent events — it is **not** a
//! shared clock, and reintroduces neither the global completed-frame hit map
//! nor the position-keyed cross-surface clock that D16 forbade (design.md D3,
//! "Reconciling with D16").
//!
//! Recognition covers `Click`/`DoubleClick`/`RightClick` and wheel `Scroll`.
//! `Moved` and `Drag` events are accepted and ignored here; hover-move spam
//! is dropped by the consuming component's first `on()` arm
//! (`MouseEventKind::Moved => return None`), never by this module (design.md
//! D7).
//!
//! ## Chosen intervals
//!
//! * **Double-click window: 400 ms**, exact-position match (legacy standard).
//! * **Wheel throttle: 30 ms** (legacy standard). The run loop ticks with
//!   `PollStrategy::Once(poll_timeout)` where `poll_timeout` is 50 ms normally
//!   and 8 ms while the visualizer runs (`src/app/shell_run.rs:497`). At the
//!   50 ms cadence a terminal wheel burst (crossterm coalesces several
//!   `ScrollUp`/`ScrollDown` per physical notch) all arrives inside one poll;
//!   a 30 ms throttle collapses that burst to one `Scroll` gesture per tick
//!   while still passing a sustained scroll (one notch per tick) through
//!   untouched. Lowering it below the 8 ms visualizer cadence would let the
//!   burst back through, so 30 ms is kept.

use std::time::{Duration, Instant};

use ratatui::layout::Position;
use tuirealm::event::{MouseButton, MouseEvent, MouseEventKind};

/// Double-click recognition window. See module docs.
const DOUBLE_CLICK_WINDOW: Duration = Duration::from_millis(400);
/// Minimum gap between emitted `Scroll` gestures. See module docs.
const WHEEL_THROTTLE: Duration = Duration::from_millis(30);

/// A recognized pointer gesture.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MouseGesture {
    Click(Position),
    DoubleClick(Position),
    RightClick(Position),
    /// Vertical wheel step: `delta` is `-1` up, `1` down.
    Scroll {
        at: Position,
        delta: i64,
    },
}

/// Per-parent gesture recognition state. One per mounted parent.
#[allow(dead_code)] // consumers land in tasks 3.4-3.6
#[derive(Debug, Default)]
pub struct MouseGestureState {
    last_click: Option<(Instant, Position)>,
    last_scroll: Option<Instant>,
}

#[allow(dead_code)] // consumers land in tasks 3.4-3.6
impl MouseGestureState {
    pub fn new() -> Self {
        Self::default()
    }

    /// Feed one raw mouse event; return the gesture it completes, if any.
    ///
    /// `Moved`, `Drag(_)`, `Up(_)` and horizontal wheel events are accepted
    /// and produce `None` (they must not panic — design.md D7).
    pub fn recognize(&mut self, event: &MouseEvent) -> Option<MouseGesture> {
        self.recognize_at(event, Instant::now())
    }

    fn recognize_at(&mut self, event: &MouseEvent, now: Instant) -> Option<MouseGesture> {
        let at = Position {
            x: event.column,
            y: event.row,
        };
        match event.kind {
            MouseEventKind::Down(MouseButton::Left) => {
                let is_double = self
                    .last_click
                    .is_some_and(|(t, p)| now.duration_since(t) < DOUBLE_CLICK_WINDOW && p == at);
                self.last_click = Some((now, at));
                Some(if is_double {
                    MouseGesture::DoubleClick(at)
                } else {
                    MouseGesture::Click(at)
                })
            }
            MouseEventKind::Down(MouseButton::Right) => Some(MouseGesture::RightClick(at)),
            MouseEventKind::ScrollUp | MouseEventKind::ScrollDown => {
                let allow = self
                    .last_scroll
                    .is_none_or(|t| now.duration_since(t) >= WHEEL_THROTTLE);
                if !allow {
                    return None;
                }
                self.last_scroll = Some(now);
                let delta = if matches!(event.kind, MouseEventKind::ScrollUp) {
                    -1
                } else {
                    1
                };
                Some(MouseGesture::Scroll { at, delta })
            }
            _ => None,
        }
    }
}

#[cfg(test)]
impl MouseGestureState {
    /// Test seam: forget the last click/scroll so the next event is neither
    /// throttled nor promoted to a double-click.
    pub(crate) fn reset_for_test(&mut self) {
        *self = Self::default();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ev(kind: MouseEventKind, x: u16, y: u16) -> MouseEvent {
        MouseEvent {
            kind,
            modifiers: tuirealm::event::KeyModifiers::NONE,
            column: x,
            row: y,
        }
    }

    #[test]
    fn two_clicks_inside_window_and_position_are_a_double_click() {
        let mut s = MouseGestureState::new();
        let t0 = Instant::now();
        assert_eq!(
            s.recognize_at(&ev(MouseEventKind::Down(MouseButton::Left), 3, 4), t0),
            Some(MouseGesture::Click(Position { x: 3, y: 4 }))
        );
        assert_eq!(
            s.recognize_at(
                &ev(MouseEventKind::Down(MouseButton::Left), 3, 4),
                t0 + Duration::from_millis(100)
            ),
            Some(MouseGesture::DoubleClick(Position { x: 3, y: 4 }))
        );
    }

    #[test]
    fn second_click_after_window_is_a_plain_click() {
        let mut s = MouseGestureState::new();
        let t0 = Instant::now();
        s.recognize_at(&ev(MouseEventKind::Down(MouseButton::Left), 3, 4), t0);
        assert_eq!(
            s.recognize_at(
                &ev(MouseEventKind::Down(MouseButton::Left), 3, 4),
                t0 + Duration::from_millis(500)
            ),
            Some(MouseGesture::Click(Position { x: 3, y: 4 }))
        );
    }

    #[test]
    fn second_click_at_a_moved_position_is_a_plain_click() {
        let mut s = MouseGestureState::new();
        let t0 = Instant::now();
        s.recognize_at(&ev(MouseEventKind::Down(MouseButton::Left), 3, 4), t0);
        assert_eq!(
            s.recognize_at(
                &ev(MouseEventKind::Down(MouseButton::Left), 3, 5),
                t0 + Duration::from_millis(100)
            ),
            Some(MouseGesture::Click(Position { x: 3, y: 5 }))
        );
    }

    #[test]
    fn rapid_scrolls_are_coalesced_by_the_throttle() {
        let mut s = MouseGestureState::new();
        let t0 = Instant::now();
        assert_eq!(
            s.recognize_at(&ev(MouseEventKind::ScrollDown, 1, 1), t0),
            Some(MouseGesture::Scroll {
                at: Position { x: 1, y: 1 },
                delta: 1
            })
        );
        assert_eq!(
            s.recognize_at(
                &ev(MouseEventKind::ScrollDown, 1, 1),
                t0 + Duration::from_millis(5)
            ),
            None
        );
        assert_eq!(
            s.recognize_at(
                &ev(MouseEventKind::ScrollUp, 1, 1),
                t0 + Duration::from_millis(40)
            ),
            Some(MouseGesture::Scroll {
                at: Position { x: 1, y: 1 },
                delta: -1
            })
        );
    }

    #[test]
    fn right_button_down_is_a_right_click() {
        let mut s = MouseGestureState::new();
        assert_eq!(
            s.recognize(&ev(MouseEventKind::Down(MouseButton::Right), 7, 8)),
            Some(MouseGesture::RightClick(Position { x: 7, y: 8 }))
        );
    }

    #[test]
    fn moved_and_drag_events_are_ignored_without_panic() {
        let mut s = MouseGestureState::new();
        assert_eq!(s.recognize(&ev(MouseEventKind::Moved, 1, 1)), None);
        assert_eq!(
            s.recognize(&ev(MouseEventKind::Drag(MouseButton::Left), 1, 1)),
            None
        );
        assert_eq!(
            s.recognize(&ev(MouseEventKind::Up(MouseButton::Left), 1, 1)),
            None
        );
    }
}
