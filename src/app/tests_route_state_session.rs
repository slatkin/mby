use super::tests_route_state::stub_endpoint;
use super::*;
use crate::app::tests::*;

#[test]
fn remote_slot_state_is_local_daemon_for_thin_client_mode() {
    let app = make_local_daemon_app_stub(make_items(3));

    assert_eq!(app.remote_slot_state(), RemoteSlotState::LocalDaemon);
    assert!(!app.can_disconnect_remote());
}

#[test]
fn player_owner_is_on_this_machine_for_in_process_local_tcp_and_unix_targets() {
    // Bare-mode in-process player: owner is on this machine.
    let mut app = make_app_stub();
    assert!(app.player_owner_is_on_this_machine());

    // Managed local daemon: owner is on this machine.
    let local_daemon_app = make_local_daemon_app_stub(make_items(3));
    assert!(local_daemon_app.player_owner_is_on_this_machine());

    // TCP endpoint (genuinely remote): owner is elsewhere.
    let (remote, remote_rx) = mbv_core::remote_player::RemotePlayer::stub(make_items(1), 0);
    app.switch_to_direct_remote(
        &make_session("remote-a", "mbv"),
        remote,
        remote_rx,
        &stub_endpoint(),
    );
    assert!(!app.player_owner_is_on_this_machine());

    // Unix endpoint: treated as not-this-machine (matching existing
    // `DaemonEndpoint::is_local()` semantics).
    let (remote, remote_rx) = mbv_core::remote_player::RemotePlayer::stub(make_items(1), 0);
    app.switch_to_direct_remote(
        &make_session("remote-b", "mbv"),
        remote,
        remote_rx,
        &stub_endpoint(),
    );
    assert!(!app.player_owner_is_on_this_machine());
}

#[test]
fn restoring_a_suspended_in_process_player_clears_locality_and_shows_in_process_ownership() {
    // Regression guard: a bare-mode app that switches to a
    // managed-local-daemon-classified target, then restores its suspended
    // in-process player must not retain local-daemon status — that stale
    // boolean used to survive restoration and enable heartbeat icons and
    // local-daemon queue paths for a now-bare-mode process.
    let mut app = make_app_stub();
    let (remote, remote_rx) = mbv_core::remote_player::RemotePlayer::stub(make_items(1), 0);
    app.switch_to_direct_remote(
        &make_session("local-daemon", "mbv"),
        remote,
        remote_rx,
        &mbv_core::remote_player::DaemonEndpoint::Local,
    );
    assert!(app.is_local_daemon());
    assert!(app.player_owner_is_on_this_machine());

    app.restore_local_mode("Disconnected from direct remote session");

    assert!(!app.is_local_daemon());
    assert!(app.player_owner_is_on_this_machine());
    assert_eq!(app.remote_slot_state(), RemoteSlotState::Off);
}

#[test]
fn announced_shutdown_of_current_remote_target_does_not_quit_local_daemon_home() {
    let _connect_guard = DAEMON_ROUTE_CONNECT_TEST_LOCK.lock().unwrap();
    fn reconnect_local(
        _endpoint: &mbv_core::remote_player::DaemonEndpoint,
    ) -> Result<
        (
            mbv_core::remote_player::RemotePlayer,
            std::sync::mpsc::Receiver<PlayerEvent>,
        ),
        String,
    > {
        Ok(mbv_core::remote_player::RemotePlayer::stub(Vec::new(), 0))
    }

    *DAEMON_ROUTE_CONNECT_OVERRIDE.lock().unwrap() = Some(reconnect_local);
    QUIT_REQUESTED.store(false, std::sync::atomic::Ordering::Relaxed);
    let (remote, player_rx) = mbv_core::remote_player::RemotePlayer::stub(make_items(1), 0);
    let mut app = App::new_remote(
        mbv_core::api::EmbyClient::new(crate::config::Config::default()),
        remote,
        player_rx,
        mbv_core::remote_player::DaemonEndpoint::Local,
    );
    // The app was launched against the local daemon, but playback has since
    // moved to a genuinely remote route. Disconnect handling must use this
    // live target flag, not the launch-time home flag.
    app.player_endpoint = Some(mbv_core::remote_player::DaemonEndpoint::Tcp(
        "127.0.0.1:0".parse().unwrap(),
    ));

    app.handle_player_event(PlayerEvent::DaemonShutdownAnnounced);

    assert!(!QUIT_REQUESTED.load(std::sync::atomic::Ordering::Relaxed));
    assert!(!matches!(
        app.pending_overlay,
        Some(super::types_overlay::OverlayRequest::DaemonLost(_))
    ));
    assert!(
        app.is_local_daemon(),
        "restore should reconnect the home daemon"
    );
    *DAEMON_ROUTE_CONNECT_OVERRIDE.lock().unwrap() = None;
    QUIT_REQUESTED.store(false, std::sync::atomic::Ordering::Relaxed);
}

#[test]
fn resetting_local_daemon_queue_view_drops_stale_remote_queue_and_scope() {
    let mut app = make_local_daemon_app_stub(make_items(1));
    app.remote_player_tab = Some(PlayerTab::from_emby_items(make_items(2), 1));
    app.queue_scope = QueueScope::Remote;

    app.reset_local_daemon_queue_view();

    assert!(app.remote_player_tab.is_none());
    assert_eq!(app.queue_scope, QueueScope::Local);
    assert_eq!(app.visible_queue_scope(), QueueScope::Local);
}

#[test]
fn attached_session_state_wins_over_local_daemon_indicator() {
    let mut app = make_local_daemon_app_stub(make_items(3));
    app.connected_session_id = Some("session-1".into());

    assert_eq!(app.remote_slot_state(), RemoteSlotState::AttachedSession);
    assert!(app.can_disconnect_remote());
}

#[test]
fn disconnect_remote_does_not_exit_local_daemon_mode() {
    let mut app = make_local_daemon_app_stub(make_items(3));

    app.disconnect_remote();

    assert_eq!(app.remote_slot_state(), RemoteSlotState::LocalDaemon);
    assert!(app.player.is_remote());
    assert!(!app.can_disconnect_remote());
    assert_eq!(app.status, "No session selected");
}

#[test]
fn disconnect_remote_clears_attached_remote_session() {
    let mut app = make_app_stub();
    app.connected_session_id = Some("session-1".into());
    app.connected_session_state = Some(make_session("remote-host", "Emby"));
    app.session_miss_count = 2;
    app.remote_pos_s = 120;

    app.disconnect_remote();

    assert_eq!(app.remote_slot_state(), RemoteSlotState::Off);
    assert!(app.connected_session_id.is_none());
    assert!(app.connected_session_state.is_none());
    assert_eq!(app.session_miss_count, 0);
    assert_eq!(app.remote_pos_s, 0);
    assert_eq!(app.status, "Disconnected from remote session");
}

#[test]
fn disconnect_remote_restores_local_for_sessions_panel_direct_remote() {
    let mut app = make_app_stub();
    let (remote, remote_rx) = mbv_core::remote_player::RemotePlayer::stub(make_items(1), 0);
    let sess = make_session("music", "mbv");

    app.switch_to_direct_remote(&sess, remote, remote_rx, &stub_endpoint());

    assert_eq!(app.direct_remote_label.as_deref(), Some("music"));
    assert!(app.can_disconnect_remote());

    app.disconnect_remote();

    assert!(app.direct_remote_label.is_none());
    assert!(app.active_route.is_none());
    assert!(!app.player.is_remote());
    assert_eq!(app.status, "Disconnected from direct remote session");
}

#[test]
fn disconnecting_attached_session_preserves_sessions_panel_direct_remote() {
    let mut app = make_app_stub();
    let (remote, remote_rx) = mbv_core::remote_player::RemotePlayer::stub(make_items(1), 0);
    let direct_session = make_session("music", "mbv");
    let attached_session = make_session("living-room", "Emby");

    app.switch_to_direct_remote(&direct_session, remote, remote_rx, &stub_endpoint());
    app.connect_to_session(&attached_session);

    assert!(app.direct_remote_connected);
    assert!(app.connected_session_id.is_some());

    app.disconnect_remote();

    assert!(app.player.is_remote());
    assert!(app.direct_remote_connected);
    assert!(app.can_disconnect_remote());

    app.disconnect_remote();

    assert!(!app.player.is_remote());
    assert!(!app.direct_remote_connected);
}

#[test]
fn displayed_queue_playback_state_stays_active_for_local_daemon_queue() {
    let app = make_local_daemon_app_stub(make_items(3));
    {
        let mut status = app.player.status.lock().unwrap();
        status.active = true;
        status.current_idx = 2;
        status.position_ticks = 42;
        status.runtime_ticks = 84;
        status.paused = true;
    }

    assert_eq!(
        app.displayed_queue_playback_state(),
        PlaybackState {
            active: true,
            active_idx: 2,
            position_ticks: 42,
            runtime_ticks: 84,
            paused: true,
        }
    );
}

#[test]
fn local_daemon_consume_adjusts_active_idx_after_removal_shift() {
    let _guard = crate::config::TestStateDirGuard::new();
    let mut app = make_local_daemon_app_stub(make_items(4));
    app.config.lock().unwrap().consume_videos = true;
    {
        let mut status = app.player.status.lock().unwrap();
        status.active = true;
        status.current_idx = 1;
    }

    app.handle_player_event(PlayerEvent::TrackCompleted {
        idx: 1,
        position_ticks: 0,
        played: true,
        consume: true,
        progress_report_accepted: false,
    });
    {
        let mut status = app.player.status.lock().unwrap();
        // Thin-client path: the remote player updates status.current_idx
        // from the daemon's TrackChanged event before App handles the
        // pending consume removal, so App must correct the shifted index.
        status.current_idx = 2;
    }
    app.handle_player_event(PlayerEvent::TrackChanged(2));

    assert_eq!(app.player_tab.queue_cursor, 1);
    assert_eq!(
        app.displayed_queue_playback_state().active_idx,
        1,
        "after removing the completed item, the active index must shift to \
             the now-playing item's new slot instead of following the stale \
             pre-removal numeric index"
    );
}

#[test]
fn direct_remote_consume_adjusts_active_idx_after_removal_shift() {
    let _guard = crate::config::TestStateDirGuard::new();
    let local_items = make_items(2);
    let remote_items = make_items(4);
    let mut app = make_remote_app_stub(local_items.clone(), remote_items.clone());
    app.config.lock().unwrap().consume_videos = true;
    app.set_queue_scope(QueueScope::Remote);
    {
        let mut status = app.player.status.lock().unwrap();
        status.active = true;
        status.current_idx = 1;
    }

    app.handle_player_event(PlayerEvent::TrackCompleted {
        idx: 1,
        position_ticks: 0,
        played: true,
        consume: true,
        progress_report_accepted: false,
    });
    {
        let mut status = app.player.status.lock().unwrap();
        // Network direct-remote path receives the same raw pre-removal
        // TrackChanged index from the daemon as the same thin-client
        // control path covered above.
        status.current_idx = 2;
    }
    app.handle_player_event(PlayerEvent::TrackChanged(2));

    let item_ids = |items: &[EmbyItem]| items.iter().map(|i| i.id.clone()).collect::<Vec<_>>();
    assert_eq!(
        serde_json::to_value(app.player_tab.emby_items()).unwrap(),
        serde_json::to_value(&local_items).unwrap()
    );
    assert_eq!(app.player_tab.queue_cursor, 0);
    assert_eq!(
        item_ids(&app.remote_player_tab.as_ref().unwrap().emby_items()),
        vec![
            remote_items[0].id.clone(),
            remote_items[2].id.clone(),
            remote_items[3].id.clone(),
        ]
    );
    assert_eq!(app.remote_player_tab.as_ref().unwrap().queue_cursor, 1);
    assert_eq!(
        app.displayed_queue_playback_state().active_idx,
        1,
        "after removing the completed remote item, the active index must \
             shift to the now-playing item's new remote-queue slot"
    );
}

#[test]
fn restore_local_mode_reconnects_local_daemon_when_no_suspended_local_player_exists() {
    // Regression guard: an `App::new_remote(..., is_local_daemon = true)`
    // instance (local-daemon home, e.g. `stay_alive` auto-detected at startup)
    // has no genuinely local in-process `Player` to suspend when it routes
    // away via `switch_to_library_route`'s already-remote branch --
    // `suspended_local` stays `None` for its whole life. Before this fix,
    // `restore_local_mode` did nothing in that case, leaving the player
    // disconnected instead of reconnected to the local daemon.
    let _guard = crate::config::TestStateDirGuard::new();
    let _connect_guard = DAEMON_ROUTE_CONNECT_TEST_LOCK.lock().unwrap();
    fn route_connect_success(
        _endpoint: &mbv_core::remote_player::DaemonEndpoint,
    ) -> Result<
        (
            mbv_core::remote_player::RemotePlayer,
            mpsc::Receiver<PlayerEvent>,
        ),
        String,
    > {
        Ok(mbv_core::remote_player::RemotePlayer::stub(
            make_items(1),
            0,
        ))
    }

    let mut app = make_local_daemon_app_stub(make_items(2));
    assert!(app.home_is_local_daemon);
    let (remote, remote_rx) = mbv_core::remote_player::RemotePlayer::stub(make_items(1), 0);
    app.switch_to_library_route("music", remote, remote_rx, &stub_endpoint());
    assert!(app.suspended_local.is_none());

    *DAEMON_ROUTE_CONNECT_OVERRIDE.lock().unwrap() = Some(route_connect_success);
    app.restore_local_mode("test: route no longer resolves");
    *DAEMON_ROUTE_CONNECT_OVERRIDE.lock().unwrap() = None;

    assert!(app.player.is_remote());
    assert!(app.is_local_daemon());
    assert!(app.active_route.is_none());
}

#[test]
fn restore_local_mode_clears_remote_queue_presentation_for_local_daemon_home() {
    // Regression guard for #424: a stay-alive client (`home_is_local_daemon`)
    // that routed to a remote mbvd via `switch_to_library_route` must land
    // back on the plain local-daemon presentation after `restore_local_mode` --
    // the reconnected daemon's items go into the unified `player_tab`,
    // `remote_player_tab` is cleared, and scope stays `Local` (no scope pill).
    // Before the fix the reconnected items were stuffed back into
    // `remote_player_tab`, leaving `has_remote_queue()` true and
    // `remote_slot_state()` reporting `DirectRemote`.
    let _guard = crate::config::TestStateDirGuard::new();
    let _connect_guard = DAEMON_ROUTE_CONNECT_TEST_LOCK.lock().unwrap();
    fn route_connect_success(
        _endpoint: &mbv_core::remote_player::DaemonEndpoint,
    ) -> Result<
        (
            mbv_core::remote_player::RemotePlayer,
            mpsc::Receiver<PlayerEvent>,
        ),
        String,
    > {
        Ok(mbv_core::remote_player::RemotePlayer::stub(
            make_items(2),
            1,
        ))
    }

    let mut app = make_local_daemon_app_stub(make_items(2));
    assert!(app.home_is_local_daemon);
    let (remote, remote_rx) = mbv_core::remote_player::RemotePlayer::stub(make_items(3), 0);
    app.switch_to_library_route("music", remote, remote_rx, &stub_endpoint());
    assert!(app.remote_player_tab.is_some());
    assert_eq!(app.remote_slot_state(), RemoteSlotState::DirectRemote);

    *DAEMON_ROUTE_CONNECT_OVERRIDE.lock().unwrap() = Some(route_connect_success);
    app.restore_local_mode("test: route no longer resolves");
    *DAEMON_ROUTE_CONNECT_OVERRIDE.lock().unwrap() = None;

    assert!(app.player.is_remote());
    assert!(app.is_local_daemon());
    assert_eq!(app.remote_slot_state(), RemoteSlotState::LocalDaemon);
    assert!(app.remote_player_tab.is_none());
    // The reconnected daemon's items land in the unified queue, not emptied.
    assert_eq!(app.displayed_queue().emby_items().len(), 2);
    assert_eq!(app.displayed_queue().queue_cursor, 1);
}

#[test]
fn restore_local_mode_flashes_combined_status_when_local_daemon_reconnect_fails() {
    // Same starting scenario as above, but the local-daemon reconnect
    // attempt itself fails -- confirms `restore_local_mode` reports the
    // daemon as unavailable instead of claiming local playback works
    // (there is no suspended local player in this path).
    let _guard = crate::config::TestStateDirGuard::new();
    let _connect_guard = DAEMON_ROUTE_CONNECT_TEST_LOCK.lock().unwrap();
    fn route_connect_failure(
        _endpoint: &mbv_core::remote_player::DaemonEndpoint,
    ) -> Result<
        (
            mbv_core::remote_player::RemotePlayer,
            mpsc::Receiver<PlayerEvent>,
        ),
        String,
    > {
        Err("test: connect failure".to_string())
    }

    let mut app = make_local_daemon_app_stub(make_items(2));
    let (remote, remote_rx) = mbv_core::remote_player::RemotePlayer::stub(make_items(1), 0);
    app.switch_to_library_route("music", remote, remote_rx, &stub_endpoint());

    *DAEMON_ROUTE_CONNECT_OVERRIDE.lock().unwrap() = Some(route_connect_failure);
    app.restore_local_mode("test: route no longer resolves");
    *DAEMON_ROUTE_CONNECT_OVERRIDE.lock().unwrap() = None;

    // `disconnect_remote` tears down the old remote's socket but doesn't
    // change `PlayerProxy`'s inner variant, so the stale remote player is
    // still what `is_remote()` reports here -- there was nothing to swap it
    // for since the reconnect failed. This mirrors `restore_local_mode`'s
    // pre-existing behavior for any other failure path: no invented
    // recovery UX, just the same left-disconnected player plus a status
    // flash.
    assert!(app.player.is_remote());
    assert!(app.status.contains("test: route no longer resolves"));
    assert!(app.status.contains("local daemon unavailable"));
    assert!(
        !app.status.contains("using local playback"),
        "must not claim local playback when the daemon is unreachable and no local player is available"
    );
}

#[test]
fn displayed_queue_playback_state_is_inactive_for_non_playback_scope() {
    let mut app = make_remote_app_stub(make_items(2), make_items(3));
    app.connected_session_state = Some(make_session("remote-host", "Emby"));
    app.connected_session_state
        .as_mut()
        .unwrap()
        .now_playing_item_id = Some("id1".into());
    app.set_queue_scope(QueueScope::Local);

    assert_eq!(app.visible_queue_scope(), QueueScope::Local);
    assert_eq!(
        app.displayed_queue_playback_state(),
        PlaybackState::default()
    );
}

#[test]
fn throbber_advances_when_session_reports_paused_but_position_advances() {
    // Some Emby clients always report IsPaused=true even while the
    // transport is actively playing. The run loop bumps
    // `remote_stalled_while_paused` based on position observations, so when
    // position keeps advancing each poll the throttle is cleared and the
    // throbber keeps ticking.

    let mut app = make_remote_app_stub(make_items(2), make_items(2));
    app.connected_session_state = Some({
        let mut s = make_session("remote-host", "Emby");
        s.now_playing = Some("Item 0".into());
        s.now_playing_item_id = Some("id0".into());
        s.runtime_ticks = 90 * mbv_core::api::TICKS_PER_SECOND;
        s.is_paused = true;
        s
    });
    // Run loop saw a position advance this poll -> not stalled.
    app.remote_stalled_while_paused = false;

    let playback = app.effective_playback_state();
    assert!(
        playback.active,
        "remote with now_playing + matching id must be active (so the playback panel is in a now-playing state)"
    );

    assert!(
        !app.playback_transport_paused(),
        "throbber must keep ticking when the latest API poll observed a position advance"
    );
}

#[test]
fn throbber_freezes_when_remote_pause_is_observed() {
    // After the user pauses remotely, the next API poll sees IsPaused=true
    // with no position advance; the run loop latches
    // `remote_stalled_while_paused` so the throbber freezes immediately
    // rather than waiting out the 22s extrapolate window.

    let mut app = make_remote_app_stub(make_items(2), make_items(2));
    app.connected_session_state = Some({
        let mut s = make_session("remote-host", "Emby");
        s.now_playing = Some("Item 0".into());
        s.now_playing_item_id = Some("id0".into());
        s.runtime_ticks = 90 * mbv_core::api::TICKS_PER_SECOND;
        s.is_paused = true;
        s
    });
    // Run loop saw IsPaused=true with no position advance this poll.
    app.remote_stalled_while_paused = true;

    assert!(
        app.playback_transport_paused(),
        "throbber must freeze once a single API poll observes IsPaused=true with no position advance"
    );
}
