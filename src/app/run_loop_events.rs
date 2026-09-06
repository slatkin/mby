use super::{App, QueueCursorPush, QueueScope};
use mbv_core::playback_queue::QueueSlotId;
use mbv_core::remote_reconciliation::{ReconciliationEffect, RemoteObservation};

#[path = "run_loop_events_session.rs"]
mod run_loop_events_session;
#[path = "run_loop_events_teardown.rs"]
mod run_loop_events_teardown;

impl App {
    fn apply_remote_observation(&mut self, session: &mbv_core::api::SessionInfo, generation: u64) {
        let Some(tracker) = self.remote_tracker.as_mut() else {
            return;
        };
        let observation = if let Some(media_id) = session.now_playing_item_id.clone() {
            RemoteObservation::playing(
                generation,
                session.id.clone(),
                media_id,
                session.position_ticks,
                session.runtime_ticks,
                Self::now_ms(),
            )
        } else {
            RemoteObservation::stopped(generation, session.id.clone(), Self::now_ms())
        };
        let effects = tracker.observe(observation);
        for effect in effects {
            match effect {
                ReconciliationEffect::Completion(item) => {
                    log::info!(
                        target: "remote_reconciliation",
                        "completed remote occurrence={} media={}",
                        item.occurrence_id,
                        item.media_id
                    );
                    self.consume_remote_occurrence(&item);
                }
                ReconciliationEffect::StateChanged { state, reason } => {
                    log::debug!(
                        target: "remote_reconciliation",
                        "tracking state={state:?} reason={reason:?}"
                    );
                }
                _ => {}
            }
        }
    }

    /// Consume, as in ncmpcpp: a finished item leaves the queue. Nothing here
    /// touches a playlist. Persisting the shortened queue back to a saved
    /// playlist is the separate `save_playlist_on_consume` feature, applied by
    /// `on_video_consumed`/`on_audio_consumed` exactly as for local playback.
    fn consume_remote_occurrence(
        &mut self,
        occurrence: &mbv_core::remote_reconciliation::SubmittedOccurrence,
    ) {
        let Some(tracker) = self.remote_tracker.as_ref() else {
            return;
        };
        let session_id = tracker.session_id().to_string();
        let epoch = tracker.epoch();
        let Some(slot_id) = self.projected_queue_slot(&session_id, epoch, occurrence.occurrence_id)
        else {
            log::warn!(
                target: "remote_reconciliation",
                "no queue slot projected for occurrence={}; not consuming",
                occurrence.occurrence_id
            );
            return;
        };
        let (should_consume, is_audio) = self.should_consume_slot(slot_id, true);
        if !should_consume {
            return;
        }
        // The tracker still gates repeat completions for one occurrence.
        if !self
            .remote_tracker
            .as_mut()
            .is_some_and(|tracker| tracker.mark_consumed(occurrence.occurrence_id))
        {
            return;
        }
        if !self.remove_projected_queue_slot(slot_id) {
            return;
        }
        if is_audio {
            self.on_audio_consumed();
        } else {
            self.on_video_consumed();
        }
    }

    /// The queue slot a tracked occurrence currently maps to, or `None` when
    /// the projection no longer describes this tracker or this queue lineage.
    fn projected_queue_slot(
        &self,
        session_id: &str,
        epoch: u64,
        occurrence_id: u64,
    ) -> Option<QueueSlotId> {
        let projection = self.remote_queue_projection.as_ref()?;
        (projection.session_id == session_id
            && projection.epoch == epoch
            && projection.queue_lineage == self.remote_queue_lineage)
            .then(|| projection.occurrence_slots.get(&occurrence_id).copied())
            .flatten()
    }

    /// Remove a slot from the queue an attached Emby session is playing,
    /// holding the display cursor on whatever it was already pointing at.
    /// Returns whether the slot was actually removed.
    fn remove_projected_queue_slot(&mut self, slot_id: QueueSlotId) -> bool {
        let selected_slot = self
            .player_tab
            .queue
            .slots()
            .get(self.player_tab.queue_cursor)
            .map(|slot| slot.slot_id);
        if !matches!(
            self.player_tab.queue.consume_slot(slot_id),
            mbv_core::playback_queue::QueueMutationResult::Applied(_)
        ) {
            return false;
        }
        self.player_tab.clamp_cursor();
        if let Some(index) =
            selected_slot.and_then(|selected| self.player_tab.queue.slot_index(selected))
        {
            self.player_tab.queue_cursor = index;
            // Projected removal from an attached session's queue: a
            // follow-the-playhead move on the Local-scope queue.
            self.playhead.pending_push = Some(QueueCursorPush::Follow(QueueScope::Local));
        }
        // An attached session must still persist an intentionally empty queue;
        // the ordinary empty-save guard protects unrelated remote-control UI.
        self.save_queue_state_after_remote_projection();
        true
    }
}
