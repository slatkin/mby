# Invariant 1 — Slot identity is stable; position is not

**Scope:** `PlaybackQueue` (`crates/mbv-core/src/playback_queue.rs`) and its
two index-translation clients: the app model (`src/app/types_player_tab.rs`,
`src/app/queue_scope.rs`, `src/app/player_event.rs`) and the daemon/player
mirror (`crates/mbv-core/src/daemon_control.rs`,
`crates/mbv-core/src/player_run_*.rs`).

## The invariant

1. Every queued item lives in a slot with a `QueueSlotId` that is **unique
   for the life of that `PlaybackQueue`** (monotonic `next_slot_id`
   allocator, `allocate_slot_id`, playback_queue.rs:532).
2. Reorder, refresh-merge, prune, and consume **move slots, never renumber
   them**. Code that means "the same item" must hold the `QueueSlotId`, not
   the `usize` index.
3. `active_slot_id` is always `None` or resolves to a live slot
   (`active_slot()` / `active_index()` return `None` on a dangling id;
   constructors and every removal path re-anchor or clear it).
4. `QueueRevision` advances on **every structural change** (membership or
   order) and on **no non-structural change**, so a revision comparison can
   answer "did the queue shape change?" (see also Invariant 4 for why the
   second half of this contract currently has no reader).

## Why it matters

Three subsystems translate between slots and positions continuously:

- mpv speaks **playlist index** (`playlist-pos`, `playlist-remove`,
  `playlist-move`, `JumpTo(idx)`, `QueueRemove(idx)`).
- `PlayerEvent`s carry **raw mpv indices** (`Stopped { idx }`,
  `TrackCompleted { idx }`, `TrackChanged(idx)`).
- The TUI cursor (`PlayerTab::queue_cursor`) and the daemon wire protocol
  (`UnifiedQueueSlot { slot_id, item }`, `active_slot`) mix both
  coordinates.

If identity ever collapses to position, a remove/move/refresh that lands
between "event emitted" and "event handled" applies progress, consume, or
`mark_played` to the **wrong item**: resume positions corrupt, the wrong slot
is consumed, or the completed slot is reported played for its neighbour.
A dangling `active_slot_id` is the same bug in miniature — `active_slot()`
returns `None` and the player advances from a stale `current_idx`.

## What breaks if it is violated

- **Progress applied to the wrong slot.** `handle_player_event` resolves
  `idx → slot_id` immediately (`player_event.rs`, `Stopped` /
  `TrackCompleted` arms) precisely so later consume/removal can't shift the
  target. Index-arithmetic instead of that resolution would misattribute
  position when a consume lands first.
- **Advance from a removed slot.** `on_end_file` resolves the completed slot
  by `active_slot_id → slot_index` with an H11 bounds-check fallback that
  stops playback rather than advancing from garbage
  (`player_run_events.rs:383+`). Without the check, `completed_idx` could
  index past the shrunken list.
- **Cursor parked on an unrelated slot.** The local and remote `PlayerTab`
  queues each allocate slot ids from 1, so ids **collide across scopes**.
  `set_queue_scope` documents this and re-anchors to the new scope's own
  follow position instead of reconciling by identity (`queue_scope.rs:305+`).
  Any future cross-scope `slot_id` comparison reintroduces the collision.
- **Stale-queue detection blind.** Anything comparing `revision` to skip work
  misses changes that didn't bump (see below), or does redundant work on
  changes that bumped without reshaping.

## How the code maintains it today

- **Allocation:** `insert`/`append`/`replace`/`from_queue_items` allocate via
  `allocate_slot_id`; `from_slot_items` preserves owner-assigned ids and
  reseeds `next_slot_id` above the max, so a snapshot round-trip
  (`daemon_reconciliation::purge_queue`, `PlayerTab::from_unified_state`)
  cannot reissue a live id.
- **Active anchoring:** `from_slot_items` filters a dangling
  `active_slot_id`; `merge_refresh` and `truncate_slots` clear-or-keep the
  active id after reshaping; `remove_existing_slot` (the `consume_slot`
  engine) re-anchors to the next slot or the last; `remove_slot` refuses the
  active slot outright (`RequiresActiveConfirmation`) so explicit user
  removal must go through `remove_active_slot_confirmed` (which clears).
- **Index→identity at every boundary:** `TrackChanged` resolves the incoming
  mpv index to a slot **before** draining the deferred consume, then
  re-resolves the post-removal position by identity
  (`player_event.rs`, `TrackChanged` arm); `merge_refreshed_queue` snapshots
  pre-refresh `slot_id → index`, prunes by identity, and emits
  `QueueRemove` for the recorded indices in descending order
  (`queue_scope.rs:188+`); `JumpTo` pins `forced_slot_id` before asking mpv
  to move so the resulting `playlist-pos` event can be attributed.
- **Bump discipline (mostly):** `insert`/`append`/`replace`/`clear`/
  `truncate_slots`/`move_slot`/`remove_active_slot_confirmed`/
  `remove_existing_slot`/prune-during-`merge_refresh` all call
  `revision.bump()`. Tests pin this (`structural_mutations_bump_revision`,
  `clear_*`, `replace_*`).

## Where it currently fails

1. **`set_active_slot` / `clear_active_slot` do not bump.**
   Defensible (cursor moves aren't structural), but it means "revision equal"
   does **not** imply "same playback position" — any future staleness check
   on revision alone will miss pure-active changes.
2. **`apply_progress`, `mark_progress_sync_pending`, `update_slot_item`
   do not bump.** Same shape: content/progress changes are invisible to
   revision. `update_slot_item` additionally resets `progress_state.local`
   from the new item, which is correct, but a revision reader can't tell it
   happened.
3. **`purge_queue` (`daemon_reconciliation.rs:29`) rebuilds via
   `from_slot_items` with the *same* revision.** Membership changed, revision
   didn't. This is the one structural path that breaks rule 4 outright; it
   is masked today only because nothing compares revisions (Invariant 4).
4. **`slots_mut()` (`playback_queue.rs:269`) escapes the discipline
   entirely.** The doc comment says "prefer the explicit mutation methods",
   but the accessor hands out `&mut [QueueSlot]`, so any caller can reorder,
   replace, or mutate items with no bump and no progress-state fixup. The
   existing abuser is test-only (`PlayerTab::set_item_at` swaps `slot.item`
   **without** resetting `progress_state`, unlike `update_slot_item` which
   does) — so today the incoherence is confined to tests, but the API
   invites a production caller to desync `slot.item` from
   `slot.progress_state.local` with no compiler complaint.
5. **Two active-removal semantics coexist.** `consume_slot` re-anchors active
   to next-or-last; `remove_active_slot_confirmed` clears to `None`. Each
   call site chose deliberately (auto-consume vs explicit removal), yet the
   difference lives only in which function was called — a new caller picking
   the wrong one parks or drops playback continuity silently.

## Cheapest strengthening (not done here)

- Delete `slots_mut()` or gate it `#[cfg(test)]`; route `set_item_at`
  through `update_slot_item`.
- Decide the revision contract explicitly: either bump on active/progress
  changes, or document that revision covers membership/order only — and then
  bump in `purge_queue`, the one path that violates even the narrow reading.
