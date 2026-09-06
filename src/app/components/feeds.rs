//! Interactive Component for the Feeds destination.
//!
//! The shell supplies validated feed snapshots. This component owns the
//! subscription/group selector and the watched filter (parent chrome); the
//! embedded canonical controls (`WideMediaList` for Wide hero Wide,
//! `InlineMediaBrowser` for inline Narrow) own the cursor and scroll over the
//! grouped-entry projection. `render_feeds_content` is the parent-owned pill
//! strip + chrome + hero painter and mounts the active control into the list
//! sub-rect below the pill strip. Refresh, playback, enqueue, and the legacy
//! `*HitRegion` mouse path remain shell/pre-#638 work.

use ratatui::layout::{Position, Rect};
use ratatui::Frame;
use tuirealm::command::{Cmd, CmdResult};
use tuirealm::component::{AppComponent, Component};
use tuirealm::event::{Event, Key, KeyEvent, KeyModifiers, MouseEvent, MouseEventKind};
use tuirealm::props::{AttrValue, Attribute, QueryResult};
use tuirealm::state::State;

use super::media_list::{
    InlineMediaBrowser, MediaKind, MediaListRow, MediaSemanticState, ViewportAnchor, WideMediaList,
};
use super::mouse::gesture::{MouseGesture, MouseGestureState};
use super::msg::{Msg, ShellRequest};
use super::user_event::UserEvent;
use crate::app::layout::LayoutMain;
use crate::app::render::{
    current_time_secs, feed_display_rows, feed_duration_text, render_feeds_content, FeedDisplayRow,
    FeedsRenderModel,
};
use crate::app::types_feed_tab::WatchedFilter;
use mbv_core::config::FeedSubscription;
use mbv_core::playback_queue::FeedEntry;

pub struct FeedsComponent {
    subscriptions: Vec<FeedSubscription>,
    entries: Vec<Vec<FeedEntry>>,
    all_entries: Vec<FeedEntry>,
    visible_entries: Vec<FeedEntry>,
    /// Canonical grouped-entry projection; selectors remain parent chrome.
    /// Both controls hold the same rows every `rebuild_visible_entries`; only
    /// one is painted per breakpoint, and they own cursor/scroll — the
    /// component keeps no mirror.
    canonical_list: WideMediaList<String>,
    inline_list: InlineMediaBrowser<String>,
    watched_filter: WatchedFilter,
    selected_group: usize,
    /// Which canonical control the last `view()` painted (Wide hero Wide vs
    /// inline Narrow). Drives the single `ViewportAnchor` handoff on a
    /// breakpoint flip and which control `cursor()` reads.
    wide: bool,
    /// The scroll offset the painter resolved this frame — observability only
    /// (characterization tests), never fed back into the control.
    painted_offset: usize,
    loading: bool,
    images_enabled: bool,
    focused: bool,
    layout: LayoutMain,
    last_subscription_urls: Vec<String>,
    /// Private per-parent gesture recognition (ADR 0024, design.md D3): owns
    /// the double-click window and wheel throttle. Not a shared clock.
    mouse_gestures: MouseGestureState,
}

impl FeedsComponent {
    pub fn new() -> Self {
        Self {
            subscriptions: Vec::new(),
            entries: Vec::new(),
            all_entries: Vec::new(),
            visible_entries: Vec::new(),
            canonical_list: WideMediaList::new(),
            inline_list: InlineMediaBrowser::new(),
            watched_filter: WatchedFilter::default(),
            selected_group: 0,
            wide: false,
            painted_offset: 0,
            loading: false,
            images_enabled: true,
            focused: false,
            layout: LayoutMain::default(),
            last_subscription_urls: Vec::new(),
            mouse_gestures: MouseGestureState::new(),
        }
    }

    /// Replace the shell-owned snapshot while preserving the component's
    /// render and input state shape.
    pub(in crate::app) fn set_images_enabled(&mut self, images_enabled: bool) {
        self.images_enabled = images_enabled;
    }

    /// Test-only: drive framework focus the way `Component::attr` does.
    #[cfg(test)]
    pub(in crate::app) fn set_focused(&mut self, focused: bool) {
        self.focused = focused;
    }

    pub(in crate::app) fn set_content(
        &mut self,
        subscriptions: &[FeedSubscription],
        entries: &[Vec<FeedEntry>],
        all_entries: &[FeedEntry],
        loading: bool,
    ) {
        let subscription_urls: Vec<String> = subscriptions
            .iter()
            .map(|subscription| subscription.url.clone())
            .collect();
        let subscriptions_changed = self.last_subscription_urls != subscription_urls;
        self.last_subscription_urls = subscription_urls;
        self.subscriptions = subscriptions.to_vec();
        self.entries = entries.to_vec();
        self.all_entries = all_entries.to_vec();
        self.selected_group = self
            .selected_group
            .min(self.group_count().saturating_sub(1));
        self.loading = loading;
        self.rebuild_visible_entries();
        // An ordinary refresh keeps the active control authoritative (the
        // selected target is preserved by `ListCore::set_content`); only a
        // subscription-set change resets the selection.
        if subscriptions_changed {
            self.reset_selection();
        }
    }

    pub(in crate::app) fn cursor(&self) -> usize {
        if self.wide {
            self.canonical_list.cursor()
        } else {
            self.inline_list.cursor()
        }
    }

    pub(in crate::app) fn watched_filter(&self) -> WatchedFilter {
        self.watched_filter
    }

    pub(in crate::app) fn selected_group(&self) -> usize {
        self.selected_group
    }

    pub(in crate::app) fn scroll(&self) -> usize {
        self.painted_offset
    }

    pub(in crate::app) fn visible_titles(&self) -> Vec<&str> {
        self.visible_entries
            .iter()
            .map(|entry| entry.title.as_str())
            .collect()
    }

    pub(in crate::app) fn subscription_names(&self) -> Vec<&str> {
        self.subscriptions
            .iter()
            .map(|subscription| subscription.name.as_str())
            .collect()
    }

    pub(in crate::app) fn layout(&self) -> &LayoutMain {
        &self.layout
    }

    pub(in crate::app) fn group_count(&self) -> usize {
        1 + self.subscriptions.len()
    }

    fn rebuild_visible_entries(&mut self) {
        // Navigation uses maps produced by the previous render; invalidate
        // them whenever filtering/group content changes.
        self.layout.left_item_rows.clear();
        self.layout.left_row_map.clear();
        let source = if self.selected_group == 0 {
            &self.all_entries
        } else {
            self.entries
                .get(self.selected_group - 1)
                .map(Vec::as_slice)
                .unwrap_or(&[])
        };
        self.visible_entries = source
            .iter()
            .filter(|entry| self.watched_filter.matches(entry.played))
            .cloned()
            .collect();

        // Project grouped `FeedEntries` into the canonical row vocabulary:
        // `FeedAgeGroup` labels become non-selectable `Heading` rows, group
        // separators become `Spacer` rows, entries become selectable `Item`
        // rows carrying the stable `entry.guid` target and the watched
        // semantic state. Structural rows are filtered out of the control's
        // selectable index, so cursor movement skips them and the control's
        // `RowGeometry` owns the selectable-index vs display-index mapping.
        let now = current_time_secs();
        let rows: Vec<MediaListRow<String>> = feed_display_rows(&self.visible_entries, now)
            .into_iter()
            .map(|row| match row {
                FeedDisplayRow::Spacer => MediaListRow::Spacer,
                FeedDisplayRow::Heading(group) => MediaListRow::Heading {
                    text: group.label().to_string(),
                },
                FeedDisplayRow::Entry(index) => {
                    let entry = &self.visible_entries[index];
                    MediaListRow::Item {
                        target: entry.guid.clone(),
                        primary: entry.title.clone(),
                        trailing: None,
                        duration: feed_duration_text(entry.duration_ticks),
                        kind: MediaKind::Media,
                        semantic_state: if entry.played {
                            MediaSemanticState::Played
                        } else {
                            MediaSemanticState::Ordinary
                        },
                    }
                }
            })
            .collect();
        self.canonical_list.set_content(rows.clone());
        self.inline_list.set_content(rows);
    }

    /// Park the selection at the first entry on both controls (a discrete
    /// group/filter change; there is no per-group cursor cache).
    fn reset_selection(&mut self) {
        self.canonical_list.select_first();
        self.inline_list.select_first();
        self.painted_offset = 0;
    }

    /// Move the selection by `delta` selectable rows on both controls in
    /// lockstep, so they stay cursor-aligned across a breakpoint flip.
    fn move_selection(&mut self, delta: i64) {
        self.canonical_list.move_selection(delta);
        self.inline_list.move_selection(delta);
    }

    /// Select the row carrying `target` (a stable guid) on both controls, so
    /// they stay cursor-aligned across a breakpoint flip.
    fn select_target(&mut self, target: &str) {
        let target = target.to_string();
        self.canonical_list.select_target(&target);
        self.inline_list.select_target(&target);
    }

    /// Adopt `target` as the selected group and rebuild.
    fn select_group(&mut self, target: usize) {
        self.selected_group = target;
        self.rebuild_visible_entries();
        self.reset_selection();
    }

    fn page_size(&self) -> i64 {
        self.layout.left_area.height.saturating_sub(1).max(1) as i64
    }

    fn cycle_group(&mut self, delta: i64) {
        let count = self.group_count();
        self.selected_group =
            (self.selected_group as i64 + delta).rem_euclid(count as i64) as usize;
        self.rebuild_visible_entries();
        self.reset_selection();
    }

    fn handle_key(&mut self, key: &KeyEvent) -> Option<Msg> {
        if !self.focused {
            return None;
        }
        if key.modifiers.contains(KeyModifiers::CONTROL)
            || key.modifiers.contains(KeyModifiers::ALT)
        {
            return None;
        }
        match key.code {
            Key::Char('r') => Some(Msg::Shell(ShellRequest::RefreshFeeds)),
            Key::Char('w') => {
                self.watched_filter = self.watched_filter.cycle();
                self.rebuild_visible_entries();
                self.reset_selection();
                None
            }
            Key::Up | Key::Char('k') => {
                self.move_selection(-1);
                None
            }
            Key::Down | Key::Char('j') => {
                self.move_selection(1);
                None
            }
            Key::Left | Key::Char('h') => {
                self.move_selection(-1);
                None
            }
            Key::Right | Key::Char('l') => {
                self.move_selection(1);
                None
            }
            Key::PageUp => {
                self.move_selection(-self.page_size());
                None
            }
            Key::PageDown => {
                self.move_selection(self.page_size());
                None
            }
            Key::Home => {
                self.canonical_list.select_first();
                self.inline_list.select_first();
                None
            }
            Key::End => {
                self.canonical_list.select_last();
                self.inline_list.select_last();
                None
            }
            Key::Char('[') => {
                self.cycle_group(-1);
                None
            }
            Key::Char(']') => {
                self.cycle_group(1);
                None
            }
            Key::Enter => self
                .visible_entries
                .get(self.cursor())
                .map(|entry| Msg::Shell(ShellRequest::FeedsPlay(Some(entry.clone()))))
                .or(Some(Msg::Shell(ShellRequest::FeedsPlay(None)))),
            Key::Char('e') => self
                .visible_entries
                .get(self.cursor())
                .map(|entry| Msg::Shell(ShellRequest::FeedsEnqueue(Some(entry.clone()))))
                .or(Some(Msg::Shell(ShellRequest::FeedsEnqueue(None)))),
            _ => None,
        }
    }

    /// Handle a TuiRealm mouse event via the private `MouseGestureState`
    /// (ADR 0024, design.md D3). Row identity comes from the active canonical
    /// control's `resolve_point` (design.md D6); the selector pills stay
    /// parent chrome resolved from `selector_tabs`. The component emits a
    /// semantic `Msg` — never raw coordinates. Feeds has no keyboard
    /// context-menu action (task 4.6), so right-click is ignored.
    fn handle_mouse(&mut self, mouse: &MouseEvent) -> Option<Msg> {
        // Feeds does not consume hover-move (design.md D7).
        if matches!(mouse.kind, MouseEventKind::Moved) {
            return None;
        }
        match self.mouse_gestures.recognize(mouse)? {
            MouseGesture::Scroll { at, delta } => {
                if !self.layout.left_area.contains(at) {
                    return None;
                }
                self.move_selection(delta);
                None
            }
            MouseGesture::Click(at) => {
                if let Some((_, target)) = self
                    .layout
                    .selector_tabs
                    .iter()
                    .find(|(rect, _)| rect.contains(at))
                {
                    if *target < self.group_count() {
                        self.select_group(*target);
                    }
                    return None;
                }
                let target = self.resolve_row_id(at)?;
                self.select_target(&target);
                Some(Msg::Shell(ShellRequest::FeedsRowClick))
            }
            MouseGesture::DoubleClick(at) => {
                let target = self.resolve_row_id(at)?;
                self.select_target(&target);
                let index = self
                    .visible_entries
                    .iter()
                    .position(|entry| entry.guid == target)?;
                Some(Msg::Shell(ShellRequest::FeedsPlay(Some(
                    self.visible_entries[index].clone(),
                ))))
            }
            _ => None,
        }
    }

    /// The stable row id under `point`, resolved by the embedded canonical
    /// control that painted the active list (design.md D6).
    fn resolve_row_id(&self, at: Position) -> Option<String> {
        if self.wide {
            return self
                .canonical_list
                .resolve_point(self.layout.left_area, at)
                .cloned();
        }
        let detail_rows = self.layout.inline_hero_area.height as usize;
        self.inline_list
            .resolve_point(self.layout.left_area, detail_rows, at)
            .cloned()
    }

    /// Test seam: reset the private gesture recognizer so a synchronous test
    /// loop can drive successive wheel/click events without the
    /// throttle/double-click window collapsing them.
    #[cfg(test)]
    pub(crate) fn reset_mouse_gestures_for_test(&mut self) {
        self.mouse_gestures.reset_for_test();
    }
}

impl Default for FeedsComponent {
    fn default() -> Self {
        Self::new()
    }
}

impl Component for FeedsComponent {
    fn view(&mut self, frame: &mut Frame, area: Rect) {
        // One `ViewportAnchor` handoff at a breakpoint flip: carry the
        // outgoing control's selected target + screen-row offset into the
        // incoming control (design.md D2/D3). The cursors already track in
        // lockstep.
        let wide = crate::app::render::wide_hero_presentation(area).is_some();
        if wide != self.wide {
            let viewport_height = self.layout.left_area.height.max(1) as usize;
            let anchor: Option<ViewportAnchor<String>> = if self.wide {
                self.canonical_list.viewport_anchor(viewport_height)
            } else {
                self.inline_list.viewport_anchor(viewport_height)
            };
            if let Some(anchor) = anchor {
                if wide {
                    self.canonical_list
                        .apply_viewport_anchor(&anchor, viewport_height);
                } else {
                    self.inline_list
                        .apply_viewport_anchor(&anchor, viewport_height);
                }
            }
            self.wide = wide;
        }

        let mut layout = LayoutMain::default();
        let selected_entry = self.visible_entries.get(self.cursor()).cloned();
        let offset = render_feeds_content(
            frame,
            area,
            self.focused,
            &mut layout,
            FeedsRenderModel {
                subscriptions: &self.subscriptions,
                visible_entries: &self.visible_entries,
                watched_filter: self.watched_filter,
                selected_group: self.selected_group,
                loading: self.loading,
                selected_entry: selected_entry.as_ref(),
                images_enabled: self.images_enabled,
            },
            &mut self.canonical_list,
            &self.inline_list,
        );
        self.painted_offset = offset;
        self.layout = layout;
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

impl AppComponent<Msg, UserEvent> for FeedsComponent {
    fn on(&mut self, event: &Event<UserEvent>) -> Option<Msg> {
        match event {
            Event::Keyboard(key) => self.handle_key(key),
            Event::Mouse(mouse) => self.handle_mouse(mouse),
            _ => None,
        }
    }
}
