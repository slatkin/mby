## Context

See `proposal.md` - Why. `QueueItem` has two Audiobookshelf shapes: episode-shaped `Audiobookshelf` and book-shaped `AudiobookshelfBook`. Shared classification already exposes `is_audiobookshelf_any()`, but several queue, Player, and Service-lifecycle paths call the episode-only predicate or match only the episode variant.

The two shapes must remain distinct at source preparation, transport capability, identity, and progress-reporting boundaries. They are equivalent only where the question is whether an item belongs to the Audiobookshelf Service or requires owner-driven active-file projection.

This change precedes `unify-queue-playback-authority`. It must not absorb that change's slot-identity, queue-start lifecycle, Consume, revision, or stop-report work.

## Goals / Non-Goals

**Goals:**

- Use one existing classification predicate at every shared Audiobookshelf boundary.
- Preserve book slots during Emby refresh.
- Select admission and active-file projection before book source preparation.
- Purge and finalize books wherever Audiobookshelf-owned state is cleared.
- Report a naturally completed book at runtime.

**Non-Goals:**

- Unifying the episode and book data shapes or playback-session protocols.
- Changing `abs-queue` and `abs-book-queue` transport capability gates.
- Changing slot identity, queue revision semantics, `pending_sync`, Consume ownership, or queue-start choreography.
- Adding UI, wire-format, persisted-format, or dependency changes.

## Decisions

### D1: Reuse `is_audiobookshelf_any()` at shared boundaries

Replace episode-only classification with the existing combined predicate where the behavior applies to the Audiobookshelf Service as a whole: owner admission, active-file projection, append/replace routing, teardown, and natural-completion finalization.

*Why:* the helper already expresses the domain distinction. Expanding each match manually would repeat the same bug-prone shape list.

*Alternative rejected:* merge books and episodes into one `QueueItem` variant. Their identities, source preparation, transport capabilities, and progress protocols are intentionally different.

### D2: Preserve non-Emby slots before Emby identity lookup

The refresh merge shall route Feed entries and both Audiobookshelf shapes around the Emby lookup. It shall retain their slot identity, order, item snapshot, and progress state unchanged.

*Why:* the refresh input contains only Emby items, so absence from that input carries no information about another Service's slots.

*Alternative rejected:* teach the Emby lookup about Audiobookshelf identities. That crosses Service boundaries and still cannot refresh those items from Emby data.

### D3: Keep capability gates shape-specific

Shared Service admission may use combined Audiobookshelf classification, but ctrl serialization continues to distinguish podcast and book capabilities.

*Why:* `abs-queue` and `abs-book-queue` are separate compatibility contracts. Queue parity must not broaden what an older peer can decode.

### D4: Fix teardown at each existing ownership boundary

Update the existing daemon reconciliation, persisted-state filtering, active lifecycle finalization, and interactive-process cleanup sites rather than introducing a new purge abstraction.

*Why:* these sites own different state and side effects. A new generic teardown layer would enlarge a predicate correction into architecture work.

### D5: Pin failures with existing test fixtures

Extend the narrowest existing queue and daemon tests, reusing the current Audiobookshelf book fixture. Cover one realistic regression per distinct boundary: Emby refresh retention, Service purge, owner admission/projection, and natural completion.

*Why:* these defects were missed because tests exercised episode-shaped items at shared boundaries. Broad playback fixtures or UI tests would add cost without stronger evidence.

## Risks / Trade-offs

- **A combined predicate is applied at a shape-specific transport boundary.** -> Keep existing podcast/book capability checks unchanged and review each replacement by its semantic question.
- **A teardown site remains episode-only.** -> Search every non-test `is_audiobookshelf()` call and `QueueItem::Audiobookshelf(_)` match, then classify each as shared or deliberately shape-specific.
- **The fix overlaps the queued authority change.** -> Land this change first; keep lifecycle consolidation and slot-identity edits in `unify-queue-playback-authority`.

## Migration Plan

No data migration is required. Apply the classification and lifecycle corrections, run focused package tests, then run the workspace gate. Rollback is a code revert; no serialized representation changes.
