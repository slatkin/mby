## 1. Drag gesture recognition

- [ ] 1.1 In `src/app/components/mouse/gesture.rs`, add a `MouseGesture::Drag { from: Position, to: Position }` variant (`from` is the press anchor, `to` the current pointer position) and a `MouseGesture::DragEnd` variant; verify with `cargo check -p mbv`.
- [ ] 1.2 In the same file add a private `drag_anchor: Option<Position>` field to `MouseGestureState`, set it in the existing `Down(MouseButton::Left)` arm (which keeps emitting `Click`/`DoubleClick` unchanged — design D2), emit `Drag` from a new `MouseEventKind::Drag(MouseButton::Left)` arm only while the anchor is set, and clear the anchor in a new `Up(MouseButton::Left)` arm that emits `DragEnd`; verify `cargo check -p mbv` passes.
- [ ] 1.3 Add unit tests in that file's existing `mod tests`: a press then `Drag(Left)` emits `Click` then `Drag` with the press anchor; a press then `Up(Left)` emits `Click` then `DragEnd` and no `Drag`; a `Drag(Left)` with no preceding press emits nothing; `Drag(Right)` emits nothing. Verify with `cargo nextest run -p mbv gesture`.
- [ ] 1.4 Add an explicit ignoring arm for `Drag`/`DragEnd` in every other `match` over `MouseGesture` the compiler now flags — never a `_` wildcard (AGENTS.md: no wildcard-hidden dispatch). Verify `cargo check -p mbv` and `cargo clippy --workspace --all-targets` are clean.

## 2. Arbitrary-destination move path

- [ ] 2.1 In `src/app/queue_actions.rs`, extract the body of `move_queue_item_by` into a new `pub(super) fn move_queue_item_to(&mut self, from: usize, to: usize)` that takes an absolute destination — keeping the slot lookup, the `apply_queue_move_by_slot` call, `retire_remote_tracking`, the `pending_remote_move_cursor` assignment, and the `UndoEntry::Move` push exactly as they are. Note there are two similarly named functions here: `apply_queue_move` (positional, pre-existing, leave it alone) and the new `move_queue_item_to`. Verify `cargo check -p mbv`.
- [ ] 2.2 Reduce `move_queue_item_by` to the ±1 clamp (its existing bounds checks) delegating to `move_queue_item_to`; `move_queue_item_up`/`move_queue_item_down` keep their current signatures and callers. Verify the pre-existing suite still passes: `cargo nextest run -p mbv tests_queue_reorder`.

## 3. Request type and shell dispatch

- [ ] 3.1 In `src/app/components/msg/queue.rs` add `QueueRequest::MoveTo { scope, slot_id, onto }` where `onto` is a `QueueSlotId` (design D4). Leave the existing `Move { .. }` variant and `QueueMove` enum untouched. Verify `cargo check -p mbv`.
- [ ] 3.2 In `src/app/shell_queue.rs` add the dispatch arm for `MoveTo`: resolve `slot_id` through the existing `select_queue_slot(scope, slot_id)` to get `from`, resolve `onto` to its index in the same scope's queue, and call `move_queue_item_to(from, to)`. Do nothing when either identity no longer resolves, or when `from == to`. Verify `cargo check -p mbv`.
- [ ] 3.3 Add a shell test alongside `src/app/tests_queue_reorder.rs` asserting that a `MoveTo` from index 0 onto index 2 produces the same item order as two `move_queue_item_down` calls, and pushes undo entries on the same stack. Verify `cargo nextest run -p mbv tests_queue_reorder`.

## 4. Queue component drag interpretation

- [ ] 4.1 In `src/app/components/queue.rs` add a private `drag_grab: Option<QueueSlotId>` field, armed in the existing `MouseGesture::Click` arm from the value `claim_slot(at)` already returns (design D6). Verify `cargo check -p mbv`.
- [ ] 4.2 Add the `MouseGesture::Drag { from: _, to }` arm to `handle_mouse`: return `None` unless `drag_grab` is set; resolve `to` via `self.list.resolve_point(self.area, to)`; return `None` when it resolves to nothing or to the grabbed slot itself (design R5); otherwise emit `Msg::Queue(QueueRequest::MoveTo { scope: self.scope, slot_id: grabbed, onto: resolved })` and select the grabbed slot so it stays highlighted. Add a `MouseGesture::DragEnd` arm that clears `drag_grab` and returns `None`. Verify `cargo check -p mbv`.
- [ ] 4.3 Add component tests in `src/app/components/queue_component_tests.rs` modelled on the existing click tests: press on row 0 then drag to row 2 emits `MoveTo` with the two slot ids; a drag onto blank space past the last row emits nothing and a subsequent drag back onto a row still emits; a drag with no preceding press emits nothing; `DragEnd` then a drag emits nothing. Verify `cargo nextest run -p mbv queue_component`.

## 5. Documentation and gates

- [ ] 5.1 Update the Queue row of `docs/architecture/interactive-surface-ledger.md` to record drag-to-reorder among its verified mouse gestures, citing the tests from 4.3. Verify by reading the row back.
- [ ] 5.2 Run `cargo fmt`, then `cargo clippy --workspace --all-targets`, `cargo nextest run -p mbv`, `ast-grep scan`, and `make check-code-file-lines`; all must be clean. If `queue.rs` or `gesture.rs` crosses the 800-line cap, split it before finishing (project rule: never open a PR with a governed file over cap).
- [ ] 5.3 Manually verify in a real terminal at the Normal and Wide breakpoints: drag a queue entry up and down, drag the currently playing entry and confirm playback continues on it, and confirm `Ctrl+Z` walks the drag back. Record the result in the change before archiving.
