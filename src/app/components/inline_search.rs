//! Shared embedded Inline Search control (design.md D1/D2).
//!
//! [`InlineSearch`] is a plain, unmounted control: active/inactive state,
//! query, the plain-or-recursive-album candidate pool, scored result order
//! stored as `(original_index, score)` pairs, result cursor/scroll, loading,
//! its last painted result geometry, and its private mouse gesture state.
//! [`InlineSearchHost`] is the minimal contract that will expose one embedded
//! control per destination to shell adapters; it does not choose a
//! destination or hand out Service/runtime objects.
//!
use ratatui::layout::Position;
use tuirealm::event::{Key, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};

use super::mouse::gesture::{MouseGesture, MouseGestureState};
use crate::app::layout::LayoutMain;
use crate::app::ui_util::move_cursor;

#[derive(Clone)]
pub(in crate::app) enum SearchPool {
    Items(Vec<mbv_core::api::EmbyItem>),
    Albums(Vec<crate::app::AlbumSearchEntry>),
}

impl SearchPool {
    fn len(&self) -> usize {
        match self {
            Self::Items(items) => items.len(),
            Self::Albums(entries) => entries.len(),
        }
    }

    /// The item at a corpus index, with an album's indexed display label
    /// substituted for its bare name (design.md D2).
    fn resolved_item_at(&self, index: usize) -> Option<mbv_core::api::EmbyItem> {
        match self {
            Self::Items(items) => items.get(index).cloned(),
            Self::Albums(entries) => entries.get(index).map(|entry| {
                let mut item = entry.album.clone();
                item.name = entry.display_label.clone();
                item
            }),
        }
    }

    /// `(original_index, score)` for every corpus entry that fuzzy-matches
    /// `query` against its match text (display name, or indexed
    /// `search_text` for albums).
    fn match_scores(
        &self,
        matcher: &fuzzy_matcher::skim::SkimMatcherV2,
        query: &str,
    ) -> Vec<(usize, i64)> {
        use fuzzy_matcher::FuzzyMatcher;
        match self {
            Self::Items(items) => items
                .iter()
                .enumerate()
                .filter_map(|(i, item)| {
                    matcher
                        .fuzzy_match(&item.display_name(), query)
                        .map(|score| (i, score))
                })
                .collect(),
            Self::Albums(entries) => entries
                .iter()
                .enumerate()
                .filter_map(|(i, entry)| {
                    matcher
                        .fuzzy_match(&entry.search_text, query)
                        .map(|score| (i, score))
                })
                .collect(),
        }
    }
}

/// Resolved effect of a key the shared control consumed (design.md D4). The
/// host translates this into its own typed shell request; the control never
/// depends on the shell's `Msg` type.
#[derive(Debug, PartialEq, Eq)]
pub(in crate::app) enum InlineSearchAction {
    Activate { id: String, item_type: String },
    Dismiss,
}

/// A mouse gesture the shared control resolved onto a result row but cannot
/// act on itself (design.md D6, decision 1: geometry stays with the owner).
/// The host translates it into its own item-based context-menu shell request,
/// resolving the target from [`InlineSearch::selected_item`].
#[derive(Debug, PartialEq, Eq)]
pub(in crate::app) enum InlineSearchMouse {
    /// A right click landed on a result row; the cursor has been moved there.
    ContextMenu,
}

/// The shared embedded Inline Search control (design.md D1). Never mounted,
/// focused, subscribed, or given a `ComponentId`; the host that embeds it
/// paints through `crate::app::render::render_inline_search` and gives it
/// first refusal on keyboard/mouse events while active.
pub(in crate::app) struct InlineSearch {
    active: bool,
    query: String,
    pool: SearchPool,
    /// Stable-sorted (ties keep corpus order) descending by score; an empty
    /// query is every corpus index in corpus order (design.md D2).
    order: Vec<(usize, i64)>,
    cursor: usize,
    scroll: usize,
    loading: bool,
    /// Last painted result geometry, published by the shared render
    /// component for column-aware cursor/mouse resolution.
    layout: LayoutMain,
    /// Private per-host gesture recognition (ADR 0024, design.md D1).
    mouse_gestures: MouseGestureState,
    /// Origin of an unreleased left press, for recognizing a drag that begins
    /// in the search bar and releases on a result row (P2: kept local so the
    /// generic `MouseGestureState` stays inert for every other consumer).
    left_press: Option<Position>,
}

impl InlineSearch {
    pub(in crate::app) fn new() -> Self {
        Self {
            active: false,
            query: String::new(),
            pool: SearchPool::Items(Vec::new()),
            order: Vec::new(),
            cursor: 0,
            scroll: 0,
            loading: false,
            layout: LayoutMain::default(),
            mouse_gestures: MouseGestureState::new(),
            left_press: None,
        }
    }

    pub(in crate::app) fn is_active(&self) -> bool {
        self.active
    }

    /// Starts a session locally with an empty query (design.md D4); reopening
    /// after a dismissal always starts empty.
    pub(in crate::app) fn open(&mut self) {
        self.active = true;
        self.query.clear();
        self.pool = SearchPool::Items(Vec::new());
        self.order.clear();
        self.cursor = 0;
        self.scroll = 0;
        self.loading = false;
    }

    /// Dismisses locally, discarding the query and results.
    pub(in crate::app) fn close(&mut self) {
        self.active = false;
        self.query.clear();
        self.order.clear();
        self.cursor = 0;
        self.scroll = 0;
    }

    pub(in crate::app) fn query(&self) -> &str {
        &self.query
    }

    pub(in crate::app) fn restore_query(&mut self, query: String) {
        self.query = query;
        self.recompute_order();
        self.cursor = self.cursor.min(self.order.len().saturating_sub(1));
    }

    pub(in crate::app) fn loading(&self) -> bool {
        self.loading
    }

    pub(in crate::app) fn set_loading(&mut self, loading: bool) {
        self.loading = loading;
    }

    pub(in crate::app) fn cursor(&self) -> usize {
        self.cursor
    }

    pub(in crate::app) fn scroll(&self) -> usize {
        self.scroll
    }

    pub(in crate::app) fn set_scroll(&mut self, scroll: usize) {
        self.scroll = scroll;
    }

    pub(in crate::app) fn selected_target(&self) -> Option<(String, String)> {
        self.selected_item().map(|item| (item.id, item.item_type))
    }

    pub(in crate::app) fn restore_target(
        &mut self,
        target: Option<(String, String)>,
        row_offset: usize,
    ) {
        if let Some((id, item_type)) = target {
            if let Some(cursor) = self.order.iter().position(|&(idx, _)| {
                self.pool
                    .resolved_item_at(idx)
                    .is_some_and(|item| item.id == id && item.item_type == item_type)
            }) {
                self.cursor = cursor;
                self.scroll = row_offset.min(cursor);
            }
        }
    }

    pub(in crate::app) fn results_len(&self) -> usize {
        self.order.len()
    }

    pub(in crate::app) fn layout(&self) -> &LayoutMain {
        &self.layout
    }

    pub(in crate::app) fn layout_mut(&mut self) -> &mut LayoutMain {
        &mut self.layout
    }

    /// Replaces the candidate pool, preserving the selected stable target
    /// (id + item type) when it is still present and otherwise clamping to
    /// the first valid result (design.md D2).
    pub(in crate::app) fn set_pool(&mut self, pool: SearchPool) {
        let target = self.selected_item().map(|item| (item.id, item.item_type));
        self.pool = pool;
        self.recompute_order();
        self.cursor = target
            .and_then(|(id, item_type)| {
                self.order.iter().position(|&(idx, _)| {
                    self.pool
                        .resolved_item_at(idx)
                        .is_some_and(|item| item.id == id && item.item_type == item_type)
                })
            })
            .unwrap_or(0);
    }

    /// The item under the cursor, resolved from the stored order without
    /// materializing the whole result set (design.md D2).
    pub(in crate::app) fn selected_item(&self) -> Option<mbv_core::api::EmbyItem> {
        let &(idx, _) = self.order.get(self.cursor)?;
        self.pool.resolved_item_at(idx)
    }

    /// Materializes the ordered result set for one paint; not used for
    /// cursor movement or selection (design.md D2).
    pub(in crate::app) fn ordered_items(&self) -> Vec<mbv_core::api::EmbyItem> {
        self.order
            .iter()
            .filter_map(|&(idx, _)| self.pool.resolved_item_at(idx))
            .collect()
    }

    fn recompute_order(&mut self) {
        if self.query.is_empty() {
            self.order = (0..self.pool.len()).map(|i| (i, 0)).collect();
            return;
        }
        use fuzzy_matcher::skim::SkimMatcherV2;
        let matcher = SkimMatcherV2::default();
        let mut scored = self.pool.match_scores(&matcher, &self.query);
        scored.sort_by_key(|&(_, score)| std::cmp::Reverse(score));
        self.order = scored;
    }

    fn move_cursor(&mut self, delta: i64) {
        self.cursor = move_cursor(self.cursor, delta, self.order.len());
    }

    /// Page size for PageUp/PageDown, derived from the last painted result
    /// area (falls back to one row before the first paint).
    fn page_size(&self) -> i64 {
        self.layout.left_area.height.max(1) as i64
    }

    fn push_char(&mut self, c: char) {
        self.query.push(c);
        self.recompute_order();
        self.cursor = 0;
        self.scroll = 0;
    }

    /// Resolves Up/Down/PageUp/PageDown/Home/End/Enter/Escape/Backspace
    /// (design.md D4). An empty-query Backspace dismisses, matching the
    /// standing dismissal contract.
    pub(in crate::app) fn handle_key(
        &mut self,
        key: &tuirealm::event::KeyEvent,
    ) -> Option<InlineSearchAction> {
        if key
            .modifiers
            .intersects(KeyModifiers::ALT | KeyModifiers::CONTROL)
        {
            return None;
        }
        match key.code {
            Key::Up => self.move_cursor(-1),
            Key::Down => self.move_cursor(1),
            Key::PageUp => {
                let step = self.page_size();
                self.move_cursor(-step);
            }
            Key::PageDown => {
                let step = self.page_size();
                self.move_cursor(step);
            }
            Key::Home => self.cursor = 0,
            Key::End => self.cursor = self.order.len().saturating_sub(1),
            Key::Enter => {
                if let Some(item) = self.selected_item() {
                    return Some(InlineSearchAction::Activate {
                        id: item.id,
                        item_type: item.item_type,
                    });
                }
            }
            Key::Esc => return Some(InlineSearchAction::Dismiss),
            Key::Char(c) => self.push_char(c),
            Key::Backspace => {
                if self.query.is_empty() {
                    return Some(InlineSearchAction::Dismiss);
                }
                self.query.pop();
                self.recompute_order();
                self.cursor = 0;
                self.scroll = 0;
            }
            _ => {}
        }
        None
    }

    /// Move the result cursor to the row painted at `at`, if `at` is inside
    /// the last painted result area. Returns whether the point was a result
    /// row.
    fn select_row_at(&mut self, at: Position) -> bool {
        if !self.layout.left_area.contains(at) {
            return false;
        }
        let row = at.y.saturating_sub(self.layout.left_area.y) as usize;
        self.cursor = move_cursor(row, 0, self.order.len());
        true
    }

    /// Mouse handling (ADR 0024, design.md D6): a left click on a result row
    /// moves the cursor to that row; a left press that begins outside the
    /// result area (e.g. in the search bar) and releases on a result row does
    /// the same. A right click on a result row moves the cursor there and asks
    /// the host to open its context menu. Every other gesture is a no-op.
    /// Resolved against the last painted result geometry.
    pub(in crate::app) fn handle_mouse(&mut self, mouse: &MouseEvent) -> Option<InlineSearchMouse> {
        if matches!(mouse.kind, MouseEventKind::Moved) {
            return None;
        }
        let point = Position {
            x: mouse.column,
            y: mouse.row,
        };
        match mouse.kind {
            MouseEventKind::Down(MouseButton::Left) => self.left_press = Some(point),
            MouseEventKind::Up(MouseButton::Left) => {
                if let Some(origin) = self.left_press.take() {
                    if origin != point && !self.layout.left_area.contains(origin) {
                        self.select_row_at(point);
                    }
                }
            }
            _ => {}
        }
        let gesture = self.mouse_gestures.recognize(mouse)?;
        match gesture {
            MouseGesture::Click(at) | MouseGesture::DoubleClick(at) => {
                self.select_row_at(at);
            }
            MouseGesture::RightClick(at) => {
                if self.select_row_at(at) {
                    return Some(InlineSearchMouse::ContextMenu);
                }
            }
            MouseGesture::Scroll { .. } => {}
        }
        None
    }

    #[cfg(test)]
    pub(in crate::app) fn test_pool_item_ids(&self) -> Vec<String> {
        match &self.pool {
            SearchPool::Items(items) => items.iter().map(|item| item.id.clone()).collect(),
            SearchPool::Albums(entries) => {
                entries.iter().map(|entry| entry.album.id.clone()).collect()
            }
        }
    }
}

impl Default for InlineSearch {
    fn default() -> Self {
        Self::new()
    }
}

/// Minimal contract exposing one embedded [`InlineSearch`] to shell adapters
/// (design.md D1). It does not define another application framework, choose
/// a destination, or expose Service/runtime objects; destinations implement
/// it once they embed the control (group 2).
pub(in crate::app) trait InlineSearchHost {
    fn inline_search(&self) -> &InlineSearch;
    fn inline_search_mut(&mut self) -> &mut InlineSearch;
    fn inline_search_transfer(&self) -> Option<(String, String, usize)> {
        let search = self.inline_search();
        search
            .selected_target()
            .map(|(id, item_type)| (id, item_type, search.scroll()))
    }
    fn apply_inline_search_transfer(
        &mut self,
        query: String,
        target: Option<(String, String)>,
        row_offset: usize,
    ) {
        let search = self.inline_search_mut();
        // A transfer is the sole exception to the normal open/close lifecycle:
        // it moves an already-open session to the other TV owner.
        search.active = true;
        search.restore_query(query);
        search.restore_target(target, row_offset);
    }
    fn selected_inline_search_item(&self) -> Option<mbv_core::api::EmbyItem> {
        self.inline_search().selected_item()
    }
    fn restore_inline_search_query(&mut self, query: String) {
        self.inline_search_mut().restore_query(query);
    }

    fn open_inline_search(&mut self) {
        self.inline_search_mut().open();
    }
    fn close_inline_search(&mut self) {
        self.inline_search_mut().close();
    }
    fn set_inline_search_content(&mut self, pool: SearchPool, loading: bool, focused: bool) {
        let search = self.inline_search_mut();
        search.set_pool(pool);
        search.set_loading(loading);
        let _ = focused;
    }
}
