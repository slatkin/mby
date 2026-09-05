use super::notify_actions::ToastSeverity;
use super::{App, DaemonLostModal, QueueCursorPush, QUIT_REQUESTED};
use mbv_core::player::{PlayerCommand, PlayerEvent};
use std::sync::atomic::Ordering;

impl App {
    /// Mirror mpv's actual volume into `ui_volume` and persist it, so volume
    /// changes made inside the mpv window (not just via mbv's keys) are kept and
    /// restored on the next launch. Skipped while controlling a remote session
    /// (the remote owns its volume) and while temporarily muted (so a mute
    /// doesn't clobber the saved level with 0).
    pub(super) fn sync_volume_from_player(&mut self) {
        if self.connected_session_id.is_some() {
            return;
        }
        if self.pre_mute_volume.is_some() {
            return;
        }
        let player_vol = {
            let s = self.player.status.lock().unwrap();
            if s.active {
                Some(s.volume.clamp(0, 200) as u8)
            } else {
                None
            }
        };
        if let Some(v) = player_vol {
            if v != self.ui_volume {
                self.ui_volume = v;
                self.save_prefs();
            }
        }
    }

    /// Handle a PlayerEvent received from the player thread.
    /// Returns true if the caller's event loop should `continue` (skip render for this tick).
    pub(super) fn handle_player_event(&mut self, ev: PlayerEvent) -> bool {
        match ev {
            PlayerEvent::Stopped {
                idx,
                position_ticks,
                played,
                consume,
                progress_report_accepted,
                error,
            } => {
                log::info!(target: "player", "Stopped event: idx={idx} position_ticks={}s played={played} error={error:?}",
                    position_ticks / mbv_core::api::TICKS_PER_SECOND);
                if self.player.is_remote_disconnected() {
                    self.next_up_item = None;
                    // An announced shutdown never reaches here: the reader
                    // thread sends PlayerEvent::DaemonShutdownAnnounced
                    // instead of a synthetic Stopped for that case (see the
                    // arm below). Assert the invariant rather than silently
                    // trusting it -- getting it backwards is exactly the
                    // spurious-modal-vs-silent-exit boundary task 7.4 tests.
                    debug_assert!(
                        !self.player.is_shutdown_announced(),
                        "an announced daemon shutdown must never surface as PlayerEvent::Stopped"
                    );
                    // A client of a local daemon can offer to restart it; a
                    // client of a genuinely remote daemon cannot, and keeps
                    // today's silent-fallback behavior (task 7.2). Before
                    // any fallback, try an auto-reconnect reattach to the
                    // same remote daemon when the option is enabled.
                    if self.is_local_daemon() {
                        self.raise_daemon_lost_modal();
                    } else if self.try_reattach_remote_daemon() {
                        return true;
                    } else {
                        self.restore_local_mode("Daemon disconnected — returned to local mode");
                    }
                    self.refresh_after_stop();
                    return true;
                }
                let is_delete = self.pending_delete_slot.take().is_some();
                let preserve_local_state = !self.has_direct_remote_queue();
                // Resolve the raw mpv index to a slot right away.
                let slot_id = self.playback_queue().resolve_slot_at(idx);
                match slot_id {
                    Some(slot_id) => {
                        if !is_delete {
                            let position = if played {
                                0
                            } else if let Some(slot) = self.playback_queue().queue.slot(slot_id) {
                                if position_ticks > 0 && !slot.item.is_audio() {
                                    position_ticks
                                } else {
                                    slot.item.playback_position_ticks()
                                }
                            } else {
                                0
                            };
                            let queue = self.playback_queue_mut();
                            let _ = queue.queue.apply_progress(slot_id, position, played);
                            if progress_report_accepted {
                                let _ = queue.queue.mark_progress_sync_pending(slot_id);
                            }
                            queue.clamp_cursor();
                            // Persist Feed lifecycle state before any
                            // consume/removal changes the queue.
                            if let Some(slot) = self.playback_queue().queue.slot(slot_id) {
                                if matches!(slot.item, mbv_core::playback_queue::QueueItem::Feed(_))
                                {
                                    let runtime = slot.item.runtime_ticks();
                                    let feed_completed =
                                        played || (runtime > 0 && position >= runtime * 95 / 100);
                                    self.persist_feed_slot_lifecycle(
                                        slot_id,
                                        position,
                                        feed_completed,
                                    );
                                }
                            }
                            if played {
                                log::info!(target: "player", "Stopped: marked played, position reset to 0");
                            } else if position_ticks > 0 {
                                log::info!(target: "player", "Stopped: saved position={}s", position_ticks / mbv_core::api::TICKS_PER_SECOND);
                            } else {
                                log::info!(target: "player", "Stopped: position not saved (position_ticks={position_ticks})");
                            }
                        }
                        if preserve_local_state {
                            if let Some(slot) = self.playback_queue().queue.slot(slot_id) {
                                self.last_played_item_id = Some(slot.item.id().to_string());
                                self.last_played_completed = played;
                            }
                        }
                    }
                    None => {
                        log::warn!(target: "player", "Stopped: idx={idx} maps to no live slot; \
                            skipping progress update");
                    }
                }
                self.next_up_item = None;
                self.status.clear();
                if is_delete {
                    // The removal, undo-push, and cursor-clamp already happened
                    // immediately at confirm time (input_confirm_keys.rs), so
                    // the visible list update isn't blocked on this round trip.
                    // All that's left here is telling the player session to
                    // drop the slot from its own internal queue mirror and
                    // mpv's playlist — that still depends on this event, since
                    // nothing told it about the removal until now.
                    self.player.send_command(PlayerCommand::QueueRemove(idx));
                } else {
                    let (should_consume, is_audio) = match slot_id {
                        Some(slot_id) => self.should_consume_slot(slot_id, consume),
                        None => (false, false),
                    };
                    if should_consume {
                        let slot_id = slot_id.expect("should_consume implies a resolved slot");
                        let removed_id = self.consume_slot_from_active_playback_queue(slot_id);
                        self.playback_queue_mut().clamp_cursor();
                        log::info!(target: "consume", "Stopped-path: removed slot_id={slot_id:?} \
                            removed_id={removed_id:?}");
                        if removed_id.is_none() {
                            log::warn!(target: "consume", "Stopped-path: slot_id={slot_id:?} not \
                                found, removal SKIPPED");
                        }
                        if is_audio {
                            self.on_audio_consumed();
                        } else {
                            self.on_video_consumed();
                        }
                    }
                }
                self.playback_queue_mut().queue.clear_active_slot();
                self.refresh_after_stop();
                if !self.has_direct_remote_queue() {
                    self.save_queue_state();
                }
            }
            PlayerEvent::TrackCompleted {
                idx,
                position_ticks,
                played,
                consume,
                progress_report_accepted,
            } => {
                // Resolve the raw mpv index to a slot right away.
                let Some(slot_id) = self.playback_queue().resolve_slot_at(idx) else {
                    log::warn!(target: "consume", "TrackCompleted: idx={idx} maps to no live slot; dropping");
                    return false;
                };
                let position = if played {
                    0
                } else if let Some(slot) = self.playback_queue().queue.slot(slot_id) {
                    // Only record meaningful progress (≥ 30 s) for video;
                    // audio and startup noise keep the prior value.
                    if position_ticks >= 300_000_000 && !slot.item.is_audio() {
                        position_ticks
                    } else {
                        slot.item.playback_position_ticks()
                    }
                } else {
                    return false;
                };
                let queue = self.playback_queue_mut();
                let _ = queue.queue.apply_progress(slot_id, position, played);
                if progress_report_accepted {
                    let _ = queue.queue.mark_progress_sync_pending(slot_id);
                }
                queue.clamp_cursor();
                // Persist Feed lifecycle state before any consume/removal.
                // TrackCompleted with `played` means EOF; for Feed entries,
                // only known-runtime EOF marks played (unknown runtime keeps
                // played=false per spec).
                if let Some(slot) = self.playback_queue().queue.slot(slot_id) {
                    if matches!(slot.item, mbv_core::playback_queue::QueueItem::Feed(_)) {
                        let runtime = slot.item.runtime_ticks();
                        let feed_completed = played && runtime > 0;
                        self.persist_feed_slot_lifecycle(slot_id, position, feed_completed);
                    }
                }
                let (should_consume, is_audio) = self.should_consume_slot(slot_id, consume);
                if should_consume {
                    self.pending_queue_removal = Some((slot_id, is_audio));
                }
            }
            PlayerEvent::TrackChanged(idx) => {
                self.visualizer_failed = false;
                self.next_up_item = None;
                if self.status.starts_with("Next up:") {
                    self.status.clear();
                }
                // Resolve the incoming index to a slot *before* draining any
                // deferred consume: `idx` is the player's report from
                // before it was told (via the QueueRemove sent below) that
                // the completed slot was removed, so it still lines up with
                // the queue's current, pre-removal shape.
                let target_slot_id = self.playback_queue().resolve_slot_at(idx);

                if let Some((slot_id, was_audio)) = self.pending_queue_removal.take() {
                    let len_before = self.playback_queue().total_queue_len();
                    let removed_id = self.consume_slot_from_active_playback_queue(slot_id);
                    let len_after = len_before - removed_id.is_some() as usize;
                    log::info!(target: "consume", "TrackChanged: consuming pending removal slot_id={slot_id:?} \
                        new_idx={idx} len_before={len_before} len_after={len_after} removed_id={removed_id:?}");
                    if removed_id.is_none() {
                        log::warn!(target: "consume", "TrackChanged: slot_id={slot_id:?} not found, \
                            removal SKIPPED");
                    }
                    if was_audio {
                        self.on_audio_consumed();
                    } else {
                        self.on_video_consumed();
                    }
                }

                // Activate the resolved slot by identity (order-independent,
                // unlike raw index arithmetic) and derive the display
                // cursor from its post-removal position — this stays
                // correct regardless of where the just-consumed slot sat
                // relative to `idx`.
                let adjusted = match target_slot_id {
                    Some(slot_id) => {
                        let _ = self.playback_queue_mut().queue.set_active_slot(slot_id);
                        self.playback_queue()
                            .queue
                            .slot_index(slot_id)
                            .unwrap_or(idx)
                    }
                    None => {
                        log::warn!(target: "player", "TrackChanged: idx={idx} maps to no live \
                            slot; skipping activation");
                        idx
                    }
                };
                self.player.status.lock().unwrap().current_idx = adjusted;
                if !self.queue_cursor_held_by_user() {
                    self.playback_queue_mut().queue_cursor = adjusted;
                    // Local mpv advance: a follow-the-playhead move for the
                    // playback-target scope (yields to an active user nav).
                    self.playhead.pending_push =
                        Some(QueueCursorPush::Follow(self.playback_target_queue_scope()));
                }
                if !self.has_direct_remote_queue() {
                    if let Some(item) = self.playback_queue().emby_item_at(adjusted) {
                        self.last_played_item_id = Some(item.id.clone());
                    }
                }
                if !self.has_direct_remote_queue() {
                    let queue = self.playback_queue();
                    log::info!(target: "consume", "TrackChanged: post-save queue len={} ids={:?}",
                        queue.total_queue_len(), queue.slots().iter().map(|s| s.item.id()).collect::<Vec<_>>());
                    self.save_queue_state();
                }
            }
            PlayerEvent::QueueNextUp { next_idx } => {
                if let Some(item) = self.playback_queue().clone_emby_item_at(next_idx) {
                    let item_id = item.id.clone();
                    let show_title = item.series_name.clone();
                    let ep_title = item.name.clone();
                    let artist = item.artist.clone();
                    self.next_up_item = Some(item.clone());
                    // Daemon sends NextUpShow to mpv directly; only send from local player.
                    if !self.player.is_remote() {
                        self.player.send_command(PlayerCommand::NextUpShow {
                            item_id,
                            show_title,
                            ep_title,
                            artist,
                        });
                    }
                }
            }
            PlayerEvent::NextUpThreshold { .. } => {
                // Series episodes now use play_queue; this only fires for movies
                // (always_play_next=false or non-series content). No action needed.
            }
            PlayerEvent::NextUpPlay => {
                log::warn!(target: "app", "next-up: play triggered");
                if let Some(item) = self.next_up_item.take() {
                    let label = item.playback_label();
                    if let Some(idx) = self
                        .playback_queue()
                        .slots()
                        .iter()
                        .position(|s| matches!(&s.item, mbv_core::playback_queue::QueueItem::Emby(e) if e.id == item.id))
                    {
                        self.player.send_command(PlayerCommand::JumpTo(idx));
                        self.playback_queue_mut().queue_cursor = idx;
                        // Auto-advance to the next-up item: a follow-the-playhead
                        // move for the playback-target scope.
                        self.playhead.pending_push = Some(QueueCursorPush::Follow(
                            self.playback_target_queue_scope(),
                        ));
                        self.flash(label, ToastSeverity::Neutral);
                    } else {
                        log::warn!(target: "app", "next-up: item not in queue, cannot jump");
                    }
                } else {
                    log::warn!(target: "app", "next-up: NextUpPlay fired but next_up_item is None");
                }
            }
            PlayerEvent::UnifiedQueueUpdated(unified) => {
                let total = unified.slots.len();

                // Derive the presentation cursor from the active slot index.
                let active_index = unified
                    .active_slot
                    .and_then(|sid| unified.slots.iter().position(|s| s.slot_id == sid));
                let active_cursor = active_index.unwrap_or(0);

                let pending_local_cursor = self.pending_queue_edit_cursor.take();
                // Preserving the user's held selection carries no new
                // cursor intent (the value is just what's already there);
                // every other branch computes a fresh authoritative index.
                let user_holding_local =
                    !self.has_direct_remote_queue() && self.queue_cursor_held_by_user();
                let cursor = if self.has_direct_remote_queue() {
                    self.pending_remote_move_cursor
                        .take()
                        .filter(|pc| *pc < total)
                        .unwrap_or(active_cursor)
                } else if user_holding_local {
                    // User is actively navigating — preserve their
                    // selection cursor, but still replace the queue
                    // contents so slot data stays current.
                    self.playback_queue().queue_cursor
                } else {
                    pending_local_cursor
                        .filter(|pc| *pc < total)
                        .unwrap_or(active_cursor)
                };

                let source = unified.source.clone();
                let queue = self.playback_queue_mut();
                queue.set_unified_state(&unified, cursor);
                self.queue_source = source;
                if !user_holding_local {
                    // Unified/direct-remote reconciliation: a follow-the-playhead
                    // move scoped to the playback target. Consumed only if the
                    // user is currently viewing that scope (a remote daemon
                    // update must not snap a Local-scope view).
                    self.playhead.pending_push =
                        Some(QueueCursorPush::Follow(self.playback_target_queue_scope()));
                }
            }
            PlayerEvent::IntroStarted { intro_end_ticks } => {
                // mbvd never auto-seeks on this event itself — it always
                // reports the boundary neutrally, regardless of daemon-host
                // config, so this client's own `always_skip_intro` is the
                // only thing that decides whether to skip.
                if self.config.lock().unwrap().always_skip_intro {
                    let secs = intro_end_ticks as f64 / mbv_core::api::TICKS_PER_SECOND as f64;
                    self.player.send_command(PlayerCommand::SeekAbsolute(secs));
                    self.player.send_command(PlayerCommand::SkipIntroDismiss);
                }
            }
            PlayerEvent::IntroEnded => {}
            PlayerEvent::SkipIntroPlay => {
                self.status.clear();
            }
            PlayerEvent::MpvQuit => {
                self.next_up_item = None;
                self.status.clear();
                self.refresh_after_stop();
            }
            PlayerEvent::CommandRejected(reason) => {
                self.pending_remote_move_cursor = None;
                self.flash(reason, ToastSeverity::Neutral);
            }
            PlayerEvent::PlaybackIntent(event) => {
                use mbv_core::ctrl::PlaybackIntentOutcome;
                let message = match event.outcome {
                    PlaybackIntentOutcome::Accepted => "Playback request accepted",
                    PlaybackIntentOutcome::Applied => "Playback request applied",
                    PlaybackIntentOutcome::Coalesced { .. } => "Playback request already pending",
                    PlaybackIntentOutcome::Superseded => "Playback request superseded",
                    PlaybackIntentOutcome::Rejected { ref reason } => {
                        use mbv_core::ctrl::PlaybackIntentRejection;
                        match reason {
                            PlaybackIntentRejection::EmptyTarget => "Nothing to play",
                            PlaybackIntentRejection::ResolutionFailed => {
                                "Couldn't load playback items"
                            }
                            PlaybackIntentRejection::AudioOnly => "Can't play audio in video mode",
                            PlaybackIntentRejection::InvalidTarget => "Invalid playback target",
                            PlaybackIntentRejection::Unavailable => "Playback unavailable",
                        }
                    }
                };
                self.flash(message.to_string(), ToastSeverity::Neutral);
            }
            PlayerEvent::PipePlaybackStatus(status) => {
                use mbv_core::ctrl::PipePlaybackPhase;
                let message = match status.phase {
                    PipePlaybackPhase::Resolving => "Resolving pipe playback target".to_string(),
                    PipePlaybackPhase::PlayerOpening => "Opening player output".to_string(),
                    PipePlaybackPhase::OutputStarted => {
                        "Output started; downstream delay is unknown".to_string()
                    }
                    PipePlaybackPhase::OutputBuffering => {
                        let remaining = status.estimated_remaining_ms.unwrap_or_default();
                        format!(
                            "Output started; estimated output buffering (~{} ms remaining)",
                            remaining
                        )
                    }
                };
                // These statuses only originate from a direct pipe-output
                // daemon. Local, attached-Emby, and ordinary daemon routes
                // never receive the event, so their presentation is unchanged.
                self.flash(message, ToastSeverity::Neutral);
            }
            PlayerEvent::PausedChanged(paused) => {
                // Persist Feed position on pause (one write per pause event).
                if paused {
                    if let Some(slot_id) = self.playback_queue().queue.active_slot_id() {
                        if let Some(slot) = self.playback_queue().queue.slot(slot_id) {
                            if let mbv_core::playback_queue::QueueItem::Feed(ref entry) = slot.item
                            {
                                if entry.feed_id.is_some() {
                                    let pos_ticks =
                                        self.player.status.lock().unwrap().position_ticks;
                                    self.persist_feed_slot_lifecycle(slot_id, pos_ticks, false);
                                }
                            }
                        }
                    }
                }
            }
            PlayerEvent::OutputStarted => {
                // If a seek was pending for a Feed slot, persist the
                // resulting position now (confirmed seek completion).
                if let Some(slot_id) = self.feed_seek_pending_slot.take() {
                    if let Some(slot) = self.playback_queue().queue.slot(slot_id) {
                        if let mbv_core::playback_queue::QueueItem::Feed(ref entry) = slot.item {
                            if entry.feed_id.is_some() {
                                let pos_ticks = self.player.status.lock().unwrap().position_ticks;
                                self.persist_feed_slot_lifecycle(slot_id, pos_ticks, false);
                            }
                        }
                    }
                }
            }
            PlayerEvent::RemoteDisconnected(reason) => {
                if self.try_reattach_remote_daemon() {
                    return true;
                }
                self.restore_local_mode(&reason);
                self.refresh_after_stop();
                return true;
            }
            PlayerEvent::EmbyAuthorityTaken(reason) => {
                // Authority-change notification: Emby remote has taken authority.
                // The connection stays open — do NOT call restore_local_mode().
                // Just flash the status so the user knows commands are temporarily rejected.
                self.flash(reason, ToastSeverity::Warning);
            }
            PlayerEvent::QueueDesynced(reason) => {
                self.flash(reason, ToastSeverity::Neutral);
            }
            // The announced-shutdown counterpart to the unannounced-loss
            // modal raised from PlayerEvent::Stopped above (task 7.2): a
            // local-daemon client prints one line and exits cleanly; a
            // client of a genuinely remote daemon keeps today's behavior.
            PlayerEvent::DaemonShutdownAnnounced => {
                if self.is_local_daemon() {
                    self.pending_exit_message =
                        Some("mbv: the local daemon was stopped — exiting.".to_string());
                    QUIT_REQUESTED.store(true, Ordering::Relaxed);
                } else {
                    self.restore_local_mode("Daemon disconnected — returned to local mode");
                    self.refresh_after_stop();
                }
            }
            PlayerEvent::AudiobookshelfProgress(ev) => {
                // No client-side generation gate: the daemon already drops
                // stale-generation updates before emitting, and the daemon's
                // generation counter is unrelated to this client's own runtime
                // generation, so comparing them would reject every live event.
                let current_time_seconds =
                    ev.position_ticks as f64 / mbv_core::api::TICKS_PER_SECOND as f64;
                self.reconcile_audiobookshelf_progress(
                    &ev.library_item_id,
                    &ev.episode_id,
                    ev.position_ticks,
                    current_time_seconds,
                    ev.is_finished,
                );
            }
            PlayerEvent::AudiobookshelfBookProgress(ev) => {
                self.reconcile_audiobookshelf_book_progress(
                    &ev.library_item_id,
                    ev.position_ticks,
                    ev.is_finished,
                );
            }
        }
        false
    }

    /// Raises the blocking daemon-lost modal (task 7.1), replacing whatever
    /// other blocking overlay was showing -- only one is ever active.
    fn raise_daemon_lost_modal(&mut self) {
        // Closing the context menu is re-homed: the DaemonLost `OverlayRequest`
        // arm calls `dismiss_blocking_modals`, which now also unmounts the
        // ContextMenu component (task 5.3c). `pending_overlay` is a single slot,
        // so it cannot both dismiss the menu and raise DaemonLost here.
        let last_playing_title = {
            let idx = self.player.status.lock().unwrap().current_idx;
            self.playback_queue()
                .item_at(idx)
                .map(|item| item.title().to_string())
        };
        self.pending_overlay = Some(super::types_overlay::OverlayRequest::DaemonLost(
            DaemonLostModal {
                last_playing_title,
                daemon_log_path: crate::state_dir()
                    .join("local-daemon.log")
                    .display()
                    .to_string(),
                restart_error: None,
            },
        ));
    }
}
