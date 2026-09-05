use super::types_playback::PendingActiveIdx;
use super::{App, PlaybackTarget};
use crate::app::render::indicators::IndicatorData;

impl PlaybackTarget {
    pub(super) fn toggle_play_pause(&self, app: &mut App) {
        match self {
            Self::Local(target) => target.toggle_play_pause(app),
            Self::Remote(target) => target.toggle_play_pause(app),
            Self::Cast(target) => target.toggle_play_pause(app),
        }
    }

    pub(super) fn stop(&self, app: &mut App) {
        match self {
            Self::Local(target) => target.stop(app),
            Self::Remote(target) => target.stop(app),
            Self::Cast(target) => target.stop(app),
        }
    }

    pub(super) fn seek_relative(&self, app: &mut App, delta: f64) {
        match self {
            Self::Local(target) => target.seek_relative(app, delta),
            Self::Remote(target) => target.seek_relative(app, delta),
            Self::Cast(target) => target.seek_relative(app, delta),
        }
    }

    pub(super) fn jump_track(&self, app: &mut App, step: i64, transport: &'static str) {
        match self {
            Self::Local(target) => target.jump_track(app, step),
            Self::Remote(target) => target.jump_track(app, step, transport),
            Self::Cast(target) => target.jump_track(app, step),
        }
    }

    pub(super) fn toggle_command_mute(&self, app: &mut App) {
        match self {
            Self::Local(target) => target.toggle_command_mute(app),
            Self::Remote(target) => target.toggle_command_mute(app),
            Self::Cast(target) => target.toggle_command_mute(app),
        }
    }

    pub(super) fn is_audio_item(&self, app: &App) -> bool {
        match self {
            Self::Local(target) => target.is_audio_item(app),
            Self::Remote(target) => target.is_audio_item(app),
            Self::Cast(target) => target.is_audio_item(app),
        }
    }

    pub(super) fn toggle_soft_mute(&self, app: &mut App) {
        match self {
            Self::Local(target) => target.toggle_soft_mute(app),
            Self::Remote(target) => target.toggle_soft_mute(app),
            Self::Cast(target) => target.toggle_soft_mute(app),
        }
    }

    pub(super) fn cycle_audio(&self, app: &mut App) {
        match self {
            Self::Local(target) => target.cycle_audio(app),
            Self::Remote(target) => target.cycle_audio(app),
            Self::Cast(target) => target.cycle_audio(app),
        }
    }

    pub(super) fn adjust_volume(&self, app: &mut App, delta: i64) {
        match self {
            Self::Local(target) => target.adjust_volume(app, delta),
            Self::Remote(target) => target.adjust_volume(app, delta),
            Self::Cast(target) => target.adjust_volume(app, delta),
        }
    }

    pub(super) fn cycle_sub(&self, app: &mut App) {
        match self {
            Self::Local(target) => target.cycle_sub(app),
            Self::Remote(target) => target.cycle_sub(app),
            Self::Cast(target) => target.cycle_sub(app),
        }
    }

    pub(super) fn displayed_volume(&self, app: &App) -> i64 {
        match self {
            Self::Local(target) => target.displayed_volume(app),
            Self::Remote(target) => target.displayed_volume(app),
            Self::Cast(target) => target.displayed_volume(app),
        }
    }

    pub(super) fn displayed_mute(&self, app: &App) -> bool {
        match self {
            Self::Local(target) => target.displayed_mute(app),
            Self::Remote(target) => target.displayed_mute(app),
            Self::Cast(target) => target.displayed_mute(app),
        }
    }

    pub(super) fn indicator_data(&self, app: &App) -> Option<IndicatorData> {
        match self {
            Self::Local(target) => target.indicator_data(app),
            Self::Remote(target) => target.indicator_data(app),
            Self::Cast(target) => target.indicator_data(app),
        }
    }
}

impl App {
    /// Whether the connected transport is currently paused. For remote
    /// sessions, returns true once a single API poll has observed
    /// `IsPaused=true` without a position advance (typically within one
    /// poll after the user pauses remotely). For pos-advancing clients that
    /// always report `IsPaused=true` (some Emby Web builds), the
    /// position-advance observation each poll keeps this returning false.
    pub(super) fn playback_transport_paused(&self) -> bool {
        if let Some(paused) = self
            .cast_attachment
            .as_ref()
            .and_then(|a| a.status.as_ref())
            .map(|s| s.state == mbv_core::cast_client::CastPlaybackState::Paused)
        {
            return paused;
        }
        if self.connected_session_state.is_some() {
            return self.remote_stalled_while_paused;
        }
        self.player.status.lock().unwrap().paused
    }

    /// Returns playback state for rendering, consuming an optimistic active
    /// index only after the player status matches the locally edited queue.
    /// The mutable receiver is intentional: reconciliation clears the pending
    /// prediction once the player thread catches up.
    pub(super) fn effective_playback_state(&mut self) -> super::PlaybackState {
        if let Some(state) = self.cast_effective_playback_state() {
            state
        } else if let Some(ref remote) = self.connected_session_state {
            let maybe_active_idx = remote.now_playing_item_id.as_ref().and_then(|id| {
                self.player_tab
                    .queue
                    .slots()
                    .iter()
                    .position(|s| s.item.id() == id)
            });
            let active_idx = maybe_active_idx.unwrap_or(0);
            let pos_ticks = {
                let elapsed_s = if remote.is_paused {
                    0.0
                } else {
                    self.remote_pos_at.elapsed().as_secs_f64()
                };
                let pos_s = (self.remote_pos_s as f64 + elapsed_s).min(remote.runtime_s as f64);
                (pos_s * mbv_core::api::TICKS_PER_SECOND as f64) as i64
            };
            super::PlaybackState {
                active: remote.now_playing.is_some() && maybe_active_idx.is_some(),
                active_idx,
                position_ticks: pos_ticks,
                runtime_ticks: remote.runtime_s * mbv_core::api::TICKS_PER_SECOND,
                paused: remote.is_paused,
            }
        } else {
            let s = self.player.status.lock().unwrap();
            // `suppress_progress` is set only while an unreconciled `Jump`
            // prediction is showing a different item than the one the player
            // status still describes — its position/runtime are the previous
            // item's and would render as stale progress on the new row.
            let (active_idx, suppress_progress) = match self.pending_active_idx {
                Some(pending)
                    if s.current_idx == pending.idx()
                        && s.queue_len == self.player_tab.total_queue_len() =>
                {
                    self.pending_active_idx = None;
                    (s.current_idx, false)
                }
                Some(pending) => (pending.idx(), matches!(pending, PendingActiveIdx::Jump(_))),
                None => (s.current_idx, false),
            };
            let (position_ticks, runtime_ticks) = if suppress_progress {
                (0, 0)
            } else {
                (s.position_ticks, s.runtime_ticks)
            };
            super::PlaybackState {
                active: s.active,
                active_idx,
                position_ticks,
                runtime_ticks,
                paused: s.paused,
            }
        }
    }

    pub(super) fn displayed_queue_playback_state(&mut self) -> super::PlaybackState {
        if self.queue_scope_is_playback(self.visible_queue_scope()) {
            self.effective_playback_state()
        } else {
            super::PlaybackState::default()
        }
    }
}
