//! Interactive Component for the generic Emby browser rows.
//!
//! The shell mirrors the active list source into this component. Generic,
//! Movies, and home-video rows use the existing typed render seam; music,
//! TV/series, and album-track presentation remain on their legacy branches
//! until their owning tasks convert them.

use ratatui::layout::{Position, Rect};
use ratatui::Frame;
use tuirealm::command::{Cmd, CmdResult};
use tuirealm::component::{AppComponent, Component};
use tuirealm::event::{Event, MouseEvent, MouseEventKind};
use tuirealm::props::{AttrValue, Attribute, QueryResult};
use tuirealm::state::State;

use mbv_core::api::EmbyItem;

use super::browser_narrow::NarrowBrowseExtras;
use super::component_id::BrowserKind;
use super::inline_search::{InlineSearch, InlineSearchHost};
use super::media_list::{
    InlineMediaBrowser, MediaKind, MediaListRow, MediaSemanticState, ViewportAnchor, WideMediaList,
};
use super::mouse::gesture::{MouseGesture, MouseGestureState};
use super::mouse::hit::HitRegions;
use super::msg::{Msg, ShellRequest};
use super::user_event::UserEvent;
use crate::app::layout::LayoutMain;
use crate::app::library_column_width::{library_cell_width, LIBRARY_COLUMN_GAP};
use crate::app::render::{
    effective_sort_str, letter_bucket, wide_hero_presentation, HomeImagePaint,
};
use crate::app::ui_util::natural_sort_key;

mod content;
mod keyboard;
mod navigation;
mod paint;

pub(in crate::app) use content::{BrowserContent, BrowserIdentity};

pub struct BrowserComponent {
    kind: BrowserKind,
    /// Position-free content the shell pushed (task 3.7). The legacy
    /// `LibraryListRenderCtx` is rebuilt on demand from this plus the
    /// control-owned `cursor`/`scroll` at a single private site.
    context: BrowserContent,
    /// The browse identity the last shell content push carried (task 3.7).
    /// `push_emby_browser_content` re-seeds position through `apply_position`
    /// only when this changes; within one identity (pagination, loading
    /// completion, refresh, cursor echo) no position crosses the boundary.
    last_identity: Option<BrowserIdentity>,
    cursor: usize,
    scroll: usize,
    focused: bool,
    layout: LayoutMain,
    /// Whether the component's own BrowserKey kind and painted geometry select
    /// the Wide hero layout. The value is derived in `view()` rather than
    /// projected from the App layout.
    wide_movies: bool,
    /// Whether the wide layout's pill row is a home-video count label (vs. a
    /// letter-range pill row). Fed by the shell from validated content.
    wide_movies_home_video: bool,
    /// Whether the wide layout shows the letter-range pill row. Fed by the
    /// shell each draw (task 5.3d.17a).
    wide_movies_letter_pills: bool,
    /// Runtime terminal-capability flag (config-derived), set by the shell so
    /// the component can paint the hero text like every other surface.
    use_nerd_fonts: bool,
    images_enabled: bool,
    /// The hero cover image `view()` computed but could not paint itself (no
    /// `App`/image-cache authority); the shell takes it right after
    /// `application.view()` and paints it via `App::paint_home_image`
    /// (mirrors `HomeComponent`, task 5.3d.17a).
    image_paint: Option<HomeImagePaint>,
    /// Shell-resolved narrow-browse extras (count label, letter pills, inline
    /// movie/series hero) for the `browser_narrow` composer, pushed each frame
    /// by `render_emby_browser_component` (task 3.3).
    narrow_extras: NarrowBrowseExtras,
    pending_anchor: Option<ViewportAnchor<String>>,
    preserved_anchor: Option<ViewportAnchor<String>>,
    /// Persistent canonical control for the applicable Wide hero Wide rails
    /// (Movies, home-video feed view). Fed from `set_content`, painted by
    /// `render_wide_movies`. Targets are item indices into `context.items`
    /// (Browser's existing typed row identity); task 3.7 removes the mirrored
    /// cursor/scroll, task 3.5c re-points navigation onto this control.
    wide_list: WideMediaList<usize>,
    /// Persistent canonical control for the applicable Narrow hero-bearing
    /// browse paths. Driven by `render_narrow_browse_with_ctx` instead of a
    /// per-frame `InlineMediaBrowser::new()`.
    inline_browser: InlineMediaBrowser<usize>,
    /// Private per-parent gesture recognition (ADR 0024, design.md D3): owns
    /// the double-click window and wheel throttle. Not a shared clock.
    mouse_gestures: MouseGestureState,
    /// Irregular painted chrome — the selector-pill row — as last-push-wins
    /// rectangles (design.md D6). Repopulated in `view()` from the pill rects
    /// the narrow/wide composer just painted into `self.layout.selector_tabs`.
    pill_regions: HitRegions<usize>,
    /// The embedded Inline Search control (design.md D1). `BrowserComponent`
    /// is its sole event boundary: it gets keyboard/mouse first refusal while
    /// active and is painted at the existing list composition point instead
    /// of the ordinary rows. Section 2 embeds this alongside the still-live
    /// to talk to this control instead and Section 4 deletes the overlay.
    inline_search: InlineSearch,
}

/// Derives the Emby-specific semantic state for a browse row. Both projection
/// sites (Inline and Wide) call this so the two trees cannot drift; the
/// provider-neutral `media_list` layer deliberately stays free of `EmbyItem`.
fn emby_semantic_state(item: &EmbyItem) -> MediaSemanticState {
    if item.playback_position_ticks > 0 && !item.played {
        let progress = if item.runtime_ticks > 0 {
            Some(
                ((item.playback_position_ticks as u64 * 100) / item.runtime_ticks as u64).min(100)
                    as u16,
            )
        } else {
            None
        };
        MediaSemanticState::active(progress)
    } else if item.played {
        MediaSemanticState::Played
    } else {
        MediaSemanticState::Ordinary
    }
}

impl BrowserComponent {
    pub fn new() -> Self {
        Self::new_for_kind(BrowserKind::Generic)
    }

    pub fn new_for_kind(kind: BrowserKind) -> Self {
        Self {
            kind,
            context: BrowserContent::default(),
            last_identity: None,
            cursor: 0,
            scroll: 0,
            focused: false,
            layout: LayoutMain::default(),
            wide_movies: false,
            wide_movies_home_video: false,
            wide_movies_letter_pills: false,
            use_nerd_fonts: false,
            images_enabled: true,
            image_paint: None,
            narrow_extras: NarrowBrowseExtras::default(),
            pending_anchor: None,
            preserved_anchor: None,
            wide_list: WideMediaList::new(),
            inline_browser: InlineMediaBrowser::new(),
            mouse_gestures: MouseGestureState::new(),
            pill_regions: HitRegions::new(),
            inline_search: InlineSearch::new(),
        }
    }

    /// Records the shell-resolved narrow-browse extras for the next `view()`
    /// (task 3.3). Pushed each frame by `render_emby_browser_component`.
    pub(in crate::app) fn set_narrow_extras(&mut self, extras: NarrowBrowseExtras) {
        self.narrow_extras = extras;
    }

    /// Records the position-free content push (task 3.7). Carries no cursor or
    /// scroll: an ordinary push never moves the control. Position is re-seeded
    /// only through the identity-gated `apply_position`. The one exception is
    /// the `ViewportAnchor` breakpoint seam, whose preserved target is
    /// re-resolved against the new item list here.
    /// Test-only: drive framework focus the way `Component::attr` does.
    #[cfg(test)]
    pub(in crate::app) fn set_focused(&mut self, focused: bool) {
        self.focused = focused;
    }

    pub(in crate::app) fn set_content(&mut self, content: BrowserContent) {
        self.context = content;
        let anchor_target = self
            .preserved_anchor
            .as_ref()
            .map(|anchor| anchor.selected_target.clone());
        if let Some(cursor) = anchor_target
            .and_then(|target| self.context.items.iter().position(|item| item.id == target))
        {
            self.cursor = cursor;
        }
        // Clamp the control-owned cursor to the new item count (a within-identity
        // refresh may return fewer items, e.g. inline search). This keeps the
        // invariant, not a position re-seed: `BrowserContent` has no cursor.
        self.cursor = self.cursor.min(self.context.item_count().saturating_sub(1));
        self.feed_inline_browser();
        self.feed_wide_list();
    }

    /// Explicit, identity-gated resting-position re-seed (task 3.7). The shell
    /// calls this from `push_emby_browser_content` ONLY when the browse
    /// identity changed (drill-in, go-back parent restore, letter-filter
    /// reset, sort change, feed/home-video group switch). Within one identity
    /// no position crosses the boundary, so pagination, loading completion,
    /// ordinary refresh, and the component's own `BrowserCursorIndex` echo
    /// leave the control-owned cursor and scroll untouched.
    pub(in crate::app) fn apply_position(&mut self, cursor: usize, scroll: usize) {
        self.cursor = cursor.min(self.context.item_count().saturating_sub(1));
        self.scroll = scroll;
        self.feed_inline_browser();
        self.inline_browser.select_target(&self.cursor);
        self.inline_browser.set_scroll(self.scroll);
        self.feed_wide_list();
    }

    /// Records the browse identity of the current shell content push and
    /// reports whether it differs from the previous push for this browser
    /// (task 3.7). A `true` result gates the `apply_position` re-seed.
    pub(in crate::app) fn note_browse_identity(&mut self, identity: BrowserIdentity) -> bool {
        let changed = self.last_identity.as_ref() != Some(&identity);
        self.last_identity = Some(identity);
        changed
    }

    /// Rebuild the persistent `InlineMediaBrowser` from position-free content.
    /// The control retains its selected target across ordinary content pushes;
    /// `apply_position` is the only path that seeds its target and scroll from
    /// the shell-owned resting position.
    fn feed_inline_browser(&mut self) {
        let ctx = &self.context;
        let mut sorted_indices: Vec<usize> = (0..ctx.items.len()).collect();
        sorted_indices
            .sort_by_cached_key(|&index| natural_sort_key(effective_sort_str(&ctx.items[index])));
        let grouped =
            !ctx.is_search_active() && (ctx.true_total() >= 50 || ctx.letter_filter.is_some());
        let mut rows = Vec::with_capacity(ctx.items.len());
        let mut last_group = None;
        for &index in &sorted_indices {
            let item = &ctx.items[index];
            if grouped {
                let bucket_total = if ctx.letter_filter.is_some() {
                    usize::MAX
                } else {
                    ctx.true_total()
                };
                let group = letter_bucket(item, bucket_total);
                if last_group.as_deref() != Some(group.as_str()) {
                    if last_group.is_some() {
                        rows.push(MediaListRow::Spacer);
                    }
                    rows.push(MediaListRow::Heading {
                        text: group.clone(),
                    });
                    last_group = Some(group);
                }
            }
            let primary = if item.is_folder && item.item_type == "Folder" && item.total_count > 0 {
                format!("{} · {} items", item.display_name(), item.total_count)
            } else if item.is_folder && item.unplayed_item_count > 0 && item.item_type != "Series" {
                format!("{} [{}]", item.display_name(), item.unplayed_item_count)
            } else {
                item.display_name()
            };
            let trailing = (!item.is_folder && item.production_year > 0)
                .then(|| item.production_year.to_string());
            let semantic_state = emby_semantic_state(item);
            rows.push(MediaListRow::Item {
                target: index,
                primary,
                trailing,
                duration: None,
                kind: MediaKind::Collection,
                semantic_state,
            });
        }
        self.inline_browser.set_content(rows);
    }

    /// Rebuild the persistent `WideMediaList` from the mirrored content for the
    /// applicable Wide rails (Movies, home-video, feed-group view), mirroring
    /// the routing `render_generic_movies_home_video_rows_with_ctx` applied:
    /// letter-grouped rows for a search-free library at or above 50 items (or
    /// with an active letter pill), plain rows otherwise. Non-applicable
    /// kinds (non-hero two-column Generic, Music, books) leave the control
    /// untouched; `view()` never paints it for them.
    fn feed_wide_list(&mut self) {
        if !(matches!(self.kind, BrowserKind::Movies | BrowserKind::HomeVideos)
            || self.context.has_group_pills())
        {
            return;
        }
        let ctx = &self.context;
        let row_for = |index: usize, item: &EmbyItem| -> MediaListRow<usize> {
            let primary = if item.is_folder && item.item_type == "Folder" && item.total_count > 0 {
                format!("{} \u{b7} {} items", item.display_name(), item.total_count)
            } else if item.is_folder && item.unplayed_item_count > 0 && item.item_type != "Series" {
                format!("{} [{}]", item.display_name(), item.unplayed_item_count)
            } else {
                item.display_name()
            };
            let semantic_state = emby_semantic_state(item);
            MediaListRow::Item {
                target: index,
                primary,
                trailing: (!item.is_folder && item.production_year > 0)
                    .then(|| item.production_year.to_string()),
                duration: None,
                kind: MediaKind::Collection,
                semantic_state,
            }
        };
        let grouped =
            !ctx.is_search_active() && (ctx.true_total() >= 50 || ctx.letter_filter.is_some());
        if grouped {
            let items = ctx
                .items
                .iter()
                .enumerate()
                .map(|(index, item)| (effective_sort_str(item).to_string(), row_for(index, item)))
                .collect();
            self.wide_list.set_letter_grouped_content(
                items,
                ctx.true_total(),
                ctx.letter_filter.is_some(),
            );
        } else {
            let rows = ctx
                .items
                .iter()
                .enumerate()
                .map(|(index, item)| row_for(index, item))
                .collect();
            self.wide_list.set_content(rows);
        }
        let cursor = self.cursor.min(ctx.item_count().saturating_sub(1));
        self.wide_list.select_target(&cursor);
        self.wide_list.set_scroll(self.scroll);
    }

    pub(in crate::app) fn cursor(&self) -> usize {
        self.cursor
    }

    /// The scroll offset the last `view()` painted the list at. The control
    /// owns it: `set_content` carries no position, so an ordinary content
    /// push never overwrites it. The shell reads this back only at navigation
    /// events (folder drill-in, `BrowserBack`) and teardown, through
    /// `persist_emby_browser_scroll` -> `persist_library_scroll`, to record
    /// the shell-owned resting position (design D3). It is not a per-frame
    /// mirror.
    pub(in crate::app) fn scroll(&self) -> usize {
        self.scroll
    }

    pub(in crate::app) fn viewport_anchor(
        &self,
        viewport_height: usize,
    ) -> Option<ViewportAnchor<String>> {
        self.active_viewport_anchor(viewport_height).or_else(|| {
            let item = self.context.items.get(self.cursor)?;
            Some(ViewportAnchor {
                selected_target: item.id.clone(),
                selected_row_offset: self
                    .cursor
                    .saturating_sub(self.scroll)
                    .min(viewport_height.saturating_sub(1)),
            })
        })
    }

    pub(in crate::app) fn apply_viewport_anchor(&mut self, anchor: ViewportAnchor<String>) {
        // Apply the explicit target immediately when content is already loaded;
        // the painted view still consumes the pending anchor to place the row.
        if let Some(cursor) = self
            .context
            .items
            .iter()
            .position(|item| item.id == anchor.selected_target)
        {
            self.cursor = cursor;
        }
        self.preserved_anchor = Some(anchor.clone());
        self.pending_anchor = Some(anchor);
    }

    pub(in crate::app) fn painted_viewport_height(&self) -> usize {
        self.layout.left_area.height as usize
    }

    /// Records the wide layout's pill-row presentation from validated shell
    /// content; whether the layout is wide is derived locally in `view()`.
    pub(in crate::app) fn configure_wide_movies(&mut self, home_video: bool, letter_pills: bool) {
        self.wide_movies_home_video = home_video;
        self.wide_movies_letter_pills = letter_pills;
    }

    /// Runtime terminal-capability flag (task 5.3d.17a): mirrors
    /// `HomeComponent::set_use_nerd_fonts` so the component can paint the
    /// wide hero text.
    pub(in crate::app) fn set_use_nerd_fonts(&mut self, use_nerd_fonts: bool) {
        self.use_nerd_fonts = use_nerd_fonts;
    }

    pub(in crate::app) fn set_images_enabled(&mut self, images_enabled: bool) {
        self.images_enabled = images_enabled;
    }

    /// Takes the hero cover image (if any) `view()` computed but could not
    /// paint itself. The shell calls this right after `application.view()`
    /// returns and paints it via `App::paint_home_image` (mirrors
    /// `HomeComponent::take_image_paint`, task 5.3d.17a).
    pub(in crate::app) fn take_image_paint(&mut self) -> Option<HomeImagePaint> {
        self.image_paint.take()
    }

    /// The panel and selected-row anchor from the active control's last paint.
    /// The shell uses this for keyboard-opened context menus instead of the
    /// legacy App layout mirror.
    pub(in crate::app) fn menu_placement_geometry(&self) -> Option<(Rect, Option<Rect>)> {
        (self.layout.left_area.width > 0 && self.layout.left_area.height > 0)
            .then_some((self.layout.left_area, self.layout.selected_item_rect))
    }

    /// Handle a mouse event against the component's painted browse geometry.
    ///
    /// Gesture recognition (click / double-click / right-click / wheel) comes
    /// from the private `MouseGestureState` (ADR 0024, design.md D3). Row
    /// identity comes only from the embedded control's `resolve_point`
    /// (design.md D6); the non-canonical generic grid keeps the parent's
    /// painted row map. The component emits a semantic `Msg` with a resolved
    /// target — never raw coordinates — except the context-menu anchor, which
    /// is display geometry it legitimately forwards (design.md D4).
    fn handle_mouse(&mut self, mouse: &MouseEvent) -> Option<Msg> {
        // Inline Search gets first refusal while active (design.md D6): it
        // is painted over the same area the ordinary list would occupy, so
        // the ordinary list never mutates for points there.
        if self.inline_search.is_active() {
            self.inline_search.handle_mouse(mouse);
            return None;
        }
        // Browse does not consume hover-move (design.md D7).
        if matches!(mouse.kind, MouseEventKind::Moved) {
            return None;
        }
        match self.mouse_gestures.recognize(mouse)? {
            MouseGesture::Scroll { at, delta } => {
                if !self.layout.left_area.contains(at) {
                    return None;
                }
                let rows = self.layout.left_item_rows.len();
                let viewport = self.layout.left_area.height as usize;
                let max_offset = rows.saturating_sub(viewport);
                self.scroll = self
                    .scroll
                    .saturating_add_signed(delta as isize)
                    .min(max_offset);
                Some(Msg::Shell(ShellRequest::BrowserScroll {
                    offset: self.scroll,
                }))
            }
            MouseGesture::Click(at) => {
                if let Some(&pill) = self.pill_regions.resolve(at) {
                    return Some(Msg::Shell(ShellRequest::BrowserPillClick { target: pill }));
                }
                if !self.claim_list_point(at) {
                    return None;
                }
                Some(Msg::Shell(ShellRequest::BrowserRowClick {
                    target: self.cursor,
                }))
            }
            MouseGesture::DoubleClick(at) => {
                if let Some(&pill) = self.pill_regions.resolve(at) {
                    return Some(Msg::Shell(ShellRequest::BrowserPillClick { target: pill }));
                }
                if !self.claim_list_point(at) {
                    return None;
                }
                Some(Msg::Shell(ShellRequest::BrowserRowActivate {
                    target: self.cursor,
                }))
            }
            MouseGesture::RightClick(at) => {
                if !self.claim_list_point(at) {
                    return None;
                }
                Some(Msg::Shell(ShellRequest::BrowserRowContextMenu {
                    target: self.cursor,
                    anchor: (mouse.column, mouse.row),
                }))
            }
        }
    }

    /// If `at` lands inside the painted list or inline-hero region, move the
    /// cursor to the row under it (a blank/gap click leaves the cursor
    /// unchanged, matching the legacy behaviour) and return `true`.
    fn claim_list_point(&mut self, at: Position) -> bool {
        if !(self.layout.left_area.contains(at) || self.layout.inline_hero_area.contains(at)) {
            return false;
        }
        if let Some(target) = self.resolve_row_target(at) {
            self.cursor = target;
        }
        true
    }

    /// The item index under `point`, resolved by the embedded canonical
    /// control that painted the active list (design.md D6). The inline hero
    /// covers the selected item, so a hero click carries the current cursor.
    /// The non-hero generic grid has no canonical control, so it falls back to
    /// the parent's painted row map.
    fn resolve_row_target(&self, point: Position) -> Option<usize> {
        if self.wide_movies {
            return self
                .wide_list
                .resolve_point(self.layout.left_area, point)
                .copied();
        }
        if self.layout.inline_hero_area.contains(point) {
            return Some(self.cursor);
        }
        if self.layout.left_item_rows.is_empty() {
            let detail_rows = self.layout.inline_hero_area.height as usize;
            return self
                .inline_browser
                .resolve_point(self.layout.left_area, detail_rows, point)
                .copied();
        }
        self.resolve_left_cursor(point.x, point.y)
    }

    /// Resolve the list item under `(col, row)` from the component's own
    /// painted `LayoutMain` for the non-canonical generic multi-column grid:
    /// the exact cell is picked when the list is two-column, and header/gap
    /// screen rows are `None` (no-op).
    fn resolve_left_cursor(&self, col: u16, row: u16) -> Option<usize> {
        let la = self.layout.left_area;
        if !la.contains((col, row).into()) {
            return None;
        }
        let click_y = (row.saturating_sub(la.y)) as usize;
        let display_row = self.scroll + click_y;
        // Cell-aware two-column resolution: pick the exact column under the
        // click. Single-column and header rows fall back to the row map below.
        if let Some(items) = self.layout.left_item_rows.get(display_row) {
            if items.len() > 1 {
                let cols = self
                    .layout
                    .left_item_rows
                    .iter()
                    .map(Vec::len)
                    .max()
                    .unwrap_or(1);
                let cell_w = library_cell_width(la, cols) as usize;
                let x = (col.saturating_sub(la.x)) as usize;
                let stride = cell_w + LIBRARY_COLUMN_GAP as usize;
                let cell = x / stride;
                if cell < items.len() && x % stride < cell_w {
                    return items.get(cell).copied();
                }
                return None;
            }
        }
        self.layout.left_row_map.get(click_y).copied().flatten()
    }

    #[cfg(test)]
    pub(crate) fn test_layout(&self) -> &LayoutMain {
        &self.layout
    }

    /// Test-only cursor seed (task 5.3d.16): `set_content` no longer mirrors
    /// the shell cursor, so tests position the authoritative local cursor
    /// directly before exercising navigation.
    #[cfg(test)]
    pub(crate) fn set_cursor_for_test(&mut self, cursor: usize) {
        self.cursor = cursor;
    }

    /// Test-only reset of the private gesture recognizer, so a synchronous
    /// test loop can drive successive wheel/click events without the
    /// throttle/double-click window collapsing them.
    #[cfg(test)]
    pub(crate) fn reset_mouse_gestures_for_test(&mut self) {
        self.mouse_gestures.reset_for_test();
    }
}

impl Default for BrowserComponent {
    fn default() -> Self {
        Self::new()
    }
}

impl InlineSearchHost for BrowserComponent {
    fn inline_search(&self) -> &InlineSearch {
        &self.inline_search
    }

    fn inline_search_mut(&mut self) -> &mut InlineSearch {
        &mut self.inline_search
    }
}

impl Component for BrowserComponent {
    fn view(&mut self, frame: &mut Frame, area: Rect) {
        // Compute the next presentation before consuming a pending anchor so a
        // control transition can carry the outgoing control's live offset into
        // the incoming control. The anchor is deliberately one-shot; the
        // receiving control consumes it on this or the next frame.
        let wide = (matches!(self.kind, BrowserKind::Movies | BrowserKind::HomeVideos)
            || self.narrow_extras.feed_items.is_some())
            && wide_hero_presentation(area).is_some();
        let switching_controls = wide != self.wide_movies;
        if switching_controls {
            if let Some(anchor) = self.active_viewport_anchor(self.painted_viewport_height()) {
                self.preserved_anchor = Some(anchor.clone());
                self.pending_anchor = Some(anchor);
            }
            self.wide_movies = wide;
        }
        if let Some(anchor) = self.pending_anchor.take() {
            if !self.apply_active_viewport_anchor(&anchor, area.height as usize) {
                if let Some(cursor) = self
                    .context
                    .items
                    .iter()
                    .position(|item| item.id == anchor.selected_target)
                {
                    self.cursor = cursor;
                    self.scroll = cursor.saturating_sub(anchor.selected_row_offset).min(
                        self.context
                            .items
                            .len()
                            .saturating_sub(area.height as usize),
                    );
                }
                // A newly-mounted receiving control may not have rows until
                // this render populates it. Retry the handoff once next frame.
                if switching_controls {
                    self.pending_anchor = Some(anchor);
                }
            }
        }
        self.layout = LayoutMain::default();
        let mut context = self
            .context
            .clone()
            .with_cursor_scroll(self.cursor, self.scroll);
        if let Some(items) = self.narrow_extras.feed_items.as_ref() {
            let feed = BrowserContent {
                items: items.clone(),
                total_count: items.len(),
                group_pills: true,
                loading: context.loading,
                ..BrowserContent::default()
            };
            context = feed
                .with_cursor_scroll(self.cursor.min(items.len().saturating_sub(1)), self.scroll);
        }
        // Task 5.3d.17a: when the wide Movies/home-video Wide hero layout
        // is active (this component's own `kind` AND the area is wide enough
        // for the shared split), paint the full hero + pills + list layout
        // itself instead of just the inner list rows; otherwise keep the
        // narrow list-row behavior.
        let rendered_scroll = if wide {
            self.render_wide_movies(frame, area, &context)
        } else if self.inline_search.is_active() {
            // Normal/non-Hero catalogs pass their whole list area to the
            // shared search painter (design.md D3); the ordinary narrow
            // composer does not also paint it.
            let items = self.inline_search.ordered_items();
            let query = self.inline_search.query().to_string();
            let loading = self.inline_search.loading();
            let cursor = self.inline_search.cursor();
            let scroll_in = self.inline_search.scroll();
            let areas = crate::app::render::arrangements::wide_hero::pill_bar_areas(area);
            let (pills_area, list_area) = (areas.pills_area, areas.content_area);
            let columns = crate::app::library_column_width::library_column_count(list_area.width);
            let new_scroll = crate::app::render::render_inline_search(
                frame,
                pills_area,
                list_area,
                &query,
                loading,
                items,
                cursor,
                scroll_in,
                self.focused,
                columns,
                self.inline_search.layout_mut(),
            );
            self.inline_search.set_scroll(new_scroll);
            self.image_paint = None;
            self.scroll
        } else {
            // Narrow generic/Movies/home-video: the component owns the full
            // surface via the `browser_narrow` composer (task 3.3). It returns
            // the landed scroll and the poster image still needing paint (the
            // shell executes it via `App::paint_home_image`, mirroring the
            // wide path and `HomeComponent`).
            let (scroll, image_paint) = crate::app::render::render_narrow_browse_with_ctx(
                frame,
                area,
                &context,
                &self.narrow_extras,
                self.focused,
                &mut self.layout,
                &mut self.inline_browser,
            );
            self.image_paint = image_paint;
            // Keep the active control's resting viewport in lockstep with the
            // painter's resolved flow; the parent field remains only the
            // shell's navigation/teardown persistence seam.
            if self.uses_inline_control() {
                self.inline_browser.set_scroll(scroll);
            }
            scroll
        };
        if !self.inline_search.is_active() {
            self.scroll = rendered_scroll;
        }

        // Adopt the selector-pill rects the composer just painted into the
        // irregular-chrome registry (design.md D6). `selector_tabs` is the
        // composer's own painted output for this frame.
        self.pill_regions.clear();
        for (rect, target) in &self.layout.selector_tabs {
            self.pill_regions.push(*rect, *target);
        }
    }

    fn query<'a>(&'a self, _attr: Attribute) -> Option<QueryResult<'a>> {
        None
    }

    fn attr(&mut self, attr: Attribute, value: AttrValue) {
        if attr == Attribute::Focus {
            self.focused = matches!(value, AttrValue::Flag(true));
        }
    }

    fn state(&self) -> State {
        State::None
    }

    fn perform(&mut self, _cmd: Cmd) -> CmdResult {
        CmdResult::NoChange
    }
}

impl AppComponent<Msg, UserEvent> for BrowserComponent {
    fn on(&mut self, event: &Event<UserEvent>) -> Option<Msg> {
        match event {
            Event::Keyboard(key) => self.handle_tui_key(*key),
            Event::Mouse(mouse) => self.handle_mouse(mouse),
            _ => None,
        }
    }
}
