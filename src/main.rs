use mimalloc::MiMalloc;

#[global_allocator]
static GLOBAL: MiMalloc = MiMalloc;

mod app;
mod config;
mod local_daemon;
mod mpris;
mod single_instance;
mod tray;

use app::{App, Model};
use config::load_config;
use mbv_core::api::EmbyClient;
use mbv_core::{applog, player, remote_player};

/// Shared by both daemon-connection call sites in `main()` below: run the
/// TUI as a thin client of a connected daemon, exiting with an error if the
/// event loop itself fails. Callers still `return` after calling this so
/// control flow at each call site stays identical to before.
///
/// `App::new_remote` starts MPRIS itself (#160, moved there from here in
/// #175 so `App` owns the resulting handle and can `rebind` it later --
/// see `App::switch_to_direct_remote` / `App::restore_local_mode`). This is
/// still safe against a same-machine bus-name collision for the reason the
/// original comment here noted: `mbvd` has no D-Bus/zbus dependency and
/// never claims `org.mpris.MediaPlayer2.mbv` itself (`on_player_ready` is
/// wired to a no-op in `crates/mbvd/src/main.rs`), so this client is the
/// only thing that will ever own the name for a daemon-connected session,
/// whether the daemon is local or genuinely remote.
fn run_remote_app(
    client: Option<EmbyClient>,
    remote: remote_player::RemotePlayer,
    player_rx: std::sync::mpsc::Receiver<player::PlayerEvent>,
    endpoint: remote_player::DaemonEndpoint,
    config: config::Config,
) {
    let mut app = App::new_remote_optional_with_config(client, remote, player_rx, endpoint, config);
    app.init_image_pickers();
    if let Err(e) = Model::new(app).run() {
        eprintln!("Error: {e}");
        std::process::exit(1);
    }
}

fn connect_daemon_arg(args: &[String]) -> Result<Option<String>, String> {
    let mut endpoint: Option<String> = None;
    let mut iter = args.iter();
    while let Some(arg) = iter.next() {
        if let Some(value) = arg.strip_prefix("--connect-daemon=") {
            endpoint = Some(value.to_string());
        } else if arg == "--connect-daemon" {
            let Some(value) = iter.next() else {
                return Err("mbv: --connect-daemon requires an endpoint".to_string());
            };
            endpoint = Some(value.to_string());
        }
    }
    Ok(endpoint)
}

fn has_flag(args: &[String], flag: &str) -> bool {
    args.iter().any(|arg| arg == flag)
}

fn cached_emby_client(config: &config::Config) -> Option<EmbyClient> {
    let token = mbv_core::config::load_service_secret(mbv_core::config::ServiceKind::Emby)?;
    let setup = config.emby_setup.as_ref()?;
    let mut client = EmbyClient::new(config.clone());
    client.config.server_url = setup.server_url.clone();
    client.user_id = setup.user_id.clone();
    client.token = token;
    Some(client)
}

fn state_dir() -> std::path::PathBuf {
    std::env::var("XDG_STATE_HOME")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|_| {
            std::path::PathBuf::from(std::env::var("HOME").unwrap_or_default())
                .join(".local")
                .join("state")
        })
        .join("mbv")
}

fn crash_log_path() -> std::path::PathBuf {
    state_dir().join("mbv.log")
}

fn config_diagnostic_summary(config: &config::Config) -> String {
    let mut routes: Vec<_> = config.library_routes.iter().collect();
    routes.sort_by(|a, b| a.0.cmp(b.0));
    let entries = routes
        .into_iter()
        .map(|(library, endpoint)| format!("{library}={endpoint}"))
        .collect::<Vec<_>>()
        .join(", ");
    format!(
        "config loaded: auto_reconnect={} library_routes={} entries=[{}]",
        config.auto_reconnect,
        config.library_routes.len(),
        entries
    )
}

fn write_crash_log(msg: &str) {
    let _ = crossterm::terminal::disable_raw_mode();
    let _ = crossterm::execute!(std::io::stdout(), crossterm::terminal::LeaveAlternateScreen);
    // Write directly to stderr (async-signal-safe, no mutex)
    use std::io::Write;
    let _ = std::io::stderr().write_all(msg.as_bytes());
    let _ = std::io::stderr().write_all(b"\n");
    log::error!(target: "crash", "{msg}");
    if let Ok(mut f) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(crash_log_path())
    {
        let _ = writeln!(f, "{msg}");
    }
}

fn install_panic_hook() {
    std::panic::set_hook(Box::new(|info| {
        let msg = format!("PANIC: {info}");
        write_crash_log(&msg);
        eprintln!("{msg}");
    }));
}

fn install_signal_handlers() {
    // Write a crash log entry for fatal signals before the process dies.
    unsafe {
        for &sig in &[libc::SIGSEGV, libc::SIGILL, libc::SIGBUS, libc::SIGFPE] {
            libc::signal(
                sig,
                signal_handler as extern "C" fn(libc::c_int) as libc::sighandler_t,
            );
        }
    }
}

extern "C" fn signal_handler(sig: libc::c_int) {
    let msg: &[u8] = match sig {
        libc::SIGSEGV => b"CRASH: signal SIGSEGV\n",
        libc::SIGILL => b"CRASH: signal SIGILL\n",
        libc::SIGBUS => b"CRASH: signal SIGBUS\n",
        libc::SIGFPE => b"CRASH: signal SIGFPE\n",
        _ => b"CRASH: fatal signal\n",
    };

    unsafe {
        libc::write(libc::STDERR_FILENO, msg.as_ptr().cast(), msg.len());
        libc::signal(sig, libc::SIG_DFL);
        libc::raise(sig);
    }
}

fn print_usage() {
    println!("mbv {}", env!("CARGO_PKG_VERSION"));
    println!("Usage: mbv [OPTIONS]");
    println!();
    println!("Options:");
    println!("  -q                        Stop the running Player owner (bare mbv, or the local");
    println!("                             daemon in stay-alive mode).");
    println!("      --connect-daemon <endpoint>");
    println!("                             Attach as a client to a running mbvd daemon at");
    println!("                             <endpoint> instead of owning a local Player.");
    println!("  -V, --version              Print the version and exit.");
    println!("  -h, --help                 Print this help message and exit.");
}

fn main() {
    install_panic_hook();
    install_signal_handlers();

    let args: Vec<String> = std::env::args().skip(1).collect();

    if has_flag(&args, "-h") || has_flag(&args, "--help") {
        print_usage();
        return;
    }

    // Hidden local-daemon self-spawn subcommand (T2, design.md decision 1):
    // `mbv --__local-daemon` re-execs itself to run the local daemon in this
    // process and never returns. Checked early, before any other CLI
    // parsing.
    if has_flag(&args, "--__local-daemon") {
        local_daemon::run_local_daemon_main();
    }

    let cli_daemon_endpoint = match connect_daemon_arg(&args) {
        Ok(endpoint) => endpoint,
        Err(e) => {
            eprintln!("{e}");
            std::process::exit(1);
        }
    };

    if has_flag(&args, "--version") || has_flag(&args, "-V") {
        println!("mbv {}", env!("CARGO_PKG_VERSION"));
        return;
    }

    // `mbv -q`: stop the running Player owner (bare `mbv`, or the local
    // daemon in stay-alive mode) (ADR 0006). Reads the PID out of the
    // single-instance lock file and SIGTERMs it for a graceful,
    // non-interactive shutdown -- the tray's Quit item does the same thing.
    if has_flag(&args, "-q") {
        let lock = single_instance::lock_path();
        match single_instance::read_pid(&lock) {
            Some(pid) => {
                let ok = unsafe { libc::kill(pid as libc::pid_t, libc::SIGTERM) } == 0;
                if ok {
                    println!("mbv: quit signal sent (pid {pid})");
                } else {
                    eprintln!(
                        "mbv: failed to signal pid {pid}: {}",
                        std::io::Error::last_os_error()
                    );
                    std::process::exit(1);
                }
            }
            None => {
                eprintln!(
                    "mbv: no running instance found; if one just started, try again in a moment"
                );
                std::process::exit(1);
            }
        }
        return;
    }

    // Reject the legacy `-d` argument before startup side effects.
    // The `-d` flag has been removed; users should enable `stay_alive` in
    // config or the settings overlay instead.
    if has_flag(&args, "-d") {
        eprintln!("mbv: the `-d` flag has been removed.");
        eprintln!("mbv: to keep the local daemon running after quit, enable `stay_alive` in config or the settings overlay.");
        std::process::exit(1);
    }

    applog::init(
        config::is_system_instance(),
        Some(state_dir().join("mbv.log")),
    );

    if let Err(e) = config::migrate_legacy_emby_token() {
        eprintln!("mbv: Emby setup migration failed: {e}");
    }
    let config = match load_config() {
        Ok(c) => c,
        Err(e) => {
            eprintln!("{e}");
            std::process::exit(1);
        }
    };
    log::info!(target: "startup", "{}", config_diagnostic_summary(&config));
    let explicit_daemon_endpoint = cli_daemon_endpoint
        .or_else(|| {
            let endpoint = config.daemon_client_endpoint.trim();
            (!endpoint.is_empty()).then(|| endpoint.to_string())
        })
        .map(|endpoint| {
            remote_player::DaemonEndpoint::parse(&endpoint).unwrap_or_else(|e| {
                eprintln!("mbv: invalid daemon endpoint {endpoint:?}: {e}");
                std::process::exit(1);
            })
        });

    log::info!(target: "startup", "mbv starting");

    // Explicit endpoint (`--connect-daemon` / config `daemon_client_endpoint`)
    // always wins: a thin client to `mbvd`, owning no Player and taking no
    // flock. Network/mbvd behavior is unchanged by stay-alive (issue #156).
    if let Some(endpoint) = explicit_daemon_endpoint {
        let client = cached_emby_client(&config);
        log::info!(target: "startup", "connecting to explicit daemon endpoint {endpoint}");
        println!("Connecting to daemon at {endpoint}...");
        match remote_player::RemotePlayer::connect_endpoint(&endpoint) {
            Ok((remote, player_rx)) => {
                log::info!(target: "startup", "daemon endpoint connected");
                run_remote_app(client, remote, player_rx, endpoint, config.clone());
                return;
            }
            Err(e) => {
                eprintln!("mbv: failed to connect to daemon endpoint {endpoint}: {e}");
                std::process::exit(1);
            }
        }
    }

    // Single-instance resolution (ADR 0006): advisory flock + control-socket
    // connectability. Independent of stay-alive; always on.
    let lock_path = single_instance::lock_path();
    let socket_path = single_instance::socket_path();

    match single_instance::resolve(&socket_path, &lock_path) {
        Ok(single_instance::Resolution::Attach) => {
            // A live local daemon exists: attach as a client alongside any
            // others already attached. Clients take no lock -- that is what
            // permits any number of them.
            log::info!(target: "startup", "local daemon detected; attaching");
            let client = cached_emby_client(&config);
            match remote_player::RemotePlayer::connect_endpoint(
                &remote_player::DaemonEndpoint::Local,
            ) {
                Ok((remote, player_rx)) => {
                    run_remote_app(
                        client,
                        remote,
                        player_rx,
                        remote_player::DaemonEndpoint::Local,
                        config.clone(),
                    );
                }
                Err(e) => {
                    eprintln!("mbv: failed to attach to local daemon: {e}");
                    std::process::exit(1);
                }
            }
        }
        Ok(single_instance::Resolution::Refuse) => {
            eprintln!("mbv: another mbv instance already owns playback in a foreground terminal.");
            match single_instance::read_pid(&lock_path) {
                Some(pid) => eprintln!("mbv: that instance's PID is {pid} (per {lock_path:?})."),
                None => {
                    eprintln!("mbv: could not determine that instance's PID from {lock_path:?}.")
                }
            }
            eprintln!(
                "mbv: only one process can own playback at a time. Close it, stop it with \
                 `mbv -q`, or enable `stay_alive` in config to run several terminals against a local daemon."
            );
            std::process::exit(1);
        }
        Ok(single_instance::Resolution::Fresh(mut guard)) => {
            let stay_alive = config.stay_alive;

            if stay_alive {
                let client = cached_emby_client(&config);
                // This process was just a liveness probe: release the lock
                // immediately (the local daemon reacquires it for real,
                // becoming the actual Player-owning process) and attach to
                // it as a client ourselves.
                drop(guard);
                if let Err(e) = local_daemon::spawn_detached(&socket_path.to_string_lossy()) {
                    eprintln!("mbv: failed to start local daemon: {e}");
                    std::process::exit(1);
                }
                match remote_player::RemotePlayer::connect_endpoint(
                    &remote_player::DaemonEndpoint::Local,
                ) {
                    Ok((remote, player_rx)) => {
                        run_remote_app(
                            client,
                            remote,
                            player_rx,
                            remote_player::DaemonEndpoint::Local,
                            config.clone(),
                        );
                        return;
                    }
                    Err(e) => {
                        eprintln!("mbv: failed to attach to local daemon: {e}");
                        std::process::exit(1);
                    }
                }
            }

            if let Err(e) = guard.write_pid() {
                log::warn!(target: "startup", "failed to write pid into lock file: {e}");
            }
            if let Err(e) = Model::new(App::new_independent(config)).run() {
                eprintln!("Error: {e}");
                std::process::exit(1);
            }
            // `guard` drops here (end of scope) at real process exit,
            // releasing the flock -- also happens automatically on any
            // process death (ADR 0006).
        }
        Err(e) => {
            eprintln!("mbv: single-instance check failed: {e}");
            std::process::exit(1);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn connect_daemon_arg_accepts_split_and_equals_forms() {
        assert_eq!(
            connect_daemon_arg(&["--connect-daemon".into(), "local".into()]).unwrap(),
            Some("local".to_string())
        );
        assert_eq!(
            connect_daemon_arg(&["--connect-daemon=unix:///tmp/mbv.sock".into()]).unwrap(),
            Some("unix:///tmp/mbv.sock".to_string())
        );
    }

    #[test]
    fn connect_daemon_arg_requires_value() {
        assert!(connect_daemon_arg(&["--connect-daemon".into()]).is_err());
    }

    #[test]
    fn has_flag_matches_exact_flag() {
        assert!(has_flag(
            &["-a".into(), "--audio-only".into()],
            "--audio-only"
        ));
        assert!(!has_flag(
            &["--audio-only=false".into(), "--audio".into()],
            "--audio-only"
        ));
    }

    /// A client missing `user_id` builds `/Users//Views`-shaped paths --
    /// Emby throws on the empty GUID segment rather than rejecting cleanly.
    /// `construct.rs` marks a cached client `Ready` immediately without
    /// re-authenticating, so a blank `user_id` here reaches real requests.
    #[test]
    fn cached_emby_client_carries_user_id_from_setup() {
        let _state_dir = mbv_core::config::TestStateDirGuard::new();
        mbv_core::config::save_service_secret(mbv_core::config::ServiceKind::Emby, "tok").unwrap();
        let config = config::Config {
            emby_setup: Some(mbv_core::config::EmbySetup::new(
                "http://emby.example:8096",
                "the-user-id",
            )),
            ..config::Config::default()
        };
        let client = cached_emby_client(&config).expect("cached client");
        assert_eq!(client.user_id, "the-user-id");
    }

    #[test]
    fn config_diagnostic_summary_is_sorted_and_sanitized() {
        let mut config = config::Config {
            auto_reconnect: true,
            password: "secret-token".to_string(),
            ..config::Config::default()
        };
        config
            .library_routes
            .insert("music".to_string(), "tcp://192.0.2.10:17831".to_string());
        config.library_routes.insert(
            "audiobooks".to_string(),
            "tcp://192.0.2.11:17831".to_string(),
        );
        let summary = config_diagnostic_summary(&config);
        assert_eq!(summary, "config loaded: auto_reconnect=true library_routes=2 entries=[audiobooks=tcp://192.0.2.11:17831, music=tcp://192.0.2.10:17831]");
        assert!(!summary.contains("secret-token"));
    }
}
