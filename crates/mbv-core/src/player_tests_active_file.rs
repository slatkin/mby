fn test_mpv() -> Mpv {
    let env_lock = crate::config::tests::SYS_ENV_LOCK.lock().unwrap();
    let result = init_mpv(&MpvRunConfig {
        headless: true,
        use_mpv_config: false,
        no_scripts: true,
        always_skip_intro: false,
        audio_pipe_path: None,
        audio_pipe_samplerate: 0,
        audio_pipe_bitdepth: 0,
        audio_device: None,
    });
    let mpv = result.unwrap().0;
    mpv.set_property("ao", "null").unwrap();
    drop(env_lock);
    mpv
}

fn noop_progress() -> ProgressGuard {
    let (stop_tx, _) = mpsc::channel();
    ProgressGuard {
        stop_tx,
        handle: None,
    }
}

fn abs_item() -> QueueItem {
    QueueItem::Audiobookshelf(crate::playback_queue::AudiobookshelfQueueItem {
        library_item_id: "show".into(),
        episode_id: "episode".into(),
        title: "Episode".into(),
        show_title: None,
        author: None,
        description: None,
        duration_ticks: Some(100_u64),
        position_ticks: 0,
        played: false,
        pub_date_secs: None,
        is_finished: false,
        cover_path: None,
    })
}

fn abs_book_item() -> QueueItem {
    QueueItem::AudiobookshelfBook(crate::playback_queue::AudiobookshelfBookQueueItem {
        library_item_id: "book".into(),
        title: "Book".into(),
        author: None,
        duration_ticks: Some(100_u64),
        position_ticks: 0,
        played: false,
        is_finished: false,
        cover_path: None,
    })
}

#[test]
fn failed_eager_transition_preserves_canonical_queue_and_mode() {
    let (mut run, _) = make_queue_session_for_pos_tests(1);
    let active_abs = run.queue.append(abs_item());
    let _ = run.queue.set_active_slot(active_abs);
    run.refresh_current_idx_from_queue();
    let old_slots: Vec<_> = run.queue.slots().iter().map(|slot| slot.slot_id).collect();
    let old_active = run.active_slot_id();
    let mpv = test_mpv();

    run.cmd_append_queue(vec![abs_item()], &mpv);

    assert_eq!(
        run.queue
            .slots()
            .iter()
            .map(|slot| slot.slot_id)
            .collect::<Vec<_>>(),
        old_slots
    );
    assert_eq!(run.active_slot_id(), old_active);
    assert!(!run.active_file);
}

#[test]
fn replacement_prepare_failure_accepts_new_stopped_queue_and_clears_mpv() {
    let (mut run, status, events) = make_queue_session_for_pos_tests_with_events(0);
    let mut replacement = abs_item();
    let QueueItem::Audiobookshelf(item) = &mut replacement else {
        unreachable!()
    };
    item.title = "Replacement".into();
    item.duration_ticks = Some(900);
    item.position_ticks = 123;
    let mpv = test_mpv();
    mpv.command("loadfile", &["av://lavfi:sine=frequency=1000", "replace"])
        .unwrap();
    assert_eq!(mpv.get_property::<i64>("playlist-count").unwrap(), 1);
    let mut progress = noop_progress();

    run.replace_with_queue_items(vec![replacement], 0, &mpv, &mut progress);

    assert_eq!(run.queue_len(), 1);
    assert_eq!(run.active_item().unwrap().title(), "Replacement");
    assert_eq!(run.current_idx, 0);
    assert!(run.active_file);
    assert_eq!(run.active_item().unwrap().id(), "episode");
    assert_eq!(mpv.get_property::<i64>("playlist-count").unwrap(), 0);
    let status = status.lock().unwrap();
    assert!(!status.active);
    assert_eq!(status.current_idx, 0);
    assert_eq!(status.queue_len, 1);
    assert_eq!(status.position_ticks, 123);
    assert_eq!(status.last_valid_pos, 123);
    assert_eq!(status.runtime_ticks, 900);
    assert_eq!(status.title, "Replacement");
    drop(status);
    let event = events.recv().unwrap();
    let PlayerEvent::Stopped {
        idx,
        position_ticks,
        error,
        ..
    } = event
    else {
        panic!("expected replacement failure stop event");
    };
    assert_eq!(idx, 0);
    assert_eq!(position_ticks, 123);
    assert_eq!(
        error.as_deref(),
        Some("failed to prepare media: service unavailable")
    );
    assert!(events.try_recv().is_err());
}

#[test]
fn active_file_replacement_uses_canonical_item_generic_path_and_one_mpv_entry() {
    let (mut run, status) = make_queue_session_for_pos_tests(0);
    run.active_file = true;
    let mpv = test_mpv();
    let mut progress = noop_progress();
    let items = vec![
        QueueItem::Emby(Box::new(make_media_item("replacement-a"))),
        QueueItem::Emby(Box::new(make_media_item("replacement-b"))),
    ];

    run.replace_with_queue_items(items, 1, &mpv, &mut progress);

    assert!(run.active_file);
    assert_eq!(run.queue_len(), 2);
    assert_eq!(run.current_idx, 1);
    assert_eq!(mpv.get_property::<i64>("playlist-count").unwrap(), 1);
    assert!(status.lock().unwrap().active);
}

#[test]
fn asynchronous_active_file_start_error_stops_and_preserves_canonical_queue() {
    let (mut run, status) = make_queue_session_for_pos_tests(0);
    run.active_file = true;
    run.active_file_starting = true;
    let slots: Vec<_> = run.queue.slots().iter().map(|slot| slot.slot_id).collect();
    let active_slot = run.active_slot_id();
    let mpv = test_mpv();
    let mut progress = noop_progress();

    assert!(!run.on_end_file(mpv_end_file_reason::Error, &mpv, &mut progress));

    assert!(!status.lock().unwrap().active);
    assert_eq!(
        run.queue
            .slots()
            .iter()
            .map(|slot| slot.slot_id)
            .collect::<Vec<_>>(),
        slots
    );
    assert_eq!(run.active_slot_id(), active_slot);
    assert!(run.prepared_source.is_none());
}

#[test]
fn active_file_preparation_finalizes_before_opening_next_session() {
    let (first_base, first_requests) = super::source_tests::serve_close(1.0);
    let (second_base, second_requests) = super::source_tests::serve_close(1.0);
    let first_context = AudiobookshelfPlayerContext::new(
        crate::service_runtime::SetupGeneration::new(12),
        crate::config::AudiobookshelfSetup::new(first_base),
        "secret".into(),
        "device".into(),
    )
    .unwrap();
    let first = prepare_source(&abs_item(), "", "", Some(&first_context)).unwrap();
    let (mut run, _) = make_queue_session_for_pos_tests(0);
    run.prepared_source = Some(first);
    run.audiobookshelf_context = Some(
        AudiobookshelfPlayerContext::new(
            crate::service_runtime::SetupGeneration::new(13),
            crate::config::AudiobookshelfSetup::new(second_base),
            "secret".into(),
            "device".into(),
        )
        .unwrap(),
    );

    let second = run.prepare_item(&abs_item()).unwrap();
    let first_requests: Vec<_> = (0..3).map(|_| first_requests.recv().unwrap()).collect();
    assert!(first_requests[0].starts_with("POST /api/items/show/play/episode HTTP/1.1"));
    assert!(first_requests[1].starts_with("POST /api/session/%3CSESSION_ID%3E/sync HTTP/1.1"));
    assert!(first_requests[2].starts_with("POST /api/session/%3CSESSION_ID%3E/close HTTP/1.1"));
    assert!(second_requests
        .recv()
        .unwrap()
        .starts_with("POST /api/items/show/play/episode HTTP/1.1"));
    drop(second);
    assert!(second_requests
        .recv()
        .unwrap()
        .starts_with("POST /api/session/%3CSESSION_ID%3E/sync HTTP/1.1"));
    assert!(second_requests
        .recv()
        .unwrap()
        .starts_with("POST /api/session/%3CSESSION_ID%3E/close HTTP/1.1"));
}
