## Why

The Queue can already be reordered from the keyboard with `Shift+Up` /
`Shift+Down`, but there is no pointer equivalent — the only way to move a track
with the mouse is not to. Every other Queue keyboard action (select, activate,
scope switch, context menu) already has a pointer equivalent, so reorder is the
outlier, and `mouse-input` names pointer parity with keyboard actions as the
standard a migrated surface must meet.

The move machinery itself already exists and is already arbitrary-distance:
`apply_queue_move_by_slot` corrects the playhead, marks the queue dirty,
persists, and prefers the slot-based unified protocol send. Only its
single-step caller and the gesture recognizer stand between today and
drag-to-reorder.

## What Changes

- `MouseGestureState` recognizes a **drag**: a left-button press arms a drag
  anchor, subsequent `Drag(Left)` events emit a drag gesture carrying the anchor
  and the current position, and `Up(Left)` disarms. Today `Drag`, `Up` and
  `Moved` are all dropped.
- The Queue interprets a drag over its list as **live reorder**: each time the
  pointer crosses into a different row, the grabbed slot moves to that row's
  position, so the row visibly travels under the pointer. No deferred drop, no
  insertion indicator, no drag ghost.
- A new Queue request variant carries an **arbitrary** destination by slot
  identity (`slot_id` grabbed, `onto` slot dropped on), matching the existing
  rule that Queue requests never carry a snapshot index.
- The shell resolves both identities to indices at dispatch and calls the
  existing arbitrary-distance move path. `Shift+Up`/`Shift+Down` keep their
  single-step request variant unchanged.
- Undo (`Ctrl+Z`) covers a drag, one row-crossing per undo entry, because
  `UndoEntry::Move` is already distance-agnostic.

Not in this change: multi-select, multi-select drag, dragging out of the Queue
to remove, and any drop-indicator painting.

## Capabilities

### New Capabilities

None. Both affected behaviours belong to existing capabilities.

### Modified Capabilities

- `mouse-input`: gesture recognition gains a drag gesture, and the Queue row in
  the mouse-parity contract gains drag-to-reorder as a verified gesture.
- `queue-canonical-list`: the Queue surface gains pointer-driven reorder over
  its canonical list rows, resolved through the embedded control's point
  resolution.

## Impact

- `src/app/components/mouse/gesture.rs` — drag anchor state and a new gesture
  variant. Every other consumer of `MouseGesture` must stay unaffected by the
  new variant (exhaustive matches gain an ignoring arm or a real one).
- `src/app/components/queue.rs` — grabbed-slot state, a drag arm in
  `handle_mouse`.
- `src/app/components/msg/queue.rs` — the arbitrary-destination request variant.
- `src/app/shell_queue.rs` — dispatch arm resolving both slot identities.
- `src/app/queue_actions.rs` — an arbitrary-destination entry point that the
  existing single-step mover delegates to.
- `docs/architecture/interactive-surface-ledger.md` — the Queue row's mouse
  gesture list.
- No change to `mbv-core`, the ctrl protocol, persistence, or the daemon:
  `PlaybackQueue::move_slot` and `queue_move_slot` already take an arbitrary
  destination index.
