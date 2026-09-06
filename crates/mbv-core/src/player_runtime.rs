struct ProgressGuard {
    stop_tx: mpsc::Sender<()>,
    handle: Option<thread::JoinHandle<()>>,
}

impl ProgressGuard {
    fn stop_and_join(&mut self, budget: Duration) {
        let _ = self.stop_tx.send(());
        if let Some(h) = self.handle.take() {
            let start = std::time::Instant::now();
            let result = crate::bounded::run_with_hard_bound(
                move || {
                    let _ = h.join();
                    Ok::<(), String>(())
                },
                budget,
            );
            let elapsed = start.elapsed();
            match result {
                Ok(()) => {
                    log::info!(target: "player", "progress_join: joined in {}ms (budget={}ms)",
                    elapsed.as_millis(), budget.as_millis())
                }
                Err(e) => {
                    log::warn!(target: "player", "progress_join: {e} after {}ms (budget={}ms)",
                    elapsed.as_millis(), budget.as_millis())
                }
            }
        }
    }
}

struct MpvRunConfig {
    headless: bool,
    use_mpv_config: bool,
    no_scripts: bool,
    always_skip_intro: bool,
    audio_pipe_path: Option<String>,
    audio_pipe_samplerate: u32,
    audio_pipe_bitdepth: u8,
    /// Mutually exclusive with `audio_pipe_path`: set only when pipe output
    /// is not selected for this run (see `resolve_run_output`).
    audio_device: Option<String>,
}

fn user_mpv_config_dir() -> Option<PathBuf> {
    if let Some(config_home) = std::env::var_os("XDG_CONFIG_HOME") {
        return Some(PathBuf::from(config_home).join("mpv"));
    }
    std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".config").join("mpv"))
}

fn is_mpv_ipc_config_line(line: &str) -> bool {
    let trimmed = line.trim_start();
    if trimmed.starts_with('#') {
        return false;
    }
    let option = trimmed.strip_prefix("--").unwrap_or(trimmed);
    let key_end = option
        .find(|c: char| c == '=' || c.is_whitespace())
        .unwrap_or(option.len());
    &option[..key_end] == "input-ipc-server"
}

fn sanitized_mpv_conf(user_conf: Option<&Path>, ipc_path: &str) -> String {
    let mut sanitized = String::new();
    if let Some(path) = user_conf {
        if let Ok(text) = fs::read_to_string(path) {
            for line in text.lines() {
                if !is_mpv_ipc_config_line(line) {
                    sanitized.push_str(line);
                    sanitized.push('\n');
                }
            }
        }
    }
    sanitized.push_str("input-ipc-server=");
    sanitized.push_str(ipc_path);
    sanitized.push('\n');
    sanitized
}

#[cfg(unix)]
fn symlink_mpv_config_entry(src: &Path, dest: &Path) -> std::io::Result<()> {
    std::os::unix::fs::symlink(src, dest)
}

#[cfg(not(unix))]
fn symlink_mpv_config_entry(src: &Path, dest: &Path) -> std::io::Result<()> {
    let meta = fs::metadata(src)?;
    if meta.is_dir() {
        fs::create_dir(dest)
    } else {
        fs::copy(src, dest).map(|_| ())
    }
}

fn reset_private_mpv_config_dir(private_dir: &Path) -> Result<(), String> {
    match fs::symlink_metadata(private_dir) {
        Ok(meta) if meta.is_dir() && !meta.file_type().is_symlink() => {
            fs::remove_dir_all(private_dir).map_err(|e| {
                format!(
                    "failed to remove private mpv config dir '{}': {e}",
                    private_dir.display()
                )
            })?;
        }
        Ok(_) => {
            fs::remove_file(private_dir).map_err(|e| {
                format!(
                    "failed to remove private mpv config path '{}': {e}",
                    private_dir.display()
                )
            })?;
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => {
            return Err(format!(
                "failed to inspect private mpv config dir '{}': {e}",
                private_dir.display()
            ));
        }
    }
    fs::create_dir_all(private_dir).map_err(|e| {
        format!(
            "failed to create private mpv config dir '{}': {e}",
            private_dir.display()
        )
    })
}

fn prepare_mpv_config_dir(use_mpv_config: bool, ipc_path: &str) -> Result<PathBuf, String> {
    let private_dir = crate::config::mpv_config_dir();
    reset_private_mpv_config_dir(&private_dir)?;

    let user_dir = use_mpv_config.then(user_mpv_config_dir).flatten();
    if let Some(user_dir) = &user_dir {
        match fs::read_dir(user_dir) {
            Ok(entries) => {
                for entry in entries.flatten() {
                    let name = entry.file_name();
                    if name == OsStr::new("mpv.conf") || name == OsStr::new("input.conf") {
                        continue;
                    }
                    let src = entry.path();
                    let dest = private_dir.join(&name);
                    if let Err(e) = symlink_mpv_config_entry(&src, &dest) {
                        log::warn!(target: "player", "mpv config: failed to link {} into private config dir: {e}", src.display());
                    }
                }
            }
            Err(e) => {
                log::warn!(target: "player", "mpv config: cannot read user config dir {}: {e}", user_dir.display());
            }
        }
    }

    let user_conf = user_dir
        .as_ref()
        .map(|dir| dir.join("mpv.conf"))
        .filter(|path| path.exists());
    let conf = sanitized_mpv_conf(user_conf.as_deref(), ipc_path);
    fs::write(private_dir.join("mpv.conf"), conf).map_err(|e| {
        format!(
            "failed to write private mpv.conf in '{}': {e}",
            private_dir.display()
        )
    })?;

    Ok(private_dir)
}

// Ensures `path` exists as a FIFO, creating it via mkfifo(3) if it doesn't
// already exist. Refuses to touch a path that exists but isn't a FIFO.
fn ensure_pipe(path: &str) -> Result<(), String> {
    use std::os::unix::fs::FileTypeExt;
    match std::fs::metadata(path) {
        Ok(meta) if meta.file_type().is_fifo() => Ok(()),
        Ok(_) => Err(format!("audio pipe path '{path}' exists and is not a FIFO")),
        Err(_) => {
            let cpath = std::ffi::CString::new(path).map_err(|e| e.to_string())?;
            let rc = unsafe { libc::mkfifo(cpath.as_ptr(), 0o644) };
            if rc != 0 {
                Err(format!(
                    "mkfifo({path}) failed: {}",
                    std::io::Error::last_os_error()
                ))
            } else {
                Ok(())
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
        SessionReporter {
            client,
            ws_tx,
            ids: Arc::new(Mutex::new((item_id, msid, sid))),
            is_audio: Arc::new(AtomicBool::new(is_audio)),
            status,
        }
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
        if !self.has_session() {
            return;
        }
        let client = self.client.clone();
        let ws_tx = self.ws_tx.clone();
        let (id, msid, sid) = self.ids.lock().unwrap_or_else(|e| e.into_inner()).clone();
        let is_audio = self.is_audio.load(Ordering::Relaxed);
        let pos = if is_audio { 0 } else { last_valid_pos };
        let runtime_ticks = self
            .status
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .runtime_ticks;
        thread::spawn(move || {
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
        });
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
        let client = self.client.clone();
        let item = item.clone();
        let media_source_id = media_source_id.clone();
        let session_id = session_id.clone();
        thread::spawn(move || {
            let ok = client.report_start(&item, &media_source_id, &session_id);
            if !ok {
                log::warn!(target: "player", "transition_to: report_start failed for item={}", item.id);
            }
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
        let client = self.client.clone();
        let ids = self.ids.clone();
        let is_audio_flag = self.is_audio.clone();
        let item = new_item.clone();
        let new_is_audio = new_item.is_audio();
        thread::spawn(move || {
            let info = client.get_playback_info(&item.id);
            {
                let mut locked = ids.lock().unwrap_or_else(|e| e.into_inner());
                locked.0 = ItemId::new(item.id.clone());
                locked.1 = info.media_source_id.clone();
                locked.2 = info.session_id.clone();
            }
            is_audio_flag.store(new_is_audio, Ordering::Relaxed);
            let ok = client.report_start(&item, &info.media_source_id, &info.session_id);
            if !ok {
                log::warn!(target: "player", "transition_to: report_start failed for item={}", item.id);
            }
        });
    }
}

fn init_mpv(config: &MpvRunConfig) -> Result<(Mpv, bool), String> {
    let ipc_path = crate::config::mpv_ipc_path();
    let private_config_dir = prepare_mpv_config_dir(config.use_mpv_config, &ipc_path)?;
    let ipc_existed = Path::new(&ipc_path).exists();
    if ipc_existed {
        let _ = std::fs::remove_file(&ipc_path);
        log::info!(target: "player", "init: removed stale ipc socket {}", ipc_path);
    }
    log::info!(target: "player", "init: ipc={} (existed={})", ipc_path, ipc_existed);

    let no_scripts = config.no_scripts;
    let use_mpv_config = config.use_mpv_config;
    let mut init_err: Option<String> = None;
    let mpv = match Mpv::with_initializer(|init| {
        macro_rules! opt {
            ($k:expr, $v:expr) => {{
                let r = init.set_option($k, $v);
                if let Err(ref e) = r {
                    init_err = Some(format!(
                        "[player] set_option('{}') failed: {}",
                        $k,
                        mpv_err_str(e)
                    ));
                }
                r?;
            }};
        }
        opt!("config", "yes");
        // Use an mbv-owned config dir so user mpv.conf cannot override
        // input-ipc-server during mpv_initialize() and clobber a live mpv socket.
        opt!("config-dir", private_config_dir.to_str().unwrap_or(""));
        opt!("input-ipc-server", ipc_path.as_str());
        opt!("input-default-bindings", "yes");
        opt!("input-vo-keyboard", "yes");
        opt!("wayland-app-id", "mbv");
        opt!("gapless-audio", "weak");
        if no_scripts || !use_mpv_config {
            opt!("load-scripts", "no");
            opt!("osc", "no");
            opt!("osd-bar", "no");
        }
        if !no_scripts && !use_mpv_config {
            let script = crate::config::osc_script_path();
            if script.exists() {
                opt!("scripts", script.to_str().unwrap_or(""));
                let fonts = crate::config::osc_fonts_dir();
                opt!("osd-fonts-dir", fonts.to_str().unwrap_or(""));
            }
        }
        Ok(())
    }) {
        Ok(m) => m,
        Err(e) => {
            let msg =
                init_err.unwrap_or_else(|| format!("[player] mpv init error: {}", mpv_err_str(&e)));
            return Err(msg);
        }
    };

    unsafe {
        let log_level = if cfg!(debug_assertions) {
            c"warn"
        } else {
            c"error"
        };
        libmpv2_sys::mpv_request_log_messages(mpv.ctx.as_ptr(), log_level.as_ptr() as _);
    }

    // Set after init so user's mpv.conf cannot override these.
    if config.headless {
        let _ = mpv.set_property("vo", "null");
        let _ = mpv.set_property("force-window", "no");
        // #656: with vo=null, attached cover art would still be selected and
        // decoded (video/image=true per audio track) for no benefit.
        let _ = mpv.set_property("audio-display", "no");
        // Audio-sized demuxer cache: a headless host has no video window to
        // justify the video-sized budget below.
        let _ = mpv.set_property("demuxer-max-bytes", "10M");
        let _ = mpv.set_property("demuxer-max-back-bytes", "10M");
    } else {
        let _ = mpv.set_property("demuxer-max-bytes", "50M");
        let _ = mpv.set_property("demuxer-max-back-bytes", "100M");
    }
    let mut startup_pause_armed = false;
    if let Some(path) = &config.audio_pipe_path {
        match ensure_pipe(path) {
            Ok(()) => {
                let rate = config.audio_pipe_samplerate.to_string();
                let (bitdepth, audio_format) = match config.audio_pipe_bitdepth {
                    16 => (16u8, "s16"),
                    24 => (24u8, "s24"),
                    _ => (32u8, "s32"),
                };
                let mut failed = Vec::new();
                if let Err(e) = mpv.set_property("ao", "pcm") {
                    failed.push(format!("ao: {}", mpv_err_str(&e)));
                }
                if let Err(e) = mpv.set_property("ao-pcm-file", path.as_str()) {
                    failed.push(format!("ao-pcm-file: {}", mpv_err_str(&e)));
                }
                if let Err(e) = mpv.set_property("ao-pcm-waveheader", "no") {
                    failed.push(format!("ao-pcm-waveheader: {}", mpv_err_str(&e)));
                }
                // Force a fixed <bitdepth>-bit/stereo/<rate> PCM format so the byte
                // stream always matches a single Snapcast `sampleformat`
                // declaration, no matter the source file's native format.
                // 32-bit remains the default for headroom, but narrower
                // bit depths improve compatibility with some Snapclients.
                if let Err(e) = mpv.set_property("audio-format", audio_format) {
                    failed.push(format!("audio-format: {}", mpv_err_str(&e)));
                }
                if let Err(e) = mpv.set_property("audio-channels", "stereo") {
                    failed.push(format!("audio-channels: {}", mpv_err_str(&e)));
                }
                if let Err(e) = mpv.set_property("audio-samplerate", rate.as_str()) {
                    failed.push(format!("audio-samplerate: {}", mpv_err_str(&e)));
                }
                if let Err(e) =
                    mpv.set_property("audio-swresample-o", "resampler=soxr,precision=28")
                {
                    failed.push(format!("audio-swresample-o: {}", mpv_err_str(&e)));
                }
                if failed.is_empty() {
                    startup_pause_armed = true;
                    log::info!(target: "player", "audio pipe: writing {rate}Hz/{bitdepth}-bit/stereo PCM to {path} (blocks until a reader attaches)");
                } else {
                    log::warn!(target: "player", "audio pipe: failed to configure pcm output for {path}: {}", failed.join(", "));
                }
            }
            Err(e) => log::warn!(target: "player", "audio pipe disabled for this session: {e}"),
        }
    } else if let Some(device) = &config.audio_device {
        // Clocked ALSA output: the device identifier alone selects the
        // backend, so `ao` is left to mpv's own negotiation.
        if let Err(e) = mpv.set_property("audio-device", device.as_str()) {
            return Err(format!(
                "clocked audio output: failed to set audio-device '{device}': {}",
                mpv_err_str(&e)
            ));
        } else {
            log::info!(target: "player", "clocked audio output: using ALSA device {device}");
        }
    }
    if startup_pause_armed {
        if let Err(e) = mpv.set_property("pause", true) {
            log::warn!(
                target: "player",
                "audio pipe: failed to pre-pause startup: {}",
                mpv_err_str(&e)
            );
            startup_pause_armed = false;
        }
    }

    Ok((mpv, startup_pause_armed))
}

fn init_volume(mpv: &Mpv, status: &Arc<Mutex<PlayerStatus>>, initial_volume: u8) {
    let mut st = status.lock().unwrap();
    let raw_max = mpv.get_property::<i64>("volume-max").unwrap_or(130);
    st.volume_max = raw_max * raw_max / 100;
    let v = (initial_volume as i64).clamp(0, st.volume_max);
    let raw = (10.0 * (v as f64).sqrt()).round() as i64;
    let _ = mpv.set_property("volume", raw as f64);
    st.volume = v;
}

fn observe_properties(mpv: &Mpv, use_mpv_config: bool) {
    let _ = mpv.observe_property("time-pos", Format::Double, 0);
    let _ = mpv.observe_property("pause", Format::Flag, 1);
    let _ = mpv.observe_property("volume", Format::Double, 2);
    let _ = mpv.observe_property("sid", Format::String, 3);
    let _ = mpv.observe_property("mute", Format::Flag, 4);
    let _ = mpv.observe_property("aid", Format::String, 5);
    let _ = mpv.observe_property("video-params/h", Format::Int64, 6);
    let _ = mpv.observe_property("audio-codec-name", Format::String, 7);
    let _ = mpv.observe_property("current-tracks/video/image", Format::Flag, 8);
    let _ = mpv.observe_property("playlist-pos", Format::Int64, 9);
    let _ = mpv.observe_property("playlist-count", Format::Int64, 10);
    if use_mpv_config {
        let _ = mpv.command("keybind", &["MOUSE_MOVE", "script-message mouse-moved"]);
    }
}

fn spawn_progress_reporter(reporter: SessionReporter) -> ProgressGuard {
    let (stop_tx, stop_rx) = mpsc::channel::<()>();
    let interval = Duration::from_secs(reporter.client.config.progress_interval_secs);
    let handle = thread::spawn(move || loop {
        match stop_rx.recv_timeout(interval) {
            Ok(_) | Err(mpsc::RecvTimeoutError::Disconnected) => break,
            Err(mpsc::RecvTimeoutError::Timeout) => {
                reporter.report_progress("TimeUpdate");
                reporter.report_ping();
            }
        }
    });
    ProgressGuard {
        stop_tx,
        handle: Some(handle),
    }
}

fn handle_intro(
    ticks: i64,
    start: i64,
    end: i64,
    intro_state: &mut IntroState,
    always_skip: bool,
    mpv: &Mpv,
    event_tx: &mpsc::Sender<PlayerEvent>,
) {
    if end <= start {
        return;
    }
    if intro_state.is_pending() && ticks >= start {
        intro_state.shown();
        if ticks < end {
            let end_secs = end as f64 / TICKS_PER_SECOND as f64;
            if always_skip {
                let _ = mpv.set_property("time-pos", end_secs);
            } else {
                let _ = event_tx.send(PlayerEvent::IntroStarted {
                    intro_end_ticks: end,
                });
                let _ = mpv.command("script-message", &["mbv-skip-intro", &end_secs.to_string()]);
            }
        } else {
            intro_state.dismissed();
        }
    }
    if intro_state == &IntroState::Shown && ticks >= end {
        intro_state.dismissed();
        let _ = event_tx.send(PlayerEvent::IntroEnded);
        let _ = mpv.command("script-message", &["mbv-skip-intro-dismiss"]);
    }
}

// ── PlaybackRun ─────────────────────────────────────────────────────────

/// Where index `idx` ends up after moving the entry at `from` to `to`
/// (both 0-based positions in the same list, `from != to`).
pub(crate) fn shift_index_for_move(idx: usize, from: usize, to: usize) -> usize {
    if idx == from {
        to
    } else if from < idx && idx <= to {
        idx - 1
    } else if to <= idx && idx < from {
        idx + 1
    } else {
        idx
    }
}
