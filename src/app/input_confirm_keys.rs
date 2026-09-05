use super::notify_actions::ToastSeverity;
use super::{
    App, ConfirmAction, ConfirmModal, PanelFocus, PendingQueueAction, QueueScope,
    SavePlaylistDialog, SavePlaylistStage, SidebarId, UndoEntry,
};
use crossterm::event::{KeyCode, KeyEvent};
use mbv_core::playback_queue::RemoveSlotResult;

impl App {
    /// Shared dispatcher for the confirmation-modal component (see
    /// `render/overlays/confirm_modal.rs`, `types_confirm.rs`): matches on
    /// which `ConfirmAction` is pending and re-uses each action's existing
    /// effect, preserving the exact key bindings each confirmation had
    /// before migrating off status-bar toast text / bespoke dialogs.
    pub(super) fn apply_confirm_action(
        &mut self,
        action: ConfirmAction,
        key: KeyEvent,
    ) -> Option<bool> {
        match action {
            ConfirmAction::ClearQueue => {
                if matches!(
                    key.code,
                    KeyCode::Char('y') | KeyCode::Char('Y') | KeyCode::Enter
                ) {
                    self.replace_queue_or_prompt(PendingQueueAction::ClearQueue);
                }
            }
            ConfirmAction::RemoveActiveQueueItem(pos) => {
                if matches!(key.code, KeyCode::Char('y')) {
                    let scope = self.viewed_queue_scope();
                    let slot_id = self.queue_for_scope_mut(scope).slot_id_at(pos);
                    if let Some(slot_id) = slot_id {
                        let removed_item = match self
                            .playback_queue_mut()
                            .queue
                            .remove_active_slot_confirmed(slot_id)
                        {
                            RemoveSlotResult::Removed(slot) => {
                                self.playback_queue_mut().clamp_cursor();
                                Some(slot.item)
                            }
                            RemoveSlotResult::RequiresActiveConfirmation(_)
                            | RemoveSlotResult::NotFound => None,
                        };
                        if let Some(item) = removed_item {
                            let queue = self.playback_queue_mut();
                            queue.clamp_cursor();
                            if !self.player.is_remote() {
                                self.queue_undo_stack.push(UndoEntry::Remove(pos, item));
                            }
                            self.pending_delete_slot = Some(slot_id);
                            if self.connected_session_id.is_some() {
                                self.playback_target().stop(self);
                            } else {
                                self.player.stop();
                            }
                            if self.local_queue_metadata_applies(scope) {
                                self.queue_dirty = true;
                            }
                            self.retire_remote_tracking(true);
                        }
                    }
                }
            }
            ConfirmAction::RescanLibrary(lib_idx) => {
                if matches!(
                    key.code,
                    KeyCode::Char('y') | KeyCode::Char('Y') | KeyCode::Enter
                ) {
                    self.trigger_lib_rescan(lib_idx);
                }
            }
            ConfirmAction::SaveOverwritePlaylist { existing_id, name } => match key.code {
                KeyCode::Char('y') => {
                    self.do_overwrite_playlist(&existing_id, &name);
                }
                KeyCode::Esc => {
                    self.open_save_playlist_dialog(SavePlaylistDialog {
                        input: name,
                        stage: SavePlaylistStage::EnterName,
                    });
                }
                _ => {}
            },
            ConfirmAction::DeletePlaylist { id, name } => match key.code {
                KeyCode::Char('y') => {
                    self.spawn_delete_playlist(id, name);
                }
                KeyCode::Esc => {}
                _ => {}
            },
            ConfirmAction::RemoveFeedSubscription(index) => {
                if matches!(key.code, KeyCode::Char('y')) {
                    self.remove_feed_confirmed(index);
                }
            }
            ConfirmAction::RemoveEmby => {
                if matches!(
                    key.code,
                    KeyCode::Char('y') | KeyCode::Char('Y') | KeyCode::Enter
                ) {
                    self.remove_emby_confirmed();
                } else if key.code == KeyCode::Esc {
                }
            }
            ConfirmAction::ReplaceEmby(generation) => {
                if key.code == KeyCode::Esc {
                    self.pending_emby_replacement = None;
                } else if matches!(
                    key.code,
                    KeyCode::Char('y') | KeyCode::Char('Y') | KeyCode::Enter
                ) {
                    self.replace_emby_confirmed(generation);
                }
            }
            ConfirmAction::RemoveAudiobookshelf => {
                if matches!(
                    key.code,
                    KeyCode::Char('y') | KeyCode::Char('Y') | KeyCode::Enter
                ) {
                    self.remove_audiobookshelf_confirmed();
                } else if key.code == KeyCode::Esc {
                }
            }
            ConfirmAction::ReplaceAudiobookshelf(generation) => {
                if key.code == KeyCode::Esc {
                    self.pending_audiobookshelf_replacement = None;
                } else if matches!(
                    key.code,
                    KeyCode::Char('y') | KeyCode::Char('Y') | KeyCode::Enter
                ) {
                    self.replace_audiobookshelf_confirmed(generation);
                }
            }
            ConfirmAction::DiscardOrSaveDirtyPlaylist => {
                let play_after = matches!(
                    self.pending_queue_action,
                    Some(PendingQueueAction::PlayItems { .. })
                );
                match key.code {
                    KeyCode::Char('s') | KeyCode::Char('S') => {
                        self.save_playlist_to_emby();
                    }
                    KeyCode::Char('d') | KeyCode::Char('D') => {
                        if let Some(action) = self.pending_queue_action.take() {
                            self.execute_pending_queue_action(action);
                        }
                        if play_after {
                            self.request_sidebar_dismiss(SidebarId::Playlists);
                            self.set_panel_focus(PanelFocus::Queue);
                        }
                    }
                    KeyCode::Esc | KeyCode::Char('c') | KeyCode::Char('C') => {
                        self.pending_queue_action = None;
                    }
                    _ => {}
                }
            }
        }
        Some(false)
    }

    /// Show the clear-queue confirmation modal (called from QueueIntent::Clear).
    pub(super) fn request_clear_queue(&mut self) {
        let scope = self.viewed_queue_scope();
        // Legacy `handle_key_clear_queue_prompt` refused a Queue-focused remote
        // scope outright, which also swallowed `c` for a socket-attached mbvd
        // whose queue the confirm's `y` handler can clear. Narrow the refusal to
        // a connected Emby session (queue owned on the remote device); the
        // direct-remote daemon queue falls through to the same prompt a local
        // queue gets.
        if scope == QueueScope::Remote && self.connected_session_id.is_some() {
            self.flash(
                "Remote queue is controlled by the daemon".into(),
                ToastSeverity::Error,
            );
            return;
        }
        if self.queue_for_scope(scope).total_queue_len() == 0 {
            return;
        }
        self.ask_confirm(ConfirmModal {
            title: " Clear Queue ".into(),
            message: "Clear the queue?".into(),
            hint: "[y] Confirm    [Esc] Cancel".into(),
            on_confirm: ConfirmAction::ClearQueue,
        });
    }
}
