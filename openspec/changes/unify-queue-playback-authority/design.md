## Context

See `proposal.md` - Why. The current system represents one Bound queue and its playback position in several independently mutable forms:

- `PlayerTab.queue` is authoritative in Bare mode but becomes a Client-side copy for an out-of-process Player owner.
- `daemon_run` owns another `PlaybackQueue` and resolves Client slot commands into indices before sending them onward.
- `PlaybackRun` constructs a third `PlaybackQueue` with unrelated slot ids and mirrors its active slot through `current_idx`, `PlayerStatus.current_idx`, `forced_slot_id`, and mpv playlist properties.
- The shell predicts active playback in `PlayheadProjection` and later treats an equal index and queue length as confirmation. One prediction and one pending cursor push are overwritten by rapid actions.
- `PlaybackIntentState` already supplies request identity and outcomes for direct playback, but stores an ordinal `target_idx` and permits more than one transition to reach mpv before observation establishes which transition produced an event.

The existing mpv playlist and active-file projection behaviors are working constraints. This change alters their control and identity boundaries, not media loading behavior.

## Goals / Non-Goals

**Goals:**

- One canonical `PlaybackQueue` for each Player owner.
- One transition coordinator shared by Bare, Local daemon, and packaged `mbvd` ownership.
- Stable slot identity and monotonic transition identity across the owner/Playback-run boundary.
- One coherent snapshot consumed by Clients without optimistic reconciliation.
- Deletion of duplicate queue authority, index clamps, and overwrite-prone pending fields.

**Non-Goals:**

- Changing mpv playlist construction, source preparation, or playback continuity.
- Changing Composed queue ownership or ordinary queue-panel cursor and scroll state.
- Persisting runtime slot or transition ids.
- Bringing Cast dispatch or Session watch under Player-owner queue semantics.
- Reorganizing the `player.rs` `include!` chain.

## Decisions

### D1: The Player owner holds the only canonical Bound queue

The owner event loop holds one `PlaybackQueue` containing canonical order, slot identity, revision, and observed active slot. In Bare mode the shell hosts this owner state; in Stay-alive and remote-control paths the daemon hosts it. Queue mutations enter this state through the same semantic owner operations before process-specific transport is considered.

A Client's `PlayerTab.queue` is a replaceable Bound-queue snapshot whenever another process owns playback. It does not predict mutation success. `PlaybackRun` receives owner-assigned slots as an execution sequence such as `Vec<(QueueSlotId, QueueItem)>`; it does not construct another canonical `PlaybackQueue`, allocate slot ids, or own a queue revision.

*Why:* preserving three instances of the same domain type preserves three claims to authority. A smaller execution projection lets mpv keep the information it needs without inheriting canonical queue behavior.

*Alternative rejected:* retain `PlaybackRun.queue` but construct it with owner ids. This fixes occurrence identity but leaves order, active position, and mutation outcome duplicated, which is the explicit non-goal of the previous design and the source of later reconciliation work.

### D2: Existing mpv projection behavior remains unchanged

The execution projection may eagerly materialize the playable sequence or materialize only the active file for lifecycle-backed sources, exactly as today. It maps owner `QueueSlotId` values to mpv-local coordinates and resolves mpv observations back to slot identity before they leave the Playback run.

mpv playlist indices remain private adapter coordinates. Queue operations may resolve a slot to an mpv coordinate immediately before issuing an mpv command, but neither commands crossing into the Playback run nor events leaving it carry a bare queue index.

*Why:* changing source loading is unnecessary. The defect is authority and correlation, not playlist playback.

### D3: Desired transition and observed playback are separate owner state

The owner stores:

```text
observed_active_slot: Option<QueueSlotId>
in_flight: Option<Transition>
queued_latest: Option<Transition>
```

`Transition` reuses the existing `PlaybackRequestId` and `PlaybackGeneration` and carries its target `QueueSlotId`. Accepting a request changes desired transition state only. `observed_active_slot` changes only when a Playback-run event names an existing canonical slot.

The Client renders observed playback from the owner snapshot. A pending target may be shown as starting, but it is never substituted for observed playback and never receives the previous slot's progress.

*Why:* an intention is not evidence that mpv changed files. Keeping both values removes the need for `PlayheadProjection`, prediction reasons, and equality-based tick reconciliation.

*Alternative rejected:* continue optimistic activation and add an epoch. Epochs prevent stale confirmation but do not stop the UI and canonical queue from claiming an unobserved slot is playing.

### D4: At most one transition is dispatched to the Playback run

When there is no in-flight transition, the owner dispatches the accepted request and records it as `in_flight`. While it remains in flight, a newer request replaces `queued_latest`; the replaced queued request receives `Superseded`. The owner does not send the queued request to mpv until the in-flight request settles or fails.

A Playback-run transition event carries the dispatched request identity and observed slot. It can settle only the matching `in_flight` transition. After settlement, the owner dispatches `queued_latest`, if present. Natural advancement carries no request identity and cannot settle an explicit request unless the owner deliberately converts the matching observation into that request's result.

*Why:* mpv does not echo application epochs. Sending several jumps and reconstructing causality from coalesced playlist events is inherently ambiguous, especially for A-to-B-to-A requests. One in-flight command makes correlation factual; one queued latest request provides explicit latest-wins behavior without an unbounded command backlog.

*Alternative rejected:* keep a queue of several dispatched transitions and match observations by target slot. Repeated targets make that matching ambiguous when mpv suppresses intermediate transitions.

### D5: One owner snapshot crosses every Client boundary

Extend the existing unified queue state into one snapshot containing:

- canonical queue revision and ordered slots;
- observed active slot;
- playback active/paused/position/runtime state;
- optional in-flight and queued-latest request summaries;
- queue source.

The owner builds and publishes the snapshot after each event-loop mutation. Ctrl connection, mutation, playback observation, transition outcome, and reconnect use the same shape. Bare mode feeds the same shape directly to its shell projection without serialization. A Client atomically replaces its previous Bound-queue snapshot and does not merge a separate `PlayerStatus.current_idx` into it.

*Why:* stable identities still tear if queue and playback state arrive independently. One snapshot provides one revision boundary and removes shell-side reconciliation.

*Alternative rejected:* preserve separate queue and status events with matching revision numbers. That requires buffering and joining two streams to recreate a snapshot the owner already had.

### D6: Stale identity is rejected; it is never repaired by position

Client commands naming absent slots use the existing command-rejection path and return the current owner snapshot. Playback-run reports naming absent slots are discarded and logged without a user-facing rejection. The daemon index clamp and every fallback from missing slot identity to current or neighbouring index are removed.

Unused queue-addressing `WireCommand` variants are removed. Current Clients already use unified queue commands and playback intents, so the in-process `PlayerCommand` and `PlayerEvent` types can carry slot and transition identity without changing `CTRL_PROTOCOL_VERSION`.

*Why:* a stale slot contains no evidence about which surviving slot was intended.

### D7: Lifecycle and Consume follow owner identity

The existing queue-start paths share one lifecycle initializer. Every completion path calls the existing near-end helper with the completed slot's own runtime. `TrackCompleted` names the completed slot, and the Player owner applies Consume before publishing its next snapshot. Client code retains presentation and Service-state reactions but does not mutate an out-of-process Bound queue.

*Why:* these are existing duplicate decisions exposed by the authority change. Consolidating them prevents owner kinds from producing different queue outcomes.

## Risks / Trade-offs

- **Serializing transitions can delay the newest request until mpv reports the in-flight transition.** -> Keep the in-flight timeout bounded; on timeout reject that transition, rebuild the execution projection from canonical owner state, then dispatch the latest queued request.
- **Atomic snapshots are larger than separate status events.** -> Publish immediately on structural and transition changes; retain the existing lightweight cadence for position-only updates if measurement shows snapshot traffic matters, but position updates must name the same observed slot and queue revision.
- **Bare mode currently combines Client and owner responsibilities in the shell.** -> Centralize owner mutation and transition state first, then make the shell consume its snapshot; do not build a second transport abstraction for the in-process path.
- **Removing PlaybackRun's `PlaybackQueue` may expose helper methods coupled to that type.** -> Move only owner semantics out; retain small local functions for slot-to-mpv-coordinate lookup rather than introducing another queue model.
- **Cross-version peers may still send removed wire variants.** -> Reject them at the ctrl compatibility boundary; do not translate them back into index-addressed internal commands.

## Migration Plan

1. Add the owner snapshot and transition fields while existing readers still compile.
2. Change source-of-truth command and event types to slot and transition identity, then update Playback-run emitters.
3. Route owner mutation and serialized transition dispatch through the shared owner state for all owner kinds.
4. Switch Clients to atomic snapshot adoption and remove optimistic playhead reconciliation.
5. Remove the Playback-run canonical queue, index clamps, duplicate lifecycle paths, and Client-side Consume.

No persisted data migration is required. Runtime ids remain ephemeral. Rollback is a code revert; no on-disk shape changes.
