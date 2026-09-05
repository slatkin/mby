use super::*;
use crate::app::tests::{make_app_stub, make_remote_app_stub};
use crate::app::{LibEvent, QueueScope};

fn key(code: KeyCode) -> KeyChord {
    KeyChord::new(code, KeyModifiers::NONE)
}

fn key_ctrl(code: KeyCode) -> KeyChord {
    KeyChord::new(code, KeyModifiers::CONTROL)
}

// ── PLAYBACK_HELP_BINDINGS stays truthful to playback_command_for_key ───

/// Characterization test: replays every `PLAYBACK_HELP_BINDINGS` sample
/// chord (all of them, not just one side of a paired display entry like
/// `< / >`) through the real `playback_command_for_key` and asserts each
/// resolves to the command the help table claims — for `gated` entries,
/// only when gated open, and never resolving to *some other* command
/// when gated closed. This is what keeps the help overlay's `[playback]`
/// section from silently drifting off the real bindings (issue #133).
#[test]
fn playback_help_bindings_match_playback_command_for_key() {
    for binding in PLAYBACK_HELP_BINDINGS {
        for (sample, command) in binding.samples {
            if binding.gated {
                assert_eq!(
                    playback_command_for_key(*sample, true, false),
                    Some(command.clone()),
                    "keys={:?} label={:?} sample={:?} should fire when active",
                    binding.keys,
                    binding.label,
                    sample
                );
                assert_eq!(
                    playback_command_for_key(*sample, false, true),
                    Some(command.clone()),
                    "keys={:?} label={:?} sample={:?} should fire on a remote session",
                    binding.keys,
                    binding.label,
                    sample
                );
                assert_eq!(
                    playback_command_for_key(*sample, false, false),
                    None,
                    "keys={:?} label={:?} sample={:?} should not fire when ungated",
                    binding.keys,
                    binding.label,
                    sample
                );
            } else {
                assert_eq!(
                    playback_command_for_key(*sample, false, false),
                    Some(command.clone()),
                    "keys={:?} label={:?} sample={:?} should fire unconditionally",
                    binding.keys,
                    binding.label,
                    sample
                );
            }
        }
    }
}

// ── playback_command_for_key: gated on (active OR has_remote_session) ────

#[test]
fn enter_never_stops() {
    assert_eq!(
        playback_command_for_key(key(KeyCode::Enter), true, true),
        None
    );
    assert_eq!(
        playback_command_for_key(key(KeyCode::Enter), false, false),
        None
    );
}

/// Assert that `code` produces `expected` for every (active, has_remote_session)
/// combination — i.e. it fires unconditionally, with no gating at all.
fn assert_fires_unconditionally(code: KeyCode, expected: Command) {
    for active in [false, true] {
        for remote in [false, true] {
            assert_eq!(
                playback_command_for_key(key(code), active, remote),
                Some(expected.clone()),
                "code={code:?} active={active} remote={remote}"
            );
        }
    }
}

// ── `z`: unconditional, no `active` gate in either branch ───────────────

#[test]
fn z_fires_unconditionally() {
    assert_fires_unconditionally(KeyCode::Char('z'), Command::CycleOrToggleSubtitle);
}

#[test]
fn ctrl_z_does_not_fire() {
    assert_eq!(
        playback_command_for_key(key_ctrl(KeyCode::Char('z')), true, true),
        None
    );
}

// ── `m`: unconditional, no session check at all (the flagged bug) ──────

#[test]
fn m_fires_unconditionally() {
    assert_fires_unconditionally(KeyCode::Char('m'), Command::ToggleMute);
}

// ── `-`/`+`: unconditional volume ────────────────────────────────────────

#[test]
fn volume_keys_fire_unconditionally() {
    assert_fires_unconditionally(KeyCode::Char('-'), Command::AdjustVolume(-5));
    assert_fires_unconditionally(KeyCode::Char('+'), Command::AdjustVolume(5));
    assert_fires_unconditionally(KeyCode::Char('='), Command::AdjustVolume(5));
}

// ── `a`: gated on (active OR has_remote_session), same as the other
// transport keys -- see #88 (previously `active` only, no remote path).

#[test]
fn ctrl_a_does_not_fire() {
    assert_eq!(
        playback_command_for_key(key_ctrl(KeyCode::Char('a')), true, true),
        None
    );
}

#[test]
fn unrelated_key_does_not_fire() {
    assert_eq!(
        playback_command_for_key(key(KeyCode::Char('q')), true, true),
        None
    );
}

#[test]
fn o_opens_an_idle_feed_link_only_when_available() {
    let o = key(KeyCode::Char('o'));
    assert_eq!(
        idle_feed_command_for_key(o, false, false, true),
        Some(Command::OpenIdleFeedLink)
    );
    assert_eq!(idle_feed_command_for_key(o, true, false, true), None);
    assert_eq!(idle_feed_command_for_key(o, false, true, true), None);
    assert_eq!(idle_feed_command_for_key(o, false, false, false), None);
    assert_eq!(
        idle_feed_command_for_key(key_ctrl(KeyCode::Char('o')), false, false, true),
        None
    );
}

// ── dispatch: state-mutating variants ────────────────────────────────────

// `MBV_SYSTEM` is a process-global env var, so tests that touch it must
// not run concurrently with other env-mutating tests. Reuse config.rs's
// `SYS_ENV_LOCK` rather than defining a second, independent mutex here.
use crate::config::tests::SYS_ENV_LOCK as ENV_LOCK;

/// RAII guard that points state-dir lookups at a fresh tempdir and
/// cleans up on drop -- including on panic.
struct XdgStateHomeGuard {
    dir: std::path::PathBuf,
    _state_dir: crate::config::TestStateDirGuard,
}

impl XdgStateHomeGuard {
    fn new() -> Self {
        let dir = tempfile_dir();
        std::env::remove_var("MBV_SYSTEM");
        let state_dir = crate::config::TestStateDirGuard::new_at(dir.join("mbv"));
        Self {
            dir,
            _state_dir: state_dir,
        }
    }
}

impl Drop for XdgStateHomeGuard {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

#[test]
fn dispatch_toggle_mute_flips_state_and_persists() {
    let _g = ENV_LOCK.lock().unwrap();
    let _xdg = XdgStateHomeGuard::new();

    let mut app = make_app_stub();
    assert!(!app.mute_on);
    app.dispatch(Command::ToggleMute);
    assert!(app.mute_on);

    let prefs_path = crate::config::prefs_path();
    let saved = std::fs::read_to_string(&prefs_path).expect("prefs written");
    let v: serde_json::Value = serde_json::from_str(&saved).unwrap();
    assert_eq!(v["mute_on"], serde_json::json!(true));

    app.dispatch(Command::ToggleMute);
    assert!(!app.mute_on);
}

#[test]
fn dispatch_toggle_mute_while_attached_to_session_mutes_the_session_not_local() {
    use crate::app::tests::make_session;

    let mut app = make_app_stub();
    app.connected_session_id = Some("session-1".into());
    let mut sess = make_session("remote-host", "Emby");
    sess.muted = false;
    app.connected_session_state = Some(sess);

    app.dispatch(Command::ToggleMute);

    assert!(
        app.connected_session_state.as_ref().unwrap().muted,
        "pressing mute while attached to a session must mute that session \
         (optimistically, before the network round-trip completes)"
    );
    assert!(
        !app.mute_on,
        "the local mute preference must not change while attached to a session"
    );
}

#[test]
fn dispatch_toggle_mute_while_attached_to_session_toggles_back_off() {
    use crate::app::tests::make_session;

    let mut app = make_app_stub();
    app.connected_session_id = Some("session-1".into());
    let mut sess = make_session("remote-host", "Emby");
    sess.muted = true;
    app.connected_session_state = Some(sess);

    app.dispatch(Command::ToggleMute);

    assert!(!app.connected_session_state.as_ref().unwrap().muted);
}

#[test]
fn dispatch_toggle_mute_while_attached_to_session_with_unknown_mute_state_mutes_first() {
    // No session-state poll has landed yet for this connected session --
    // `connected_session_state` is still `None`. The first press should
    // be treated as "currently not muted" and mute.
    let mut app = make_app_stub();
    app.connected_session_id = Some("session-1".into());
    app.connected_session_state = None;

    app.dispatch(Command::ToggleMute);

    assert!(!app.mute_on);
}

#[test]
fn dispatch_toggle_play_pause_local_sends_player_command() {
    let mut app = make_app_stub();
    let rx = app.player.spy_on_commands();

    app.dispatch(Command::TogglePlayPause);

    assert!(matches!(rx.try_recv(), Ok(PlayerCommand::TogglePause)));
}

#[test]
fn dispatch_toggle_play_pause_remote_does_not_touch_local_player() {
    let mut app = make_app_stub();
    app.connected_session_id = Some("session-1".into());
    let rx = app.player.spy_on_commands();

    app.dispatch(Command::TogglePlayPause);

    assert!(
        !matches!(rx.try_recv(), Ok(PlayerCommand::TogglePause)),
        "the remote playback target must not leak transport commands into the local player"
    );
}

// ── dispatch: QueuePlayCursor (issue #134) ───────────────────────────────
// Shared by the queue tab's `Enter` key and a double-click on a queue row
// (`handle_mouse`); see the `Command::QueuePlayCursor` doc comment.

use crate::app::tests::make_item;
use crate::app::tests::make_items;

fn set_local_queue(app: &mut crate::app::App, items: Vec<mbv_core::api::EmbyItem>, cursor: usize) {
    app.player_tab.set_items(items, cursor);
}

#[test]
fn queue_play_cursor_on_empty_queue_is_a_no_op() {
    let mut app = make_app_stub();
    assert!(!app.dispatch(Command::QueuePlayCursor(0)));
    assert!(app.status.is_empty());
}

#[test]
fn queue_play_cursor_while_attached_to_session_hands_off_to_session() {
    let mut app = make_app_stub();
    set_local_queue(
        &mut app,
        vec![
            make_item("Track One", "Audio"),
            make_item("Track Two", "Audio"),
        ],
        1,
    );
    app.connected_session_id = Some("session-1".into());

    app.dispatch(Command::QueuePlayCursor(1));

    assert!(
        app.status.contains("Requesting playback"),
        "expected a remote-handoff status flash, got {:?}",
        app.status
    );
}

#[test]
fn queue_play_cursor_with_direct_remote_switches_to_remote_scope() {
    let mut app = make_remote_app_stub(make_items(2), make_items(3));
    set_local_queue(&mut app, make_items(2), 1);
    app.set_queue_scope(QueueScope::Local);
    app.connected_session_id = Some("session-1".into());

    app.dispatch(Command::QueuePlayCursor(1));

    assert!(
        app.status.contains("Requesting playback"),
        "expected a remote-handoff status flash, got {:?}",
        app.status
    );
    assert_eq!(
        app.viewed_queue_scope(),
        QueueScope::Remote,
        "queue scope should switch to Remote when Direct remote control is active"
    );
}

#[test]
fn queue_play_cursor_without_direct_remote_stays_on_local_scope() {
    let mut app = make_app_stub();
    set_local_queue(
        &mut app,
        vec![
            make_item("Track One", "Audio"),
            make_item("Track Two", "Audio"),
        ],
        1,
    );
    app.connected_session_id = Some("session-1".into());

    app.dispatch(Command::QueuePlayCursor(1));

    assert!(
        app.status.contains("Requesting playback"),
        "expected a remote-handoff status flash, got {:?}",
        app.status
    );
    assert_eq!(
        app.viewed_queue_scope(),
        QueueScope::Local,
        "queue scope should remain Local when there is no Direct remote control"
    );
}

#[test]
fn queue_play_cursor_jumps_to_cursor_when_active_and_playback_scope() {
    let mut app = make_app_stub();
    set_local_queue(
        &mut app,
        vec![
            make_item("Track One", "Audio"),
            make_item("Track Two", "Audio"),
        ],
        1,
    );
    {
        let mut st = app.player.status.lock().unwrap();
        st.active = true;
        st.current_idx = 0;
    }
    let rx = app.player.spy_on_commands();

    app.dispatch(Command::QueuePlayCursor(1));

    assert!(matches!(rx.try_recv(), Ok(PlayerCommand::JumpTo(1))));
}

#[test]
fn queue_play_cursor_seeks_to_start_when_cursor_is_the_current_playing_audio_item() {
    let mut app = make_app_stub();
    set_local_queue(&mut app, vec![make_item("Track One", "Audio")], 0);
    {
        let mut st = app.player.status.lock().unwrap();
        st.active = true;
        st.current_idx = 0;
    }
    let rx = app.player.spy_on_commands();

    app.dispatch(Command::QueuePlayCursor(0));

    assert!(matches!(
        rx.try_recv(),
        Ok(PlayerCommand::SeekAbsolute(pos)) if pos == 0.0
    ));
}

// Same unique-tempdir convention as api.rs's test-only `make_temp_data_dir`
// (uuid-suffixed, under the OS tempdir).
fn tempfile_dir() -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("mbv-test-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn playlists_load_error_preserves_existing_list_and_flashes() {
    // Regression: a failed playlist-list fetch used to replace the
    // existing list with an empty vec via `unwrap_or_default()`, making
    // the UI show "No playlists found". Now the error event clears
    // loading without touching the list.
    let mut app = make_app_stub();
    app.playlists = vec![make_item("Existing", "Playlist")];
    app.playlists_loading = true;

    app.handle_lib_event(LibEvent::PlaylistsLoadError(
        "connection refused".to_string(),
    ));

    assert!(!app.playlists_loading);
    assert_eq!(app.playlists.len(), 1);
    assert_eq!(app.playlists[0].name, "Existing");
    assert!(
        app.status.contains("connection refused"),
        "status was: {:?}",
        app.status
    );
}

#[test]
fn playlist_items_load_error_preserves_existing_items_and_flashes() {
    // Regression: a failed playlist-items fetch used to replace the
    // open playlist's items with an empty vec, rendering "Playlist is
    // empty". Now the error event clears loading without touching items.
    let mut app = make_app_stub();
    app.playlists_open = Some(make_item("My Playlist", "Playlist"));
    app.playlists_open_items = vec![make_item("Track 1", "Audio")];
    app.playlists_open_loading = true;

    app.handle_lib_event(LibEvent::PlaylistItemsLoadError {
        playlist_id: "id".to_string(),
        error: "timeout".to_string(),
    });

    assert!(!app.playlists_open_loading);
    assert_eq!(app.playlists_open_items.len(), 1);
    assert_eq!(app.playlists_open_items[0].name, "Track 1");
    assert!(app.status.contains("timeout"));
}
