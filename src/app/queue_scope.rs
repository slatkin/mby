use super::notify_actions::ToastSeverity;
use super::{
    App, PendingQueueAction, PlayerTab, QueueCursorPush, QueueScope, QueueScopeResolution,
    UndoEntry,
};
use mbv_core::api::EmbyItem;
use mbv_core::playback_queue::{QueueItem, QueueMutationResult, QueueSlotId, RefreshMergeResult};
use mbv_core::player::PlayerCommand;

impl App {
    pub(super) fn has_remote_queue(&self) -> bool {
        self.remote_player_tab.is_some()
    }

    pub(super) fn has_direct_remote_queue(&self) -> bool {
        self.player.is_remote() && self.has_remote_queue()
    }

    pub(super) fn queue_for_scope(&self, scope: QueueScope) -> &PlayerTab {
        match scope {
            QueueScope::Local => &self.player_tab,
            QueueScope::Remote => self
                .remote_player_tab
                .as_ref()
                .expect("remote queue scope requires remote queue"),
        }
    }

    pub(super) fn queue_for_scope_mut(&mut self, scope: QueueScope) -> &mut PlayerTab {
        match scope {
            QueueScope::Local => &mut self.player_tab,
            QueueScope::Remote => self
                .remote_player_tab
                .as_mut()
                .expect("remote queue scope requires remote queue"),
        }
    }

    pub(super) fn undo_stack_for_scope_mut(&mut self, scope: QueueScope) -> &mut Vec<UndoEntry> {
        match scope {
            QueueScope::Local => &mut self.queue_undo_stack,
            QueueScope::Remote => &mut self.remote_queue_undo_stack,
        }
    }

    fn queue_scope_resolution(&self) -> QueueScopeResolution {
        QueueScopeResolution::new(self.has_direct_remote_queue(), self.queue_scope)
    }

    pub(super) fn local_queue_metadata_applies(&self, scope: QueueScope) -> bool {
        self.queue_scope_resolution().local_metadata_applies(scope)
    }

    pub(super) fn queue_scope_is_playback(&self, scope: QueueScope) -> bool {
        scope == self.playing_queue_scope()
    }

    fn action_queue_scope(&self, action: &PendingQueueAction) -> QueueScope {
        match action {
            PendingQueueAction::PlayItems { .. } => self.playing_queue_scope(),
            PendingQueueAction::ClearQueue => self.viewed_queue_scope(),
        }
    }

    pub(super) fn action_touches_local_queue(&self, action: &PendingQueueAction) -> bool {
        self.local_queue_metadata_applies(self.action_queue_scope(action))
    }

    pub(super) fn clear_local_queue_metadata(&mut self) {
        self.queue_source = crate::config::QueueSource::Unknown;
        self.queue_dirty = false;
        self.queue_undo_stack.clear();
    }

    pub(super) fn persist_local_queue_state_if_needed(&mut self, scope: QueueScope) {
        if self.local_queue_metadata_applies(scope) {
            self.save_queue_state();
        }
    }

    pub(super) fn replace_direct_remote_queue(&mut self, items: Vec<EmbyItem>, cursor: usize) {
        self.retire_remote_tracking(true);
        let cursor = cursor.min(items.len().saturating_sub(1));
        self.player
            .send_command(crate::player::PlayerCommand::ReplaceQueue {
                items: items.clone(),
                start_idx: cursor,
            });
        if let Some(queue) = self.remote_player_tab.as_mut() {
            queue.set_items(items, cursor);
        }
        // A full replacement regenerates slot ids, so a preserved prior
        // selection could collide with a new slot; force a re-anchor to the
        // replacement's start index.
        self.playhead.pending_push = Some(QueueCursorPush::Reanchor(QueueScope::Remote));
    }

    pub(super) fn sync_playback_queue_after_append(
        &mut self,
        scope: QueueScope,
        items: Vec<EmbyItem>,
    ) -> bool {
        self.sync_playback_queue_items_after_append(
            scope,
            items
                .into_iter()
                .map(|item| QueueItem::Emby(Box::new(item)))
                .collect(),
        )
    }

    pub(super) fn sync_playback_queue_items_after_append(
        &mut self,
        scope: QueueScope,
        items: Vec<QueueItem>,
    ) -> bool {
        if items.is_empty() || scope != self.playing_queue_scope() {
            return true;
        }
        if !self.player.is_remote() && !self.player.status.lock().unwrap().active {
            // A cold in-process player has no command channel yet. The app's
            // canonical queue is authoritative until the next Play starts it.
            return true;
        }
        let sent = self.player.queue_append(items);
        if !sent && self.player.is_remote() && !self.player.supports_queue_append() {
            self.flash(
                "Remote append is not supported by this direct mbv peer".to_string(),
                ToastSeverity::Error,
            );
            return false;
        }
        if !sent {
            self.flash(
                "Playback owner rejected the queue append".to_string(),
                ToastSeverity::Error,
            );
            return false;
        }
        true
    }

    pub(super) fn playing_queue_scope(&self) -> QueueScope {
        self.queue_scope_resolution().playback_target()
    }

    pub(super) fn replace_playback_queue(&mut self, items: Vec<EmbyItem>, cursor: usize) {
        self.retire_remote_tracking(true);
        let cursor = cursor.min(items.len().saturating_sub(1));
        match self.playing_queue_scope() {
            QueueScope::Local => {
                self.player_tab.set_items(items, cursor);
            }
            QueueScope::Remote => {
                let queue = self
                    .remote_player_tab
                    .as_mut()
                    .expect("direct remote playback queue requires remote queue");
                queue.set_items(items, cursor);
            }
        }
        // A full replacement regenerates slot ids: a preserved prior selection
        // could collide with an unrelated new slot, so force a re-anchor to
        // the replacement's start index rather than relying on `Preserve`.
        self.playhead.pending_push = Some(QueueCursorPush::Reanchor(self.playing_queue_scope()));
    }

    pub(super) fn viewed_queue_scope(&self) -> QueueScope {
        self.queue_scope_resolution().visible_scope()
    }

    pub(super) fn displayed_queue(&self) -> &PlayerTab {
        self.queue_for_scope(self.viewed_queue_scope())
    }

    pub(super) fn displayed_queue_mut(&mut self) -> &mut PlayerTab {
        self.queue_for_scope_mut(self.viewed_queue_scope())
    }

    pub(super) fn playback_queue(&self) -> &PlayerTab {
        self.queue_for_scope(self.playing_queue_scope())
    }

    pub(super) fn playback_queue_mut(&mut self) -> &mut PlayerTab {
        self.queue_for_scope_mut(self.playing_queue_scope())
    }

    pub(super) fn merge_refreshed_queue(
        &mut self,
        scope: QueueScope,
        fetched_items: Vec<EmbyItem>,
    ) -> RefreshMergeResult {
        let queue_len = self.queue_for_scope(scope).total_queue_len();
        let sync_player_prunes =
            scope == self.playing_queue_scope() && !self.has_direct_remote_queue();
        let active_index = if scope == self.playing_queue_scope() {
            let st = self.player.status.lock().unwrap();
            (st.active && st.current_idx < queue_len).then_some(st.current_idx)
        } else {
            None
        };
        let (result, pre_refresh_indices) = {
            let queue = self.queue_for_scope_mut(scope);
            queue.sync_active_slot(active_index);
            let pre_refresh_indices = sync_player_prunes.then(|| {
                queue
                    .queue
                    .slots()
                    .iter()
                    .enumerate()
                    .map(|(index, slot)| (slot.slot_id, index))
                    .collect::<std::collections::HashMap<_, _>>()
            });
            (queue.merge_refresh(fetched_items), pre_refresh_indices)
        };
        if let Some(pre_refresh_indices) = pre_refresh_indices {
            let mut pruned_indices: Vec<_> = result
                .pruned_slots
                .iter()
                .filter_map(|slot_id| pre_refresh_indices.get(slot_id).copied())
                .collect();
            pruned_indices.sort_unstable_by(|left, right| right.cmp(left));
            for index in pruned_indices {
                self.player.send_command(PlayerCommand::QueueRemove(index));
            }
        }
        result
    }

    /// Whether the previous/next transport controls (playback-header mouse
    /// buttons and, implicitly, the `P`/`N` keys) are currently at a usable
    /// queue position: `(prev_available, next_available)`.
    ///
    /// A connected remote session exposes no queue-position/length fields in
    /// `SessionInfo` (see `mbv_core::api::SessionInfo`), so there is no way to
    /// tell whether it's at a queue boundary; both remain available there,
    /// mirroring `Command::PreviousTrack`/`Command::NextTrack`'s dispatch, which
    /// calls `session_jump_track` unconditionally for a connected session with
    /// no boundary check. Local playback uses `PlayerStatus::previous_idx`/
    /// `next_idx`, which already fold in `active` and `queue_len`.
    pub(super) fn transport_prev_next_available(&self) -> (bool, bool) {
        if self.connected_session_id.is_some() {
            return (true, true);
        }
        let st = self.player.status.lock().unwrap();
        (st.previous_idx().is_some(), st.next_idx().is_some())
    }

    /// Slot-keyed check of whether a completed/stopped queue slot should be
    /// consumed, given a player-reported completion (`consume`) and the
    /// type-specific consume flags. Returns `(should_consume, is_audio)` --
    /// callers that act on the removal need `is_audio` afterward to route to
    /// `on_video_consumed`/`on_audio_consumed`. Resolves the audio/video
    /// flag from the queue model by slot identity instead of raw index.
    pub(super) fn should_consume_slot(&self, slot_id: QueueSlotId, consume: bool) -> (bool, bool) {
        let item = self.playback_queue().queue.slot(slot_id).map(|s| &s.item);
        let is_video = item.is_some_and(|i| i.is_video());
        let is_audio = item.is_some_and(|i| i.is_audio());
        let (consume_videos, consume_audio) = {
            let config = self.config.lock().unwrap();
            let cfg = &*config;
            (cfg.consume_videos, cfg.consume_audio)
        };
        let should_consume =
            consume && ((is_video && consume_videos) || (is_audio && consume_audio));
        log::info!(target: "consume", "consume check: slot_id={slot_id:?} consume={consume} \
            is_video={is_video} consume_videos={consume_videos} \
            is_audio={is_audio} consume_audio={consume_audio} => {should_consume}");
        (should_consume, is_audio)
    }

    /// Removes the given slot from the currently active playback queue by
    /// identity and, if something was actually removed, tells the player to
    /// drop the slot's current index from its own internal queue copy.
    /// Uses `consume_slot` rather than `remove_slot` so a slot that is
    /// currently marked active in the model (set via `set_active_slot`) can
    /// still be consumed; the active-confirmation gate on `remove_slot` only
    /// applies to explicit user-initiated removal. Returns the removed
    /// item's id, or `None` if the slot no longer exists.
    pub(super) fn consume_slot_from_active_playback_queue(
        &mut self,
        slot_id: QueueSlotId,
    ) -> Option<String> {
        let idx = self.playback_queue().queue.slot_index(slot_id)?;
        let removed = match self.playback_queue_mut().queue.consume_slot(slot_id) {
            QueueMutationResult::Applied(slot) => slot,
            QueueMutationResult::NotFound => return None,
        };
        self.playback_queue_mut().clamp_cursor();
        // Prefer slot-based removal for unified-capable remote peers.
        let sent_unified = self
            .player
            .queue_remove_slot(mbv_core::ctrl::slot_id_to_u64(slot_id));
        if !sent_unified {
            self.player.send_command(PlayerCommand::QueueRemove(idx));
        }
        Some(removed.item.id().to_string())
    }

    pub(super) fn set_queue_scope(&mut self, scope: QueueScope) {
        let resolved = if scope == QueueScope::Remote && self.has_direct_remote_queue() {
            QueueScope::Remote
        } else {
            QueueScope::Local
        };
        if resolved != self.queue_scope {
            // A real scope switch (`[`/`]`, scope pill, session
            // attach/detach) must adopt the new scope's own follow
            // position rather than reconciling by slot identity: the two
            // queues hand out colliding `QueueSlotId`s (each is a
            // per-`PlaybackQueue` counter starting at 1), so identity
            // reconciliation can park the cursor on an unrelated slot.
            self.playhead.pending_push = Some(QueueCursorPush::Reanchor(resolved));
        }
        self.queue_scope = resolved;
    }
}
