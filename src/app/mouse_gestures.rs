//! Shell-invoked mouse effect handlers for migrated interactive surfaces.

use crate::app::action::Command;
use crate::app::components::msg::TvHit;
use crate::app::{App, QueueCursorPush, QueueScope};
use mbv_core::api::TICKS_PER_SECOND;
use mbv_core::player::PlayerCommand;
use mbv_core::remote_reconciliation::RemoteIntent;
use std::time::{Duration, Instant};

impl App {
    /// Seek to a 0.0..=1.0 `fraction` of the runtime. `PlaybackComponent`
    /// resolves the click column against its own painted `seekbar_area`, so the
    /// shell never reads that component-owned geometry.
    pub(super) fn seek_to_fraction(&mut self, fraction: f64) {
        if let Some(ref conn_id) = self.connected_session_id.clone() {
            let runtime_s = self
                .connected_session_state
                .as_ref()
                .map(|s| s.runtime_s)
                .unwrap_or(0);
            if runtime_s == 0 {
                return;
            }
            let ticks = (fraction * (runtime_s * mbv_core::api::TICKS_PER_SECOND) as f64) as i64;
            let id = conn_id.clone();
            self.remote_pos_s = (fraction * runtime_s as f64) as i64;
            self.remote_pos_at = Instant::now();
            self.remote_seek_pending_until = Instant::now() + Duration::from_secs(4);
            self.issue_remote_intent(RemoteIntent::Seek);
            self.do_reconciliation_session_command(&id.clone(), move |c| {
                c.session_seek(&id, ticks)
            });
            return;
        }
        let runtime_ticks = self.player.status.lock().unwrap().runtime_ticks;
        if runtime_ticks == 0 {
            return;
        }
        let target_secs = (fraction * runtime_ticks as f64) / TICKS_PER_SECOND as f64;
        self.player
            .send_command(PlayerCommand::SeekAbsolute(target_secs));
        // Mark a pending Feed seek so the next OutputStarted persists
        // the resulting position (confirmed seek completion).
        if let Some(slot_id) = self.playback_queue().queue.active_slot_id() {
            if let Some(slot) = self.playback_queue().queue.slot(slot_id) {
                if matches!(slot.item, mbv_core::playback_queue::QueueItem::Feed(ref e) if e.feed_id.is_some())
                {
                    self.feed_seek_pending_slot = Some(slot_id);
                }
            }
        }
    }

    pub(super) fn handle_mouse_scroll_queue(&mut self, delta: i64) {
        let n = self.displayed_queue().total_queue_len();
        if n > 0 {
            let scope = self.viewed_queue_scope();
            let queue = self.displayed_queue_mut();
            queue.queue_cursor = super::ui_util::move_cursor(queue.queue_cursor, delta * 3, n);
            // The user's own wheel input: the mounted QueueComponent must adopt
            // this index rather than reconciling by slot identity.
            self.playhead.pending_push = Some(QueueCursorPush::Reanchor(scope));
        }
    }

    pub(super) fn handle_mouse_single_click_emby(&mut self, lib_idx: usize, target: usize) {
        self.set_panel_focus(super::PanelFocus::Library);
        if let Some(level) = self
            .libs
            .get_mut(lib_idx)
            .and_then(|lib| lib.nav_stack.last_mut())
        {
            if target < level.items.len() {
                level.set_resting_cursor(target);
                self.save_default_library_position(lib_idx);
            }
        }
    }

    pub(super) fn handle_mouse_single_click_queue(
        &mut self,
        slot_id: Option<mbv_core::playback_queue::QueueSlotId>,
    ) -> Option<usize> {
        self.set_panel_focus(super::PanelFocus::Queue);
        let slot_id = slot_id?;
        let index = self
            .displayed_queue()
            .queue
            .slots()
            .iter()
            .position(|slot| slot.slot_id == slot_id)?;
        self.mark_queue_cursor_user_active();
        self.displayed_queue_mut().queue_cursor = index;
        Some(index)
    }

    pub(super) fn handle_mouse_selector_click_queue(&mut self, scope: QueueScope) {
        self.set_queue_scope(scope);
    }

    pub(super) fn handle_mouse_selector_click_emby(&mut self, lib_idx: usize, target: usize) {
        if self.is_music_group_view(lib_idx) {
            self.select_music_group(lib_idx, target);
        } else if self.is_feed_home_video_group_view(lib_idx) {
            self.select_feed_folder_group(lib_idx, target);
        } else if self.should_show_letter_pills(lib_idx) {
            self.select_letter_pill(lib_idx, target);
        }
    }

    pub(super) fn handle_mouse_double_click_emby(&mut self, lib_idx: usize, target: usize) {
        self.handle_mouse_single_click_emby(lib_idx, target);
        if self.is_viewing_album_folders(lib_idx) {
            let album = self.libs[lib_idx]
                .nav_stack
                .last()
                .and_then(|level| level.items.get(target))
                .cloned();
            self.activate_album_folder_row(album);
        } else if !self.activate_selected_series(lib_idx) {
            // The double-click already landed `target` as the level cursor;
            // resolve the item at it and activate via the item-taking tail
            // (task 4.3, R1: `select`'s cursor read is gone).
            if let Some(item) = self.current_lib_item(lib_idx, target) {
                self.select_item(lib_idx, item);
            }
        }
    }

    pub(super) fn handle_mouse_double_click_queue(
        &mut self,
        slot_id: Option<mbv_core::playback_queue::QueueSlotId>,
    ) {
        // The single-click resolves the clicked slot to an index; that
        // resolved index is passed straight to the play effect (D2) instead
        // of being recovered from `queue_cursor`, so a follow update cannot
        // play a different row.
        if let Some(index) = self.handle_mouse_single_click_queue(slot_id) {
            self.dispatch(Command::QueuePlayCursor(index));
        }
    }

    pub(super) fn handle_mouse_right_click_emby(
        &mut self,
        lib_idx: usize,
        target: usize,
        col: u16,
        row: u16,
    ) {
        self.handle_mouse_single_click_emby(lib_idx, target);
        // Emby-library right-click is never a Home-tab menu, so the
        // Continue-Watching-selected fact and the CW item are harmless
        // `false`/`None` (the `self.tab.is_home()` guard short-circuits them).
        self.open_context_menu_at(col, row, false, None);
    }

    pub(super) fn handle_mouse_right_click_queue(
        &mut self,
        slot_id: Option<mbv_core::playback_queue::QueueSlotId>,
        col: u16,
        row: u16,
        home_cw_selected: bool,
    ) {
        self.handle_mouse_single_click_queue(slot_id);
        // Queue right-click renders the queue item; the Continue Watching
        // item (for the odd Remove-from-CW coupling entry) is resolved at
        // the Model boundary at execution time, so `None` is correct here
        // (task 5.3d).
        self.open_context_menu_at(col, row, home_cw_selected, None);
    }

    /// Keyboard `.` in the Queue panel (design: `.` is a selection-dependent
    /// chord owned by the focused component, not the central router). Pins the
    /// selection to the component-resolved slot and opens the context menu
    /// anchored at the selected row (legacy `SelectedItem` anchor), the same
    /// menu the right-click path builds.
    pub(super) fn handle_keyboard_context_menu_queue(
        &mut self,
        slot_id: Option<mbv_core::playback_queue::QueueSlotId>,
        home_cw_selected: bool,
    ) {
        self.handle_mouse_single_click_queue(slot_id);
        self.open_context_menu(home_cw_selected, None);
    }

    pub(super) fn handle_mouse_single_click_tv(&mut self, lib_idx: usize, hit: TvHit) {
        match hit {
            TvHit::SeasonTab(_) | TvHit::EpisodeRow(_) => {
                self.set_panel_focus(super::PanelFocus::Library);
            }
            TvHit::SeriesRow(target) => {
                // The component resolved the series under the click; apply it
                // to `App`'s library cursor before any further pane effect.
                if let Some(level) = self.libs[lib_idx].nav_stack.last_mut() {
                    level.set_resting_cursor(target);
                }
            }
            TvHit::EpisodesPane => {}
        }
    }

    pub(super) fn handle_mouse_double_click_tv(&mut self, lib_idx: usize, hit: TvHit) {
        if let TvHit::SeriesRow(target) = hit {
            // Apply the clicked series before activating (the click may land
            // on a series other than the focused one).
            if let Some(level) = self.libs[lib_idx].nav_stack.last_mut() {
                level.set_resting_cursor(target);
            }
        }
        if matches!(hit, TvHit::EpisodeRow(_) | TvHit::SeriesRow(_)) {
            self.activate_selected_series(lib_idx);
        }
    }

    pub(super) fn handle_mouse_right_click_tv(
        &mut self,
        lib_idx: usize,
        hit: TvHit,
        col: u16,
        row: u16,
    ) {
        self.handle_mouse_single_click_tv(lib_idx, hit);
        // TV-workspace right-click is never a Home-tab menu, so the
        // Continue-Watching-selected fact and the CW item are harmless
        // `false`/`None`.
        self.open_context_menu_at(col, row, false, None);
    }
}
