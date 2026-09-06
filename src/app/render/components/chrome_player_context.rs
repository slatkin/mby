use super::chrome_player::PlaybackRenderContext;
use crate::app::layout::LayoutPlayback;
use crate::app::{palette, App, PanelMode};
use mbv_core::api::TICKS_PER_SECOND;
use ratatui::layout::Rect;
use ratatui::style::Color;

impl App {
    pub(in crate::app) fn playback_panel_context<'a>(
        &'a mut self,
        area: Rect,
        playback: &'a mut LayoutPlayback,
        player_h: u16,
        show_controls: bool,
        now_playing_title: &Option<(String, Color)>,
        panel_bg: Color,
    ) -> PlaybackRenderContext<'a> {
        PlaybackRenderContext {
            area,
            playback,
            player_h,
            show_controls,
            now_playing_title: now_playing_title.clone(),
            panel_bg,
            narrow_player: self.effective_panel_mode() == PanelMode::QueueOnly,
            progress: self.playback_progress(),
            use_nerd_fonts: self.use_nerd_fonts,
            stop_available: self.connected_session_id.is_some()
                || self.player.status.lock().unwrap().active,
            next_available: self.transport_prev_next_available().1,
            status_indicators: self.build_status_indicator_spans(),
            throbber: self.now_playing_throbber_span(),
            title_parts: now_playing_title
                .as_ref()
                .map(|(title, color)| self.playback_title_parts(title, *color))
                .unwrap_or_default(),
            idle_feed_title: self.idle_feed.as_ref().and_then(|feed| {
                feed.items.get(feed.current_index).map(|item| {
                    (
                        item.title.clone(),
                        item.link.as_deref().is_some_and(|link| !link.is_empty()),
                    )
                })
            }),
            marquee_text: &mut self.marquee_text,
            marquee_started_at: &mut self.marquee_started_at,
        }
    }

    pub(in crate::app) fn playback_title_parts(
        &mut self,
        title: &str,
        title_color: Color,
    ) -> Vec<(String, Color)> {
        let playback = self.effective_playback_state();
        playback
            .active
            .then(|| self.playback_queue().emby_item_at(playback.active_idx))
            .flatten()
            .filter(|item| item.item_type == "Episode" && !item.series_name.is_empty())
            .filter(|item| item.display_name() == title)
            .map(|item| {
                vec![
                    (item.series_name.clone(), palette::TEXT_FOCUS_ACCENT),
                    (format!(" {}", item.name), palette::STATUS_AVAILABLE),
                ]
            })
            .unwrap_or_else(|| vec![(title.to_string(), title_color)])
    }

    pub(in crate::app) fn playback_progress(&self) -> (i64, i64, bool) {
        if let Some(ref remote) = self.connected_session_state {
            let elapsed_s = self.remote_pos_at.elapsed().as_secs_f64();
            let pos_s = (self.remote_pos_s as f64 + elapsed_s).min(remote.runtime_s as f64);
            (
                (pos_s * TICKS_PER_SECOND as f64) as i64,
                remote.runtime_s * TICKS_PER_SECOND,
                self.playback_transport_paused(),
            )
        } else {
            let status = self.player.status.lock().unwrap();
            (status.position_ticks, status.runtime_ticks, status.paused)
        }
    }
}
