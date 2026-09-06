//! Keyboard handling for `MusicWorkspaceComponent`, split out of
//! `music_workspace.rs` to keep that file under the repository's file-size
//! ceiling. Pure code relocation: no behaviour change.

use tuirealm::event::{Key, KeyModifiers};

use super::inline_search::InlineSearchAction;
use super::msg::{AlbumCursorKind, Msg, ShellRequest};
use super::music_workspace::MusicWorkspaceComponent;
use crate::app::ui_util::move_cursor;

impl MusicWorkspaceComponent {
    fn move_album_rows(&mut self, rows: i64, columns: usize, wrap: bool) -> Option<usize> {
        let order = &self.context.album_order;
        if order.is_empty() {
            return None;
        }
        let position = order
            .iter()
            .position(|&index| index == self.album_cursor)
            .unwrap_or(0);
        let delta = rows.saturating_mul(columns.max(1) as i64);
        let target_position = if wrap {
            move_cursor(position, delta, order.len())
        } else if delta.is_negative() {
            position.saturating_sub(delta.unsigned_abs() as usize)
        } else {
            position
                .saturating_add(delta as usize)
                .min(order.len().saturating_sub(1))
        };
        self.album_cursor = order[target_position];
        Some(self.album_cursor)
    }

    fn can_emit_album_cursor(&self) -> bool {
        self.context.focused && self.track_cursor.is_none()
    }

    fn move_track(&mut self, delta: i64) {
        let count = self.context.album_tracks.as_ref().map_or(0, Vec::len);
        if count > 0 {
            self.track_cursor = Some(move_cursor(self.track_cursor.unwrap_or(0), delta, count));
            self.track_list.select_index(self.track_cursor.unwrap_or(0));
        }
    }

    /// Ctrl+P/S/A on the selected Inline Search result reuse the ordinary
    /// library result-row effects, resolved against the search cursor (result-
    /// row shortcut actions stay available while search is open).
    fn inline_search_result_action(&self, key: &tuirealm::event::KeyEvent) -> Option<Msg> {
        if !self.context.focused || !key.modifiers.contains(KeyModifiers::CONTROL) {
            return None;
        }
        let item = self.inline_search.selected_item()?;
        let request = match key.code {
            Key::Char('p') => ShellRequest::EmbyLibraryPlay { item },
            Key::Char('s') => ShellRequest::EmbyLibraryShuffle { item },
            Key::Char('a') => ShellRequest::EmbyLibraryEnqueue { item },
            _ => return None,
        };
        Some(Msg::Shell(request))
    }

    pub(super) fn handle_key(&mut self, key: &tuirealm::event::KeyEvent) -> Option<Msg> {
        // Inline Search gets first refusal while active (design.md D4): the
        // component returns immediately after delegating, even when search
        // consumes the key without producing a message.
        if self.inline_search.is_active() {
            return match self.inline_search.handle_key(key) {
                Some(InlineSearchAction::Activate { id, item_type }) => {
                    Some(Msg::Shell(ShellRequest::InlineSearchActivate {
                        id,
                        item_type,
                    }))
                }
                Some(InlineSearchAction::Dismiss) => {
                    // Escape/empty-query Backspace dismiss locally
                    // (design.md D4); no shell effect.
                    self.inline_search.close();
                    None
                }
                // Ctrl+P/S/A that the shared control does not consume act on
                // the selected result row via the ordinary result-row effects.
                None => self.inline_search_result_action(key),
            };
        }
        if !self.context.focused {
            return None;
        }
        match key.code {
            // Activation while an inline album track is focused: play the
            // focused track through the album queue path. The shell resolves
            // the track from `track_cursor()` (target resolution lives at
            // the shell/component boundary, not in `App`).
            Key::Enter if self.track_cursor.is_some() => {
                Some(Msg::Shell(ShellRequest::MusicTrackActivate))
            }
            // Ctrl+P keeps its "play current" meaning: with a focused track
            // that is the track, exactly like Enter.
            Key::Char('p')
                if key.modifiers.contains(KeyModifiers::CONTROL) && self.track_cursor.is_some() =>
            {
                Some(Msg::Shell(ShellRequest::MusicTrackActivate))
            }
            // Enter on an album row (Library panel): enter inline track
            // focus when wide with cached tracks; otherwise request the
            // narrow album activation effect from the shell.
            Key::Enter if self.track_cursor.is_none() => {
                if self.can_enter_track_focus() {
                    self.track_cursor = Some(0);
                    self.track_list.select_first();
                    return None;
                }
                Some(Msg::Shell(ShellRequest::MusicAlbumActivate))
            }
            // Exit inline track focus locally; the key must not reach the
            // unprefixed panel's Esc/Stop semantics.
            Key::Esc | Key::Backspace if self.track_cursor.is_some() => {
                self.track_cursor = None;
                self.track_list.select_first();
                None
            }
            // Track moves are local to the component while a track is
            // focused and the Library panel owns the keys; with the Queue
            // panel focused the keys are left unclaimed for the central
            // router.
            Key::Up | Key::Char('k') if self.track_cursor.is_some() && self.context.focused => {
                self.move_track(-1);
                None
            }
            Key::Down | Key::Char('j') if self.track_cursor.is_some() && self.context.focused => {
                self.move_track(1);
                None
            }
            // Enqueue / context menu target the focused track while one is
            // focused (Library panel); otherwise leave the key unhandled.
            Key::Char('a')
                if key.modifiers.contains(KeyModifiers::CONTROL)
                    && self.track_cursor.is_some()
                    && self.context.focused =>
            {
                Some(Msg::Shell(ShellRequest::MusicTrackEnqueue))
            }
            Key::Char('.') if self.track_cursor.is_some() && self.context.focused => {
                Some(Msg::Shell(ShellRequest::MusicTrackContextMenu))
            }
            // Album-level library actions apply only when no inline track is
            // focused; track Ctrl+P/Ctrl+A above retain precedence.
            Key::Char('p')
                if key.modifiers.contains(KeyModifiers::CONTROL) && self.track_cursor.is_none() =>
            {
                self.selected_item()
                    .map(|item| Msg::Shell(ShellRequest::EmbyLibraryPlay { item }))
            }
            Key::Char('a')
                if key.modifiers.contains(KeyModifiers::CONTROL) && self.track_cursor.is_none() =>
            {
                self.selected_item()
                    .map(|item| Msg::Shell(ShellRequest::EmbyLibraryEnqueue { item }))
            }
            Key::Char('w')
                if key.modifiers.contains(KeyModifiers::CONTROL) && self.track_cursor.is_none() =>
            {
                self.selected_item()
                    .map(|item| Msg::Shell(ShellRequest::EmbyLibraryToggleWatched { item }))
            }
            Key::Char('s')
                if key.modifiers.contains(KeyModifiers::CONTROL) && self.track_cursor.is_none() =>
            {
                self.selected_item()
                    .map(|item| Msg::Shell(ShellRequest::EmbyLibraryShuffle { item }))
            }
            Key::Char('r')
                if key.modifiers.contains(KeyModifiers::CONTROL) && self.track_cursor.is_none() =>
            {
                Some(Msg::Shell(ShellRequest::EmbyLibraryRescan))
            }
            Key::Char('r')
                if self.track_cursor.is_none()
                    && !key.modifiers.contains(KeyModifiers::CONTROL)
                    && !key.modifiers.contains(KeyModifiers::ALT) =>
            {
                Some(Msg::Shell(ShellRequest::EmbyLibraryRefresh))
            }
            Key::Char('.') if self.track_cursor.is_none() => {
                Some(Msg::Shell(ShellRequest::EmbyLibraryContextMenu {
                    item: self.selected_item()?,
                }))
            }
            Key::Char('/') if self.track_cursor.is_none() => {
                self.inline_search.open();
                Some(Msg::Shell(ShellRequest::OpenInlineSearch))
            }
            // `[`/`]` at the album-list level cycle the App-owned group pill.
            // A focused inline track is a track-level context, so guard on
            // `track_cursor.is_none()`; the focus gate is the early return.
            Key::Char('[')
                if self.track_cursor.is_none()
                    && !key.modifiers.contains(KeyModifiers::CONTROL)
                    && !key.modifiers.contains(KeyModifiers::ALT) =>
            {
                Some(Msg::Shell(ShellRequest::MusicGroupSwitch { delta: -1 }))
            }
            Key::Char(']')
                if self.track_cursor.is_none()
                    && !key.modifiers.contains(KeyModifiers::CONTROL)
                    && !key.modifiers.contains(KeyModifiers::ALT) =>
            {
                Some(Msg::Shell(ShellRequest::MusicGroupSwitch { delta: 1 }))
            }
            Key::Up | Key::Char('k') if self.can_emit_album_cursor() => {
                let target = self
                    .move_album_rows(-1, self.album_columns, true)
                    .unwrap_or(self.album_cursor);
                Some(Msg::Shell(ShellRequest::MusicAlbumCursor {
                    target,
                    kind: AlbumCursorKind::Move,
                }))
            }
            Key::Down | Key::Char('j') if self.can_emit_album_cursor() => {
                let target = self
                    .move_album_rows(1, self.album_columns, true)
                    .unwrap_or(self.album_cursor);
                Some(Msg::Shell(ShellRequest::MusicAlbumCursor {
                    target,
                    kind: AlbumCursorKind::Move,
                }))
            }
            Key::Home if self.can_emit_album_cursor() => {
                let target = self
                    .context
                    .album_order
                    .first()
                    .copied()
                    .unwrap_or(self.album_cursor);
                self.album_cursor = target;
                Some(Msg::Shell(ShellRequest::MusicAlbumCursor {
                    target,
                    kind: AlbumCursorKind::Jump,
                }))
            }
            Key::End if self.can_emit_album_cursor() => {
                let target = self
                    .context
                    .album_order
                    .last()
                    .copied()
                    .unwrap_or(self.album_cursor);
                self.album_cursor = target;
                Some(Msg::Shell(ShellRequest::MusicAlbumCursor {
                    target,
                    kind: AlbumCursorKind::Jump,
                }))
            }
            Key::PageUp if self.can_emit_album_cursor() => {
                let target = self
                    .move_album_rows(-(self.page_rows as i64), self.album_columns, false)
                    .unwrap_or(self.album_cursor);
                Some(Msg::Shell(ShellRequest::MusicAlbumCursor {
                    target,
                    kind: AlbumCursorKind::Page,
                }))
            }
            Key::PageDown if self.can_emit_album_cursor() => {
                let target = self
                    .move_album_rows(self.page_rows as i64, self.album_columns, false)
                    .unwrap_or(self.album_cursor);
                Some(Msg::Shell(ShellRequest::MusicAlbumCursor {
                    target,
                    kind: AlbumCursorKind::Page,
                }))
            }
            _ => None,
        }
    }
}
