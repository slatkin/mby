// Write end of a self-pipe used to wake the player event loop immediately
// (see player_session_run.rs) instead of it polling on a fixed timeout.
// Closes the fd on drop so replacing it (each play()/play_queue() call makes
// a fresh pipe) never leaks fds.
struct WakeupWriter(RawFd);

impl WakeupWriter {
    fn notify(&self) {
        let byte = [0u8; 1];
        unsafe {
            libc::write(self.0, byte.as_ptr() as *const libc::c_void, 1);
        }
    }
}

impl Drop for WakeupWriter {
    fn drop(&mut self) {
        unsafe {
            libc::close(self.0);
        }
    }
}

// Returns (read_fd, write end) on success. Both fds are non-blocking. `None`
// on pipe(2) failure; callers fall back to bounded polling in that case.
fn make_wakeup_pipe() -> Option<(RawFd, WakeupWriter)> {
    let mut fds = [-1i32; 2];
    if unsafe { libc::pipe(fds.as_mut_ptr()) } != 0 {
        log::warn!(target: "player", "wakeup pipe: pipe(2) failed: {}", std::io::Error::last_os_error());
        return None;
    }
    for fd in fds {
        unsafe {
            libc::fcntl(fd, libc::F_SETFL, libc::O_NONBLOCK);
        }
    }
    Some((fds[0], WakeupWriter(fds[1])))
}

pub struct QuitHandle {
    stop_tx: Arc<Mutex<Option<mpsc::Sender<()>>>>,
    shutdown_report_timeout: Arc<Mutex<Option<Duration>>>,
}

impl QuitHandle {
    pub fn stop(&self) {
        if let Some(tx) = self.stop_tx.lock().unwrap().take() {
            let _ = tx.send(());
        }
    }

    pub fn stop_for_shutdown(&self, timeout: Duration) {
        *self.shutdown_report_timeout.lock().unwrap() = Some(timeout);
        self.stop();
    }
}

// ── Player ────────────────────────────────────────────────────────────────────

pub struct Player {
    credentials: Arc<Mutex<Option<(String, String)>>>,
    audiobookshelf_context: Arc<Mutex<Option<AudiobookshelfPlayerContext>>>,
    show_audio_window: bool,
    use_mpv_config: bool,
    no_scripts: bool,
    /// Fixed for this Player's lifetime; `Some` only for packaged-daemon
    /// clocked output (see `with_audio_device`). `None` for bare mode and
    /// the Local daemon, and for any run where pipe output is selected.
    audio_device: Option<String>,
    pub always_skip_intro: bool,
    pub subtitle_prefs: Arc<Mutex<SubtitlePrefs>>,
    origin: Mutex<PlaybackOrigin>,
    current_is_headless: Arc<AtomicBool>,
    pub event_tx: mpsc::Sender<PlayerEvent>,
    stop_tx: Arc<Mutex<Option<mpsc::Sender<()>>>>,
    shutdown_report_timeout: Arc<Mutex<Option<Duration>>>,
    pub cmd_tx: Arc<Mutex<Option<mpsc::Sender<PlayerCommand>>>>,
    wakeup_fd: Arc<Mutex<Option<WakeupWriter>>>,
    pre_warmed_mpv: Arc<Mutex<Option<(Mpv, bool)>>>,
    pub status: Arc<Mutex<PlayerStatus>>,
    thread_handle: Mutex<Option<thread::JoinHandle<()>>>,
    ws_tx: Arc<Mutex<Option<crate::ws::WsSender>>>,
}

impl Player {
    pub fn new(
        server_url: String,
        token: String,
        show_audio_window: bool,
        use_mpv_config: bool,
        no_scripts: bool,
        always_skip_intro: bool,
        subtitle_prefs: SubtitlePrefs,
        event_tx: mpsc::Sender<PlayerEvent>,
        ws_tx: Option<crate::ws::WsSender>,
    ) -> Self {
        Player {
            credentials: Arc::new(Mutex::new(
                (!server_url.is_empty() || !token.is_empty()).then_some((server_url, token)),
            )),
            audiobookshelf_context: Arc::new(Mutex::new(None)),
            show_audio_window,
            use_mpv_config,
            no_scripts,
            audio_device: None,
            always_skip_intro,
            subtitle_prefs: Arc::new(Mutex::new(subtitle_prefs)),
            origin: Mutex::new(PlaybackOrigin::Standalone),
            current_is_headless: Arc::new(AtomicBool::new(false)),
            event_tx,
            stop_tx: Arc::new(Mutex::new(None)),
            shutdown_report_timeout: Arc::new(Mutex::new(None)),
            cmd_tx: Arc::new(Mutex::new(None)),
            wakeup_fd: Arc::new(Mutex::new(None)),
            pre_warmed_mpv: Arc::new(Mutex::new(None)),
            status: Arc::new(Mutex::new(PlayerStatus::default())),
            thread_handle: Mutex::new(None),
            ws_tx: Arc::new(Mutex::new(ws_tx)),
        }
    }

    /// Sets the fixed ALSA device identifier packaged-daemon clocked output
    /// projects on every run for the remainder of this Player's lifetime
    /// (restart-required, matching `audio_device`'s owner-local semantics).
    /// Bare mode and the Local daemon must never call this.
    pub fn with_audio_device(mut self, audio_device: Option<String>) -> Self {
        self.audio_device = audio_device;
        self
    }

    pub fn pre_warm(&self, pipe_path: Option<String>, samplerate: u32, bitdepth: u8) {
        if pipe_path.is_none() {
            return;
        }
        let config = MpvRunConfig {
            headless: true,
            use_mpv_config: self.use_mpv_config,
            no_scripts: self.no_scripts,
            always_skip_intro: self.always_skip_intro,
            audio_pipe_path: pipe_path,
            audio_pipe_samplerate: samplerate,
            audio_pipe_bitdepth: bitdepth,
            audio_device: None,
        };
        match init_mpv(&config) {
            Ok(warmed) => {
                log::info!(target: "player", "pre-warmed mpv for pipe output");
                *self.pre_warmed_mpv.lock().unwrap() = Some(warmed);
            }
            Err(e) => {
                log::warn!(target: "player", "pre-warm failed: {e}");
            }
        }
    }

    /// Update the local Player owner's Emby access after late Service setup.
    /// This is intentionally an in-process seam; it is not part of ctrl.
    pub fn update_emby_credentials(&self, server_url: String, token: String) {
        // The playback thread snapshots these fields when a run starts. The
        // mutexes keep late setup and a concurrent submission from racing.
        let mut credentials = self.credentials.lock().unwrap();
        if server_url.is_empty() && token.is_empty() {
            *credentials = None;
        } else {
            *credentials = Some((server_url, token));
        }
    }

    pub fn update_emby_runtime(
        &self,
        server_url: String,
        token: String,
        ws_tx: crate::ws::WsSender,
    ) {
        self.update_emby_credentials(server_url, token);
        *self.ws_tx.lock().unwrap() = Some(ws_tx);
    }

    /// Replace or clear runtime-only Audiobookshelf access. This seam is
    /// deliberately in-process and is not represented by PlayerCommand/ctrl.
    pub fn update_audiobookshelf_context(&self, context: Option<AudiobookshelfPlayerContext>) {
        *self.audiobookshelf_context.lock().unwrap() = context;
    }

    /// Audiobookshelf admission belongs only to this in-process Player. The
    /// prepared-source boundary and the closed reporting/finalization
    /// lifecycle are owner-local code paths in `PlaybackRun`; the current
    /// context is the runtime gate that makes that complete capability
    /// available. Clearing it immediately makes new Bound admission fail.
    pub fn can_admit_audiobookshelf(&self) -> bool {
        self.audiobookshelf_context.lock().unwrap().is_some()
    }

    #[cfg(any(test, feature = "test-support"))]
    pub fn audiobookshelf_generation(&self) -> Option<crate::service_runtime::SetupGeneration> {
        self.audiobookshelf_context
            .lock()
            .unwrap()
            .as_ref()
            .map(AudiobookshelfPlayerContext::generation)
    }

    #[cfg(any(test, feature = "test-support"))]
    pub fn emby_credentials(&self) -> Option<(String, String)> {
        self.credentials.lock().unwrap().clone()
    }

    pub fn join(&self) {
        let handle = self.thread_handle.lock().unwrap().take();
        if let Some(h) = handle {
            let _ = h.join();
        }
    }

    // Join the player thread but give up after `timeout`. Used on SIGHUP/SIGTERM
    // so the process always exits even if an HTTP call is hanging.
    pub fn join_or_timeout(&self, timeout: std::time::Duration) {
        let handle = self.thread_handle.lock().unwrap().take();
        if let Some(h) = handle {
            let (tx, rx) = std::sync::mpsc::channel::<()>();
            std::thread::spawn(move || {
                let _ = h.join();
                let _ = tx.send(());
            });
            let _ = rx.recv_timeout(timeout);
        }
    }

    /// Returns `true` if the command was sent, `false` if the player thread is gone.
    pub fn send_command(&self, cmd: PlayerCommand) -> bool {
        let sent = if let Some(tx) = self.cmd_tx.lock().unwrap().as_ref() {
            tx.send(cmd).is_ok()
        } else {
            false
        };
        if sent {
            if let Some(w) = self.wakeup_fd.lock().unwrap().as_ref() {
                w.notify();
            }
        }
        sent
    }

    #[cfg(test)]
    pub(crate) fn spy_on_commands(&self) -> mpsc::Receiver<PlayerCommand> {
        let (tx, rx) = mpsc::channel();
        *self.cmd_tx.lock().unwrap() = Some(tx);
        rx
    }

    pub fn next(&self) -> bool {
        match self.status.lock().unwrap().next_idx() {
            Some(idx) => self.send_command(PlayerCommand::JumpTo(idx)),
            None => false,
        }
    }

    pub fn previous(&self) -> bool {
        match self.status.lock().unwrap().previous_idx() {
            Some(idx) => self.send_command(PlayerCommand::JumpTo(idx)),
            None => false,
        }
    }

    pub fn set_paused(&self, paused: bool) -> bool {
        match self.status.lock().unwrap().toggle_to_reach(paused) {
            Some(cmd) => self.send_command(cmd),
            None => false,
        }
    }

    /// Seed queue/status state without starting playback. Used when a freshly
    /// spawned local daemon should inherit a queue snapshot before any thin
    /// client connects, while an already-running daemon keeps its live state.
    pub fn set_initial_queue(&self, items: &[QueueItem], cursor: usize) {
        let mut st = self.status.lock().unwrap();
        if items.is_empty() {
            st.position_ticks = 0;
            st.runtime_ticks = 0;
            st.paused = false;
            st.current_idx = 0;
            st.queue_len = 0;
            st.active = false;
            st.clear_current_item_metadata();
            return;
        }

        let cursor = cursor.min(items.len().saturating_sub(1));
        let start_item = &items[cursor];
        st.seed_from_item(start_item, cursor, items.len());
        st.active = false;
    }

    // Pipe mode always forces headless (no video window), regardless of item
    // type. Reads `audio_pipe_enabled` from `client.config` (rather than a
    // field cached on `Player`) so a setting toggled mid-session takes effect
    // on the very next play() call instead of requiring an app restart.
    pub(crate) fn headless_for(&self, client: &EmbyClient, is_audio: bool) -> bool {
        client.config.audio_pipe_enabled || (!self.show_audio_window && is_audio)
    }

    pub fn play(&self, item: &EmbyItem, client: Arc<EmbyClient>, initial_volume: u8) {
        let headless = self.headless_for(&client, item.is_audio());
        self.submit_queue(
            vec![QueueItem::Emby(Box::new(item.clone()))],
            0,
            Some(client),
            headless,
            initial_volume,
        );
    }

    pub fn play_queue(
        &self,
        items: Vec<EmbyItem>,
        start_idx: usize,
        client: Arc<EmbyClient>,
        initial_volume: u8,
    ) {
        if items.is_empty() {
            return;
        }
        let all_audio = items
            .iter()
            .all(|i| i.media_type == "Audio" || i.item_type == "Audio");
        let headless = self.headless_for(&client, all_audio);
        let queue_items: Vec<QueueItem> = items
            .into_iter()
            .map(|i| QueueItem::Emby(Box::new(i)))
            .collect();
        self.submit_queue(
            queue_items,
            start_idx,
            Some(client),
            headless,
            initial_volume,
        );
    }

    /// Item-generic queue submission: replace the current queue with `items`
    /// and start playback from `start_idx`.  When the player is already
    /// active with a matching headless state the queue is replaced in place
    /// via a `SubmitQueue` command; otherwise a fresh mpv process is
    /// spawned.
    ///
    /// `client` provides the Emby session for reporting (and audio-pipe
    /// config).  Pass `None` for feed-only playback where no Emby session
    /// is established.
    ///
    /// `headless` is computed by the caller — the `play`/`play_queue`
    /// wrappers derive it from `headless_for`.
    pub fn submit_queue(
        &self,
        items: Vec<QueueItem>,
        start_idx: usize,
        client: Option<Arc<EmbyClient>>,
        headless: bool,
        initial_volume: u8,
    ) -> bool {
        if items.is_empty()
            || (items.iter().any(QueueItem::is_audiobookshelf_any) && !self.can_admit_audiobookshelf())
        {
            return false;
        }
        let start_idx = start_idx.min(items.len() - 1);

        // Fast path: reuse existing mpv window when headless state matches.
        if self.status.lock().unwrap().active
            && (self.current_is_headless.load(Ordering::Relaxed) == headless)
        {
            let start_item = &items[start_idx];
            {
                let mut st = self.status.lock().unwrap();
                st.seed_from_item(start_item, start_idx, items.len());
            }
            return self.send_command(PlayerCommand::SubmitQueue { items, start_idx });
        }

        // Cold start: stop, join, spawn fresh player thread.
        self.stop();
        self.join();

        let (audio_pipe_path, audio_pipe_samplerate, audio_pipe_bitdepth, always_skip_intro) =
            if let Some(ref c) = client {
                (
                    c.config.audio_pipe_target(),
                    c.config.audio_pipe_samplerate,
                    c.config.audio_pipe_bitdepth,
                    self.always_skip_intro,
                )
            } else {
                (None, 0, 0, false)
            };
        // Mutually exclusive output target: pipe output wins when selected
        // for this run, and clocked ALSA projection applies only when it is
        // not.
        let (audio_pipe_path, audio_device) = if audio_pipe_path.is_some() {
            (audio_pipe_path, None)
        } else {
            (None, self.audio_device.clone())
        };

        let config = MpvRunConfig {
            headless,
            use_mpv_config: self.use_mpv_config,
            no_scripts: self.no_scripts,
            always_skip_intro,
            audio_pipe_path,
            audio_pipe_samplerate,
            audio_pipe_bitdepth,
            audio_device,
        };
        let status = self.status.clone();
        let event_tx = self.event_tx.clone();
        let ws_tx = if client.is_some() {
            self.ws_tx.lock().unwrap().clone()
        } else {
            None
        };
        let subtitle_prefs = self.subtitle_prefs.clone();
        let shutdown_report_timeout = self.shutdown_report_timeout.clone();
        let (server_url, token) = self.credentials.lock().unwrap().clone().unwrap_or_default();
        let audiobookshelf_context = self.audiobookshelf_context.lock().unwrap().clone();
        let origin = if items.len() == 1 {
            PlaybackOrigin::Standalone
        } else {
            PlaybackOrigin::Queue
        };
        *self.origin.lock().unwrap() = origin;
        self.current_is_headless.store(headless, Ordering::Relaxed);

        // Set initial status for the start item.
        let start_item = &items[start_idx];
        {
            let mut st = status.lock().unwrap();
            st.seed_from_item(start_item, start_idx, items.len());
            st.active = true;
        }

        let (stop_tx, stop_rx) = mpsc::channel::<()>();
        *self.stop_tx.lock().unwrap() = Some(stop_tx);
        *self.shutdown_report_timeout.lock().unwrap() = None;
        let (cmd_tx, cmd_rx) = mpsc::channel::<PlayerCommand>();
        *self.cmd_tx.lock().unwrap() = Some(cmd_tx);
        let wakeup_pipe = make_wakeup_pipe();
        let wakeup_read_fd = wakeup_pipe.as_ref().map(|(r, _)| *r).unwrap_or(-1);
        let wakeup_write_fd = wakeup_pipe.as_ref().map(|(_, w)| w.0).unwrap_or(-1);
        *self.wakeup_fd.lock().unwrap() = wakeup_pipe.map(|(_, w)| w);
        let pre_warmed = self.pre_warmed_mpv.lock().unwrap().take();

        let handle = thread::spawn(move || {
            let (mpv, startup_pause_for_pipe) = match pre_warmed {
                Some(w) => w,
                None => match init_mpv(&config) {
                    Ok(v) => v,
                    Err(e) => {
                        log::error!(target: "player", "{}", e);
                        status.lock().unwrap().active = false;
                        let _ = event_tx.send(PlayerEvent::Stopped {
                            idx: 0,
                            position_ticks: 0,
                            played: false,
                            consume: false,
                            progress_report_accepted: false,
                            error: Some(format!("mpv startup failed: {e}")),
                        });
                        return;
                    }
                },
            };
            init_volume(&mpv, &status, initial_volume);

            let active_file_projection = items.iter().any(QueueItem::is_audiobookshelf_any);
            let load_indices: Vec<_> = if active_file_projection {
                vec![start_idx]
            } else {
                queue_load_indices(items.len(), start_idx).collect()
            };
            let mut active_prepared_source = None;
            for i in load_indices {
                let item = &items[i];
                let prepared = match prepare_source(
                    item,
                    &server_url,
                    &token,
                    audiobookshelf_context.as_ref(),
                ) {
                    Ok(source) => source,
                    Err(error) => {
                        status.lock().unwrap().active = false;
                        let _ = event_tx.send(PlayerEvent::Stopped {
                            idx: start_idx,
                            position_ticks: 0,
                            played: false,
                            consume: false,
                            progress_report_accepted: false,
                            error: Some(format!("failed to prepare media: {error}")),
                        });
                        return;
                    }
                };
                let (mode, index) = queue_load_location(i, start_idx);
                let opts = prepared.mpv_load_options(item);
                if let Err(e) =
                    mpv.command("loadfile", &[prepared.url.as_str(), mode, &index, &opts])
                {
                    log::warn!(
                        target: "player",
                        "submit_queue loadfile error: {e} | mode={mode}",
                    );
                    if i == start_idx {
                        let mut prepared = prepared;
                        prepared.close(0.0);
                        status.lock().unwrap().active = false;
                        let _ = event_tx.send(PlayerEvent::Stopped {
                            idx: 0,
                            position_ticks: 0,
                            played: false,
                            consume: false,
                            progress_report_accepted: false,
                            error: Some(format!("failed to load media: {e}")),
                        });
                        return;
                    }
                }
                if i == start_idx {
                    active_prepared_source = Some(prepared);
                }
            }
            // send_ep_info only for Emby items.
            if let Some(emby) = items.get(start_idx).and_then(|i| i.as_emby()) {
                send_ep_info(&mpv, emby);
            }
            observe_properties(&mpv, config.use_mpv_config);

            // Set up reporter based on client availability and item variant.
            let (reporter, progress) = if let Some(client) = client {
                if let Some(emby) = items.get(start_idx).and_then(|i| i.as_emby()) {
                    let info = client.get_playback_info(&emby.id);
                    let reporter = SessionReporter::new(
                        client.clone(),
                        ws_tx,
                        ItemId::new(emby.id.clone()),
                        info.media_source_id.clone(),
                        info.session_id.clone(),
                        emby.is_audio(),
                        status.clone(),
                    );
                    let progress = spawn_progress_reporter(reporter.clone());
                    {
                        let c = client;
                        let item = emby.clone();
                        let msid = info.media_source_id;
                        let sid = info.session_id;
                        thread::spawn(move || {
                            let ok = c.report_start(&item, &msid, &sid);
                            if !ok {
                                log::warn!(
                                    target: "player",
                                    "report_start failed for item={}",
                                    item.id,
                                );
                            }
                        });
                    }
                    (reporter, progress)
                } else {
                    // Feed item with client available — clear session so
                    // all report calls are no-ops.
                    let reporter = SessionReporter::new(
                        client,
                        ws_tx,
                        ItemId::empty(),
                        MediaSourceId::new(""),
                        EmbySessionId::new(""),
                        items[start_idx].is_audio(),
                        status.clone(),
                    );
                    reporter.clear_session();
                    let (noop_stop_tx, _noop_stop_rx) = mpsc::channel();
                    let progress = ProgressGuard {
                        stop_tx: noop_stop_tx,
                        handle: None,
                    };
                    (reporter, progress)
                }
            } else {
                // No client (feed-only): no-op reporter.
                let client = Arc::new(EmbyClient::new(crate::config::Config::default()));
                let reporter = SessionReporter::new(
                    client,
                    None,
                    ItemId::empty(),
                    MediaSourceId::new(""),
                    EmbySessionId::new(""),
                    items[start_idx].is_audio(),
                    status.clone(),
                );
                reporter.clear_session();
                let (noop_stop_tx, _noop_stop_rx) = mpsc::channel();
                let progress = ProgressGuard {
                    stop_tx: noop_stop_tx,
                    handle: None,
                };
                (reporter, progress)
            };

            let session = PlaybackRun::new_from_queue_items(
                items,
                start_idx,
                origin,
                reporter,
                config,
                startup_pause_for_pipe,
                status,
                event_tx,
                subtitle_prefs,
                shutdown_report_timeout,
                server_url,
                token,
                audiobookshelf_context,
                active_prepared_source,
            );
            session.run(
                mpv,
                stop_rx,
                cmd_rx,
                progress,
                wakeup_read_fd,
                wakeup_write_fd,
            );
        });
        *self.thread_handle.lock().unwrap() = Some(handle);
        true
    }

    pub fn queue_append(&self, items: Vec<QueueItem>) -> bool {
        if items.is_empty()
            || (items.iter().any(QueueItem::is_audiobookshelf_any) && !self.can_admit_audiobookshelf())
        {
            return false;
        }
        self.send_command(PlayerCommand::QueueAppend { items })
    }

    pub fn stop(&self) {
        if let Some(tx) = self.stop_tx.lock().unwrap().take() {
            let _ = tx.send(());
        }
        if let Some(w) = self.wakeup_fd.lock().unwrap().as_ref() {
            w.notify();
        }
        // Don't clear cmd_tx here: a LoadNew command sent after stop() must still
        // reach the thread so it can cancel the quit and load the new file instead.
    }

    pub fn stop_for_shutdown(&self, timeout: Duration) {
        *self.shutdown_report_timeout.lock().unwrap() = Some(timeout);
        self.stop();
    }
}

// ── PlayerProxy ─────────────────────────────────────────────────────────────
// Wraps either a local Player or a RemotePlayer so App can use a single type.
