## MODIFIED Requirements

### Requirement: Each queue has one canonical ordered representation

Every Composed or Bound queue SHALL be represented by one ordered collection of queue slots containing `QueueItem` values. A Player owner SHALL hold the only authoritative collection for its Bound queue. A Client MAY hold a replaceable snapshot of a Bound queue, and a Playback run MAY hold an mpv execution projection, but neither SHALL independently decide canonical order, active slot, revision, or queue mutation outcome. A component SHALL NOT maintain parallel item-kind collections whose synchronization is required to determine queue contents, order, length, or current slot. An mpv projection MAY contain the full playable sequence or only the active materialized file as required by source lifecycle, but that projection SHALL NOT become queue authority.

#### Scenario: Mixed queue order

- **WHEN** a queue contains interleaved Emby items, Feed entries, and Audiobookshelf podcast episodes
- **THEN** every queue operation and view SHALL observe the Player owner's canonical slot order
- **AND** no item kind SHALL be constrained to a prefix or tail

#### Scenario: Bound queue is viewed by a Client

- **WHEN** a Client displays or mutates a Bound queue
- **THEN** it SHALL use the Player owner's latest queue snapshot
- **AND** its local representation SHALL NOT become an independent queue authority

#### Scenario: Queue coordinates

- **WHEN** a queue reports its length or current position
- **THEN** both values SHALL use the canonical slot sequence regardless of how many files mpv has materialized

#### Scenario: Owner-driven active-file projection

- **WHEN** a Playback run uses owner-driven projection
- **THEN** mpv SHALL contain exactly the active materialized file while the canonical queue retains every slot
- **AND** mpv playlist position/count observations SHALL NOT resize, reorder, or reposition the canonical queue

#### Scenario: Eager mpv projection

- **WHEN** a Playback run materializes multiple canonical slots in mpv
- **THEN** the materialized entries SHALL remain an execution projection of owner-assigned slots
- **AND** mpv playlist mutation or position SHALL NOT independently redefine the Bound queue

### Requirement: Queue occurrences have stable slot identity

Each occurrence of a `QueueItem` SHALL have stable runtime slot identity independent of its provider-qualified content identity or source URL. Operations on an existing queue occurrence SHALL target its slot identity. A slot identity assigned by a Player owner SHALL be used by the Client snapshot and Playback-run projection for that occurrence. An ordinal index MAY be used only as a presentation or mpv-adapter coordinate within the component that resolved it, and SHALL NOT address a queue occurrence across a component boundary or after the queue may have mutated.

#### Scenario: Duplicate content occurrences

- **WHEN** the same `QueueItem` is appended twice
- **THEN** the queue SHALL contain two independently addressable slots

#### Scenario: Play an existing slot

- **WHEN** the user plays an item already present in the queue
- **THEN** playback SHALL select that slot
- **AND** SHALL NOT append another occurrence as a side effect

#### Scenario: Playback run reports an occurrence

- **WHEN** a Playback run reports which occurrence became active, completed, or stopped
- **THEN** it SHALL name the slot identity assigned by the Player owner
- **AND** the owner SHALL resolve that report without inferring the occurrence from an ordinal position

#### Scenario: Occurrence moves while a report is pending

- **WHEN** a queue occurrence changes ordinal position while a command or report naming it is in flight
- **THEN** the occurrence SHALL still resolve to the same slot
- **AND** no other occurrence SHALL be activated, completed, consumed, or removed in its place

### Requirement: Completion and consumption address the canonical slot

Natural completion and explicit consumption SHALL identify the affected canonical Queue slot and apply the queue's existing consume policy without branching by item kind. Content identity SHALL NOT be used to remove other occurrences. Consume SHALL be applied by the Player owner that holds the Bound queue, so the same completion produces the same queue outcome for Bare, Local daemon, and packaged `mbvd` ownership and whether or not a Client is attached.

#### Scenario: Feed slot completes naturally

- **WHEN** playback naturally completes a Feed entry whose slot is eligible for consumption
- **THEN** the Player owner SHALL consume that slot through the same slot-based queue operation used for an Emby item
- **AND** SHALL preserve any other slot containing the same Feed entry

#### Scenario: Slot is retained by policy

- **WHEN** playback completes a slot that the active consume policy retains
- **THEN** the slot SHALL remain in the canonical queue regardless of item kind

#### Scenario: Out-of-process owner consumes a completed slot

- **WHEN** a daemon Player owner completes a slot its consume policy removes
- **THEN** that owner's canonical queue SHALL no longer contain the slot
- **AND** attached Clients SHALL observe the shortened queue through the next owner snapshot

#### Scenario: Completion arrives with no Client attached

- **WHEN** a slot completes on a Player owner while no Client is attached
- **THEN** the consume policy SHALL be applied to the canonical queue
- **AND** a Client attaching afterwards SHALL observe the shortened queue

### Requirement: Bound queue state synchronizes atomically

A Player owner SHALL publish its canonical queue revision, ordered slots, observed active slot, playback status, and pending transition as one coherent snapshot. Initial connection, mutation, playback observation, transition settlement, and reconnect SHALL use that representation. A Client SHALL replace its prior Bound-queue snapshot atomically and SHALL NOT reconcile independently delivered queue and playback coordinates into a second answer.

#### Scenario: Reconnect to a mixed Bound queue

- **WHEN** a compatible Client reconnects to an owner holding a mixed queue
- **THEN** it SHALL reconstruct the same slots, order, observed active slot, playback status, and pending transition from one owner snapshot

#### Scenario: Playback changes during a queue mutation

- **WHEN** the queue revision and observed active slot both change during one owner event-loop turn
- **THEN** Clients SHALL receive values from the same resulting owner state
- **AND** SHALL NOT temporarily pair the new queue with the previous active coordinate

#### Scenario: Client receives a newer snapshot

- **WHEN** a Client receives an owner snapshot with a newer queue revision or transition state
- **THEN** it SHALL replace its previous Bound-queue snapshot
- **AND** SHALL NOT preserve an optimistic active slot from the previous snapshot

#### Scenario: Player reports a slot change

- **WHEN** mpv advances to any slot in a mixed queue
- **THEN** the Player owner and connected Client SHALL report that slot using the canonical Queue slot identity
- **AND** the observation SHALL be published with the matching queue revision and playback status

## ADDED Requirements

### Requirement: Desired and observed playback remain distinct

A request to play a Queue slot SHALL create desired transition state and SHALL NOT itself change the observed active slot. The Player owner SHALL change the observed active slot only from a Playback-run observation naming an owner-assigned slot. User-visible playback state SHALL identify observed playback separately from any pending desired slot.

#### Scenario: Slot selection is accepted

- **WHEN** a valid request selects a different Queue slot
- **THEN** the Player owner SHALL record the requested slot as pending
- **AND** the previously observed slot SHALL remain observed until the Playback run reports a transition

#### Scenario: Requested slot starts

- **WHEN** the Playback run reports the requested slot under the matching transition identity
- **THEN** the Player owner SHALL make that slot the observed active slot
- **AND** SHALL settle the request as applied

#### Scenario: Intermediate slot is observed

- **WHEN** a superseded transition briefly starts before the latest requested transition
- **THEN** the owner snapshot SHALL report that slot as observed playback
- **AND** SHALL retain the newer desired transition as pending

### Requirement: Playback transitions are serialized with latest-wins queuing

Each explicit playback transition SHALL have monotonic request identity. A Player owner SHALL dispatch at most one transition to its Playback run at a time and SHALL retain at most one undispatched transition, replacing that queued transition when a newer request arrives. A Playback-run observation SHALL carry the identity of the single dispatched transition it settles; an observation with an older identity SHALL NOT settle or overwrite a newer request.

#### Scenario: Two requests arrive before confirmation

- **WHEN** a second transition request arrives while the first is in flight
- **THEN** the first SHALL remain the sole dispatched transition
- **AND** the second SHALL become the latest queued transition

#### Scenario: Three rapid requests arrive

- **WHEN** two newer transition requests arrive while one transition is in flight
- **THEN** only the newest undispatched request SHALL remain queued
- **AND** every displaced queued request SHALL be reported as superseded

#### Scenario: In-flight transition settles after a newer request

- **WHEN** the Playback run reports the identity and target of an older in-flight transition
- **THEN** that observation MAY update observed playback
- **AND** SHALL NOT settle the newer queued request
- **AND** the Player owner SHALL then dispatch the newest queued transition

#### Scenario: Same slot is requested again

- **WHEN** requests form an A-to-B-to-A sequence before all transitions settle
- **THEN** a late observation of the first A request SHALL carry its older identity
- **AND** SHALL NOT be accepted as confirmation of the newer A request

### Requirement: Stale slot addressing is rejected, not reinterpreted

A queue command or report that names a slot the receiving component no longer holds SHALL be rejected without mutating the canonical queue. A component SHALL NOT substitute a neighbouring slot, clamp to the nearest position, or fall back to a remembered position when the named slot is absent. Client-initiated rejection SHALL be observable through the existing command-rejection path; stale internal observations SHALL be discarded without user-facing noise.

#### Scenario: Mutation and command cross in flight

- **WHEN** a slot is removed from the canonical queue while a command addressing that slot is in flight
- **THEN** the command SHALL be rejected
- **AND** no other slot SHALL be removed, moved, or activated as a result

#### Scenario: Queue shrinks beneath an in-flight report

- **WHEN** a Playback run reports a slot that the Player owner's queue no longer contains
- **THEN** the owner SHALL discard the report without changing its observed active slot
- **AND** SHALL NOT clamp the report onto the nearest surviving slot

#### Scenario: Rejected mutation is surfaced

- **WHEN** a Client's queue mutation is rejected because its target slot is gone
- **THEN** the Client SHALL be told the mutation did not apply
- **AND** the Client SHALL adopt the Player owner's current snapshot

### Requirement: Near-end completion uses one rule

The decision that playback finished close enough to the end to count as completed SHALL be evaluated by one rule applied to the completed occurrence's own runtime. Every completion path, including ordinary advance, end of queue, quit, and process shutdown, SHALL reach the same verdict for the same completed occurrence, position, and media kind.

#### Scenario: Same completion, different exit path

- **WHEN** the same occurrence at the same position ends through natural advance, quit, or owner shutdown
- **THEN** each path SHALL produce the same near-end verdict
- **AND** the same watched-state and Consume outcome SHALL follow

#### Scenario: Runtime belongs to the completed occurrence

- **WHEN** live playback status already describes the next occurrence when completion is evaluated
- **THEN** the near-end verdict SHALL use the completed occurrence's runtime
- **AND** SHALL NOT use the replacement occurrence's runtime
