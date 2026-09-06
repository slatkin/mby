use super::notify_actions::ToastSeverity;
use super::ui_util::natural_sort_key;
use super::{App, LocalPlaybackTarget, PanelFocus, PlaybackTarget, RemotePlaybackTarget};
use mbv_core::api::EmbyItem;
use mbv_core::playback_queue::{QueueItem, QueueItemContentId};
use mbv_core::player::PlayerCommand;
use mbv_core::ItemId;
use std::sync::Arc;

/// Where playback should resume within a restored queue. Prefers locating
/// `last_played_content_id` by identity (robust to the saved `cursor` index having
/// drifted, e.g. if the list was edited before the last save) and falls back
/// to the saved cursor only when there's no last-played id to anchor on.
pub(crate) fn queue_restore_cursor(
    items: &[QueueItem],
    saved_cursor: usize,
    last_played_content_id: Option<&QueueItemContentId>,
    legacy_last_played_item_id: Option<&str>,
    last_played_completed: bool,
) -> usize {
    let fallback = saved_cursor.min(items.len().saturating_sub(1));
    let identity = last_played_content_id.cloned().or_else(|| {
        let id = legacy_last_played_item_id?;
        let mut matches = items.iter().filter(|item| item.id() == id);
        let first = matches.next()?;
        if matches.next().is_some() {
            None
        } else {
            Some(first.content_id())
        }
    });
    let Some(identity) = identity else {
        return fallback;
    };
    // If the last-played item is no longer in the restored list (e.g. it was
    // removed from the queue before quitting), fall back to the saved cursor
    // rather than silently jumping to the front of the queue.
    let mut matches = items
        .iter()
        .enumerate()
        .filter(|(_, item)| item.content_id() == identity);
    let Some((idx, _)) = matches.next() else {
        return fallback;
    };
    if matches.next().is_some() {
        return fallback;
    }
    if last_played_completed {
        (idx + 1).min(items.len().saturating_sub(1))
    } else {
        idx
    }
}

impl App {
    /// Cast takes priority over an attached Emby session: the two are
    /// mutually exclusive attachment slots (see `remote_slot_state.rs`), but
    /// this ordering keeps the seam correct even if that invariant is ever
    /// relaxed.
    pub(super) fn playback_target(&self) -> PlaybackTarget {
        if self.cast_attachment.is_some() {
            return PlaybackTarget::Cast(super::CastPlaybackTarget);
        }
        match self.connected_session_id.clone() {
            Some(session_id) => PlaybackTarget::Remote(RemotePlaybackTarget { session_id }),
            None => PlaybackTarget::Local(LocalPlaybackTarget),
        }
    }

    pub(super) fn playback_display_target(&self) -> PlaybackTarget {
        if self.cast_attachment.is_some() || self.connected_session_state.is_some() {
            self.playback_target()
        } else {
            PlaybackTarget::Local(LocalPlaybackTarget)
        }
    }

    pub(super) fn playback_indicator_target(&self) -> PlaybackTarget {
        let local_active = self.player.status.lock().unwrap().active;
        if local_active {
            PlaybackTarget::Local(LocalPlaybackTarget)
        } else {
            self.playback_display_target()
        }
    }
}

impl App {
    pub(super) fn remote_audio_indexes(&self) -> Vec<i64> {
        self.connected_session_state
            .as_ref()
            .map(|state| {
                state
                    .media_info
                    .audio_streams
                    .iter()
                    .map(|stream| stream.index)
                    .collect()
            })
            .unwrap_or_default()
    }

    pub(super) fn remote_subtitle_indexes(&self) -> Vec<i64> {
        self.connected_session_state
            .as_ref()
            .map(|state| {
                state
                    .media_info
                    .subtitle_streams
                    .iter()
                    .map(|stream| stream.index)
                    .collect()
            })
            .unwrap_or_default()
    }

    pub(super) fn lib_page_size(&self) -> usize {
        // The library list is rendered into the right panel; use the panel
        // height directly (rows are single-line; subtract 1 for the
        // count/search header line).
        (self.layout.main.left_area.height as usize)
            .saturating_sub(1)
            .max(1)
    }

    /// Resolve the item the library panel currently selects. `cursor` is the
    /// resolved index the caller owns (component-resolved for the generic
    /// browser, or the App nav-level cursor on the legacy context-menu/mouse
    /// paths) — never re-read from `BrowseLevel` (task 4.3, R1).
    pub(super) fn current_lib_item(&self, lib_idx: usize, cursor: usize) -> Option<EmbyItem> {
        let lib = self.libs.get(lib_idx)?;
        if lib.nav_stack.is_empty() {
            Some(lib.library.clone())
        } else {
            if self.is_feed_home_video_group_view(lib_idx) {
                return self.selected_feed_home_video_item(lib_idx);
            }
            let lvl = lib.nav_stack.last()?;
            lvl.items.get(cursor).cloned()
        }
    }

    pub(super) fn play_items_routed(
        &mut self,
        items: Vec<EmbyItem>,
        start_idx: usize,
        queue_source: crate::config::QueueSource,
    ) {
        if let Some(item) = items.get(start_idx).or_else(|| items.first()) {
            log::info!(target: "library_route", "user action=queue-replace item_id={:?} item_name={:?}", item.id, item.name);
            if self.in_non_library_thin_client_mode() {
                log::info!(target: "library_route", "route bypass action=queue-replace item_id={:?} item_name={:?} reason=non-library thin-client owns playback", item.id, item.name);
            } else {
                let item = item.clone();
                self.apply_route_for_playback(&item);
            }
        }
        let direct_remote = self.has_direct_remote_queue();
        if !direct_remote {
            self.on_queue_replace_silent();
        }
        self.queue_source = queue_source;
        self.set_queue_scope(self.playing_queue_scope());
        // Keep library focus when playing from the library panel.
        if !matches!(self.effective_panel_focus(), PanelFocus::Library) {
            self.set_panel_focus(PanelFocus::Queue);
        }
        if let Some(ref conn_id) = self.connected_session_id.clone() {
            self.clear_playback_overlays();
            let id = conn_id.clone();
            let label = items
                .get(start_idx)
                .map(|i| i.playback_label())
                .unwrap_or_default();
            self.flash(
                format!("Requesting playback: {label}"),
                ToastSeverity::Neutral,
            );
            self.submit_attached_sequence(&id, &items, start_idx);
            return;
        }
        let Some(c) = self.emby_snapshot().map(Arc::new) else {
            self.flash("Emby is unavailable".into(), ToastSeverity::Warning);
            return;
        };
        if direct_remote {
            if let Some(item) = items.get(start_idx) {
                self.flash(
                    format!("Requesting playback: {}", item.playback_label()),
                    ToastSeverity::Neutral,
                );
            }
        }
        self.player.play_queue(
            items,
            start_idx,
            self.queue_source.clone(),
            c,
            self.ui_volume,
        );
        self.player
            .send_command(PlayerCommand::SetMute(self.mute_on));
    }

    pub(super) fn play_item(&mut self, item: EmbyItem) {
        log::info!(target: "library_route", "user action=play item_id={:?} item_name={:?}", item.id, item.name);
        if self.in_non_library_thin_client_mode() {
            log::info!(target: "library_route", "route bypass action=play item_id={:?} item_name={:?} reason=non-library thin-client owns playback", item.id, item.name);
        } else {
            self.apply_route_for_playback(&item);
        }
        let direct_remote = self.has_direct_remote_queue();
        if !direct_remote {
            self.on_queue_replace_silent();
        }
        // Keep library focus when playing from the library panel.
        if !matches!(self.effective_panel_focus(), PanelFocus::Library) {
            self.set_panel_focus(PanelFocus::Queue);
        }
        let label = item.playback_label();
        if let Some(ref conn_id) = self.connected_session_id.clone() {
            self.retire_remote_tracking(true);
            self.clear_playback_overlays();
            let id = conn_id.clone();
            let item_id = item.id.clone();
            let start_ticks = item.playback_position_ticks;
            self.flash(
                format!("Requesting playback: {label}"),
                ToastSeverity::Neutral,
            );
            self.do_session_command(move |c| c.session_play(&id, &item_id, start_ticks));
            return;
        }
        if !item.series_id.is_empty() && self.player.always_play_next {
            let Some(client) = self.emby_client() else {
                self.flash("Emby is unavailable".into(), ToastSeverity::Warning);
                return;
            };
            let c = client.lock().unwrap();
            let episodes = c.get_episodes_from(
                &ItemId::new(item.series_id.as_str()),
                &ItemId::new(item.id.as_str()),
            );
            drop(c);
            if episodes.len() > 1 {
                let Some(c) = self.emby_snapshot().map(Arc::new) else {
                    self.flash("Emby is unavailable".into(), ToastSeverity::Warning);
                    return;
                };
                if !direct_remote {
                    self.on_queue_replace_silent();
                    self.replace_playback_queue(episodes.clone(), 0);
                }
                self.queue_source = crate::config::QueueSource::Series;
                self.player
                    .play_queue(episodes, 0, self.queue_source.clone(), c, self.ui_volume);
                self.player
                    .send_command(PlayerCommand::SetMute(self.mute_on));
                if !self.has_direct_remote_queue() {
                    self.save_queue_state();
                }
                return;
            }
        }
        let Some(c) = self.emby_snapshot().map(Arc::new) else {
            self.flash("Emby is unavailable".into(), ToastSeverity::Warning);
            return;
        };
        if !direct_remote {
            self.replace_playback_queue(vec![item.clone()], 0);
        } else {
            self.flash(
                format!("Requesting playback: {label}"),
                ToastSeverity::Neutral,
            );
        }
        self.player
            .play(&item, self.queue_source.clone(), c, self.ui_volume);
        self.player
            .send_command(PlayerCommand::SetMute(self.mute_on));
    }

    pub(super) fn do_enqueue_folder(&mut self, item: mbv_core::api::EmbyItem) {
        log::info!(target: "library_route", "user action=enqueue item_id={:?} item_name={:?}", item.id, item.name);
        let resolved = self.resolve_route_for_enqueue_folder(&item);
        if self.enqueue_route_conflict(resolved) {
            return;
        }
        let Some(client) = self.emby_client() else {
            self.flash("Emby is unavailable".into(), ToastSeverity::Warning);
            return;
        };
        let client = client.lock().unwrap();
        match client.get_all_playable_recursive(&item.id) {
            Ok(mut items) => {
                items.retain(|i| !i.is_folder);
                items.sort_by_key(|a| natural_sort_key(a.sort_key()));
                let count = items.len();
                drop(client);
                if count == 0 {
                    self.flash("Nothing to enqueue".into(), ToastSeverity::Error);
                    return;
                }
                let scope = self.viewed_queue_scope();
                let appended = items.clone();
                let previous_dirty = self.queue_dirty;
                let previous_queue = self.queue_for_scope(scope).clone();
                {
                    let queue = self.queue_for_scope_mut(scope);
                    queue.append_items(items);
                }
                if self.local_queue_metadata_applies(scope) {
                    self.queue_dirty = true;
                }
                if self.sync_playback_queue_after_append(scope, appended) {
                    self.persist_local_queue_state_if_needed(scope);
                    self.retire_remote_tracking(true);
                } else {
                    self.queue_dirty = previous_dirty;
                    *self.queue_for_scope_mut(scope) = previous_queue;
                }
            }
            Err(e) => {
                drop(client);
                self.flash(format!("Couldn't enqueue items: {e}"), ToastSeverity::Error);
            }
        }
    }

    /// Shared tail for submitting a single `QueueItem` to the canonical queue
    /// (Task 8.1): play looks up an existing slot by `content_id()`, appends
    /// if absent, sets cursor/active slot, and submits the full queue to the
    /// player, rolling the queue back and flashing on rejection; enqueue
    /// appends without starting playback and syncs/persists like the library
    /// enqueue path. Callers resolve their own provider-specific
    /// selection/admission ahead of the call. Returns whether the submit
    /// succeeded.
    pub(super) fn submit_queue_item(&mut self, item: QueueItem, start_playback: bool) -> bool {
        let scope = if start_playback {
            self.playing_queue_scope()
        } else {
            self.viewed_queue_scope()
        };
        if !start_playback {
            let previous_dirty = self.queue_dirty;
            let previous_queue = self.queue_for_scope(scope).clone();
            self.queue_for_scope_mut(scope).queue.append(item.clone());
            if self.local_queue_metadata_applies(scope) {
                self.queue_dirty = true;
            }
            if self.sync_playback_queue_items_after_append(scope, vec![item]) {
                self.persist_local_queue_state_if_needed(scope);
                self.retire_remote_tracking(true);
                return true;
            }
            self.queue_dirty = previous_dirty;
            *self.queue_for_scope_mut(scope) = previous_queue;
            return false;
        }
        let previous_queue = self.queue_for_scope(scope).clone();
        let existing_index = self
            .queue_for_scope(scope)
            .slots()
            .iter()
            .position(|slot| slot.item.content_id() == item.content_id());
        let selected_index = existing_index.unwrap_or_else(|| {
            self.queue_for_scope_mut(scope).queue.append(item.clone());
            self.queue_for_scope(scope).total_queue_len() - 1
        });
        let selected_slot = self
            .queue_for_scope(scope)
            .slot_id_at(selected_index)
            .expect("selected queue slot disappeared");
        {
            let queue = self.queue_for_scope_mut(scope);
            queue.queue_cursor = selected_index;
            let _ = queue.queue.set_active_slot(selected_slot);
        }
        let all_items = self.queue_for_scope(scope).all_queue_items();
        // While a cast target is attached, playing a selection dispatches it
        // to the receiver instead of the local player (cast-session-control
        // "Attaching to a cast target does not engage the local player").
        // `submit_queue`/local playback state below is never touched on this
        // path.
        if self.is_cast_attached() {
            self.dispatch_selection_to_cast(all_items, selected_index);
            self.set_queue_scope(scope);
            if !matches!(self.effective_panel_focus(), PanelFocus::Library) {
                self.set_panel_focus(PanelFocus::Queue);
            }
            return true;
        }
        let audio_only = all_items.iter().all(QueueItem::is_audio);
        let submitted =
            self.player
                .submit_queue(all_items, selected_index, None, audio_only, self.ui_volume);
        if !submitted {
            *self.queue_for_scope_mut(scope) = previous_queue;
            self.flash(
                "Playback owner rejected this item".into(),
                ToastSeverity::Error,
            );
            return false;
        }
        self.set_queue_scope(scope);
        if !matches!(self.effective_panel_focus(), PanelFocus::Library) {
            self.set_panel_focus(PanelFocus::Queue);
        }
        true
    }
}

#[cfg(test)]
#[path = "actions_tests_letter.rs"]
mod letter_tests;
#[cfg(test)]
#[path = "actions_tests_queue_enrich.rs"]
mod queue_enrich_tests;
#[cfg(test)]
#[path = "actions_tests_queue_state_controls.rs"]
mod queue_state_control_tests;
#[cfg(test)]
#[path = "actions_tests_queue_state.rs"]
mod queue_state_tests;
#[cfg(test)]
#[path = "actions_tests_queue.rs"]
mod queue_tests;
#[cfg(test)]
#[path = "actions_tests_routes.rs"]
mod route_tests;
#[cfg(test)]
#[path = "actions_tests.rs"]
mod tests;
