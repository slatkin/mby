use super::types_playback::PlayheadConfidence;
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

    /// Reconciles the playhead projection against fresh player status: the one
    /// place that mutates `self.playhead` from `player.status`, resolving the
    /// prediction (active slot, progress-suppression) only. Position/runtime are
    /// never snapshotted here -- they are read live per frame in
    /// `effective_playback_state`. Runs once per tick after player events drain,
    /// never during paint (`queue-canonical-list`: reconciliation does not run
    /// during paint). Local playback only -- when a remote session or cast owns
    /// playback the local status snapshot is irrelevant.
    pub(super) fn reconcile_playhead(&mut self) {
        if self.connected_session_state.is_some() || self.cast_effective_playback_state().is_some()
        {
            return;
        }
        let (current_idx, queue_len) = {
            let s = self.player.status.lock().unwrap();
            (s.current_idx, s.queue_len)
        };
        if matches!(self.playhead.confidence, PlayheadConfidence::Predicted(_))
            && current_idx == self.playhead.slot
            && queue_len == self.player_tab.total_queue_len()
        {
            self.playhead.confidence = PlayheadConfidence::Confirmed;
        }
        if matches!(self.playhead.confidence, PlayheadConfidence::Confirmed) {
            self.playhead.slot = current_idx;
        }
    }

    /// Returns playback state for rendering. A pure reader: predictions are
    /// cleared only by `reconcile_playhead` on the event tick, never here.
    pub(super) fn effective_playback_state(&self) -> super::PlaybackState {
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
            // Only the active slot is a reconciled prediction; `active`/`paused`
            // and position/runtime are read live off status. The sole exception
            // is `Predicted(ItemSelected)`, where the lock still holds the
            // previous item's position, so progress is forced to 0/0 until the
            // player thread reconciles.
            let active_idx = match self.playhead.confidence {
                PlayheadConfidence::Predicted(_) => self.playhead.idx(),
                PlayheadConfidence::Confirmed => s.current_idx,
            };
            let (position_ticks, runtime_ticks) = if self.playhead.suppresses_progress() {
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

    pub(super) fn displayed_queue_playback_state(&self) -> super::PlaybackState {
        if self.queue_scope_is_playback(self.visible_queue_scope()) {
            self.effective_playback_state()
        } else {
            super::PlaybackState::default()
        }
    }
}
