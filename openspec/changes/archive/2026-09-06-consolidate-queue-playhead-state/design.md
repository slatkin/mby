## Context

See `proposal.md` — Why. The scattered state today:

| Field | Owner | Meaning |
| --- | --- | --- |
| `PlayerTab::queue_cursor` | each `PlayerTab` (local / remote / session) | the queue's own selection/scroll anchor — presentation state, persisted |
| player status `current_idx` | player thread, via `status` lock | the slot the owner is actually playing |
| `App::pending_active_idx: Option<PendingActiveIdx>` | `App` | optimistic active index the player thread has not yet confirmed |
| `App::queue_cursor_pushed: Option<QueueCursorPush>` | `App` | one-shot "make `sync_queue` push an index as `Set`, scoped" flag |
| reconciliation match | inline in `effective_playback_state(&mut self)` | consumes `pending_active_idx` when `current_idx` + `queue_len` catch up |

`queue_cursor` is a real, separate concept (what the user has selected/where
the list is scrolled) and stays. The other four are all facets of one thing —
where playback is, how sure we are, and what the UI should do about it — and
they are read/reconciled in the render path, which is why a paint can mutate
prediction state.

`queue-canonical-list`'s "Queue projection is bounded presentation data"
requirement already owns the child-facing contract; this change makes the
shell-side source of that projection a single value.

## Goals / Non-Goals

**Goals:**

- One owned value on `App` for the playback playhead: active scope, active
  slot, position/runtime, and a `Confirmed | Predicted(reason)` state that
  folds in `pending_active_idx` and `queue_cursor_pushed`.
- Reconciliation is one function called once per tick after player events are
  drained; `effective_playback_state` and `sync_queue` become pure readers.
- Scope accessors renamed off an explicit viewed-vs-playing axis.

**Non-Goals:**

- No change to `PlayerTab::queue_cursor`, queue persistence, or the
  component's local selection model.
- No change to the player-thread `status` contract, `mbv-core`, ctrl, or the
  daemon. This is shell-only.
- Not the owner ↔ Playback-run slot-identity work — that is the separate
  `unify-queue-playback-authority` change. The two do not depend on each
  other and touch disjoint files (that change is `mbv-core`/`daemon_run.rs`;
  this one is `src/app/`).

## Decisions

### The projection lives on `App` as one field, not on `PlayerTab`

The playhead spans all three `PlayerTab`s (it names which scope is playing),
and it is reconciled against the player thread, not persisted — so it belongs
next to the other shell-runtime state on `App`, not inside a per-scope
`PlayerTab`. Replace `pending_active_idx` and `queue_cursor_pushed` with:

```rust
struct PlayheadProjection {
    scope: QueueScope,
    slot: usize,
    confidence: PlayheadConfidence, // Confirmed | Predicted(PredictionReason)
}
enum PredictionReason { Relocated, ItemSelected } // = today's Shift / Jump
```

The scoped-push behavior (`Follow` yields to an active user navigation,
`Reanchor` always wins, consumed only when the scope is visible) is preserved
as a method on the projection — `Follow` maps to `Predicted(Relocated)`
semantics for the yield rule, `Reanchor` to an authoritative re-anchor.
Alternative considered: keep two separate fields but give each a doc comment.
Rejected — that is the status quo the #650 bugs came from.

### Reconciliation is a tick step, not a render-path side effect

Add `App::reconcile_playhead()` called from the run loop immediately after
`handle_player_event` drains the player channel (`shell_run.rs` ~L263), in the
same spot the other post-event re-projections already run. It compares the
projection's predicted slot against `status.current_idx` + `status.queue_len`
and drops the prediction on a match. It resolves only the *prediction* — which
slot is active and whether stale progress is suppressed. It does NOT snapshot
position/runtime: those stay a live per-frame read from `player.status` (the
single source of truth — the mpv thread advances them without emitting a
`PlayerEvent`, so a pinned copy would freeze between discrete transitions).
`effective_playback_state` then just reads the projection plus live status; it
loses its `&mut self`.

Alternative considered: reconcile inside `sync_queue`. Rejected — `sync_queue`
also runs on pure layout ticks with no new player event, and the reconcile
should be tied to "new status arrived", not "we happen to be re-syncing".

### Scope accessors collapse to two named predicates

Introduce `viewed_queue_scope()` (was `visible_queue_scope`) and
`playing_queue_scope()` (was `playback_target_queue_scope`), and derive the
rest: `queue_scope_is_playback` becomes `scope == self.playing_queue_scope()`,
`displayed_queue()` / `playback_queue()` keep their names but are documented as
"queue for the viewed scope" / "queue for the playing scope". This is a rename
+ doc pass, no behavior change; it is the lowest-value part of the change and
can be dropped if it balloons.

### CONTEXT.md gains "Playhead"

"playhead" is used informally in code today but is not a defined term. Add a
short entry under `## Queue` so the projection has a named concept, per
AGENTS.md ("add new terms with the change").

## Risks / Trade-offs

- **Retargeting existing tests is the bulk of the diff** → the behavior is
  already locked by #650 tests; port them to the projection API rather than
  rewriting assertions. Net-new tests limited to one: reconcile runs on the
  event tick and not on a bare layout tick.
- **A missed push/prediction arm site silently regresses follow-the-playhead**
  → the compiler forces every site (the field type changes); enumerate them
  from the #650 commit (`player_event.rs` ×3, `run_loop_events*.rs` ×2,
  `queue_scope.rs` ×3, `mouse_gestures.rs`, `action.rs`, `queue_actions.rs`
  ×2) and convert each with its existing scope + reason.
- **`effective_playback_state` losing `&mut self` ripples to callers** →
  it has few callers (indicator render, queue playback state); they already
  hold `&mut App` or `&App` and a shared read is strictly looser.
- **Scope-accessor rename churn** → keep it in its own commit so it can be
  reverted independently if review finds it not worth the noise.

## Migration Plan

Pure internal refactor, no runtime migration. Land in one branch, ideally
three commits: (1) projection type + reconcile step + arm-site conversion,
(2) `effective_playback_state`/`sync_queue` become readers, (3) scope-accessor
rename. Rollback = revert the branch; no persisted or wire state changes.
