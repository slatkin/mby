use mbv_core::{applog, config, daemon};
use mimalloc::MiMalloc;
use std::io::{self, BufRead, BufReader, IsTerminal, Write};
use std::os::unix::net::UnixStream;
use std::time::Duration;

#[global_allocator]
static GLOBAL: MiMalloc = MiMalloc;

fn print_usage() {
    eprintln!("Usage: mbvd [--audio-only] [-q|--quit] [--export-shared-data] [--connect emby] [--connect abs] [--disconnect abs] [--version]");
}

fn daemon_running() -> bool {
    let Ok(s) = std::fs::read_to_string(daemon::pid_file()) else {
        return false;
    };
    let Ok(pid) = s.trim().parse::<u32>() else {
        return false;
    };
    std::path::Path::new(&format!("/proc/{pid}")).exists()
}

fn stop_daemon() -> Result<String, String> {
    let path = daemon::pid_file();
    let pid = std::fs::read_to_string(&path)
        .map_err(|_| "mbvd: no daemon running".to_string())?
        .trim()
        .to_string();
    let ok = std::process::Command::new("kill")
        .arg(&pid)
        .status()
        .map(|s| s.success())
        .unwrap_or(false);
    if ok {
        let _ = std::fs::remove_file(&path);
        Ok(format!("mbvd: daemon stopped (pid {pid})"))
    } else {
        Err(format!("mbvd: failed to stop daemon (pid {pid})"))
    }
}

#[derive(Debug, PartialEq, Eq)]
enum Action {
    Serve { audio_only: bool },
    ConnectEmby,
    ConnectAbs,
    DisconnectAbs,
    Quit,
    Export,
    Help,
    Version,
}

fn parse_action(args: &[String]) -> Result<Action, String> {
    let mut audio_only = false;
    let mut action = None;
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--audio-only" => audio_only = true,
            "--help" | "-h" => select_action(&mut action, Action::Help)?,
            "--version" | "-V" => select_action(&mut action, Action::Version)?,
            "--quit" | "-q" => select_action(&mut action, Action::Quit)?,
            "--export-shared-data" => select_action(&mut action, Action::Export)?,
            "--connect" => {
                i += 1;
                let Some(service) = args.get(i) else {
                    return Err("mbvd: --connect requires a Service (supported: emby, abs)".into());
                };
                match service.as_str() {
                    "emby" => select_action(&mut action, Action::ConnectEmby)?,
                    "abs" => select_action(&mut action, Action::ConnectAbs)?,
                    _ => {
                        return Err(
                            "mbvd: unsupported Service; supported Services: emby, abs".into()
                        )
                    }
                }
            }
            "--disconnect" => {
                i += 1;
                let Some(service) = args.get(i) else {
                    return Err("mbvd: --disconnect requires a Service (supported: abs)".into());
                };
                if service != "abs" {
                    return Err("mbvd: unsupported Service; supported Services: abs".into());
                }
                select_action(&mut action, Action::DisconnectAbs)?;
            }
            arg => return Err(format!("mbvd: unknown argument {arg:?}")),
        }
        i += 1;
    }
    if audio_only
        && matches!(
            action,
            Some(Action::ConnectEmby | Action::ConnectAbs | Action::DisconnectAbs)
        )
    {
        return Err("mbvd: service administration cannot be combined with daemon selectors".into());
    }
    Ok(action.unwrap_or(Action::Serve { audio_only }))
}

fn select_action(action: &mut Option<Action>, next: Action) -> Result<(), String> {
    if action.is_some() {
        return Err("mbvd: action selectors are mutually exclusive".into());
    }
    *action = Some(next);
    Ok(())
}

fn interactive_terminal() -> bool {
    io::stdin().is_terminal() && io::stdout().is_terminal()
}

fn prompt(label: &str) -> Result<String, String> {
    print!("{label}: ");
    io::stdout()
        .flush()
        .map_err(|_| "mbvd: prompt failed".to_string())?;
    let mut value = String::new();
    io::stdin()
        .read_line(&mut value)
        .map_err(|_| "mbvd: input failed".to_string())?;
    Ok(value.trim().to_string())
}

fn prompt_secret(label: &str) -> Result<String, String> {
    let stdin = io::stdin();
    let mut termios =
        nix::sys::termios::tcgetattr(&stdin).map_err(|_| "mbvd: prompt failed".to_string())?;
    let original = termios.clone();
    termios
        .local_flags
        .remove(nix::sys::termios::LocalFlags::ECHO);
    nix::sys::termios::tcsetattr(&stdin, nix::sys::termios::SetArg::TCSADRAIN, &termios)
        .map_err(|_| "mbvd: prompt failed".to_string())?;
    let result = prompt(label);
    let _ = nix::sys::termios::tcsetattr(&stdin, nix::sys::termios::SetArg::TCSADRAIN, &original);
    println!();
    result
}

fn administration_lock(stem: &str) -> Result<nix::fcntl::Flock<std::fs::File>, String> {
    let path = config::data_dir_system_or_local().join(format!("{stem}-connect.lock"));
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|_| "mbvd: cannot create administration lock".to_string())?;
    }
    let file = std::fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(path)
        .map_err(|_| "mbvd: cannot open administration lock".to_string())?;
    nix::fcntl::Flock::lock(file, nix::fcntl::FlockArg::LockExclusiveNonblock)
        .map_err(|_| format!("mbvd: another {stem} administration command is running"))
}

fn classified_auth_error(error: &str) -> String {
    let lower = error.to_ascii_lowercase();
    if lower.contains("401") || lower.contains("403") || lower.contains("authentication") {
        "mbvd: Emby authentication rejected".into()
    } else {
        "mbvd: Emby server unavailable or returned an invalid authentication response".into()
    }
}

fn connect_emby() -> Result<(), String> {
    if !interactive_terminal() {
        return Err("mbvd: --connect emby requires an interactive terminal".into());
    }
    // The packaged command always uses the daemon's system-instance paths.
    std::env::set_var("MBV_SYSTEM", "1");
    let _lock = administration_lock("emby")?;
    let server_url = prompt("Emby server URL")?;
    let username = prompt("Username")?;
    let password = prompt_secret("Password")?;
    let config = config::load_config()
        .map_err(|_| "mbvd: could not load owner configuration".to_string())?;
    let existing = config.emby_setup.clone();
    let client = mbv_core::api::EmbyClient::new(config);
    let exchange = client
        .exchange_credentials_bounded(&server_url, &username, &password, Duration::from_secs(10))
        .map_err(|error| classified_auth_error(&error))?;
    let mut setup = config::EmbySetup::new(exchange.server_url.clone(), exchange.user_id);
    setup.revision = match existing.as_ref() {
        None => 1,
        Some(old) => old
            .revision
            .checked_add(1)
            .ok_or_else(|| "mbvd: Emby setup revision exhausted".to_string())?,
    };
    let same_server = existing
        .as_ref()
        .is_some_and(|old| old.server_url == setup.server_url);
    if existing.is_none() || same_server {
        config::persist_emby_setup_and_secret(&setup, &exchange.token)
            .map_err(|_| "mbvd: could not persist Emby setup".to_string())?;
    } else {
        config::replace_emby_setup_and_secret(&setup, &exchange.token)
            .map_err(|_| "mbvd: could not replace Emby setup".to_string())?;
    }
    if daemon_running() {
        reconcile_running_owner(config::ServiceKind::Emby, setup.revision)?;
        println!(
            "mbvd: Emby setup committed and active for {}",
            setup.server_url
        );
    } else {
        println!(
            "mbvd: Emby setup committed for {}; loaded on next startup",
            setup.server_url
        );
    }
    Ok(())
}

fn classified_abs_error(error: &mbv_core::audiobookshelf::AudiobookshelfError) -> String {
    use mbv_core::audiobookshelf::AudiobookshelfFailureClass;
    match error.class {
        AudiobookshelfFailureClass::AuthenticationRejected => {
            "mbvd: Audiobookshelf authentication rejected".into()
        }
        _ => "mbvd: Audiobookshelf server unavailable or returned an invalid response".into(),
    }
}

fn clear_audiobookshelf_owned_state() -> Result<(), String> {
    match config::load_queue_state() {
        Some(state) if !state.items.is_empty() => {
            config::save_queue_state(&state.without_audiobookshelf())
        }
        _ => config::clear_queue_state(),
    }
}

fn connect_abs() -> Result<(), String> {
    if !interactive_terminal() {
        return Err("mbvd: --connect abs requires an interactive terminal".into());
    }
    // The packaged command always uses the daemon's system-instance paths.
    std::env::set_var("MBV_SYSTEM", "1");
    let _lock = administration_lock("abs")?;
    let server_url = prompt("Audiobookshelf server URL")?;
    let api_key = prompt_secret("Audiobookshelf API key")?;
    let config = config::load_config()
        .map_err(|_| "mbvd: could not load owner configuration".to_string())?;
    let existing = config.audiobookshelf_setup.clone();
    let old_queue = config::load_queue_state();
    let validated = mbv_core::audiobookshelf::AudiobookshelfClient::validate_setup_bounded(
        &server_url,
        &api_key,
        Duration::from_secs(10),
    )
    .map_err(|error| classified_abs_error(&error))?;
    let (setup, _user, api_key) = validated.into_parts();
    let same_server = existing
        .as_ref()
        .is_some_and(|old| old.server_url == setup.server_url);
    let revision = if existing.is_none() || same_server {
        config::persist_audiobookshelf_setup_and_secret(&setup, &api_key)
            .map_err(|_| "mbvd: could not persist Audiobookshelf setup".to_string())?
    } else {
        config::replace_audiobookshelf_setup_and_secret(
            &setup,
            &api_key,
            clear_audiobookshelf_owned_state,
            move || {
                if let Some(old) = old_queue.as_ref() {
                    let _ = config::save_queue_state(old);
                }
            },
        )
        .map_err(|_| "mbvd: could not replace Audiobookshelf setup".to_string())?
    };
    if daemon_running() {
        reconcile_running_owner(config::ServiceKind::Audiobookshelf, revision)?;
        println!(
            "mbvd: Audiobookshelf setup committed and active for {}",
            setup.server_url
        );
    } else {
        println!(
            "mbvd: Audiobookshelf setup committed for {}; loaded on next startup",
            setup.server_url
        );
    }
    Ok(())
}

fn disconnect_abs() -> Result<(), String> {
    if !interactive_terminal() {
        return Err("mbvd: --disconnect abs requires an interactive terminal".into());
    }
    std::env::set_var("MBV_SYSTEM", "1");
    let _lock = administration_lock("abs")?;
    let config = config::load_config()
        .map_err(|_| "mbvd: could not load owner configuration".to_string())?;
    let was_installed = config.audiobookshelf_setup.is_some();
    config::remove_audiobookshelf_setup_and_secret_with_owned_state(
        clear_audiobookshelf_owned_state,
        || {},
    )
    .map_err(|_| "mbvd: could not remove Audiobookshelf setup".to_string())?;
    if was_installed {
        println!("mbvd: Audiobookshelf credential removed");
    } else {
        println!("mbvd: no Audiobookshelf setup was installed");
    }
    if daemon_running() {
        // A revision of 0 signals removal: the running owner rereads its own
        // storage, sees no setup, and drops its context.
        if let Err(error) = reconcile_running_owner(config::ServiceKind::Audiobookshelf, 0) {
            return Err(format!(
                "{error}; the running process may retain the deleted key in memory"
            ));
        }
        println!("mbvd: Audiobookshelf setup removed and active");
    } else {
        println!("mbvd: Audiobookshelf setup removed; cleared on next startup");
    }
    Ok(())
}

fn reconcile_running_owner(kind: config::ServiceKind, revision: u64) -> Result<(), String> {
    let stream = UnixStream::connect(config::control_socket_path())
        .map_err(|_| "mbvd: restart required (packaged daemon ctrl unavailable)".to_string())?;
    stream
        .set_read_timeout(Some(Duration::from_secs(6)))
        .map_err(|_| "mbvd: restart required (cannot read packaged daemon ctrl)".to_string())?;
    let mut writer = stream
        .try_clone()
        .map_err(|_| "mbvd: restart required (cannot write packaged daemon ctrl)".to_string())?;
    let mut reader = BufReader::new(stream);
    let mut line = String::new();
    reader
        .read_line(&mut line)
        .map_err(|_| "mbvd: restart required (packaged daemon did not acknowledge)".to_string())?;
    match serde_json::from_str::<mbv_core::ctrl::CtrlEvent>(&line) {
        Ok(mbv_core::ctrl::CtrlEvent::Hello(hello)) => hello
            .validate_peer()
            .map_err(|_| "mbvd: restart required (ctrl protocol mismatch)".to_string())?,
        _ => return Err("mbvd: restart required (invalid packaged daemon ctrl hello)".into()),
    }
    let hello = serde_json::to_string(&mbv_core::ctrl::CtrlCmd::Hello(
        mbv_core::ctrl::CtrlHello::current(),
    ))
    .map_err(|_| "mbvd: restart required (cannot serialize ctrl hello)".to_string())?;
    writeln!(writer, "{hello}")
        .and_then(|_| {
            serde_json::to_string(&mbv_core::ctrl::CtrlCmd::ApplyServiceSetup { kind, revision })
                .map_err(|_| io::Error::other("cannot serialize setup request"))
                .and_then(|request| writeln!(writer, "{request}"))
        })
        .map_err(|_| "mbvd: restart required (cannot send setup request)".to_string())?;
    writer
        .flush()
        .map_err(|_| "mbvd: restart required (cannot flush setup request)".to_string())?;

    for next in reader.lines() {
        let line = next.map_err(|_| {
            "mbvd: restart required (setup acknowledgement unavailable)".to_string()
        })?;
        let event = serde_json::from_str::<mbv_core::ctrl::CtrlEvent>(&line)
            .map_err(|_| "mbvd: restart required (invalid setup acknowledgement)".to_string())?;
        match event {
            mbv_core::ctrl::CtrlEvent::ServiceSetupApplied { .. } => return Ok(()),
            mbv_core::ctrl::CtrlEvent::ServiceSetupRejected { reason, .. } => {
                return Err(format!(
                    "mbvd: restart required (live setup rejected: {reason:?})"
                ))
            }
            _ => {}
        }
    }
    Err("mbvd: restart required (setup acknowledgement unavailable)".into())
}

fn log_path() -> std::path::PathBuf {
    config::data_dir_system_or_local().join("mbv.log")
}

fn install_panic_hook() {
    std::panic::set_hook(Box::new(|info| {
        let msg = format!("PANIC: {info}");
        eprintln!("{msg}");
        log::error!(target: "crash", "{msg}");
    }));
}

fn install_signal_handlers() {
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

fn run() -> Result<(), String> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let action = match parse_action(&args) {
        Ok(action) => action,
        Err(error) => {
            print_usage();
            return Err(error);
        }
    };
    let audio_only = match action {
        Action::Help => {
            print_usage();
            return Ok(());
        }
        Action::Version => {
            println!("mbvd {}", env!("CARGO_PKG_VERSION"));
            return Ok(());
        }
        Action::ConnectEmby => return connect_emby(),
        Action::ConnectAbs => return connect_abs(),
        Action::DisconnectAbs => return disconnect_abs(),
        Action::Quit => {
            println!("{}", stop_daemon()?);
            return Ok(());
        }
        Action::Export => {
            if daemon_running() {
                return Err("mbvd: a daemon is already running".to_string());
            }
            let db = mbv_core::shared_store::open_existing_shared_db()?;
            println!("{}", mbv_core::shared_worker::export_json_pretty(&db)?);
            return Ok(());
        }
        Action::Serve { audio_only } => audio_only,
    };
    if daemon_running() {
        return Err("mbvd: a daemon is already running".to_string());
    }

    let config = config::load_config()?;
    applog::init(config::is_system_instance(), Some(log_path()));
    log::info!(target: "startup", "mbvd starting");

    daemon::run_with_options(
        daemon::DaemonStartupContext::new(config, daemon::DaemonRole::Packaged),
        audio_only,
        daemon::DaemonRuntimeHooks {
            on_player_ready: Box::new(|_| {}),
            // Deliberately a stub: mbvd runs as a system service with no
            // user session, so there's no tray to spawn into.
            on_tray_ready: Box::new(|_| None),
        },
    );
}

fn main() {
    install_panic_hook();
    install_signal_handlers();
    if let Err(e) = run() {
        eprintln!("{e}");
        std::process::exit(exit_code_for_error(&e));
    }
}

fn exit_code_for_error(error: &str) -> i32 {
    if error.contains("restart required") {
        3
    } else if error.contains("requires an interactive terminal")
        || error.contains("unsupported Service")
        || error.contains("action selectors")
        || error.contains("unknown argument")
        || error.contains("requires a Service")
        || error.contains("daemon selectors")
    {
        2
    } else {
        1
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    include!("tests.rs");
}
