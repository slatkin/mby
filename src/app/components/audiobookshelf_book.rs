use ratatui::layout::Rect;
use ratatui::Frame;
use tuirealm::command::{Cmd, CmdResult};
use tuirealm::component::{AppComponent, Component};
use tuirealm::event::{Event, Key, KeyEvent, KeyModifiers, MouseEvent, MouseEventKind};
use tuirealm::props::{AttrValue, Attribute, QueryResult};
use tuirealm::state::State;

use super::media_list::{InlineMediaBrowser, ViewportAnchor, WideMediaList};
use super::mouse::gesture::{MouseGesture, MouseGestureState};
use super::msg::{AudiobookshelfBookIntent, AudiobookshelfBookMove, Msg, ShellRequest};
use super::user_event::UserEvent;
use crate::app::render::{
    render_audiobookshelf_book_content, wide_hero_presentation, AudiobookshelfBookGeometry,
    BookInteraction, HomeImagePaint,
};
use crate::app::types_audiobookshelf_browse::AudiobookshelfBookBrowseState;

pub struct AudiobookshelfBookComponent {
    state: AudiobookshelfBookBrowseState,
    /// `false` until the first `set_content`: the initial projection adopts
    /// the shell snapshot wholesale; only later pushes reset stale
    /// component-owned fields (split-audiobookshelf-cursor-ownership D4).
    initialized: bool,
    /// Component-owned interaction state, never present on the projected
    /// content type (split-browse-state-interaction-fields task 2.2).
    chapter_selection: Option<usize>,
    selected_bucket: usize,
    browser_offset: usize,
    focused: bool,
    images_enabled: bool,
    geometry: AudiobookshelfBookGeometry,
    /// Whether the last rendered presentation actually exposes chapter focus.
    /// Narrow layouts may retain chapter state across a projection, so input
    /// must follow the rendered wide/chapter geometry rather than that state.
    chapters_visible: bool,
    image_paint: Option<HomeImagePaint>,
    /// Persistent narrow-presentation control, fed the canonical book-row
    /// projection by the renderer each frame. Never constructed during a
    /// render pass. The wide rail composes its own per-frame `WideMediaList`.
    narrow_list: InlineMediaBrowser<String>,
    /// Parent-owned chapter control for the wide hero workspace.
    chapter_list: WideMediaList<String>,
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

impl AudiobookshelfBookComponent {
    pub fn new() -> Self {
        Self {
            state: AudiobookshelfBookBrowseState::new(
                mbv_core::audiobookshelf::AudiobookshelfLibrary {
                    id: String::new(),
                    name: String::new(),
                    media_type: "book".into(),
                },
            ),
            initialized: false,
            chapter_selection: None,
            selected_bucket: 0,
            browser_offset: 0,
            focused: false,
            images_enabled: false,
            geometry: AudiobookshelfBookGeometry::default(),
            chapters_visible: false,
            image_paint: None,
            narrow_list: InlineMediaBrowser::new(),
            chapter_list: WideMediaList::new(),
            pending_anchor: None,
            last_wide: None,
            painted_row_offset: None,
            mouse_gestures: MouseGestureState::new(),
        }
    }

    /// The outgoing control's `ViewportAnchor` for the last painted
    /// presentation (mirrors `AudiobookshelfPodcastComponent::viewport_anchor`).
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
        snapshot: &AudiobookshelfBookBrowseState,
        images_enabled: bool,
    ) {
        // Content and interaction are separate types now: the projected
        // snapshot carries no `chapter_selection` / `selected_bucket`, so
        // adopting it wholesale cannot clobber them and there is nothing to
        // save and restore (split-browse-state-interaction-fields task 2.2).
        // Whether the book the component was showing survived the new content
        // decides if its derived local state still means anything.
        let survived = self.initialized
            && self.state.selected_id.as_ref().is_some_and(|prior| {
                snapshot
                    .books
                    .iter()
                    .any(|book| &book.library_item_id == prior)
            });
        self.state = snapshot.clone();
        if self.initialized && !survived {
            // The selected book dropped out of the new content: reset the
            // derived local state rather than adopting anything.
            self.chapter_selection = None;
            self.browser_offset = 0;
        }
        // Re-anchor the surname-bucket pill onto the selected book
        // (book-browsing spec: refresh/paging preserves the selected book
        // regardless of its new bucket).
        let cursor = self.state.cursor();
        if let Some(pos) = self
            .state
            .buckets
            .iter()
            .position(|bucket| cursor >= bucket.start && cursor < bucket.end)
        {
            self.selected_bucket = pos;
        }
        self.selected_bucket = self
            .selected_bucket
            .min(self.state.buckets.len().saturating_sub(1));
        self.initialized = true;
        self.images_enabled = images_enabled;
    }

    pub(in crate::app) fn take_image_paint(&mut self) -> Option<HomeImagePaint> {
        self.image_paint.take()
    }

    /// The geometry the component computed during its last `view`, exposed so
    /// the shell can anchor the context menu (task 5.3d.13, render ownership).
    pub(in crate::app) fn geometry(&self) -> &AudiobookshelfBookGeometry {
        &self.geometry
    }

    pub(in crate::app) fn chapter_selection(&self) -> Option<usize> {
        self.chapter_selection
    }

    #[cfg(test)]
    pub(crate) fn selected_book_id(&self) -> Option<&str> {
        self.state.selected_id.as_deref()
    }

    #[cfg(test)]
    pub(crate) fn selected_bucket(&self) -> usize {
        self.selected_bucket
    }

    /// The page stride from the component's own painted geometry
    /// (split-audiobookshelf-cursor-ownership D1): the list/content area's
    /// height minus its header line — the same value `App::lib_page_size()`
    /// derived from the projected `left_area`, now sourced locally so the
    /// shell applies no competing stride.
    fn page_size(&self) -> usize {
        (self.geometry.left_area.height as usize)
            .saturating_sub(1)
            .max(1)
    }

    fn book_request(&self) -> Msg {
        Msg::Shell(ShellRequest::AudiobookshelfBookMove(
            AudiobookshelfBookMove::Book(self.state.cursor()),
        ))
    }

    fn bucket_request(&self) -> Msg {
        Msg::Shell(ShellRequest::AudiobookshelfBookMove(
            AudiobookshelfBookMove::Bucket(self.selected_bucket),
        ))
    }

    fn chapter_focus_request(&self) -> Msg {
        Msg::Shell(ShellRequest::AudiobookshelfBookMove(
            AudiobookshelfBookMove::ChapterFocus(self.chapter_selection),
        ))
    }

    fn move_book(&mut self, delta: i64) {
        let Some(bucket) = self.state.buckets.get(self.selected_bucket) else {
            return;
        };
        if bucket.end <= bucket.start {
            return;
        }
        let cursor = (self.state.cursor() as i64).clamp(bucket.start as i64, bucket.end as i64 - 1);
        self.state
            .select((cursor + delta).clamp(bucket.start as i64, bucket.end as i64 - 1) as usize);
    }

    fn move_chapter(&mut self, delta: i64) {
        let Some(id) = self.state.selected_id.as_deref() else {
            return;
        };
        let count = self.state.visible_rows(id).len();
        if count > 0 {
            self.chapter_selection = Some(crate::app::ui_util::move_cursor(
                self.chapter_selection.unwrap_or(0),
                delta,
                count,
            ));
        }
    }

    fn handle_key(&mut self, key: &KeyEvent) -> Option<Msg> {
        if !self.focused {
            return None;
        }

        let chapters_focused = self.chapters_visible && self.chapter_selection.is_some();
        match key.code {
            Key::Char('[') if key.modifiers.is_empty() => {
                self.cycle_bucket(-1);
                Some(self.bucket_request())
            }
            Key::Char(']') if key.modifiers.is_empty() => {
                self.cycle_bucket(1);
                Some(self.bucket_request())
            }
            Key::Up | Key::Char('k') if chapters_focused => {
                self.move_chapter(-1);
                Some(self.chapter_focus_request())
            }
            Key::Down | Key::Char('j') if chapters_focused => {
                self.move_chapter(1);
                Some(self.chapter_focus_request())
            }
            Key::Right if chapters_focused => {
                self.chapter_selection = None;
                Some(self.chapter_focus_request())
            }
            Key::Left if self.chapters_visible && !chapters_focused => {
                self.chapter_selection = Some(0);
                Some(self.chapter_focus_request())
            }
            Key::Up | Key::Char('k') => {
                self.move_book(-1);
                Some(self.book_request())
            }
            Key::Down | Key::Char('j') => {
                self.move_book(1);
                Some(self.book_request())
            }
            Key::PageUp if !chapters_focused => {
                self.move_book(-(self.page_size() as i64));
                Some(self.book_request())
            }
            Key::PageDown if !chapters_focused => {
                self.move_book(self.page_size() as i64);
                Some(self.book_request())
            }
            Key::Home if !chapters_focused => {
                self.select_bucket_edge(false);
                Some(self.book_request())
            }
            Key::End if !chapters_focused => {
                self.select_bucket_edge(true);
                Some(self.book_request())
            }
            Key::Esc | Key::Backspace if chapters_focused => {
                self.chapter_selection = None;
                Some(self.chapter_focus_request())
            }
            Key::Char(' ') if chapters_focused => Some(Msg::Shell(
                ShellRequest::AudiobookshelfBookIntent(AudiobookshelfBookIntent::ActivateChapter),
            )),
            Key::Enter if chapters_focused => Some(Msg::Shell(
                ShellRequest::AudiobookshelfBookIntent(AudiobookshelfBookIntent::ActivateChapter),
            )),
            Key::Char(' ') => Some(Msg::Shell(ShellRequest::AudiobookshelfBookIntent(
                AudiobookshelfBookIntent::Play,
            ))),
            Key::Enter => Some(Msg::Shell(ShellRequest::AudiobookshelfBookIntent(
                AudiobookshelfBookIntent::Activate,
            ))),
            Key::Char('a')
                if !chapters_focused && key.modifiers.contains(KeyModifiers::CONTROL) =>
            {
                Some(Msg::Shell(ShellRequest::AudiobookshelfBookIntent(
                    AudiobookshelfBookIntent::Enqueue,
                )))
            }
            _ => None,
        }
    }

    fn cycle_bucket(&mut self, delta: i64) {
        let count = self.state.buckets.len();
        if count > 0 {
            self.selected_bucket =
                (self.selected_bucket as i64 + delta).rem_euclid(count as i64) as usize;
            if let Some(bucket) = self.state.buckets.get(self.selected_bucket).copied() {
                if !(bucket.start..bucket.end).contains(&self.state.cursor()) {
                    self.state.select(bucket.start);
                }
            }
        }
    }

    fn select_bucket_edge(&mut self, end: bool) {
        if let Some(bucket) = self.state.buckets.get(self.selected_bucket).copied() {
            if bucket.end > bucket.start {
                self.state
                    .select(if end { bucket.end - 1 } else { bucket.start });
            }
        }
    }

    /// Handle a TuiRealm mouse event via the private `MouseGestureState`
    /// (ADR 0024, design.md D3). Row identity comes from the painted row
    /// rects (`book_rows`/`chapter_rows`) — the wide rail is composed
    /// per-frame in the renderer and is not a persistent control, so
    /// `resolve_point` does not apply there (task 4.1). Effects reuse the
    /// existing move/intent Msgs (task 4.5). Book has no keyboard
    /// context-menu action (task 4.6), so right-click is ignored.
    fn handle_mouse(&mut self, mouse: &MouseEvent) -> Option<Msg> {
        // The book surface does not consume hover-move (design.md D7).
        if matches!(mouse.kind, MouseEventKind::Moved) {
            return None;
        }
        match self.mouse_gestures.recognize(mouse)? {
            MouseGesture::Click(at) => {
                if let Some((_, bucket)) = self
                    .geometry
                    .selector_tabs
                    .iter()
                    .find(|(rect, _)| rect.contains(at))
                {
                    if let Some(range) = self.state.buckets.get(*bucket).copied() {
                        self.selected_bucket = *bucket;
                        self.state.select(range.start);
                    }
                    return Some(self.bucket_request());
                }
                if let Some((_, index)) = self
                    .geometry
                    .book_rows
                    .iter()
                    .find(|(rect, _)| rect.contains(at))
                {
                    self.state.select(*index);
                    return Some(self.book_request());
                }
                if self.chapters_visible {
                    if let Some((_, index)) = self
                        .geometry
                        .chapter_rows
                        .iter()
                        .find(|(rect, _)| rect.contains(at))
                    {
                        self.chapter_selection = Some(*index);
                        return Some(self.chapter_focus_request());
                    }
                }
                None
            }
            MouseGesture::DoubleClick(at) => {
                if let Some((_, index)) = self
                    .geometry
                    .book_rows
                    .iter()
                    .find(|(rect, _)| rect.contains(at))
                {
                    self.state.select(*index);
                    return Some(Msg::Shell(ShellRequest::AudiobookshelfBookIntent(
                        AudiobookshelfBookIntent::Activate,
                    )));
                }
                None
            }
            MouseGesture::Scroll { at, delta } => {
                // Page-size move + re-request (Home/Queue precedent, task
                // 4.1); this surface had no wheel arm before. Mirrors the
                // keyboard PageUp/PageDown focus split.
                if !self.geometry.left_area.contains(at) {
                    return None;
                }
                let rows = delta * self.page_size() as i64;
                if self.chapters_visible && self.chapter_selection.is_some() {
                    self.move_chapter(rows);
                    return Some(self.chapter_focus_request());
                }
                self.move_book(rows);
                Some(self.book_request())
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

impl Default for AudiobookshelfBookComponent {
    fn default() -> Self {
        Self::new()
    }
}

impl Component for AudiobookshelfBookComponent {
    fn view(&mut self, frame: &mut Frame, area: Rect) {
        // Chapter focus belongs only to the rendered wide Wide hero
        // presentation. Clear it before painting a narrow frame so a
        // wide→narrow resize cannot leave keyboard input targeting a hidden
        // chapter pane.
        let wide = wide_hero_presentation(area).is_some();
        self.chapters_visible = wide;
        if !self.chapters_visible {
            self.chapter_selection = None;
        }
        // §2.5: at a breakpoint flip carry the outgoing control's anchor into
        // the incoming one so the selected book keeps its screen-row offset.
        if let Some(was_wide) = self.last_wide {
            if was_wide != wide && self.pending_anchor.is_none() {
                self.pending_anchor = self.viewport_anchor();
            }
        }
        let flip_anchor = self.pending_anchor.take();
        if let Some(anchor) = &flip_anchor {
            if let Some(idx) = self
                .state
                .books
                .iter()
                .position(|book| book.library_item_id == anchor.selected_target)
            {
                self.state.select(idx);
            }
        }
        self.image_paint = render_audiobookshelf_book_content(
            frame,
            area,
            self.focused,
            &mut self.state,
            BookInteraction {
                chapter_selection: self.chapter_selection,
                selected_bucket: self.selected_bucket,
            },
            self.images_enabled,
            &mut self.geometry,
            &mut self.browser_offset,
            &mut self.narrow_list,
            &mut self.chapter_list,
            flip_anchor.as_ref(),
        );
        self.painted_row_offset = self.geometry.selected_row_offset;
        self.last_wide = Some(wide);
        // A wide frame can still have no painted chapter rows (for example
        // while detail is loading or when the selected book has no chapters).
        // Do not advertise focus for geometry that was not rendered.
        if self.geometry.chapter_rows.is_empty() {
            self.chapters_visible = false;
            self.chapter_selection = None;
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

impl AppComponent<Msg, UserEvent> for AudiobookshelfBookComponent {
    fn on(&mut self, event: &Event<UserEvent>) -> Option<Msg> {
        match event {
            Event::Keyboard(key) => self.handle_key(key),
            Event::Mouse(mouse) => self.handle_mouse(mouse),
            _ => None,
        }
    }
}
