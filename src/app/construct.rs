use super::types_playback::{PlayheadProjection, QueueScope};
use super::types_player_tab::PlayerTab;
use super::types_settings::{PanelFocus, PanelMode};
use super::types_tab_selection::TabSelection;
use super::{
    bootstrap_local_daemon_queue, bootstrap_unified_queue, layout, render, spawn_resize_worker,
    App, AppInit, SessionEvent, LEFT_WIDTH_DEFAULT,
};
use mbv_core::api::{EmbyClient, EmbyItem};
use mbv_core::player::{Player, PlayerEvent, PlayerProxy};
use mbv_core::remote_player::DaemonEndpoint;
use mbv_core::service_runtime::{AudiobookshelfRuntime, EmbyRuntime};
use ratatui_image::picker::Picker;
use std::sync::{mpsc, Arc, Mutex};
use std::time::{Duration, Instant};

impl App {
    pub(super) fn build(init: AppInit) -> Self {
        // Must run before `load_prefs()`: the guard redirects `config_dir()`/
        // `state_dir()` to an isolated tmpdir, and `load_prefs()` resolves
        // its path through that same lookup. Installing the guard after
        // reading prefs left tests reading (and initializing state from)
        // the real on-disk prefs.json instead of a fresh one.
        #[cfg(test)]
        let _test_state_dir_guard = crate::config::TestStateDirGuard::new_if_unset();
        let prefs = Self::load_prefs();
        let (resize_register_tx, resize_response_rx) = spawn_resize_worker();
        let (cast_tx, cast_rx) = mpsc::channel();
        let mut app = App {
            #[cfg(test)]
            _test_state_dir_guard,
            config: init.config,
            emby_runtime: init.emby_runtime,
            audiobookshelf_runtime: init.audiobookshelf_runtime,
            emby_startup_rx: init.emby_startup_rx,
            emby_startup_request: init.emby_startup_request,
            audiobookshelf_startup_rx: init.audiobookshelf_startup_rx,
            audiobookshelf_startup_request: init.audiobookshelf_startup_request,
            audiobookshelf_catalog_rx: None,
            audiobookshelf_libraries: Vec::new(),
            audiobookshelf_shelf_cache: std::collections::HashMap::new(),
            audiobookshelf_browse: Vec::new(),
            audiobookshelf_book_browse: Vec::new(),
            audiobookshelf_test_rx: init.audiobookshelf_test_rx,
            audiobookshelf_setup_rx: init.audiobookshelf_setup_rx,
            emby_setup_form: init.emby_setup_form,
            audiobookshelf_setup_form: None,
            emby_setup_rx: init.emby_setup_rx,
            pending_emby_replacement: None,
            pending_audiobookshelf_replacement: None,
            shared_client: None,
            shared_reconnect_rx: None,
            player: init.player,
            mpris: None,
            player_rx: init.player_rx,
            ws_rx: init.ws_rx,
            ws_send_tx: init.ws_send_tx,
            audiobookshelf_socket_rx: init.audiobookshelf_socket_rx,
            audiobookshelf_socket_tx: init.audiobookshelf_socket_tx,
            audiobookshelf_socket_generation: init.audiobookshelf_socket_generation,
            player_tab: init.player_tab,
            remote_player_tab: init.remote_player_tab,
            system_notifications: init.system_notifications,
            image_protocol: init.image_protocol,
            image_protocol_enabled: init.image_protocol_enabled,
            library_position_state: crate::config::load_library_position_state(),
            hidden_libraries: init.hidden_libraries,
            library_routes: init.library_routes,
            hidden_latest: init.hidden_latest,
            music_levels: init.music_levels,
            album_indexes: std::collections::HashMap::new(),
            use_nerd_fonts: init.use_nerd_fonts,
            indicator_style: init.indicator_style,
            image_cache_size: init.image_cache_size,
            lib_tx: init.lib_tx,
            lib_rx: init.lib_rx,
            search_tx: init.search_tx,
            search_rx: init.search_rx,
            sessions_tx: init.sessions_tx,
            sessions_rx: init.sessions_rx,
            card_image_tx: init.card_image_tx,
            card_image_rx: init.card_image_rx,
            resize_register_tx,
            resize_response_rx,
            notif_action_tx: init.notif_action_tx,
            notif_action_rx: init.notif_action_rx,
            libs: Vec::new(),
            status: String::new(),
            status_expires: None,
            status_severity: super::notify_actions::ToastSeverity::default(),
            layout: layout::AppLayout::default(),
            terminal_width: 80,
            terminal_height: 24,

            last_space_press: None,
            last_esc_press: None,
            pending_overlay: None,
            pending_exit_message: None,
            pending_delete_slot: None,
            pending_queue_removal: None,
            queue_undo_stack: Vec::new(),
            remote_queue_undo_stack: Vec::new(),
            pending_remote_move_cursor: None,
            pending_queue_edit_cursor: None,
            playhead: PlayheadProjection::new(),
            next_up_item: None,
            // #361: read the new prefs key, falling back to the pre-#361 one
            // for one release. `power_focus`/`power_left_tab`/`power_left_width`
            // on disk are renamed to `panel_focus`/`library_tab`/`queue_column_width`;
            // this fallback can be deleted a release after that lands.
            panel_focus: PanelFocus::from_pref(
                prefs["panel_focus"]
                    .as_str()
                    .or_else(|| prefs["power_focus"].as_str()),
            ),
            tab: TabSelection::Home,
            queue_column_width: prefs["queue_column_width"]
                .as_u64()
                .or_else(|| prefs["power_left_width"].as_u64())
                .map(|v| (v as u16).max(LEFT_WIDTH_DEFAULT))
                .unwrap_or(LEFT_WIDTH_DEFAULT),
            panel_mode: PanelMode::default(),
            // Mini view always starts on the queue panel; not persisted.
            mini_view_focus: PanelFocus::Queue,
            // Always start on Home. The saved queue is restored independently;
            // the saved library tab remains available for runtime persistence.
            library_tab_pending: 0,
            ui_volume: prefs["ui_volume"].as_u64().unwrap_or(100).min(200) as u8,
            pre_mute_volume: prefs["pre_mute_volume"].as_u64().map(|v| v as u8),
            mute_on: prefs["mute_on"].as_bool().unwrap_or(false),
            // Visualizer selection is session-local; every launch starts on
            // artwork so a stale visualizer choice never blanks the card.
            visualizer_enabled: false,
            visualizer_failed: false,
            visualizer: None,
            visualizer_window: Default::default(),
            visualizer_glyph: init.visualizer_glyph,
            now_playing_throbber_index: 0,
            last_throbber_advance: std::time::Instant::now(),
            marquee_text: String::new(),
            marquee_started_at: std::time::Instant::now(),
            last_played_item_id: None,
            last_played_completed: false,
            card_image_states: std::collections::HashMap::new(),
            card_image_loading: std::collections::HashSet::new(),
            last_card_height: 0,
            last_card_width: 0,
            image_picker: None,
            halfblock_picker: None,
            dim_backdrop_active: false,
            image_cache_size_total: init.image_cache_size.saturating_mul(2),
            settings_destination: super::types_settings::SettingsDestination::Main,
            settings_save_at: None,
            confirm_logout: false,
            notif_failed: false,
            sessions: Vec::new(),
            cast_receivers: Vec::new(),
            panel_targets: Vec::new(),
            sessions_loading: false,
            playlists: Vec::new(),
            playlists_cursor: 0,
            playlists_scroll: 0,
            playlists_loading: false,
            playlists_open: None,
            playlists_open_items: Vec::new(),
            playlists_open_cursor: 0,
            playlists_open_scroll: 0,
            playlists_open_loading: false,
            queue_source: crate::config::QueueSource::Unknown,
            queue_dirty: false,
            pending_queue_action: None,
            last_keepalive: Instant::now(),
            last_capabilities: Instant::now(),
            connected_session_id: None,
            connected_session_state: None,
            cast_attachment: None,
            cast_tx,
            cast_rx,
            last_cast_poll: Instant::now() - Duration::from_secs(60),
            cast_status_loading: false,
            remote_tracker: None,
            remote_queue_projection: None,
            remote_queue_lineage: 0,
            playlist_mutations: std::collections::HashMap::new(),
            next_playlist_mutation: 1,
            session_poll_generation: 0,
            direct_remote_connected: false,
            direct_remote_label: None,
            last_session_poll: Instant::now() - Duration::from_secs(60),
            session_miss_count: 0,
            remote_pos_s: 0,
            remote_pos_at: Instant::now(),
            remote_api_pos_advanced_at: Instant::now() - Duration::from_secs(60),
            remote_stalled_while_paused: false,
            remote_seek_pending_until: Instant::now() - Duration::from_secs(1),
            runtime_zero_since: None,
            suspended_local: None,
            active_route: None,
            library_route_cache: std::collections::HashMap::new(),
            force_clear: false,
            tab_scroll: 0,
            last_nav_at: Instant::now() - Duration::from_secs(1),
            last_library_nav_at: Instant::now() - Duration::from_secs(1),
            library_position_dirty: false,
            library_position_dirty_at: Instant::now() - Duration::from_secs(1),
            refocus_at: None,
            album_artist_cache: std::collections::HashMap::new(),
            album_artist_loading: std::collections::HashSet::new(),
            pending_album_artist_fetches: std::collections::VecDeque::new(),
            album_artist_fetches_active: 0,
            album_tracks_cache: std::collections::HashMap::new(),
            album_tracks_loading: std::collections::HashSet::new(),
            series_detail_cache: std::collections::HashMap::new(),
            series_detail_loading: std::collections::HashSet::new(),
            series_season_loading: std::collections::HashSet::new(),
            image_lru: std::collections::VecDeque::new(),
            pending_image_fetches: std::collections::VecDeque::new(),
            image_fetches_active: 0,
            queue_scope: init.initial_queue_scope,
            launched_as_remote: false,
            player_endpoint: None,
            home_is_local_daemon: false,
            idle_feed: init.idle_feed,
            feed_seek_pending_slot: None,
            feed_tab: super::types_feed_tab::FeedTabState::default(),
        };
        app.sync_feed_subscriptions();
        app
    }

    /// Construct the bare Player owner without creating an Emby client or
    /// performing any network work. Configured Emby setup is initialized by
    /// the bounded worker once `run()` has entered the TUI.
    pub fn new_independent(app_config: crate::config::Config) -> Self {
        let (player_tx, player_rx) = mpsc::channel();
        let (_, ws_rx) = mpsc::channel();
        let (lib_tx, lib_rx) = mpsc::channel();
        let (sessions_tx, sessions_rx) = mpsc::channel::<SessionEvent>();
        let (card_image_tx, card_image_rx) =
            mpsc::channel::<(String, Option<image::DynamicImage>)>();
        let (notif_action_tx, notif_action_rx) = mpsc::channel::<String>();
        let (search_tx, search_rx) = mpsc::channel::<(String, Result<Vec<EmbyItem>, String>)>();
        let ui_config = crate::config::load_ui_config().unwrap_or_default();
        let indicator_style = ui_config.indicator_style.parse().unwrap_or_default();
        let configured = app_config.emby_setup.is_some();
        let credential_present =
            mbv_core::config::load_service_secret(mbv_core::config::ServiceKind::Emby).is_some();
        let generation = mbv_core::service_runtime::SetupGeneration::default();
        let audiobookshelf_configured = app_config.audiobookshelf_setup.is_some();
        let audiobookshelf_credential_present =
            mbv_core::config::load_service_secret(mbv_core::config::ServiceKind::Audiobookshelf)
                .is_some();
        let raw_player = Player::new(
            String::new(),
            String::new(),
            app_config.show_audio_window,
            app_config.use_mpv_config,
            app_config.no_scripts,
            app_config.always_skip_intro,
            mbv_core::player::SubtitlePrefs {
                mode: app_config.subtitle_mode.clone(),
                subtitle_lang: app_config.subtitle_lang.clone(),
                audio_lang: app_config.audio_lang.clone(),
            },
            player_tx,
            None,
        );
        let player = PlayerProxy::local(raw_player, app_config.always_play_next);
        let mut app = Self::build(AppInit {
            config: Arc::new(Mutex::new(app_config.clone())),
            emby_runtime: {
                let mut runtime = EmbyRuntime::new(configured);
                runtime.state =
                    super::service_startup::initial_state(configured, credential_present);
                runtime
            },
            audiobookshelf_runtime: {
                let mut runtime = AudiobookshelfRuntime::new(audiobookshelf_configured);
                runtime.state = super::service_startup::audiobookshelf_initial_state(
                    audiobookshelf_configured,
                    audiobookshelf_credential_present,
                );
                runtime
            },
            emby_startup_rx: None,
            emby_startup_request: None,
            audiobookshelf_startup_rx: None,
            audiobookshelf_startup_request: None,
            audiobookshelf_test_rx: None,
            audiobookshelf_setup_rx: None,
            emby_setup_form: None,
            emby_setup_rx: None,
            player,
            player_rx,
            ws_rx,
            ws_send_tx: None,
            audiobookshelf_socket_rx: {
                let (_, rx) = mpsc::channel();
                rx
            },
            audiobookshelf_socket_tx: None,
            audiobookshelf_socket_generation: None,
            player_tab: PlayerTab::default(),
            remote_player_tab: None,
            initial_queue_scope: QueueScope::Local,
            system_notifications: app_config.system_notifications,
            image_protocol: ui_config.image_protocol.clone(),
            image_protocol_enabled: ui_config.image_protocol.is_some(),
            hidden_libraries: app_config.hidden_libraries.clone(),
            library_routes: app_config.library_routes.clone(),
            hidden_latest: app_config.hidden_latest.clone(),
            music_levels: app_config.music_levels.clone(),
            use_nerd_fonts: ui_config.use_nerd_fonts,
            indicator_style,
            image_cache_size: ui_config.image_cache_size,
            visualizer_glyph: ui_config.visualizer_glyph.clone(),
            lib_tx,
            lib_rx,
            sessions_tx,
            sessions_rx,
            card_image_tx,
            card_image_rx,
            notif_action_tx,
            notif_action_rx,
            search_tx,
            search_rx,
            idle_feed: None,
        });
        app.emby_startup_request = configured.then_some((app_config.clone(), generation));
        app.audiobookshelf_startup_request = (audiobookshelf_configured
            && audiobookshelf_credential_present)
            .then_some((app_config.clone(), generation));
        if super::service_startup::should_open_services(&app_config) {
            app.open_services_settings();
        }
        app
    }

    /// `endpoint` is the daemon endpoint the remote player is connected to.
    /// The endpoint's `is_local()` distinguishes local-daemon attach
    /// (`DaemonEndpoint::Local`) from a genuinely remote daemon:
    /// - `Local`: behaves like a plain local session — one unified queue,
    ///   normal queue-state persistence — the only difference is that the
    ///   daemon owns mpv instead of an in-process `Player`.
    /// - `Tcp`/`Unix`: a separate `remote_player_tab` is kept so the user
    ///   can browse locally while a daemon elsewhere plays something else,
    ///   with the Local/Remote scope pill to switch between them.
    #[cfg(test)]
    pub fn new_remote(
        client: EmbyClient,
        remote: mbv_core::remote_player::RemotePlayer,
        player_rx: mpsc::Receiver<PlayerEvent>,
        endpoint: DaemonEndpoint,
    ) -> Self {
        let config = crate::config::load_config().unwrap_or_default();
        Self::new_remote_optional_with_config(Some(client), remote, player_rx, endpoint, config)
    }

    #[cfg(test)]
    pub fn new_remote_with_config(
        client: EmbyClient,
        remote: mbv_core::remote_player::RemotePlayer,
        player_rx: mpsc::Receiver<PlayerEvent>,
        endpoint: DaemonEndpoint,
        config: crate::config::Config,
    ) -> Self {
        Self::new_remote_optional_with_config(Some(client), remote, player_rx, endpoint, config)
    }

    pub fn new_remote_optional_with_config(
        client: Option<EmbyClient>,
        remote: mbv_core::remote_player::RemotePlayer,
        player_rx: mpsc::Receiver<PlayerEvent>,
        endpoint: DaemonEndpoint,
        app_config: crate::config::Config,
    ) -> Self {
        let (_, ws_rx) = mpsc::channel::<mbv_core::ws::WsEvent>();
        let (lib_tx, lib_rx) = mpsc::channel();
        let (sessions_tx, sessions_rx) = mpsc::channel::<SessionEvent>();
        let (card_image_tx, card_image_rx) =
            mpsc::channel::<(String, Option<image::DynamicImage>)>();
        let (notif_action_tx, notif_action_rx) = mpsc::channel::<String>();
        let (search_tx, search_rx) = mpsc::channel::<(String, Result<Vec<EmbyItem>, String>)>();
        let ui_config = crate::config::load_ui_config().unwrap_or_default();
        let hidden_libraries = app_config.hidden_libraries.clone();
        let library_routes = app_config.library_routes.clone();
        let hidden_latest = app_config.hidden_latest.clone();
        let music_levels = app_config.music_levels.clone();
        let always_play_next = app_config.always_play_next;
        let image_protocol = ui_config.image_protocol.clone();
        let image_protocol_enabled = image_protocol.is_some();
        let image_cache_size = ui_config.image_cache_size;
        let use_nerd_fonts = ui_config.use_nerd_fonts;
        let indicator_style: render::indicators::IndicatorStyle =
            ui_config.indicator_style.parse().unwrap_or_default();
        crate::config::evict_old_image_cache();
        // EmbyClient retains this snapshot only for constructing Emby API
        // requests. App general state owns the independent application copy;
        // it is never synchronized back into this concrete API boundary.
        let emby_configured = app_config.emby_setup.is_some();
        let emby_credential_present =
            mbv_core::config::load_service_secret(mbv_core::config::ServiceKind::Emby).is_some();
        let audiobookshelf_configured = app_config.audiobookshelf_setup.is_some();
        let audiobookshelf_credential_present =
            mbv_core::config::load_service_secret(mbv_core::config::ServiceKind::Audiobookshelf)
                .is_some();
        let config = Arc::new(Mutex::new(app_config));
        let client_arc = client.map(|client| Arc::new(Mutex::new(client)));
        let remote_items = remote.items.lock().unwrap().clone();
        let remote_cursor = remote.status.lock().unwrap().current_idx;
        let remote_unified_state = remote.unified_queue_state();
        let remote_queue_source = remote.queue_source.lock().unwrap().clone();
        let remote_has_items = remote_unified_state
            .as_ref()
            .map_or(!remote_items.is_empty(), |state| !state.slots.is_empty());
        let initial_queue_scope = if !endpoint.is_local() && remote_has_items {
            QueueScope::Remote
        } else {
            QueueScope::Local
        };
        let local_daemon_bootstrap = endpoint.is_local().then(|| {
            remote_unified_state
                .as_ref()
                .filter(|state| !state.slots.is_empty())
                .map_or_else(
                    || {
                        bootstrap_local_daemon_queue(
                            remote_items.clone(),
                            remote_cursor,
                            remote_queue_source.clone(),
                            crate::config::load_queue_state(),
                        )
                    },
                    bootstrap_unified_queue,
                )
        });
        // `adopt_queue` returns false when the ctrl socket is already dead
        // (the command send failed); tracked so construction doesn't
        // silently carry on with a queue the daemon never actually adopted
        // (#119 task 5) — see `handle_failed_local_daemon_adoption` below.
        let local_daemon_adoption_failed = local_daemon_bootstrap
            .as_ref()
            .and_then(|bootstrap| bootstrap.adopt_queue.clone())
            .is_some_and(|(items, cursor, source)| !remote.adopt_queue(items, cursor, source));
        // Start MPRIS against this `RemotePlayer` (#175, previously done in
        // `main.rs::run_remote_app` before this constructor even ran).
        // Moved here so App owns the resulting handle and can `rebind` it
        // later if `switch_to_direct_remote` / `restore_local_mode` swap
        // which target owns playback.
        let mpris_remote = remote.clone();
        let mpris_handle = crate::mpris::start(
            mpris_remote.status.clone(),
            move |cmd| {
                mpris_remote.send_command(cmd);
            },
            Some(remote.disconnected_flag()),
        );
        let player = PlayerProxy::remote(remote, always_play_next);
        let (player_tab, remote_player_tab) = if endpoint.is_local() {
            // Local daemon: one unified queue, exactly like plain local
            // playback — no separate remote_player_tab, no scope pill.
            (
                local_daemon_bootstrap.as_ref().unwrap().player_tab.clone(),
                None,
            )
        } else {
            // Remote/network daemon: keep a separate remote queue so the
            // user can browse locally while the daemon plays elsewhere.
            (
                PlayerTab::default(),
                Some(remote_unified_state.as_ref().map_or_else(
                    || PlayerTab::from_emby_items(remote_items, remote_cursor),
                    PlayerTab::from_unified_state,
                )),
            )
        };
        let mut app = Self::build(AppInit {
            config,
            emby_runtime: client_arc.as_ref().map_or_else(
                || {
                    let mut runtime = EmbyRuntime::new(emby_configured);
                    runtime.state = super::service_startup::initial_state(
                        emby_configured,
                        emby_credential_present,
                    );
                    runtime
                },
                |client| EmbyRuntime::ready(client.clone()),
            ),
            audiobookshelf_runtime: {
                let mut runtime = AudiobookshelfRuntime::new(audiobookshelf_configured);
                runtime.state = super::service_startup::audiobookshelf_initial_state(
                    audiobookshelf_configured,
                    audiobookshelf_credential_present,
                );
                runtime
            },
            emby_startup_rx: None,
            emby_startup_request: None,
            audiobookshelf_startup_rx: None,
            audiobookshelf_startup_request: None,
            audiobookshelf_test_rx: None,
            audiobookshelf_setup_rx: None,
            emby_setup_form: None,
            emby_setup_rx: None,
            player,
            player_rx,
            ws_rx,
            ws_send_tx: None,
            audiobookshelf_socket_rx: {
                let (_, rx) = mpsc::channel();
                rx
            },
            audiobookshelf_socket_tx: None,
            audiobookshelf_socket_generation: None,
            player_tab,
            remote_player_tab,
            initial_queue_scope,
            system_notifications: false,
            image_protocol,
            image_protocol_enabled,
            hidden_libraries,
            library_routes,
            hidden_latest,
            music_levels,
            use_nerd_fonts,
            indicator_style,
            image_cache_size,
            visualizer_glyph: ui_config.visualizer_glyph,
            lib_tx,
            lib_rx,
            sessions_tx,
            sessions_rx,
            card_image_tx,
            card_image_rx,
            notif_action_tx,
            notif_action_rx,
            search_tx,
            search_rx,
            idle_feed: None,
        });
        app.mpris = Some(mpris_handle);
        app.player_endpoint = Some(endpoint.clone());
        app.home_is_local_daemon = endpoint.is_local();
        app.sync_subtitle_prefs_to_player();
        app.initialize_shared_state();
        app.launched_as_remote = true;
        debug_assert_eq!(
            app.player.is_remote(),
            app.player_endpoint.is_some(),
            "player-endpoint invariant"
        );
        if endpoint.is_local() {
            let bootstrap = local_daemon_bootstrap.unwrap();
            app.queue_source = bootstrap.queue_source;
            app.last_played_item_id = bootstrap.last_played_item_id;
            app.last_played_completed = bootstrap.last_played_completed;
            if !bootstrap.positions.is_empty() {
                app.spawn_enrich_queue_state(bootstrap.positions);
            }
        } else {
            app.queue_source = remote_queue_source;
        }
        if local_daemon_adoption_failed {
            app.handle_failed_local_daemon_adoption();
        }
        if endpoint.is_local() {
            app.try_auto_reconnect();
        }
        // Cast reattach doesn't depend on Emby readiness (7.3), so unlike
        // the Emby restore above it isn't gated on `endpoint.is_local()`:
        // cast discovery/connect run on this machine's own LAN regardless
        // of which daemon this launch's player talks to.
        app.try_cast_auto_reconnect();
        let generation = app.audiobookshelf_runtime.generation();
        app.audiobookshelf_startup_request = (audiobookshelf_configured
            && audiobookshelf_credential_present)
            .then_some((app.config.lock().unwrap().clone(), generation));
        app
    }

    /// Routes a local-daemon queue adoption whose command send failed (dead
    /// ctrl socket, see `new_remote`) through the same disconnect handling a
    /// live `PlayerEvent::RemoteDisconnected` uses, instead of silently
    /// continuing to build on optimistic queue state the daemon never
    /// actually received (#119 task 5).
    pub(super) fn handle_failed_local_daemon_adoption(&mut self) {
        self.handle_player_event(PlayerEvent::RemoteDisconnected(
            "local daemon connection lost while restoring the saved queue".to_string(),
        ));
    }

    /// Query the terminal for its image protocol (sixel/kitty/iterm2/etc,
    /// via `Picker::from_query_stdio`, falling back to halfblocks), then
    /// apply `self.image_protocol`'s override if it names one of the known
    /// protocols. Called once at startup by `run`.
    pub(super) fn build_image_picker(&self) -> Picker {
        use ratatui_image::picker::ProtocolType;
        let protocol_override = self.image_protocol.clone();
        let mut picker = Picker::from_query_stdio().unwrap_or_else(|_| Picker::halfblocks());
        let proto = protocol_override
            .as_deref()
            .and_then(|s| match s.to_lowercase().as_str() {
                "sixel" => Some(ProtocolType::Sixel),
                "kitty" => Some(ProtocolType::Kitty),
                "iterm2" => Some(ProtocolType::Iterm2),
                "halfblocks" => Some(ProtocolType::Halfblocks),
                _ => None, // "auto" or unknown: use picker's detected protocol
            });
        if let Some(proto) = proto {
            picker.set_protocol_type(proto);
        }
        picker
    }

    /// Populate `image_picker` (terminal-detected, with the config override)
    /// and `halfblock_picker` (the #451 dimmed-backdrop fallback: modals
    /// re-encode images to halfblocks so the dim applies uniformly).
    ///
    /// MUST run before the TuiRealm crossterm listener starts
    /// (`Application::init`): `Picker::from_query_stdio` writes a
    /// `CSI 16 t` cell-size query to the terminal and reads the reply with a
    /// raw `io::stdin().read()`. If the listener thread is already draining
    /// stdin it eats the reply, the picker falls back to a wrong cell size,
    /// and Kitty renders images clipped on the right/bottom (#654).
    pub(crate) fn init_image_pickers(&mut self) {
        let picker = self.build_image_picker();
        log::debug!(
            target: "startup",
            "image picker: protocol={:?} font_size={:?}",
            picker.protocol_type(),
            picker.font_size()
        );
        self.image_picker = Some(picker);
        self.halfblock_picker = Some(Picker::halfblocks());
    }
}
