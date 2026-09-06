// Snapshotted inputs for a stopped-report job: the values report_stopped
// captures today before handing off, so the worker never reads ids/status
// under a lock itself. `is_audio` and `last_valid_pos` are only needed for
// the log line (`pos` is already the zeroed-for-audio value to send).
struct StoppedReportData {
    client: Arc<EmbyClient>,
    ws_tx: Option<crate::ws::WsSender>,
    id: ItemId,
    msid: MediaSourceId,
    sid: EmbySessionId,
    is_audio: bool,
    last_valid_pos: i64,
    pos: i64,
    runtime_ticks: i64,
}

// How a start-report job gets its media_source_id/session_id: either the
// caller already resolved them synchronously (transition_to), or the fetch
// is deferred to the worker (transition_to_deferred, for pipe output where
// even the synchronous get_playback_info call would delay loadfile).
enum StartIds {
    Resolved {
        media_source_id: MediaSourceId,
        session_id: EmbySessionId,
    },
    Deferred {
        ids: Arc<Mutex<(ItemId, MediaSourceId, EmbySessionId)>>,
        is_audio: Arc<AtomicBool>,
    },
}

// The three jobs SessionReporter used to run on detached per-call threads,
// now executed FIFO on one worker thread fed by `SessionReporter::job_tx`.
enum ReportJob {
    Stopped(StoppedReportData),
    Start {
        client: Arc<EmbyClient>,
        item: EmbyItem,
        ids: StartIds,
    },
    ProgressJoinThenStopped {
        handle: Option<thread::JoinHandle<()>>,
        budget: Duration,
        stopped: Option<StoppedReportData>,
    },
}

fn execute_stopped_report(data: StoppedReportData) {
    let StoppedReportData {
        client,
        ws_tx,
        id,
        msid,
        sid,
        is_audio,
        last_valid_pos,
        pos,
        runtime_ticks,
    } = data;
    if let Some(ref tx) = ws_tx {
        if tx.is_connected() {
            let _ = tx.flush(Duration::from_secs(1));
        }
    }
    log::info!(target: "player", "report_stopped: item={id} is_audio={is_audio} last_valid_pos={}s sending pos={}s",
        last_valid_pos / TICKS_PER_SECOND, pos / TICKS_PER_SECOND);
    let ok = client.report_stopped(&id, &msid, pos, &sid, runtime_ticks);
    if !ok {
        log::warn!(target: "player", "transition_to: report_stopped failed for prev item");
    }
}

fn run_report_worker(rx: mpsc::Receiver<ReportJob>) {
    for job in rx {
        match job {
            ReportJob::Stopped(data) => execute_stopped_report(data),
            ReportJob::Start { client, item, ids } => {
                let (media_source_id, session_id) = match ids {
                    StartIds::Resolved {
                        media_source_id,
                        session_id,
                    } => (media_source_id, session_id),
                    StartIds::Deferred { ids, is_audio } => {
                        let info = client.get_playback_info(&item.id);
                        {
                            let mut locked = ids.lock().unwrap_or_else(|e| e.into_inner());
                            locked.0 = ItemId::new(item.id.clone());
                            locked.1 = info.media_source_id.clone();
                            locked.2 = info.session_id.clone();
                        }
                        is_audio.store(item.is_audio(), Ordering::Relaxed);
                        (info.media_source_id, info.session_id)
                    }
                };
                let ok = client.report_start(&item, &media_source_id, &session_id);
                if !ok {
                    log::warn!(target: "player", "transition_to: report_start failed for item={}", item.id);
                }
            }
            ReportJob::ProgressJoinThenStopped {
                handle,
                budget,
                stopped,
            } => {
                if let Some(h) = handle {
                    let _ = crate::bounded::run_with_hard_bound(
                        move || {
                            let _ = h.join();
                            Ok::<(), String>(())
                        },
                        budget,
                    );
                }
                if let Some(data) = stopped {
                    execute_stopped_report(data);
                }
            }
        }
    }
}

// Shared between the event loop thread and the progress reporter thread.
// All mutable fields are Arc-wrapped so transitions are visible to both.
#[derive(Clone)]
struct SessionReporter {
    client: Arc<EmbyClient>,
    ws_tx: Option<crate::ws::WsSender>,
    // (item_id, msid, sid) in a single lock so progress and event-loop threads never
    // observe a torn triple during item transitions.
    ids: Arc<Mutex<(ItemId, MediaSourceId, EmbySessionId)>>,
    // Shared with progress thread so it knows whether to send progress or just ping.
    is_audio: Arc<AtomicBool>,
    status: Arc<Mutex<PlayerStatus>>,
    // FIFO worker: stopped/start/progress-join jobs are sent here instead of
    // spawning a detached thread per call, so an outgoing stopped-report and
    // an incoming start-report can never race out of order (#bound-daemon-
    // playback-memory). The worker thread ends when every clone's sender
    // drops.
    job_tx: mpsc::Sender<ReportJob>,
}

impl SessionReporter {
    fn new(
        client: Arc<EmbyClient>,
        ws_tx: Option<crate::ws::WsSender>,
        item_id: ItemId,
        msid: MediaSourceId,
        sid: EmbySessionId,
        is_audio: bool,
        status: Arc<Mutex<PlayerStatus>>,
    ) -> Self {
        let (job_tx, job_rx) = mpsc::channel::<ReportJob>();
        thread::spawn(move || run_report_worker(job_rx));
        SessionReporter {
            client,
            ws_tx,
            ids: Arc::new(Mutex::new((item_id, msid, sid))),
            is_audio: Arc::new(AtomicBool::new(is_audio)),
            status,
            job_tx,
        }
    }

    // Snapshots the values report_stopped_background/the progress-join job
    // need, so the worker reads nothing from a shared lock. `None` when the
    // reporter has no session (feed-only playback), matching has_session's
    // no-op contract for callers.
    fn stopped_report_data(&self, last_valid_pos: i64) -> Option<StoppedReportData> {
        if !self.has_session() {
            return None;
        }
        let (id, msid, sid) = self.ids.lock().unwrap_or_else(|e| e.into_inner()).clone();
        let is_audio = self.is_audio.load(Ordering::Relaxed);
        let pos = if is_audio { 0 } else { last_valid_pos };
        let runtime_ticks = self
            .status
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .runtime_ticks;
        Some(StoppedReportData {
            client: self.client.clone(),
            ws_tx: self.ws_tx.clone(),
            id,
            msid,
            sid,
            is_audio,
            last_valid_pos,
            pos,
            runtime_ticks,
        })
    }

    /// Returns `true` when the reporter holds a real Emby session (non-empty
    /// item ID). Guards against noisy failed HTTP calls during feed-only
    /// playback where no Emby session was ever established.
    fn has_session(&self) -> bool {
        !self
            .ids
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .0
            .as_str()
            .is_empty()
    }

    /// Clear all session IDs so subsequent report calls are safe no-ops.
    /// Called when transitioning from Emby playback to a feed entry to
    /// prevent stale session state from leaking into the feed lifecycle.
    fn clear_session(&self) {
        let mut ids = self.ids.lock().unwrap_or_else(|e| e.into_inner());
        ids.0 = ItemId::empty();
        ids.1 = MediaSourceId::new("");
        ids.2 = EmbySessionId::new("");
    }

    // Sends progress via websocket when connected, otherwise falls back to HTTP.
    // Recovers from poisoned mutexes so the progress thread never panics while
    // holding a lock.  No-op when the reporter has no session (feed-only
    // playback) so callers never need to guard.
    fn report_progress(&self, event_name: &str) {
        if !self.has_session() {
            return;
        }
        let (id, msid, sid) = self.ids.lock().unwrap_or_else(|e| e.into_inner()).clone();
        let (pos, runtime, paused) = {
            let s = self.status.lock().unwrap_or_else(|e| e.into_inner());
            (s.position_ticks, s.runtime_ticks, s.paused)
        };
        if let Some(ref tx) = self.ws_tx {
            if tx.is_connected() {
                self.client
                    .report_progress_ws(&id, &msid, pos, runtime, paused, &sid, event_name, tx);
                return;
            }
        }
        self.client
            .report_progress_http(&id, &msid, pos, paused, &sid, event_name);
    }

    // Zeroes position for audio items so Emby doesn't resume audio from mid-track.
    // Returns false (no-op) when the reporter has no session so callers get the
    // correct StopReport state without needing per-site guards.
    fn report_stopped(&self, last_valid_pos: i64) -> bool {
        if !self.has_session() {
            return false;
        }
        let (id, msid, sid) = self.ids.lock().unwrap_or_else(|e| e.into_inner()).clone();
        let is_audio = self.is_audio.load(Ordering::Relaxed);
        let pos = if is_audio { 0 } else { last_valid_pos };
        let runtime_ticks = self
            .status
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .runtime_ticks;
        log::info!(target: "player", "report_stopped: item={id} is_audio={is_audio} last_valid_pos={}s sending pos={}s",
            last_valid_pos / TICKS_PER_SECOND, pos / TICKS_PER_SECOND);
        self.client
            .report_stopped(&id, &msid, pos, &sid, runtime_ticks)
    }

    // Fire-and-forget variant of report_stopped for the item being left behind
    // during a transition, so the player thread can issue loadfile for the new
    // item immediately instead of waiting on this HTTP call (and the WS flush,
    // which report_stopped's synchronous callers don't do — it's bookkeeping
    // that doesn't affect playback).  No-op when the reporter has no session.
    fn report_stopped_background(&self, last_valid_pos: i64) {
        if let Some(data) = self.stopped_report_data(last_valid_pos) {
            let _ = self.job_tx.send(ReportJob::Stopped(data));
        }
    }

    // Fire-and-forget report_start for a new item. Used once transition_to has
    // already updated self.ids synchronously via get_playback_info, so this
    // call is pure Emby bookkeeping the session doesn't need to wait on.
    fn report_start_background(
        &self,
        item: &EmbyItem,
        media_source_id: &MediaSourceId,
        session_id: &EmbySessionId,
    ) {
        let _ = self.job_tx.send(ReportJob::Start {
            client: self.client.clone(),
            item: item.clone(),
            ids: StartIds::Resolved {
                media_source_id: media_source_id.clone(),
                session_id: session_id.clone(),
            },
        });
    }

    fn report_stopped_for_shutdown(&self, last_valid_pos: i64, timeout: Duration) -> bool {
        if !self.has_session() {
            return false;
        }
        let (id, msid, sid) = self.ids.lock().unwrap_or_else(|e| e.into_inner()).clone();
        let is_audio = self.is_audio.load(Ordering::Relaxed);
        let pos = if is_audio { 0 } else { last_valid_pos };
        let runtime_ticks = self
            .status
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .runtime_ticks;
        if let Some(ref tx) = self.ws_tx {
            if tx.is_connected() {
                let _ = tx.flush(timeout.min(Duration::from_secs(1)));
            }
        }
        log::info!(target: "player", "report_stopped shutdown: item={id} is_audio={is_audio} last_valid_pos={}s sending pos={}s timeout={}ms",
            last_valid_pos / TICKS_PER_SECOND, pos / TICKS_PER_SECOND, timeout.as_millis());
        self.client
            .report_stopped_for_shutdown(&id, &msid, pos, &sid, runtime_ticks, timeout)
    }

    fn report_ping(&self) {
        let sid = self.ids.lock().unwrap_or_else(|e| e.into_inner()).2.clone();
        self.client.report_ping(&sid);
    }

    // get_playback_info + report_start for a new item, updating tracking ids
    // *before* the network call so the progress reporter thread never sends
    // stale IDs to Emby.
    // Returns (ext_sub_urls, success).
    fn start_item(&self, item: &EmbyItem) -> (Vec<String>, bool) {
        let info = self.client.get_playback_info(&item.id);
        // Update ids before report_start so the progress reporter (which reads
        // ids on a 10-second timer) always sees the new item.
        {
            let mut ids = self.ids.lock().unwrap_or_else(|e| e.into_inner());
            ids.0 = ItemId::new(item.id.clone());
            ids.1 = info.media_source_id.clone();
            ids.2 = info.session_id.clone();
        }
        self.is_audio.store(item.is_audio(), Ordering::Relaxed);
        let ok = self
            .client
            .report_start(item, &info.media_source_id, &info.session_id);
        (info.external_subtitle_urls, ok)
    }

    // report_stopped for the current item and report_start for the new one are
    // both pure Emby bookkeeping, so both fire on background threads. Only
    // get_playback_info runs synchronously here — the session needs its ids
    // and ext_sub_urls before loadfile can be issued for the new item.
    fn transition_to(&self, new_item: &EmbyItem, last_valid_pos: i64) -> Vec<String> {
        self.report_stopped_background(last_valid_pos);
        let info = self.client.get_playback_info(&new_item.id);
        let ext_sub_urls = info.external_subtitle_urls;
        {
            let mut ids = self.ids.lock().unwrap_or_else(|e| e.into_inner());
            ids.0 = ItemId::new(new_item.id.clone());
            ids.1 = info.media_source_id.clone();
            ids.2 = info.session_id.clone();
        }
        self.is_audio.store(new_item.is_audio(), Ordering::Relaxed);
        self.report_start_background(new_item, &info.media_source_id, &info.session_id);
        ext_sub_urls
    }

    // Fully deferred variant of transition_to for pipe output, where ext_sub_urls
    // are irrelevant (audio-only) and the progress reporter can tolerate briefly
    // stale ids. Moves get_playback_info off the player thread so loadfile can be
    // issued immediately.
    fn transition_to_deferred(&self, new_item: &EmbyItem, last_valid_pos: i64) {
        self.report_stopped_background(last_valid_pos);
        let _ = self.job_tx.send(ReportJob::Start {
            client: self.client.clone(),
            item: new_item.clone(),
            ids: StartIds::Deferred {
                ids: self.ids.clone(),
                is_audio: self.is_audio.clone(),
            },
        });
    }
}
