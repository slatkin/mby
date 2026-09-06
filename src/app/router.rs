//! One Central Keyboard Router (ADR 0023).
//!
//! `UiRoot` is the single keyboard routing authority. This module resolves a
//! chord against the ordered policy and returns ADR 0002's three outcomes —
//! `Command` (run this semantic command, discard the focused leaf's message),
//! `Swallow` (run nothing, discard the leaf's message), or `FallThrough` (the
//! leaf's own typed request stands).

use crossterm::event::KeyEvent;

use super::action::Command;
use super::components::ComponentId;
use super::input_resolver::KeyChord;
use super::key_policy::{command_for_policy, resolve_policy, KeyPolicyBinding};

pub(super) use super::key_policy::RouterSnapshot;

/// ADR 0002's three routing outcomes, exactly. `Application::tick` returns the
/// focused component's message before subscribers'; the router's outcome
/// selects between running the leaf's request and discarding it.
#[derive(Debug, Clone, PartialEq)]
pub(super) enum RouterOutcome {
    /// Run this semantic command and discard the leaf's message for this tick.
    Command(Command),
    /// Run nothing and discard the leaf's message for this tick.
    Swallow,
    /// The leaf's message stands (if it produced one).
    FallThrough,
}

/// Resolve a chord against the live ordered policy. A matched command is
/// dispatched by the shell; blocking and catch-all layers swallow the leaf.
///
/// The two-argument form treats every chord as occurring with no text entry
/// focused. Production routing goes through
/// `resolve_router_outcome_with_focused`, which carries the focused leaf so
/// the policy can tell "the leaf is the blocking overlay" from "an overlay is
/// mounted elsewhere".
/// Resolve a chord against the live ordered policy, carrying the focused
/// leaf so the policy can apply the two text-entry/overlay rules:
///
/// 1. **Never swallow the focused leaf's own typed request.** When the
///    policy would return `Swallow` and the focused leaf is the blocking
///    overlay (`snapshot.blocking_overlay_open` is true and the focused id
///    is one of the blocking-overlay `ComponentId`s), return `FallThrough`
///    so the leaf's request stands.
/// 2. **Text entry keeps ordinary characters.** When the policy matches a
///    global binding and `snapshot.text_entry_focused` is true, return
///    `FallThrough` instead of `Command` so the leaf's character input stands.
///    The F1-F4 sidebar bindings remain router-owned so sidebars switch
///    directly even while Settings or Search text entry is focused.
///
/// The `blocking_overlay_open` catch-all rules stay: they still discard the
/// focused leaf's message when no overlay is mounted.
pub(super) fn resolve_router_outcome_with_focused(
    key: KeyEvent,
    snapshot: &RouterSnapshot,
    focused: Option<&ComponentId>,
) -> RouterOutcome {
    let chord = KeyChord::from_key(key);
    let focused_is_blocking_overlay =
        snapshot.blocking_overlay_open && focused.is_some_and(is_blocking_overlay);
    match resolve_policy(chord, snapshot) {
        Some(entry) if entry.blocking && entry.name == "selection_modal" => {
            if focused_is_blocking_overlay {
                RouterOutcome::FallThrough
            } else {
                RouterOutcome::Swallow
            }
        }
        Some(entry) if entry.blocking => RouterOutcome::Swallow,
        Some(entry) => {
            if snapshot.text_entry_focused
                && entry.global
                && !matches!(
                    entry.binding,
                    KeyPolicyBinding::SettingsOpen
                        | KeyPolicyBinding::SessionsOpen
                        | KeyPolicyBinding::PlaylistsOpen
                        | KeyPolicyBinding::HelpOpen
                )
            {
                return RouterOutcome::FallThrough;
            }
            match command_for_policy(entry.binding, chord, snapshot) {
                Some(cmd) => RouterOutcome::Command(cmd),
                None => {
                    if snapshot.blocking_overlay_open {
                        if focused_is_blocking_overlay {
                            RouterOutcome::FallThrough
                        } else {
                            RouterOutcome::Swallow
                        }
                    } else {
                        RouterOutcome::FallThrough
                    }
                }
            }
        }
        None if snapshot.blocking_overlay_open => {
            if focused_is_blocking_overlay {
                RouterOutcome::FallThrough
            } else {
                RouterOutcome::Swallow
            }
        }
        None => RouterOutcome::FallThrough,
    }
}

/// Whether a component id is one of the blocking overlays the policy mounts.
/// Mirrors the shell's `blocking_overlay_active` set so the router can tell
/// "the focused leaf is the overlay itself" from "an overlay is mounted
/// elsewhere".
pub(super) fn is_blocking_overlay(id: &ComponentId) -> bool {
    use super::components::{ModalId, OverlayId, PopupId};
    matches!(
        id,
        ComponentId::Overlay(OverlayId::ContextMenu | OverlayId::SelectionModal)
            | ComponentId::Modal(
                ModalId::Confirm
                    | ModalId::DaemonLost
                    | ModalId::RemoteReanchor
                    | ModalId::SavePlaylist,
            )
            | ComponentId::Popup(
                PopupId::Multiselect | PopupId::LibraryRoutes | PopupId::FeedManage,
            )
    )
}
