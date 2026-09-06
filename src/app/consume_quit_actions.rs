use super::App;
use std::time::{Duration, Instant};

impl App {
    /// `q` (and every keyboard/mouse path that routes here). Always a real
    /// quit for this client process: in stay-alive mode playback continues
    /// regardless, because the local daemon is a separate process that this
    /// client never owned. `mbv -q` / tray-Quit stop the daemon itself and
    /// are unaffected by this.
    ///
    /// Any dirty saved-playlist queue is saved/discarded **silently** per
    /// `save_playlist_on_quit` — no interactive modal (that modal is
    /// reserved for the attended ClearQueue/PlayItems cases; see issue
    /// #156).
    pub(super) fn try_quit(&mut self) -> bool {
        if self.queue_dirty && self.queue_is_saved_playlist() {
            let save_on_quit = self.config.lock().unwrap().save_playlist_on_quit;
            if save_on_quit {
                let playlist_id = self.queue_playlist_id().map(str::to_string);
                self.save_playlist_to_emby();
                if let Some(playlist_id) = playlist_id {
                    let deadline = Instant::now()
                        + Duration::from_secs(self.config.lock().unwrap().quit_timeout_secs);
                    while self.playlist_mutations.contains_key(&playlist_id)
                        && Instant::now() < deadline
                    {
                        let remaining = deadline.saturating_duration_since(Instant::now());
                        match self.sessions_rx.recv_timeout(remaining) {
                            Ok(event) => self.handle_session_event(event),
                            Err(_) => break,
                        }
                    }
                }
            } else {
                self.on_queue_replace_silent();
            }
        }
        self.save_prefs();
        if !self.player.is_remote() {
            self.player.stop();
        }
        true
    }

    /// Called when a video item is removed from the queue because "consume" is enabled.
    /// Marks the queue dirty (matching how other queue-mutating actions behave, so the
    /// user is prompted to save on quit/replace), and — if the user has opted in via
    /// `save_playlist_on_consume` and the current queue is a saved Emby playlist — pushes
    /// the shorter item list back to Emby immediately, so other devices loading this
    /// playlist see the consumed items already removed instead of stale, longer state.
    ///
    /// Both checks are gated on `local_queue_metadata_applies`: `save_playlist_to_emby`
    /// always pushes `player_tab.items` (the *local* queue), so if the consume actually
    /// happened on a direct-remote/daemon queue, autosaving here would push an unrelated,
    /// unmodified local playlist to Emby instead of the queue that actually changed.
    pub(super) fn on_video_consumed(&mut self) {
        let scope = self.playing_queue_scope();
        log::info!(target: "consume", "on_video_consumed: scope={scope:?} has_local_metadata={}",
            self.local_queue_metadata_applies(scope));
        if !self.local_queue_metadata_applies(scope) {
            return;
        }
        self.queue_dirty = true;
        let save_on_consume = self.config.lock().unwrap().save_playlist_on_consume;
        let is_saved_playlist = self.queue_is_saved_playlist();
        log::info!(target: "consume", "on_video_consumed: queue_dirty=true save_playlist_on_consume={save_on_consume} \
            is_saved_playlist={is_saved_playlist}");
        if save_on_consume && is_saved_playlist {
            self.queue_dirty = false;
            self.save_playlist_to_emby();
        }
    }

    /// Called when an audio item is removed from the queue because "consume" is enabled.
    /// Mirrors `on_video_consumed`, but is gated on the audio-specific
    /// `save_playlist_on_consume_audio` flag instead — kept as a separate method (rather
    /// than a shared helper with a boolean parameter) so the video and audio consume paths
    /// stay independently readable and don't require the caller to track which flag applies.
    pub(super) fn on_audio_consumed(&mut self) {
        let scope = self.playing_queue_scope();
        log::info!(target: "consume", "on_audio_consumed: scope={scope:?} has_local_metadata={}",
            self.local_queue_metadata_applies(scope));
        if !self.local_queue_metadata_applies(scope) {
            return;
        }
        self.queue_dirty = true;
        let save_on_consume = self.config.lock().unwrap().save_playlist_on_consume_audio;
        let is_saved_playlist = self.queue_is_saved_playlist();
        log::info!(target: "consume", "on_audio_consumed: queue_dirty=true \
            save_playlist_on_consume_audio={save_on_consume} is_saved_playlist={is_saved_playlist}");
        if save_on_consume && is_saved_playlist {
            self.queue_dirty = false;
            self.save_playlist_to_emby();
        }
    }
}
