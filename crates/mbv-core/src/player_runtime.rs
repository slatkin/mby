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
