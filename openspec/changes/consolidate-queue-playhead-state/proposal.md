## Why

"Where playback currently is" is spread across at least five fields in four
files — `PlayerTab::queue_cursor`, the player status `current_idx`,
`pending_active_idx` (optimistic index prediction), `queue_cursor_pushed`
(authoritative-push flag), and the reconciliation logic embedded in
`effective_playback_state`'s render path. Every recent queue desync bug in
#650 was a mismatch *between* these pieces, not a defect in any one of them:
a bare `bool` that could not say which scope armed it, a bare `usize` that
could not say whether the playing item had changed. Two of those were just
patched with local enums (`QueueCursorPush`, `PendingActiveIdx`); this change
finishes the job by giving the playhead a single owner and a single
reconcile step so the next seam bug has nowhere to hide.

## What Changes

- Introduce one owned value — a **playhead projection** — that holds the
  active scope, the active slot index, playback position and runtime, and
  whether that index is **confirmed** by the player thread or **predicted**
  (with the reason: a queue edit relocated the still-playing item, or a
  different item was selected). It replaces `pending_active_idx` and absorbs
  the reconciliation currently inlined in `effective_playback_state`.
- Fold the existing `queue_cursor_pushed: Option<QueueCursorPush>` push flag
  and the `pending_active_idx: Option<PendingActiveIdx>` prediction into the
  one projection type, so a single place decides "does the component adopt
  this index, and does the progress bar trust this position".
- Move playhead reconciliation out of the render path: it becomes a
  tick-phase step that consumes player-thread status and clears predictions,
  leaving `effective_playback_state` (and the queue sync) as pure readers.
- Name the scope accessors off an explicit two-axis model — the scope the
  user is *viewing* versus the scope that is *playing* — and collapse the
  near-synonym helpers (`visible_queue_scope`, `playback_target_queue_scope`,
  `queue_scope_is_playback`, `displayed_queue`, `playback_queue`, and their
  `_mut` pairs) onto it.
- No user-observable behavior change beyond what #650 already fixed. The
  scope-aware push consumption and the stale-progress suppression that the
  #650 branch introduced are preserved exactly, now expressed as properties
  of the projection rather than ad hoc checks.

## Capabilities

### New Capabilities

_None._

### Modified Capabilities

- `queue-canonical-list`: the "Queue projection is bounded presentation data"
  requirement gains the constraint that the projection's semantic active
  state and progress derive from a single owned playhead projection that is
  reconciled in one place, and that an optimistic active index is typed by
  the reason it is optimistic (so a still-playing relocated item keeps its
  progress while a newly selected item does not).

## Impact

- **Code (TUI binary only)**: `src/app/app_struct.rs` (field consolidation),
  `src/app/playback_target.rs` (`effective_playback_state` becomes a reader),
  a new tick-phase reconcile step in the shell run loop, `src/app/action.rs`
  and `src/app/queue_actions.rs` (prediction arm sites), `src/app/player_event.rs`
  and `src/app/run_loop_events*.rs` (push arm sites), `src/app/shell_queue.rs`
  (push consumption), `src/app/queue_scope.rs` (scope accessors),
  `src/app/types_playback.rs` (the projection type; `QueueCursorPush` and
  `PendingActiveIdx` fold in).
- **No changes** to `mbv-core`, the ctrl protocol, persistence, the daemon,
  or any provider. The player-thread status contract is unchanged — this is
  purely how the shell holds and reconciles what the player reports.
- **Tests**: the existing playhead/prediction/scope tests are retargeted to
  the projection API; net new tests kept to a minimum (the #650 behaviors
  they lock in are already covered).
