use super::notify_actions::ToastSeverity;
use super::App;
use mbv_core::config::QueueState;
use mbv_core::service_runtime::ServiceState;

impl App {
    fn update_local_audiobookshelf_context(
        &self,
        context: Option<mbv_core::player::AudiobookshelfPlayerContext>,
    ) {
        self.player.update_audiobookshelf_context(context.clone());
        if let Some(suspended) = &self.suspended_local {
            suspended.player.update_audiobookshelf_context(context);
        }
    }

    fn signal_running_local_daemon(&mut self, revision: u64) {
        if let Err(error) = mbv_core::remote_player::signal_local_daemon_service_setup(
            mbv_core::config::ServiceKind::Audiobookshelf,
            revision,
        ) {
            self.flash(error, ToastSeverity::Warning);
        }
    }

    pub(super) fn clear_audiobookshelf_authentication(&mut self) -> Result<(), String> {
        let current_generation = self.audiobookshelf_runtime.generation();
        self.stop_audiobookshelf_socket();
        self.audiobookshelf_runtime
            .cancel_setup(current_generation, ServiceState::NeedsAuthentication);
        self.clear_audiobookshelf_catalog();
        self.stop_active_audiobookshelf_playback();
        self.update_local_audiobookshelf_context(None);
        self.audiobookshelf_runtime.user = None;
        mbv_core::config::clear_service_secret_result(mbv_core::config::ServiceKind::Audiobookshelf)
    }

    fn stop_active_audiobookshelf_playback(&self) {
        let active_is_audiobookshelf = self
            .playback_queue()
            .queue
            .active_slot()
            .is_some_and(|slot| slot.item.is_audiobookshelf_any());
        if active_is_audiobookshelf {
            self.player.stop();
        }
    }

    pub(super) fn apply_audiobookshelf_setup_completion(
        &mut self,
        completion: super::service_startup::AudiobookshelfSetupCompletion,
    ) {
        use super::notify_actions::ToastSeverity;
        if !self.audiobookshelf_runtime.accepts(completion.generation) {
            return;
        }
        match completion.result {
            Ok(candidate) => {
                let existing = self.config.lock().unwrap().audiobookshelf_setup.clone();
                if existing
                    .as_ref()
                    .is_some_and(|setup| setup.server_url != candidate.setup.server_url)
                {
                    self.audiobookshelf_runtime
                        .complete(completion.generation, completion.previous_state);
                    self.pending_audiobookshelf_replacement =
                        Some(super::service_startup::AudiobookshelfPendingReplacement {
                            candidate,
                            previous_state: completion.previous_state,
                        });
                    self.audiobookshelf_setup_form = None;
                    self.ask_confirm(super::types_confirm::ConfirmModal {
                        title: " Replace Audiobookshelf ".into(),
                        message:
                            "Replace Audiobookshelf? Service-owned setup and state will be cleared."
                                .into(),
                        hint: "[y/Enter] Replace    [Esc] Cancel".into(),
                        on_confirm: super::types_confirm::ConfirmAction::ReplaceAudiobookshelf(
                            completion.generation,
                        ),
                    });
                    return;
                }
                let user = candidate.user.clone();
                let setup = candidate.setup.clone();
                let result = mbv_core::config::commit_audiobookshelf_candidate(
                    mbv_core::audiobookshelf::AudiobookshelfValidatedSetup::new(
                        candidate.setup,
                        candidate.user,
                        candidate.api_key,
                    ),
                );
                match result {
                    Ok((_, revision)) => {
                        let mut committed = setup.clone();
                        committed.revision = revision;
                        self.config.lock().unwrap().audiobookshelf_setup = Some(committed);
                        self.audiobookshelf_runtime
                            .commit_ready(completion.generation, user.clone());
                        self.start_audiobookshelf_socket(completion.generation);
                        self.install_audiobookshelf_player_context(completion.generation);
                        self.audiobookshelf_setup_form = None;
                        self.signal_running_local_daemon(revision);
                        self.flash(
                            format!(
                                "Audiobookshelf {} is ready for {}",
                                setup.server_url, user.username
                            ),
                            ToastSeverity::Success,
                        );
                    }
                    Err(_) => {
                        self.audiobookshelf_runtime
                            .complete(completion.generation, completion.previous_state);
                        if let Some(form) = self.audiobookshelf_setup_form.as_mut() {
                            form.busy = false;
                            form.error = "Could not save Audiobookshelf setup".into();
                        }
                    }
                }
            }
            Err(error) => {
                self.audiobookshelf_runtime
                    .complete(completion.generation, completion.previous_state);
                if let Some(form) = self.audiobookshelf_setup_form.as_mut() {
                    form.busy = false;
                    form.error = error.to_string();
                }
            }
        }
    }

    pub(super) fn handle_audiobookshelf_setup_worker_disconnect(&mut self) {
        let previous = self
            .audiobookshelf_setup_form
            .as_ref()
            .map_or(ServiceState::NotConfigured, |form| form.previous_state);
        if let Some(form) = self.audiobookshelf_setup_form.as_mut() {
            form.busy = false;
            form.error = "Audiobookshelf setup stopped unexpectedly; retry".into();
        }
        self.audiobookshelf_runtime.state = previous;
    }

    /// Helper that persists a filtered queue or clears the file when empty.
    /// Mirrors Emby's `persist_filtered_queue` but for Audiobookshelf.
    fn persist_filtered_queue_abs(state: &Option<QueueState>) -> Result<(), String> {
        match state {
            Some(state) if !state.items.is_empty() => mbv_core::config::save_queue_state(state),
            _ => mbv_core::config::clear_queue_state(),
        }
    }

    fn clear_audiobookshelf_queue_memory(&mut self) {
        // If the currently active slot is Audiobookshelf, stop playback.
        let active_is_abs = self
            .playback_queue()
            .queue
            .active_slot()
            .is_some_and(|slot| slot.item.is_audiobookshelf_any());
        if active_is_abs {
            self.player.stop();
        }
        // Filter both local and remote player tabs, keeping Emby + Feed items.
        let mut queues = vec![&mut self.player_tab];
        if let Some(queue) = self.remote_player_tab.as_mut() {
            queues.push(queue);
        }
        for queue in queues {
            let cursor_before = queue.queue_cursor;
            let kept = queue
                .all_queue_items()
                .into_iter()
                .filter(|item| !item.is_audiobookshelf_any())
                .collect::<Vec<_>>();
            let new_cursor = cursor_before.min(kept.len().saturating_sub(1));
            queue.set_queue_items(kept, new_cursor);
        }
        // Clear transient queue mutation state that might reference ABS slots.
        self.pending_delete_slot = None;
        self.pending_queue_removal = None;
        // If queue_source was tied to ABS (currently QueueSource has no ABS variant,
        // but future-proof: if items empty, reset source).
        if self.player_tab.total_queue_len() == 0 {
            self.queue_source = crate::config::QueueSource::Unknown;
        }
        self.queue_dirty = false;
    }

    pub(super) fn remove_audiobookshelf_confirmed(&mut self) {
        self.stop_audiobookshelf_socket();
        self.stop_active_audiobookshelf_playback();
        // Snapshot for rollback if persistence fails, mirroring Emby removal.
        let old_queue = mbv_core::config::load_queue_state();
        let filtered = old_queue.as_ref().map(QueueState::without_audiobookshelf);
        // Use the transactional boundary that accepts a clear_owned_state closure.
        // Queue filtering (persisted + in-memory) is performed inside that closure
        // so setup/secret removal and queue purge are atomic from the caller's view.
        let persist_result =
            mbv_core::config::remove_audiobookshelf_setup_and_secret_with_owned_state(
                || Self::persist_filtered_queue_abs(&filtered),
                || {},
            );

        if let Err(error) = persist_result {
            // Rollback: restore setup/secret handled inside transaction rollback;
            // The transaction restores durable setup, secret, and queue state;
            // in-memory queues have not been changed on this path.
            self.flash(
                format!("Could not remove Audiobookshelf safely: {error}"),
                ToastSeverity::Error,
            );
            return;
        }

        self.clear_audiobookshelf_catalog();
        self.update_local_audiobookshelf_context(None);
        self.clear_audiobookshelf_queue_memory();
        self.config.lock().unwrap().audiobookshelf_setup = None;
        self.audiobookshelf_runtime.remove_setup();
        self.signal_running_local_daemon(0);
        self.flash(
            "Audiobookshelf removed; Emby and Feeds remain available".into(),
            ToastSeverity::Success,
        );
    }

    pub(super) fn replace_audiobookshelf_confirmed(
        &mut self,
        generation: mbv_core::service_runtime::SetupGeneration,
    ) {
        if !self.audiobookshelf_runtime.accepts(generation) {
            return;
        }
        let previous_state = self
            .pending_audiobookshelf_replacement
            .as_ref()
            .map_or(self.audiobookshelf_runtime.state, |pending| {
                pending.previous_state
            });
        self.stop_active_audiobookshelf_playback();
        let Some(pending) = self.pending_audiobookshelf_replacement.take() else {
            return;
        };
        let candidate = pending.candidate;
        let user = candidate.user.clone();
        let setup = candidate.setup.clone();

        // Snapshot old queue for rollback explanation (persisted state rollback
        // itself is handled inside the transaction's restore hook, but we also
        // need to restore in-memory queue on failure).
        let old_queue = mbv_core::config::load_queue_state();
        let filtered = old_queue.as_ref().map(QueueState::without_audiobookshelf);
        let old_player_items = self.player_tab.all_queue_items();
        let old_player_cursor = self.player_tab.queue_cursor;
        let old_remote_items = self
            .remote_player_tab
            .as_ref()
            .map(|tab| (tab.all_queue_items(), tab.queue_cursor));

        let result = mbv_core::config::replace_audiobookshelf_candidate(
            mbv_core::audiobookshelf::AudiobookshelfValidatedSetup::new(
                candidate.setup,
                candidate.user,
                candidate.api_key,
            ),
            || Self::persist_filtered_queue_abs(&filtered),
            || {
                // Restore in-memory queues on failure.
                self.player_tab
                    .set_queue_items(old_player_items.clone(), old_player_cursor);
                if let Some((items, cursor)) = old_remote_items.clone() {
                    if let Some(tab) = self.remote_player_tab.as_mut() {
                        tab.set_queue_items(items, cursor);
                    }
                }
                if let Some(q) = old_queue.as_ref() {
                    let _ = mbv_core::config::save_queue_state(q);
                }
            },
        );
        match result {
            Ok((_, revision)) => {
                self.audiobookshelf_runtime
                    .cancel_setup(generation, previous_state);
                let replacement_generation = self.audiobookshelf_runtime.generation();
                self.clear_audiobookshelf_catalog();
                self.clear_audiobookshelf_queue_memory();
                let mut committed = setup.clone();
                committed.revision = revision;
                self.config.lock().unwrap().audiobookshelf_setup = Some(committed);
                self.audiobookshelf_runtime
                    .commit_ready(replacement_generation, user.clone());
                self.start_audiobookshelf_socket(replacement_generation);
                self.install_audiobookshelf_player_context(replacement_generation);
                self.signal_running_local_daemon(revision);
                self.flash(
                    format!(
                        "Audiobookshelf {} is ready for {}",
                        setup.server_url, user.username
                    ),
                    ToastSeverity::Success,
                );
            }
            Err(error) => {
                self.audiobookshelf_runtime.state = previous_state;
                self.flash(
                    format!("Could not replace Audiobookshelf safely: {error}"),
                    ToastSeverity::Error,
                );
            }
        }
    }

    pub(super) fn install_audiobookshelf_player_context(
        &self,
        generation: mbv_core::service_runtime::SetupGeneration,
    ) {
        let setup = self.config.lock().unwrap().audiobookshelf_setup.clone();
        let credential =
            mbv_core::config::load_service_secret(mbv_core::config::ServiceKind::Audiobookshelf);
        let context = setup.zip(credential).and_then(|(setup, credential)| {
            mbv_core::player::AudiobookshelfPlayerContext::new(
                generation,
                setup,
                credential,
                mbv_core::api::device_id(),
            )
            .map(|context| {
                let (sender, receiver) = std::sync::mpsc::channel();
                let context = context.with_progress_updates(sender);
                let lib_tx = self.lib_tx.clone();
                let _ =
                    std::thread::spawn(move || {
                        for update in receiver {
                            if lib_tx
                            .send(super::types_events::LibEvent::AudiobookshelfProgressAcknowledged(
                                update,
                            ))
                            .is_err()
                        {
                            break;
                        }
                        }
                    });
                let (book_sender, book_receiver) = std::sync::mpsc::channel();
                let context = context.with_book_progress_updates(book_sender);
                let lib_tx = self.lib_tx.clone();
                let _ =
                    std::thread::spawn(move || {
                        for update in book_receiver {
                            if lib_tx
                            .send(
                                super::types_events::LibEvent::AudiobookshelfBookProgressAcknowledged(
                                    update,
                                ),
                            )
                            .is_err()
                        {
                            break;
                        }
                        }
                    });
                context
            })
        });
        self.update_local_audiobookshelf_context(context);
    }

    // ---- Audiobookshelf Socket.IO lifecycle (tasks 2.5-2.6) ----

    /// Open an Audiobookshelf Socket.IO connection for the given setup
    /// generation. Shuts down any existing socket first (for replace).
    pub(super) fn start_audiobookshelf_socket(
        &mut self,
        generation: mbv_core::service_runtime::SetupGeneration,
    ) {
        // Shutdown any existing socket first (replace scenario).
        self.stop_audiobookshelf_socket();

        let Some((setup, key)) =
            super::service_startup::audiobookshelf_setup_and_key(&self.config.lock().unwrap())
        else {
            return;
        };
        let Some(url) = mbv_core::audiobookshelf_socket::socket_url(&setup.server_url) else {
            return;
        };
        let (event_tx, rx) = std::sync::mpsc::channel();
        self.audiobookshelf_socket_tx =
            Some(mbv_core::audiobookshelf_socket::start(url, key, event_tx));
        self.audiobookshelf_socket_rx = rx;
        self.audiobookshelf_socket_generation = Some(generation);
    }

    /// Shut down the Audiobookshelf Socket.IO connection (if any) and
    /// replace the receiver with a dummy so the drain loop has no effect.
    pub(super) fn stop_audiobookshelf_socket(&mut self) {
        if let Some(tx) = self.audiobookshelf_socket_tx.take() {
            let _ = tx.send(());
        }
        let (_, rx) = std::sync::mpsc::channel();
        self.audiobookshelf_socket_rx = rx;
        self.audiobookshelf_socket_generation = None;
    }

    /// Handle a decoded socket event. The progress-merge body (task 3.1-3.3)
    /// is delegated to `apply_audiobookshelf_socket_progress`.
    pub(super) fn handle_audiobookshelf_socket_event(
        &mut self,
        ev: mbv_core::audiobookshelf_socket::SocketEvent,
    ) {
        use super::notify_actions::ToastSeverity;
        match ev {
            mbv_core::audiobookshelf_socket::SocketEvent::Authenticated => {}
            mbv_core::audiobookshelf_socket::SocketEvent::InvalidToken => {
                // Task 2.3: surface the same ABS authentication failure
                // classification used elsewhere; do NOT clear the installed
                // API key alone from this.
                self.audiobookshelf_runtime.state = ServiceState::NeedsAuthentication;
                self.flash(
                    "Audiobookshelf rejected its credential over the socket connection".into(),
                    ToastSeverity::Warning,
                );
            }
            mbv_core::audiobookshelf_socket::SocketEvent::ProgressUpdated(progress) => {
                self.apply_audiobookshelf_socket_progress(progress);
            }
            // Open, ConnectAck are consumed by the background thread and
            // never forwarded to the app.
            mbv_core::audiobookshelf_socket::SocketEvent::Open { .. }
            | mbv_core::audiobookshelf_socket::SocketEvent::ConnectAck => {}
        }
    }

    /// Apply a `user_item_progress_updated` event from the socket.
    ///
    /// Task 3.1-3.3: generation gate, active-slot skip, in-place merge
    /// via reconcile (no REST call). Task 3.4 covers test cases.
    fn apply_audiobookshelf_socket_progress(
        &mut self,
        progress: mbv_core::audiobookshelf_socket::AudiobookshelfProgress,
    ) {
        // Task 3.3: drop events from a superseded connection generation.
        let Some(gen) = self.audiobookshelf_socket_generation else {
            return;
        };
        if !self.audiobookshelf_runtime.accepts(gen) {
            return;
        }

        // Task 3.2: never touch the actively Player-owned slot.
        if self.player_owns_active_match(&progress) {
            return;
        }

        // Task 3.1: only merge when the episode is known in browse or
        // queue (the socket spec says unmatched episodes SHALL apply
        // no change — no browse-map insert, unlike the daemon route).
        let known = self.audiobookshelf_browse.iter().any(|state| {
            state.progress.contains_key(&(
                progress.library_item_id.clone(),
                progress.episode_id.clone(),
            )) || state.episodes.as_ref().is_some_and(|eps| {
                eps.iter().any(|ep| {
                    ep.library_item_id == progress.library_item_id
                        && ep.episode_id == progress.episode_id
                })
            })
        }) || self.player_tab.queue.slots().iter().any(|slot| {
            slot.item.as_audiobookshelf().is_some_and(|ep| {
                ep.library_item_id == progress.library_item_id
                    && ep.episode_id == progress.episode_id
            })
        });
        if !known {
            return;
        }

        // Task 3.1: merge in place (no REST call) via the existing
        // shared reconcile path that the daemon-route ack also uses.
        let position_ticks =
            super::audiobookshelf_browse_actions::seconds_to_ticks(progress.current_time_seconds);
        self.reconcile_audiobookshelf_progress(
            &progress.library_item_id,
            &progress.episode_id,
            position_ticks,
            progress.current_time_seconds,
            progress.is_finished,
        );
    }

    /// Returns `true` when the active slot in the Player owner's queue
    /// matches the given progress event's identity.
    fn player_owns_active_match(
        &self,
        progress: &mbv_core::audiobookshelf_socket::AudiobookshelfProgress,
    ) -> bool {
        self.playback_queue()
            .queue
            .active_slot()
            .and_then(|slot| slot.item.as_audiobookshelf())
            .is_some_and(|episode| {
                episode.library_item_id == progress.library_item_id
                    && episode.episode_id == progress.episode_id
            })
    }
}
