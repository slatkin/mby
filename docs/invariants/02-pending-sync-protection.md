# Invariant 2 — `pending_sync` must cover every accepted report, exactly once

**Scope:** `ProgressState::{local, pending_sync}`
(`crates/mbv-core/src/playback_queue.rs:84+`), `SlotProgress::from_queue_item`
/ `apply_to_item`, `PlaybackQueue::{apply_progress,
mark_progress_sync_pending, merge_refresh, merge_fetched_slot,
update_slot_item}`, the `PlayerEvent → mark_progress_sync_pending` coupling in
`src/app/player_event.rs` (`Stopped`, `TrackCompleted`), and `SessionReporter`
(`crates/mbv-core/src/player_runtime.rs:210+`, `report_stopped*`).

## The invariant

1. `local` is the **newest owner-known position** for the slot. Every
   accepted playback report (`progress_report_accepted == true`) is followed
   by `mark_progress_sync_pending(slot_id)`, snapshotting `local` into
   `pending_sync`.
2. While `pending_sync` is `Some`, a server refresh **must not overwrite**
   the slot: the slot is protected from pruning
   (`should_protect_missing_slot`) and its fetched content is either adopted
   only when the server confirms the pending position (within
   `PROGRESS_CONFIRMATION_TOLERANCE_TICKS = 3 s` *and* equal `played`), or
   held as stale-pending.
3. `pending_sync` clears **only** on server confirmation
   (`merge_fetched_slot`, pending-matches branch). Nothing else clears it.
4. `local` and `slot.item`'s embedded position agree after every mutation
   (`apply_progress` writes both; `apply_to_item` pushes `local` into the
   item; `update_slot_item` rebuilds `local` from the new item).

## Why it matters

There is an unavoidable race: the player reports `report_stopped` to Emby,
and a library refresh can land **before Emby has applied the report**. The
fetched item then carries the *old* server position. Without `pending_sync`,
`merge_refresh` would adopt the stale server row and the just-reported
progress — the user's resume point — would silently rewind. `pending_sync`
is the only thing standing between "report accepted" and "refresh arrived
early". The tolerance exists because Emby may round or clamp; the `played`
equality exists because position-within-3s with a flipped watched flag is
*not* a confirmation.

## What breaks if it is violated

- **Lost resume after stop-then-refresh.** Report accepted → refresh lands
  first → stale server position adopted → user resumes minutes behind (or at
  0 for a just-finished item whose `played` flip hadn't propagated).
- **Played-state flapping.** A near-end stop reports `played=true` at
  position 0; a stale fetch with `played=false` and old position is within
  position tolerance on position alone — the `played` conjunct is what keeps
  the merge from "confirming" it. Drop the conjunct and finished items
  un-finish.
- **Phantom protection.** If `pending_sync` is set but never cleared (no
  confirmation ever arrives, e.g. the report actually failed), the slot is
  protected from pruning **forever** — a deleted-on-server item lingers in
  the queue. The code accepts this deliberately (see below); anything that
  sets `pending_sync` speculatively widens the leak.
- **`local`/item desync.** Readers are split: some read
  `slot.progress_state.local`, others read `slot.item.playback_position_ticks`
  (e.g. `player_event.rs` falls back to the item when `position_ticks == 0`).
  If a mutation updates one but not the other, adjacent reads disagree about
  where the user is.

## How the code maintains it today

- **Set path:** both `PlayerEvent::Stopped` and `PlayerEvent::TrackCompleted`
  call `apply_progress(slot_id, position, played)` and then, **iff**
  `progress_report_accepted`, `mark_progress_sync_pending(slot_id)`. The
  flag comes from the player thread's `StopReport::is_accepted()`, so only
  reports Emby actually took (or the fire-and-forget background path
  deliberately treats as taken — `report_stop_now_or_background`,
  `player_run_queue.rs:46+`) arm protection.
- **Protect path:** `should_protect_missing_slot` = active **or**
  `pending_sync.is_some()`; `merge_refresh` routes protected slots around
  pruning, and `merge_fetched_slot` holds stale-pending slots (`stale_pending
  + protected`) instead of adopting the server row.
- **Confirm path:** `pending.matches_server_confirmation(&fetched_item)`
  checks position-within-3s **and** `played` equality; on match it clears
  `pending_sync`, adopts the fetched item, and re-applies local progress for
  the active slot.
- **Coherence path:** `apply_progress` sets `local` *and* `apply_to_item`;
  `update_slot_item` rebuilds `local` from the replacement item; `emit` paths
  read consistently per arm.
- **Failure bias:** the background-report path (`report_stop_now_or_background`,
  non-quit branch) marks `StopReport::Accepted` without knowing the outcome,
  with a comment explaining the choice: an unconfirmed `pending_sync` keeps
  the slot protected (safe: stale position retained) rather than risking a
  rewind (unsafe: fresh position lost).

## Where it currently fails

1. **`update_slot_item` drops `pending_sync` silently**
   (`playback_queue.rs:445`). It overwrites `slot.item` and rebuilds
   `local` but leaves a stale `pending_sync` (or, reading the other way,
   discards the *meaning* of the pending snapshot while keeping the flag, so
   the slot stays protected by a position that no longer describes it).
   There is no `stale_pending` accounting on this path, unlike the merge
   path. No production caller hits it today (callers are teardown/session
   paths), but the function signature promises a clean item swap and
   delivers a half-swapped progress state.
2. **No expiry on `pending_sync`.** A report the server never applies (failed
   background call, item deleted server-side mid-flight) protects the slot
   indefinitely — `merge_refresh` will carry it forever, and
   `should_protect_missing_slot` never ages out. The only exits are a later
   confirmation or a structural removal. Bounded staleness (age or
   refresh-count cap) doesn't exist.
3. **The `Accepted`-on-fire-and-forget assumption is load-bearing and
   invisible at the merge site.** `merge_fetched_slot` treats
   `pending_sync` as "the server has this (or will, within tolerance)". That
   holds only because the player thread upholds the "optimistic accept"
   policy documented 400 lines away in `player_run_queue.rs`. A future
   `report_stopped` caller that returns `true` loosely would silently widen
   phantom protection to unearned slots.
4. **`set_slot_progress_by_index` / `set_item_at` (test helpers) bypass the
   coupling.** They mutate `local`/item without touching `pending_sync`,
   which is fine for tests *as long as no merge test relies on them to set
   up pending state* — worth knowing when reading `playback_queue_tests.rs`
   setup code, not a production hole.
