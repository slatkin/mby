//! Interactive Component for grouped Music's wide workspace.
//!
//! The shell mirrors album data and cached tracks. Album/track cursor state is
//! local here; cross-authority effects use typed shell requests.

use ratatui::layout::{Position, Rect};
use ratatui::Frame;
use tuirealm::command::{Cmd, CmdResult};
use tuirealm::component::{AppComponent, Component};
use tuirealm::event::{Event, MouseEvent, MouseEventKind};
use tuirealm::props::{AttrValue, Attribute, QueryResult};
use tuirealm::state::State;

use super::inline_search::{InlineSearch, InlineSearchHost, InlineSearchMouse};
use super::media_list::{
    InlineMediaBrowser, MediaKind, MediaListRow, MediaSemanticState, ViewportAnchor, WideMediaList,
};
use super::mouse::gesture::{MouseGesture, MouseGestureState};
use super::mouse::hit::HitRegions;
use super::msg::{AlbumCursorKind, Msg, ShellRequest};
use super::user_event::UserEvent;
use crate::app::layout::LayoutMain;
use crate::app::render::{
    render_narrow_music_group_with_ctx, render_wide_music_group_with_ctx, wide_hero_presentation,
    MusicImagePaint, MusicWideRenderCtx,
};
use crate::app::ui_util::list_duration_secs;
use mbv_core::api::{EmbyItem, TICKS_PER_SECOND};

fn build_track_rows(tracks: &[EmbyItem]) -> Vec<MediaListRow<String>> {
    tracks
        .iter()
        .enumerate()
        .map(|(index, track)| {
            let number = if track.index_number > 0 {
                track.index_number
            } else {
                index as i64 + 1
            };
            let duration = list_duration_secs(track.runtime_ticks / TICKS_PER_SECOND);
            MediaListRow::Item {
                target: track.id.clone(),
                primary: format!("{number}. {}", track.name),
                trailing: None,
                duration,
                kind: MediaKind::Media,
                semantic_state: MediaSemanticState::Ordinary,
            }
        })
        .collect()
}

pub struct MusicWorkspaceComponent {
    pub(super) context: MusicWideRenderCtx,
    pub(super) album_cursor: usize,
    pub(super) album_columns: usize,
    pub(super) page_rows: usize,
    album_scroll: usize,
    pub(super) track_cursor: Option<usize>,
    /// Selected-album identity from the last pushed context. When it changes
    /// (group switch, recursive-album activation, position restore), inline
    /// track focus must reset: a focused track index refers to the previous
    /// album's track list.
    last_album_id: Option<String>,
    layout: LayoutMain,
    image_paint: Option<MusicImagePaint>,
    inline_track_focus_enabled: bool,
    /// Persistent narrow-presentation control: fed the canonical grouped-album
    /// row projection by `set_content`, painted by
    /// `render_narrow_music_group_with_ctx`. Never constructed during a render
    /// pass. The wide rail composes its own per-frame `WideMediaList`.
    narrow_list: InlineMediaBrowser<String>,
    /// One-shot `ViewportAnchor` carried across a breakpoint flip (§2.5).
    pending_anchor: Option<ViewportAnchor<String>>,
    /// The presentation the last `view` painted; `None` before the first paint.
    last_wide: Option<bool>,
    /// Painted viewport heights and the wide selected-row screen offset,
    /// captured each `view` so `viewport_anchor` can report the outgoing
    /// control's anchor at a flip.
    narrow_viewport_height: usize,
    wide_viewport_height: usize,
    wide_selected_row_offset: Option<usize>,
    /// Private per-parent gesture recognition (ADR 0024, design.md D3): owns
    /// the double-click window and wheel throttle. Not a shared clock.
    mouse_gestures: MouseGestureState,
    /// The wide right-rail's canonical album control, seeded each `view()` by
    /// `render_wide_right_album_browser_with_ctx` from the pushed context.
    /// Its `resolve_point` gives the wide-rail row identity for the mouse
    /// path (design.md D6). The narrow list / track table are net-new mouse
    /// work in task 6.1.
    wide_list: WideMediaList<String>,
    pub(super) track_list: WideMediaList<String>,
    /// Narrow presentation only: the screen rect the album rows were painted
    /// into this frame, captured from `render_narrow_music_group_with_ctx`.
    /// The narrow mouse path resolves row hits against it through
    /// `narrow_list.resolve_point` (design.md D6, task 6.1).
    narrow_list_area: Rect,
    /// Group-pill rects (design.md D6), repopulated in `view()` from
    /// `layout.selector_tabs` — the pill painter's own output — for both
    /// breakpoints. The tag is the 0-based group index.
    pill_regions: HitRegions<usize>,
    /// The embedded Inline Search control (design.md D1). See
    /// `BrowserComponent::inline_search` for the migration-phase notes.
    /// `pub(super)`, matching `track_cursor`/`album_cursor`, so the sibling
    /// `music_workspace_keys` module (split out for file size) can reach it.
    pub(super) inline_search: InlineSearch,
}

impl MusicWorkspaceComponent {
    pub fn new() -> Self {
        Self {
            context: MusicWideRenderCtx::new(
                crate::app::render::LibraryListRenderCtx::from_items(Vec::new(), 0, 0),
                None,
                String::new(),
                Vec::new(),
                0,
                Vec::new(),
                Vec::new(),
                false,
                None,
                false,
                None,
            ),
            album_cursor: 0,
            album_columns: 1,
            page_rows: 1,
            album_scroll: 0,
            track_cursor: None,
            last_album_id: None,
            layout: LayoutMain::default(),
            image_paint: None,
            inline_track_focus_enabled: false,
            narrow_list: InlineMediaBrowser::new(),
            pending_anchor: None,
            last_wide: None,
            narrow_viewport_height: 1,
            wide_viewport_height: 1,
            wide_selected_row_offset: None,
            mouse_gestures: MouseGestureState::new(),
            wide_list: WideMediaList::new(),
            track_list: WideMediaList::new(),
            narrow_list_area: Rect::default(),
            pill_regions: HitRegions::new(),
            inline_search: InlineSearch::new(),
        }
    }

    /// The stable id of the album under the component's authoritative
    /// `album_cursor`, resolved through the cached render context.
    fn selected_album_id(&self) -> Option<String> {
        self.context
            .list
            .items
            .get(self.album_cursor)
            .map(|album| album.id.clone())
    }

    /// Keep the narrow control's selection in lockstep with the authoritative
    /// `album_cursor` (the component owns the cursor; the control owns only
    /// viewport + row geometry).
    fn sync_narrow_selection(&mut self) {
        if let Some(id) = self.selected_album_id() {
            self.narrow_list.select_target(&id);
        }
    }

    /// Place the wide rail's resting scroll so the anchor's target lands at the
    /// requested screen-row offset, clamping where the grouped row flow does
    /// not allow it. The wide painter re-clamps to keep the selection visible.
    fn apply_wide_anchor(&mut self, anchor: &ViewportAnchor<String>, viewport_height: usize) {
        let rows = self.context.grouped_rows();
        let Some(display_row) = rows
            .iter()
            .position(|row| row.selectable_target() == Some(&anchor.selected_target))
        else {
            return;
        };
        let max_offset = rows.len().saturating_sub(viewport_height.max(1));
        self.album_scroll = display_row
            .saturating_sub(anchor.selected_row_offset)
            .min(max_offset);
    }

    /// The outgoing control's `ViewportAnchor` for the last painted
    /// presentation (mirrors `TvWorkspaceComponent::viewport_anchor`).
    pub(in crate::app) fn viewport_anchor(
        &self,
        viewport_height: usize,
    ) -> Option<ViewportAnchor<String>> {
        match self.last_wide {
            Some(true) => Some(ViewportAnchor {
                selected_target: self.selected_album_id()?,
                selected_row_offset: self.wide_selected_row_offset?,
            }),
            _ => self.narrow_list.viewport_anchor(viewport_height),
        }
    }

    pub(in crate::app) fn painted_viewport_height(&self) -> usize {
        match self.last_wide {
            Some(true) => self.wide_viewport_height,
            _ => self.narrow_viewport_height,
        }
    }

    /// Deliver a `ViewportAnchor` to the kept-mounted workspace; consumed at
    /// the next `view` against the then-current presentation.
    pub(in crate::app) fn apply_viewport_anchor(&mut self, anchor: ViewportAnchor<String>) {
        if let Some(idx) = self
            .context
            .list
            .items
            .iter()
            .position(|item| item.id == anchor.selected_target)
        {
            self.album_cursor = idx;
        }
        self.sync_narrow_selection();
        self.pending_anchor = Some(anchor);
    }

    /// Test-only: drive framework focus the way `Component::attr` does when
    /// TuiRealm delivers `Attribute::Focus`.
    #[cfg(test)]
    pub(in crate::app) fn set_focused(&mut self, focused: bool) {
        self.context.focused = focused;
    }

    pub(in crate::app) fn set_inline_track_focus_enabled(&mut self, enabled: bool) {
        self.inline_track_focus_enabled = enabled;
        if !enabled {
            self.track_cursor = None;
        }
    }

    pub(in crate::app) fn set_content(&mut self, context: MusicWideRenderCtx) {
        let album_changed = self.last_album_id.as_deref()
            != context
                .selected_album
                .as_ref()
                .map(|album| album.id.as_str());
        // Inline track focus is owned here; a selected-album identity change
        // (group switch, recursive-album activation, position restore) is the
        // one content-driven reset -- a focused track index refers to the
        // previous album's track list. That is an event on content identity,
        // not an echo test (D4).
        if album_changed {
            self.track_cursor = None;
        }
        self.last_album_id = context
            .selected_album
            .as_ref()
            .map(|album| album.id.clone());
        // Content projection never carries framework focus; preserve the
        // component-owned value across the shell snapshot swap.
        let focused = self.context.focused;
        self.context = context;
        self.context.focused = focused;
        self.track_list.set_content(build_track_rows(
            self.context.album_tracks.as_deref().unwrap_or_default(),
        ));
        if let Some(cursor) = self.track_cursor {
            self.track_list.select_index(cursor);
        }
        // The component owns `album_cursor`/`album_scroll` outright; an
        // ordinary content push never adopts the shell's cursor. A shrunk
        // projection can still orphan the local cursor, so clamp it against
        // the new content.
        self.album_cursor = self
            .album_cursor
            .min(self.context.list.item_count().saturating_sub(1));
        if let Some(cursor) = self.track_cursor {
            let count = self.context.album_tracks.as_ref().map_or(0, Vec::len);
            if count > 0 {
                self.track_cursor = Some(cursor.min(count - 1));
            }
        }
        // The narrow control retains its selected target across an ordinary
        // content push; re-sync it to the authoritative cursor afterwards
        // (D3: no shell cursor mirror, local clamp only).
        self.narrow_list.set_content(self.context.grouped_rows());
        self.sync_narrow_selection();
    }

    /// Shell-driven re-anchor of the album cursor/scroll at a navigation
    /// event: group switch, recursive-album activation, saved-position
    /// restore, or the first projection after mount. Unlike a content push
    /// this adopts the shell's value unconditionally -- the outcome does not
    /// depend on whether the user moved the cursor since the last push.
    pub(in crate::app) fn re_anchor(&mut self, cursor: usize, scroll: usize) {
        self.album_cursor = cursor.min(self.context.list.item_count().saturating_sub(1));
        self.album_scroll = scroll;
        // An explicit shell re-anchor supersedes any pending breakpoint anchor.
        self.pending_anchor = None;
        self.sync_narrow_selection();
        self.narrow_list.set_scroll(scroll);
    }

    pub(in crate::app) fn set_album_columns(&mut self, columns: usize) {
        self.album_columns = columns.max(1);
    }

    pub(in crate::app) fn set_page_rows(&mut self, rows: usize) {
        self.page_rows = rows.max(1);
    }

    pub(in crate::app) fn album_cursor(&self) -> usize {
        self.album_cursor
    }

    pub(in crate::app) fn album_scroll(&self) -> usize {
        self.album_scroll
    }

    pub(in crate::app) fn painted_album_cursor_and_order(&self) -> (usize, &[usize]) {
        (self.album_cursor, &self.context.album_order)
    }

    /// The album item under the component's own album cursor, cloned out of
    /// the cached render context. Mirrors `TvWorkspaceComponent::selected_item()`
    /// for outcome 3 readers (R16/R18): the shell supplies this instead of
    /// reading `BrowseLevel.cursor` for selected-album construction.
    /// First-mount fallback to App-derived item when the component is
    /// freshly mounted (before the first content push from the shell).
    pub(in crate::app) fn selected_item(&self) -> Option<mbv_core::api::EmbyItem> {
        self.context
            .list
            .clone()
            .with_cursor_scroll(self.album_cursor, self.album_scroll)
            .selected_item()
            .cloned()
    }

    pub(in crate::app) fn track_cursor(&self) -> Option<usize> {
        self.track_cursor
    }

    #[cfg(test)]
    pub(in crate::app) fn track_selected_row(&self) -> Option<usize> {
        self.track_list.selected_display_row()
    }

    #[cfg(test)]
    pub(in crate::app) fn album_tracks_loading(&self) -> bool {
        self.context.album_tracks_loading
    }

    /// Whether inline track focus can be entered right now: wide mode
    /// (`inline_track_focus_enabled`) with the selected album's tracks
    /// cached. Narrow mode keeps `track_cursor` `None` by construction.
    pub(super) fn can_enter_track_focus(&self) -> bool {
        self.inline_track_focus_enabled
            && self.context.focused
            && self
                .context
                .album_tracks
                .as_ref()
                .is_some_and(|tracks| !tracks.is_empty())
    }

    /// Shell-driven entry into inline track focus (recursive album
    /// activation): enters only when the feature is enabled and the selected
    /// album's tracks are cached; a no-op in narrow mode.
    pub(in crate::app) fn enter_track_focus(&mut self) {
        if self.can_enter_track_focus() {
            self.track_cursor = Some(0);
            self.track_list.select_first();
        }
    }

    /// Shell-driven clear of inline track focus (position restore): the
    /// deleted track-focus-clear rehome.
    pub(in crate::app) fn clear_track_focus(&mut self) {
        self.track_cursor = None;
        self.track_list.select_first();
    }

    /// Handle a mouse event against the wide workspace's painted geometry.
    ///
    /// Recognition comes from the private `MouseGestureState` (ADR 0024,
    /// design.md D3). The wide right-rail album row identity comes from the
    /// embedded `WideMediaList::resolve_point` (design.md D6). The wide-left
    /// track table keeps its component-local cursor claim unchanged — its
    /// full mouse surface is net-new work in task 6.1.
    fn handle_mouse(&mut self, mouse: &MouseEvent) -> Option<Msg> {
        // Inline Search gets first refusal while active (design.md D6): it
        // is painted over the same area the ordinary album rail/rows would
        // occupy, so they never mutate for points there.
        if self.inline_search.is_active() {
            return match self.inline_search.handle_mouse(mouse) {
                Some(InlineSearchMouse::ContextMenu) => self
                    .inline_search
                    .selected_item()
                    .map(|item| Msg::Shell(ShellRequest::EmbyLibraryContextMenu { item })),
                None => None,
            };
        }
        // Music does not consume hover-move (design.md D7).
        if matches!(mouse.kind, MouseEventKind::Moved) {
            return None;
        }
        let wide = self.last_wide.unwrap_or(false);
        match self.mouse_gestures.recognize(mouse)? {
            // Wide right-rail / track table: unchanged from task 3.6.
            MouseGesture::Click(at) | MouseGesture::DoubleClick(at) if wide => {
                if let Some(track) = self.layout.wide_music_track_at(at) {
                    self.track_cursor = Some(track);
                    self.track_list.select_index(track);
                    return None;
                }
                let album = self.resolve_wide_album(at)?;
                self.album_cursor = album;
                Some(Msg::Shell(ShellRequest::MusicAlbumCursor {
                    target: album,
                    kind: AlbumCursorKind::Move,
                }))
            }
            // Narrow: group pills, then album rows (task 6.1).
            MouseGesture::Click(at) => {
                if let Some(msg) = self.claim_group_pill(at) {
                    return Some(msg);
                }
                let album = self.claim_narrow_album(at)?;
                Some(Msg::Shell(ShellRequest::MusicAlbumCursor {
                    target: album,
                    kind: AlbumCursorKind::Move,
                }))
            }
            MouseGesture::DoubleClick(at) => {
                if let Some(msg) = self.claim_group_pill(at) {
                    return Some(msg);
                }
                self.claim_narrow_album(at)?;
                Some(Msg::Shell(ShellRequest::MusicAlbumActivate))
            }
            MouseGesture::RightClick(at) if !wide => {
                self.claim_narrow_album(at)?;
                Some(Msg::Shell(ShellRequest::MusicAlbumContextMenu {
                    anchor: (mouse.column, mouse.row),
                }))
            }
            _ => None,
        }
    }

    /// Move the album selection to the narrow row under `at`, resolved by the
    /// embedded `InlineMediaBrowser` against the rect it painted (design.md
    /// D6). Returns the pushed-context item index, or `None` for a
    /// heading/spacer row or a point outside the list.
    fn claim_narrow_album(&mut self, at: Position) -> Option<usize> {
        let detail_rows = self.layout.inline_hero_area.height as usize;
        let id = self
            .narrow_list
            .resolve_point(self.narrow_list_area, detail_rows, at)?;
        let album = self
            .context
            .list
            .items
            .iter()
            .position(|item| &item.id == id)?;
        self.album_cursor = album;
        self.sync_narrow_selection();
        Some(album)
    }

    /// If `at` lands on a group pill, emit the relative `MusicGroupSwitch` that
    /// reaches the clicked group (the only group-selection keyboard action is
    /// the relative `[`/`]`). A click on the current pill is a no-op.
    fn claim_group_pill(&self, at: Position) -> Option<Msg> {
        let &target = self.pill_regions.resolve(at)?;
        let delta = target as i64 - self.context.group_cursor as i64;
        (delta != 0).then_some(Msg::Shell(ShellRequest::MusicGroupSwitch { delta }))
    }

    /// The album index under `at` on the wide right rail, resolved by the
    /// embedded canonical control against the rail area it painted, then
    /// mapped to the pushed context's item index (design.md D6). `None` for a
    /// heading/spacer row or a point outside the rail.
    fn resolve_wide_album(&self, at: Position) -> Option<usize> {
        let id = self
            .wide_list
            .resolve_point(self.layout.wide_music_browser_area, at)?;
        self.context
            .list
            .items
            .iter()
            .position(|item| &item.id == id)
    }

    pub(in crate::app) fn take_image_paint(&mut self) -> Option<MusicImagePaint> {
        self.image_paint.take()
    }

    /// Geometry painted during the last view pass. The shell mirrors the
    /// interaction targets into App layout for legacy readers that still
    /// consume frame geometry.
    pub(in crate::app) fn layout(&self) -> &LayoutMain {
        &self.layout
    }

    #[cfg(test)]
    pub(in crate::app) fn test_narrow_list_area(&self) -> Rect {
        self.narrow_list_area
    }

    #[cfg(test)]
    pub(in crate::app) fn test_pill_regions(&self) -> &[(Rect, usize)] {
        self.pill_regions.regions()
    }
}

impl Default for MusicWorkspaceComponent {
    fn default() -> Self {
        Self::new()
    }
}

impl InlineSearchHost for MusicWorkspaceComponent {
    fn inline_search(&self) -> &InlineSearch {
        &self.inline_search
    }

    fn inline_search_mut(&mut self) -> &mut InlineSearch {
        &mut self.inline_search
    }
}

impl Component for MusicWorkspaceComponent {
    fn view(&mut self, frame: &mut Frame, area: Rect) {
        self.layout = LayoutMain::default();
        let wide = wide_hero_presentation(area).is_some();

        // §2.5: at a breakpoint flip carry the outgoing control's anchor into
        // the incoming one so the selected album keeps its screen-row offset.
        // Music is a single kept-mounted component, so the hand-off is internal
        // to `view` (no cross-component shell dance like TV).
        if let Some(was_wide) = self.last_wide {
            if was_wide != wide && self.pending_anchor.is_none() {
                self.pending_anchor = self.viewport_anchor(self.painted_viewport_height());
            }
        }
        let flip_anchor = self.pending_anchor.take();
        if let Some(anchor) = &flip_anchor {
            if let Some(idx) = self
                .context
                .list
                .items
                .iter()
                .position(|item| item.id == anchor.selected_target)
            {
                self.album_cursor = idx;
            }
            // The write side uses the same *content* viewport height the read
            // side (`viewport_anchor`) measured against: for Wide that is the
            // last painted browser-area height (unchanged across a flip at a
            // fixed terminal size); for Narrow the painter applies it itself
            // at its own content-area height.
            if wide {
                self.apply_wide_anchor(anchor, self.wide_viewport_height.max(1));
            }
        }
        self.sync_narrow_selection();

        let context = self.context.clone().with_local_state(
            self.album_cursor,
            self.album_scroll,
            self.track_cursor,
        );
        if !wide && self.inline_search.is_active() {
            // Normal Music passes its whole list area to the shared search
            // painter (design.md D3); the ordinary grouped composer does not
            // also paint it.
            let items = self.inline_search.ordered_items();
            let query = self.inline_search.query().to_string();
            let loading = self.inline_search.loading();
            let cursor = self.inline_search.cursor();
            let scroll_in = self.inline_search.scroll();
            let areas = crate::app::render::arrangements::wide_hero::pill_bar_areas(area);
            let list_area = areas.content_area;
            let columns = crate::app::library_column_width::library_column_count(list_area.width);
            let new_scroll = crate::app::render::render_inline_search(
                frame,
                areas.pills_area,
                list_area,
                &query,
                loading,
                items,
                cursor,
                scroll_in,
                self.context.focused,
                columns,
                self.inline_search.layout_mut(),
            );
            self.inline_search.set_scroll(new_scroll);
            self.narrow_list_area = Rect::default();
            self.image_paint = None;
        } else if !wide {
            let output = render_narrow_music_group_with_ctx(
                frame,
                area,
                &context,
                &mut self.layout,
                &mut self.narrow_list,
                flip_anchor.as_ref(),
            );
            self.album_scroll = output.final_scroll;
            self.narrow_list.set_scroll(output.final_scroll);
            self.narrow_viewport_height = output.viewport_height;
            self.narrow_list_area = output.narrow_list_area;
            self.image_paint = output.image_paint;
        } else {
            self.narrow_list_area = Rect::default();
            let output = render_wide_music_group_with_ctx(
                frame,
                area,
                &context,
                &mut self.layout,
                &mut self.wide_list,
                &mut self.track_list,
                &mut self.inline_search,
            );
            if !self.inline_search.is_active() {
                self.album_scroll = output.final_scroll;
            }
            self.image_paint = output.image_paint;
            self.wide_viewport_height = self.layout.wide_music_browser_area.height as usize;
            self.wide_selected_row_offset = self
                .layout
                .selected_item_rect
                .and_then(|rect| rect.y.checked_sub(self.layout.wide_music_browser_area.y))
                .map(usize::from);
        }
        // Adopt the group-pill rects the pill painter just published into the
        // irregular-chrome registry (design.md D6), for both breakpoints.
        self.pill_regions.clear();
        for (rect, target) in &self.layout.selector_tabs {
            self.pill_regions.push(*rect, *target);
        }
        self.last_wide = Some(wide);
    }

    fn query<'a>(&'a self, _attr: Attribute) -> Option<QueryResult<'a>> {
        None
    }

    fn attr(&mut self, attr: Attribute, value: AttrValue) {
        if attr == Attribute::Focus {
            self.context.focused = matches!(value, AttrValue::Flag(true));
        }
    }

    fn state(&self) -> State {
        State::None
    }

    fn perform(&mut self, _cmd: Cmd) -> CmdResult {
        CmdResult::NoChange
    }
}

impl AppComponent<Msg, UserEvent> for MusicWorkspaceComponent {
    fn on(&mut self, event: &Event<UserEvent>) -> Option<Msg> {
        match event {
            Event::Keyboard(key) => self.handle_key(key),
            Event::Mouse(mouse) => self.handle_mouse(mouse),
            _ => None,
        }
    }
}
