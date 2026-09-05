use super::*;
use crate::app::tests::*;
use mbv_core::remote_player::{DaemonEndpoint, RemotePlayer};
use std::io::Read as _;
use std::net::TcpListener;

pub(super) fn stub_endpoint() -> DaemonEndpoint {
    DaemonEndpoint::Tcp("127.0.0.1:0".parse().unwrap())
}

pub(super) fn spawn_stub_daemon() -> (
    std::net::SocketAddr,
    std::thread::JoinHandle<std::net::TcpStream>,
) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    let handle = std::thread::spawn(move || {
        let (stream, _) = listener.accept().unwrap();
        crate::app::tests::run_stub_daemon_handshake(stream)
    });
    (addr, handle)
}

#[test]
fn remote_slot_state_is_off_for_local_only_app() {
    let app = make_app_stub();
    assert_eq!(app.remote_slot_state(), RemoteSlotState::Off);
    assert!(!app.can_disconnect_remote());
}

#[test]
fn app_stub_starts_with_no_active_library_route() {
    let app = make_app_stub();
    assert!(app.active_route.is_none());
    assert!(app.library_routes.is_empty());
    assert!(app.library_route_cache.is_empty());
}

#[test]
fn remote_slot_state_is_attached_session_when_connected_to_remote_session() {
    let mut app = make_app_stub();
    app.connected_session_id = Some("session-1".into());

    assert_eq!(app.remote_slot_state(), RemoteSlotState::AttachedSession);
    assert!(app.can_disconnect_remote());
}

#[test]
fn remote_slot_state_direct_remote_display_does_not_imply_sessions_panel_disconnect() {
    let app = make_remote_app_stub(make_items(2), make_items(3));

    assert_eq!(app.remote_slot_state(), RemoteSlotState::DirectRemote);
    assert!(!app.can_disconnect_remote());
}

#[test]
fn direct_remote_connect_keeps_local_scope_when_remote_queue_is_empty() {
    let mut app = make_app_stub();
    app.player_tab
        .set_items(make_items(2), app.player_tab.queue_cursor);
    let (remote, remote_rx) = mbv_core::remote_player::RemotePlayer::stub(Vec::new(), 0);
    let sess = make_session("remote-host", "mbv");

    app.switch_to_direct_remote(&sess, remote, remote_rx, &stub_endpoint());

    assert_eq!(app.queue_scope, QueueScope::Local);
    assert_eq!(app.viewed_queue_scope(), QueueScope::Local);
    assert!(app
        .remote_player_tab
        .as_ref()
        .unwrap()
        .emby_items()
        .is_empty());
    assert_eq!(app.player_tab.emby_items().len(), 2);
}

#[test]
fn direct_remote_connect_switches_to_remote_scope_when_remote_queue_has_items() {
    let mut app = make_app_stub();
    app.player_tab
        .set_items(make_items(2), app.player_tab.queue_cursor);
    let remote_items = make_items(1);
    let (remote, remote_rx) = mbv_core::remote_player::RemotePlayer::stub(remote_items.clone(), 0);
    let sess = make_session("remote-host", "mbv");

    app.switch_to_direct_remote(&sess, remote, remote_rx, &stub_endpoint());

    assert_eq!(app.queue_scope, QueueScope::Remote);
    assert_eq!(app.viewed_queue_scope(), QueueScope::Remote);
    assert_eq!(
        app.remote_player_tab.as_ref().unwrap().emby_items()[0].id,
        remote_items[0].id
    );
    assert_eq!(app.player_tab.emby_items().len(), 2);
}

#[test]
fn switch_to_direct_remote_rebinds_mpris_to_the_new_remote_status() {
    // #175: before `switch_to_direct_remote` called `mpris::rebind`,
    // MPRIS stayed wired to whatever `PlayerStatus` was live when the
    // D-Bus service was first registered (almost always the initial
    // local `Player`'s), so local desktop MPRIS never picked up a
    // remote daemon's playback after a mid-session "Direct Remote"
    // takeover -- exactly the bug this issue reports. This drives the
    // real `App` method (not just `mpris::rebind` in isolation) to
    // prove the wiring at the call site is actually in place.
    let mut app = make_app_stub();
    let local_status = app.player.status.clone();
    app.mpris = Some(crate::mpris::test_handle(
        local_status.clone(),
        |_| {},
        None,
    ));

    let remote_items = make_items(1);
    let (remote, remote_rx) = mbv_core::remote_player::RemotePlayer::stub(remote_items, 0);
    let remote_status = remote.status.clone();
    let sess = make_session("remote-host", "mbv");

    app.switch_to_direct_remote(&sess, remote, remote_rx, &stub_endpoint());

    let handle = app.mpris.as_ref().expect("mpris handle still present");
    let bound_status = crate::mpris::test_status(handle);
    assert!(
        Arc::ptr_eq(&bound_status, &remote_status),
        "switch_to_direct_remote must rebind MPRIS to the new remote's status"
    );
    assert!(!Arc::ptr_eq(&bound_status, &local_status));
}

#[test]
fn switch_to_direct_remote_disconnects_the_previous_remote_on_a_remote_to_remote_swap() {
    // Same #233 regression, but for the Sessions-panel direct-remote
    // #233: second Direct Remote upgrade must disconnect the old.
    let (addr_a, daemon_a) = spawn_stub_daemon();
    let (addr_b, daemon_b) = spawn_stub_daemon();

    let mut app = make_app_stub();
    let sess_a = make_session("daemon-a", "mbv");
    let (remote_a, remote_a_rx) =
        RemotePlayer::connect_endpoint(&DaemonEndpoint::Tcp(addr_a)).unwrap();
    app.switch_to_direct_remote(&sess_a, remote_a, remote_a_rx, &stub_endpoint());

    let sess_b = make_session("daemon-b", "mbv");
    let (remote_b, remote_b_rx) =
        RemotePlayer::connect_endpoint(&DaemonEndpoint::Tcp(addr_b)).unwrap();
    app.switch_to_direct_remote(&sess_b, remote_b, remote_b_rx, &stub_endpoint());

    let mut daemon_a_stream = daemon_a.join().unwrap();
    daemon_a_stream
        .set_read_timeout(Some(Duration::from_secs(2)))
        .unwrap();
    let mut pending_control_bytes = Vec::new();
    daemon_a_stream
        .read_to_end(&mut pending_control_bytes)
        .expect("old direct-remote client socket must be shut down after the swap");

    drop(daemon_b);
    let _ = addr_b;
}

#[test]
fn switch_to_library_route_sets_active_route_and_suspends_local() {
    let mut app = make_app_stub();
    let (remote, remote_rx) = mbv_core::remote_player::RemotePlayer::stub(make_items(1), 0);

    app.switch_to_library_route("music", remote, remote_rx, &stub_endpoint());

    assert_eq!(app.active_route.as_deref(), Some("music"));
    assert!(app.player.is_remote());
    assert!(app.suspended_local.is_some());
    assert!(app.remote_player_tab.is_some());
    assert!(app.connected_session_id.is_none());
    assert!(app.direct_remote_label.is_none());
}

#[test]
fn route_owned_transport_is_not_sessions_panel_disconnectable() {
    let mut app = make_app_stub();
    let (remote, remote_rx) = mbv_core::remote_player::RemotePlayer::stub(make_items(1), 0);

    app.switch_to_library_route("music", remote, remote_rx, &stub_endpoint());
    app.status.clear();

    assert_eq!(app.remote_slot_state(), RemoteSlotState::DirectRemote);
    assert!(!app.can_disconnect_remote());

    app.disconnect_remote();

    assert_eq!(app.active_route.as_deref(), Some("music"));
    assert!(app.player.is_remote());
    assert!(app.suspended_local.is_some());
    assert!(app.remote_player_tab.is_some());
    assert_eq!(app.status, "No session selected");
}

#[test]
fn switch_to_library_route_sets_remote_queue_scope_when_daemon_has_items() {
    let mut app = make_app_stub();
    let (remote, remote_rx) = mbv_core::remote_player::RemotePlayer::stub(make_items(2), 0);

    app.switch_to_library_route("music", remote, remote_rx, &stub_endpoint());

    assert!(app.has_direct_remote_queue());
}

#[test]
fn switch_to_library_route_disconnects_the_previous_remote_on_a_route_to_route_swap() {
    // #233: route-to-route swap must disconnect the old RemotePlayer's socket.
    let (addr_a, daemon_a) = spawn_stub_daemon();
    let (addr_b, daemon_b) = spawn_stub_daemon();

    let mut app = make_app_stub();
    let (remote_a, remote_a_rx) =
        RemotePlayer::connect_endpoint(&DaemonEndpoint::Tcp(addr_a)).unwrap();
    app.switch_to_library_route("music", remote_a, remote_a_rx, &stub_endpoint());
    assert!(!app.player.is_remote_disconnected());

    let (remote_b, remote_b_rx) =
        RemotePlayer::connect_endpoint(&DaemonEndpoint::Tcp(addr_b)).unwrap();
    app.switch_to_library_route("movies", remote_b, remote_b_rx, &stub_endpoint());

    let mut daemon_a_stream = daemon_a.join().unwrap();
    daemon_a_stream
        .set_read_timeout(Some(Duration::from_secs(2)))
        .unwrap();
    let mut pending_control_bytes = Vec::new();
    daemon_a_stream
        .read_to_end(&mut pending_control_bytes)
        .expect("old library route's client socket must be shut down after the swap");

    drop(daemon_b);
    let _ = addr_b;
}

#[test]
fn restore_local_mode_rebinds_mpris_back_to_the_suspended_local_status() {
    // #175 follow-through: after a Direct Remote takeover ends (however
    // it ends -- disconnect, user action, etc.), MPRIS must follow
    // playback back to the restored local `Player`, not stay wired to
    // the now-defunct remote session.
    let mut app = make_app_stub();
    let local_status = app.player.status.clone();
    app.mpris = Some(crate::mpris::test_handle(
        local_status.clone(),
        |_| {},
        None,
    ));

    let remote_items = make_items(1);
    let (remote, remote_rx) = mbv_core::remote_player::RemotePlayer::stub(remote_items, 0);
    let remote_status = remote.status.clone();
    let sess = make_session("remote-host", "mbv");
    app.switch_to_direct_remote(&sess, remote, remote_rx, &stub_endpoint());

    app.restore_local_mode("test: ending direct remote session");

    let handle = app.mpris.as_ref().expect("mpris handle still present");
    let bound_status = crate::mpris::test_status(handle);
    assert!(
        Arc::ptr_eq(&bound_status, &local_status),
        "restore_local_mode must rebind MPRIS back to the restored local status"
    );
    assert!(!Arc::ptr_eq(&bound_status, &remote_status));
}

#[test]
fn restore_local_mode_clears_active_route() {
    let mut app = make_app_stub();
    let (remote, remote_rx) = mbv_core::remote_player::RemotePlayer::stub(make_items(1), 0);
    app.switch_to_library_route("music", remote, remote_rx, &stub_endpoint());
    assert_eq!(app.active_route.as_deref(), Some("music"));

    app.restore_local_mode("Local playback restored");

    assert!(app.active_route.is_none());
    assert!(!app.player.is_remote());
}

#[test]
fn restore_local_mode_disconnects_the_remote_before_restoring_local() {
    // #233: restore_local_mode must disconnect the old remote before restoring local.
    let (addr, daemon) = spawn_stub_daemon();

    let mut app = make_app_stub();
    let (remote, remote_rx) = RemotePlayer::connect_endpoint(&DaemonEndpoint::Tcp(addr)).unwrap();
    app.switch_to_library_route("music", remote, remote_rx, &stub_endpoint());
    assert!(!app.player.is_remote_disconnected());

    app.restore_local_mode("test: ending library route session");

    // The client may still have in-flight protocol traffic queued ahead of
    // the close (e.g. a trailing status message), so drain reads until the
    // socket actually reaches EOF rather than asserting on a single read.
    let mut daemon_stream = daemon.join().unwrap();
    daemon_stream
        .set_read_timeout(Some(Duration::from_millis(200)))
        .unwrap();
    let deadline = std::time::Instant::now() + Duration::from_secs(2);
    let mut buf = [0u8; 256];
    loop {
        match daemon_stream.read(&mut buf) {
            Ok(0) => break,
            Ok(_) => {}
            Err(e)
                if matches!(
                    e.kind(),
                    std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                ) => {}
            Err(e) => panic!("unexpected read error: {e}"),
        }
        assert!(
            std::time::Instant::now() < deadline,
            "old remote's client socket must be shut down after restore_local_mode"
        );
    }
}
