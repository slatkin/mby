//! Command seam between key-event translation (`input.rs`) and effects
//! (`actions.rs`, `player.rs`). See issue #78.
//!
//! `playback_command_for_key` is a pure function: given a key event and two
//! booleans describing playback state, it decides *whether* a key should be
//! intercepted and *what* it means, without touching `App` at all. `dispatch`
//! then owns the state transitions for each `Command` variant.
//!
//! Converted so far, `handle_playback_key` (the issue #78 pilot). The help
//! overlay was converted to a TuiRealm Interactive Component
//! (`src/app/components/help.rs`) and no longer routes through this `Command`
//! enum. Other modal handlers still speak directly to `App` and are expected to
//! migrate to this same `Command` enum over time, one handler at a time.

use super::input_resolver::KeyChord;
use super::notify_actions::ToastSeverity;
use super::types_playback::{PlayheadConfidence, PredictionReason};
use super::App;
use crossterm::event::{KeyCode, KeyModifiers};
use mbv_core::api::EmbyItem;
use mbv_core::player::PlayerCommand;
use std::sync::Arc;

#[derive(Debug, Clone, PartialEq)]
pub(super) enum Command {
    OpenIdleFeedLink,
    ToggleVisualizer,
    TogglePlayPause,
    Stop,
    /// Relative seek in seconds; negative rewinds, positive fast-forwards.
    SeekRelative(f64),
    NextTrack,
    PreviousTrack,
    /// `z`: `dispatch` always calls `cycle_sub()`, which cycles through all
    /// available subtitle tracks (plus "off") for both remote sessions and
    /// local playback -- unified in #86 so the two backends no longer
    /// diverge (local used to be a plain on/off `toggle_sub()`). The
    /// local-idle fallback (cycling the default subtitle *mode* when there's
    /// no active player) still lives inside `cycle_sub()`, since it has no
    /// session equivalent to unify with.
    CycleOrToggleSubtitle,
    AdjustVolume(i64),
    /// The `m` key: flips `mute_on` and sends `PlayerCommand::SetMute`.
    /// **Not** the same mechanism as `ToggleMuteOrCycleAudio`'s mute path
    /// below, which instead flips `ui_volume`/`pre_mute_volume` via
    /// `SetVolume` — these are two separate, pre-existing "mute" code paths
    /// with no cross-reference in the original code; not unified here since
    /// that would be a behavior change (see issue #78 follow-up, #84).
    ToggleMute,
    /// The `a` key: `dispatch` replicates the `is_audio_item()` branch,
    /// calling `toggle_mute()` (the `ui_volume`/`pre_mute_volume`/`SetVolume`
    /// mechanism, *not* `Command::ToggleMute`'s `mute_on`/`SetMute`) if the
    /// current item is audio-only, otherwise `cycle_audio()`. Gated the same
    /// way as the other transport keys (`active OR has_remote_session`) —
    /// see #88. The shared `PlaybackTarget` seam owns the local-vs-remote
    /// split underneath `is_audio_item()`, `toggle_mute()`, and
    /// `cycle_audio()`, so this action layer no longer re-derives it in each
    /// helper.
    ToggleMuteOrCycleAudio,

    // ── queue activation (issue #134) ───────────────────────────────────
    /// Activate the item at the given queue index: `Enter` on the queue
    /// tab, or a double-click on a queue row (`handle_mouse`'s
    /// `is_double`/queue branch — the two were already made to match in
    /// a70ad7a, before either went through `Command`; this variant is the
    /// single implementation both now share). Session-attached: hands the
    /// item off to the remote session. Otherwise: seeks to the top if it's
    /// the already-playing audio item, jumps to it if it's elsewhere in the
    /// active playback queue, or replaces the local playback queue and plays
    /// from this index if the visible queue isn't the one currently playing.
    /// The target index is carried explicitly (split-queue-cursor-ownership
    /// D2): the shell resolves the slot the user selected and passes it
    /// rather than this command re-reading `queue.queue_cursor` as an
    /// ambient argument channel.
    QueuePlayCursor(usize),

    /// `x`: cycle the Power View layout through both, queue-only, and
    /// library-only (see `PanelMode`); below the mini-view threshold it
    /// toggles queue-only and library-only.
    CyclePanelMode,

    // ── destination-independent routing ─────────────────────────────────
    /// Quit the client through the normal dirty-queue/prefs shutdown path.
    Quit,
    NextLibraryTab,
    PreviousLibraryTab,
    SetLibraryTab(usize),
    ForceClear,
    /// Raise the "Clear queue?" confirmation prompt (the global `c` binding).
    RequestClearQueue,
    RefreshCurrentView,
    ToggleSettings,
    OpenSessions,
    OpenPlaylists,
    OpenSearch,
    /// Model-owned because mounting Help belongs to the TuiRealm shell.
    OpenHelp,
    FocusPanel(super::PanelFocus),
}

/// Resolve the idle-feed link shortcut separately from transport bindings so
/// `o` remains available to the view when no link is displayed. A daemon-backed
/// player is still an idle feed view when no Emby session is connected, so the
/// playback backend and connected-session gates stay separate here.
pub(super) fn idle_feed_command_for_key(
    chord: KeyChord,
    player_active: bool,
    has_connected_session: bool,
    link_available: bool,
) -> Option<Command> {
    match chord.code {
        KeyCode::Char('o')
            if chord.mods.is_empty()
                && !player_active
                && !has_connected_session
                && link_available =>
        {
            Some(Command::OpenIdleFeedLink)
        }
        _ => None,
    }
}

/// Translate a key event into a playback `Command`, or `None` if this handler
/// doesn't intercept the key. Pure function: no `App`/`Player` access, so it's
/// testable without constructing either.
///
/// Gating is **not** a single shared rule; it mirrors the three sequential
/// match blocks `handle_playback_key` used to have, key by key:
///
/// | Keys | Fires when |
/// | --- | --- |
/// | Space, `<`/`>` (seek), `N`/`P`, Esc (stop), `a` (audio) | `has_remote_session` OR `active` |
/// | `z` (sub cycle/toggle) | unconditionally |
/// | `m` (mute) | unconditionally, no session check |
/// | `-`/`+` (volume) | unconditionally |
pub(super) fn playback_command_for_key(
    chord: KeyChord,
    active: bool,
    has_remote_session: bool,
) -> Option<Command> {
    let ctrl = chord.mods.contains(KeyModifiers::CONTROL);
    let gated = has_remote_session || active;
    match chord.code {
        KeyCode::Char(' ') if gated => Some(Command::TogglePlayPause),
        KeyCode::Esc if gated => Some(Command::Stop),
        KeyCode::Char('<') if gated => Some(Command::SeekRelative(-5.0)),
        KeyCode::Char('>') if gated => Some(Command::SeekRelative(5.0)),
        KeyCode::Char('N') if gated => Some(Command::NextTrack),
        KeyCode::Char('P') if gated => Some(Command::PreviousTrack),
        KeyCode::Char('z') if !ctrl => Some(Command::CycleOrToggleSubtitle),
        KeyCode::Char('m') => Some(Command::ToggleMute),
        KeyCode::Char('-') => Some(Command::AdjustVolume(-5)),
        KeyCode::Char('+') | KeyCode::Char('=') => Some(Command::AdjustVolume(5)),
        KeyCode::Char('a') if gated && !ctrl => Some(Command::ToggleMuteOrCycleAudio),
        _ => None,
    }
}

/// Help-overlay metadata for a subset of `playback_command_for_key`'s
/// bindings — the "[playback]" section of the help overlay renders directly
/// from this table (see `render_help_panel`) instead of a hand-copied list,
/// so the two can no longer silently drift apart. See issue #133 (phase 4)
/// and `docs/adr/0002-centralized-input-handling.md`.
///
/// Each entry pairs display text with a *sample* chord (or chords) + gating
/// flag that a characterization test
/// (`playback_help_bindings_match_playback_command_for_key`, below) replays
/// through `playback_command_for_key` to assert this table stays truthful.
/// When a display entry covers more than one physical key (`<`/`>`, `N`/`P`,
/// `-`/`+`/`=`), `samples` lists every one of them, each paired with the
/// command it must resolve to — so the test exercises the whole displayed
/// claim, not just one side of it.
///
/// View-specific bindings that are not playback commands stay documented
/// separately in `render_help_panel`.
pub(super) struct PlaybackHelpBinding {
    /// Display text shown in the help overlay (e.g. `"Space"`, `"< / >"`).
    pub keys: &'static str,
    /// One-line description shown next to `keys`.
    pub label: &'static str,
    // Only read by the `playback_help_bindings_match_playback_command_for_key`
    // characterization test below; kept outside `#[cfg(test)]` since these
    // fields are part of the type's intended (drift-guard) purpose, not
    // test-only scaffolding — mirrors `ContextEntry::name` in
    // `input_resolver.rs`.
    #[allow(dead_code)]
    /// Every chord that produces the paired command via
    /// `playback_command_for_key`, used only to keep this table honest in
    /// tests — not consulted at runtime.
    pub samples: &'static [(KeyChord, Command)],
    #[allow(dead_code)]
    /// Whether each sample in `samples` only resolves to its command when
    /// gated (`active || has_remote_session`); `false` means it fires
    /// unconditionally.
    pub gated: bool,
}

pub(super) const PLAYBACK_HELP_BINDINGS: &[PlaybackHelpBinding] = &[
    PlaybackHelpBinding {
        keys: "Space (x2)",
        label: "Pause/Resume",
        samples: &[(
            KeyChord {
                code: KeyCode::Char(' '),
                mods: KeyModifiers::NONE,
            },
            Command::TogglePlayPause,
        )],
        gated: true,
    },
    PlaybackHelpBinding {
        keys: "Esc (x2)",
        label: "Stop",
        samples: &[(
            KeyChord {
                code: KeyCode::Esc,
                mods: KeyModifiers::NONE,
            },
            Command::Stop,
        )],
        gated: true,
    },
    PlaybackHelpBinding {
        keys: "< / >",
        label: "Seek \u{b1}5 seconds",
        samples: &[
            (
                KeyChord {
                    code: KeyCode::Char('<'),
                    mods: KeyModifiers::NONE,
                },
                Command::SeekRelative(-5.0),
            ),
            (
                KeyChord {
                    code: KeyCode::Char('>'),
                    mods: KeyModifiers::NONE,
                },
                Command::SeekRelative(5.0),
            ),
        ],
        gated: true,
    },
    PlaybackHelpBinding {
        keys: "Shift+N / P",
        label: "Next / Previous track",
        samples: &[
            (
                KeyChord {
                    code: KeyCode::Char('N'),
                    mods: KeyModifiers::NONE,
                },
                Command::NextTrack,
            ),
            (
                KeyChord {
                    code: KeyCode::Char('P'),
                    mods: KeyModifiers::NONE,
                },
                Command::PreviousTrack,
            ),
        ],
        gated: true,
    },
    PlaybackHelpBinding {
        keys: "- / +",
        label: "Volume down / up",
        samples: &[
            (
                KeyChord {
                    code: KeyCode::Char('-'),
                    mods: KeyModifiers::NONE,
                },
                Command::AdjustVolume(-5),
            ),
            (
                KeyChord {
                    code: KeyCode::Char('+'),
                    mods: KeyModifiers::NONE,
                },
                Command::AdjustVolume(5),
            ),
            (
                KeyChord {
                    code: KeyCode::Char('='),
                    mods: KeyModifiers::NONE,
                },
                Command::AdjustVolume(5),
            ),
        ],
        gated: false,
    },
    PlaybackHelpBinding {
        keys: "m",
        label: "Mute",
        samples: &[(
            KeyChord {
                code: KeyCode::Char('m'),
                mods: KeyModifiers::NONE,
            },
            Command::ToggleMute,
        )],
        gated: false,
    },
    PlaybackHelpBinding {
        keys: "a",
        label: "Cycle audio track",
        samples: &[(
            KeyChord {
                code: KeyCode::Char('a'),
                mods: KeyModifiers::NONE,
            },
            Command::ToggleMuteOrCycleAudio,
        )],
        gated: true,
    },
    PlaybackHelpBinding {
        keys: "z",
        label: "Cycle subtitles",
        samples: &[(
            KeyChord {
                code: KeyCode::Char('z'),
                mods: KeyModifiers::NONE,
            },
            Command::CycleOrToggleSubtitle,
        )],
        gated: false,
    },
];

impl App {
    /// Own the state transitions for a `Command`. Returns whether the app
    /// should quit (`true` only for `Command::Quit`'s non-prompting path;
    /// `false` for every other variant).
    ///
    /// For most playback variants this means picking a remote-session
    /// command vs. a local `Player` command, matching the divergent behavior
    /// `handle_playback_key` had inline (including its known bugs — see issue
    /// #78 follow-up).
    pub(super) fn dispatch(&mut self, command: Command) -> bool {
        match command {
            Command::OpenIdleFeedLink => {
                self.open_idle_feed_link();
            }
            Command::ToggleVisualizer => self.toggle_visualizer(),

            Command::TogglePlayPause => {
                self.playback_target().toggle_play_pause(self);
            }
            Command::Stop => {
                self.playback_target().stop(self);
            }
            Command::SeekRelative(delta) => {
                self.playback_target().seek_relative(self, delta);
            }
            Command::NextTrack => {
                self.playback_target().jump_track(self, 1, "NextTrack");
            }
            Command::PreviousTrack => {
                self.playback_target().jump_track(self, -1, "PreviousTrack");
            }
            Command::CycleOrToggleSubtitle => {
                // cycle_sub() branches internally on connected_session_id,
                // and falls back to the idle subtitle-mode cycle itself when
                // local playback has no active player (see #86).
                self.cycle_sub();
            }
            Command::AdjustVolume(delta) => {
                // adjust_volume already branches session vs. local internally.
                self.adjust_volume(delta);
            }
            Command::ToggleMute => {
                self.playback_target().toggle_command_mute(self);
            }
            Command::ToggleMuteOrCycleAudio => {
                if self.is_audio_item() {
                    self.toggle_mute();
                } else {
                    self.cycle_audio();
                }
            }

            Command::QueuePlayCursor(t) => {
                let (n, item) = {
                    let queue = self.displayed_queue();
                    let n = queue.total_queue_len();
                    let item = queue.item_at(t).cloned();
                    (n, item)
                };
                if t >= n {
                    return false;
                }
                // Validate the item at the cursor exists.
                let Some(item) = item else {
                    return false;
                };
                let owner_can_admit_audiobookshelf = self.player.can_admit_audiobookshelf();
                if !item.admissible_for_owner_with_audiobookshelf(
                    false,
                    |service| {
                        service != mbv_core::config::ServiceKind::Audiobookshelf
                            || owner_can_admit_audiobookshelf
                    },
                    owner_can_admit_audiobookshelf,
                ) {
                    self.flash(
                        "Playback owner rejected this Audiobookshelf item".into(),
                        super::notify_actions::ToastSeverity::Error,
                    );
                    return false;
                }
                // Validate source for Feed entries early.
                if let mbv_core::playback_queue::QueueItem::Feed(ref entry) = item {
                    if entry.primary_source().is_none() {
                        self.flash(
                            "Feed entry has no playable source".into(),
                            super::notify_actions::ToastSeverity::Error,
                        );
                        return false;
                    }
                }
                // Hydrate stored feed-entry state before building the
                // playback snapshot so resume uses the latest position.
                if let mbv_core::playback_queue::QueueItem::Feed(ref entry) = item {
                    let hydrated = self.hydrate_feed_entry_state(entry.clone());
                    let sid = self.playback_queue().slot_id_at(t);
                    if let Some(sid) = sid {
                        let queue_mut = self.playback_queue_mut();
                        let _ = queue_mut.queue.apply_progress(
                            sid,
                            hydrated.position_ticks,
                            hydrated.played,
                        );
                    }
                }
                // Snapshot data from the queue before any mutable borrows.
                let queue = self.displayed_queue();
                let emby_items: Vec<EmbyItem> = queue
                    .queue
                    .slots()
                    .iter()
                    .filter_map(|slot| slot.item.as_emby().cloned())
                    .collect();
                let all_items = queue.all_queue_items();
                let slot_id = queue.slot_id_at(t);
                // Pre-compute the Emby-only projection index for the cursor
                // position, needed by the session API boundary.
                let emby_start = queue
                    .queue
                    .slots()
                    .iter()
                    .take(t)
                    .filter(|s| s.item.as_emby().is_some())
                    .count();
                // Connected remote session: hand off Emby items to the
                // session; Feed entries cannot cross the Emby session API
                // so they fall through to the local/direct-remote path.
                if let mbv_core::playback_queue::QueueItem::Emby(_) = &item {
                    if let Some(conn_id) = self.connected_session_id.clone() {
                        let label = item.display_name();
                        self.flash(
                            format!("Requesting playback: {label}"),
                            ToastSeverity::Neutral,
                        );
                        self.set_queue_scope(self.playback_target_queue_scope());
                        if let Some(occurrence_id) = self.tracked_occurrence_at_queue_index(t) {
                            self.issue_remote_intent(
                                mbv_core::remote_reconciliation::RemoteIntent::Select {
                                    target: occurrence_id,
                                },
                            );
                            let item_ids: Vec<String> =
                                emby_items.iter().map(|e| e.id.clone()).collect();
                            let start_ticks = item.playback_position_ticks();
                            self.do_reconciliation_session_command(
                                &conn_id.clone(),
                                move |client| {
                                    client.session_play_items(
                                        &conn_id,
                                        &item_ids,
                                        emby_start,
                                        start_ticks,
                                    )
                                },
                            );
                            return false;
                        }
                        self.submit_attached_sequence(&conn_id, &emby_items, emby_start);
                        return false;
                    }
                }
                // Local / direct-remote playback.  The same path handles
                // both Feed and Emby items: jump to an active slot or
                // cold-start the full canonical queue.
                let scope = self.visible_queue_scope();
                let st = self.player.status.lock().unwrap();
                let active = st.active;
                let current_idx = st.current_idx;
                drop(st);
                if active && self.queue_scope_is_playback(scope) {
                    let is_audio = item.is_audio();
                    if t == current_idx && is_audio {
                        self.player.send_command(PlayerCommand::SeekAbsolute(0.0));
                    } else if t != current_idx {
                        self.playhead.confidence =
                            PlayheadConfidence::Predicted(PredictionReason::ItemSelected);
                        self.playhead.slot = t;
                        self.playhead.scope = self.playback_target_queue_scope();
                        if self.player.is_remote() {
                            let Some(slot_id) = slot_id else {
                                return false;
                            };
                            if !self
                                .player
                                .queue_play_slot(mbv_core::ctrl::slot_id_to_u64(slot_id))
                            {
                                self.flash(
                                    "Playback owner rejected the queue selection".into(),
                                    ToastSeverity::Error,
                                );
                            }
                        } else {
                            self.player.send_command(PlayerCommand::JumpTo(t));
                        }
                    }
                } else {
                    // Cold start: submit the full canonical queue (all
                    // variants) so the player's internal playlist matches
                    // the PlayerTab's queue exactly.
                    let owner_can_admit_audiobookshelf = self.player.can_admit_audiobookshelf();
                    let eligible: Vec<_> = all_items
                        .into_iter()
                        .filter(|i| {
                            i.admissible_for_owner_with_audiobookshelf(
                                false,
                                |service| {
                                    service != mbv_core::config::ServiceKind::Audiobookshelf
                                        || owner_can_admit_audiobookshelf
                                },
                                owner_can_admit_audiobookshelf,
                            )
                        })
                        .collect();
                    if eligible.is_empty() {
                        self.flash(
                            "Playback owner rejected the queue".into(),
                            ToastSeverity::Error,
                        );
                        return false;
                    }
                    let start_idx = eligible
                        .iter()
                        .position(|i| i.content_id() == item.content_id())
                        .unwrap_or_else(|| {
                            eligible
                                .iter()
                                .take(t)
                                .count()
                                .min(eligible.len().saturating_sub(1))
                        });
                    let headless = eligible.iter().all(|item| item.is_audio());
                    self.player.submit_queue(
                        eligible,
                        start_idx,
                        self.emby_snapshot().map(Arc::new),
                        headless,
                        self.ui_volume,
                    );
                }
            }

            Command::Quit => return self.try_quit(),
            Command::NextLibraryTab => self.library_tab_next(),
            Command::PreviousLibraryTab => self.library_tab_prev(),
            Command::SetLibraryTab(index) => {
                if index < self.tab_count() {
                    self.set_library_tab(index);
                }
            }
            Command::ForceClear => self.force_clear = true,
            Command::RequestClearQueue => self.request_clear_queue(),
            Command::RefreshCurrentView => self.refresh_current_view(),
            Command::ToggleSettings => self.request_sidebar_toggle(super::SidebarId::Settings),
            Command::OpenSessions => self.request_sidebar_toggle(super::SidebarId::Sessions),
            Command::OpenPlaylists => self.open_playlists_panel(),
            Command::OpenSearch => self.open_search_sidebar(),
            // Model handles this shell-only command before delegating the
            // remaining commands to App::dispatch.
            Command::OpenHelp => unreachable!("OpenHelp is dispatched by Model"),
            Command::FocusPanel(focus) => {
                self.set_panel_focus(focus);
                self.last_card_height = 0;
                self.last_card_width = 0;
            }

            Command::CyclePanelMode => {
                // Narrow terminal (< MINI_VIEW_THRESHOLD columns): mini view
                // toggles exactly two states, library-only ⇄ queue-only.
                if self.terminal_width < super::MINI_VIEW_THRESHOLD {
                    self.mini_view_focus = match self.mini_view_focus {
                        super::PanelFocus::Library => super::PanelFocus::Queue,
                        super::PanelFocus::Queue => super::PanelFocus::Library,
                    };
                    if matches!(self.mini_view_focus, super::PanelFocus::Queue) {
                        self.focus_queue_initial_item();
                    }
                } else {
                    self.panel_mode = match self.panel_mode {
                        super::PanelMode::Both => super::PanelMode::QueueOnly,
                        super::PanelMode::QueueOnly => super::PanelMode::LibraryOnly,
                        super::PanelMode::LibraryOnly => super::PanelMode::Both,
                    };
                    match self.panel_mode {
                        super::PanelMode::LibraryOnly => {
                            if matches!(self.panel_focus, super::PanelFocus::Queue) {
                                self.set_panel_focus(super::PanelFocus::Library);
                            }
                        }
                        super::PanelMode::QueueOnly => {
                            self.set_panel_focus(super::PanelFocus::Queue);
                        }
                        super::PanelMode::Both => {}
                    }
                }
            }
        }
        false
    }
}

#[cfg(test)]
#[path = "action_tests.rs"]
mod tests;
