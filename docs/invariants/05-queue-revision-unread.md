# Invariant 5 — `QueueRevision` is written everywhere, read nowhere

**Scope:** `QueueRevision::bump` sites (`crates/mbv-core/src/playback_queue.rs`),
`UnifiedQueueStateData.revision` (`crates/mbv-core/src/ctrl.rs:233`),
`unified_queue_state_for_peer` / `broadcast_queue_state`
(`daemon_control_queue.rs:67,101`), `PlayerTab::from_unified_state`
(`src/app/types_player_tab.rs:26`), `apply_unified_queue_state`
(`remote_player_connect.rs:390`), and every `CtrlCmd::UnifiedQueue*` handler
(`daemon_control.rs:330+`).

## The invariant (as the code implies it)

`QueueRevision` is a monotonically increasing structural clock: it bumps on
every membership/order change and is published on the wire
(`UnifiedQueueStateData.revision`) so that a receiver can distinguish "new
state" from "replay/reorder of an old broadcast" — i.e. revision must be
**monotonic per owner, preserved across snapshot round-trips, and compared
before application**. (For the bump half of the contract — including the
paths that break it — see Invariant 1 §4.)

## Why it would matter

The daemon broadcasts full queue snapshots on *every* mutation, and the
client applies each snapshot by wholesale replacement
(`from_slot_items` / `from_unified_state`: new ids, new revision, new
cursor). Without a revision check, a delayed or reordered broadcast
overwrites newer state: an item the user just removed reappears, a just-set
active slot jumps back, a consume is undone. The `active_slot` filter in
`unified_queue_state_for_peer` (drop an active id not present in the
filtered slots) only keeps a single snapshot *internally* consistent — it
does nothing against *stale* snapshots. Revision comparison is the missing
second half.

## What breaks because it is violated (i.e. absent)

Concretely, nothing compares today — so all of these are live exposures,
not hypotheticals:

- **Stale broadcast wins.** `apply_unified_queue_state`
  (remote_player_connect.rs:390) applies unconditionally: no
  `incoming.revision > current.revision` guard, and it doesn't even have the
  current revision at hand (the stored `unified_queue` snapshot is kept, but
  never compared). A reconnect snapshot racing a live broadcast, or two
  broadcasts reordered on the socket, applies last-writer-wins by arrival
  order, not by causality.
- **Round-trip preserves but nobody verifies.** `from_unified_state`
  faithfully restores `QueueRevision::from_raw(state.revision)` — the value
  survives the trip and is then never read. The only non-test reader of any
  `.revision` in the app is that constructor itself (verified by search:
  `types_player_tab.rs:38` is the sole survivor after excluding setup
  revisions, shared-doc revisions, and music-grouping revisions).
- **Daemon handlers don't need it (yet) — which hides the gap.**
  `UnifiedQueueRemoveSlot/MoveSlot/PlaySlot` re-check existence against the
  live queue (`queue.slot(sid).is_none() → reject`), so a stale command
  fails safe on identity rather than revision. That per-command safety is
  real but narrow: it protects single-slot commands, not whole-snapshot
  application, which is where the client side lives.
- **The one structural path that doesn't bump is masked by the same
  absence.** `purge_queue` rebuilds with the *same* revision (Invariant 1
  §3). Today no reader can tell — because there are no readers at all. The
  day a comparison is added, purge must bump or the comparison lies.

## How the code maintains it today

Half: the write side is diligent — every structural mutation bumps
(Invariant 1), every broadcast publishes (`unified_queue_state_for_peer`
reads `queue.revision().raw()`), every snapshot carries the value through
`broadcast_queue_state`, the reconnect snapshot (`shared_queue`), and the
ctrl wire. Tests pin bump behavior (`structural_mutations_bump_revision`,
`replace_*`, `clear_*`). The read side simply doesn't exist yet.

## Cheapest strengthening (not done here)

1. Store the last-applied revision alongside the client's adopted queue
   (`PlayerTab` or the `unified_queue` snapshot holder) and skip
   `apply_unified_queue_state` when `incoming.revision <= applied` — with a
   first-snapshot exception (`None` applies unconditionally).
2. Decide whether `QueueRevision` is per-owner-monotonic across snapshot
   round-trips: `from_slot_items` preserving the wire revision (rather than
   bumping) is correct for adoption, but any *local* mutation after adoption
   must bump from there — verify `next_slot_id` reseeding and revision
   interact sanely when two owners' id-spaces merge (cf. the colliding-id
   note in `set_queue_scope`, Invariant 1).
3. Fix `purge_queue` to bump (or document revision as membership-only and
   bump there too) *before* adding the comparison — otherwise service
   teardown broadcasts compare equal to their predecessors and a stale
   pre-purge snapshot can win over the purge.
