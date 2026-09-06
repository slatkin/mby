#[test]
fn packaged_startup_context_is_service_independent() {
    let startup = DaemonStartupContext::new(Config::default(), DaemonRole::Packaged);
    assert_eq!(startup.role, DaemonRole::Packaged);
    assert!(startup.emby.is_none());
    assert!(startup.audiobookshelf.is_none());
}

#[test]
fn audiobookshelf_reconciliation_installs_context_and_enables_admission() {
    let _guard = crate::config::TestStateDirGuard::new();
    crate::config::persist_audiobookshelf_setup_and_secret(
        &crate::config::AudiobookshelfSetup::new("https://books.example"),
        "owner-secret",
    )
    .unwrap();

    let item = abs_qi("library-a", "episode-1");
    assert!(
        !daemon_admits(&item, false, false, false),
        "without installed runtime the item is ineligible"
    );

    let mut current = None;
    reconcile_abs(1, &mut current).unwrap();
    assert!(
        current.is_some(),
        "matching revision installs the Audiobookshelf owner context"
    );

    assert!(
        daemon_admits(&item, false, false, true),
        "installed runtime enables Audiobookshelf admission"
    );
}

#[test]
fn packaged_context_loads_unreachable_emby_without_authenticating() {
    let _guard = crate::config::TestStateDirGuard::new();
    let mut config = Config::default();
    config.emby_setup = Some(crate::config::EmbySetup::new(
        "http://127.0.0.1:1",
        "owner-user",
    ));
    crate::config::save_service_secret(crate::config::ServiceKind::Emby, "unreachable-token")
        .unwrap();
    let owner =
        EmbyOwnerContext::from_packaged_storage_result(&config).expect("owner context loads");
    assert_eq!(owner.revision, 1);
    assert_eq!(
        owner.client.lock().unwrap().config.server_url,
        "http://127.0.0.1:1"
    );
}

#[test]
fn emby_absence_keeps_feed_admission_and_rejects_emby_admission() {
    let feed = QueueItem::Feed(FeedEntry {
        guid: "feed-1".into(),
        title: "Episode".into(),
        enclosure_url: Some("https://example.test/episode.mp3".into()),
        link: None,
        mime_type: Some("audio/mpeg".into()),
        duration_ticks: None,
        pub_date_secs: None,
        feed_kind: Some(crate::config::FeedKind::Audio),
        feed_id: None,
        position_ticks: 0,
        played: false,
    });
    assert!(daemon_admits(&feed, false, false, false));
    assert!(!daemon_admits(
        &emby_qi("old", "Video", "Movie"),
        false,
        false,
        false
    ));
}

#[test]
fn absent_emby_websocket_is_a_noop_for_ctrl_and_queue_state() {
    let player = cold_player();
    let registry = Arc::new(Mutex::new(CtrlClients::default()));
    let (id, rx) = {
        let mut clients = registry.lock().unwrap();
        connect_client(&mut clients)
    };
    let mut queue = PlaybackQueue::default();
    let mut source = QueueSource::Unknown;
    handle_ws(
        WsEvent::TogglePause,
        None,
        &player,
        false,
        &mut queue,
        &mut source,
        &shared_queue_state(),
        &registry,
    );
    assert!(registry.lock().unwrap().has_client(id));
    assert!(rx.try_recv().is_err());
    assert!(queue.is_empty());
}

#[test]
fn owner_administration_is_local_transport_only() {
    assert!(owner_admin_transport_allowed(
        DaemonRole::Packaged,
        crate::config::ServiceKind::Emby,
        Some(CtrlTransport::Local)
    ));
    assert!(!owner_admin_transport_allowed(
        DaemonRole::Packaged,
        crate::config::ServiceKind::Emby,
        Some(CtrlTransport::Tcp)
    ));
    assert!(!owner_admin_transport_allowed(
        DaemonRole::Packaged,
        crate::config::ServiceKind::Emby,
        None
    ));
    // The user-owned Local daemon may reconcile Audiobookshelf but not Emby.
    assert!(owner_admin_transport_allowed(
        DaemonRole::Local,
        crate::config::ServiceKind::Audiobookshelf,
        Some(CtrlTransport::Local)
    ));
    assert!(!owner_admin_transport_allowed(
        DaemonRole::Local,
        crate::config::ServiceKind::Emby,
        Some(CtrlTransport::Local)
    ));
}

#[test]
fn audiobookshelf_reconciliation_rejects_revision_mismatch_without_state_change() {
    let _guard = crate::config::TestStateDirGuard::new();
    crate::config::persist_audiobookshelf_setup_and_secret(
        &crate::config::AudiobookshelfSetup::new("https://books.example"),
        "owner-secret",
    )
    .unwrap();

    let mut current = None;
    reconcile_abs(1, &mut current).unwrap();
    assert!(current.is_some(), "matching revision must install context");
    let pre = current.as_ref().unwrap().generation;

    let result = reconcile_abs(2, &mut current);
    assert!(
        matches!(result, Err(ServiceSetupRejection::RevisionMismatch)),
        "mismatched revision must be rejected, got {result:?}"
    );
    assert_eq!(
        current.as_ref().unwrap().generation,
        pre,
        "a rejected reconciliation must not change the installed runtime"
    );
}

#[test]
fn audiobookshelf_reconciliation_reports_storage_unavailable_without_state_change() {
    let _guard = crate::config::TestStateDirGuard::new();
    crate::config::persist_audiobookshelf_setup_and_secret(
        &crate::config::AudiobookshelfSetup::new("https://books.example"),
        "owner-secret",
    )
    .unwrap();

    let mut current = None;
    reconcile_abs(1, &mut current).unwrap();
    assert!(current.is_some());
    let pre = current.as_ref().unwrap().generation;

    // Drop the Service secret so the owner context can no longer be loaded.
    crate::config::clear_service_secret(crate::config::ServiceKind::Audiobookshelf);

    let result = reconcile_abs(1, &mut current);
    assert!(
        matches!(result, Err(ServiceSetupRejection::StorageUnavailable)),
        "unreadable storage must be rejected, got {result:?}"
    );
    assert_eq!(
        current.as_ref().unwrap().generation,
        pre,
        "a rejected reconciliation must not change the installed runtime"
    );
}

#[test]
fn audiobookshelf_reconciliation_drops_context_when_setup_is_absent() {
    let _guard = crate::config::TestStateDirGuard::new();
    crate::config::persist_audiobookshelf_setup_and_secret(
        &crate::config::AudiobookshelfSetup::new("https://books.example"),
        "owner-secret",
    )
    .unwrap();

    let mut current = None;
    reconcile_abs(1, &mut current).unwrap();
    assert!(
        current.is_some(),
        "setup must install context before removal"
    );

    crate::config::remove_audiobookshelf_setup_and_secret().unwrap();
    reconcile_abs(1, &mut current).unwrap();
    assert!(
        current.is_none(),
        "removal signal must drop the Audiobookshelf owner context"
    );
}

// A reconcile fixture with a specific canonical queue so the replacement and
// removal paths can assert Bound-slot purge alongside context changes.
fn reconcile_abs_with_queue(
    revision: u64,
    current: &mut Option<super::AudiobookshelfOwnerContext>,
    queue: &mut PlaybackQueue,
    source: &mut QueueSource,
) -> Result<(), ServiceSetupRejection> {
    let player = cold_player();
    let shared = shared_queue_state();
    let clients = Arc::new(Mutex::new(CtrlClients::default()));
    let client = Arc::new(Mutex::new(crate::api::EmbyClient::new(Config::default())));
    reconcile_packaged_audiobookshelf(
        revision, current, &player, queue, source, &shared, &clients, &client,
    )
}

#[test]
fn audiobookshelf_replacement_finalizes_and_purges_abs_slots() {
    let _guard = crate::config::TestStateDirGuard::new();
    crate::config::persist_audiobookshelf_setup_and_secret(
        &crate::config::AudiobookshelfSetup::new("https://a.example"),
        "secret-a",
    )
    .unwrap();

    let mut queue = PlaybackQueue::from_queue_items(
        vec![abs_qi("li_1", "ep_1"), emby_qi("movie1", "Video", "Movie")],
        Some(0),
    );
    let mut source = QueueSource::Remote;

    let mut current = None;
    reconcile_abs_with_queue(1, &mut current, &mut queue, &mut source).unwrap();
    assert!(current.is_some(), "initial setup installs context");

    // Replace with a different server.
    crate::config::replace_audiobookshelf_setup_and_secret(
        &crate::config::AudiobookshelfSetup::new("https://b.example"),
        "secret-b",
        || Ok(()),
        || {},
    )
    .unwrap();

    reconcile_abs_with_queue(2, &mut current, &mut queue, &mut source).unwrap();

    assert_eq!(
        queue.len(),
        1,
        "replacement purges the ABS slot and keeps the Emby slot"
    );
    assert!(queue.slots()[0].item.is_emby());
    assert!(
        queue
            .slots()
            .iter()
            .all(|slot| !slot.item.is_audiobookshelf()),
        "no Audiobookshelf slot may survive a different-server replacement"
    );
    assert_eq!(
        current.as_ref().unwrap().setup.server_url,
        "https://b.example",
        "replacement installs the new server context"
    );
}

#[test]
fn audiobookshelf_disconnect_stops_queue_and_purges_abs_slots() {
    let _guard = crate::config::TestStateDirGuard::new();
    crate::config::persist_audiobookshelf_setup_and_secret(
        &crate::config::AudiobookshelfSetup::new("https://a.example"),
        "secret-a",
    )
    .unwrap();

    let mut queue = PlaybackQueue::from_queue_items(
        vec![
            abs_qi("li_1", "ep_1"),
            book_qi("book_1"),
            emby_qi("movie1", "Video", "Movie"),
            QueueItem::Feed(FeedEntry {
                guid: "feed-1".into(),
                title: "Episode".into(),
                enclosure_url: Some("https://example.test/episode.mp3".into()),
                link: None,
                mime_type: Some("audio/mpeg".into()),
                duration_ticks: None,
                pub_date_secs: None,
                feed_kind: Some(crate::config::FeedKind::Audio),
                feed_id: None,
                position_ticks: 0,
                played: false,
            }),
        ],
        Some(0),
    );
    let mut source = QueueSource::Remote;

    let mut current = None;
    reconcile_abs_with_queue(1, &mut current, &mut queue, &mut source).unwrap();
    assert!(current.is_some());

    // Removal (the daemon-side effect of `mbvd --disconnect abs`).
    crate::config::remove_audiobookshelf_setup_and_secret().unwrap();
    reconcile_abs_with_queue(0, &mut current, &mut queue, &mut source).unwrap();

    assert!(current.is_none(), "removal drops the owner context");
    assert_eq!(
        queue.len(),
        2,
        "ABS episode and book slots purged, Emby and Feed slots retained"
    );
    assert!(
        queue.slots()[0].item.is_emby() && queue.slots()[1].item.is_feed(),
        "Emby and Feed slots must remain in canonical order"
    );
    assert!(
        queue
            .slots()
            .iter()
            .all(|slot| !slot.item.is_audiobookshelf_any()),
        "no Audiobookshelf episode or book slot may survive a disconnect"
    );
}

#[test]
fn every_setup_rejection_reason_is_wire_representable() {
    for reason in [
        ServiceSetupRejection::UnsupportedService,
        ServiceSetupRejection::RevisionMismatch,
        ServiceSetupRejection::StorageUnavailable,
        ServiceSetupRejection::TransitionRejected,
    ] {
        let event = CtrlEvent::ServiceSetupRejected {
            kind: crate::config::ServiceKind::Emby,
            revision: 4,
            reason,
        };
        let decoded: CtrlEvent =
            serde_json::from_str(&serde_json::to_string(&event).unwrap()).unwrap();
        assert!(
            matches!(decoded, CtrlEvent::ServiceSetupRejected { reason: decoded_reason, .. } if decoded_reason == reason)
        );
    }
}
use super::{
    daemon_admits, install_daemon_audiobookshelf_context, owner_admin_transport_allowed,
    reconcile_packaged_audiobookshelf, DaemonRole, DaemonStartupContext, EmbyOwnerContext,
};
use crate::ctrl::ServiceSetupRejection;

#[test]
fn daemon_install_audiobookshelf_context_enables_player_admission() {
    let _guard = crate::config::TestStateDirGuard::new();
    crate::config::persist_audiobookshelf_setup_and_secret(
        &crate::config::AudiobookshelfSetup::new("https://books.example"),
        "owner-secret",
    )
    .unwrap();
    let runtime = super::AudiobookshelfOwnerContext::from_packaged_storage_result(
        &crate::config::load_config().unwrap(),
    )
    .unwrap();

    let player = cold_player();
    let (merged_tx, _merged_rx) = mpsc::channel::<DaemonEvent>();

    assert!(!player.can_admit_audiobookshelf());
    install_daemon_audiobookshelf_context(&player, &Some(runtime), &merged_tx);
    assert!(
        player.can_admit_audiobookshelf(),
        "installed runtime enables Audiobookshelf admission on the daemon player"
    );

    install_daemon_audiobookshelf_context(&player, &None, &merged_tx);
    assert!(
        !player.can_admit_audiobookshelf(),
        "clearing the runtime clears Audiobookshelf admission"
    );
}

/// Reconcile the Audiobookshelf owner with a fresh, empty player and queue.
/// Isolates the reconcile/context-install path from active playback so the
/// setup-transition tests stay focused on runtime state.
fn reconcile_abs(
    revision: u64,
    current: &mut Option<super::AudiobookshelfOwnerContext>,
) -> Result<(), ServiceSetupRejection> {
    let mut queue = PlaybackQueue::default();
    let mut source = QueueSource::Unknown;
    reconcile_abs_with_queue(revision, current, &mut queue, &mut source)
}
