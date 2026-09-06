## Why

Audiobookshelf books are a distinct `QueueItem` shape, but several shared queue and playback paths still recognize only podcast episodes. As a result, a book can be pruned during an Emby refresh, routed through the wrong mpv projection, survive Audiobookshelf Service teardown, fail admission late, or close below its natural-completion position.

Fixing this parity gap before broader queue-authority work keeps known data-loss and normal-action failures out of that larger change.

## What Changes

- Treat both Audiobookshelf queue-item shapes identically at shared classification boundaries while preserving their distinct source-resolution and progress-reporting behavior.
- Preserve book slots during Emby-only queue refreshes.
- Route book submissions and appends through owner admission and active-file projection, including clean refusal when the Player owner lacks Audiobookshelf context.
- Remove books from live and persisted queues during Audiobookshelf Service replacement or removal and finalize any active book lifecycle first.
- Close naturally completed books at their runtime so Audiobookshelf records completion reliably.
- Remove the unused episode-only queue-slot predicate.
- Add the smallest regression coverage for refresh, admission/projection, teardown, and natural completion.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `audiobookshelf-book-playback`: Require book queue submission, active-file projection, admission failure, and natural completion to follow the intended book lifecycle.
- `unified-playback-queue`: Require shared queue refresh and item-kind-agnostic operations to preserve Audiobookshelf books rather than treating them as Emby items.
- `audiobookshelf-service-setup`: Require Audiobookshelf replacement and removal to purge both podcast episodes and books from live and persisted Service-owned queue state.

## Impact

- Queue classification and refresh in `crates/mbv-core/src/playback_queue.rs` and `config_state.rs`.
- Player admission, projection, append/replace routing, and lifecycle completion in `crates/mbv-core/src/player_runtime_controller.rs` and `player_run_*.rs`.
- Audiobookshelf owner reconciliation in `crates/mbv-core/src/daemon_reconciliation.rs`.
- Interactive-process Audiobookshelf teardown in `src/app/audiobookshelf_service_actions.rs`.
- No wire-format, persisted-format, dependency, or UI changes.
