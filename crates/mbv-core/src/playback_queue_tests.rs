use super::*;

fn item(id: &str) -> EmbyItem {
    EmbyItem {
        id: id.to_string(),
        name: format!("Item {id}"),
        item_type: "Episode".to_string(),
        is_folder: false,
        media_type: "Video".to_string(),
        collection_type: String::new(),
        runtime_ticks: 30 * TICKS_PER_SECOND,
        played: false,
        playback_position_ticks: 0,
        series_id: String::new(),
        series_name: String::new(),
        album_id: String::new(),
        album: String::new(),
        index_number: 0,
        parent_index_number: 0,
        unplayed_item_count: 0,
        path: String::new(),
        artist: String::new(),
        sort_name: String::new(),
        production_year: 0,
        end_year: 0,
        overview: String::new(),
        premiere_date: String::new(),
        date_added: String::new(),
        total_count: 0,
        container: String::new(),
        director: String::new(),
        video_info: String::new(),
        audio_info: String::new(),
        genre: String::new(),
        playlist_item_id: String::new(),
    }
}

fn audiobookshelf_episode(library_item_id: &str, episode_id: &str) -> AudiobookshelfQueueItem {
    AudiobookshelfQueueItem {
        library_item_id: library_item_id.into(),
        episode_id: episode_id.into(),
        title: "ABS episode".into(),
        show_title: Some("Show".into()),
        author: Some("Author".into()),
        description: None,
        duration_ticks: Some(120 * TICKS_PER_SECOND as u64),
        position_ticks: 30 * TICKS_PER_SECOND,
        played: false,
        pub_date_secs: Some(1_700_000_000),
        is_finished: false,
        cover_path: Some("/covers/show.jpg".into()),
    }
}

fn audiobookshelf_book(library_item_id: &str) -> AudiobookshelfBookQueueItem {
    AudiobookshelfBookQueueItem {
        library_item_id: library_item_id.into(),
        title: "ABS book".into(),
        author: Some("Author".into()),
        duration_ticks: Some(3600 * TICKS_PER_SECOND as u64),
        position_ticks: 900 * TICKS_PER_SECOND,
        played: false,
        is_finished: false,
        cover_path: Some("/covers/book.jpg".into()),
    }
}

fn item_with_progress(id: &str, position_seconds: i64, played: bool) -> EmbyItem {
    let mut item = item(id);
    item.playback_position_ticks = position_seconds * TICKS_PER_SECOND;
    item.played = played;
    item
}

fn slot_ids(queue: &PlaybackQueue) -> Vec<QueueSlotId> {
    queue.slots().iter().map(|slot| slot.slot_id).collect()
}

#[test]
fn duplicate_item_ids_receive_distinct_queue_slot_ids() {
    let queue = PlaybackQueue::from_items(vec![item("same"), item("same")], Some(0));

    assert_ne!(queue.slots()[0].slot_id, queue.slots()[1].slot_id);
    assert_eq!(queue.slots()[0].item.id(), queue.slots()[1].item.id());
}

#[test]
fn removing_before_active_slot_preserves_active_identity() {
    let mut queue = PlaybackQueue::from_items(vec![item("a"), item("b"), item("c")], Some(2));
    let active = queue.active_slot_id().unwrap();
    let before_active = queue.slots()[0].slot_id;

    assert!(matches!(
        queue.remove_slot(before_active),
        RemoveSlotResult::Removed(_)
    ));

    assert_eq!(queue.active_slot_id(), Some(active));
    assert_eq!(queue.slot_index(active), Some(1));
}

#[test]
fn moving_slots_around_active_slot_preserves_active_identity() {
    let mut queue = PlaybackQueue::from_items(vec![item("a"), item("b"), item("c")], Some(1));
    let ids = slot_ids(&queue);
    let active = queue.active_slot_id().unwrap();

    assert!(matches!(
        queue.move_slot(ids[0], 2),
        QueueMutationResult::Applied(())
    ));
    assert_eq!(queue.active_slot_id(), Some(active));
    assert_eq!(queue.slot_index(active), Some(0));

    assert!(matches!(
        queue.move_slot(ids[2], 0),
        QueueMutationResult::Applied(())
    ));
    assert_eq!(queue.active_slot_id(), Some(active));
    assert_eq!(queue.slot_index(active), Some(1));
}

#[test]
fn moving_active_slot_keeps_active_identity_on_that_slot() {
    let mut queue = PlaybackQueue::from_items(vec![item("a"), item("b"), item("c")], Some(1));
    let active = queue.active_slot_id().unwrap();

    assert!(matches!(
        queue.move_slot(active, 0),
        QueueMutationResult::Applied(())
    ));

    assert_eq!(queue.active_slot_id(), Some(active));
    assert_eq!(queue.slot_index(active), Some(0));
}

#[test]
fn set_active_slot_targets_slot_after_reorder() {
    let mut queue = PlaybackQueue::from_items(vec![item("a"), item("b"), item("c")], Some(0));
    let target = queue.slots()[2].slot_id;

    assert!(matches!(
        queue.move_slot(target, 0),
        QueueMutationResult::Applied(())
    ));
    assert!(matches!(
        queue.set_active_slot(target),
        QueueMutationResult::Applied(())
    ));

    assert_eq!(queue.active_slot_id(), Some(target));
    assert_eq!(queue.slot_index(target), Some(0));
}

#[test]
fn consume_removes_intended_slot_occurrence() {
    let mut queue = PlaybackQueue::from_items(vec![item("same"), item("same"), item("c")], Some(2));
    let consumed = queue.slots()[1].slot_id;

    let QueueMutationResult::Applied(slot) = queue.consume_slot(consumed) else {
        panic!("expected consume to remove the slot");
    };

    assert_eq!(slot.slot_id, consumed);
    assert!(queue.slot(consumed).is_none());
    assert_eq!(queue.slots().len(), 2);
    assert_eq!(queue.slots()[0].item.id(), "same");
}

#[test]
fn progress_applies_to_intended_slot_after_index_shifts() {
    let mut queue = PlaybackQueue::from_items(vec![item("a"), item("b"), item("c")], Some(2));
    let target = queue.slots()[2].slot_id;
    let removed = queue.slots()[0].slot_id;

    assert!(matches!(
        queue.remove_slot(removed),
        RemoveSlotResult::Removed(_)
    ));
    assert!(matches!(
        queue.apply_progress(target, 12 * TICKS_PER_SECOND, false),
        QueueMutationResult::Applied(())
    ));

    assert_eq!(
        queue.slot(target).unwrap().item.playback_position_ticks(),
        12 * TICKS_PER_SECOND
    );
}

#[test]
fn progress_for_removed_slot_is_rejected() {
    let mut queue = PlaybackQueue::from_items(vec![item("a"), item("b")], Some(1));
    let removed = queue.slots()[0].slot_id;
    assert!(matches!(
        queue.remove_slot(removed),
        RemoveSlotResult::Removed(_)
    ));

    assert!(matches!(
        queue.apply_progress(removed, 12 * TICKS_PER_SECOND, false),
        QueueMutationResult::NotFound
    ));
}

#[test]
fn active_slot_progress_is_protected_from_server_refresh() {
    let mut queue = PlaybackQueue::from_items(vec![item("a"), item("b")], Some(0));
    let active = queue.active_slot_id().unwrap();
    assert!(matches!(
        queue.apply_progress(active, 20 * TICKS_PER_SECOND, false),
        QueueMutationResult::Applied(())
    ));

    let result = queue.merge_refresh(vec![
        item_with_progress("a", 3, false),
        item_with_progress("b", 4, false),
    ]);

    assert!(result.protected_slots.contains(&active));
    assert_eq!(
        queue.slot(active).unwrap().item.playback_position_ticks(),
        20 * TICKS_PER_SECOND
    );
}

#[test]
fn refresh_applies_one_fetched_item_to_duplicate_queue_slots() {
    let mut queue = PlaybackQueue::from_items(vec![item("same"), item("same")], Some(0));
    let duplicate = queue.slots()[1].slot_id;

    let result = queue.merge_refresh(vec![item_with_progress("same", 5, false)]);

    assert!(result.pruned_slots.is_empty());
    assert!(queue.slot(duplicate).is_some());
    assert_eq!(
        queue
            .slot(duplicate)
            .unwrap()
            .item
            .playback_position_ticks(),
        5 * TICKS_PER_SECOND
    );
}

#[test]
fn refresh_matches_duplicate_fetched_items_in_queue_order() {
    let mut queue = PlaybackQueue::from_items(vec![item("same"), item("same")], None);
    let first = queue.slots()[0].slot_id;
    let second = queue.slots()[1].slot_id;

    let result = queue.merge_refresh(vec![
        item_with_progress("same", 5, false),
        item_with_progress("same", 9, false),
    ]);

    assert!(result.pruned_slots.is_empty());
    assert_eq!(
        queue.slot(first).unwrap().item.playback_position_ticks(),
        5 * TICKS_PER_SECOND
    );
    assert_eq!(
        queue.slot(second).unwrap().item.playback_position_ticks(),
        9 * TICKS_PER_SECOND
    );
}

#[test]
fn pending_progress_sync_blocks_stale_server_userdata() {
    let mut queue = PlaybackQueue::from_items(vec![item("a")], Some(0));
    let slot = queue.active_slot_id().unwrap();
    assert!(matches!(
        queue.apply_progress(slot, 20 * TICKS_PER_SECOND, false),
        QueueMutationResult::Applied(())
    ));
    assert!(matches!(
        queue.mark_progress_sync_pending(slot),
        QueueMutationResult::Applied(_)
    ));

    let result = queue.merge_refresh(vec![item_with_progress("a", 2, false)]);

    assert!(result.stale_pending_slots.contains(&slot));
    assert_eq!(
        queue.slot(slot).unwrap().item.playback_position_ticks(),
        20 * TICKS_PER_SECOND
    );
    assert!(queue
        .slot(slot)
        .unwrap()
        .progress_state
        .pending_sync
        .is_some());
}

#[test]
fn active_pending_progress_confirmation_clears_pending_but_keeps_local_progress() {
    let mut queue = PlaybackQueue::from_items(vec![item("a")], Some(0));
    let active = queue.active_slot_id().unwrap();
    assert!(matches!(
        queue.apply_progress(active, 20 * TICKS_PER_SECOND, false),
        QueueMutationResult::Applied(())
    ));
    assert!(matches!(
        queue.mark_progress_sync_pending(active),
        QueueMutationResult::Applied(_)
    ));

    let result = queue.merge_refresh(vec![item_with_progress("a", 22, false)]);

    assert!(result.pending_confirmed_slots.contains(&active));
    assert!(result.protected_slots.contains(&active));
    assert!(queue
        .slot(active)
        .unwrap()
        .progress_state
        .pending_sync
        .is_none());
    assert_eq!(
        queue.slot(active).unwrap().item.playback_position_ticks(),
        20 * TICKS_PER_SECOND
    );
}

#[test]
fn pending_progress_sync_clears_when_server_position_matches_within_tolerance() {
    let mut queue = PlaybackQueue::from_items(vec![item("a")], None);
    let slot = queue.slots()[0].slot_id;
    assert!(matches!(
        queue.apply_progress(slot, 20 * TICKS_PER_SECOND, false),
        QueueMutationResult::Applied(())
    ));
    assert!(matches!(
        queue.mark_progress_sync_pending(slot),
        QueueMutationResult::Applied(_)
    ));

    let result = queue.merge_refresh(vec![item_with_progress("a", 22, false)]);

    assert!(result.pending_confirmed_slots.contains(&slot));
    assert!(queue
        .slot(slot)
        .unwrap()
        .progress_state
        .pending_sync
        .is_none());
    assert_eq!(
        queue.slot(slot).unwrap().item.playback_position_ticks(),
        22 * TICKS_PER_SECOND
    );
}

#[test]
fn watched_state_confirmation_requires_exact_match() {
    let mut queue = PlaybackQueue::from_items(vec![item("a")], Some(0));
    let slot = queue.active_slot_id().unwrap();
    assert!(matches!(
        queue.apply_progress(slot, 20 * TICKS_PER_SECOND, true),
        QueueMutationResult::Applied(())
    ));
    assert!(matches!(
        queue.mark_progress_sync_pending(slot),
        QueueMutationResult::Applied(_)
    ));

    let result = queue.merge_refresh(vec![item_with_progress("a", 20, false)]);

    assert!(result.stale_pending_slots.contains(&slot));
    assert!(queue
        .slot(slot)
        .unwrap()
        .progress_state
        .pending_sync
        .is_some());
    assert!(queue.slot(slot).unwrap().item.played());
}

#[test]
fn refresh_prunes_inactive_non_pending_missing_slots() {
    let mut queue = PlaybackQueue::from_items(vec![item("a"), item("b"), item("c")], Some(0));
    let pruned = queue.slots()[1].slot_id;

    let result = queue.merge_refresh(vec![item("a"), item("c")]);

    assert_eq!(result.pruned_slots, vec![pruned]);
    assert!(queue.slot(pruned).is_none());
    assert_eq!(queue.slots().len(), 2);
}

#[test]
fn refresh_cannot_prune_active_or_pending_sync_slots() {
    let mut queue = PlaybackQueue::from_items(vec![item("a"), item("b"), item("c")], Some(0));
    let active = queue.slots()[0].slot_id;
    let pending = queue.slots()[1].slot_id;
    assert!(matches!(
        queue.apply_progress(pending, 9 * TICKS_PER_SECOND, false),
        QueueMutationResult::Applied(())
    ));
    assert!(matches!(
        queue.mark_progress_sync_pending(pending),
        QueueMutationResult::Applied(_)
    ));

    let result = queue.merge_refresh(vec![item("c")]);

    assert!(result.protected_slots.contains(&active));
    assert!(result.protected_slots.contains(&pending));
    assert!(queue.slot(active).is_some());
    assert!(queue.slot(pending).is_some());
}

#[test]
fn active_slot_removal_requires_confirmation_decision() {
    let mut queue = PlaybackQueue::from_items(vec![item("a"), item("b")], Some(0));
    let active = queue.active_slot_id().unwrap();

    assert!(matches!(
        queue.remove_slot(active),
        RemoveSlotResult::RequiresActiveConfirmation(slot_id) if slot_id == active
    ));
    assert!(queue.slot(active).is_some());
}

#[test]
fn confirmed_active_slot_removal_clears_active_identity() {
    let mut queue = PlaybackQueue::from_items(vec![item("a"), item("b")], Some(0));
    let active = queue.active_slot_id().unwrap();

    assert!(matches!(
        queue.remove_active_slot_confirmed(active),
        RemoveSlotResult::Removed(_)
    ));

    assert!(queue.slot(active).is_none());
    assert_eq!(queue.active_slot_id(), None);
}

#[test]
fn structural_mutations_bump_revision() {
    let mut queue = PlaybackQueue::from_items(vec![item("a"), item("b")], Some(0));
    let initial = queue.revision();

    let inserted = queue.append(QueueItem::Emby(Box::new(item("c"))));
    assert!(queue.revision() > initial);
    let after_insert = queue.revision();

    assert!(matches!(
        queue.move_slot(inserted, 0),
        QueueMutationResult::Applied(())
    ));
    assert!(queue.revision() > after_insert);
    let after_move = queue.revision();

    assert!(matches!(
        queue.consume_slot(inserted),
        QueueMutationResult::Applied(_)
    ));
    assert!(queue.revision() > after_move);
}

// ---------------------------------------------------------------------------
// QueueItem persistence round-trip tests (task 3.3)
// ---------------------------------------------------------------------------

#[test]
fn queue_item_serializes_tagged() {
    let emby = QueueItem::Emby(Box::new(item("e1")));
    let json = serde_json::to_string(&emby).unwrap();
    assert!(json.contains(r#""kind":"Emby""#));
    assert!(json.contains(r#""id":"e1""#));

    let feed = QueueItem::Feed(FeedEntry {
        guid: "feed-1".into(),
        title: "Episode 1".into(),
        enclosure_url: Some("https://example.com/ep1.mp3".into()),
        link: None,
        mime_type: Some("audio/mpeg".into()),
        duration_ticks: Some(3600 * TICKS_PER_SECOND as u64),
        pub_date_secs: None,
        feed_kind: Some(crate::config::FeedKind::Audio),
        feed_id: None,
        position_ticks: 0,
        played: false,
    });
    let json = serde_json::to_string(&feed).unwrap();
    assert!(json.contains(r#""kind":"Feed""#));
    assert!(json.contains(r#""guid":"feed-1""#));
}

#[test]
fn queue_item_deserializes_tagged() {
    let json = r#"{"kind":"Emby","id":"e1","name":"Item e1","item_type":"Episode","is_folder":false,"media_type":"Video","collection_type":"","runtime_ticks":300000000,"played":false,"playback_position_ticks":0,"series_id":"","series_name":"","album_id":"","album":"","index_number":0,"parent_index_number":0,"unplayed_item_count":0,"path":"","artist":"","sort_name":"","production_year":0,"end_year":0,"overview":"","premiere_date":"","date_added":"","total_count":0,"container":"","director":"","video_info":"","audio_info":"","genre":"","playlist_item_id":""}"#;
    let qi: QueueItem = serde_json::from_str(json).unwrap();
    assert!(matches!(qi, QueueItem::Emby(_)));
    assert_eq!(qi.id(), "e1");
}

#[test]
fn queue_item_deserializes_legacy_bare_emby_item() {
    // Legacy format: bare EmbyItem object (no "kind" field)
    let json = r#"{"id":"legacy-1","name":"Legacy Item","item_type":"Episode","is_folder":false,"media_type":"Video","collection_type":"","runtime_ticks":300000000,"played":false,"playback_position_ticks":0,"series_id":"","series_name":"","album_id":"","album":"","index_number":0,"parent_index_number":0,"unplayed_item_count":0,"path":"","artist":"","sort_name":"","production_year":0,"end_year":0,"overview":"","premiere_date":"","date_added":"","total_count":0,"container":"","director":"","video_info":"","audio_info":"","genre":"","playlist_item_id":""}"#;
    let qi: QueueItem = serde_json::from_str(json).unwrap();
    assert!(matches!(qi, QueueItem::Emby(_)));
    assert_eq!(qi.id(), "legacy-1");
}

#[test]
fn queue_item_deserializes_tagged_feed() {
    let json = r#"{"kind":"Feed","guid":"feed-1","title":"Episode 1","enclosure_url":"https://example.com/ep1.mp3","link":null,"mime_type":"audio/mpeg","duration_ticks":36000000000}"#;
    let qi: QueueItem = serde_json::from_str(json).unwrap();
    assert!(matches!(qi, QueueItem::Feed(_)));
    assert_eq!(qi.id(), "feed-1");
}

#[test]
fn queue_state_round_trip_preserves_item_kind() {
    let emby_item = item("emby-1");
    let feed_entry = FeedEntry {
        guid: "feed-1".into(),
        title: "Podcast Episode".into(),
        enclosure_url: Some("https://example.com/ep1.mp3".into()),
        link: None,
        mime_type: Some("audio/mpeg".into()),
        duration_ticks: Some(3600 * TICKS_PER_SECOND as u64),
        pub_date_secs: None,
        feed_kind: Some(crate::config::FeedKind::Audio),
        feed_id: None,
        position_ticks: 0,
        played: false,
    };
    let queue_items = vec![
        QueueItem::Emby(Box::new(emby_item.clone())),
        QueueItem::Feed(feed_entry.clone()),
    ];

    // Serialize
    let json = serde_json::to_string(&queue_items).unwrap();

    // Deserialize
    let restored: Vec<QueueItem> = serde_json::from_str(&json).unwrap();
    assert_eq!(restored.len(), 2);

    // Emby kind preserved
    match &restored[0] {
        QueueItem::Emby(e) => assert_eq!(e.id, "emby-1"),
        QueueItem::Feed(_) => panic!("expected Emby variant"),
        QueueItem::Audiobookshelf(_) => panic!("expected Emby variant"),
        QueueItem::AudiobookshelfBook(_) => panic!("expected Emby variant"),
    }

    // Feed kind preserved
    match &restored[1] {
        QueueItem::Emby(_) => panic!("expected Feed variant"),
        QueueItem::Feed(f) => {
            assert_eq!(f.guid, "feed-1");
            assert_eq!(f.title, "Podcast Episode");
            assert_eq!(
                f.enclosure_url.as_deref(),
                Some("https://example.com/ep1.mp3")
            );
        }
        QueueItem::Audiobookshelf(_) => panic!("expected Feed variant"),
        QueueItem::AudiobookshelfBook(_) => panic!("expected Feed variant"),
    }
}

#[test]
fn audiobookshelf_identity_and_mixed_queue_round_trip_are_typed() {
    let first = QueueItem::Audiobookshelf(audiobookshelf_episode("library-a", "episode-1"));
    let second = first.clone();
    assert_eq!(first.content_id(), second.content_id());

    let mut queue = PlaybackQueue::from_queue_items(
        vec![
            QueueItem::Emby(Box::new(item("emby-1"))),
            first,
            QueueItem::Feed(feed("feed-1")),
            second,
        ],
        Some(1),
    );
    assert_ne!(queue.slots()[1].slot_id, queue.slots()[3].slot_id);
    assert!(matches!(
        queue.move_slot(queue.slots()[3].slot_id, 0),
        QueueMutationResult::Applied(())
    ));
    assert_eq!(queue.active_index(), Some(2));

    let json = serde_json::to_string(
        &queue
            .slots()
            .iter()
            .map(|slot| &slot.item)
            .collect::<Vec<_>>(),
    )
    .unwrap();
    assert!(json.contains(r#""kind":"Audiobookshelf""#));
    assert!(!json.contains("api_key"));
    assert!(!json.contains("sessionId"));
    assert!(!json.contains("resolved_url"));
    let restored: Vec<QueueItem> = serde_json::from_str(&json).unwrap();
    assert!(matches!(restored[0], QueueItem::Audiobookshelf(_)));
    assert!(matches!(restored[2], QueueItem::Audiobookshelf(_)));
}

#[test]
fn refresh_preserves_inactive_audiobookshelf_book_slot() {
    // Both Audiobookshelf queue-item shapes (episode + book) must survive an
    // Emby-only refresh untouched, retaining slot, order, and progress state.
    let mut queue = PlaybackQueue::from_queue_items(
        vec![
            QueueItem::Emby(Box::new(item("emby-1"))),
            QueueItem::Feed(feed("feed-1")),
        ],
        Some(0),
    );
    let episode_slot = queue.append(QueueItem::Audiobookshelf(audiobookshelf_episode(
        "library-a",
        "episode-1",
    )));
    let book_slot = queue.append(QueueItem::AudiobookshelfBook(audiobookshelf_book(
        "library-book-1",
    )));
    let ids = slot_ids(&queue);

    let result = queue.merge_refresh(vec![item_with_progress("emby-1", 7, false)]);

    assert!(result.pruned_slots.is_empty());
    assert_eq!(slot_ids(&queue), ids);
    assert!(matches!(
        queue.slot(episode_slot).unwrap().item,
        QueueItem::Audiobookshelf(_)
    ));
    assert!(matches!(
        queue.slot(book_slot).unwrap().item,
        QueueItem::AudiobookshelfBook(ref book) if book.position_ticks == 900 * TICKS_PER_SECOND
    ));
    assert_eq!(
        queue
            .slot(episode_slot)
            .unwrap()
            .item
            .playback_position_ticks(),
        30 * TICKS_PER_SECOND
    );
}

#[test]
fn audiobookshelf_admission_and_purge_keep_other_kinds() {
    let abs = QueueItem::Audiobookshelf(audiobookshelf_episode("library-a", "episode-1"));
    assert!(!abs.admissible_for_owner(false, |_| true));
    assert!(!abs.admissible_for_owner(true, |_| false));
    assert!(QueueItem::Feed(feed("feed-1")).admissible_for_owner(true, |_| false));

    let state = crate::config::QueueState {
        source: crate::config::QueueSource::Unknown,
        items: vec![
            QueueItem::Emby(Box::new(item("emby-1"))),
            abs,
            QueueItem::AudiobookshelfBook(audiobookshelf_book("library-book-1")),
            QueueItem::Feed(feed("feed-1")),
        ],
        cursor: 1,
        last_played_content_id: None,
        last_played_item_id: None,
        last_played_completed: false,
        positions: Default::default(),
    };
    let filtered = state.without_audiobookshelf();
    assert_eq!(filtered.items.len(), 2);
    assert!(filtered
        .items
        .iter()
        .all(|item| !item.is_audiobookshelf_any()));
    assert!(filtered.items.iter().any(QueueItem::is_emby));
    assert!(filtered.items.iter().any(QueueItem::is_feed));
}

#[test]
fn queue_state_legacy_bare_items_load_as_emby() {
    // Simulate a legacy queue_state.json where items are bare EmbyItem objects
    let legacy_json = r#"[{"id":"old-1","name":"Old Item","item_type":"Episode","is_folder":false,"media_type":"Video","collection_type":"","runtime_ticks":300000000,"played":false,"playback_position_ticks":0,"series_id":"","series_name":"","album_id":"","album":"","index_number":0,"parent_index_number":0,"unplayed_item_count":0,"path":"","artist":"","sort_name":"","production_year":0,"end_year":0,"overview":"","premiere_date":"","date_added":"","total_count":0,"container":"","director":"","video_info":"","audio_info":"","genre":"","playlist_item_id":""}]"#;
    let restored: Vec<QueueItem> = serde_json::from_str(legacy_json).unwrap();
    assert_eq!(restored.len(), 1);
    assert!(matches!(&restored[0], QueueItem::Emby(e) if e.id == "old-1"));
}

fn feed(guid: &str) -> FeedEntry {
    FeedEntry {
        guid: guid.to_string(),
        title: format!("Feed {guid}"),
        enclosure_url: Some(format!("https://example.com/{guid}.mp3")),
        link: None,
        mime_type: Some("audio/mpeg".into()),
        duration_ticks: Some(60 * TICKS_PER_SECOND as u64),
        pub_date_secs: None,
        feed_kind: Some(crate::config::FeedKind::Audio),
        feed_id: None,
        position_ticks: 0,
        played: false,
    }
}

#[test]
fn feed_slot_participates_in_queue_ordering_and_survives_refresh() {
    let mut queue = PlaybackQueue::from_items(vec![item("a"), item("b")], Some(0));
    let feed_slot = queue.append(QueueItem::Feed(feed("f1")));

    // The Feed slot holds its own identity alongside the Emby slots.
    assert_eq!(queue.slots().last().unwrap().slot_id, feed_slot);
    assert_eq!(queue.slots().last().unwrap().item.id(), "f1");

    assert!(matches!(
        queue.set_active_slot(feed_slot),
        QueueMutationResult::Applied(())
    ));
    assert_eq!(queue.active_slot_id(), Some(feed_slot));

    assert!(matches!(
        queue.move_slot(feed_slot, 0),
        QueueMutationResult::Applied(())
    ));
    assert_eq!(queue.slots()[0].slot_id, feed_slot);

    // Feed slots have no server-side counterpart; a refresh must leave
    // them in place rather than pruning them.
    let result = queue.merge_refresh(vec![item("a"), item("b")]);
    assert!(result.pruned_slots.is_empty());
    assert!(queue.slot(feed_slot).is_some());
    assert!(matches!(
        queue.slot(feed_slot).unwrap().item,
        QueueItem::Feed(_)
    ));
}

include!("playback_queue_tests_feed.rs");

// ---------------------------------------------------------------------------
// PlaybackQueue operation tests (task 1.2)
// ---------------------------------------------------------------------------

#[test]
fn replace_clears_queue_and_sets_new_items() {
    let mut queue = PlaybackQueue::from_items(vec![item("a"), item("b")], Some(0));
    let initial = queue.revision();
    let old_active = queue.replace(vec![
        QueueItem::Emby(Box::new(item("x"))),
        QueueItem::Feed(feed("f1")),
        QueueItem::Emby(Box::new(item("y"))),
    ]);

    assert_eq!(old_active, Some(0));
    assert_eq!(queue.len(), 3);
    assert_eq!(queue.slots()[0].item.id(), "x");
    assert_eq!(queue.slots()[1].item.id(), "f1");
    assert_eq!(queue.slots()[2].item.id(), "y");
    assert_eq!(queue.active_slot_id(), None);
    assert!(queue.revision() > initial);
}

#[test]
fn replace_with_empty_vec_clears() {
    let mut queue = PlaybackQueue::from_items(vec![item("a")], Some(0));
    let old_active = queue.replace(vec![]);

    assert_eq!(old_active, Some(0));
    assert!(queue.is_empty());
    assert_eq!(queue.active_slot_id(), None);
}

#[test]
fn clear_removes_all_slots_and_bumps_revision() {
    let mut queue = PlaybackQueue::from_items(vec![item("a"), item("b"), item("c")], Some(1));
    let initial = queue.revision();
    queue.clear();

    assert!(queue.is_empty());
    assert_eq!(queue.len(), 0);
    assert_eq!(queue.active_slot_id(), None);
    assert!(queue.revision() > initial);
}

#[test]
fn clear_on_empty_queue_is_noop() {
    let mut queue = PlaybackQueue::default();
    let before = queue.revision();
    queue.clear();

    assert!(queue.is_empty());
    assert_eq!(queue.revision(), before);
}

#[test]
fn len_and_active_index_reflect_queue_state() {
    let mut queue = PlaybackQueue::from_items(vec![item("a"), item("b"), item("c")], Some(1));

    assert_eq!(queue.len(), 3);
    assert_eq!(queue.active_index(), Some(1));

    let target = queue.slots()[2].slot_id;
    queue.set_active_slot(target);
    assert_eq!(queue.active_index(), Some(2));

    queue.clear_active_slot();
    assert_eq!(queue.active_index(), None);
}

#[test]
fn mixed_queue_replace_preserves_item_variants() {
    let mut queue = PlaybackQueue::default();
    queue.replace(vec![
        QueueItem::Feed(feed("f1")),
        QueueItem::Emby(Box::new(item("e1"))),
        QueueItem::Feed(feed("f2")),
    ]);

    assert!(matches!(queue.slots()[0].item, QueueItem::Feed(_)));
    assert!(matches!(queue.slots()[1].item, QueueItem::Emby(_)));
    assert!(matches!(queue.slots()[2].item, QueueItem::Feed(_)));
}
