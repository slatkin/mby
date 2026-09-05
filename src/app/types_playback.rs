use mbv_core::api::EmbyItem;
use mbv_core::playback_queue::{QueueItem, QueueSlotId};
use mbv_core::player::{PlayerEvent, PlayerProxy};
use mbv_core::ws::WsEvent;
use std::collections::HashMap;
use std::collections::VecDeque;
use std::sync::mpsc;

/// Shared local-vs-remote playback seam for the TUI action layer.
#[derive(Clone, Copy)]
pub(super) struct LocalPlaybackTarget;

#[derive(Clone)]
pub(super) struct RemotePlaybackTarget {
    pub(super) session_id: String,
}

/// Reads/writes `app.cast_attachment` directly, the same way
/// `LocalPlaybackTarget` reads `app.player` -- see `playback_target_cast.rs`.
#[derive(Clone, Copy)]
pub(super) struct CastPlaybackTarget;

#[derive(Clone)]
pub(super) enum PlaybackTarget {
    Local(LocalPlaybackTarget),
    Remote(RemotePlaybackTarget),
    Cast(CastPlaybackTarget),
}

/// Why an optimistic playhead prediction is not yet confirmed by the playback
/// owner. Replaces the roles of the former `PendingActiveIdx::Shift` /
/// `PendingActiveIdx::Jump` respectively.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum PredictionReason {
    /// A queue edit relocated the still-playing item to a new slot; the live
    /// `position_ticks`/`runtime_ticks` in the player status lock stay valid
    /// and are read through untouched.
    Relocated,
    /// A different queue item was selected to play; the status lock still holds
    /// the previous item's position/runtime, so progress reports as 0/0 until
    /// the player thread reconciles.
    ItemSelected,
}

/// How sure the shell is that the projected playhead slot is the one the
/// playback owner is actually playing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum PlayheadConfidence {
    Confirmed,
    Predicted(PredictionReason),
}

/// The shell's single source of truth for where playback is, how sure we are,
/// and the one-shot `queue_cursor` push the next `sync_queue` should apply.
///
/// Folds in the former `App::pending_active_idx` (the optimistic active index
/// awaiting player-thread acknowledgement — now `confidence` plus
/// `scope`/`slot`) and the former `App::queue_cursor_pushed` (the scoped
/// one-shot cursor push — now `pending_push`).
///
/// The resting "no prediction, no armed push" state is `confidence: Confirmed`
/// with `pending_push: None`; `slot` is only meaningful while `confidence` is
/// `Predicted`. Position/runtime are never projected here — they are a live
/// per-frame read from `player.status`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct PlayheadProjection {
    pub(super) scope: QueueScope,
    pub(super) slot: usize,
    pub(super) confidence: PlayheadConfidence,
    /// One-shot cursor push for the next `sync_queue`, scoped to one queue
    /// scope and consumed only while that scope is visible. `Follow` yields to
    /// an in-progress user navigation (matching `Predicted(Relocated)`
    /// follow-the-playhead semantics); `Reanchor` is authoritative and always
    /// wins.
    pub(super) pending_push: Option<QueueCursorPush>,
}

impl PlayheadProjection {
    /// Resting state: playhead confirmed by the owner, no armed cursor push.
    pub(super) fn new() -> Self {
        Self {
            scope: QueueScope::Local,
            slot: 0,
            confidence: PlayheadConfidence::Confirmed,
            pending_push: None,
        }
    }

    /// The predicted active slot (former `PendingActiveIdx::idx`).
    pub(super) fn idx(&self) -> usize {
        self.slot
    }

    /// Whether an in-flight prediction forces progress to 0/0: true only for
    /// `Predicted(ItemSelected)` (former `PendingActiveIdx::Jump`). Every other
    /// state reads live position/runtime straight off `player.status`.
    pub(super) fn suppresses_progress(&self) -> bool {
        matches!(
            self.confidence,
            PlayheadConfidence::Predicted(PredictionReason::ItemSelected)
        )
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) struct PlaybackState {
    pub(super) active: bool,
    pub(super) active_idx: usize,
    pub(super) position_ticks: i64,
    pub(super) runtime_ticks: i64,
    pub(super) paused: bool,
}

/// Which queue an operation refers to.
///
/// `Local` is this TUI instance's own queue and carries local-only metadata:
/// dirty state, undo history, saved-playlist source, and on-disk persistence.
/// `Remote` is the queue owned by a directly-controlled mbv daemon or remote
/// instance. A stale `Remote` UI preference is ignored unless a direct remote
/// queue is actually present.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum QueueScope {
    Local,
    Remote,
}

/// Why the shell armed a one-shot `queue_cursor` push for the next
/// `sync_queue`, and which queue scope it applies to. `sync_queue` consumes
/// it (clearing the flag) only when the visible scope matches; a push armed
/// for a scope the user is not looking at is dropped rather than snapping the
/// other scope's independent selection.
///
/// `Follow` tracks the playhead (local mpv advance, now-playing snap,
/// projected remote/session queue updates) and yields to an in-progress user
/// navigation (`queue_cursor_held_by_user`). `Reanchor` is an authoritative
/// content change (scope switch, full queue replacement, wheel scroll,
/// jump-to-now-playing) whose regenerated slot identities may collide with the
/// old ones, so it always wins over slot-identity reconciliation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum QueueCursorPush {
    Follow(QueueScope),
    Reanchor(QueueScope),
}

impl QueueCursorPush {
    pub(crate) fn scope(self) -> QueueScope {
        match self {
            Self::Follow(scope) | Self::Reanchor(scope) => scope,
        }
    }
}

/// Derived answers for the local/remote queue boundary.
///
/// The three answers intentionally differ:
/// - playback commands target `Remote` whenever a direct remote queue exists;
/// - the visible queue is `Remote` only when a direct remote queue exists and
///   the user has selected the remote scope;
/// - local queue metadata applies only to local scope while a direct remote
///   queue exists, but applies to any effective scope when no direct remote
///   queue exists because all queue state is local then.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct QueueScopeResolution {
    pub(super) has_direct_remote_queue: bool,
    pub(super) requested_visible_scope: QueueScope,
}

impl QueueScopeResolution {
    pub(super) fn new(has_direct_remote_queue: bool, requested_visible_scope: QueueScope) -> Self {
        Self {
            has_direct_remote_queue,
            requested_visible_scope,
        }
    }

    pub(super) fn playback_target(self) -> QueueScope {
        if self.has_direct_remote_queue {
            QueueScope::Remote
        } else {
            QueueScope::Local
        }
    }

    pub(super) fn visible_scope(self) -> QueueScope {
        if self.has_direct_remote_queue && self.requested_visible_scope == QueueScope::Remote {
            QueueScope::Remote
        } else {
            QueueScope::Local
        }
    }

    pub(super) fn local_metadata_applies(self, scope: QueueScope) -> bool {
        scope == QueueScope::Local || !self.has_direct_remote_queue
    }
}

/// A reversible queue edit. `Remove` re-inserts the item at its old position;
/// `Move` swaps the slot back from `to` to `from`. `slot_id` is the runtime
/// queue occurrence that landed at `to`, checked at undo time so a queue edit
/// made after the move is refused instead of swapping the wrong items.
#[derive(Debug)]
pub(super) enum UndoEntry {
    Remove(usize, QueueItem),
    Move {
        from: usize,
        to: usize,
        slot_id: QueueSlotId,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum RemoteSlotState {
    Off,
    AttachedSession,
    DirectRemote,
    LocalDaemon,
}

/// Which destination a Home "Latest" pill belongs to: an Emby library (view)
/// id, an Audiobookshelf podcast library id, or the single flattened Feeds
/// pill. This is the merge key — each provider/library only ever touches its
/// own entries when populating `HomeContent.latest`.
/// `Audiobookshelf`/`Feeds` variants are constructed by Parts 2 and 3 of
/// #543; matching on them here already keeps the merge keyed per provider.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(super) enum HomeLatestSource {
    Emby(String),
    Audiobookshelf(String),
    Feeds,
}

impl HomeLatestSource {
    /// Solid string identity for persistence: `"emby:<id>"`, `"abs:<id>"`,
    /// or `"feeds"`. Restoring by identity (not section index) lets Home
    /// leave the pill unselected until a section matching it actually arrives
    /// asynchronously.
    pub(super) fn pref_key(&self) -> String {
        match self {
            HomeLatestSource::Emby(id) => format!("emby:{id}"),
            HomeLatestSource::Audiobookshelf(id) => format!("abs:{id}"),
            HomeLatestSource::Feeds => "feeds".into(),
        }
    }

    pub(super) fn from_pref_key(key: &str) -> Option<Self> {
        let (prefix, id) = key.split_once(':').unwrap_or((key, ""));
        match prefix {
            "emby" => Some(HomeLatestSource::Emby(id.to_string())),
            "abs" => Some(HomeLatestSource::Audiobookshelf(id.to_string())),
            "feeds" => Some(HomeLatestSource::Feeds),
            _ => None,
        }
    }
}

/// Model-owned Home content (task 5.3d): the authoritative snapshot the
/// shell pushes to `HomeComponent` at its writers. Re-homed from the deleted
/// `App.home` (`HomePane`) + `App.home_loading`; `loading` mirrors the old
/// `home_loading` flag (true from startup until the first fetch completes,
/// then set false synchronously after every content computation). The
/// Continue Watching column cursor (`continue_cursor`) is the preserved
/// legacy quirk cursor — the component owns the flat render cursor, App
/// effects act on this one.
pub(super) struct HomeContent {
    pub(super) continue_items: Vec<EmbyItem>,
    pub(super) continue_cursor: usize,
    pub(super) latest: Vec<(String, HomeLatestSource, Vec<QueueItem>, usize)>, // (title, source, items, cursor)
    pub(super) loading: bool,
}

impl HomeContent {
    /// Default Home state at shell construction: no items/pills, the Continue
    /// Watching column cursor parked at 0, and `loading` true — the startup
    /// skeleton, mirroring the deleted `App.home_loading`/`construct` state.
    pub(super) fn new() -> Self {
        Self {
            continue_items: Vec::new(),
            continue_cursor: 0,
            latest: Vec::new(),
            loading: true,
        }
    }
}

pub(super) struct SuspendedLocalSession {
    pub(super) player: PlayerProxy,
    pub(super) player_rx: mpsc::Receiver<PlayerEvent>,
    pub(super) ws_rx: mpsc::Receiver<WsEvent>,
    pub(super) ws_send_tx: Option<mbv_core::ws::WsSender>,
    pub(super) audiobookshelf_socket_rx:
        mpsc::Receiver<mbv_core::audiobookshelf_socket::SocketEvent>,
    pub(super) audiobookshelf_socket_tx: Option<mpsc::Sender<()>>,
    pub(super) audiobookshelf_socket_generation: Option<mbv_core::service_runtime::SetupGeneration>,
}

pub(super) enum PendingQueueAction {
    PlayItems {
        items: Vec<EmbyItem>,
        start_idx: usize,
        source: crate::config::QueueSource,
    },
    ClearQueue,
}

pub(super) struct RemoteReanchorPopup {
    pub(super) targets: Vec<(usize, String)>,
    pub(super) cursor: usize,
}

#[derive(Clone, Debug)]
pub(super) enum PlaylistMutation {
    Save {
        mutation_id: u64,
        queue_lineage: u64,
        source_playlist_id: String,
        item_ids: Option<Vec<String>>,
    },
    CreateAs {
        mutation_id: u64,
        coordinator_key: String,
        name: String,
        queue_lineage: u64,
        source_playlist_id: Option<String>,
        item_ids: Option<Vec<String>>,
    },
    Replace {
        mutation_id: u64,
        queue_lineage: u64,
        source_playlist_id: String,
        name: String,
        item_ids: Option<Vec<String>>,
    },
}

impl PlaylistMutation {
    pub(super) fn mutation_id(&self) -> u64 {
        match self {
            Self::Save { mutation_id, .. }
            | Self::CreateAs { mutation_id, .. }
            | Self::Replace { mutation_id, .. } => *mutation_id,
        }
    }
}

#[derive(Clone, Debug, Default)]
pub(super) struct PlaylistMutationState {
    pub(super) active: Option<PlaylistMutation>,
    pub(super) queued: VecDeque<PlaylistMutation>,
}

#[derive(Clone, Debug)]
pub(super) struct RemoteQueueProjection {
    pub(super) session_id: String,
    pub(super) epoch: u64,
    pub(super) queue_lineage: u64,
    pub(super) occurrence_slots: HashMap<u64, QueueSlotId>,
    pub(super) slot_occurrences: HashMap<QueueSlotId, u64>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn relocated_keeps_progress_item_selected_zeroes_it() {
        let relocated = PlayheadProjection {
            scope: QueueScope::Local,
            slot: 3,
            confidence: PlayheadConfidence::Predicted(PredictionReason::Relocated),
            pending_push: None,
        };
        assert!(!relocated.suppresses_progress());

        let selected = PlayheadProjection {
            confidence: PlayheadConfidence::Predicted(PredictionReason::ItemSelected),
            ..relocated
        };
        assert!(selected.suppresses_progress());
        assert_eq!(selected.idx(), 3);

        let confirmed = PlayheadProjection {
            confidence: PlayheadConfidence::Confirmed,
            ..relocated
        };
        assert!(!confirmed.suppresses_progress());
    }
}
