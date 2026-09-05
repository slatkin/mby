//! Playlist mutation tracking, queue-state persistence, and remote sync
//! helpers for [`App`], split out of `queue_actions.rs` to keep that file
//! within the repository's file-size limit.

use super::*;

impl App {
    pub(super) fn start_playlist_mutation(&mut self, playlist_id: &str) {
        let stale = self
            .playlist_mutations
            .get(playlist_id)
            .and_then(|state| state.active.as_ref())
            .is_some_and(|mutation| match mutation {
                PlaylistMutation::Save {
                    queue_lineage,
                    source_playlist_id,
                    ..
                } => {
                    *queue_lineage != self.remote_queue_lineage
                        || self.queue_playlist_id() != Some(source_playlist_id.as_str())
                }
                PlaylistMutation::CreateAs {
                    queue_lineage,
                    source_playlist_id,
                    ..
                } => {
                    *queue_lineage != self.remote_queue_lineage
                        || self.queue_playlist_id() != source_playlist_id.as_deref()
                }
                PlaylistMutation::Replace { queue_lineage, .. } => {
                    *queue_lineage != self.remote_queue_lineage
                }
            });
        if stale {
            log::debug!(target: "playlist", "discarding stale queued playlist mutation for {playlist_id}");
            if let Some(mutation_id) = self
                .playlist_mutations
                .get(playlist_id)
                .and_then(|state| state.active.as_ref())
                .map(PlaylistMutation::mutation_id)
            {
                self.finish_playlist_mutation(playlist_id, mutation_id);
            }
            return;
        }
        // Full updates (Save/Replace) recreate server playlist-entry IDs.
        // Consume eligibility derived from the old IDs dies at this boundary,
        // after earlier same-playlist mutations have completed, not when the
        // user merely queues the update. A Replace genuinely replaces queue
        // content/source and advances visible-queue lineage; a Save does not
        // change queue slots, content, or source, so it must preserve the
        // request lineage — otherwise its successful completion can never
        // satisfy the original-lineage checks that clear dirty state and run
        // an explicit save-before-replace continuation.
        let is_full_update = self
            .playlist_mutations
            .get(playlist_id)
            .and_then(|state| state.active.as_ref())
            .is_some_and(|mutation| {
                matches!(
                    mutation,
                    PlaylistMutation::Save { .. } | PlaylistMutation::Replace { .. }
                )
            });
        if is_full_update {
            let is_replace = self
                .playlist_mutations
                .get(playlist_id)
                .and_then(|state| state.active.as_ref())
                .is_some_and(|mutation| matches!(mutation, PlaylistMutation::Replace { .. }));
            if self.remote_tracking_source_is(playlist_id) {
                self.retire_remote_tracking(is_replace);
            }
            // A full update recreates entry identities for the playlist it
            // targets. Only the current source's items carry those identities,
            // so clear and persist them exactly when the update targets that
            // source — and regardless of whether a tracker is active. An
            // unrelated overwrite leaves the current source's identities valid
            // until completion flips the source.
            if self.queue_playlist_id() == Some(playlist_id) {
                self.clear_local_playlist_entry_ids();
                self.save_queue_state();
            }
        }
        let Some(client) = self.emby_snapshot() else {
            return;
        };
        let Some(state) = self.playlist_mutations.get_mut(playlist_id) else {
            return;
        };
        let Some(mutation) = state.active.as_mut() else {
            return;
        };
        let tx = self.sessions_tx.clone();
        let playlist_id = playlist_id.to_string();
        let mutation_id = mutation.mutation_id();
        match mutation {
            PlaylistMutation::Save {
                queue_lineage,
                source_playlist_id,
                item_ids,
                ..
            } => {
                *item_ids = Some(
                    self.player_tab
                        .emby_items()
                        .iter()
                        .map(|e| e.id.clone())
                        .collect(),
                );
                let ids = item_ids.clone().unwrap_or_default();
                let queue_lineage = *queue_lineage;
                let source_playlist_id = source_playlist_id.clone();
                std::thread::spawn(move || {
                    let result = client.update_playlist_items(&playlist_id, &ids);
                    let _ = tx.send(SessionEvent::PlaylistMutationComplete {
                        mutation_id,
                        playlist_id,
                        queue_lineage,
                        source_playlist_id,
                        result,
                    });
                });
            }
            PlaylistMutation::CreateAs {
                name,
                coordinator_key,
                item_ids,
                queue_lineage,
                source_playlist_id,
                ..
            } => {
                *item_ids = Some(
                    self.player_tab
                        .emby_items()
                        .iter()
                        .map(|e| e.id.clone())
                        .collect(),
                );
                let ids = item_ids.clone().unwrap_or_default();
                let name = name.clone();
                let coordinator_key = coordinator_key.clone();
                let source_playlist_id = source_playlist_id.clone();
                let queue_lineage = *queue_lineage;
                std::thread::spawn(move || {
                    let result = client.create_playlist(&name, &ids);
                    let _ = tx.send(SessionEvent::PlaylistCreateComplete {
                        mutation_id,
                        coordinator_key,
                        name,
                        queue_lineage,
                        source_playlist_id,
                        result,
                    });
                });
            }
            PlaylistMutation::Replace {
                name,
                queue_lineage,
                source_playlist_id,
                item_ids,
                ..
            } => {
                *item_ids = Some(
                    self.player_tab
                        .emby_items()
                        .iter()
                        .map(|e| e.id.clone())
                        .collect(),
                );
                let replacement_name = name.to_string();
                let ids = item_ids.clone().unwrap_or_default();
                let queue_lineage = *queue_lineage;
                let source_playlist_id = source_playlist_id.clone();
                std::thread::spawn(move || {
                    let result = client
                        .delete_playlist(&playlist_id)
                        .and_then(|_| client.create_playlist(&replacement_name, &ids));
                    let _ = tx.send(SessionEvent::PlaylistReplacementComplete {
                        mutation_id,
                        playlist_id,
                        queue_lineage,
                        source_playlist_id,
                        name: replacement_name,
                        result,
                    });
                });
            }
        }
    }

    pub(super) fn build_queue_state(&self) -> crate::config::QueueState {
        let positions: std::collections::HashMap<String, i64> = self
            .player_tab
            .queue
            .slots()
            .iter()
            .filter_map(|s| s.item.as_emby())
            .filter(|i| i.playback_position_ticks > 0 && !i.is_audio())
            .map(|i| (i.id.clone(), i.playback_position_ticks))
            .collect();
        crate::config::QueueState {
            source: self.queue_source.clone(),
            items: self
                .player_tab
                .queue
                .slots()
                .iter()
                .map(|s| s.item.clone())
                .collect(),
            cursor: self.player_tab.queue_cursor,
            last_played_content_id: self
                .player_tab
                .queue
                .slots()
                .get(self.player_tab.queue_cursor)
                .map(|slot| slot.item.content_id()),
            last_played_item_id: self.last_played_item_id.clone(),
            last_played_completed: self.last_played_completed,
            positions,
        }
    }

    pub(in crate::app) fn save_queue_state(&mut self) {
        let state = self.build_queue_state();
        if state.items.is_empty() {
            // Don't nuke the on-disk queue just because the local tab happens to be
            // empty while attached to a remote session — that reflects remote-control
            // UI state, not the user intentionally clearing their local queue.
            if self.connected_session_id.is_none() {
                if let Err(e) = crate::config::clear_queue_state() {
                    log::warn!(target: "queue", "failed to clear queue state: {e}");
                }
            }
        } else {
            if let Err(e) = crate::config::save_queue_state(&state) {
                log::warn!(target: "queue", "failed to save queue state: {e}");
            }
        }
        self.persist_shared_queue_state(&state, false);
    }

    /// Like `save_queue_state`, but never deletes the on-disk snapshot when the
    /// in-memory queue happens to be empty. Quit is not a genuine "user cleared
    /// the queue" signal — an empty `player_tab.items` at quit time can equally
    /// mean this session never touched the local queue at all, and unconditionally
    /// deleting in that case wipes a perfectly valid snapshot from an earlier
    /// session with no recovery path. Only an explicit `ClearQueue` action (which
    /// goes through `save_queue_state`) should ever delete the file.
    pub(in crate::app) fn save_queue_state_no_clear(&mut self) {
        let state = self.build_queue_state();
        if !state.items.is_empty() {
            if let Err(e) = crate::config::save_queue_state(&state) {
                log::warn!(target: "queue", "failed to save queue state (no-clear): {e}");
            }
        }
        self.persist_shared_queue_state(&state, false);
    }

    pub(super) fn save_queue_state_after_explicit_clear(&mut self) {
        self.save_queue_state();
        let state = self.build_queue_state();
        self.persist_shared_queue_state(&state, true);
    }

    pub(in crate::app) fn save_queue_state_after_remote_projection(&mut self) {
        let state = self.build_queue_state();
        if state.items.is_empty() {
            if let Err(e) = crate::config::clear_queue_state() {
                log::warn!(target: "queue", "failed to clear projected queue state: {e}");
            }
        } else if let Err(e) = crate::config::save_queue_state(&state) {
            log::warn!(target: "queue", "failed to save projected queue state: {e}");
        }
        self.persist_shared_queue_state(&state, true);
    }

    pub(super) fn persist_shared_queue_state(
        &mut self,
        state: &crate::config::QueueState,
        allow_empty: bool,
    ) {
        if state.items.is_empty() && !allow_empty {
            return;
        }
        if let Ok(value) = serde_json::to_value(state) {
            let _ = self.persist_shared_document(
                mbv_core::shared_state::SharedDocumentKind::QueueState,
                value,
            );
        }
    }

    /// Wraps `restore_queue_state` with the guard startup needs: a local-
    /// daemon `App::new_remote` instance already had its queue populated
    /// during construction (`bootstrap_local_daemon_queue`, live-adopted
    /// from the daemon or loaded from disk for a cold daemon), so reading
    /// `queue_state.json` again here would clobber a live daemon queue with
    /// a stale disk snapshot on every new attach, and is simply redundant
    /// in the cold case.
    pub(in crate::app) fn maybe_restore_queue_state(&mut self) {
        if self.is_local_daemon() {
            return;
        }
        if self.shared_client.as_ref().is_some_and(|client| {
            matches!(
                client.state(),
                mbv_core::shared_client::SharedClientState::Shared
            )
        }) {
            return;
        }
        self.restore_queue_state();
    }

    /// Restore the queue from disk immediately and synchronously — the file
    /// already holds full `EmbyItem`s, so this is a local read, no network
    /// round-trip, no in-flight window where the queue could be superseded
    /// by a real user action before it lands. See `spawn_enrich_queue_state`
    /// for the separate, best-effort refresh of played/position state.
    pub(in crate::app) fn restore_queue_state(&mut self) {
        let Some(state) = crate::config::load_queue_state() else {
            log::info!(target: "queue", "restore: no queue_state.json found, nothing to restore");
            return;
        };
        if state.items.is_empty() {
            log::info!(target: "queue", "restore: queue_state.json has no items, nothing to restore");
            return;
        }
        let queue_items = state.items;
        let restored_count = queue_items.len();
        let cursor = super::super::actions::queue_restore_cursor(
            &queue_items,
            state.cursor,
            state.last_played_content_id.as_ref(),
            state.last_played_item_id.as_deref(),
            state.last_played_completed,
        );
        self.last_played_item_id = state.last_played_item_id;
        self.last_played_completed = state.last_played_completed;
        self.queue_source = state.source;
        self.player_tab.set_queue_items(queue_items, cursor);
        self.queue_dirty = false;
        log::info!(target: "queue", "restore: restored {restored_count} item(s), cursor={cursor}");
        self.spawn_enrich_queue_state(state.positions);
    }

    /// Best-effort background refresh of played/position state for whatever
    /// is currently in `player_tab.items` (populated by `restore_queue_state`
    /// just before this is called). Merges by item ID into the *current*
    /// queue when it resolves, so it can never resurrect an item the user
    /// has since consumed, nor clobber a queue they've since replaced —
    /// unlike a wholesale overwrite, an ID that's no longer present is simply
    /// skipped.
    pub(in crate::app) fn spawn_enrich_queue_state(
        &self,
        positions: std::collections::HashMap<String, i64>,
    ) {
        let item_ids: Vec<String> = self
            .player_tab
            .emby_items()
            .iter()
            .map(|e| e.id.clone())
            .collect();
        if item_ids.is_empty() {
            return;
        }
        let Some(client) = self.emby_snapshot() else {
            return;
        };
        let tx = self.lib_tx.clone();
        std::thread::spawn(move || {
            let mut items = match client.get_items_by_ids(&item_ids) {
                Ok(items) => items,
                Err(e) => {
                    log::warn!(target: "queue", "restore: enrichment fetch failed: {e}");
                    return;
                }
            };
            // Apply locally-saved positions where they are fresher than what Emby returned.
            // Emby's UserData may lag by up to a few seconds after a Stopped report.
            for item in &mut items {
                if let Some(&saved_pos) = positions.get(&item.id) {
                    if saved_pos > item.playback_position_ticks {
                        log::info!(target: "player", "restore: applying saved pos={}s (Emby had {}s) for item={}",
                            saved_pos / mbv_core::api::TICKS_PER_SECOND,
                            item.playback_position_ticks / mbv_core::api::TICKS_PER_SECOND,
                            item.id);
                        item.playback_position_ticks = saved_pos;
                    }
                }
            }
            let _ = tx.send(LibEvent::QueueEnriched { items });
        });
    }

    /// Enqueue an explicitly resolved Home Continue Watching item (task 5.3d,
    /// Home effect decoupling): the shared Home branch of the former
    /// `enqueue_selected` (deleted in task 4.3, R1 — the item is now
    /// resolved at the caller), extracted so the CW effects pass the already-
    /// resolved item directly instead of temporarily pointing `home.section`
    /// at section 0 and re-reading `current_home_item`. Folder/non-playable
    /// guards and the ancestor-route resolution match the legacy Home branch
    /// exactly.
    pub(in crate::app) fn enqueue_home_item(&mut self, item: EmbyItem) {
        if item.is_folder {
            self.do_enqueue_folder(item);
            return;
        }
        if !is_playable(&item) {
            return;
        }
        log::info!(target: "library_route", "user action=enqueue item_id={:?} item_name={:?} source=home", item.id, item.name);
        let resolved = self.route_for_item_via_ancestors(&item.id).map(|(n, _)| n);
        if self.enqueue_route_conflict(resolved) {
            return;
        }
        self.append_item_to_queue_and_sync(item);
    }

    /// Enqueue an explicitly resolved library-view item (task 5.3d, Album
    /// track focus): the shared Emby branch of `enqueue_selected` (deleted in
    /// task 4.3, R1), extracted so the shell can enqueue a focused album
    /// track whose item resolution lives at the shell/component boundary
    /// rather than in `current_lib_item`.
    pub(in crate::app) fn enqueue_lib_item(&mut self, lib_idx: usize, item: EmbyItem) {
        if item.is_folder {
            self.do_enqueue_folder(item);
            return;
        }
        if !is_playable(&item) {
            return;
        }
        log::info!(target: "library_route", "user action=enqueue item_id={:?} item_name={:?} source=library-view", item.id, item.name);
        let resolved = self.route_for_active_library_view(lib_idx).map(|(n, _)| n);
        if self.enqueue_route_conflict(resolved) {
            return;
        }
        self.append_item_to_queue_and_sync(item);
    }

    /// Shared append/sync/rollback tail for a single-item enqueue
    /// (extracted from `enqueue_selected`'s two branches, which had
    /// duplicated this verbatim before the wrapper was deleted in task
    /// 4.3, R1): appends `item` to the visible queue,
    /// marks local queue metadata dirty when applicable, and syncs the
    /// append to the direct-remote queue / local persistence -- rolling
    /// the whole append back if the sync fails.  The visible queue
    /// mutation is the success confirmation; no enqueue success toast is
    /// emitted.
    pub(super) fn append_item_to_queue_and_sync(&mut self, item: EmbyItem) {
        let scope = self.viewed_queue_scope();
        let appended = item.clone();
        let previous_dirty = self.queue_dirty;
        let previous_queue = self.queue_for_scope(scope).clone();
        self.queue_for_scope_mut(scope).append_item(item);
        if self.local_queue_metadata_applies(scope) {
            self.queue_dirty = true;
        }
        if self.sync_playback_queue_after_append(scope, vec![appended]) {
            self.persist_local_queue_state_if_needed(scope);
            self.retire_remote_tracking(true);
        } else {
            self.queue_dirty = previous_dirty;
            *self.queue_for_scope_mut(scope) = previous_queue;
        }
    }
}
