//! Per-frame layout geometry produced by `App::compose_base_frame` and
//! consumed by mouse hit-testing in `input.rs`.
//!
//! `App` owns a single `AppLayout` value (`app.layout`) instead of ~35
//! scattered `layout_*`/`*`/`queue_*` fields. Grouping by view
//! mirrors the boundaries `render/` and `input.rs` already use, rather than
//! inventing a new one.
//!
//! Render code does not write into `self.layout` in place. Each call to
//! `App::compose_base_frame` builds a fresh, local `AppLayout::default()` and threads it
//! (or the relevant per-view sub-struct) through the render call graph as an
//! explicit parameter; every render function that used to write
//! `self.layout.<view>.<field> = ...` now writes `layout.<field> = ...` on
//! that local value instead. Only once the full pass completes does
//! `compose_base_frame` swap it into `self.layout` in a single atomic
//! assignment. This means
//! `self.layout` (read by `input.rs`) always reflects the last frame that
//! rendered in full, or is left completely untouched by an early return
//! (e.g. the zero-area guard) -- it can never hold a mix of fields from two
//! different frames.

use ratatui::layout::Rect;

/// Seekbar rect, the two divider status indicators that still have a click
/// target (remote-session and mute), the volume pill's scroll target, and
/// the mouse hit targets for the one-row playback header's transport
/// controls (play/pause glyph and next).
/// The button/track/volume/subtitle/audio rects this used to hold were
/// removed with the expanded playback view; see the "Tab bar restyle" commit
/// that zeroed them out.
#[derive(Default)]
pub(crate) struct LayoutPlayback {
    pub player_area: Rect,
    /// Status-bar area used by the shell-mounted playback prompt component.
    pub status_area: Rect,
    pub seekbar_area: Rect,
    pub ind_rc: Rect,
    pub ind_mu: Rect,
    /// Status-bar volume pill; scroll-wheel hit test.
    pub ind_vol: Rect,
    /// Playback header play/pause glyph; always clickable when the row renders.
    pub play_pause_area: Rect,
    /// Playback header stop glyph; only wired to the action when
    /// `App::transport_stop_available()` is true.
    pub stop_area: Rect,
    /// Playback header next glyph; only wired to the action when
    /// `App::transport_prev_next_available().1` is true.
    pub next_area: Rect,
    /// Idle-feed headline; only populated when its current item has a link.
    pub idle_feed_link_area: Rect,
}

/// Geometry produced by the queue card's authoritative render operation.
///
/// The card renderer returns the existing `(height, width, loading)` tuple;
/// this typed checkpoint records its dimensions in the fresh frame draft so
/// downstream queue placement consumes the published dimensions instead of
/// deriving them independently.
#[derive(Default)]
pub(crate) struct CardGeometry {
    pub height: u16,
    pub width: u16,
}

/// Library panel, queue panel, and home-grid geometry.
#[derive(Default)]
pub(crate) struct LayoutMain {
    /// Card geometry published immediately after the card's authoritative
    /// cache/size/fetch render path.
    pub card: CardGeometry,
    /// Full expanded sidebar covered by an F1-F4 panel, when present.
    pub panel_area: Rect,
    /// Content bounds inside `panel_area`, shared with panel mouse hit-testing.
    pub panel_content_area: Rect,
    pub left_row_map: Vec<Option<usize>>,
    /// Item rows of the last-rendered flat library list (plain and
    /// letter-grouped renderers), parallel to the display-row sequence:
    /// each entry holds the item indices occupying that display row, left to
    /// right (empty for headers/fillers). Column-aware cursor movement and
    /// mouse hit-testing resolve cells from this between frames.
    pub left_item_rows: Vec<Vec<usize>>,
    /// Screen-row offset for `left_item_rows` when the renderer packs display
    /// rows into screen rows (e.g. grouped album views with two-column
    /// layout). The mouse handler adds this (instead of `lvl.scroll`) to
    /// `click_y` to index into `left_item_rows`.
    pub left_screen_offset: usize,
    /// Grouped-album row targets for visible packed screen rows. The grouped
    /// display plan publishes these before any row or detail painter.
    pub left_row_targets: Vec<Option<usize>>,
    /// Source-item order published by the authoritative grouped display plan
    /// (and identity order for ungrouped lists).
    pub left_sorted_indices: Vec<usize>,
    pub left_area: Rect,
    /// The full area `App::render_home_list` was given (hero + pills + list,
    /// not just the inner list). The shell reads this to re-paint the
    /// mounted `HomeComponent`'s `view()` over the same area right after
    /// `App::compose_base_frame` returns (task 3.4).
    pub home_area: Rect,
    /// The full area passed to the Feeds renderer. The shell uses this to
    /// repaint the mounted `FeedsComponent` over the legacy frame.
    pub feeds_area: Rect,
    /// The selected item's hero geometry. Wide screens place it beside `left_area`;
    /// inline screens place the replacement inside the list and use it as the
    /// selected parent's activation geometry.
    pub hero_area: Rect,
    /// Selected-parent geometry only for inline replacement. Wide hero areas
    /// remain render bookkeeping and are intentionally not interactive.
    pub inline_hero_area: Rect,
    /// Queue placement and scope areas published independently of mounted
    /// component-local queue geometry.
    pub queue_area: Rect,
    pub queue_title_area: Option<Rect>,
    /// Screen rect of the selected row/cell in the library panel. The outer
    /// selectable renderer owns this; nested detail/hero renderers never
    /// overwrite it. Consumed by the context menu's keyboard anchor.
    pub selected_item_rect: Option<Rect>,
    /// Screen rect of the selected queue row. Owned by the queue renderer.
    pub queue_selected_item_rect: Option<Rect>,
    /// Pill/tab hitboxes published by the owning pill painters; placement and
    /// width remain owned by the shared pill-bar component.
    /// Pill/tab hitboxes published by the owning pill painters; placement and
    /// width remain owned by the shared pill-bar component. The music
    /// group-selector publishes these before paint.
    pub selector_tabs: Vec<(Rect, usize)>,
    pub breadcrumbs: Vec<(u16, u16, u16, usize)>,
    /// Per-track hit targets for the wide Music left pane. Each entry is
    /// `(screen_rect, track_index)` covering all wrapped physical rows of
    /// that logical track. Cleared every frame; populated only when the
    /// wide Music layout is active. This remains paint-coupled by design.
    pub wide_music_track_hitmap: Vec<(Rect, usize)>,
    /// Per-tab hit targets published by `render_tabs` (task 6.5). Each entry
    /// is `(screen_rect, tab_position)` for a visible tab, using the tab's
    /// real position (`all_names` index), not its visible-slot index. Does
    /// not include the `«`/`»` scroll-indicator glyphs. Cleared and
    /// repopulated every frame; paint-coupled by design, mirroring
    /// `wide_music_track_hitmap` above.
    pub tabs_hitmap: Vec<(Rect, usize)>,
    /// Bounding rect of the wide Music left pane's hero artwork area.
    /// Clicks here should not activate track selection or playback.
    pub wide_music_art_area: Rect,
    /// Full area passed to the grouped Music component after legacy layout.
    pub wide_music_area: Rect,
    /// Bounding rect of the wide Music right pane (album browser).
    /// Populated only when the wide Music layout is active.
    pub wide_music_right_area: Rect,
    /// Bounding rect of the wide Movies right rail (pills + list).
    /// Populated only when the wide Movies Wide hero layout is active.
    pub movies_wide_right_area: Rect,
    // TV-wide geometry (2.1i): `tv_wide_area`/`tv_wide_left_area`/
    // `tv_wide_right_area`/`tv_wide_list_area` are published at their
    // natural checkpoint before `render_list`, gated by the
    // `wide_hero_presentation` breakpoint, with loading preserved
    // component-side. Shared widget geometry (`selector_tabs`, left row
    // maps, hero/selected rects) was already published pre-paint by rows
    // 2.1a–2.1f. Component-local `tv_wide_episode_rows`/
    // `tv_wide_season_tabs` are paint-coupled by design (2.1b carve-out,
    // component-internal only).
    pub tv_wide_right_area: Rect,
    pub tv_wide_list_area: Rect,
    /// Paint area of the embedded episode `WideMediaList` (task 4.2d): the
    /// canonical control resolves its own row hits against this rect, so no
    /// per-row hit map is published here.
    pub tv_wide_episode_list_area: Rect,
    pub tv_wide_season_tabs: Vec<(Rect, usize)>,
    pub tv_wide_left_area: Rect,
    pub tv_wide_area: Rect,
    /// Bounding rect of the grouped-album browser itself (`Self::
    /// render_wide_right_album_browser`), the sub-rect of
    /// `wide_music_right_area` below the pill row. `left_row_targets` is
    /// indexed relative to this rect's top -- set by both the wide and
    /// narrow inline callers of the shared browser renderer, since
    /// they share row-target indexing but differ in outer gating rect.
    /// This is published at its natural checkpoint before paint.
    pub wide_music_browser_area: Rect,
    /// Full area passed to the Audiobookshelf podcast component after the
    /// legacy frame computes the current library layout.
    pub audiobookshelf_podcast_area: Rect,
    /// Full area passed to the Audiobookshelf book component after the legacy
    /// frame computes the current library layout.
    pub audiobookshelf_book_area: Rect,
}

impl LayoutMain {
    /// Returns the track index whose hit target contains `pos`, if any.
    pub(crate) fn wide_music_track_at(&self, pos: ratatui::layout::Position) -> Option<usize> {
        self.wide_music_track_hitmap
            .iter()
            .find(|(rect, _)| rect.contains(pos))
            .map(|(_, track_idx)| *track_idx)
    }

    /// Returns the tab position whose hit target contains `pos`, if any.
    pub(crate) fn tab_at(&self, pos: ratatui::layout::Position) -> Option<usize> {
        self.tabs_hitmap
            .iter()
            .find(|(rect, _)| rect.contains(pos))
            .map(|(_, tab_pos)| *tab_pos)
    }
}

/// Root/chrome frame geometry computed paint-free by
/// `App::compute_frame_layout` and consumed by `App::render_main` and the
/// chrome painters. This is the partial typed subresult of the staged
/// geometry/paint split (D2, task 2.1a): it owns the root/chrome fields only.
/// The full `AppLayout` remains the aggregate shared by every surface family;
/// non-migrated fields (queue/list/card/etc.) keep their legacy computation
/// until their own family rows migrate them.
#[derive(Default)]
pub(crate) struct FrameChromeGeometry {
    /// Full expanded sidebar covered by an F1-F4 panel, when present
    /// (`LayoutMain::panel_area`).
    pub panel_area: Rect,
    /// Content bounds inside `panel_area` (`LayoutMain::panel_content_area`).
    pub panel_content_area: Rect,
    /// Left panel (card + queue) column rect.
    pub left_area: Rect,
    /// Right panel (tabs, player, library, status) rect.
    pub right_area: Rect,
    /// Full-column right-panel background rect (tabs/player/library/status).
    pub right_full_area: Rect,
    /// Inner left-column content rect with the shared horizontal padding
    /// applied (queue and card paint areas are derived from this).
    pub left_content: Rect,
    /// Tab-bar box rect at the top of the right column.
    pub tab_bar_area: Rect,
    /// Tab-bar hit targets (`AppLayout::tabs_area`), published only when the
    /// right panel is visible; `Rect::default()` otherwise.
    pub tabs_area: Rect,
    /// Player-panel rect directly below the tab bar (right column only).
    pub player_area: Rect,
    /// Status-bar rect at the bottom of the right panel.
    pub status_area: Rect,
    /// Whether the right panel is visible this frame (`panel_mode != QueueOnly`).
    pub right_visible: bool,
    /// Whether the queue panel holds panel focus this frame.
    pub queue_focused: bool,
}

/// All per-frame layout geometry, grouped by the view that produces it.
/// `App` stores exactly one of these (`app.layout`); render writes into it,
/// input reads from it. See module docs for the rationale.
#[derive(Default)]
pub(crate) struct AppLayout {
    pub playback: LayoutPlayback,
    pub main: LayoutMain,
    pub tabs_area: Rect,
}
