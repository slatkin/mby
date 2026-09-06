use crate::app::layout::{
    AppLayout, CardGeometry, FrameChromeGeometry, LayoutMain, LayoutPlayback,
};
use crate::app::render::arrangements::chrome::{
    chrome_geometry, ChromeGeometryInput, PLAYER_BOX_HEIGHT,
};
use crate::app::render::arrangements::queue::{queue_panel_geometry, QueuePanelInputs};
use crate::app::render::components::queue::render_queue_status;
use crate::app::render::components::widgets::{render_queue_panel_frame, right_panel_content_area};
use crate::app::{palette, App, PanelFocus, PanelMode, TabSelection};
use ratatui::layout::Rect;
use ratatui::style::{Color, Style};
use ratatui::text::Span;
use ratatui::widgets::Block;
use ratatui::Frame;
use std::time::Instant;

impl App {
    pub(in crate::app) fn now_playing_throbber_span(&self) -> Span<'static> {
        const FRAMES: [&str; 9] = [" ", "▏", "▎", "▍", "▌", "▋", "▊", "▉", "█"];
        Span::styled(
            FRAMES[self.now_playing_throbber_index % FRAMES.len()],
            Style::default().fg(palette::ACCENT),
        )
    }

    /// Paint-free geometry seam entry (D2). Returns `None` on a zero-dimension
    /// frame before ANY mutation, so `self.layout` keeps reflecting the last
    /// frame that rendered in full. Otherwise updates the frame-dependent
    /// inputs (terminal size, mini-view focus, queue column width) and then
    /// computes the root/chrome geometry into the typed partial subresult
    /// `FrameChromeGeometry`. `compose_base_frame` publishes the migrated chrome fields
    /// into the fresh `AppLayout` and `render_main` consumes this subresult
    /// instead of recomputing root/chrome geometry inline.
    pub(in crate::app) fn compute_frame_layout(
        &mut self,
        area: Rect,
    ) -> Option<FrameChromeGeometry> {
        // Guard against zero-dimension terminal (e.g. minimized or piped)
        // before any state mutation or geometry computation.
        if area.width == 0 || area.height == 0 {
            return None;
        }
        if area.width != self.terminal_width || area.height != self.terminal_height {
            self.card_image_states.clear();
            self.card_image_loading.clear();
        }
        if self.terminal_width >= crate::app::MINI_VIEW_THRESHOLD
            && area.width < crate::app::MINI_VIEW_THRESHOLD
        {
            self.mini_view_focus = PanelFocus::Queue;
        }
        self.terminal_width = area.width;
        self.terminal_height = area.height;
        if self.clamp_queue_column_width() {
            self.save_prefs();
        }
        Some(self.compute_chrome_geometry(area))
    }

    /// Compute the root/chrome geometry for one frame, paint-free. Pure reads
    /// of `self` state; the single production caller is `compute_frame_layout`
    /// (the only seam entry), and `render_main` consumes the result rather
    /// than recomputing it. `pub(in crate::app)` so the render test
    /// helpers can render a view through `render_main` with the same
    /// authoritative geometry without pulling in `compute_frame_layout`'s
    /// terminal-normalization side effects.
    pub(in crate::app) fn compute_chrome_geometry(&self, area: Rect) -> FrameChromeGeometry {
        chrome_geometry(ChromeGeometryInput {
            area,
            panel_mode: self.effective_panel_mode(),
            panel_focus: self.effective_panel_focus(),
            queue_column_width: self.queue_column_width,
            terminal_width: self.terminal_width,
        })
    }

    /// Compose and paint the legacy base frame for one draw: the paint-free
    /// chrome checkpoint, the fresh draft `AppLayout`, the ordered `render_main`
    /// dispatch, and one atomic install of the completed layout. Called by the
    /// sole draw entry point `Model::draw_frame` and, in tests, directly against
    /// a bare `App`. Not named `render` — issue #607: there is no parallel
    /// legacy render path, only this base-frame composer beneath the mounted
    /// component views.
    pub fn compose_base_frame(&mut self, f: &mut Frame, cursor_scroll: Option<(usize, usize)>) {
        let area = f.area();
        let Some(chrome) = self.compute_frame_layout(area) else {
            // Zero-dimension terminal: `self.layout` is left untouched here --
            // it still reflects the last frame that rendered in full.
            return;
        };

        // Every render sub-call below writes into this fresh, local value
        // instead of `self.layout` directly. It's swapped into `self.layout`
        // in one atomic assignment only once this pass completes in full, so
        // an early return partway through can never leave `self.layout`
        // holding a mix of fields from two different frames.
        let mut layout = AppLayout::default();

        let active = self.player.status.lock().unwrap().active;
        let show_controls =
            active || self.connected_session_id.is_some() || self.cast_attachment.is_some();
        let playing_panel = show_controls;
        // Always reserve the player rows (title + controls) so
        // that content doesn't shift when the player appears or disappears.
        let player_h = PLAYER_BOX_HEIGHT;

        // Migrated root/chrome fields are published here from the subresult
        // (one authoritative computation). `render_main` bails on a frame too
        // short to draw (height < 4) without writing anything; publishing is
        // gated identically so a degenerate frame still installs the
        // all-default layout, matching the pre-split behavior.
        if area.height >= 4 {
            layout.main.panel_area = chrome.panel_area;
            layout.main.panel_content_area = chrome.panel_content_area;
            layout.playback.player_area = chrome.player_area;
            layout.playback.status_area = chrome.status_area;
            layout.tabs_area = chrome.tabs_area;
        }

        // Clear expired toast before any rendering so the status bar sees the latest state.
        if self.status_expires.is_some_and(|t| t <= Instant::now()) {
            self.status.clear();
            self.status_expires = None;
            self.status_severity = crate::app::notify_actions::ToastSeverity::default();
            self.force_clear = true;
        }

        let now_playing: Option<String> = if active {
            let idx = self.player.status.lock().unwrap().current_idx;
            let queue = self.playback_queue();
            queue.item_at(idx).map(|item| item.title().to_string())
        } else {
            None
        };
        let title_color = palette::PLAYBACK_VALUE_FG;
        let now_playing_title: Option<(String, Color)> = if playing_panel {
            if active {
                now_playing.map(|t| (t, title_color))
            } else if let Some(ref cast) = self.cast_attachment {
                self.cast_now_playing_title(cast).map(|t| (t, title_color))
            } else if let Some(ref state) = self.connected_session_state {
                state.now_playing.clone().map(|t| (t, title_color))
            } else {
                None
            }
        } else {
            None
        };
        // Render dispatch (issue #275; folded into a single unconditional
        // call by #361 commit 2, since the deleted Standard view was the
        // only other arm).
        self.render_main(
            f,
            area,
            &chrome,
            &mut layout.main,
            &mut layout.playback,
            player_h,
            show_controls,
            &now_playing_title,
            cursor_scroll,
        );

        // The Context menu is an owned TuiRealm component now (task 5.3c):
        // the shell mounts it from `pending_overlay` and paints it via the
        // overlay stack, so nothing is written to `layout` here.

        // One atomic replace, reached only once the full pass above has
        // completed -- `self.layout` never observes a half-updated frame.
        self.layout = layout;
    }
}

impl App {
    pub(in crate::app) fn render_main(
        &mut self,
        f: &mut Frame,
        area: Rect,
        chrome: &FrameChromeGeometry,
        layout: &mut LayoutMain,
        playback: &mut LayoutPlayback,
        player_h: u16,
        show_controls: bool,
        now_playing_title: &Option<(String, Color)>,
        cursor_scroll: Option<(usize, usize)>,
    ) {
        if area.height < 4 {
            return;
        }
        // Apply the tab saved from the previous session once libs have loaded.
        if self.library_tab_pending > 0
            && (!self.libs.is_empty() || !self.audiobookshelf_libraries.is_empty())
        {
            let fp = self.feeds_tab_pos();
            let emby = self.libs.len();
            let audio = self.audiobookshelf_libraries.len();
            let max_pos = fp.unwrap_or(emby + audio);
            let pos = self.library_tab_pending.min(max_pos);
            self.tab = TabSelection::from_position_with_counts(pos, emby, audio, fp.is_some());
            self.library_tab_pending = 0;
        }
        // A selected Service library index that no longer exists (async
        // Service removal/replacement) becomes Home. Home needs no
        // Service-specific library state, so we keep rendering the (now)
        // Home view instead of aborting to a blank, mostly-default frame:
        // the completed frame installs a full Home layout tagged Home below.
        self.normalize_stale_browse_destination();

        // Root/chrome geometry (left/right columns, tabs/player/status
        // placement, focus facts) comes from the paint-free subresult
        // computed by `compute_frame_layout`; it is consumed here, never
        // recomputed.
        let FrameChromeGeometry {
            panel_area: _,
            panel_content_area: _,
            left_area,
            right_area,
            // Consumed by `paint_legacy_chrome` (via the `chrome` ref), not the body.
            right_full_area: _,
            left_content,
            tab_bar_area: _,
            tabs_area: _,
            player_area: _,
            status_area,
            right_visible,
            queue_focused,
        } = *chrome;
        // Header row removed — the tab bar above indicates current location.
        layout.breadcrumbs = Vec::new();
        layout.selector_tabs = Vec::new();

        // Pre-body legacy chrome (column backgrounds, tab bar) underpaints the
        // card/queue/library body below. The right-column player panel is
        // painted solely by the mounted `PlaybackComponent` (row 3.9).
        self.paint_legacy_chrome(f, chrome, layout);

        let (lib_area, queue_geometry) = if self.effective_panel_mode() == PanelMode::LibraryOnly {
            (right_area, Default::default())
        } else {
            // The card fills the top of the left column; the queue list takes
            // the rows below it. Short terminals keep that same structure.
            let is_queue_only = self.effective_panel_mode() == PanelMode::QueueOnly;
            let is_wide = is_queue_only && left_area.width >= 100;
            // The card's cache/size/fetch operation is authoritative for its
            // dimensions. Publish its unchanged tuple result into the fresh
            // frame draft before deriving the downstream queue area.
            let (card_h, card_w, _) = self.render_card(f, left_content, is_wide);
            layout.card = CardGeometry {
                height: card_h,
                width: card_w,
            };

            // Queue-only mode has no right column, so the playback panel
            // (seekbar + title + controls) renders here instead: stacked
            // below the card on narrow terminals, or beside it on wide ones.
            let mut narrow_player_h = 0;
            if is_queue_only {
                if is_wide {
                    let panel_area = Rect {
                        x: left_content.x + layout.card.width + 2,
                        y: left_content.y,
                        width: left_content.width.saturating_sub(layout.card.width + 2),
                        height: layout.card.height,
                    };
                    f.render_widget(
                        Block::default().style(Style::default().bg(palette::SURFACE_CHROME)),
                        panel_area,
                    );
                    crate::app::render::render_player_panel(
                        f,
                        self.playback_panel_context(
                            panel_area,
                            playback,
                            player_h,
                            show_controls,
                            now_playing_title,
                            palette::SURFACE_CHROME,
                        ),
                    );
                } else {
                    let panel_area = Rect {
                        x: left_content.x,
                        y: left_content.y + layout.card.height,
                        width: left_content.width,
                        height: player_h,
                    };
                    crate::app::render::render_player_panel(
                        f,
                        self.playback_panel_context(
                            panel_area,
                            playback,
                            player_h,
                            show_controls,
                            now_playing_title,
                            palette::SURFACE_CHROME,
                        ),
                    );
                    narrow_player_h = player_h;
                }
            }

            let queue_geometry = queue_panel_geometry(QueuePanelInputs {
                left_content,
                card_height: layout.card.height,
                narrow_player_height: narrow_player_h,
            });
            (right_area, queue_geometry)
        };

        // Apply the shared horizontal padding once here, at the single point
        // where the tab content area is finalized, so every tab kind (and the
        // music-group pills row below) inherits consistent left/right gutters
        // instead of each renderer inventing its own. When the left column is
        // collapsed the user has asked to reclaim maximum width, so the gutters
        // are dropped and the library spans the panel edge-to-edge.
        let lib_area =
            right_panel_content_area(lib_area, self.effective_panel_mode() != PanelMode::Both);
        let render_lib_area = lib_area;
        // Both letter-range pills (large non-music libraries) and the
        // narrow music-group selector render inside `render_list` itself
        // now, below the hero (`list.rs`), unified with every other
        // inline browser's pill placement (design.md decision 6: pill
        // *position* is geometry, not a per-screen declaration) -- not
        // carved out of `lib_area` here. Wide grouped Music is the one
        // exception: its pills sit in the Wide hero right rail instead
        // (`render_wide_music_group`), which `list.rs` still branches to
        // internally before reaching the inline presentation path.

        if self.effective_panel_mode() != PanelMode::LibraryOnly {
            render_queue_panel_frame(f, queue_geometry.panel_area, queue_focused);
            layout.queue_title_area = queue_geometry.title_area;
            layout.queue_area = queue_geometry.content_area;
            layout.queue_selected_item_rect = None;
            if let Some(pill_row) = queue_geometry.pill_row {
                render_queue_status(
                    f,
                    pill_row,
                    self.playlist_status_spans(),
                    self.autosave_status_spans(),
                );
            }
        }
        if right_visible {
            self.render_library(f, render_lib_area, layout, cursor_scroll);
        }

        // Status bar at the bottom of the right panel. Playback prompts are
        // painted by the shell-mounted component after this legacy frame.
        if status_area.width > 0 {
            self.render_status_bar(f, status_area, playback, false);
        }
    }

    /// Paints the pre-body legacy chrome that underpaints the card/queue/library
    /// body: the left/right column backgrounds and the tab bar.
    ///
    /// The right-column player panel is not painted here: the mounted
    /// `PlaybackComponent` is its sole painter (row 3.9). The queue-only-mode
    /// player panels stay in `render_main` as the sole legacy renderer (D5),
    /// because `player_area` is empty in queue-only mode so the component
    /// cannot paint there.
    ///
    /// Called from within `render_main` at the root/chrome checkpoint -- after
    /// the `self.tab` normalization block and `normalize_stale_browse_destination`,
    /// before any body paint -- because `render_tabs` reads the normalized
    /// `self.tab`. Task 2.3's `Model::draw_frame` will hoist this call out of
    /// `render_main` and settle the `self.tab` normalization ordering so it can
    /// run after the body.
    pub(in crate::app) fn paint_legacy_chrome(
        &mut self,
        f: &mut Frame,
        chrome: &FrameChromeGeometry,
        layout: &mut LayoutMain,
    ) {
        let FrameChromeGeometry {
            left_area,
            right_full_area,
            tab_bar_area,
            tabs_area,
            right_visible,
            queue_focused,
            ..
        } = *chrome;

        crate::app::render::components::chrome::render_legacy_backdrops(
            f,
            left_area,
            right_full_area,
            queue_focused,
            self.effective_panel_mode() != PanelMode::LibraryOnly,
            right_visible,
        );

        // Tab bar at the very top of the right column.
        if right_visible {
            self.render_tabs(f, tab_bar_area, tabs_area, layout);
        }
    }
}
