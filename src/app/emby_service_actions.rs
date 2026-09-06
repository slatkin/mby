use super::notify_actions::ToastSeverity;
use super::{App, LibEvent};
use mbv_core::config::{QueueState, ServiceKind};
use mbv_core::playback_queue::QueueItem;
use mbv_core::service_runtime::{ServiceState, SetupGeneration};

impl App {
    fn persist_filtered_queue(&self, state: &Option<QueueState>) -> Result<(), String> {
        match state {
            Some(state) if !state.items.is_empty() => mbv_core::config::save_queue_state(state),
            _ => mbv_core::config::clear_queue_state(),
        }
    }

    fn restore_emby_setup(setup: Option<&mbv_core::config::EmbySetup>, token: Option<&str>) {
        match (setup, token) {
            (Some(setup), Some(token)) => {
                let _ = mbv_core::config::persist_emby_setup_and_secret(setup, token);
            }
            (Some(setup), None) => {
                let _ = mbv_core::config::save_emby_setup(setup);
            }
            _ => {}
        }
    }

    fn restore_persisted_queue(state: Option<&QueueState>) {
        match state {
            Some(state) => {
                let _ = mbv_core::config::save_queue_state(state);
            }
            None => {
                let _ = mbv_core::config::clear_queue_state();
            }
        }
    }

    fn clear_emby_memory(&mut self) {
        let active_is_feed = self
            .playback_queue()
            .queue
            .active_slot()
            .is_some_and(|slot| matches!(slot.item, QueueItem::Feed(_)));
        if !active_is_feed {
            self.player.stop();
        }
        let mut queues = vec![&mut self.player_tab];
        if let Some(queue) = self.remote_player_tab.as_mut() {
            queues.push(queue);
        }
        for queue in queues {
            let non_emby_items = queue
                .all_queue_items()
                .into_iter()
                .filter(|item| !matches!(item, QueueItem::Emby(_)))
                .collect::<Vec<_>>();
            queue.set_queue_items(non_emby_items, 0);
        }
        self.queue_source = crate::config::QueueSource::Unknown;
        self.queue_dirty = false;
        self.queue_undo_stack.clear();
        self.remote_queue_undo_stack.clear();
        self.pending_delete_slot = None;
        self.pending_queue_removal = None;
        self.playhead.confidence = super::types_playback::PlayheadConfidence::Confirmed;
        self.pending_queue_edit_cursor = None;
        self.remote_tracker = None;
        self.remote_queue_projection = None;
        self.next_up_item = None;
        self.last_played_item_id = None;
        self.last_played_completed = false;
        // Home content is Model-owned (task 5.3d): clearing it is delivered
        // through lib_tx; the shell wipes `home_content` (items, cursor,
        // latest — the `loading` flag is intentionally left alone, matching
        // the legacy clear) and re-projects.
        let _ = self.lib_tx.send(LibEvent::HomeContentCleared);
        self.libs.clear();
        self.sessions.clear();
        self.playlists.clear();
        self.playlists_open = None;
        self.playlists_open_items.clear();
        self.library_route_cache.clear();
        self.library_routes.clear();
        self.album_artist_cache.clear();
        self.album_artist_loading.clear();
        self.pending_album_artist_fetches.clear();
        self.album_tracks_cache.clear();
        self.album_tracks_loading.clear();
        self.series_detail_cache.clear();
        self.series_detail_loading.clear();
        self.series_season_loading.clear();
        self.card_image_states.clear();
        self.card_image_loading.clear();
        self.image_lru.clear();
        self.pending_image_fetches.clear();
        self.image_fetches_active = 0;
        self.library_position_state = Default::default();
        self.active_route = None;
        self.connected_session_id = None;
        self.connected_session_state = None;
        self.direct_remote_connected = false;
        self.direct_remote_label = None;
        self.ws_send_tx = None;
        self.emby_runtime.client = None;
        self.player
            .update_emby_credentials(String::new(), String::new());
        let mut config = self.config.lock().unwrap();
        config.emby_setup = None;
        config.server_url.clear();
        config.username.clear();
        config.password.clear();
        config.api_key.clear();
        config.library_routes.clear();
    }

    pub(super) fn remove_emby_confirmed(&mut self) {
        let old_setup = self.config.lock().unwrap().emby_setup.clone();
        let old_token = mbv_core::config::load_service_secret(ServiceKind::Emby);
        let old_queue = mbv_core::config::load_queue_state();
        let old_positions = mbv_core::config::load_library_position_state();
        let filtered = old_queue.as_ref().map(QueueState::without_emby);
        if let Err(error) =
            mbv_core::config::save_library_position_state_result(&Default::default())
                .and_then(|_| mbv_core::config::remove_emby_setup_and_secret())
                .and_then(|_| self.persist_filtered_queue(&filtered))
        {
            Self::restore_emby_setup(old_setup.as_ref(), old_token.as_deref());
            Self::restore_persisted_queue(old_queue.as_ref());
            let _ = mbv_core::config::save_library_position_state_result(&old_positions);
            self.flash(
                format!("Could not remove Emby safely: {error}"),
                ToastSeverity::Error,
            );
            return;
        }
        self.clear_emby_memory();
        self.emby_runtime.remove_setup();
        self.flash(
            "Emby removed; Feeds remain available".into(),
            ToastSeverity::Success,
        );
    }

    pub(super) fn replace_emby_confirmed(&mut self, generation: SetupGeneration) {
        if !self.emby_runtime.accepts(generation) {
            return;
        }
        let Some(candidate) = self.pending_emby_replacement.take() else {
            return;
        };
        let old_setup = self.config.lock().unwrap().emby_setup.clone();
        let old_token = mbv_core::config::load_service_secret(ServiceKind::Emby);
        let old_queue = mbv_core::config::load_queue_state();
        let old_positions = mbv_core::config::load_library_position_state();
        let filtered = old_queue.as_ref().map(QueueState::without_emby);
        let replacement = candidate.setup.clone();
        let token = candidate.client.token.clone();
        let result = mbv_core::config::save_library_position_state_result(&Default::default())
            .and_then(|_| mbv_core::config::remove_emby_setup_and_secret())
            .and_then(|_| self.persist_filtered_queue(&filtered))
            .and_then(|_| mbv_core::config::persist_emby_setup_and_secret(&replacement, &token));
        if let Err(error) = result {
            Self::restore_emby_setup(old_setup.as_ref(), old_token.as_deref());
            Self::restore_persisted_queue(old_queue.as_ref());
            let _ = mbv_core::config::save_library_position_state_result(&old_positions);
            self.flash(
                format!("Could not replace Emby safely: {error}"),
                ToastSeverity::Error,
            );
            return;
        }
        self.clear_emby_memory();
        let ws_url = candidate.client.ws_url();
        let client = std::sync::Arc::new(std::sync::Mutex::new(candidate.client));
        let (ws_tx, ws_rx) = std::sync::mpsc::channel();
        self.ws_send_tx = Some(mbv_core::ws::start(ws_url, ws_tx));
        self.ws_rx = ws_rx;
        self.player
            .update_emby_credentials(replacement.server_url.clone(), token);
        self.emby_runtime.client = Some(client);
        // The App-internal confirm path cannot touch Model-owned
        // `home_content`; deliver the freshly bootstrapped content through
        // lib_tx (the `clear_emby_memory` above already queued
        // `HomeContentCleared`, so the drain sees the wipe first, then this
        // full snapshot). The merge input is empty because the clear just
        // wiped the pills (task 5.3d).
        let content = self.apply_emby_bootstrap(candidate.bootstrap, &[]);
        let _ = self
            .lib_tx
            .send(LibEvent::HomeContentRefreshed(Box::new(content)));
        let mut config = self.config.lock().unwrap();
        config.emby_setup = Some(replacement.clone());
        config.server_url = replacement.server_url.clone();
        config.username.clear();
        config.password.clear();
        config.api_key.clear();
        drop(config);
        self.emby_runtime.state = ServiceState::Ready;
        self.sync_subtitle_prefs_from_emby();
        self.flash("Emby replaced and ready".into(), ToastSeverity::Success);
    }
}
