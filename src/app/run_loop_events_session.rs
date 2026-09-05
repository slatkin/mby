//! `SessionEvent` handling, split out of `run_loop_events.rs` to keep that
//! file within the repository's file-size limit.

use crate::app::notify_actions::ToastSeverity;
use crate::app::{App, PanelFocus, QueueCursorPush, QueueScope, SessionEvent, SidebarId};
use std::time::{Duration, Instant};

impl App {
    /// Handle a single `SessionEvent` from the sessions-poll channel. Faithful
    /// transcription of the match arms previously inlined in `run()`'s
    /// `sessions_rx` drain loop (see `drain_session_events`).
    pub(in crate::app) fn handle_session_event(&mut self, ev: SessionEvent) {
        match ev {
            SessionEvent::Loaded {
                sessions,
                generation,
            } => {
                self.sessions = sessions;
                self.sessions_loading = false;
                self.last_session_poll = Instant::now();
                // Rebuilds the F3 panel's merged Emby+Cast list and
                // re-locates the panel cursor by identity (8.1); this
                // supersedes what used to be a `self.sessions`-only
                // old_id/cursor-clamp here.
                self.rebuild_panel_targets();
                // Update connected session state; auto-disconnect if gone
                if let Some(ref conn_id) = self.connected_session_id.clone() {
                    if let Some(s) = self.sessions.iter().find(|s| &s.id == conn_id).cloned() {
                        // Maintain a monotonic position estimate within a single video.
                        // Reset the anchor only when the playing item ID changes.
                        // Avoid keying on runtime or title — the API occasionally returns
                        // missing RunTimeTicks (as_i64 returns None → 0) or a slightly
                        // different name, which would spuriously reset the position anchor
                        // every poll and prevent smooth interpolation.
                        let now = Instant::now();
                        let prev_item_id = self
                            .connected_session_state
                            .as_ref()
                            .and_then(|p| p.now_playing_item_id.as_deref());
                        let item_changed = s.now_playing_item_id.as_deref() != prev_item_id;
                        if item_changed {
                            // Refresh the previous item so played/progress reflects
                            // what the remote client reported to the server.
                            if let Some(prev_id) = self
                                .connected_session_state
                                .as_ref()
                                .and_then(|p| p.now_playing_item_id.clone())
                            {
                                if let Some(client) = self.emby_snapshot() {
                                    let tx = self.sessions_tx.clone();
                                    std::thread::spawn(move || {
                                        if let Ok(mut items) =
                                            client.get_items_by_ids(std::slice::from_ref(&prev_id))
                                        {
                                            if let Some(fresh) = items.pop() {
                                                let _ = tx.send(SessionEvent::ItemRefreshed(
                                                    prev_id,
                                                    Box::new(fresh),
                                                ));
                                            }
                                        }
                                    });
                                }
                            }
                        }
                        // Detect playback via API position advancing, not IsPaused.
                        // Some Emby clients always report IsPaused=true even while playing;
                        // the only reliable signal is that PositionTicks keeps moving.
                        let prev_api_pos = self
                            .connected_session_state
                            .as_ref()
                            .map_or(0, |p| p.position_s);
                        if s.position_s > prev_api_pos {
                            self.remote_api_pos_advanced_at = now;
                            self.remote_stalled_while_paused = false;
                        } else if s.is_paused {
                            // Position not advancing AND the session says paused:
                            // the transport is genuinely paused. Buggy clients
                            // that report IsPaused=true while playing still
                            // advance the position each poll, so this branch
                            // won't latch on for them.
                            self.remote_stalled_while_paused = true;
                        }
                        // Extrapolate if API advanced recently (within 2× the ~11s report
                        // interval). After that window lapses we treat it as paused/stopped.
                        let api_active = self.remote_api_pos_advanced_at.elapsed().as_secs() < 22;
                        let seek_pending = now < self.remote_seek_pending_until;
                        if seek_pending && !item_changed {
                            // A seek was just dispatched; hold the optimistic position until
                            // the API catches up. Once the API reports the new position (or
                            // the window expires) we fall through to normal reconciliation.
                            log::debug!(target: "sessions",
                                "pos hold (seek pending): api={}s remote_pos_s={}s",
                                s.position_s, self.remote_pos_s);
                        } else if item_changed {
                            log::debug!(target: "sessions",
                                "pos reset (item change): api_pos={}s → remote_pos_s {}s→{}s",
                                s.position_s, self.remote_pos_s, s.position_s);
                            self.remote_pos_s = s.position_s;
                            self.remote_api_pos_advanced_at = now;
                            self.remote_seek_pending_until = now - Duration::from_secs(1);
                        } else if api_active {
                            let elapsed = self.remote_pos_at.elapsed().as_secs_f64();
                            let extrapolated = Self::extrapolated_remote_position(
                                self.remote_pos_s,
                                self.remote_pos_at.elapsed(),
                            );
                            let new_pos = s.position_s.max(extrapolated);
                            log::debug!(target: "sessions",
                                "pos extrap: api={}s paused={} elapsed={:.2}s → remote_pos_s {}s→{}s",
                                s.position_s, s.is_paused, elapsed, self.remote_pos_s, new_pos);
                            self.remote_pos_s = new_pos;
                        } else {
                            log::debug!(target: "sessions",
                                "pos idle (no api advance in 22s): api_pos={}s → remote_pos_s {}s→{}s",
                                s.position_s, self.remote_pos_s, s.position_s);
                            self.remote_pos_s = s.position_s;
                        }
                        if !seek_pending || item_changed {
                            self.remote_pos_at = now;
                        }
                        if item_changed {
                            if !self.queue_cursor_held_by_user() {
                                if let Some(new_idx) =
                                    s.now_playing_item_id.as_ref().and_then(|id| {
                                        self.player_tab
                                            .queue
                                            .slots()
                                            .iter()
                                            .position(|slot| slot.item.id() == id)
                                    })
                                {
                                    self.player_tab.queue_cursor = new_idx;
                                    // Attached-Emby session item change: a
                                    // follow-the-playhead move on Local scope.
                                    self.playhead.pending_push =
                                        Some(QueueCursorPush::Follow(QueueScope::Local));
                                }
                            }
                            self.runtime_zero_since = None;
                        }
                        self.connected_session_state = Some(s.clone());
                        self.session_miss_count = 0;
                        self.apply_remote_observation(&s, generation);
                        // Remote hasn't started playing yet — repoll sooner.
                        // Cap fast-poll at 30 s: if runtime stays 0 that long the
                        // remote client likely won't report it and we stop hammering.
                        if s.runtime_s == 0 {
                            let since = self.runtime_zero_since.get_or_insert_with(Instant::now);
                            if since.elapsed() < Duration::from_secs(30) {
                                self.last_session_poll =
                                    Instant::now() - Duration::from_millis(500);
                            }
                        } else {
                            self.runtime_zero_since = None;
                        }
                    } else {
                        self.session_miss_count += 1;
                        // A poll gap means the connected session is not
                        // currently observable, but the logical attachment is
                        // still held (capable of observing a return), so
                        // tracking suspends rather than staying confidently
                        // current or retiring early. Only the three-miss
                        // policy clears the attachment, and tracking retires
                        // in that same transition (below).
                        if let Some(tracker) = self.remote_tracker.as_mut() {
                            tracker.session_disappeared();
                        }
                        if self.session_miss_count >= 3 {
                            log::warn!(target: "sessions", "connected session gone; disconnecting");
                            self.flash(
                                "Remote session ended; disconnected".to_string(),
                                ToastSeverity::Error,
                            );
                            self.connected_session_id = None;
                            self.connected_session_state = None;
                            self.retire_remote_tracking(false);
                            self.session_miss_count = 0;
                            self.remote_pos_s = 0;
                        } else {
                            log::warn!(target: "sessions", "connected session not in poll ({}/3); holding", self.session_miss_count);
                        }
                    }
                }
            }
            SessionEvent::ItemRefreshed(item_id, fresh) => {
                if let Some(slot_id) = self
                    .player_tab
                    .queue
                    .slots()
                    .iter()
                    .find(|s| s.item.id() == item_id)
                    .map(|s| s.slot_id)
                {
                    let _ = self.player_tab.queue.update_slot_item(
                        slot_id,
                        mbv_core::playback_queue::QueueItem::Emby(fresh),
                    );
                }
            }
            SessionEvent::CommandAcknowledged(command) => {
                if let Some(tracker) = self.remote_tracker.as_mut() {
                    if tracker.session_id() == command.session_id
                        && tracker.tracking_id() == command.tracking_id
                        && tracker.epoch() == command.tracker_epoch
                    {
                        tracker.acknowledge_command(command.generation);
                    }
                }
            }
            SessionEvent::CommandError {
                error,
                reconciliation,
            } => {
                if let (Some(command), Some(tracker)) =
                    (reconciliation, self.remote_tracker.as_mut())
                {
                    if tracker.session_id() == command.session_id
                        && tracker.tracking_id() == command.tracking_id
                        && tracker.epoch() == command.tracker_epoch
                        && tracker.command_generation_matches(command.generation)
                    {
                        tracker.command_failed();
                        self.retire_remote_tracking(false);
                    }
                }
                self.flash(
                    format!("Remote command failed: {error}"),
                    ToastSeverity::Error,
                );
            }
            SessionEvent::PlaylistMutationComplete {
                mutation_id,
                playlist_id,
                queue_lineage,
                source_playlist_id,
                result,
            } => {
                let succeeded = result.is_ok();
                if let Err(error) = result {
                    self.flash(
                        format!("Playlist save failed: {error}"),
                        ToastSeverity::Error,
                    );
                } else if queue_lineage == self.remote_queue_lineage
                    && self.queue_playlist_id() == Some(source_playlist_id.as_str())
                {
                    self.queue_dirty = false;
                    // A successful Save recreated server entry identities
                    // (cleared locally at the mutation boundary); persist that
                    // cleared state so stale identities cannot survive restart.
                    self.save_queue_state();
                }
                self.finish_playlist_mutation(&playlist_id, mutation_id);
                if succeeded
                    && queue_lineage == self.remote_queue_lineage
                    && self.queue_playlist_id() == Some(playlist_id.as_str())
                    && self.pending_queue_action.is_some()
                {
                    if let Some(action) = self.pending_queue_action.take() {
                        self.execute_pending_queue_action(action);
                    }
                    self.request_sidebar_dismiss(SidebarId::Playlists);
                    self.set_panel_focus(PanelFocus::Queue);
                }
            }
            SessionEvent::PlaylistReplacementComplete {
                mutation_id,
                playlist_id,
                queue_lineage,
                source_playlist_id,
                name,
                result,
            } => {
                match result {
                    Ok(id) if queue_lineage == self.remote_queue_lineage => {
                        if self.remote_tracking_source_is(&source_playlist_id) {
                            self.retire_remote_tracking(true);
                        }
                        self.queue_source =
                            crate::config::QueueSource::Playlist { id: Some(id), name };
                        self.queue_dirty = false;
                        // The queue now identifies the replacement playlist; its
                        // items must not retain entry identities from a previously
                        // current source. Persist before reporting the overwrite
                        // clean so stale identities cannot survive restart.
                        self.clear_local_playlist_entry_ids();
                        self.save_queue_state();
                    }
                    Ok(_) => {
                        log::debug!(target: "playlist", "discarding stale playlist replacement completion")
                    }
                    Err(error) => self.flash(
                        format!("Playlist overwrite failed: {error}"),
                        ToastSeverity::Error,
                    ),
                }
                self.finish_playlist_mutation(&playlist_id, mutation_id);
            }
            SessionEvent::PlaylistCreateComplete {
                mutation_id,
                coordinator_key,
                name,
                queue_lineage,
                source_playlist_id,
                result,
            } => {
                match result {
                    Ok(id)
                        if queue_lineage == self.remote_queue_lineage
                            && self.queue_playlist_id() == source_playlist_id.as_deref() =>
                    {
                        self.queue_source = crate::config::QueueSource::Playlist {
                            id: Some(id),
                            name: name.clone(),
                        };
                        self.queue_dirty = false;
                        // The new source must never retain entry identities
                        // from the old playlist.
                        self.clear_local_playlist_entry_ids();
                        self.save_queue_state();
                        self.flash(
                            format!("Saved as playlist \"{name}\""),
                            ToastSeverity::Success,
                        );
                    }
                    Ok(_) => {
                        log::debug!(target: "playlist", "discarding stale Save As completion");
                    }
                    Err(error) => self.flash(
                        format!("Playlist save failed: {error}"),
                        ToastSeverity::Error,
                    ),
                }
                self.finish_playlist_mutation(&coordinator_key, mutation_id);
            }
            SessionEvent::Error(e) => {
                self.sessions_loading = false;
                self.flash(format!("Sessions error: {e}"), ToastSeverity::Error);
            }
        }
    }
}
