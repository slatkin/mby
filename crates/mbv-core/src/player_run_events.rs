fn is_clocked_audio_error(error: &libmpv2::Error, audio_device_configured: bool) -> bool {
    audio_device_configured
        && matches!(
            error,
            libmpv2::Error::Raw(code) if *code == libmpv2::mpv_error::AoInitFailed
        )
}

impl PlaybackRun {
    fn on_time_pos(&mut self, pos_secs: f64, mpv: &Mpv) {
        let ticks = (pos_secs * TICKS_PER_SECOND as f64) as i64;
        {
            let mut st = self.status.lock().unwrap();
            st.position_ticks = ticks;
            if pos_secs > 0.0 {
                if self.last_valid_pos == 0 {
                    log::info!(target: "player", "playlist last_valid_pos first non-zero: {}s idx={}", ticks / TICKS_PER_SECOND, self.current_idx);
                }
                self.last_valid_pos = ticks;
                st.last_valid_pos = ticks;
            }
        }

        if self.origin == PlaybackOrigin::Queue {
            // Playlist next-up: match Emby Web's timing from videoosd.js.
            // 60 s before end. Minimum episode: 10 min. Minimum remaining when shown: 20 s.
            const MIN_RUNTIME_TICKS: i64 = 600 * TICKS_PER_SECOND;
            const MIN_REMAIN_TICKS: i64 = 20 * TICKS_PER_SECOND;
            if self.current_idx + 1 < self.queue_len()
                && self.active_item().is_some_and(QueueItem::is_tv_episode)
                && self
                    .item_at(self.current_idx + 1)
                    .is_some_and(QueueItem::is_tv_episode)
            {
                let runtime = self.status.lock().unwrap().runtime_ticks;
                if runtime > 0 {
                    let show_at = runtime - 60 * TICKS_PER_SECOND;
                    let remaining = runtime - ticks;
                    if self.queue_next_up.is_fired() && ticks < show_at {
                        self.queue_next_up.reset();
                    }
                    if !self.queue_next_up.is_fired() && runtime >= MIN_RUNTIME_TICKS {
                        if remaining >= MIN_REMAIN_TICKS && ticks >= show_at {
                            self.queue_next_up.fire();
                            let _ = self.event_tx.send(PlayerEvent::QueueNextUp {
                                next_idx: self.current_idx + 1,
                            });
                        } else if self.queue_next_up == NextUp::Idle
                            && ticks > 0
                            && ticks < TICKS_PER_SECOND * 5
                        {
                            self.queue_next_up.arm();
                            log::info!(target: "player", "queue next-up armed idx={}", self.current_idx + 1);
                        }
                    }
                }
            }
        } else if !self.next_up.is_fired() {
            const NEXT_UP_TICKS: i64 = 60 * TICKS_PER_SECOND;
            if self.series_id.as_str().is_empty() {
                if self.next_up == NextUp::Idle && ticks > 0 && ticks < TICKS_PER_SECOND * 5 {
                    self.next_up.arm();
                    log::warn!(target: "player", "next-up disabled: no series_id (Episode item without SeriesId in fetch)");
                }
            } else {
                let runtime = self.status.lock().unwrap().runtime_ticks;
                if runtime > NEXT_UP_TICKS && ticks > runtime - NEXT_UP_TICKS {
                    self.next_up.fire();
                    log::warn!(target: "player", "next-up: threshold reached series={}", self.series_id);
                    let _ = self.event_tx.send(PlayerEvent::NextUpThreshold {
                        series_id: self.series_id.clone(),
                        season: self.season,
                        episode: self.episode,
                    });
                } else if self.next_up == NextUp::Idle && ticks > 0 && ticks < TICKS_PER_SECOND * 5
                {
                    self.next_up.arm();
                    log::info!(target: "player", "next-up: armed series={} runtime={}s", self.series_id, runtime / TICKS_PER_SECOND);
                }
            }
        }

        handle_intro(
            ticks,
            self.intro_start,
            self.intro_end,
            &mut self.intro_state,
            self.config.always_skip_intro,
            mpv,
            &self.event_tx,
        );
    }

    fn on_playlist_pos_changed(&mut self, pos: i64) {
        if self.active_file {
            return;
        }
        if pos < 0 {
            return;
        }
        let pos = pos as usize;
        if self.pending_initial_playlist_layout
            || !self.load_state.is_ready()
            || self.forced_slot_id.is_some()
        {
            log::debug!(
                target: "player",
                "ignoring transient playlist-pos={pos} while queue transition is pending"
            );
            return;
        }
        if pos >= self.queue_len() {
            log::warn!(
                target: "player",
                "ignoring out-of-range playlist-pos={pos} for queue len {}",
                self.queue_len()
            );
            return;
        }
        let _ = self.set_active_index(pos);
    }

    fn on_playlist_count_changed(&mut self, count: usize) {
        if self.active_file {
            return;
        }
        if count == self.queue_len() {
            return;
        }
        let old_n = self.queue_len();
        if count < old_n {
            let removed = old_n - count;
            log::warn!(target: "player", "playlist-count dropped from {} to {}: {} item(s) removed externally", old_n, count, removed);
            let removed_slot_ids: Vec<_> = self
                .queue
                .slots()
                .iter()
                .skip(count)
                .map(|slot| slot.slot_id)
                .collect();
            for slot_id in removed_slot_ids {
                if self.active_slot_id() == Some(slot_id) {
                    let _ = self.queue.remove_active_slot_confirmed(slot_id);
                } else {
                    let _ = self.queue.remove_slot(slot_id);
                }
            }
            self.refresh_current_idx_from_queue();
            let _ = self.event_tx.send(PlayerEvent::QueueDesynced(format!(
                "Queue desynced: {removed} item(s) removed externally"
            )));
        } else {
            let added = count - old_n;
            log::warn!(target: "player", "playlist-count increased from {} to {}: {} item(s) added externally", old_n, count, added);
            // We cannot reconstruct the added EmbyItems from mpv's playlist,
            // so we keep the queue as-is. Clamp current_idx to the last
            // known item in case the external tool also changed position.
            if self.current_idx >= self.queue_len() && self.queue_len() > 0 {
                self.current_idx = self.queue_len() - 1;
            }
            self.sync_status_position();
            let _ = self.event_tx.send(PlayerEvent::QueueDesynced(format!(
                "Queue desynced: {added} item(s) added externally"
            )));
        }
    }

    fn on_playback_restart(&mut self, mpv: &Mpv) {
        let was_seek = self.last_seek_at.is_some();
        self.active_file_starting = false;
        // `PlaybackRestart` is the concrete mpv-owned event used by mbvd as
        // its output-started boundary. It says nothing about downstream pipe
        // buffers or actual audibility.
        let _ = self.event_tx.send(PlayerEvent::OutputStarted);
        self.pending_initial_playlist_layout = false;
        {
            let h: i64 = mpv.get_property("video-params/h").unwrap_or(0);
            let is_img: bool = mpv
                .get_property("current-tracks/video/image")
                .unwrap_or(false);
            let codec: String = mpv.get_property("audio-codec-name").unwrap_or_default();
            let mut st = self.status.lock().unwrap();
            st.video_height = h;
            st.audio_codec = codec.to_lowercase();
            st.video_is_image = is_img;
        }
        if self.startup_pause.is_holding() {
            self.startup_pause.clear();
            log::info!(
                target: "player",
                "audio pipe: startup gate cleared on PlaybackRestart (playlist)"
            );
            let _ = mpv.set_property("pause", false);
        }
        let mut event_name = "TimeUpdate";
        if !self.tracks_initialized {
            let prefs = self.subtitle_prefs.lock().unwrap().clone();
            for url in &self.ext_sub_urls {
                if let Err(e) = mpv.command("sub-add", &[url.as_str()]) {
                    log::warn!(target: "player", "sub-add failed: {url}: {e:?}");
                }
            }
            auto_select_tracks(mpv, &self.status, &prefs);
            self.tracks_initialized = true;
            if let Some(item) = self.active_item().cloned() {
                if let Some(emby) = item.as_emby() {
                    send_ep_info(mpv, emby);
                }
            }
            if self.config.use_mpv_config {
                let _ = mpv.command("show-text", &[&self.osd_title, "3000"]);
            }
        } else {
            if self.origin == PlaybackOrigin::Standalone {
                self.next_up.reset();
                event_name = "Seek";
            }
            if self.last_seek_at.take().is_some() && self.config.use_mpv_config {
                let _ = mpv.command("show-text", &[&self.osd_title, "2000"]);
            }
        }
        let seek_settled = self
            .last_seek_at
            .is_none_or(|t| t.elapsed() > Duration::from_millis(500));
        if self.quit_at.is_none() && seek_settled {
            self.last_seek_at = None;
            if self.origin == PlaybackOrigin::Standalone {
                self.reporter.report_progress(event_name);
            } else if !self.reporter.is_audio.load(Ordering::Relaxed) {
                self.reporter.report_progress("TimeUpdate");
            }
        }
        if was_seek {
            self.observe_reporting(true);
        }
    }

    // libmpv2 returns MPV_EVENT_END_FILE failures as Err(Error::Raw(...)),
    // so classify the output-specific error before the generic event logging.
    fn on_mpv_error(&mut self, error: libmpv2::Error, progress: &mut ProgressGuard) -> bool {
        if !is_clocked_audio_error(&error, self.config.audio_device.is_some()) {
            log::warn!(target: "player", "event error: {}", mpv_err_str(&error));
            return false;
        }

        let device = self.config.audio_device.clone().unwrap_or_default();
        self.close_prepared_source();
        progress.stop_and_join(self.progress_join_budget());
        self.close_prepared_source_at(self.last_valid_pos);
        self.status.lock().unwrap().active = false;
        let _ = self.event_tx.send(PlayerEvent::Stopped {
            idx: self.current_idx,
            position_ticks: 0,
            played: false,
            consume: false,
            progress_report_accepted: false,
            error: Some(format!("audio output failed to start (device: {device})")),
        });
        true
    }

    // Returns true if the event loop should `continue`.
    fn on_end_file(
        &mut self,
        reason: EndFileReason,
        mpv: &Mpv,
        progress: &mut ProgressGuard,
    ) -> bool {
        if self.quit_at.is_some() {
            return true;
        }
        if !self.load_state.is_ready() {
            match self.load_state.drain() {
                Drained::HitZero => {
                    // Once all pending EndFiles from a ReplaceQueue are drained, the new item's
                    // lifecycle begins — reset stop_report so on_end_file/on_shutdown can report it.
                    self.stop_report.reset();
                }
                Drained::StillPending | Drained::AlreadyReady => {}
            }
            return true;
        }
        if self.active_file && self.active_file_starting && reason == mpv_end_file_reason::Error {
            self.active_file_starting = false;
            self.close_prepared_source();
            progress.stop_and_join(self.progress_join_budget());
            self.close_prepared_source_at(self.last_valid_pos);
            self.status.lock().unwrap().active = false;
            let _ = self.event_tx.send(PlayerEvent::Stopped {
                idx: self.current_idx,
                position_ticks: 0,
                played: false,
                consume: false,
                progress_report_accepted: false,
                error: Some("failed to start media".into()),
            });
            return false;
        }
        if reason == mpv_end_file_reason::Error {
            log::warn!(target: "player", "EndFile: playback error (file may be unreadable or format unsupported)");
        }

        let completed_is_audio = self.reporter.is_audio.load(Ordering::Relaxed);
        let runtime = self.status.lock().unwrap().runtime_ticks;

        if self.origin == PlaybackOrigin::Queue && reason == mpv_end_file_reason::Quit {
            let completed_runtime = self.active_item().map_or(0, |item| item.runtime_ticks());
            let near_end = is_near_end(
                completed_is_audio,
                false,
                self.last_valid_pos,
                completed_runtime,
            );
            log::warn!(target: "player", "quit path: last_valid_pos={} runtime={} stop_report={:?}",
                self.last_valid_pos, completed_runtime, self.stop_report);
            if self.stop_report == StopReport::NotSent {
                // mpv-initiated quits (for example a compositor close request)
                // must not wait on Emby before mpv can finish its own shutdown.
                self.report_stop_now_or_background(progress);
            }
            if near_end && !completed_is_audio && self.reporter.has_session() {
                let id = self.reporter.ids.lock().unwrap().0.clone();
                if let Err(e) = self.reporter.client.mark_played(id.as_str()) {
                    log::warn!(target: "player", "mark_played failed id={id}: {e}; scheduling retry");
                    retry_mark_played(self.reporter.client.clone(), id);
                }
            }
            self.stopped_near_end = near_end;
            return true; // wait for Shutdown to fire PlayerEvent::Stopped
        }

        if self.origin == PlaybackOrigin::Standalone {
            let natural_end = reason == mpv_end_file_reason::Eof && runtime > 0;

            if reason == mpv_end_file_reason::Quit {
                // Keep an external window close off the mpv event loop. Natural
                // EOF still reports synchronously before its completion event.
                self.report_stop_now_or_background(progress);
            } else {
                progress.stop_and_join(self.progress_join_budget());
                self.stop_report = StopReport::mark_sent(self.report_stopped_for_end_file(reason));
            }

            let lifecycle_pos = self.active_item().map_or(self.last_valid_pos, |item| {
                provider_lifecycle_close_pos(item, natural_end, runtime, self.last_valid_pos)
            });
            self.close_prepared_source_at(lifecycle_pos);

            if natural_end && self.reporter.has_session() {
                let id = self.reporter.ids.lock().unwrap().0.clone();
                if !completed_is_audio {
                    match self.reporter.client.mark_played(id.as_str()) {
                        Ok(()) => log::info!(target: "player", "mark_played ok id={id}"),
                        Err(e) => {
                            log::warn!(target: "player", "mark_played failed id={id}: {e}; will retry");
                            self.mark_played_id = Some(id.clone());
                        }
                    }
                }
            }
            if !self.stopped_event_sent {
                let _ = self.event_tx.send(PlayerEvent::Stopped {
                    idx: 0,
                    position_ticks: 0,
                    played: natural_end && !completed_is_audio && self.reporter.has_session(),
                    consume: false,
                    progress_report_accepted: self.stop_report.is_accepted(),
                    error: None,
                });
                self.stopped_event_sent = true;
            }
            return false;
        }

        // QueueSlotId is authoritative inside the Player owner. PlayerEvent
        // indices remain local UI snapshots: carrying slot identity farther
        // would change the serializable event/ctrl boundary, while the owner
        // can resolve the completed occurrence before emitting that snapshot.
        let completed_slot_id = self.active_slot_id();
        let completed_idx = completed_slot_id
            .and_then(|slot_id| self.queue.slot_index(slot_id))
            .unwrap_or(self.current_idx);
        log::warn!(target: "player", "advance path: reason={reason:?} last_valid_pos={} runtime={}",
            self.last_valid_pos, self.status.lock().unwrap().runtime_ticks);
        // H11: bounds-check completed_idx — QueueRemove can shrink the list
        // while the current track is finishing.
        let Some(completed_item) = completed_slot_id
            .and_then(|slot_id| self.queue.slot(slot_id))
            .map(|slot| slot.item.clone())
        else {
            log::warn!(target: "player", "on_end_file: completed_idx={completed_idx} out of bounds (len={}), stopping",
                self.queue_len());
            progress.stop_and_join(self.progress_join_budget());
            self.status.lock().unwrap().active = false;
            self.stop_report =
                StopReport::mark_sent(self.reporter.report_stopped(self.last_valid_pos));
            let _ = self.event_tx.send(PlayerEvent::Stopped {
                idx: completed_idx.min(self.queue_len().saturating_sub(1)),
                position_ticks: self.last_valid_pos,
                played: false,
                consume: false,
                progress_report_accepted: self.stop_report.is_accepted(),
                error: None,
            });
            return false;
        };
        let completed_runtime = completed_item.runtime_ticks();
        let natural = reason == mpv_end_file_reason::Eof && completed_runtime > 0;
        let near_end = is_near_end(
            completed_is_audio,
            natural,
            self.last_valid_pos,
            completed_runtime,
        );
        let was_next_up = std::mem::replace(&mut self.next_up_jump, false);
        let track_finished = natural || near_end || was_next_up;
        // played_out drives mark-played/Emby watched-status and stays video-only;
        // consume_track drives queue auto-removal and is type-agnostic — the app layer
        // gates it per-type against consume_videos/consume_audio.
        let played_out = track_finished && !completed_is_audio;
        let consume_track = track_finished;
        log::info!(target: "consume", "on_end_file decision: idx={completed_idx} reason={reason:?} \
            natural={natural} near_end={near_end} was_next_up={was_next_up} \
            completed_is_audio={completed_is_audio} last_valid_pos={} runtime={} \
            => played_out={played_out} consume_track={consume_track}",
            self.last_valid_pos, completed_runtime);
        let completed_pos =
            queue_completed_pos(completed_is_audio, natural, near_end, self.last_valid_pos);

        let next_idx = self
            .forced_slot_id
            .take()
            .and_then(|slot_id| self.queue.slot_index(slot_id))
            .unwrap_or(self.current_idx + 1);

        if next_idx >= self.queue_len() {
            progress.stop_and_join(self.progress_join_budget());
            self.status.lock().unwrap().active = false;
            self.stop_report = StopReport::mark_sent(self.reporter.report_stopped(completed_pos));
            self.close_prepared_source_at(provider_lifecycle_close_pos(
                &completed_item,
                natural,
                completed_runtime,
                self.last_valid_pos,
            ));
            if played_out {
                if let Some(emby) = completed_item.as_emby() {
                    let id = emby.id.clone();
                    if let Err(e) = self.reporter.client.mark_played(&id) {
                        log::warn!(target: "player", "mark_played failed id={id}: {e}; scheduling retry");
                        retry_mark_played(self.reporter.client.clone(), ItemId::new(id));
                    }
                }
            }
            let _ = self.event_tx.send(PlayerEvent::Stopped {
                idx: completed_idx,
                position_ticks: completed_pos,
                played: played_out,
                consume: consume_track,
                progress_report_accepted: self.stop_report.is_accepted(),
                error: None,
            });
            return false; // signals run() to return
        }

        // Update UI to the next track immediately, before slow network calls.
        // next_idx < queue_len() was already checked above, so set_active_index
        // (which only fails when the index is out of bounds) cannot fail here.
        let next_slot_id = self.slot_id_at(next_idx);
        let advanced = if self.active_file {
            self.close_prepared_source_at(provider_lifecycle_close_pos(
                &completed_item,
                natural,
                completed_runtime,
                self.last_valid_pos,
            ));
            next_slot_id.is_some_and(|slot_id| self.select_active_slot(slot_id, mpv).is_ok())
        } else {
            self.set_active_index(next_idx)
        };
        debug_assert!(
            advanced,
            "set_active_index({next_idx}) must succeed: already bounds-checked against queue_len={}",
            self.queue_len()
        );
        let next_item = self
            .active_item()
            .cloned()
            .expect("active item must exist after successful set_active_index");
        self.load_active_item_state();
        if self.active_file && !advanced {
            let error = AudiobookshelfError::from_class(AudiobookshelfFailureClass::Unavailable);
            progress.stop_and_join(self.progress_join_budget());
            self.status.lock().unwrap().active = false;
            let _ = self.event_tx.send(PlayerEvent::Stopped {
                idx: self.current_idx,
                position_ticks: 0,
                played: false,
                consume: false,
                progress_report_accepted: false,
                error: Some(format!("failed to prepare media: {error}")),
            });
            return false;
        }
        self.tracks_initialized = false;
        {
            let mut s = self.status.lock().unwrap();
            s.position_ticks = 0;
            s.runtime_ticks = next_item.runtime_ticks();
            s.current_idx = self.current_idx;
            s.queue_len = self.queue_len();
            if let Some(emby) = next_item.as_emby() {
                s.set_current_item_metadata(emby);
            } else {
                s.title = next_item.title().to_string();
                s.art_item_id = next_item.id().to_string();
            }
        }

        let stop_report_accepted = self.reporter.report_stopped(completed_pos);
        if played_out {
            if let Some(emby) = completed_item.as_emby() {
                let id = emby.id.clone();
                if let Err(e) = self.reporter.client.mark_played(&id) {
                    log::warn!(target: "player", "mark_played failed id={id}: {e}; scheduling retry");
                    retry_mark_played(self.reporter.client.clone(), ItemId::new(id));
                }
            }
        }

        let _ = mpv.set_property("start", "0");
        self.queue_next_up.reset();
        if let Some(emby) = next_item.as_emby() {
            send_ep_info(mpv, emby);
        }
        let _ = mpv.command("script-message", &["mbv-skip-intro-dismiss"]);

        // Stop progress reporter during transition to prevent stale reports.
        progress.stop_and_join(self.progress_join_budget());
        if let Some(emby) = next_item.as_emby() {
            let (urls, ok) = self.reporter.start_item(emby);
            self.ext_sub_urls = urls;
            if !ok {
                log::warn!(target: "player", "start_item failed for playlist track-transition item={}", emby.id);
            }
        } else {
            self.ext_sub_urls = vec![];
            // Feed item: clear Emby session IDs so the progress reporter
            // (if any) becomes a no-op.  The old item's report_stopped has
            // already been sent above with the original IDs.
            self.reporter.clear_session();
        }
        *progress = spawn_progress_reporter(self.reporter.clone());

        log::info!(target: "player", "playlist track-transition idx={}", self.current_idx);

        let _ = self.event_tx.send(PlayerEvent::TrackCompleted {
            idx: completed_idx,
            position_ticks: completed_pos,
            played: played_out,
            consume: consume_track,
            progress_report_accepted: stop_report_accepted,
        });
        let _ = self
            .event_tx
            .send(PlayerEvent::TrackChanged(self.current_idx));
        false
    }

    fn on_shutdown(&mut self, progress: &mut ProgressGuard) {
        self.close_prepared_source();
        log::warn!(target: "player", "shutdown: last_valid_pos={} stop_report={:?}",
            self.last_valid_pos, self.stop_report);
        if self.stop_report == StopReport::NotSent {
            self.report_stop_now_or_background(progress);
        }
        let client = self.reporter.client.clone();
        if self.origin == PlaybackOrigin::Standalone {
            // Retry mark_played in a detached thread so Shutdown never blocks.
            if let Some(mid) = self.mark_played_id.take() {
                retry_mark_played(client.clone(), mid);
            }
            let completed_runtime = self.active_item().map_or(0, |item| item.runtime_ticks());
            let is_audio = self.reporter.is_audio.load(Ordering::Relaxed);
            let near_end = self.reporter.has_session()
                && is_near_end(is_audio, false, self.last_valid_pos, completed_runtime);
            if near_end {
                let id = self.reporter.ids.lock().unwrap().0.clone();
                retry_mark_played(client.clone(), id);
            }
            self.status.lock().unwrap().active = false;
            if !self.stopped_event_sent {
                let _ = self.event_tx.send(PlayerEvent::Stopped {
                    idx: 0,
                    position_ticks: self.last_valid_pos,
                    played: near_end,
                    consume: false,
                    progress_report_accepted: self.stop_report.is_accepted(),
                    error: None,
                });
            }
            // mpv exited on its own (not via our stop command, e.g. the user
            // closed the mpv window directly) — despite the event name,
            // App::handle_player_event's PlayerEvent::MpvQuit arm does not
            // quit the app; it just clears some UI state and returns false.
            if self.quit_at.is_none() {
                let _ = self.event_tx.send(PlayerEvent::MpvQuit);
            }
            return;
        }
        self.status.lock().unwrap().active = false;
        // played and consume are deliberately the same value here: stopped_near_end
        // is already video-only (see is_near_end's !is_audio gate), so a quit/cancel
        // near the end of an audio item never sets either — consistent with on_end_file's
        // normal advance path, where only natural/next-up (not near-end) triggers audio consume.
        let _ = self.event_tx.send(PlayerEvent::Stopped {
            idx: self.current_idx,
            position_ticks: self.last_valid_pos,
            played: self.stopped_near_end,
            consume: self.stopped_near_end,
            progress_report_accepted: self.stop_report.is_accepted(),
            error: None,
        });
        // mpv exited on its own (not via our stop command) — tell the app to quit.
        if self.quit_at.is_none() {
            let _ = self.event_tx.send(PlayerEvent::MpvQuit);
        }
    }
}

fn provider_lifecycle_close_pos(
    item: &QueueItem,
    natural_end: bool,
    runtime: i64,
    last_valid_pos: i64,
) -> i64 {
    if item.is_audiobookshelf_any() && natural_end {
        runtime.max(0)
    } else {
        last_valid_pos
    }
}
