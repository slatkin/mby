use super::*;
use crate::app::components::{BrowserComponent, MusicWorkspaceComponent, TvWorkspaceComponent};
use crate::app::images::SERIES_IMAGE_CACHE_KEY_INFIX;
use std::sync::atomic::Ordering;
use std::time::{Duration, Instant};

impl Model {
    pub(crate) fn sync_mounted_surfaces(&mut self) {
        // Apply App-owned effect handoffs to their mounted components.
        // `sync_home` was deleted (task 5.3d, sync_home mirror deletion):
        // Home content/focus is projected event-driven by
        // `push_home_content` at the seams above.
        self.sync_modal_requests();
        self.sync_sidebar_overlays();
        self.sync_playback();
        self.sync_feeds();
        self.sync_audiobookshelf_podcast();
        self.sync_audiobookshelf_book();
        self.sync_queue();
        // Publish wide-TV geometry before other readers of `tv_wide_right_area`/
        // `tv_wide_left_area` (e.g. context-menu anchors) see this frame's
        // values, since those fields are otherwise a previous-frame paint
        // signal.
        self.prime_wide_tv_geometry();
        self.hand_off_tv_breakpoint();
        self.sync_emby_browser();
        self.sync_tv_workspace();
        // TV's narrow owner is already mounted, so consume its transfer only
        // after the destination has received its current pool/loading state.
        self.apply_pending_inline_search_transfer();
        self.sync_music_workspace();
        // Retire destination components whose Service library left the
        // catalog before the focus pass routes to the active destination
        // (keep-destination-components-mounted tasks 1.3).
        self.reconcile_destination_mounts();
        self.sync_active_destination();
        // ADR 0024 D2: mouse eligibility is derived off the same
        // active-destination derivation, in the same pass, right after it.
        self.sync_mouse_subscriptions();
    }

    /// The sole base-frame orchestrator (D3): legacy base paint, resize
    /// content pushes, then the mounted component views and overlay stack, in
    /// that order. All three terminal draws route through it — the two startup
    /// draws pass `false, false` since no resize locals exist yet, and the
    /// steady-state draw passes the per-tick locals mutated by
    /// `handle_terminal_message`.
    pub(in crate::app) fn draw_frame(
        &mut self,
        f: &mut ratatui::Frame,
        music_resize: bool,
        tv_resize: bool,
    ) {
        // The legacy base frame reads the blocking-overlay state for its dim
        // backdrop and stay-alive indicator; that fact now lives in TuiRealm
        // mount state, so the shell computes it once per frame (the deleted
        // App-level `blocking_overlay_active` adapter, task 5.3d).
        self.app.dim_backdrop_active = self.blocking_overlay_active();
        let cursor_scroll = self.app.tab.emby_library_index().and_then(|_| {
            self.emby_browser_component_id()
                .and_then(|id| self.application.get_component(&id))
                .and_then(|c| c.as_any().downcast_ref::<BrowserComponent>())
                .map(|c| (c.cursor(), c.scroll()))
                .or_else(|| {
                    self.tv_workspace_component_id()
                        .and_then(|id| self.application.get_component(&id))
                        .and_then(|c| c.as_any().downcast_ref::<TvWorkspaceComponent>())
                        .map(|c| (c.cursor(), c.scroll()))
                })
                .or_else(|| {
                    self.music_workspace_component_id()
                        .and_then(|id| self.application.get_component(&id))
                        .and_then(|c| c.as_any().downcast_ref::<MusicWorkspaceComponent>())
                        .map(|c| (c.album_cursor(), c.album_scroll()))
                })
        });
        self.app.compose_base_frame(f, cursor_scroll);
        if music_resize {
            self.push_music_workspace_content();
        }
        if tv_resize {
            self.push_tv_workspace_content();
        }
        self.render_playback_component(f);
        self.render_home_component(f);
        self.render_feeds_component(f);
        self.render_audiobookshelf_podcast_component(f);
        self.render_audiobookshelf_book_component(f);
        self.render_emby_browser_component(f);
        self.render_tv_workspace_component(f);
        self.render_music_workspace_component(f);
        self.render_queue_component(f);
        self.render_overlay_stack(f);
    }

    /// Drain completed card-image fetches into the render cache. Returns whether
    /// any completion arrived.
    ///
    /// A completion under a Series (`:ser:` family) key re-projects TV workspace
    /// content: the Wide push reserved its painted key and painted the
    /// placeholder, and the cached entry only reaches the screen through that
    /// re-push. The infix is the only Series-family marker, so both live chains
    /// (Wide's Thumb-first, narrow's `Primary`) gate without a suffix list that
    /// can drift from the key the painter builds.
    pub(in crate::app) fn drain_card_image_completions(&mut self) -> bool {
        let mut series_image_changed = false;
        let mut drained = false;
        while let Ok((cache_key, img_opt)) = self.app.card_image_rx.try_recv() {
            drained = true;
            series_image_changed |= cache_key.contains(SERIES_IMAGE_CACHE_KEY_INFIX);
            self.app.card_image_loading.remove(&cache_key);
            self.app.image_fetches_active = self.app.image_fetches_active.saturating_sub(1);
            let entry = self.app.build_cached_image(&cache_key, img_opt);
            if entry.img.is_some() {
                self.app.image_lru.retain(|k| k != &cache_key);
                self.app.image_lru.push_back(cache_key.clone());
                while self.app.image_lru.len() > self.app.image_cache_size_total {
                    if let Some(evict) = self.app.image_lru.pop_front() {
                        self.app.card_image_states.remove(&evict);
                    }
                }
            }
            self.app.card_image_states.insert(cache_key, entry);
        }
        if series_image_changed {
            self.push_tv_workspace_content();
        }
        drained
    }

    /// The run loop — the moved body of the former `App::run`.
    pub fn run(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        let mut terminal = init_terminal()?;
        terminal.clear()?;

        // Image pickers are initialised in `main` before `Model::new` starts
        // the TuiRealm listener — see `App::init_image_pickers` (#654).

        // Don't clobber a still-live flash message (e.g. try_auto_reconnect's
        // outcome, set during App::new) -- only show "Loading..." if there's
        // no pending flash, mirroring the render loop's own expiry check.
        let has_live_flash = self.app.status_expires.is_some_and(|t| t > Instant::now());
        if !has_live_flash {
            self.app.status = self
                .app
                .emby_client()
                .map(|_| "Loading...".into())
                .unwrap_or_else(|| {
                    service_startup::startup_status(self.app.emby_runtime.state).into()
                });
        }
        self.home_content.loading = true;
        terminal.draw(|f| self.draw_frame(f, false, false))?;

        // Only start the configured Remote Service after the first TUI frame
        // has been rendered. The selected Player owner and UI therefore never
        // wait for Emby setup, authentication, or connectivity.
        if let Some((config, generation)) = self.app.emby_startup_request.take() {
            self.app.emby_startup_rx = Some(service_startup::start(config, generation));
        }
        if let Some((config, generation)) = self.app.audiobookshelf_startup_request.take() {
            self.app.audiobookshelf_startup_rx = Some(service_startup::start_audiobookshelf(
                config,
                generation,
                service_startup::AudiobookshelfCompletionKind::Startup,
            ));
        }

        // Auto-fetch configured feeds asynchronously so the Feeds tab and the
        // Home "Feeds" pill are populated shortly after startup instead
        // of staying empty until the user presses the manual refresh key.
        self.app.start_feed_fetch();

        if let Some(client) = self.app.emby_client() {
            client.lock().unwrap().register_capabilities();
        }

        // Home populates now; Emby's startup merges its portion later (#543).
        self.fetch_home_at_startup();
        self.app.maybe_restore_queue_state();

        // Initialize idle feed if configured
        if self.app.config.lock().unwrap().idle_feed_rss_url.is_empty() {
            // No RSS URL configured, skip idle feed
        } else {
            let (items_tx, items_rx) = std::sync::mpsc::channel();
            self.app.idle_feed = Some(IdleFeed {
                items: Vec::new(),
                current_index: 0,
                last_rotation: Instant::now(),
                last_fetch: Instant::now(),
                items_tx,
                items_rx,
            });
            self.app.spawn_idle_feed_fetch();
        }

        terminal.draw(|f| self.draw_frame(f, false, false))?;

        install_signal_handlers();
        let quit_timeout = Duration::from_secs(self.app.config.lock().unwrap().quit_timeout_secs);
        start_quit_watchdog(self.app.player.quit_handle(), quit_timeout);

        let mut last_render = Instant::now() - Duration::from_secs(2);

        'outer: loop {
            let mut had_events = false;
            let mut music_resize = false;
            let mut tv_resize = false;
            if QUIT_REQUESTED.load(Ordering::Relaxed) {
                break;
            }

            if let Some(worker) = self.app.emby_startup_rx.take() {
                match worker.rx.try_recv() {
                    Ok(completion) => {
                        had_events = true;
                        // Emby bootstrap wrote Home content; assign + re-project
                        // (5.3d); stale/error return None.
                        self.apply_emby_completion_drain(completion);
                    }
                    Err(std::sync::mpsc::TryRecvError::Empty) => {
                        self.app.emby_startup_rx = Some(worker);
                    }
                    Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                        self.app
                            .handle_emby_startup_worker_disconnect(worker.generation);
                        had_events = true;
                    }
                }
            }
            if let Some(rx) = self.app.emby_setup_rx.take() {
                match rx.try_recv() {
                    Ok(completion) => {
                        had_events = true;
                        // Emby setup drain re-bootstraps Home content; assign +
                        // re-project (5.3d); stale/decline return None.
                        self.apply_emby_setup_completion_drain(completion);
                    }
                    Err(std::sync::mpsc::TryRecvError::Empty) => {
                        self.app.emby_setup_rx = Some(rx);
                    }
                    Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                        self.app.handle_emby_setup_worker_disconnect();
                        had_events = true;
                    }
                }
            }
            let drained_abs_events = self.app.drain_audiobookshelf_events();
            had_events |= drained_abs_events;
            // ABS startup/refresh reset the browse state and reconcile
            // per-episode progress; re-project the active podcast browser
            // (task 5.3d.11 U6). Only project when the drain actually reported
            // an event.
            if drained_abs_events {
                self.push_audiobookshelf_podcast_content();
                self.push_audiobookshelf_book_content();
                self.push_music_workspace_content();
            }
            if let Ok(ev) = self.app.player_rx.try_recv() {
                had_events = true;
                let restart = self.app.handle_player_event(ev);
                // Playback completion refetches Home; re-project (task 5.3d, sync_home
                // mirror deletion).
                self.push_home_content();
                // Emby browser content may have changed (5.3d.15/M2).
                self.push_emby_browser_content();
                // Player events can reconcile ABS podcast progress; re-project (5.3d.11 U6).
                self.push_audiobookshelf_podcast_content();
                // Player events can reconcile ABS book progress; re-project (5.3d).
                self.push_audiobookshelf_book_content();
                self.push_music_workspace_content();
                // Reconcile the playhead against the fresh status now that a
                // player event drained -- the single non-paint reconcile point.
                self.app.reconcile_playhead();
                if restart {
                    continue 'outer;
                }
            }

            had_events |= self.app.drain_notif_actions();

            while let Ok(ev) = self.app.lib_rx.try_recv() {
                had_events = true;
                match ev {
                    // Recursive album activation used to write `Some(0)` on
                    // the deleted inline track-focus field directly; the
                    // component owns the cursor now, so the shell delivers
                    // the same trigger as a one-shot request consumed at the
                    // next sync (wide only -- narrow keeps track focus off).
                    super::super::LibEvent::RecursiveAlbumActivated {
                        library_id,
                        nav_stack,
                    } => {
                        let library_id_lookup = library_id.clone();
                        self.app.handle_lib_event(
                            super::super::LibEvent::RecursiveAlbumActivated {
                                library_id,
                                nav_stack,
                            },
                        );
                        // Bind the enter request to the activated album (the
                        // resting cursor of the replaced nav stack) so it can
                        // retry once the album's tracks arrive without ever
                        // firing on an album the user moved to meanwhile.
                        self.music_track_focus_request = self
                            .app
                            .libs
                            .iter()
                            .find(|lib| lib.library.id == library_id_lookup)
                            .and_then(|lib| {
                                let level = lib.nav_stack.last()?;
                                level
                                    .items
                                    .get(level.resting().cursor())
                                    .map(|item| item.id.clone())
                            })
                            .map(|album_id| MusicTrackFocusRequest::Enter { album_id });
                        // Nav stack was replaced wholesale; its resting cursor
                        // now points at the activated album. Re-anchor the
                        // component explicitly, regardless of prior local moves.
                        self.music_workspace_reanchor = true;
                    }
                    // Position restore used to clear the deleted track-focus
                    // field; route the same reset to the component at the
                    // next sync.
                    super::super::LibEvent::RestoreLibraryPosition { .. } => {
                        self.app.handle_lib_event(ev);
                        self.music_track_focus_request = Some(MusicTrackFocusRequest::Clear);
                        // Saved position restored into the nav stack; re-anchor
                        // the workspace cursor to it at this event rather than
                        // by an equality test on the next content push.
                        self.music_workspace_reanchor = true;
                        self.push_inline_search_content();
                    }
                    // App-internal Home writers deliver content/section deltas
                    // to assign/merge into Model-owned `home_content` (5.3d).
                    super::super::LibEvent::HomeContentRefreshed(content) => {
                        self.assign_home_content(*content)
                    }
                    super::super::LibEvent::SeriesDetailFetched { .. } => {
                        self.app.handle_lib_event(ev);
                        self.push_tv_workspace_content();
                    }
                    super::super::LibEvent::HomeContentCleared => self.clear_home_content(),
                    super::super::LibEvent::AudiobookshelfLatestRebuilt(sections) => {
                        self.merge_home_abs_sections(sections)
                    }
                    super::super::LibEvent::FeedsLatestRebuilt(sections) => {
                        self.merge_home_feeds_sections(sections)
                    }
                    ev => self.handle_inline_search_lib_event(ev),
                }
                // Every lib event re-projects Home and the podcast browser
                // (idempotent; 5.3d.11 U6): lib events deliver ShowsFetched /
                // DetailFetched async completions, RestoreLibraryPosition
                // saved-position restore, and audio progress reconciles.
                self.push_home_content();
                self.push_audiobookshelf_podcast_content();
                // Emby browser content may have changed (5.3d.15/M2).
                self.push_emby_browser_content();
                // ABS book async completions (BooksFetched / BookDetailFetched)
                // and saved-position restore arrive via lib events; re-project (5.3d).
                self.push_audiobookshelf_book_content();
                self.push_music_workspace_content();
                self.push_tv_workspace_content();
            }

            // Search results drain: the shell drains `search_rx` and writes
            // each result into the `SearchSidebarComponent` via downcast
            // (task 3.2). The debounce is component-owned; the shell fires
            // the wall clock via the sweep below (#609) and routes any
            // emitted `Msg::Service(SearchQuery)` through the same
            // service-request handler the keyboard path uses.
            had_events |= self.drain_search_results();

            // Search debounce sweep (#609): production never wired a
            // `UserEvent::Clock` publisher, so the shell supplies the
            // wall-clock tick directly via `tick_search_clock` once per
            // main-loop iteration. The component owns the deadline and
            // emits `Msg::Service(SearchQuery)` when it passes; the shell
            // dispatches it through `handle_service_request` (the same
            // path the keyboard arm routes Service requests through).
            if let Some(Msg::Service(request)) = self.tick_search_clock(Instant::now()) {
                had_events = true;
                self.handle_service_request(request);
            }

            had_events |= self.app.drain_session_events();

            had_events |= self.app.drain_cast_events();

            had_events |= self.app.drain_shared_events();

            // Feed results rebuild Home's Feeds pill via `FeedsLatestRebuilt`,
            // drained next loop pass; re-project for the other inputs (5.3d).
            if self.app.drain_feed_tab_results() {
                had_events = true;
                self.push_home_content();
                // Emby browser content may have changed (5.3d.15/M2).
                self.push_emby_browser_content();
            }

            had_events |= self.drain_feed_add_results();

            had_events |= self.drain_card_image_completions();
            self.app.drain_image_fetches();

            // Apply completed off-thread resize+encode results (#164). A
            // response for an evicted/replaced/absent key is silently
            // dropped here; `update_resized_protocol` also guards on
            // ThreadProtocol's internal id, so a stale response racing a
            // newer resize request for the same (still-present) key is a
            // no-op too.
            while let Ok((key, response)) = self.app.resize_response_rx.try_recv() {
                had_events = true;
                // Responses are tagged with the per-suffix mem-key
                // ("bare@suffix"); route them into the matching protocol of
                // the bare-key cache entry.
                if let Some((bare_key, suffix)) = key.rsplit_once('@') {
                    if let Some(entry) = self.app.card_image_states.get_mut(bare_key) {
                        if let Some(state) = entry.protocols.get_mut(suffix) {
                            state.update_resized_protocol(response);
                        }
                    }
                }
            }

            while let Ok(ev) = self.app.ws_rx.try_recv() {
                had_events = true;
                self.app.handle_ws_event(ev);
                // `UserDataChanged` refetches Home inside the handler; re-project (5.3d).
                self.push_home_content();
                // Emby browser content may have changed (5.3d.15/M2).
                self.push_emby_browser_content();
                self.push_music_workspace_content();
            }

            while let Ok(ev) = self.app.audiobookshelf_socket_rx.try_recv() {
                had_events = true;
                self.app.handle_audiobookshelf_socket_event(ev);
                // Socket events reconcile ABS podcast episode progress;
                // re-project (5.3d.11 U6).
                self.push_audiobookshelf_podcast_content();
                // Socket events reconcile ABS book progress; re-project (5.3d).
                self.push_audiobookshelf_book_content();
                self.push_music_workspace_content();
            }

            // Drain idle feed items
            if let Some(ref mut idle_feed) = self.app.idle_feed {
                while let Ok(items) = idle_feed.items_rx.try_recv() {
                    had_events = true;
                    idle_feed.items = items;
                    idle_feed.current_index = 0;
                }
                // Re-fetch every 30 minutes
                if idle_feed.last_fetch.elapsed() >= Duration::from_secs(1800) {
                    idle_feed.last_fetch = Instant::now();
                    self.app.spawn_idle_feed_fetch();
                }
            }

            self.app.sync_visualizer();

            if let Some(at) = self.app.settings_save_at {
                if Instant::now() >= at {
                    let cfg = self.app.config.lock().unwrap().clone();
                    crate::config::save_config_with_ui(&cfg, &self.app.ui_config_snapshot());
                    self.app.settings_save_at = None;
                }
            }

            // Periodic session poll when connected to a remote session
            if self.app.connected_session_id.is_some()
                && self.app.last_session_poll.elapsed() >= Duration::from_secs(1)
                && !self.app.sessions_loading
            {
                self.app.spawn_sessions_load();
            }

            // Periodic status poll while attached to a cast target (6.1). The
            // keep-alive heartbeat this poll's background thread sends is not
            // optional -- see `CAST_STATUS_POLL_INTERVAL`'s doc comment.
            if self.app.cast_attachment.is_some()
                && self.app.last_cast_poll.elapsed()
                    >= super::super::cast_status_actions::CAST_STATUS_POLL_INTERVAL
                && !self.app.cast_status_loading
            {
                self.app.spawn_cast_status_poll();
            }

            // Keep this session visible to other Emby clients
            if let Some(ref tx) = self.app.ws_send_tx {
                if self.app.last_keepalive.elapsed() >= Duration::from_secs(30) {
                    let _ = tx.send_text("{\"MessageType\":\"KeepAlive\"}".to_string());
                    self.app.last_keepalive = Instant::now();
                }
            }
            if self.app.ws_send_tx.is_some()
                && self.app.last_capabilities.elapsed() >= Duration::from_secs(600)
            {
                if let Some(client) = self.app.emby_snapshot() {
                    std::thread::spawn(move || client.register_capabilities());
                }
                self.app.last_capabilities = Instant::now();
            }

            // Terminal event poll is now driven by TuiRealm. `tick` polls the
            // crossterm listener (a background worker) for one event within
            // the same timeout the legacy loop used (8 ms with the visualizer,
            // 50 ms otherwise). UiRoot observes every event independently of
            // the active component's `Option<Msg>`; its typed event signal is
            // only handed to the legacy fallback when UiRoot has focus. This
            // preserves D12 redraws for local mutations without duplicating
            // legacy handling on converted surfaces. When the terminal closes
            // (SIGHUP), the listener's failed poll/read surfaces as a tick
            // error; breaking here lets post-loop cleanup run (player.stop +
            // join) — same contract as the legacy direct poll/read path.
            let poll_timeout = if self.app.visualizer.is_some() {
                Duration::from_millis(8)
            } else {
                Duration::from_millis(50)
            };
            let messages = match self.application.tick(PollStrategy::Once(poll_timeout)) {
                Ok(msgs) => msgs,
                Err(_) => break,
            };
            // ADR 0024: fold the mouse-derived messages (one per eligible
            // subscribed component) down to at most one before the keyboard
            // router fold and `handle_terminal_message` dispatch. A keyboard
            // tick passes through untouched.
            let messages = fold_mouse_messages(messages);
            if !messages.is_empty() {
                had_events = true;
                // `PollStrategy::Once` delivers at most one terminal event per
                // tick, so this runs 0 or 1 times; `quit` handles the legacy
                // `handle_key`-returns-true loop break without a labelled
                // break inside the fold.
                let mut quit = false;
                // Snapshot focus before handling any messages. A legacy key can
                // mount or dismiss an overlay, changing focus before UiRoot's
                // observer message is folded; routing by the live focus then
                // double-delivers that same terminal event.
                let focused = self.application.focus().cloned();
                // ADR 0023: the Keyboard Router fold. `Application::tick`
                // returns the focused component's message first, then the
                // UiRoot observer's. With `PollStrategy::Once` there is at
                // most one terminal event per tick, so the leaf's request and
                // the router's resolution for the same chord arrive together.
                // The router's outcome selects between them: `Command` runs the
                // semantic command and discards the leaf's message, `Swallow`
                // runs nothing and discards it, `FallThrough` lets the leaf's
                // own request stand.
                let router = self.router_outcome(&messages);
                if let RouterOutcome::Command(command) = &router {
                    quit |= self.dispatch_router_command(command.clone());
                }
                for msg in apply_router_outcome(messages, focused.as_ref(), &router) {
                    if self.handle_terminal_message(msg, &mut music_resize, &mut tv_resize) {
                        quit = true;
                    }
                }
                if quit {
                    break 'outer;
                }
            }

            // Keep in sync with tests_tick_harness.rs, the other caller of this shared pass.
            self.sync_mounted_surfaces();

            self.app.expire_music_grouping_candidates();
            self.app.sync_volume_from_player();
            self.app.flush_library_position_if_idle();

            // Advance idle feed rotation
            self.app.advance_idle_feed_rotation();

            // See `render_interval`'s doc comment for the fast/slow cadence rules.
            let render_interval = self.app.render_interval();
            if self
                .app
                .wants_terminal_render(had_events, last_render, render_interval)
            {
                if self.app.last_throbber_advance.elapsed() >= std::time::Duration::from_millis(300)
                {
                    let playback = self.app.effective_playback_state();
                    if playback.active && !self.app.playback_transport_paused() {
                        self.app.now_playing_throbber_index =
                            self.app.now_playing_throbber_index.wrapping_add(1);
                    }
                    self.app.last_throbber_advance = std::time::Instant::now();
                }
                if self.app.force_clear {
                    self.app.force_clear = false;
                    if let Err(e) = terminal.clear() {
                        log::error!(
                            target: "run_loop",
                            "terminal.clear() failed: {e:?} (kind={:?})",
                            e.kind()
                        );
                        return Err(e.into());
                    }
                }
                if self.app.visualizer.is_some() {
                    self.app.sync_visualizer();
                }
                if let Err(e) = terminal.draw(|f| self.draw_frame(f, music_resize, tv_resize)) {
                    log::error!(
                        target: "run_loop",
                        "terminal.draw() failed: {e:?} (kind={:?})",
                        e.kind()
                    );
                    return Err(e.into());
                }
                last_render = Instant::now();
            }
        }

        self.persist_emby_browser_scroll_for_active_library();
        self.app.teardown(quit_timeout);
        let _ = restore_terminal(terminal); // ignore errors — terminal may be gone (SIGHUP)
                                            // Printed only after the terminal is restored (task 7.2): anything
                                            // written while still in the alternate screen would never be
                                            // visible once it's left.
        if let Some(msg) = self.app.pending_exit_message.take() {
            println!("{msg}");
        }
        Ok(())
    }
}

#[cfg(test)]
#[path = "shell_run_tests.rs"]
mod tests;
