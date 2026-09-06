// Task 2.3: shared-boundary routing and failure surfacing.
// Verifies that bare-local and stay-alive-local playback routes through
// `submit_queue`, that the fast path sends a SubmitQueue command, that
// the cold path sets status.active before spawning, and that selecting
// an existing Feed slot in a mixed queue preserves queue contents.

#[test]
fn submit_queue_fast_path_sends_command_for_feed_entry() {
    // When the player is already active with matching headless state,
    // submit_queue must route through the SubmitQueue command (fast path)
    // rather than spawning a new thread. This proves bare-local and
    // stay-alive-local playback share the same boundary.
    let (event_tx, _event_rx) = mpsc::channel();
    let player = Player::new(
        String::new(),
        String::new(),
        false,
        false,
        false,
        false,
        SubtitlePrefs::default(),
        event_tx,
        None,
    );
    player.status.lock().unwrap().active = true;
    player.current_is_headless.store(false, Ordering::Relaxed);
    let cmd_rx = player.spy_on_commands();

    let entry = make_feed_entry("feed-1", "Podcast Episode 1");
    player.submit_queue(vec![QueueItem::Feed(entry)], 0, None, false, 100);

    let cmd = cmd_rx
        .try_recv()
        .expect("expected a command from submit_queue");
    match cmd {
        PlayerCommand::SubmitQueue { items, start_idx } => {
            assert_eq!(items.len(), 1);
            assert_eq!(start_idx, 0);
            assert!(matches!(&items[0], QueueItem::Feed(e) if e.guid == "feed-1"));
        }
        _ => panic!("expected SubmitQueue command"),
    }
}

#[test]
fn submit_queue_fast_path_updates_status_before_sending() {
    // The status must reflect the new queue items before the command is
    // sent, so any reader that sees status.active = true also sees
    // consistent queue_len and title.
    let (event_tx, _event_rx) = mpsc::channel();
    let player = Player::new(
        String::new(),
        String::new(),
        false,
        false,
        false,
        false,
        SubtitlePrefs::default(),
        event_tx,
        None,
    );
    player.status.lock().unwrap().active = true;
    player.current_is_headless.store(false, Ordering::Relaxed);
    let _cmd_rx = player.spy_on_commands();

    let entry = make_feed_entry("feed-test", "Test Episode");
    player.submit_queue(vec![QueueItem::Feed(entry)], 0, None, false, 100);

    let st = player.status.lock().unwrap();
    assert_eq!(st.queue_len, 1);
    assert_eq!(st.current_idx, 0);
    assert_eq!(st.title, "Test Episode");
    assert!(!st.paused);
}

#[test]
fn selecting_existing_feed_slot_preserves_mixed_queue() {
    // Regression: selecting an existing Feed slot in a mixed queue must
    // preserve queue length and order — the correct operation is slot
    // selection (set_active_slot), not queue replacement.
    let mut queue = PlaybackQueue::default();
    let emby_slot = queue.append(QueueItem::Emby(Box::new(EmbyItem {
        id: "emby-1".into(),
        name: "Emby Item".into(),
        item_type: "Episode".into(),
        is_folder: false,
        media_type: "Video".into(),
        collection_type: String::new(),
        runtime_ticks: 30 * crate::api::TICKS_PER_SECOND,
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
    })));
    let feed_slot = queue.append(QueueItem::Feed(make_feed_entry("podcast-ep", "Podcast Ep")));
    let _other = queue.append(QueueItem::Feed(make_feed_entry("other-ep", "Other Ep")));
    assert_eq!(queue.len(), 3);

    // Select the existing Feed slot — this is the correct operation for
    // playing an item already in the queue (JumpTo), not queue replacement.
    let result = queue.set_active_slot(feed_slot);
    assert!(
        matches!(result, QueueMutationResult::Applied(())),
        "set_active_slot must succeed for an existing slot"
    );

    // Queue contents are unchanged: same length, same order, same slot IDs.
    assert_eq!(queue.len(), 3, "queue length must be preserved");
    assert_eq!(
        queue.slots().iter().map(|s| s.slot_id).collect::<Vec<_>>(),
        vec![emby_slot, feed_slot, _other],
        "slot IDs and order must be preserved"
    );
    assert_eq!(
        queue.active_slot_id(),
        Some(feed_slot),
        "the selected Feed slot must be active"
    );
    assert_eq!(
        queue.active_index(),
        Some(1),
        "active index points to the Feed slot"
    );
}

#[test]
fn submit_queue_cold_start_sets_active_before_spawning_thread() {
    // When submit_queue cold-starts (player inactive), it must set
    // status.active = true before spawning the player thread. This
    // ensures that any subsequent submit_queue call sees active = true
    // and takes the fast path rather than spawning a second thread.
    let (event_tx, _event_rx) = mpsc::channel();
    let player = Player::new(
        String::new(),
        String::new(),
        false,
        false,
        false,
        false,
        SubtitlePrefs::default(),
        event_tx,
        None,
    );
    assert!(
        !player.status.lock().unwrap().active,
        "player must start inactive"
    );

    let entry = make_feed_entry("cold-feed", "Cold Feed");
    player.submit_queue(vec![QueueItem::Feed(entry)], 0, None, false, 100);

    // The cold path sets active = true before spawning the thread, so
    // the status should reflect the new queue immediately.
    let st = player.status.lock().unwrap();
    assert!(st.active, "cold start must set active = true");
    assert_eq!(st.queue_len, 1);
    assert_eq!(st.current_idx, 0);
    assert_eq!(st.title, "Cold Feed");
}

fn audiobookshelf_item() -> QueueItem {
    QueueItem::Audiobookshelf(crate::playback_queue::AudiobookshelfQueueItem {
        library_item_id: "show-1".into(),
        episode_id: "episode-1".into(),
        title: "Episode 1".into(),
        show_title: Some("Show".into()),
        author: None,
        description: None,
        duration_ticks: Some(100),
        position_ticks: 0,
        played: false,
        pub_date_secs: None,
        is_finished: false,
        cover_path: None,
    })
}

fn audiobookshelf_context() -> AudiobookshelfPlayerContext {
    AudiobookshelfPlayerContext::new(
        crate::service_runtime::SetupGeneration::new(7),
        crate::config::AudiobookshelfSetup::new("https://books.example"),
        "secret".into(),
        "device".into(),
    )
    .unwrap()
}

#[test]
fn complete_bare_player_admits_audiobookshelf_without_ctrl_transport() {
    let (event_tx, _event_rx) = mpsc::channel();
    let player = Player::new(
        String::new(),
        String::new(),
        false,
        false,
        false,
        false,
        SubtitlePrefs::default(),
        event_tx,
        None,
    );
    player.update_audiobookshelf_context(Some(audiobookshelf_context()));
    player.status.lock().unwrap().active = true;
    let commands = player.spy_on_commands();

    assert!(player.can_admit_audiobookshelf());
    assert!(player.submit_queue(vec![audiobookshelf_item()], 0, None, false, 100));
    assert!(matches!(
        commands.try_recv().unwrap(),
        PlayerCommand::SubmitQueue { items, start_idx }
            if start_idx == 0 && items.len() == 1 && items[0].is_audiobookshelf()
    ));
}

#[test]
fn init_mpv_projects_mutually_exclusive_output() {
    // Real (headless) mpv init, same pattern as player_tests_active_file.rs's
    // test_mpv() -- serialized on SYS_ENV_LOCK because init_mpv reads env
    // vars for its config/ipc paths.
    let env_lock = crate::config::tests::SYS_ENV_LOCK.lock().unwrap();

    // ALSA mode: only the selected device is projected; the pipe startup
    // guard never arms.
    let (mpv, startup_pause_armed) = init_mpv(&MpvRunConfig {
        headless: true,
        use_mpv_config: false,
        no_scripts: true,
        always_skip_intro: false,
        audio_pipe_path: None,
        audio_pipe_samplerate: 0,
        audio_pipe_bitdepth: 0,
        audio_device: Some("alsa".to_string()),
    })
    .unwrap();
    assert!(
        !startup_pause_armed,
        "clocked ALSA output must not arm the pipe startup guard"
    );
    assert_eq!(mpv.get_property::<String>("audio-device").unwrap(), "alsa");
    mpv.set_property("ao", "null").unwrap(); // avoid opening real hardware for the rest of the suite
    drop(mpv);

    // Explicit pipe mode: existing pipe properties are preserved and
    // audio_device is ignored.
    let pipe_path = std::env::temp_dir()
        .join(format!("mbv-test-init-mpv-pipe-{}", uuid::Uuid::new_v4()))
        .display()
        .to_string();
    let (mpv, startup_pause_armed) = init_mpv(&MpvRunConfig {
        headless: true,
        use_mpv_config: false,
        no_scripts: true,
        always_skip_intro: false,
        audio_pipe_path: Some(pipe_path.clone()),
        audio_pipe_samplerate: 48_000,
        audio_pipe_bitdepth: 16,
        audio_device: Some("alsa/hw:Loopback,0,0".to_string()),
    })
    .unwrap();
    assert!(
        startup_pause_armed,
        "explicit pipe output must still arm the startup guard"
    );
    assert_eq!(mpv.get_property::<String>("ao").unwrap(), "pcm");
    assert_ne!(
        mpv.get_property::<String>("audio-device").unwrap(),
        "alsa/hw:Loopback,0,0",
        "audio_device must be ignored while pipe output is selected"
    );
    let _ = std::fs::remove_file(&pipe_path);

    drop(env_lock);
}

#[test]
fn init_mpv_headless_disables_cover_art_display() {
    // Issue #656: headless playback (vo=null) must not select and decode
    // attached cover art (video/image=true per audio track). Real (headless)
    // mpv init, serialized on SYS_ENV_LOCK like the other init_mpv tests.
    let env_lock = crate::config::tests::SYS_ENV_LOCK.lock().unwrap();
    let (mpv, _) = init_mpv(&MpvRunConfig {
        headless: true,
        use_mpv_config: false,
        no_scripts: true,
        always_skip_intro: false,
        audio_pipe_path: None,
        audio_pipe_samplerate: 0,
        audio_pipe_bitdepth: 0,
        audio_device: None,
    })
    .unwrap();
    assert_eq!(mpv.get_property::<String>("audio-display").unwrap(), "no");
    mpv.set_property("ao", "null").unwrap();
    drop(mpv);
    drop(env_lock);
}

#[test]
fn mpv_audio_errors_only_classify_alsa_initialization_failures() {
    assert!(is_clocked_audio_error(
        &libmpv2::Error::Raw(libmpv2::mpv_error::AoInitFailed),
        true,
    ));
    assert!(!is_clocked_audio_error(
        &libmpv2::Error::Raw(libmpv2::mpv_error::LoadingFailed),
        true,
    ));
    assert!(!is_clocked_audio_error(
        &libmpv2::Error::Raw(libmpv2::mpv_error::AoInitFailed),
        false,
    ));
}

#[test]
fn alsa_initialization_error_stops_run_with_output_error() {
    let (mut run, status, events) = make_queue_session_for_pos_tests_with_events(0);
    run.config.audio_pipe_path = None;
    run.config.audio_device = Some("alsa/hw:Loopback,0,0".into());
    run.reporter.clear_session();
    let mut progress = noop_progress();

    assert!(run.on_mpv_error(
        libmpv2::Error::Raw(libmpv2::mpv_error::AoInitFailed),
        &mut progress,
    ));
    assert!(!status.lock().unwrap().active);
    let PlayerEvent::Stopped { error, .. } = events.recv().unwrap() else {
        panic!("expected stopped event");
    };
    assert_eq!(
        error.as_deref(),
        Some("audio output failed to start (device: alsa/hw:Loopback,0,0)")
    );
}

#[test]
fn media_end_file_error_is_not_reported_as_alsa_failure() {
    let (mut run, _status, events) = make_queue_session_for_pos_tests_with_events(0);
    run.origin = PlaybackOrigin::Standalone;
    run.config.audio_pipe_path = None;
    run.config.audio_device = Some("alsa".into());
    run.reporter.clear_session();
    let mpv = test_mpv();
    let mut progress = noop_progress();

    assert!(!run.on_end_file(mpv_end_file_reason::Error, &mpv, &mut progress));
    let PlayerEvent::Stopped { error, .. } = events.recv().unwrap() else {
        panic!("expected stopped event");
    };
    assert!(error.is_none());
}

#[test]
fn context_loss_rejects_audiobookshelf_without_mutating_bound_submission() {
    let (event_tx, _event_rx) = mpsc::channel();
    let player = Player::new(
        String::new(),
        String::new(),
        false,
        false,
        false,
        false,
        SubtitlePrefs::default(),
        event_tx,
        None,
    );
    player.update_audiobookshelf_context(Some(audiobookshelf_context()));
    player.status.lock().unwrap().active = true;
    let commands = player.spy_on_commands();
    player.update_audiobookshelf_context(None);

    assert!(!player.can_admit_audiobookshelf());
    assert!(!player.submit_queue(vec![audiobookshelf_item()], 0, None, false, 100));
    assert!(commands.try_recv().is_err());
}
