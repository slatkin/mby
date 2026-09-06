
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

