use super::components::{
    ComponentId, QueueColumnResize, QueueComponent, QueueCursorUpdate, QueueIntent, QueueMove,
    QueueRequest,
};
use super::shell::Model;
use super::{PanelFocus, QueueCursorPush, QueueScope};
use crate::app::notify_actions::ToastSeverity;

impl Model {
    pub(super) fn sync_queue(&mut self) {
        let id = ComponentId::Queue;
        if !self.application.mounted(&id) {
            self.application
                .mount(id.clone(), Box::new(QueueComponent::new()), vec![])
                .expect("mount Queue");
        }

        let queue_focused = matches!(self.app.effective_panel_focus(), PanelFocus::Queue)
            && !self.blocking_overlay_active();
        // A mounted sidebar/modal/popup owns focus while it is up; re-activating
        // Queue here would steal the keypress it needs to close itself (mini
        // view keeps `effective_panel_focus` on Queue, so this pass fires every
        // tick otherwise).
        if !self.overlay_holds_focus() {
            if queue_focused {
                if self.application.focus() != Some(&id) {
                    self.application.active(&id).expect("activate Queue");
                }
            } else if self.application.focus() == Some(&id) {
                self.application.blur().expect("blur Queue");
            }
        }

        let scope = self.app.visible_queue_scope();
        let slots = self.app.queue_for_scope(scope).slots().to_vec();
        // An authoritative writer armed `queue_cursor_pushed` for a specific
        // scope. Consume it as a `Set` that wins over slot-identity
        // reconciliation only when that scope is the one on screen: a push
        // armed for a scope the user is not viewing (e.g. a remote daemon
        // update while the user browses Local) must not snap the visible
        // scope's independent selection. A `Follow` push additionally yields
        // to an in-progress user navigation; a `Reanchor` (scope switch, full
        // replacement, wheel scroll, jump-to-now-playing) always wins.
        // Anything else is a routine content refresh: `Preserve` the
        // component's own selection pinned to its slot.
        let cursor = match self.app.queue_cursor_pushed.take() {
            Some(push) if push.scope() == scope => match push {
                QueueCursorPush::Follow(_) if self.app.queue_cursor_held_by_user() => {
                    QueueCursorUpdate::Preserve
                }
                _ => QueueCursorUpdate::Set(self.app.queue_for_scope(scope).queue_cursor),
            },
            _ => QueueCursorUpdate::Preserve,
        };
        let playback = self.app.displayed_queue_playback_state();
        let title = self.app.queue_title_model();
        let title_area = self.app.layout.main.queue_title_area;
        if let Some(comp) = self.application.get_component_mut(&id) {
            if let Some(queue) = comp.as_any_mut().downcast_mut::<QueueComponent>() {
                queue.set_content(slots, cursor, scope, playback, title);
                queue.set_area(self.app.layout.main.queue_area);
                queue.set_title_area(title_area);
            }
        }
    }

    pub(super) fn render_queue_component(&mut self, frame: &mut ratatui::Frame) {
        let id = ComponentId::Queue;
        if !self.application.mounted(&id) {
            return;
        }
        if let Some(comp) = self.application.get_component_mut(&id) {
            if let Some(queue) = comp.as_any_mut().downcast_mut::<QueueComponent>() {
                queue.set_area(self.app.layout.main.queue_area);
                queue.set_title_area(self.app.layout.main.queue_title_area);
            }
        }
        self.application
            .view(&id, frame, self.app.layout.main.queue_area);
        self.app.layout.main.queue_selected_item_rect = self
            .application
            .get_component(&id)
            .and_then(|comp| comp.as_any().downcast_ref::<QueueComponent>())
            .and_then(QueueComponent::selected_row_rect);
    }

    pub(super) fn handle_queue_request(&mut self, request: QueueRequest) {
        match request {
            QueueRequest::Scope(scope) => {
                if scope == QueueScope::Local || self.app.has_direct_remote_queue() {
                    self.app.set_queue_scope(scope);
                }
            }
            QueueRequest::Cursor { scope, slot_id } => {
                // Plain navigation: select_queue_slot applies the scope/focus/
                // hold-window side effects; there is no effect to drive with
                // the resolved index (D2), so it is discarded.
                let _ = self.select_queue_slot(scope, slot_id);
            }
            QueueRequest::Play { scope, slot_id } => {
                if let Some(index) = self.select_queue_slot(scope, slot_id) {
                    self.app
                        .dispatch(super::action::Command::QueuePlayCursor(index));
                }
            }
            QueueRequest::Remove { scope, slot_id } => {
                if let Some(index) = self.select_queue_slot(scope, slot_id) {
                    self.app.remove_from_queue(index);
                }
            }
            QueueRequest::Move {
                scope,
                slot_id,
                direction,
            } => {
                if let Some(index) = self.select_queue_slot(scope, slot_id) {
                    match direction {
                        QueueMove::Up => self.app.move_queue_item_up(index),
                        QueueMove::Down => self.app.move_queue_item_down(index),
                    }
                }
            }
            QueueRequest::Undo { scope } => {
                // The component can still be showing (and emit for) `Remote`
                // for a frame after a remote disconnect, before the projection
                // refresh flips it back. Once no direct remote queue exists the
                // visible queue is the Local one, so treat the undo as
                // targeting Local rather than flashing a spurious error.
                let scope = if scope == QueueScope::Remote && !self.app.has_direct_remote_queue() {
                    QueueScope::Local
                } else {
                    scope
                };
                if scope == QueueScope::Remote {
                    self.app.flash(
                        "Undo is not supported for remote queue edits".into(),
                        ToastSeverity::Error,
                    );
                } else {
                    self.app.undo_last_queue_edit(scope);
                }
            }
        }
    }

    pub(super) fn handle_queue_intent(&mut self, intent: QueueIntent) {
        match intent {
            QueueIntent::Clear => self.app.request_clear_queue(),
            QueueIntent::ResizeColumn(direction) => {
                if self.app.effective_panel_mode() == super::PanelMode::Both {
                    self.app
                        .resize_queue_column(direction == QueueColumnResize::Wider);
                }
            }
            QueueIntent::StopRemoteTracking => {
                if self.app.remote_tracker.is_some() {
                    self.app.stop_remote_tracking();
                }
            }
            QueueIntent::ReanchorRemoteTracking => {
                if self.app.remote_tracker.is_some() {
                    self.app.reanchor_remote_tracking();
                }
            }
            QueueIntent::PlayNow => {
                let (active, current_idx) = {
                    let status = self.app.player.status.lock().unwrap();
                    (status.active, status.current_idx)
                };
                if active {
                    self.app.playback_queue_mut().queue_cursor = current_idx;
                    if self.app.player.is_remote() {
                        self.app.set_queue_scope(QueueScope::Remote);
                    }
                    // Jump-to-now-playing is an explicit, authoritative move.
                    self.app.queue_cursor_pushed = Some(QueueCursorPush::Reanchor(
                        self.app.playback_target_queue_scope(),
                    ));
                } else {
                    self.app
                        .flash("Nothing is playing".into(), ToastSeverity::Error);
                }
            }
            QueueIntent::SavePlaylist => {
                if self.app.player_tab.total_queue_len() > 0 {
                    self.app
                        .open_save_playlist_dialog(super::SavePlaylistDialog {
                            input: self.app.queue_playlist_name().to_string(),
                            stage: super::SavePlaylistStage::EnterName,
                        });
                }
            }
            QueueIntent::Navigate { scope, slot_id } => {
                let Some(cursor) = self.select_queue_slot(scope, slot_id) else {
                    return;
                };
                let Some(item) = self.app.queue_for_scope(scope).emby_item_at(cursor) else {
                    return;
                };
                let item_id = item.id.clone();
                let item_type = item.item_type.clone();
                let libs = self
                    .app
                    .libs
                    .iter()
                    .enumerate()
                    .map(|(i, lib)| {
                        (
                            i,
                            lib.library.id.clone(),
                            lib.library.collection_type.clone(),
                        )
                    })
                    .collect();
                self.app.spawn_navigate_to_item(item_id, item_type, libs);
            }
        }
    }

    /// Resolves `slot_id` to its index in `scope`'s queue and applies the
    /// scope/focus/hold-window side effects, returning the resolved index.
    /// The index is the operand for the shell-owned effect the caller is
    /// about to run (D2: `remove_from_queue(index)`, `move_queue_item_up/
    /// down(index)`, `Command::QueuePlayCursor(index)`). The component's
    /// own cursor is authoritative for selection; App's `queue_cursor` is
    /// not written here (task 3.1: the mirror is gone).
    fn select_queue_slot(
        &mut self,
        scope: QueueScope,
        slot_id: mbv_core::playback_queue::QueueSlotId,
    ) -> Option<usize> {
        if scope == QueueScope::Remote && !self.app.has_direct_remote_queue() {
            return None;
        }
        let index = self
            .app
            .queue_for_scope(scope)
            .slots()
            .iter()
            .position(|slot| slot.slot_id == slot_id)?;
        self.app.set_queue_scope(scope);
        self.app.set_panel_focus(PanelFocus::Queue);
        self.app.mark_queue_cursor_user_active();
        Some(index)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::components::{Msg, QueueRequest};
    use crate::app::tests::{make_app_stub, make_item, make_remote_app_stub};
    use tuirealm::event::{Event, Key, KeyEvent, KeyModifiers};

    #[test]
    fn queue_arrow_moves_component_cursor_only_not_app_follow() {
        // QueueRequest::Cursor is plain component navigation: arrowing in
        // the mounted component moves only the component's own cursor; App's
        // `queue_cursor` (the shell-owned follow position) is not written
        // (task 3.2 — the mirror in select_queue_slot is gone).
        let mut app = make_app_stub();
        app.player_tab.set_queue_items(
            vec![
                mbv_core::playback_queue::QueueItem::Emby(Box::new(make_item("one", "Movie"))),
                mbv_core::playback_queue::QueueItem::Emby(Box::new(make_item("two", "Movie"))),
            ],
            0,
        );
        app.panel_focus = PanelFocus::Queue;
        let mut model = Model::new(app);
        model.sync_queue();
        let id = ComponentId::Queue;
        let component_cursor = |model: &Model| {
            model
                .application
                .get_component(&id)
                .and_then(|component| {
                    component
                        .as_any()
                        .downcast_ref::<QueueComponent>()
                        .map(QueueComponent::test_cursor)
                })
                .expect("Queue component mounted")
        };
        assert_eq!(component_cursor(&model), 0);
        assert_eq!(model.app.player_tab.queue_cursor, 0);

        let message = model
            .application
            .get_component_mut(&id)
            .expect("Queue component mounted")
            .on(&Event::Keyboard(KeyEvent {
                code: Key::Down,
                modifiers: KeyModifiers::NONE,
            }));
        let Some(Msg::Queue(request @ QueueRequest::Cursor { .. })) = message else {
            panic!("queue navigation must emit a slot cursor request");
        };
        // The request carries the moved-to slot; the component cursor moved.
        model.handle_queue_request(request);
        assert_eq!(
            component_cursor(&model),
            1,
            "component cursor moved to row 1"
        );
        assert_eq!(
            model.app.player_tab.queue_cursor, 0,
            "QueueRequest::Cursor must not write App's follow cursor"
        );
    }

    fn emby_items(n: usize) -> Vec<mbv_core::playback_queue::QueueItem> {
        (0..n)
            .map(|i| {
                mbv_core::playback_queue::QueueItem::Emby(Box::new(make_item(
                    &format!("row-{i}"),
                    "Movie",
                )))
            })
            .collect()
    }

    fn queue_cursor(model: &Model) -> usize {
        model
            .application
            .get_component(&ComponentId::Queue)
            .and_then(|c| c.as_any().downcast_ref::<QueueComponent>())
            .map(QueueComponent::test_cursor)
            .expect("Queue component mounted")
    }

    fn press_down(model: &mut Model) {
        model
            .application
            .get_component_mut(&ComponentId::Queue)
            .expect("Queue component mounted")
            .on(&Event::Keyboard(KeyEvent {
                code: Key::Down,
                modifiers: KeyModifiers::NONE,
            }));
    }

    #[test]
    fn stale_follow_push_yields_to_a_user_navigation_after_it_was_armed() {
        // Finding 2: a follow push must not snap the selection back onto the
        // playhead slot once the user has arrowed away in the meantime.
        // The playhead cursor (App `queue_cursor`) stays at row 0 while the
        // component sits on row 1, so a consumed `Set` would visibly move the
        // component and this assertion fails if the hold-window guard is
        // dropped from `sync_queue`.
        let mut app = make_app_stub();
        app.player_tab.set_queue_items(emby_items(3), 0);
        app.panel_focus = PanelFocus::Queue;
        let mut model = Model::new(app);
        model.sync_queue();

        // User arrows down to row 1 and the shell records the navigation
        // (arming the hold window via `select_queue_slot`).
        let msg = model
            .application
            .get_component_mut(&ComponentId::Queue)
            .unwrap()
            .on(&Event::Keyboard(KeyEvent {
                code: Key::Down,
                modifiers: KeyModifiers::NONE,
            }));
        let Some(Msg::Queue(request)) = msg else {
            panic!("Down must emit a Cursor request");
        };
        model.handle_queue_request(request);
        assert_eq!(
            queue_cursor(&model),
            1,
            "component moved under user control"
        );
        assert_eq!(
            model.app.player_tab.queue_cursor, 0,
            "the playhead cursor is still the stale row 0"
        );

        // A follow push for the visible scope arms *after* the navigation.
        model.app.queue_cursor_pushed = Some(QueueCursorPush::Follow(QueueScope::Local));
        model.sync_queue();

        assert_eq!(
            queue_cursor(&model),
            1,
            "user navigation wins; the follow push must not re-snap to row 0"
        );
        assert!(
            model.app.queue_cursor_pushed.is_none(),
            "the stale push is cleared"
        );
    }

    #[test]
    fn cursor_push_is_scope_aware() {
        // Finding 3: a push armed for Remote scope (e.g. a remote daemon queue
        // update) must not force the component while the user views Local.
        let mut app = make_remote_app_stub(
            crate::app::tests::make_items(3),
            crate::app::tests::make_items(3),
        );
        app.queue_scope = QueueScope::Local;
        app.panel_focus = PanelFocus::Queue;
        let mut model = Model::new(app);
        model.sync_queue();
        assert_eq!(model.app.visible_queue_scope(), QueueScope::Local);

        model.app.player_tab.queue_cursor = 2;
        model.app.queue_cursor_pushed = Some(QueueCursorPush::Follow(QueueScope::Remote));
        model.sync_queue();
        assert_eq!(
            queue_cursor(&model),
            0,
            "a Remote-scoped push must not move the Local view"
        );
        assert!(
            model.app.queue_cursor_pushed.is_none(),
            "stale push cleared"
        );

        // A push armed for the visible scope still applies.
        model.app.queue_cursor_pushed = Some(QueueCursorPush::Follow(QueueScope::Local));
        model.sync_queue();
        assert_eq!(queue_cursor(&model), 2, "matching-scope push applies");
    }

    #[test]
    fn full_replacement_reanchors_instead_of_preserving() {
        // Finding 4: a full queue replacement regenerates slot ids, so a
        // preserved selection could collide with an unrelated new slot. The
        // replacement must arm a Set push to the new start index.
        let mut app = make_app_stub();
        app.player_tab.set_queue_items(emby_items(3), 0);
        app.panel_focus = PanelFocus::Queue;
        let mut model = Model::new(app);
        model.sync_queue();

        press_down(&mut model);
        press_down(&mut model);
        assert_eq!(queue_cursor(&model), 2);

        model
            .app
            .replace_playback_queue(crate::app::tests::make_items(4), 1);
        assert_eq!(
            model.app.queue_cursor_pushed,
            Some(QueueCursorPush::Reanchor(QueueScope::Local)),
            "a replacement arms a Reanchor push"
        );

        model.sync_queue();
        assert_eq!(
            queue_cursor(&model),
            1,
            "component re-anchors to the replacement's start index"
        );
    }

    #[test]
    fn remote_undo_falls_back_to_local_when_no_direct_remote_queue() {
        // Finding 5: after a remote disconnect the still-mounted component can
        // emit Undo { scope: Remote } for a frame. With no direct remote queue
        // the visible queue is Local, so undo the Local edit instead of
        // flashing an error.
        use crate::app::types_playback::UndoEntry;
        let mut app = make_app_stub();
        app.player_tab.set_queue_items(emby_items(2), 0);
        app.queue_undo_stack.push(UndoEntry::Remove(
            0,
            mbv_core::playback_queue::QueueItem::Emby(Box::new(make_item("restored", "Movie"))),
        ));
        let mut model = Model::new(app);
        assert!(!model.app.has_direct_remote_queue());

        model.handle_queue_request(QueueRequest::Undo {
            scope: QueueScope::Remote,
        });

        assert_ne!(
            model.app.status_severity,
            ToastSeverity::Error,
            "no spurious remote-undo-unsupported error"
        );
        assert!(
            model.app.queue_undo_stack.is_empty(),
            "the Local undo entry was consumed"
        );
        assert_eq!(
            model.app.player_tab.total_queue_len(),
            3,
            "the removed item was restored to the Local queue"
        );
    }
}
