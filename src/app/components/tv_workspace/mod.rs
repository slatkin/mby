//! Interactive Component for the wide Emby TV workspace.
//!
//! The shell mirrors the App-derived browser/detail snapshot. The component
//! keeps the active pane and the season/episode cursor used to paint the two
//! child targets; cross-authority effects use typed shell requests.

use ratatui::layout::{Position, Rect};
use ratatui::Frame;
use tuirealm::command::{Cmd, CmdResult};
use tuirealm::component::{AppComponent, Component};
use tuirealm::event::{Event, MouseEvent, MouseEventKind};
use tuirealm::props::{AttrValue, Attribute, QueryResult};
use tuirealm::state::State;

use mbv_core::api::{EmbyItem, TICKS_PER_SECOND};

use super::inline_search::{InlineSearch, InlineSearchHost, InlineSearchMouse};
use super::media_list::{MediaKind, MediaListRow, MediaSemanticState, WideMediaList};
use super::mouse::gesture::{MouseGesture, MouseGestureState};
use super::mouse::hit::HitRegions;
use super::msg::{Msg, ShellRequest, TvHit};
use super::user_event::UserEvent;
#[cfg(test)]
use crate::app::layout::LayoutMain;
use crate::app::render::{
    effective_sort_str, letter_bucket, render_wide_tv_with_ctx, HomeImagePaint, TvWideRenderCtx,
};
use crate::app::ui_util::{list_duration_secs, natural_sort_key};
#[cfg(test)]
use tuirealm::event::Key;

mod keyboard;
mod navigation;

#[derive(Clone, Copy, Eq, PartialEq)]
enum Pane {
    Series,
    Episodes,
}

pub struct TvWorkspaceComponent {
    context: TvWideRenderCtx,
    list: WideMediaList<String>,
    cursor: usize,
    season_cursor: usize,
    /// Embedded canonical control for the recessed episode media-list box
    /// (task 4.2d): owns cursor/scroll/hit-resolution for the current
    /// season's episode rows. Content always mirrors the current season
    /// (like `list` mirrors the series rail) so the box previews episodes
    /// even while the Series pane holds focus; `pane` controls whether its
    /// selected row paints as focused.
    episodes: WideMediaList<String>,
    pane: Pane,
    initialized: bool,
    last_series_id: Option<String>,
    layout: crate::app::layout::LayoutMain,
    image_paint: Option<HomeImagePaint>,
    pending_anchor: Option<super::media_list::ViewportAnchor<String>>,
    viewport_height: usize,
    /// Private per-parent gesture recognition (ADR 0024, design.md D3): owns
    /// the double-click window and wheel throttle. Not a shared clock.
    mouse_gestures: MouseGestureState,
    /// Irregular Episodes-pane chrome — season pills only (design.md D6),
    /// repopulated in `view()` from the geometry the wide-TV painter just
    /// produced. Both panes now have an embedded canonical control, so row
    /// identity comes from `WideMediaList::resolve_ordinal_at_y` instead; the
    /// blank Episodes-pane fallback is resolved directly against
    /// `tv_wide_left_area` in `resolve_hit`.
    tv_chrome: HitRegions<TvHit>,
    /// The embedded Inline Search control (design.md D1). See
    /// `BrowserComponent::inline_search` for the migration-phase notes.
    inline_search: InlineSearch,
}

/// Build the embedded episode `WideMediaList`'s rows from a season's
/// episodes (task 4.2d): the same title/duration formatting the hand-painted
/// table previously rendered, now the canonical control's row content.
fn build_episode_rows(episodes: &[EmbyItem]) -> Vec<MediaListRow<String>> {
    episodes
        .iter()
        .enumerate()
        .map(|(index, episode)| {
            let number = if episode.index_number > 0 {
                episode.index_number
            } else {
                index as i64 + 1
            };
            MediaListRow::Item {
                target: episode.id.clone(),
                primary: format!("{number}. {}", episode.name),
                trailing: None,
                duration: list_duration_secs(episode.runtime_ticks / TICKS_PER_SECOND),
                kind: MediaKind::Media,
                semantic_state: MediaSemanticState::Ordinary,
            }
        })
        .collect()
}

impl TvWorkspaceComponent {
    pub fn new() -> Self {
        let context = TvWideRenderCtx::new(
            crate::app::render::LibraryListRenderCtx::from_items(Vec::new(), 0, 0),
            None,
            None,
            0,
            None,
            false,
        );
        Self {
            context,
            list: WideMediaList::new(),
            cursor: 0,
            season_cursor: 0,
            episodes: WideMediaList::new(),
            pane: Pane::Series,
            initialized: false,
            last_series_id: None,
            layout: Default::default(),
            image_paint: None,
            pending_anchor: None,
            viewport_height: 1,
            mouse_gestures: MouseGestureState::new(),
            tv_chrome: HitRegions::new(),
            inline_search: InlineSearch::new(),
        }
    }

    pub(in crate::app) fn set_content(&mut self, context: TvWideRenderCtx) {
        let grouped = !context.list.is_search_active()
            && (context.show_letter_pills
                || context.list.has_letter_filter()
                || context.list.true_total() >= 50);
        let bucket_total = if context.list.has_letter_filter() {
            usize::MAX
        } else {
            context.list.true_total()
        };
        let mut sorted_items: Vec<&EmbyItem> = context.list.items.iter().collect();
        sorted_items.sort_by_key(|item| natural_sort_key(effective_sort_str(item)));
        let rows = sorted_items.iter().enumerate().flat_map(|(index, item)| {
            let heading = grouped
                .then(|| {
                    let current = letter_bucket(item, bucket_total);
                    let previous = index
                        .checked_sub(1)
                        .map(|i| letter_bucket(sorted_items[i], bucket_total));
                    (previous.as_deref() != Some(current.as_str())).then(|| {
                        let heading = MediaListRow::Heading { text: current };
                        if previous.is_some() {
                            vec![MediaListRow::Spacer, heading]
                        } else {
                            vec![heading]
                        }
                    })
                })
                .flatten();
            heading
                .into_iter()
                .flatten()
                .chain(std::iter::once(MediaListRow::Item {
                    target: item.id.clone(),
                    primary: item.display_name(),
                    trailing: (item.production_year > 0).then(|| item.production_year.to_string()),
                    duration: None,
                    kind: MediaKind::Collection,
                    // TV series rows are never dimmed on watched/played state
                    // (legacy rail parity): the canonical row colour follows
                    // panel focus only.
                    semantic_state: MediaSemanticState::Ordinary,
                }))
        });
        let rows = rows.collect::<Vec<_>>();
        // The canonical cursor is in the rendered (natural-sort) order. Seed
        // the local list from that order on first mount; thereafter preserve
        // the stable target already owned by the component.
        let restore_target = self.list.selected_target().cloned();
        self.list.set_content(rows);
        if !self.initialized {
            self.list.select_index(context.list.cursor());
        } else if let Some(target) = restore_target {
            self.list.select_target(&target);
        }
        let series_changed =
            context.selected_series.as_ref().map(|item| &item.id) != self.last_series_id.as_ref();
        if series_changed {
            self.season_cursor = 0;
            self.episodes.set_content(Vec::new());
            self.pane = Pane::Series;
            self.last_series_id = context.selected_series.as_ref().map(|item| item.id.clone());
        }
        let mut restore_episode_index = None;
        if !self.initialized {
            if !series_changed {
                self.season_cursor = context.season_cursor;
                restore_episode_index = context.episode_cursor;
                self.pane = if context.episode_cursor.is_some() {
                    Pane::Episodes
                } else {
                    Pane::Series
                };
            }
            self.initialized = true;
        }
        // Content projection never carries framework focus; preserve the
        // component-owned value across the shell snapshot swap.
        let focused = self.context.focused;
        self.context = context;
        self.context.focused = focused;
        self.cursor = self.list.cursor();
        let season_count = self
            .context
            .series_detail
            .as_ref()
            .map_or(0, |detail| detail.seasons.len());
        self.season_cursor = self.season_cursor.min(season_count.saturating_sub(1));
        // Missing detail or episode data means the season's refresh is still
        // loading; do not discard the component-local episode selection by
        // clearing the canonical control's content in that interval. Once
        // the season's episode key is present (even an empty `Vec`), refresh
        // unconditionally -- the box previews episodes regardless of pane.
        if self.current_season_episodes_key_present() {
            self.refresh_episode_rows();
            if let Some(index) = restore_episode_index {
                self.episodes.select_index(index);
            }
        }
    }

    /// Test-only: drive framework focus the way `Component::attr` does.
    #[cfg(test)]
    pub(in crate::app) fn set_focused(&mut self, focused: bool) {
        self.context.focused = focused;
    }

    /// The current season's episode `Vec`, `&[]` when the season has no
    /// episodes loaded yet or `series_detail`/`season_cursor` cannot resolve
    /// one.
    fn current_season_episodes(&self) -> &[EmbyItem] {
        self.context
            .series_detail
            .as_ref()
            .and_then(|detail| {
                let season = detail.seasons.get(self.season_cursor)?;
                detail.episodes.get(&season.id)
            })
            .map(Vec::as_slice)
            .unwrap_or(&[])
    }

    /// Whether the current season's episode key is present in
    /// `series_detail.episodes` at all (even mapped to an empty `Vec`) --
    /// the loaded/loading distinction the refresh guard in `set_content`
    /// needs, unlike [`Self::current_season_episodes`] which treats both as
    /// empty.
    fn current_season_episodes_key_present(&self) -> bool {
        self.context
            .series_detail
            .as_ref()
            .and_then(|detail| {
                let season = detail.seasons.get(self.season_cursor)?;
                detail.episodes.get(&season.id)
            })
            .is_some()
    }

    /// Rebuild the embedded episode `WideMediaList`'s content from the
    /// current season's episodes (design.md D3): preserves the selected
    /// target where still present, otherwise clamps locally -- the same
    /// canonical machinery the series rail uses.
    fn refresh_episode_rows(&mut self) {
        let rows = build_episode_rows(self.current_season_episodes());
        self.episodes.set_content(rows);
    }

    pub(in crate::app) fn cursor(&self) -> usize {
        self.list.cursor()
    }

    pub(in crate::app) fn viewport_anchor(
        &self,
        viewport_height: usize,
    ) -> Option<super::media_list::ViewportAnchor<String>> {
        self.list.viewport_anchor(viewport_height)
    }

    pub(in crate::app) fn painted_viewport_height(&self) -> usize {
        self.viewport_height
    }

    /// Whether letter pills are enabled in the pushed context.
    pub(in crate::app) fn show_letter_pills(&self) -> bool {
        self.context.show_letter_pills
    }

    /// The scroll offset the component tracks for its series list. Read by
    /// the breakpoint hand-off so the resting `BrowseLevel` scroll matches
    /// the wide workspace before the narrow `BrowserComponent` adopts it.
    pub(in crate::app) fn scroll(&self) -> usize {
        self.list.scroll()
    }

    /// One-shot re-anchor of the series cursor/scroll to a shell-owned
    /// resting position (breakpoint hand-off, migrate-narrow-browse task 2.3
    /// / D5). Mirrors `MusicWorkspaceComponent::re_anchor`: an ordinary
    /// `set_content` keeps the component's divergent local cursor, so the
    /// shell re-anchors explicitly when the active-destination pointer flips
    /// back to this kept-mounted component.
    pub(in crate::app) fn apply_viewport_anchor(
        &mut self,
        anchor: super::media_list::ViewportAnchor<String>,
    ) {
        if self.list.select_target(&anchor.selected_target) {
            self.cursor = self.list.cursor();
        }
        self.pending_anchor = Some(anchor);
    }

    pub(in crate::app) fn take_image_paint(&mut self) -> Option<HomeImagePaint> {
        self.image_paint.take()
    }

    pub(in crate::app) fn selected_item_id(&self) -> Option<String> {
        let target = self.list.selected_target()?;
        self.context
            .list
            .items
            .iter()
            .find(|item| &item.id == target)
            .map(|item| item.id.clone())
    }

    /// The series item under the component's own cursor, cloned out of the
    /// cached render context. `handle_key`'s Series Enter attaches this to
    /// `ShellRequest::TvActivate` so the shell effect targets the component
    /// selection instead of the mirrored App browse cursor.
    pub(in crate::app) fn selected_item(&self) -> Option<EmbyItem> {
        // Resolve through the same natural/effective order used to build the
        // rail. Stable IDs normally make this equivalent to target lookup;
        // ordinal resolution also keeps malformed duplicate-ID payloads from
        // collapsing two visibly distinct rows onto the first item.
        let mut items: Vec<&EmbyItem> = self.context.list.items.iter().collect();
        items.sort_by_key(|item| natural_sort_key(effective_sort_str(item)));
        items.get(self.list.cursor()).cloned().cloned()
    }

    /// The Series snapshot the shell pushed for this frame (`context
    /// .selected_series`), exposed so tests can verify the pushed detail
    /// follows the component's authoritative selection rather than the App
    /// browse cursor.
    pub(in crate::app) fn selected_series_snapshot(&self) -> Option<&EmbyItem> {
        self.context.selected_series.as_ref()
    }

    /// Return the component-owned selection needed to activate an episode.
    /// The shell uses these cursors to resolve the episode from App's cache;
    /// it never re-reads the library cursor for this action.
    pub(in crate::app) fn episode_activation_selection(&self) -> Option<(String, usize, usize)> {
        if self.episodes.is_empty() {
            return None;
        }
        Some((
            self.context.selected_series.as_ref()?.id.clone(),
            self.season_cursor,
            self.episodes.cursor(),
        ))
    }

    pub(in crate::app) fn selected_season(&self) -> Option<(String, String)> {
        let series_id = self.context.selected_series.as_ref()?.id.clone();
        let season_id = self
            .context
            .series_detail
            .as_ref()?
            .seasons
            .get(self.season_cursor)?
            .id
            .clone();
        Some((series_id, season_id))
    }

    /// Handle a mouse event against the component's painted workspace geometry.
    ///
    /// Gesture recognition (click / double-click / right-click / wheel) comes
    /// from the private `MouseGestureState` (ADR 0024, design.md D3). Season
    /// pills resolve through `tv_chrome` (design.md D6); both panes' row
    /// identity comes from their embedded `WideMediaList`. The component
    /// emits a semantic `Msg` with a resolved `TvHit` — never raw coordinates
    /// — except the context-menu anchor (design.md D4). A left click moves
    /// the component's local pane + pane cursor; a right click never does.
    fn handle_mouse(&mut self, mouse: &MouseEvent) -> Option<Msg> {
        // Inline Search gets first refusal while active (design.md D6): it
        // is painted over the same area the series rail would occupy, so
        // the rail never mutates for points there.
        if self.inline_search.is_active() {
            return match self.inline_search.handle_mouse(mouse) {
                Some(InlineSearchMouse::ContextMenu) => self
                    .inline_search
                    .selected_item()
                    .map(|item| Msg::Shell(ShellRequest::EmbyLibraryContextMenu { item })),
                None => None,
            };
        }
        // TV does not consume hover-move (design.md D7).
        if matches!(mouse.kind, MouseEventKind::Moved) {
            return None;
        }
        match self.mouse_gestures.recognize(mouse)? {
            MouseGesture::Scroll { at, delta } => {
                // Wheel scroll over the series list (`left_area` is the
                // right-pane list area this renderer publishes — the exact
                // region the legacy scroll arm hit-tested). The Episodes
                // pane has no wheel behaviour.
                if !self.layout.left_area.contains(at) {
                    return None;
                }
                self.move_rows(delta);
                Some(Msg::Shell(ShellRequest::TvScroll { delta }))
            }
            MouseGesture::Click(at) => {
                let hit = self.resolve_hit(at)?;
                self.apply_pane_click(hit);
                Some(Msg::Shell(ShellRequest::TvHitClick { hit }))
            }
            MouseGesture::DoubleClick(at) => {
                let hit = self.resolve_hit(at)?;
                self.apply_pane_click(hit);
                Some(Msg::Shell(ShellRequest::TvHitDoubleClick { hit }))
            }
            MouseGesture::RightClick(at) => {
                let hit = self.resolve_hit(at)?;
                Some(Msg::Shell(ShellRequest::TvHitContextMenu {
                    hit,
                    anchor: (mouse.column, mouse.row),
                }))
            }
        }
    }

    /// Move the component's local pane + pane cursor to the clicked `hit`.
    /// A click in the unfocused pane moves local focus there; a click in the
    /// already-focused pane keeps it. Clicking a season pill also selects
    /// that season; blank Episodes-pane space is consumed without changing
    /// the pane. Right-clicks never call this.
    fn apply_pane_click(&mut self, hit: TvHit) {
        match hit {
            TvHit::SeasonTab(index) => {
                self.pane = Pane::Episodes;
                self.season_cursor = index;
                self.refresh_episode_rows();
                self.episodes.select_first();
            }
            TvHit::EpisodeRow(index) => {
                self.pane = Pane::Episodes;
                self.episodes.select_index(index);
            }
            TvHit::SeriesRow(index) => {
                self.pane = Pane::Series;
                self.cursor = index;
            }
            TvHit::EpisodesPane => {}
        }
    }

    /// Resolve a workspace position to the pane + hit it lands in, from the
    /// component's own painted geometry. `None` = outside every TV rect
    /// (the clicks that remain unhandled).
    fn resolve_hit(&self, position: Position) -> Option<TvHit> {
        if let Some(&hit) = self.tv_chrome.resolve(position) {
            return Some(hit);
        }
        if self.layout.tv_wide_right_area.contains(position) {
            // Resolve the series row under the click from the embedded
            // canonical control (design.md D6). A header/gap cell (or a click
            // in the pane outside the list rows) returns the current series
            // cursor, matching the legacy blank-space click no-op. A click in
            // the right pane above the list clamps to the first row, matching
            // the legacy `saturating_sub` row keying.
            let list_area = self.layout.tv_wide_list_area;
            let target = self
                .list
                .resolve_ordinal_at_y(list_area, position.y.max(list_area.y))
                .unwrap_or(self.cursor);
            return Some(TvHit::SeriesRow(target));
        }
        // Episode rows resolve the same way against the embedded episode
        // control (design.md D6) before falling back to the blank
        // Episodes-pane no-op.
        let episode_list_area = self.layout.tv_wide_episode_list_area;
        if episode_list_area.contains(position) {
            if let Some(target) = self
                .episodes
                .resolve_ordinal_at_y(episode_list_area, position.y)
            {
                return Some(TvHit::EpisodeRow(target));
            }
        }
        if self.layout.tv_wide_left_area.contains(position) {
            return Some(TvHit::EpisodesPane);
        }
        None
    }

    #[cfg(test)]
    pub(crate) fn test_layout(&self) -> &LayoutMain {
        &self.layout
    }
}

impl Default for TvWorkspaceComponent {
    fn default() -> Self {
        Self::new()
    }
}

impl InlineSearchHost for TvWorkspaceComponent {
    fn inline_search(&self) -> &InlineSearch {
        &self.inline_search
    }

    fn inline_search_mut(&mut self) -> &mut InlineSearch {
        &mut self.inline_search
    }
}

impl Component for TvWorkspaceComponent {
    fn view(&mut self, frame: &mut Frame, area: Rect) {
        self.viewport_height = area.height as usize;
        self.layout = Default::default();
        self.image_paint = None;
        if let Some(anchor) = self.pending_anchor.take() {
            self.list
                .apply_viewport_anchor(&anchor, area.height as usize);
        }
        // `episode_cursor` in the render context only signals the Episodes
        // pane's focus/highlight state now (the embedded control tracks the
        // real cursor); it is `Some` exactly while `pane == Pane::Episodes`.
        let episode_focus_cursor = (self.pane == Pane::Episodes).then(|| self.episodes.cursor());
        let context = self.context.clone().with_local_state(
            self.list.cursor(),
            self.list.scroll(),
            self.season_cursor,
            episode_focus_cursor,
        );
        let (scroll, image_paint) = render_wide_tv_with_ctx(
            frame,
            area,
            &context,
            &mut self.layout,
            &mut self.list,
            &mut self.episodes,
            &mut self.inline_search,
        );
        if !self.inline_search.is_active() {
            self.list.set_scroll(scroll);
        }
        self.cursor = self.list.cursor();
        self.image_paint = image_paint;

        // Adopt the season-pill chrome the wide-TV painter just produced into
        // the irregular-chrome registry (design.md D6); both list rows and
        // the blank Episodes-pane fallback resolve directly in `resolve_hit`.
        self.tv_chrome.clear();
        for (rect, index) in &self.layout.tv_wide_season_tabs {
            self.tv_chrome.push(*rect, TvHit::SeasonTab(*index));
        }
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

impl AppComponent<Msg, UserEvent> for TvWorkspaceComponent {
    fn on(&mut self, event: &Event<UserEvent>) -> Option<Msg> {
        match event {
            Event::Keyboard(key) => self.handle_key(key),
            Event::Mouse(mouse) => self.handle_mouse(mouse),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::render::LibraryListRenderCtx;
    use crate::app::tests::make_item;
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;
    use tuirealm::component::Component;
    use tuirealm::event::{Event, KeyEvent, KeyModifiers};

    /// Task 4.2d: the embedded episode `WideMediaList` field replaces the
    /// old `Option<usize>` episode cursor. This exercises the same
    /// component-local persistence through the canonical control -- moving
    /// the cursor via keyboard, then re-syncing the same series/season data,
    /// must preserve it (target-preserving `WideMediaList::set_content`).
    #[test]
    fn tv_workspace_keeps_episode_pane_cursor_local_between_syncs() {
        let mut component = TvWorkspaceComponent::new();
        component.set_focused(true);
        let mut series = make_item("Series", "Series");
        series.id = "series-id".into();
        let mut season = make_item("Season 1", "Season");
        season.id = "season-1".into();
        let episode = |name: &str, id: &str| {
            let mut item = make_item(name, "Episode");
            item.id = id.into();
            item
        };
        let detail = crate::app::SeriesDetail {
            seasons: vec![season],
            episodes: [(
                "season-1".into(),
                vec![
                    episode("Episode 1", "episode-1"),
                    episode("Episode 2", "episode-2"),
                ],
            )]
            .into_iter()
            .collect(),
        };
        component.set_content(TvWideRenderCtx::new(
            LibraryListRenderCtx::from_items(vec![series.clone()], 0, 0),
            Some(series.clone()),
            Some(detail.clone()),
            0,
            None,
            false,
        ));
        component.on(&Event::Keyboard(KeyEvent {
            code: Key::Right,
            modifiers: KeyModifiers::NONE,
        }));
        let message = component.on(&Event::Keyboard(KeyEvent {
            code: Key::Down,
            modifiers: KeyModifiers::NONE,
        }));
        assert!(matches!(
            message,
            Some(Msg::Shell(ShellRequest::TvEpisodeMove { delta: 1 }))
        ));
        assert_eq!(component.episodes.cursor(), 1);

        component.set_content(TvWideRenderCtx::new(
            LibraryListRenderCtx::from_items(vec![series.clone()], 0, 0),
            Some(series),
            Some(detail),
            0,
            None,
            false,
        ));
        assert_eq!(component.episodes.cursor(), 1);
    }

    #[test]
    fn tv_workspace_series_change_resets_local_selection() {
        let mut component = TvWorkspaceComponent::new();
        component.set_focused(true);
        let mut season_one = make_item("Season 1", "Season");
        season_one.id = "season-1".into();
        let mut season_two = make_item("Season 2", "Season");
        season_two.id = "season-2".into();
        let detail = crate::app::SeriesDetail {
            seasons: vec![season_one, season_two],
            episodes: std::collections::HashMap::new(),
        };
        let mut series_a = make_item("Series A", "Series");
        series_a.id = "series-a".into();
        let mut series_b = make_item("Series B", "Series");
        series_b.id = "series-b".into();

        component.set_content(TvWideRenderCtx::new(
            LibraryListRenderCtx::from_items(vec![series_a.clone()], 0, 0),
            Some(series_a),
            Some(detail.clone()),
            0,
            None,
            false,
        ));
        component.move_season(1);

        component.set_content(TvWideRenderCtx::new(
            LibraryListRenderCtx::from_items(vec![series_b.clone()], 0, 0),
            Some(series_b),
            Some(detail),
            0,
            None,
            false,
        ));

        assert_eq!(component.season_cursor, 0);
        assert!(component.episodes.is_empty());
        assert!(matches!(component.pane, Pane::Series));
    }

    #[test]
    fn tv_workspace_renders_the_wide_workspace_without_app() {
        let mut component = TvWorkspaceComponent::new();
        component.set_focused(true);
        component.set_content(TvWideRenderCtx::new(
            LibraryListRenderCtx::from_items(vec![make_item("Series", "Series")], 0, 0),
            None,
            None,
            0,
            None,
            false,
        ));
        let mut terminal = Terminal::new(TestBackend::new(100, 20)).unwrap();
        terminal
            .draw(|frame| component.view(frame, frame.area()))
            .unwrap();
        assert!(terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .any(|cell| cell.symbol() == "S"));
    }
}
