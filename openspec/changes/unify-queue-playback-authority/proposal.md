## Why

A Bound queue is currently copied into the Client, Player owner, and Playback run, while active playback is represented again by indices and optimistic shell state. Rapid commands and queue mutations therefore reconcile unrelated snapshots instead of confirming one authoritative transition, which repeatedly selects or reports the wrong Queue slot.

## What Changes

- A Player owner becomes the sole authority for its Bound queue, observed active slot, and transition lifecycle. A Client holds a replaceable snapshot; a Playback run holds only the mpv execution projection needed to play the owner's slots.
- Playback selection separates desired state from observed state. The active slot changes only after the Playback run reports that exact owner-assigned `QueueSlotId`.
- Playback transitions are serialized as one in-flight request plus one latest-wins queued request. Every request has monotonic identity; a late observation can settle only the transition that produced it.
- Bound queue slots, observed playback state, pending transition, and queue revision are published as one coherent owner snapshot for Bare, Local daemon, and packaged `mbvd` ownership.
- Queue commands and Playback-run events address occurrences by `QueueSlotId`, stale addresses are rejected rather than clamped, and unused index-addressed wire commands are removed.
- Duplicate queue-start lifecycle code and near-end rules are collapsed; Consume moves to the Player owner.
- Existing mpv loading and playlist behavior is preserved. This change does not redesign media preparation or playback continuity.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `unified-playback-queue`: Make the Player owner the sole Bound-queue and playback-transition authority, publish coherent snapshots, and replace optimistic/index-based reconciliation with confirmed slot-addressed transitions.

## Impact

- `crates/mbv-core/src/`: owner state and ctrl snapshots, `PlayerCommand` / `PlayerEvent`, Playback-run queue projection and lifecycle, queue mutation and Consume handling.
- `src/app/`: Bound-queue snapshot adoption, playback requests, Player-event handling, and removal of optimistic Playhead reconciliation.
- Ctrl queue operations remain capability-gated. Persisted `QueueState` and current mpv playlist behavior remain unchanged.
