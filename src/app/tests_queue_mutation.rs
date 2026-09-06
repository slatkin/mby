use super::*;
use crate::app::tests::*;
use mbv_core::playback_queue::QueueItem;

#[cfg(test)]
#[path = "tests_queue_mutation_playlist_save.rs"]
mod tests_queue_mutation_playlist_save;

#[cfg(test)]
#[path = "tests_queue_mutation_retention.rs"]
mod tests_queue_mutation_retention;

fn tracking_stub() -> mbv_core::remote_reconciliation::ReconciliationTracker {
    mbv_core::remote_reconciliation::ReconciliationTracker::new(
        "session",
        vec![
            mbv_core::remote_reconciliation::SubmittedOccurrence::new(1, "id0"),
            mbv_core::remote_reconciliation::SubmittedOccurrence::new(2, "id1"),
        ],
        0,
        0,
    )
    .unwrap()
}

/// Task 5.3d, Home typed-effect keyboard ownership: the Ctrl+A chord is
/// component-owned and reaches this effect as `ShellRequest::HomeEnqueue`
/// (see `home_component_tests` + `shell_home_effects`); the App boundary is
/// `App::home_enqueue_target`, which enqueues the shell-resolved CW item
/// immediately. Home content is Model-owned, so the test supplies the item
/// directly (the flat-cursor resolution is covered at the Model boundary).
#[test]
fn home_enqueue_from_home_view_applies_immediately() {
    let _guard = crate::config::TestStateDirGuard::new();
    let mut app = make_app_stub();
    let item = make_items(1).remove(0);

    app.home_enqueue_target(QueueItem::Emby(Box::new(item)), true);

    assert_eq!(app.player_tab.emby_items().len(), 1);
    assert_eq!(app.player_tab.emby_items()[0].id, "id0");
}

#[test]
fn home_enqueue_appends_to_direct_remote_queue() {
    let _guard = crate::config::TestStateDirGuard::new();
    let local_items = make_items(2);
    let remote_items = make_items(3);
    let (mut app, cmd_rx) = make_remote_app_stub_with_cmd_rx(local_items, remote_items.clone());
    app.queue_scope = QueueScope::Remote;
    let item = make_items(1).remove(0);

    // The shell resolves the CW item at the Model boundary (task 5.3d); the
    // App effect receives it directly.
    app.home_enqueue_target(QueueItem::Emby(Box::new(item)), true);

    assert_eq!(
        app.remote_player_tab
            .as_ref()
            .unwrap()
            .emby_items()
            .iter()
            .map(|i| i.id.as_str())
            .collect::<Vec<_>>(),
        remote_items
            .iter()
            .map(|i| i.id.as_str())
            .chain(std::iter::once("id0"))
            .collect::<Vec<_>>()
    );
    assert!(matches!(
        cmd_rx.try_recv(),
        Ok(mbv_core::ctrl::CtrlCmd::UnifiedQueueAppend { items })
            if items.len() == 1 && items[0].id() == "id0"
    ));
    assert!(
        cmd_rx.try_recv().is_err(),
        "Ctrl+A append must not follow UnifiedQueueAppend with ReplaceQueue"
    );
}

#[test]
fn enqueue_stops_tracking_and_applies_immediately() {
    let _guard = crate::config::TestStateDirGuard::new();
    let mut app = make_app_stub();
    let item = make_items(1).remove(0);
    app.remote_tracker = Some(tracking_stub());

    app.home_enqueue_target(QueueItem::Emby(Box::new(item)), true);
    assert!(!matches!(
        app.pending_overlay,
        Some(super::types_overlay::OverlayRequest::Confirm(_))
    ));
    assert!(app.remote_tracker.is_none());
    assert_eq!(app.player_tab.emby_items().len(), 1);
}

#[test]
fn tracked_playlist_deletes_apply_immediately() {
    let _guard = crate::config::TestStateDirGuard::new();
    let mut app = make_app_stub();
    app.player_tab
        .set_items(make_items(4), app.player_tab.queue_cursor);
    app.queue_source = crate::config::QueueSource::Playlist {
        id: Some("playlist-1".into()),
        name: "Saved".into(),
    };
    app.remote_tracker = Some(tracking_stub());

    app.remove_from_queue(1);
    app.remove_from_queue(1);

    assert!(app.remote_tracker.is_none());
    assert!(!matches!(
        app.pending_overlay,
        Some(super::types_overlay::OverlayRequest::Confirm(_))
    ));
    assert_eq!(
        app.player_tab
            .emby_items()
            .iter()
            .map(|item| item.id.as_str())
            .collect::<Vec<_>>(),
        vec!["id0", "id3"]
    );
    assert!(app.queue_dirty);
}

#[test]
fn clearing_local_queue_in_direct_remote_mode_leaves_remote_queue_intact() {
    let _guard = crate::config::TestStateDirGuard::new();
    let local_items = make_items(2);
    let remote_items = make_items(3);
    let mut app = make_remote_app_stub(local_items, remote_items.clone());
    app.set_queue_scope(QueueScope::Local);
    app.queue_source = crate::config::QueueSource::Album;
    app.queue_dirty = true;

    app.execute_pending_queue_action(PendingQueueAction::ClearQueue);

    assert!(app.player_tab.emby_items().is_empty());
    assert_eq!(app.player_tab.queue_cursor, 0);
    assert_eq!(
        app.remote_player_tab
            .as_ref()
            .unwrap()
            .emby_items()
            .iter()
            .map(|i| i.id.as_str())
            .collect::<Vec<_>>(),
        remote_items
            .iter()
            .map(|i| i.id.as_str())
            .collect::<Vec<_>>()
    );
    assert!(matches!(
        app.queue_source,
        crate::config::QueueSource::Unknown
    ));
    assert!(!app.queue_dirty);
}

#[test]
fn queue_edit_forwards_to_local_daemon_while_daemon_is_idle() {
    // Reproduces: attaching to a tracked remote Emby session (which never
    // touches `self.player`) while the local daemon that owns this queue
    // isn't itself playing anything (`active == false`). Queue edits must
    // still reach the daemon over ctrl, or its authoritative copy diverges
    // from what the client shows and re-adopting it on the next launch
    // resurrects deleted items.
    let _guard = crate::config::TestStateDirGuard::new();
    use crate::config::Config;
    use mbv_core::api::EmbyClient;
    let (remote, player_rx, cmd_rx) =
        mbv_core::remote_player::RemotePlayer::stub_with_command_rx(vec![], 0);
    let mut app = App::new_remote(
        EmbyClient::new(Config::default()),
        remote,
        player_rx,
        mbv_core::remote_player::DaemonEndpoint::Local,
    );
    app.player_tab
        .set_items(make_items(3), app.player_tab.queue_cursor);
    app.player_tab.queue_cursor = 0;
    app.player.status.lock().unwrap().active = false;
    assert!(app.remote_player_tab.is_none());
    // Drain the subtitle-prefs sync `App::new_remote` sends on attach so it
    // isn't mistaken for the removal's own command below.
    while cmd_rx.try_recv().is_ok() {}

    app.remove_from_queue(1);

    assert_eq!(
        app.player_tab
            .emby_items()
            .iter()
            .map(|i| i.id.as_str())
            .collect::<Vec<_>>(),
        vec!["id0", "id2"]
    );
    // Unified-capable remote peer: removal is sent as UnifiedQueueRemoveSlot.
    let cmd = cmd_rx.try_recv().unwrap();
    assert!(
        matches!(cmd, mbv_core::ctrl::CtrlCmd::UnifiedQueueRemoveSlot { .. }),
        "expected UnifiedQueueRemoveSlot"
    );
}

#[test]
fn clearing_remote_queue_in_direct_remote_mode_leaves_local_queue_metadata_intact() {
    let _guard = crate::config::TestStateDirGuard::new();
    let local_items = make_items(2);
    let remote_items = make_items(3);
    let mut app = make_remote_app_stub(local_items.clone(), remote_items);
    app.queue_source = crate::config::QueueSource::Playlist {
        id: Some("playlist-1".into()),
        name: "Saved".into(),
    };
    app.queue_dirty = true;

    app.execute_pending_queue_action(PendingQueueAction::ClearQueue);

    assert!(app
        .remote_player_tab
        .as_ref()
        .unwrap()
        .emby_items()
        .is_empty());
    assert_eq!(
        app.player_tab
            .emby_items()
            .iter()
            .map(|i| i.id.as_str())
            .collect::<Vec<_>>(),
        local_items
            .iter()
            .map(|i| i.id.as_str())
            .collect::<Vec<_>>()
    );
    assert!(matches!(
        app.queue_source,
        crate::config::QueueSource::Playlist { .. }
    ));
    assert!(app.queue_dirty);
}

/// `c` (`request_clear_queue`) must raise the confirmation for a direct-remote
/// daemon queue -- the queue lives in `remote_player_tab`, so the old
/// `self.player_tab.total_queue_len() == 0` check (the always-empty local queue)
/// silently swallowed the key. Clearing itself is already supported
/// (`execute_pending_queue_action` -> `replace_direct_remote_queue`).
#[test]
fn clear_queue_prompt_opens_for_direct_remote_daemon_queue() {
    let _guard = crate::config::TestStateDirGuard::new();
    // Empty local queue is the real socket-attached-mbvd state: the queue lives
    // only in `remote_player_tab`. Pre-fix this made the old `player_tab` check
    // early-return, so this test fails without the fix.
    let mut app = make_remote_app_stub(Vec::new(), make_items(3));
    app.set_queue_scope(QueueScope::Remote);
    assert_eq!(app.viewed_queue_scope(), QueueScope::Remote);

    app.request_clear_queue();

    assert!(
        matches!(
            app.pending_overlay,
            Some(super::types_overlay::OverlayRequest::Confirm(_))
        ),
        "clear-queue confirmation must open for a direct-remote daemon queue"
    );
}

/// A connected Emby session owns its queue on the remote device; `c` still
/// refuses it with the explanatory toast rather than opening the prompt.
#[test]
fn clear_queue_prompt_refused_for_connected_session_queue() {
    let _guard = crate::config::TestStateDirGuard::new();
    let mut app = make_remote_app_stub(make_items(2), make_items(3));
    app.set_queue_scope(QueueScope::Remote);
    app.connected_session_id = Some("session".into());

    app.request_clear_queue();

    assert!(!matches!(
        app.pending_overlay,
        Some(super::types_overlay::OverlayRequest::Confirm(_))
    ));
}

#[test]
fn clearing_tracked_queue_applies_immediately_and_stops_tracking() {
    let _guard = crate::config::TestStateDirGuard::new();
    let mut app = make_app_stub();
    app.player_tab
        .set_items(make_items(2), app.player_tab.queue_cursor);
    app.remote_tracker = Some(tracking_stub());

    app.execute_pending_queue_action(PendingQueueAction::ClearQueue);

    assert!(app.remote_tracker.is_none());
    assert!(!matches!(
        app.pending_overlay,
        Some(super::types_overlay::OverlayRequest::Confirm(_))
    ));
    assert!(app.player_tab.emby_items().is_empty());

    app.connected_session_id = Some("session".into());
    let mut advanced_session = make_session("Client", "Emby");
    advanced_session.id = "session".into();
    advanced_session.now_playing_item_id = Some("id1".into());
    app.handle_session_event(SessionEvent::Loaded {
        sessions: vec![advanced_session],
        generation: 1,
    });

    assert!(app.remote_tracker.is_none());
    assert!(app.sessions_rx.try_recv().is_err());
}

#[test]
fn removing_from_local_queue_in_direct_remote_mode_does_not_touch_remote_queue() {
    let _guard = crate::config::TestStateDirGuard::new();
    let local_items = make_items(3);
    let remote_items = make_items(2);
    let mut app = make_remote_app_stub(local_items.clone(), remote_items.clone());
    app.set_queue_scope(QueueScope::Local);

    app.remove_from_queue(1);

    assert_eq!(app.player_tab.emby_items().len(), 2);
    assert_eq!(
        app.player_tab
            .emby_items()
            .iter()
            .map(|i| i.id.as_str())
            .collect::<Vec<_>>(),
        vec![local_items[0].id.as_str(), local_items[2].id.as_str()]
    );
    assert_eq!(
        app.remote_player_tab
            .as_ref()
            .unwrap()
            .emby_items()
            .iter()
            .map(|i| i.id.as_str())
            .collect::<Vec<_>>(),
        remote_items
            .iter()
            .map(|i| i.id.as_str())
            .collect::<Vec<_>>()
    );
    assert!(app.queue_dirty);
    assert_eq!(app.remote_queue_undo_stack.len(), 0);
}

#[test]
fn removing_from_remote_queue_in_direct_remote_mode_does_not_touch_local_queue() {
    let _guard = crate::config::TestStateDirGuard::new();
    let local_items = make_items(2);
    let remote_items = make_items(3);
    let mut app = make_remote_app_stub(local_items.clone(), remote_items.clone());

    app.remove_from_queue(1);

    assert_eq!(
        app.remote_player_tab.as_ref().unwrap().emby_items().len(),
        2
    );
    assert_eq!(
        app.remote_player_tab
            .as_ref()
            .unwrap()
            .emby_items()
            .iter()
            .map(|i| i.id.as_str())
            .collect::<Vec<_>>(),
        vec![remote_items[0].id.as_str(), remote_items[2].id.as_str()]
    );
    assert_eq!(
        app.player_tab
            .emby_items()
            .iter()
            .map(|i| i.id.as_str())
            .collect::<Vec<_>>(),
        local_items
            .iter()
            .map(|i| i.id.as_str())
            .collect::<Vec<_>>()
    );
    assert!(!app.queue_dirty);
    assert_eq!(app.queue_undo_stack.len(), 0);
    assert_eq!(app.remote_queue_undo_stack.len(), 1);
}

#[test]
fn removing_non_active_item_keeps_cursor_off_now_playing_after_daemon_ack() {
    let _guard = crate::config::TestStateDirGuard::new();
    let mut app = make_local_daemon_app_stub(make_items(4));
    app.player.status.lock().unwrap().active = true;
    // Track 0 is playing; the user has selected track 2 in the queue view.
    app.player_tab.queue_cursor = 2;

    app.remove_from_queue(2);

    // Simulate the daemon's async ack of the removal: its `active_slot` reports
    // the playback position (still track 0), not the UI's selection.
    app.handle_player_event(PlayerEvent::UnifiedQueueUpdated(Box::new(
        emby_unified_state(&app.player_tab.emby_items(), 0),
    )));

    assert_eq!(
        app.player_tab.queue_cursor, 2,
        "deleting a non-playing item must not snap the display cursor onto \
         the now-playing item"
    );
}

#[test]
fn clearing_remote_queue_does_not_prompt_to_save_local_playlist() {
    let mut app = make_remote_app_stub(make_items(2), make_items(3));
    app.queue_source = crate::config::QueueSource::Playlist {
        id: Some("playlist-1".into()),
        name: "Saved".into(),
    };
    app.queue_dirty = true;

    app.replace_queue_or_prompt(PendingQueueAction::ClearQueue);

    assert!(!matches!(
        app.pending_overlay,
        Some(super::types_overlay::OverlayRequest::Confirm(_))
    ));
    assert!(app.pending_queue_action.is_none());
    assert!(app
        .remote_player_tab
        .as_ref()
        .unwrap()
        .emby_items()
        .is_empty());
    assert!(app.queue_dirty);
}

#[test]
fn context_menu_remove_targets_displayed_remote_queue() {
    let _guard = crate::config::TestStateDirGuard::new();
    let local_items = make_items(2);
    let remote_items = make_items(3);
    let mut app = make_remote_app_stub(local_items.clone(), remote_items.clone());
    app.panel_focus = PanelFocus::Queue;
    app.set_queue_scope(QueueScope::Remote);
    app.remote_player_tab.as_mut().unwrap().queue_cursor = 2;

    app.open_context_menu(false, None);

    let menu = match app.pending_overlay.as_ref() {
        Some(super::types_overlay::OverlayRequest::ContextMenu(menu)) => menu,
        _ => panic!("context menu"),
    };
    let action = menu
        .entries
        .iter()
        .find_map(|entry| match entry.action.as_ref() {
            Some(ContextAction::RemoveFromQueue(pos)) => Some(*pos),
            _ => None,
        })
        .expect("remove from queue action");
    assert_eq!(action, 2);

    app.execute_context_action(Some(ContextAction::RemoveFromQueue(action)), None);

    let item_ids = |items: &[EmbyItem]| items.iter().map(|i| i.id.clone()).collect::<Vec<_>>();
    assert_eq!(
        item_ids(&app.player_tab.emby_items()),
        item_ids(&local_items)
    );
    assert_eq!(
        item_ids(&app.remote_player_tab.as_ref().unwrap().emby_items()),
        vec![remote_items[0].id.clone(), remote_items[1].id.clone()]
    );
    assert_eq!(app.remote_queue_undo_stack.len(), 1);
}

#[test]
fn stale_context_menu_remove_remote_queue_index_is_ignored() {
    let _guard = crate::config::TestStateDirGuard::new();
    let local_items = make_items(2);
    let remote_items = make_items(3);
    let mut app = make_remote_app_stub(local_items.clone(), remote_items.clone());
    app.panel_focus = PanelFocus::Queue;
    app.set_queue_scope(QueueScope::Remote);
    app.remote_player_tab.as_mut().unwrap().queue_cursor = 2;

    app.open_context_menu(false, None);

    let menu = match app.pending_overlay.as_ref() {
        Some(super::types_overlay::OverlayRequest::ContextMenu(menu)) => menu,
        _ => panic!("context menu"),
    };
    let action = menu
        .entries
        .iter()
        .find_map(|entry| match entry.action.as_ref() {
            Some(ContextAction::RemoveFromQueue(pos)) => Some(*pos),
            _ => None,
        })
        .expect("remove from queue action");
    // Simulate the remote queue shrinking while the context menu is open.
    // We truncate the slots directly to preserve the cursor position
    // (simulating a race where the cursor hasn't been clamped yet).
    {
        let tab = app.remote_player_tab.as_mut().unwrap();
        tab.queue.truncate_slots(2);
    }

    app.execute_context_action(Some(ContextAction::RemoveFromQueue(action)), None);

    let item_ids = |items: &[EmbyItem]| items.iter().map(|i| i.id.clone()).collect::<Vec<_>>();
    assert_eq!(
        item_ids(&app.player_tab.emby_items()),
        item_ids(&local_items)
    );
    assert_eq!(
        item_ids(&app.remote_player_tab.as_ref().unwrap().emby_items()),
        vec![remote_items[0].id.clone(), remote_items[1].id.clone()]
    );
    assert_eq!(app.remote_player_tab.as_ref().unwrap().queue_cursor, 1);
    assert!(app.remote_queue_undo_stack.is_empty());
}

#[test]
fn boundary_queue_edit_does_not_retire_tracking() {
    let mut app = make_app_stub();
    app.player_tab
        .set_items(make_items(2), app.player_tab.queue_cursor);
    app.remote_tracker = Some(tracking_stub());
    app.player_tab.queue_cursor = 0;

    app.move_queue_item_up(app.player_tab.queue_cursor);

    assert!(app.remote_tracker.is_some());
    assert_eq!(app.player_tab.emby_items().len(), 2);
}

#[test]
fn empty_clear_queue_does_not_retire_tracking() {
    let mut app = make_app_stub();
    app.remote_tracker = Some(tracking_stub());

    app.execute_pending_queue_action(PendingQueueAction::ClearQueue);

    assert!(app.remote_tracker.is_some());
}
