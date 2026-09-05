use ratatui::layout::{Position, Rect};
use ratatui::style::Style;
use ratatui::widgets::Paragraph;
use ratatui::Frame;
use tuirealm::command::{Cmd, CmdResult};
use tuirealm::component::{AppComponent, Component};
use tuirealm::event::{Event, Key, KeyEvent, MouseEvent, MouseEventKind};
use tuirealm::props::{AttrValue, Attribute, QueryResult};
use tuirealm::state::State;

use super::media_list::{MediaKind, MediaListRow, MediaSemanticState, WideMediaList};
use super::mouse::gesture::{MouseGesture, MouseGestureState};
use super::mouse::hit::HitRegions;
use super::msg::{Msg, QueueColumnResize, QueueIntent, QueueMove, QueueRequest, ShellRequest};
use super::user_event::UserEvent;
use crate::app::palette;
use crate::app::render::{
    render_queue_title_content, render_wide_media_list, QueueRenderGeometry, QueueTitleModel,
};
use crate::app::types_playback::{PlaybackState, QueueScope};
use crate::app::ui_util::{fmt_duration_short, fmt_playback_pct};
use mbv_core::api::TICKS_PER_SECOND;
use mbv_core::playback_queue::{QueueItem, QueueSlot, QueueSlotId};

/// Why the shell is pushing a cursor. `Preserve` keeps the user's selection
/// pinned to its slot across a content refresh; `Set` is an authoritative
/// move (follow-the-playhead, jump-to-now-playing, wheel scroll, scope switch)
/// that must win over slot-identity reconciliation.
pub(in crate::app) enum QueueCursorUpdate {
    Preserve,
    Set(usize),
}

pub struct QueueComponent {
    /// The canonical fixed-row control owns the Queue rows, the local cursor,
    /// the resting scroll offset, and viewport/scrollbar geometry
    /// (migrate-queue-to-canonical-list D1/D2). The parent keeps only the
    /// prepared projection and shell-owned chrome below.
    list: WideMediaList<QueueSlotId>,
    scope: QueueScope,
    focused: bool,
    empty_text: String,
    title: Option<QueueTitleModel>,
    title_area: Option<Rect>,
    area: Rect,
    geometry: QueueRenderGeometry,
    /// Private per-parent gesture recognition (ADR 0024, design.md D3): owns
    /// the double-click window and wheel throttle.
    mouse_gestures: MouseGestureState,
    /// Scope-pill rects (design.md D6), repopulated in `view()` from the
    /// geometry the title painter just produced.
    scope_regions: HitRegions<QueueScope>,
}

impl QueueComponent {
    pub(crate) fn selected_row_rect(&self) -> Option<Rect> {
        let selected = self.list.selected_target()?;
        self.geometry
            .rows
            .iter()
            .find(|(_, slot_id)| slot_id == selected)
            .map(|(rect, _)| *rect)
    }

    pub fn new() -> Self {
        Self {
            list: WideMediaList::new(),
            scope: QueueScope::Local,
            focused: false,
            empty_text: String::new(),
            title: None,
            title_area: None,
            area: Rect::default(),
            geometry: QueueRenderGeometry::default(),
            mouse_gestures: MouseGestureState::new(),
            scope_regions: HitRegions::new(),
        }
    }

    /// Test-only: drive framework focus the way `Component::attr` does.
    #[cfg(test)]
    pub(in crate::app) fn set_focused(&mut self, focused: bool) {
        self.focused = focused;
    }

    pub(in crate::app) fn set_content(
        &mut self,
        slots: Vec<QueueSlot>,
        cursor: QueueCursorUpdate,
        scope: QueueScope,
        playback: PlaybackState,
        title: QueueTitleModel,
    ) {
        // Scroll is component-owned (split-queue-cursor-ownership D3): a new
        // scope's content starts at the top. Reset before the canonical child
        // reconciles so its viewport never reuses an old offset.
        if scope != self.scope {
            self.list.set_scroll(0);
        }
        // The canonical child re-pins its cursor to the selected `QueueSlotId`
        // and locally clamps when the target is gone (D3 / D2); no App mirror.
        self.list.set_content(queue_media_rows(&slots, playback));
        if let QueueCursorUpdate::Set(idx) = cursor {
            // An authoritative move (follow-the-playhead, jump-to-now-playing,
            // wheel scroll, scope switch): skip identity reconciliation.
            self.list.select_index(idx);
        }
        // Keep the resting offset from running ahead of the cursor row; the
        // painter's height-aware clamp finishes the job every frame.
        self.list
            .set_scroll(self.list.scroll().min(self.list.cursor()));
        self.scope = scope;
        self.empty_text = if scope == QueueScope::Local {
            "  Add items with p from Home or library tabs".into()
        } else {
            "  Remote queue is empty".into()
        };
        self.title = Some(title);
    }

    pub(in crate::app) fn set_area(&mut self, area: Rect) {
        self.area = area;
    }

    pub(in crate::app) fn set_title_area(&mut self, area: Option<Rect>) {
        self.title_area = area;
    }

    fn cursor_message(&self) -> Option<Msg> {
        self.list.selected_target().map(|&slot_id| {
            Msg::Queue(QueueRequest::Cursor {
                scope: self.scope,
                slot_id,
            })
        })
    }

    fn move_cursor(&mut self, delta: i64) -> Option<Msg> {
        self.list.move_selection(delta);
        self.cursor_message()
    }

    fn handle_key(&mut self, key: &KeyEvent) -> Option<Msg> {
        let selected = || {
            self.list
                .selected_target()
                .map(|&slot_id| (self.scope, slot_id))
        };
        match key.code {
            Key::Char('[')
                if !key
                    .modifiers
                    .contains(tuirealm::event::KeyModifiers::CONTROL)
                    && !key.modifiers.contains(tuirealm::event::KeyModifiers::ALT) =>
            {
                self.scope = QueueScope::Local;
                // Scope is preassigned here, before the request reaches the
                // shell, so the set_content scope-change reset would not fire;
                // the component resets its own scroll itself (D3).
                self.list.set_scroll(0);
                return Some(Msg::Queue(QueueRequest::Scope(self.scope)));
            }
            Key::Char(']')
                if !key
                    .modifiers
                    .contains(tuirealm::event::KeyModifiers::CONTROL)
                    && !key.modifiers.contains(tuirealm::event::KeyModifiers::ALT) =>
            {
                self.scope = QueueScope::Remote;
                // Scope is preassigned here, before the request reaches the
                // shell, so the set_content scope-change reset would not fire;
                // the component resets its own scroll itself (D3).
                self.list.set_scroll(0);
                return Some(Msg::Queue(QueueRequest::Scope(self.scope)));
            }
            Key::Left | Key::Right if key.modifiers == tuirealm::event::KeyModifiers::SHIFT => {
                return Some(Msg::Shell(ShellRequest::QueueIntent(
                    QueueIntent::ResizeColumn(if key.code == Key::Left {
                        QueueColumnResize::Narrower
                    } else {
                        QueueColumnResize::Wider
                    }),
                )));
            }
            Key::Up if key.modifiers.is_empty() => {
                if self.list.cursor() == 0 {
                    return None;
                }
                return self.move_cursor(-1);
            }
            Key::Down if key.modifiers.is_empty() => {
                if self.list.cursor() + 1 >= self.list.selectable_len() {
                    return None;
                }
                return self.move_cursor(1);
            }
            Key::PageUp if key.modifiers.is_empty() => {
                return self.move_cursor(-(self.area.height.saturating_sub(1).max(1) as i64));
            }
            Key::PageDown if key.modifiers.is_empty() => {
                return self.move_cursor(self.area.height.saturating_sub(1).max(1) as i64);
            }
            Key::Home if key.modifiers.is_empty() => {
                self.list.select_first();
                return self.cursor_message();
            }
            Key::End if key.modifiers.is_empty() => {
                self.list.select_last();
                return self.cursor_message();
            }
            Key::Enter => {
                return selected()
                    .map(|(scope, slot_id)| Msg::Queue(QueueRequest::Play { scope, slot_id }));
            }
            Key::Delete => {
                return selected()
                    .map(|(scope, slot_id)| Msg::Queue(QueueRequest::Remove { scope, slot_id }));
            }
            Key::Up if key.modifiers.contains(tuirealm::event::KeyModifiers::SHIFT) => {
                return selected().map(|(scope, slot_id)| {
                    Msg::Queue(QueueRequest::Move {
                        scope,
                        slot_id,
                        direction: QueueMove::Up,
                    })
                });
            }
            Key::Down if key.modifiers.contains(tuirealm::event::KeyModifiers::SHIFT) => {
                return selected().map(|(scope, slot_id)| {
                    Msg::Queue(QueueRequest::Move {
                        scope,
                        slot_id,
                        direction: QueueMove::Down,
                    })
                });
            }
            Key::Char('t')
                if key
                    .modifiers
                    .contains(tuirealm::event::KeyModifiers::CONTROL) =>
            {
                return Some(Msg::Shell(ShellRequest::QueueIntent(
                    QueueIntent::StopRemoteTracking,
                )));
            }
            Key::Char('r')
                if key
                    .modifiers
                    .contains(tuirealm::event::KeyModifiers::CONTROL) =>
            {
                return Some(Msg::Shell(ShellRequest::QueueIntent(
                    QueueIntent::ReanchorRemoteTracking,
                )));
            }
            Key::Char('z')
                if key
                    .modifiers
                    .contains(tuirealm::event::KeyModifiers::CONTROL) =>
            {
                return Some(Msg::Queue(QueueRequest::Undo { scope: self.scope }));
            }
            Key::Char('.') if key.modifiers.is_empty() => {
                // `.` is a selection-dependent chord the focused component
                // owns (CONTEXT.md "Global chord"): emit the queue context-menu
                // request for the currently selected row.
                return Some(Msg::Shell(ShellRequest::QueueContextMenu {
                    slot_id: self.list.selected_target().copied(),
                }));
            }
            Key::Char('i') => {
                return selected().map(|(scope, slot_id)| {
                    Msg::Shell(ShellRequest::QueueIntent(QueueIntent::Navigate {
                        scope,
                        slot_id,
                    }))
                });
            }
            Key::Char('p') => {
                return Some(Msg::Shell(ShellRequest::QueueIntent(QueueIntent::PlayNow)));
            }
            Key::Char('s')
                if key
                    .modifiers
                    .contains(tuirealm::event::KeyModifiers::CONTROL) =>
            {
                return Some(Msg::Shell(ShellRequest::QueueIntent(
                    QueueIntent::SavePlaylist,
                )));
            }
            Key::Char('c') if !key.modifiers.contains(tuirealm::event::KeyModifiers::ALT) => {
                return Some(Msg::Shell(ShellRequest::QueueIntent(QueueIntent::Clear)));
            }
            _ => {}
        }
        None
    }

    /// Gesture recognition (click / double-click / right-click / wheel) comes
    /// from the private `MouseGestureState` (ADR 0024, design.md D3). Row
    /// identity comes from the embedded control's `resolve_point`
    /// (design.md D6); scope pills from `scope_regions`. The component emits a
    /// semantic `Msg` with a resolved `QueueSlotId`/scope — never raw
    /// coordinates — except the context-menu anchor (design.md D4).
    fn handle_mouse(&mut self, mouse: &MouseEvent) -> Option<Msg> {
        // Queue does not consume hover-move (design.md D7).
        if matches!(mouse.kind, MouseEventKind::Moved) {
            return None;
        }
        match self.mouse_gestures.recognize(mouse)? {
            MouseGesture::Scroll { at, delta } => {
                if !self.area.contains(at) {
                    return None;
                }
                Some(Msg::Shell(ShellRequest::QueueScroll { delta }))
            }
            MouseGesture::Click(at) => {
                if let Some(scope) = self.claim_scope_pill(at) {
                    return Some(Msg::Shell(ShellRequest::QueueScopeClick { scope }));
                }
                if !self.area.contains(at) {
                    return None;
                }
                self.claim_slot(at);
                Some(Msg::Shell(ShellRequest::QueueRowClick {
                    slot_id: self.list.selected_target().copied(),
                }))
            }
            MouseGesture::DoubleClick(at) => {
                if let Some(scope) = self.claim_scope_pill(at) {
                    return Some(Msg::Shell(ShellRequest::QueueScopeClick { scope }));
                }
                if !self.area.contains(at) {
                    return None;
                }
                self.claim_slot(at);
                Some(Msg::Shell(ShellRequest::QueueRowActivate {
                    slot_id: self.list.selected_target().copied(),
                }))
            }
            MouseGesture::RightClick(at) => {
                if !self.area.contains(at) {
                    return None;
                }
                // Legacy parity: a right-click on blank queue space opens no
                // menu. Only resolve a menu when the click lands on a row —
                // never fall back to the prior selection (design.md D4).
                let slot_id = self.claim_slot(at)?;
                Some(Msg::Shell(ShellRequest::QueueRowContextMenu {
                    slot_id: Some(slot_id),
                    anchor: (mouse.column, mouse.row),
                }))
            }
        }
    }

    /// If `at` lands on a scope pill, switch the component's own scope and
    /// reset its scroll (design.md D3), and return the new scope.
    fn claim_scope_pill(&mut self, at: Position) -> Option<QueueScope> {
        let &scope = self.scope_regions.resolve(at)?;
        self.scope = scope;
        self.list.set_scroll(0);
        Some(scope)
    }

    /// Resolve the slot under `at` from the embedded control and pin the
    /// selection to it (a blank click keeps the previous slot, preserving the
    /// legacy no-op). Returns the resolved slot, if any.
    fn claim_slot(&mut self, at: Position) -> Option<QueueSlotId> {
        let slot_id = self.list.resolve_point(self.area, at).copied();
        if let Some(id) = slot_id {
            self.list.select_target(&id);
        }
        slot_id
    }

    #[cfg(test)]
    pub(crate) fn test_rows(&self) -> &[(Rect, mbv_core::playback_queue::QueueSlotId)] {
        &self.geometry.rows
    }

    #[cfg(test)]
    pub(crate) fn test_selected_target(&self) -> Option<QueueSlotId> {
        self.list.selected_target().copied()
    }

    #[cfg(test)]
    pub(crate) fn test_cursor(&self) -> usize {
        self.list.cursor()
    }

    #[cfg(test)]
    pub(crate) fn test_scroll(&self) -> usize {
        self.list.scroll()
    }

    #[cfg(test)]
    pub(crate) fn test_scope_pill_areas(&self) -> (Rect, Rect) {
        (
            self.geometry.scope_local_area,
            self.geometry.scope_remote_area,
        )
    }
}

impl Default for QueueComponent {
    fn default() -> Self {
        Self::new()
    }
}

impl Component for QueueComponent {
    fn view(&mut self, frame: &mut Frame, area: ratatui::layout::Rect) {
        // The shell passes the current layout area every frame. Never reuse the
        // previous area when this panel is hidden or resized: stale geometry
        // would repaint the old queue panel and leave a ghost behind.
        self.area = area;
        self.geometry = QueueRenderGeometry::default();
        if let (Some(title_area), Some(title)) = (self.title_area, self.title.as_ref()) {
            render_queue_title_content(frame, title_area, title, &mut self.geometry);
        }
        // Adopt the scope-pill rects the title painter just produced into the
        // irregular-chrome registry (design.md D6).
        self.scope_regions.clear();
        if self.title.is_some() {
            self.scope_regions
                .push(self.geometry.scope_local_area, QueueScope::Local);
            self.scope_regions
                .push(self.geometry.scope_remote_area, QueueScope::Remote);
        }
        if area.height < 1 {
            return;
        }
        if self.list.is_empty() {
            frame.render_widget(
                Paragraph::new(self.empty_text.clone())
                    .style(Style::default().fg(palette::TEXT_MUTED)),
                area,
            );
            return;
        }
        // The canonical child is the sole Queue body painter
        // (migrate-queue-to-canonical-list D4). It persists the resolved scroll
        // offset back into the list; Queue resolves click slots via
        // `list.resolve_point` now, and keeps `geometry.rows` (rebuilt below)
        // only for `selected_row_rect` and tests.
        // Legacy `render_queue_content` parity: the selected row takes the
        // focused queue-column surface (its parent panel).
        render_wide_media_list(
            frame,
            area,
            area,
            &mut self.list,
            self.focused,
            palette::SURFACE_FOCUSED,
        );
        let viewport = self.list.resolve_viewport(area.height as usize);
        self.geometry.rows = (viewport.offset..viewport.total_rows)
            .take(viewport.height)
            .enumerate()
            .filter_map(|(line, row)| {
                let slot_id = *self.list.rows()[row].selectable_target()?;
                let rect = Rect {
                    x: area.x,
                    y: area.y + line as u16,
                    width: area.width,
                    height: 1,
                };
                Some((rect, slot_id))
            })
            .collect();
    }

    fn query<'a>(&'a self, _attr: Attribute) -> Option<QueryResult<'a>> {
        None
    }
    fn attr(&mut self, attr: Attribute, value: AttrValue) {
        if attr == Attribute::Focus {
            self.focused = matches!(value, AttrValue::Flag(true));
        }
    }
    fn state(&self) -> State {
        State::None
    }
    fn perform(&mut self, _cmd: Cmd) -> CmdResult {
        CmdResult::NoChange
    }
}

impl AppComponent<Msg, UserEvent> for QueueComponent {
    fn on(&mut self, event: &Event<UserEvent>) -> Option<Msg> {
        match event {
            Event::Keyboard(key) => self.handle_key(key),
            Event::Mouse(mouse) => self.handle_mouse(mouse),
            _ => None,
        }
    }
}

/// Project Queue slots into the canonical provider-neutral row vocabulary
/// (migrate-queue-to-canonical-list D2): a stable `QueueSlotId` target, the
/// slot title, duration/elapsed metadata, and semantic active state whose
/// progress is clamped to `0..=100` at this projection boundary. No ticks,
/// runtime, source, credentials, callbacks, or effects cross the child edge.
pub(in crate::app) fn queue_media_rows(
    slots: &[QueueSlot],
    playback: PlaybackState,
) -> Vec<MediaListRow<QueueSlotId>> {
    slots
        .iter()
        .enumerate()
        .map(|(index, slot)| {
            let is_active = playback.active && playback.active_idx == index;
            let (title, pos_ticks, duration_ticks) =
                queue_row_fields(&slot.item, playback, is_active);
            let time_text = queue_row_time_text(pos_ticks, duration_ticks, is_active);
            let (semantic_state, trailing) = if is_active {
                let progress = (pos_ticks > 0 && duration_ticks > 0)
                    .then(|| (pos_ticks * 100 / duration_ticks).clamp(0, 100) as u16);
                // The active-row `%` comes from the Active progress path.
                (MediaSemanticState::active(progress), None)
            } else {
                // Non-active video rows carry a watch-% badge as FOAM metadata
                // (legacy queue painter); audio/feed/audiobookshelf rows do not.
                let pct = match &slot.item {
                    QueueItem::Emby(item) if !item.is_audio() => {
                        let pct =
                            fmt_playback_pct(item.playback_position_ticks, item.runtime_ticks);
                        (!pct.is_empty()).then_some(pct)
                    }
                    _ => None,
                };
                (MediaSemanticState::Ordinary, pct)
            };
            MediaListRow::Item {
                target: slot.slot_id,
                primary: title,
                trailing,
                // Duration/elapsed is a right-aligned green element, not FOAM.
                duration: (!time_text.is_empty()).then_some(time_text),
                kind: MediaKind::Media,
                semantic_state,
            }
        })
        .collect()
}

/// The title and (position, duration) ticks a Queue row paints, resolved per
/// item kind and overridden with live playback ticks for the active row.
fn queue_row_fields(
    item: &QueueItem,
    playback: PlaybackState,
    is_active: bool,
) -> (String, i64, i64) {
    match item {
        QueueItem::Emby(item) => {
            let (pos, runtime) = if is_active {
                (
                    if playback.position_ticks > 0 {
                        playback.position_ticks
                    } else {
                        item.playback_position_ticks
                    },
                    playback.runtime_ticks,
                )
            } else {
                (item.playback_position_ticks, item.runtime_ticks)
            };
            (item.name.clone(), pos, runtime)
        }
        QueueItem::Feed(entry) => (
            entry.title.clone(),
            if is_active {
                playback.position_ticks
            } else {
                0
            },
            entry.duration_ticks.unwrap_or(0) as i64,
        ),
        QueueItem::Audiobookshelf(ep) => (
            ep.title.clone(),
            if is_active {
                playback.position_ticks
            } else {
                0
            },
            ep.duration_ticks.unwrap_or(0) as i64,
        ),
        QueueItem::AudiobookshelfBook(book) => (
            book.title.clone(),
            if is_active {
                playback.position_ticks
            } else {
                0
            },
            book.duration_ticks.unwrap_or(0) as i64,
        ),
    }
}

fn queue_row_time_text(pos_ticks: i64, dur_ticks: i64, show_elapsed: bool) -> String {
    let dur_s = dur_ticks / TICKS_PER_SECOND;
    if dur_s <= 0 {
        return String::new();
    }
    if show_elapsed {
        format!(
            "{} / {}",
            fmt_duration_short(pos_ticks / TICKS_PER_SECOND),
            fmt_duration_short(dur_s)
        )
    } else {
        fmt_duration_short(dur_s)
    }
}
