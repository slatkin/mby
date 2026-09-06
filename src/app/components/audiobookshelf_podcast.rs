//! Interactive Component for one Audiobookshelf podcast library.
//!
//! The shell mirrors validated browse content into this stable browser
//! instance. Show, episode, filter, and scroll state stays local here; typed
//! shell requests remain the shell-owned effect path.

use ratatui::layout::Rect;
use ratatui::Frame;
use tuirealm::command::{Cmd, CmdResult};
use tuirealm::component::{AppComponent, Component};
use tuirealm::event::{Event, Key, KeyEvent, KeyModifiers, MouseEvent, MouseEventKind};
use tuirealm::props::{AttrValue, Attribute, QueryResult};
use tuirealm::state::State;

use super::media_list::{InlineMediaBrowser, ViewportAnchor, WideMediaList};
use super::mouse::gesture::{MouseGesture, MouseGestureState};
use super::msg::{Msg, PodcastEpisodeIntent, PodcastEpisodeTransition, ShellRequest};
use super::user_event::UserEvent;
use crate::app::render::{
    render_audiobookshelf_podcast_content, wide_hero_presentation, AudiobookshelfPodcastGeometry,
    HomeImagePaint, PodcastInteraction,
};
use crate::app::types_audiobookshelf_browse::{
    AudiobookshelfBrowseState, AudiobookshelfEpisodeFilter,
};

pub struct AudiobookshelfPodcastComponent {
    state: AudiobookshelfBrowseState,
    /// Component-owned interaction state, never present on the projected
    /// content type (split-browse-state-interaction-fields task 3.2).
    episode_filter: AudiobookshelfEpisodeFilter,
    episode_selection: Option<usize>,
    scroll: usize,
    initialized: bool,
    focused: bool,
    images_enabled: bool,
    geometry: AudiobookshelfPodcastGeometry,
    image_paint: Option<HomeImagePaint>,
    /// Persistent narrow-presentation control, fed the canonical show-row
    /// projection by the renderer each frame. Never constructed during a
    /// render pass. The wide rail composes its own per-frame `WideMediaList`.
    narrow_list: InlineMediaBrowser<String>,
    /// Parent-owned wide episode list; its rows and viewport are projected
    /// from the selected, filtered episode snapshot during view.
    wide_episode_list: WideMediaList<String>,
    /// One-shot `ViewportAnchor` carried across a Wide<->Narrow breakpoint
    /// flip (§2.5); consumed by the next `view`.
    pending_anchor: Option<ViewportAnchor<String>>,
    /// The presentation the last `view` painted; `None` before the first paint.
    last_wide: Option<bool>,
    /// Selected-row screen offset captured each `view`, so `viewport_anchor`
    /// can report the outgoing control's anchor at a flip.
    painted_row_offset: Option<usize>,
    /// Private per-parent gesture recognition (ADR 0024, design.md D3): owns
    /// the double-click window and wheel throttle. Not a shared clock.
    mouse_gestures: MouseGestureState,
}

impl AudiobookshelfPodcastComponent {
    pub fn new() -> Self {
        Self {
            state: AudiobookshelfBrowseState::new(
                mbv_core::audiobookshelf::AudiobookshelfLibrary {
                    id: String::new(),
                    name: String::new(),
                    media_type: "podcast".into(),
                },
            ),
            episode_filter: AudiobookshelfEpisodeFilter::All,
            episode_selection: None,
            scroll: 0,
            initialized: false,
            focused: false,
            images_enabled: false,
            geometry: AudiobookshelfPodcastGeometry::default(),
            image_paint: None,
            narrow_list: InlineMediaBrowser::new(),
            wide_episode_list: WideMediaList::new(),
            pending_anchor: None,
            last_wide: None,
            painted_row_offset: None,
            mouse_gestures: MouseGestureState::new(),
        }
    }

    /// The outgoing control's `ViewportAnchor` for the last painted
    /// presentation (mirrors `MusicWorkspaceComponent::viewport_anchor`).
    fn viewport_anchor(&self) -> Option<ViewportAnchor<String>> {
        Some(ViewportAnchor {
            selected_target: self.state.selected_id.clone()?,
            selected_row_offset: self.painted_row_offset?,
        })
    }

    /// Test-only: drive framework focus the way `Component::attr` does.
    #[cfg(test)]
    pub(in crate::app) fn set_focused(&mut self, focused: bool) {
        self.focused = focused;
    }

    pub(in crate::app) fn set_content(
        &mut self,
        snapshot: &AudiobookshelfBrowseState,
        images_enabled: bool,
    ) {
        // Content and interaction are separate types now: the projected
        // snapshot carries no `episode_filter` / `episode_selection` /
        // `scroll`, so adopting it wholesale cannot clobber them and there is
        // nothing to save and restore (split-browse-state-interaction-fields
        // task 3.4). Whether the show the component had selected survived the
        // new content decides if its derived local state still means anything.
        let survived = self.initialized
            && self.state.selected_id.as_ref().is_some_and(|prior| {
                snapshot
                    .shows
                    .iter()
                    .any(|show| &show.library_item_id == prior)
            });
        self.state = snapshot.clone();
        if self.initialized && !survived {
            // The selected show dropped out of the new content: reset the
            // component-owned interaction state rather than carrying it.
            self.episode_filter = AudiobookshelfEpisodeFilter::All;
            self.episode_selection = None;
            self.scroll = 0;
        }
        self.initialized = true;
        self.images_enabled = images_enabled;
    }

    pub(in crate::app) fn cursor(&self) -> usize {
        self.state.cursor()
    }

    /// Re-home accessors (task 5.3d.11 U0): owned/copy views of the shared
    /// `AudiobookshelfBrowseState` members the App-level readers read. The
    /// state struct is shared with `App.audiobookshelf_browse`, so these let
    /// the shell read the component's authoritative selection without touching
    /// the legacy App readers.
    pub(in crate::app) fn selected_id(&self) -> Option<String> {
        self.state.selected_id.clone()
    }

    pub(in crate::app) fn episode_selection(&self) -> Option<usize> {
        self.episode_selection
    }

    pub(in crate::app) fn episode_filter(&self) -> AudiobookshelfEpisodeFilter {
        self.episode_filter
    }

    /// Sets the component-owned episode filter, keeping the in-progress
    /// episode selection (if any) valid by re-homing it to row `0` -- the
    /// reset semantics `AudiobookshelfBrowseState::set_episode_filter` used to
    /// carry (split-browse-state-interaction-fields task 3.2).
    pub(in crate::app) fn set_episode_filter(&mut self, filter: AudiobookshelfEpisodeFilter) {
        self.episode_filter = filter;
        if self.episode_selection.is_some() {
            self.episode_selection = Some(0);
        }
    }

    pub(in crate::app) fn set_episode_selection(&mut self, selection: Option<usize>) {
        self.episode_selection = selection;
    }

    /// Moves the show cursor to `cursor`, resetting the component-owned
    /// episode filter / episode-mode selection when that changes the selected
    /// show -- the identity-change reset that lived in
    /// `AudiobookshelfBrowseState::select` before task 3.2.
    fn select_show(&mut self, cursor: usize) {
        if self.state.select_changed_identity(cursor) {
            self.episode_filter = AudiobookshelfEpisodeFilter::All;
        }
        self.episode_selection = None;
        self.state.select(cursor);
    }

    /// The image-paint plan this component computed during its last `view`
    /// (task 5.3d.10b): `Some` only when images are enabled, a selected show
    /// hero was actually admitted/painted, and the hero reserved an image
    /// rect. Replaced on every `view`, taken once by the shell after paint.
    pub(in crate::app) fn take_image_paint(&mut self) -> Option<HomeImagePaint> {
        self.image_paint.take()
    }

    /// The geometry the component computed during its last `view`, exposed so
    /// the shell can anchor overlays / read painted areas (task 5.3d.10c,
    /// render ownership). Immutable: the component owns painting; callers do
    /// not write back.
    pub(in crate::app) fn geometry(&self) -> &AudiobookshelfPodcastGeometry {
        &self.geometry
    }

    fn move_cursor(&mut self, delta: i64) {
        let cursor = self.state.cursor();
        let count = self.state.shows.len();
        if count == 0 {
            return;
        }
        let next = crate::app::ui_util::move_cursor(cursor, delta, count);
        self.select_show(next);
    }

    fn cycle_show_bucket(&mut self, delta: i64) {
        let buckets =
            crate::app::types_audiobookshelf_browse::build_show_title_buckets(&self.state.shows);
        if buckets.is_empty() {
            return;
        }
        let current = buckets
            .iter()
            .position(|bucket| {
                self.state.cursor() >= bucket.start && self.state.cursor() < bucket.end
            })
            .unwrap_or(0);
        let next = (current as i64 + delta).rem_euclid(buckets.len() as i64) as usize;
        if let Some(bucket) = buckets.get(next) {
            self.select_show(bucket.start);
        }
    }

    /// The resolved-index show-move request for the cursor the component just
    /// landed on (split-audiobookshelf-cursor-ownership D1). Every show-list
    /// key resolves its own movement locally and carries only the result.
    fn show_move_request(&self) -> Msg {
        Msg::Shell(ShellRequest::AudiobookshelfPodcastShowMove {
            index: self.state.cursor(),
        })
    }

    fn handle_key(&mut self, key: &KeyEvent) -> Option<Msg> {
        if !self.focused {
            return None;
        }
        match key.code {
            Key::Up | Key::Char('k') if self.episode_selection.is_none() => {
                self.move_cursor(-(self.geometry.columns.max(1) as i64));
                Some(self.show_move_request())
            }
            Key::Down | Key::Char('j') if self.episode_selection.is_none() => {
                self.move_cursor(self.geometry.columns.max(1) as i64);
                Some(self.show_move_request())
            }
            Key::Left | Key::Char('h') if self.episode_selection.is_none() => {
                self.move_cursor(-1);
                Some(self.show_move_request())
            }
            Key::Right | Key::Char('l') if self.episode_selection.is_none() => {
                self.move_cursor(1);
                Some(self.show_move_request())
            }
            Key::PageUp if self.episode_selection.is_none() => {
                let page_rows = self.geometry.list_area.height.saturating_sub(1).max(1) as usize;
                self.move_cursor(-((page_rows * self.geometry.columns.max(1)) as i64));
                Some(self.show_move_request())
            }
            Key::PageDown if self.episode_selection.is_none() => {
                let page_rows = self.geometry.list_area.height.saturating_sub(1).max(1) as usize;
                self.move_cursor((page_rows * self.geometry.columns.max(1)) as i64);
                Some(self.show_move_request())
            }
            Key::Home if self.episode_selection.is_none() => {
                self.select_show(0);
                Some(self.show_move_request())
            }
            Key::End if self.episode_selection.is_none() => {
                self.select_show(self.state.shows.len().saturating_sub(1));
                Some(self.show_move_request())
            }
            Key::Char('[') if self.episode_selection.is_none() && key.modifiers.is_empty() => {
                self.cycle_show_bucket(-1);
                Some(self.show_move_request())
            }
            Key::Char(']') if self.episode_selection.is_none() && key.modifiers.is_empty() => {
                self.cycle_show_bucket(1);
                Some(self.show_move_request())
            }
            Key::Up | Key::Char('k') => {
                self.move_episode(-1);
                Some(Msg::Shell(
                    ShellRequest::AudiobookshelfPodcastEpisodeTransition(
                        PodcastEpisodeTransition::PreviousEpisode,
                    ),
                ))
            }
            Key::Down | Key::Char('j') => {
                self.move_episode(1);
                Some(Msg::Shell(
                    ShellRequest::AudiobookshelfPodcastEpisodeTransition(
                        PodcastEpisodeTransition::NextEpisode,
                    ),
                ))
            }
            Key::Char('[') if self.episode_selection.is_some() && key.modifiers.is_empty() => {
                self.cycle_filter(-1);
                Some(Msg::Shell(
                    ShellRequest::AudiobookshelfPodcastEpisodeTransition(
                        PodcastEpisodeTransition::PreviousFilter,
                    ),
                ))
            }
            Key::Char(']') if self.episode_selection.is_some() && key.modifiers.is_empty() => {
                self.cycle_filter(1);
                Some(Msg::Shell(
                    ShellRequest::AudiobookshelfPodcastEpisodeTransition(
                        PodcastEpisodeTransition::NextFilter,
                    ),
                ))
            }
            Key::Esc | Key::Backspace if self.episode_selection.is_some() => {
                self.episode_selection = None;
                Some(Msg::Shell(
                    ShellRequest::AudiobookshelfPodcastEpisodeTransition(
                        PodcastEpisodeTransition::Exit,
                    ),
                ))
            }
            // Space/Enter/Ctrl+A action intents (task 5.3d.7): the component
            // only reports the matched intent; the shell resolves the
            // episode-selection and wide/narrow conditions from App state at
            // the Model boundary and runs the existing App effect (D17).
            Key::Char(' ') => Some(Msg::Shell(
                ShellRequest::AudiobookshelfPodcastEpisodeIntent(PodcastEpisodeIntent::FocusOrPlay),
            )),
            Key::Enter => Some(Msg::Shell(
                ShellRequest::AudiobookshelfPodcastEpisodeIntent(PodcastEpisodeIntent::OpenOrPlay),
            )),
            Key::Char('a') if key.modifiers.contains(KeyModifiers::CONTROL) => Some(Msg::Shell(
                ShellRequest::AudiobookshelfPodcastEpisodeIntent(PodcastEpisodeIntent::Enqueue),
            )),
            _ => None,
        }
    }

    fn move_episode(&mut self, delta: i64) {
        let count = self.state.visible_episodes(self.episode_filter).len();
        if count == 0 {
            return;
        }
        let current = self.episode_selection.unwrap_or(0);
        self.episode_selection = Some(crate::app::ui_util::move_cursor(current, delta, count));
    }

    fn cycle_filter(&mut self, delta: i64) {
        let current = AudiobookshelfEpisodeFilter::ALL
            .iter()
            .position(|filter| *filter == self.episode_filter)
            .unwrap_or(0);
        let next = crate::app::ui_util::move_cursor(
            current,
            delta,
            AudiobookshelfEpisodeFilter::ALL.len(),
        );
        self.set_episode_filter(AudiobookshelfEpisodeFilter::ALL[next]);
    }

    /// Handle a TuiRealm mouse event via the private `MouseGestureState`
    /// (ADR 0024, design.md D3): the state also owns the wheel throttle that
    /// the legacy raw wheel arm lacked. Show-row identity comes from the
    /// painted `show_rows` rects — the wide rail is composed per-frame in the
    /// renderer and is not a persistent control, so `resolve_point` does not
    /// apply there (task 4.1). Effects reuse the existing move/intent Msgs
    /// (task 4.5). Podcast has no keyboard context-menu action (task 4.6),
    /// so right-click is ignored.
    fn handle_mouse(&mut self, mouse: &MouseEvent) -> Option<Msg> {
        // The podcast surface does not consume hover-move (design.md D7).
        if matches!(mouse.kind, MouseEventKind::Moved) {
            return None;
        }
        match self.mouse_gestures.recognize(mouse)? {
            MouseGesture::Scroll { at, delta } => {
                if self.episode_selection.is_some() || !self.geometry.list_area.contains(at) {
                    return None;
                }
                let columns = self.geometry.columns.max(1) as i64;
                self.move_cursor(delta * 3 * columns);
                Some(self.show_move_request())
            }
            MouseGesture::Click(at) => {
                if let Some((_, index)) = self
                    .geometry
                    .show_rows
                    .iter()
                    .find(|(rect, _)| rect.contains(at))
                {
                    self.select_show(*index);
                    return Some(self.show_move_request());
                }
                if let Some((_, bucket)) = self
                    .geometry
                    .selector_tabs
                    .iter()
                    .find(|(rect, _)| rect.contains(at))
                {
                    if let Some(range) =
                        crate::app::types_audiobookshelf_browse::build_show_title_buckets(
                            &self.state.shows,
                        )
                        .get(*bucket)
                    {
                        self.select_show(range.start);
                        return Some(self.show_move_request());
                    }
                    return None;
                }
                None
            }
            MouseGesture::DoubleClick(at) => {
                if let Some((_, index)) = self
                    .geometry
                    .show_rows
                    .iter()
                    .find(|(rect, _)| rect.contains(at))
                {
                    self.select_show(*index);
                    return Some(Msg::Shell(
                        ShellRequest::AudiobookshelfPodcastEpisodeIntent(
                            PodcastEpisodeIntent::OpenOrPlay,
                        ),
                    ));
                }
                None
            }
            _ => None,
        }
    }

    /// Test seam: reset the private gesture recognizer so a synchronous test
    /// loop can drive successive wheel/click events without the
    /// throttle/double-click window collapsing them.
    #[cfg(test)]
    pub(crate) fn reset_mouse_gestures_for_test(&mut self) {
        self.mouse_gestures.reset_for_test();
    }
}

impl Default for AudiobookshelfPodcastComponent {
    fn default() -> Self {
        Self::new()
    }
}

impl Component for AudiobookshelfPodcastComponent {
    fn view(&mut self, frame: &mut Frame, area: Rect) {
        let wide = wide_hero_presentation(area).is_some();
        // §2.5: at a breakpoint flip carry the outgoing control's anchor into
        // the incoming one so the selected show keeps its screen-row offset.
        if let Some(was_wide) = self.last_wide {
            if was_wide != wide && self.pending_anchor.is_none() {
                self.pending_anchor = self.viewport_anchor();
            }
        }
        let flip_anchor = self.pending_anchor.take();
        if let Some(anchor) = &flip_anchor {
            if let Some(idx) = self
                .state
                .shows
                .iter()
                .position(|show| show.library_item_id == anchor.selected_target)
            {
                self.state.select(idx);
            }
        }

        self.image_paint = render_audiobookshelf_podcast_content(
            frame,
            area,
            self.focused,
            self.images_enabled,
            &mut self.state,
            PodcastInteraction {
                episode_filter: self.episode_filter,
                episode_selection: self.episode_selection,
            },
            &mut self.scroll,
            &mut self.narrow_list,
            &mut self.wide_episode_list,
            flip_anchor.as_ref(),
            &mut self.geometry,
        );

        self.painted_row_offset = self.geometry.selected_row_offset;
        self.last_wide = Some(wide);
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

impl AppComponent<Msg, UserEvent> for AudiobookshelfPodcastComponent {
    fn on(&mut self, event: &Event<UserEvent>) -> Option<Msg> {
        match event {
            Event::Keyboard(key) => self.handle_key(key),
            Event::Mouse(mouse) => self.handle_mouse(mouse),
            _ => None,
        }
    }
}
