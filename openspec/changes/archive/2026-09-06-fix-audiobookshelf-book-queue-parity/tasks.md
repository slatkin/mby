## 1. Fix shared queue classification

- [x] 1.1 Preserve both Audiobookshelf `QueueItem` shapes in `PlaybackQueue::merge_refresh` before the Emby identity lookup, and extend the existing refresh test to prove an inactive book retains its slot, order, and progress state.
- [x] 1.2 Make `QueueState::without_audiobookshelf` remove episodes and books while retaining Emby and Feed items, and extend its mixed-state test with the existing book fixture.
- [x] 1.3 Delete the unused episode-only `is_audiobookshelf_slot` helper, then run `cargo nextest run -p mbv-core playback_queue` to verify the queue changes.

## 2. Fix admission and playback projection

- [x] 2.1 Use combined Audiobookshelf classification for submit and append admission and cold-start active-file projection in `player_runtime_controller.rs`; verify a book-only submission without owner context returns the normal refusal before source preparation.
- [x] 2.2 Use combined Audiobookshelf classification in the active Playback run's append and submit routing so books never reach the ordinary mpv-playlist URL path; verify the existing daemon book fixture reaches active-file projection on cold start and active-player reuse.
- [x] 2.3 Use combined Audiobookshelf classification for natural-completion close position, and add one direct lifecycle test proving a naturally completed book closes at runtime while a non-natural stop retains the last valid position.

## 3. Fix Audiobookshelf Service teardown

- [x] 3.1 Update daemon reconciliation purge and active Audiobookshelf finalization to include books, then extend the existing mixed-queue reconciliation test to verify books and episodes are removed while Emby and Feed slots remain.
- [x] 3.2 Update interactive-process Audiobookshelf replacement and removal cleanup to include books in Composed, local Bound, and remote Bound queue state; verify by extending the narrowest existing Service-action test rather than adding a UI test.
- [x] 3.3 Search non-test `is_audiobookshelf()` calls and `QueueItem::Audiobookshelf(_)` matches across `crates/` and `src/`; verify every remaining episode-only use is at an identity, source, progress, or transport boundary and document that classification inline only where it is otherwise ambiguous.

## 4. Verify the change

- [x] 4.1 Run `cargo check -p mbv-core && cargo nextest run -p mbv-core` and fix every failure related to queue classification, owner admission, projection, lifecycle, or teardown.
- [x] 4.2 Run `cargo check -p mbv && cargo nextest run -p mbv` to verify interactive-process teardown and queue adoption remain correct.
- [x] 4.3 Run `cargo clippy --workspace --all-targets && cargo fmt --all -- --check && ast-grep scan && make check-code-file-lines`, then confirm the diff contains no wire-format, persisted-format, queue-revision, `pending_sync`, Consume-ownership, or queue-start-lifecycle changes.
