## Context

See `proposal.md` — Why. The constraints that shape the approach:

* `MouseGestureState` (`src/app/components/mouse/gesture.rs`) is deliberately
  stateless between clicks and documents that `Moved`, `Drag(_)` and `Up(_)`
  produce `None`. Drag is the first gesture that needs state spanning a
  press/release pair.
* `WideMediaList::resolve_point` already maps a screen position to a
  `QueueSlotId` from the same `row_geometry` the painter consumes, so drag
  hit-testing needs no new geometry.
* Queue rows are exactly one cell tall (`queue.rs` rebuilds `geometry.rows` at
  `height: 1`). A row has no upper or lower half, so a drop target is a row, and
  a row is an index. The above/below insertion ambiguity of GUI drag-and-drop
  does not exist here.
* `apply_queue_move_by_slot` (`src/app/queue_actions.rs`) is already an
  arbitrary from→to move: playhead correction, `queue_dirty`, persistence, and
  the slot-addressed unified protocol send with a positional fallback. Only its
  caller `move_queue_item_by` clamps the distance to one row.
* `ADR 0024` forbids a global hit map or coordinate router; gesture state is
  per-mounted-parent.

## Goals / Non-Goals

**Goals:**

* Pointer reorder that reaches the same code path, undo entries, persistence and
  protocol sends as `Shift+Up`/`Shift+Down`.
* No new painting. No new geometry. No new layout math in `WideMediaList`.
* The new gesture is inert for every parent that does not interpret it.

**Non-Goals:**

* Drop indicators, insertion carets, or a drag ghost.
* Deferred-until-release drop semantics.
* Coalescing a drag's undo entries into one.
* Drag in `InlineMediaBrowser`, or on any surface other than the Queue.
* Horizontal drag, drag-out-to-remove, drag between scopes.

## Decisions

### D1 — Live reorder, not deferred drop

The drag reorders on every row crossing rather than holding the entry and
committing on release.

*Why:* it needs no new painting at all — the row visibly travels because the
queue actually reorders — and edge auto-scroll comes free, since the moved
entry stays selected and `WideMediaList` already keeps the selection in view.

*Alternative rejected:* hold the grab and commit one move on release. That needs
a drag ghost or an insertion indicator painted between rows, which `WideMediaList`
has no concept of (its row flow is contiguous fixed-height rows serving Hero
rails too), plus a bespoke auto-scroll timer in a component that holds no
tick-driven state today. Materially more work for the same end state.

*Accepted cost:* see R1 and R2.

### D2 — A press stays a click; drag is an additional gesture

`Down(Left)` keeps emitting `Click` exactly as today, and a drag is recognized
only once motion follows. The click selects the row, which is also the row being
grabbed, so the selection is correct either way.

*Alternative rejected:* defer click emission to `Up(Left)` so a drag never
produces a click. That changes the meaning of every existing click test across
every migrated surface and adds release latency to every click in the app, to
avoid a selection side effect that is already the desired one.

### D3 — The drag gesture carries positions; the parent resolves them

`MouseGesture` gains a variant carrying the press anchor and the current
position. The Queue resolves both through `list.resolve_point` and emits a
`Msg` carrying two `QueueSlotId`s. No coordinates cross the component boundary,
per the framework rule that messages carry resolved identities.

The anchor is carried rather than the grabbed slot because the recognizer knows
nothing about rows; resolution is the parent's job.

### D4 — The request carries `onto`, an identity, not an index

The new variant is shaped `MoveTo { scope, slot_id, onto }` where both are
`QueueSlotId`. `msg/queue.rs` already states the rule and the reason: the queue
can be reordered by the Player between paint and dispatch. The shell resolves
both to indices at dispatch, immediately before applying the move.

`QueueMove::Up`/`Down` stays untouched — the keyboard's single-step semantics
are genuinely different (they are relative, and no-op at the ends).

### D5 — The single-step mover delegates to the arbitrary one

`move_queue_item_by` currently computes `to` and inlines the undo push and
remote-cursor bookkeeping. That body becomes an arbitrary
`move_queue_item_to(from, to)`, and `move_queue_item_by` becomes the ±1 clamp
that calls it. One move path, so a drag and a `Shift+Down` cannot drift apart.

### D6 — The grab is armed by press, not by the first motion

The Queue holds `Option<QueueSlotId>` armed on the click that resolves to a row,
cleared on drag end. A drag whose current position resolves to no row is a no-op
that leaves the grab armed, so dragging out over blank space and back resumes.
This is what makes the "past the end of the list" scenario a no-op rather than a
move to the last row.

*Alternative rejected:* clamping an unresolved position to the nearest row.
That would make an accidental slip below the list silently move the entry to the
end.

### D7 — Verification

`MouseGestureState` is already unit-testable without a terminal by synthesizing
`MouseEvent`s (`gesture.rs` tests, and the mouse-input spec requires it). Drag
recognition, the press-without-motion case, and drag-end get unit tests there.

Queue drag behaviour gets component tests in `queue_component_tests.rs` driving
synthesized press/drag/release through `AppComponent::on`, asserting the emitted
`Msg` — the existing click/double-click/right-click tests are the model.

The shell dispatch arm gets a test asserting a `MoveTo` between two known slots
produces the same queue order and undo depth as the equivalent keyboard moves.

Per project rules: no raw pane-coordinate assertions, and no test that simulates
a whole end-to-end app flow.

## Risks / Trade-offs

* **R1 — A drag pushes one undo entry per row crossed.** Dragging an entry ten
  rows then pressing `Ctrl+Z` unwinds one row at a time. → Accepted for this
  change; it is surprising but not wrong, and the entry is recoverable. If it
  grates, the fix is to merge consecutive `UndoEntry::Move` records with a
  matching `slot_id` where `prev.to == next.from`, which would also improve
  repeated `Shift+Down`. Out of scope here because it changes keyboard behaviour
  too.
* **R2 — A drag in the Remote scope sends one protocol move per row crossed.**
  Bounded by queue length, sent on row crossings rather than on raw motion
  events, and each is the slot-addressed `queue_move_slot` which is not
  index-racy. → Accepted. If it proves chatty in practice the deferred-drop
  variant (D1's alternative) becomes worth its painting cost, but that is a
  measurement, not a guess.
* **R3 — Terminals differ in whether they report `Drag` vs bare `Moved` with a
  button held.** Crossterm reports `Drag(button)` when the button is held; a
  terminal that only reports `Moved` would produce no drag. → Recognize drag
  from `Drag(Left)` only. The Queue's existing `Moved => return None` guard
  stays, so a terminal that misreports simply has no drag rather than a drag
  that fires with no button held.
* **R4 — Adding a `MouseGesture` variant touches every exhaustive match on it.**
  → That is the intended safety: the compiler lists the surfaces that must
  decide, and each gets an explicit ignoring arm rather than a wildcard, per the
  project rule against wildcard-hidden dispatch.
* **R5 — Reordering under the pointer moves the row the pointer is over, which
  can re-trigger on the next motion event.** → The move is emitted only when the
  resolved slot differs from the grabbed slot; after the move the grabbed slot
  *is* the row under the pointer, so a stationary pointer emits nothing further.
