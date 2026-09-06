Standing verification for each Rust task: run `cargo check -p mbv-core` for core-only edits or `cargo check -p mbv` when `src/` changes. Use the existing test suite first; add or extend one test only where the task names a regression that existing coverage cannot prove. Run `cargo fmt` after every Rust work group and accept its complete reflow.

## 1. Establish owner state and snapshot contracts

- [ ] 1.1 Extend the owner snapshot type from the existing unified queue state so one value carries queue revision, ordered slots, observed active slot, playback status, queue source, and pending transition summaries; verify serialization round-trips every field and retains existing QueueItem capability gating.
- [ ] 1.2 Define the minimal transition state using existing `PlaybackRequestId` and `PlaybackGeneration`: one `in_flight` transition and one `queued_latest` transition, each targeting `QueueSlotId`; verify an A-to-B-to-A sequence retains distinct request identities.
- [ ] 1.3 Change queue-addressing `PlayerCommand` variants and transition-bearing commands at their source-of-truth definitions to carry owner `QueueSlotId` and request identity instead of `usize`; verify compile errors enumerate all callers before updating them.
- [ ] 1.4 Change `PlayerEvent::TrackChanged`, `TrackCompleted`, and `Stopped` at their source-of-truth definitions to report owner `QueueSlotId`, with transition observations carrying the dispatched request identity; verify no queue-addressing event field remains a bare `usize`.
- [ ] 1.5 Remove unused queue-addressing `WireCommand` variants and their conversions while preserving transport commands and `CTRL_PROTOCOL_VERSION`; verify `rg -n 'WireCommand::(JumpTo|QueueAppend|QueueRemove|QueueMove|ReplaceQueue|LoadNew)' crates/ src/` has no non-test sender.

## 2. Make the Playback run an execution projection

- [ ] 2.1 Change queue submission and append paths to pass owner-assigned `(QueueSlotId, QueueItem)` pairs into the Playback run; verify duplicate QueueItems retain distinct owner ids through submission and append.
- [ ] 2.2 Replace `PlaybackRun`'s canonical `PlaybackQueue` storage with the smallest slot-bearing execution sequence needed for mpv projection, retaining current eager-playlist and active-file loading behavior; verify existing playback-session and playlist tests pass unchanged.
- [ ] 2.3 Resolve mpv playlist positions to `QueueSlotId` inside the Playback run immediately before emitting events, and resolve incoming slot commands to mpv-local positions immediately before commands; verify moving a slot while an event is pending never changes the event's slot identity.
- [ ] 2.4 Remove Playback-run slot allocation, queue revision ownership, `refresh_current_idx_from_queue`, and fallbacks from missing slot identity to `current_idx`; verify no Playback-run path can create a canonical Queue slot.
- [ ] 2.5 Emit transition observations with the sole dispatched request identity and emit natural advancement without one; extend the existing rapid-jump regression test to prove A-to-B-to-A cannot confirm the final A from the first A observation.

## 3. Centralize Player-owner transitions and queue mutation

- [ ] 3.1 Centralize Bound-queue mutation, observed active slot, and transition coordination in owner state used by Bare, Local daemon, and packaged `mbvd` paths; verify each owner kind reaches the same mutation functions rather than mutating a Client snapshot.
- [ ] 3.2 Implement one-in-flight dispatch: dispatch immediately only when no transition is in flight, otherwise retain only the newest queued request and emit `Superseded` for the displaced queued request; verify three rapid requests send one Player command initially and retain one queued request.
- [ ] 3.3 Settle only a Playback-run observation matching both the in-flight request identity and target slot, then dispatch `queued_latest`; verify an older observation can update observed playback but cannot settle or overwrite the newer request.
- [ ] 3.4 Add bounded in-flight failure handling that rejects the stalled request, restores the execution projection from canonical owner state, and dispatches the latest queued request; verify a missing mpv confirmation cannot leave transition state permanently blocked.
- [ ] 3.5 Remove ordinal `target_idx` matching from `PlaybackIntentState`, daemon index clamping, and missing-slot neighbour fallback; verify stale Client commands reject visibly while stale internal reports leave canonical queue and observed slot unchanged.

## 4. Make Clients consume owner snapshots

- [ ] 4.1 Publish the owner snapshot after canonical queue mutation, playback observation, transition settlement, initial connection, and reconnect; verify each publication is built after all changes from that owner event-loop turn.
- [ ] 4.2 Update ctrl Clients to replace their Bound-queue snapshot atomically instead of merging separate queue and `PlayerStatus.current_idx` updates; verify reconnect during playback yields matching queue revision, slots, observed slot, and status.
- [ ] 4.3 Route Bare mode through the same owner snapshot projection without serializing it or introducing a second in-process transport abstraction; verify Bare selection follows accepted, observed, and applied states in the same order as a daemon owner.
- [ ] 4.4 Remove `PlayheadProjection`, `reconcile_playhead`, `pending_push`, prediction reasons, and shell-side optimistic Bound-queue activation; verify a selected slot displays as pending while the previous slot retains its own progress until observation.
- [ ] 4.5 Keep queue-panel cursor, scroll, and Composed queue state Client-owned and separate from owner snapshots; verify ordinary cursor movement sends no Player command and survives an unrelated playback snapshot.

## 5. Consolidate completion behavior and verify

- [ ] 5.1 Collapse queue-start lifecycle initialization shared by queue submission and replacement, then delete the redundant `ReplaceQueue` path; verify `stop_report`, load state, status projection, and reporter initialization each have one queue-start write site plus the distinct Standalone path.
- [ ] 5.2 Replace duplicate near-end calculations with the existing helper using the completed slot's runtime; verify the existing helper test covers ordinary advance, quit, and shutdown decisions without adding full-session fixtures.
- [ ] 5.3 Move Consume to Player-owner `TrackCompleted` handling keyed by `QueueSlotId`, and remove Client-side out-of-process removal plus `pending_queue_removal`; verify completion with no Client attached shortens the canonical queue seen by a later Client.
- [ ] 5.4 Run `cargo nextest run --workspace && cargo clippy --workspace --all-targets && cargo fmt --all -- --check && ast-grep scan && make check-code-file-lines`; fix every regression before marking the change ready.
- [ ] 5.5 Manually exercise Bare, Local daemon, and packaged `mbvd` ownership with rapid A-to-B-to-A selection and a queue move/removal during transition; verify the displayed observed slot, pending target, completed slot, and final queue remain correct in all three.
