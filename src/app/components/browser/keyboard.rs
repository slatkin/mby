use tuirealm::event::{Key, KeyEvent, KeyModifiers};

use super::BrowserComponent;
use crate::app::components::inline_search::InlineSearchAction;
use crate::app::components::msg::{Msg, ShellRequest};

impl BrowserComponent {
    pub(in crate::app) fn handle_tui_key(&mut self, key: KeyEvent) -> Option<Msg> {
        // Inline Search gets first refusal while active (design.md D4): the
        // component returns immediately after delegating, even when search
        // consumes the key without producing a message, so no printable
        // character or list shortcut reaches the ordinary handling below.
        if self.inline_search.is_active() {
            return match self.inline_search.handle_key(&key) {
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
                None => self.inline_search_result_action(&key),
            };
        }
        match key.code {
            Key::Char('/') if key.modifiers.is_empty() => {
                self.inline_search.open();
                return Some(Msg::Shell(ShellRequest::OpenInlineSearch));
            }
            _ => {}
        }
        if key.modifiers.contains(KeyModifiers::ALT)
            && matches!(key.code, Key::Left | Key::Right | Key::Up | Key::Down)
        {
            return None;
        }
        // Local keyboard navigation routes through typed `ShellRequest`s:
        // the component mutates only its own cursor, then returns the
        // request in place of the raw key so the shell drives the App cursor
        // through the same methods as the legacy `handle_lib_key` arms.
        // Unfocused browsers leave every chord untouched; the central router
        // handles destination-independent behavior.
        if self.focused {
            if matches!(
                key.code,
                Key::Up
                    | Key::Down
                    | Key::Char('j')
                    | Key::Char('k')
                    | Key::PageUp
                    | Key::PageDown
                    | Key::Home
                    | Key::End
                    | Key::Left
                    | Key::Right
                    | Key::Char('h')
                    | Key::Char('l')
            ) {
                self.pending_anchor = None;
                self.preserved_anchor = None;
            }
            match key.code {
                Key::Up | Key::Char('k') => {
                    let index = self.move_by_item_rows(-1);
                    return Some(Msg::Shell(ShellRequest::BrowserCursorIndex { index }));
                }
                Key::Down | Key::Char('j') => {
                    let index = self.move_by_item_rows(1);
                    return Some(Msg::Shell(ShellRequest::BrowserCursorIndex { index }));
                }
                Key::PageUp => {
                    let rows = -self.page_rows();
                    let index = self.move_by_item_rows(rows);
                    return Some(Msg::Shell(ShellRequest::BrowserCursorIndex { index }));
                }
                Key::PageDown => {
                    let rows = self.page_rows();
                    let index = self.move_by_item_rows(rows);
                    return Some(Msg::Shell(ShellRequest::BrowserCursorIndex { index }));
                }
                Key::Home => {
                    let index = self.jump_cursor(false);
                    return Some(Msg::Shell(ShellRequest::BrowserCursorIndex { index }));
                }
                Key::End => {
                    let index = self.jump_cursor(true);
                    return Some(Msg::Shell(ShellRequest::BrowserCursorIndex { index }));
                }
                // Column navigation applies only to a painted list with
                // more than one column. A one-column list leaves
                // Left/Right/h/l unbound locally.
                Key::Left | Key::Char('h') if self.columns() > 1 => {
                    let index = self.move_cursor_delta(-1);
                    return Some(Msg::Shell(ShellRequest::BrowserCursorIndex { index }));
                }
                Key::Right | Key::Char('l') if self.columns() > 1 => {
                    let index = self.move_cursor_delta(1);
                    return Some(Msg::Shell(ShellRequest::BrowserCursorIndex { index }));
                }
                _ => {}
            }
        }
        // The selected-item effects resolve targets from the component's own
        // local cursor/content and return typed requests. `focused` preserves
        // the legacy Library-panel gate exactly; an empty list or an
        // unclaimed chord returns `None` for the central router to handle.
        if self.focused {
            let selected = self.selected_effect_item();
            let request = match key.code {
                Key::Enter => selected.map(|item| ShellRequest::BrowserActivate { item }),
                Key::Char('p') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                    selected.map(|item| ShellRequest::BrowserPlay { item })
                }
                Key::Char('a') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                    selected.map(|item| ShellRequest::BrowserEnqueue { item })
                }
                Key::Char('w') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                    selected.map(|item| ShellRequest::BrowserToggleWatched { item })
                }
                // Bare `.` opens the context menu for the component-selected
                // item (task 5.3d, Emby browser context-menu decoupling).
                // Modified `.` (e.g. Ctrl+.) is not claimed here.
                Key::Char('.') if key.modifiers.is_empty() => {
                    selected.map(|item| ShellRequest::BrowserContextMenu { item })
                }
                // Ctrl+S shuffles the component-selected item. Control-
                // modifier guarded exactly as the legacy `handle_lib_key`
                // arm; with no selected item this chord remains unclaimed.
                Key::Char('s') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                    selected.map(|item| ShellRequest::BrowserShuffle { item })
                }
                // Ctrl+`r` rescans the focused library; bare `r` refreshes
                // it. The CONTROL arm comes first so it can never be shadowed
                // by the bare arm, preserving legacy precedence.
                Key::Char('r') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                    Some(ShellRequest::BrowserRescan)
                }
                Key::Char('r') => Some(ShellRequest::BrowserRefresh),
                // Esc or Backspace go back through the browse history (task
                // 5.3d, Emby browser back): uses a typed request for the
                // focused browser. No modifier guard — the legacy
                // `handle_lib_key` `Esc | Backspace` arm matched any
                // modifiers, so this preserves that modifier-insensitive
                // behavior exactly. The shell owns the effect (`go_back`) and
                // derives the active library index from its own tab state.
                Key::Esc | Key::Backspace => Some(ShellRequest::BrowserBack),
                // `[`/`]` cycle the letter-range pill row for the focused
                // generic/Movies/home-video browser. A typed request carries
                // the delta, and the shell derives the active Emby library
                // index from its own tab state.
                Key::Char(c @ ('[' | ']'))
                    if !key.modifiers.contains(KeyModifiers::CONTROL)
                        && !key.modifiers.contains(KeyModifiers::ALT) =>
                {
                    let delta = if c == '[' { -1 } else { 1 };
                    // The shell-projected content decides which pill row this
                    // chord drives: a feed/home-video group picker
                    // (`is_feed_home_video_group_view`, task 2.2) cycles its
                    // group pills; every other browse surface cycles its
                    // letter-range pills.
                    Some(if self.context.has_group_pills() {
                        ShellRequest::BrowserCycleGroup { delta }
                    } else {
                        ShellRequest::BrowserCycleLetterPill { delta }
                    })
                }
                _ => None,
            };
            // The component owns the selection: the item is resolved at the
            // component-local cursor in the mirrored content, never a re-read
            // of an App field.
            if let Some(request) = request {
                return Some(Msg::Shell(request));
            }
        }
        None
    }

    /// Ctrl+P/S/A on the selected Inline Search result reuse the ordinary
    /// result-row shell effects, resolved against the search cursor rather than
    /// the ordinary browse cursor (result-row shortcut actions stay available
    /// while search is open).
    fn inline_search_result_action(&self, key: &KeyEvent) -> Option<Msg> {
        if !self.focused || !key.modifiers.contains(KeyModifiers::CONTROL) {
            return None;
        }
        let item = self.inline_search.selected_item()?;
        let request = match key.code {
            Key::Char('p') => ShellRequest::BrowserPlay { item },
            Key::Char('s') => ShellRequest::BrowserShuffle { item },
            Key::Char('a') => ShellRequest::BrowserEnqueue { item },
            _ => return None,
        };
        Some(Msg::Shell(request))
    }

    /// Resolve the item at the component's own local cursor over the mirrored
    /// content. The local cursor is authoritative for effect targets; no App
    /// cursor is re-read.
    fn selected_effect_item(&self) -> Option<mbv_core::api::EmbyItem> {
        self.context
            .clone()
            .with_cursor_scroll(self.cursor, self.scroll)
            .selected_item()
            .cloned()
    }
}
