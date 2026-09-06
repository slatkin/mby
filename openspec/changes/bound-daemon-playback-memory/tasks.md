## 1. Headless cache budget

- [x] 1.1 In `crates/mbv-core/src/player_runtime.rs`, remove `demuxer-max-bytes` and `demuxer-max-back-bytes` from the `Mpv::with_initializer` option block and set both explicitly after init: `50M`/`100M` when `config.headless` is false, `10M`/`10M` when it is true. Both branches must set both properties so a user `mpv.conf` cannot supply either. Verify `cargo check -p mbv-core` passes.
- [x] 1.2 Extend the existing `#656` headless-init test in `crates/mbv-core/src/player_tests_submit.rs` (the one asserting `audio-display == "no"`) to also assert `demuxer-max-bytes` and `demuxer-max-back-bytes` read back as the headless values, and add the non-headless counterpart asserting `50M`/`100M`. Verify with `cargo nextest run -p mbv-core player_tests_submit`.

## 2. Single reporter worker

- [x] 2.1 Define a `ReportJob` enum in the `SessionReporter` module covering the three jobs the current spawns perform: stopped-report (with its ws-flush step), start-report, and progress-join-then-stopped-report. Each variant carries the values the corresponding closure captures today — ids, positions, runtime ticks, the progress `JoinHandle` and its budget — so the worker reads nothing from a shared lock. Verify `cargo check -p mbv-core` passes.
- [x] 2.2 Give `SessionReporter` an unbounded `mpsc::Sender<ReportJob>` and one worker thread created with the reporter, draining jobs FIFO and executing the bodies moved from the four current closures. Keep `bounded::run_with_hard_bound` around the progress join so a hung join cannot block queued jobs. The worker exits when the sender drops. Verify `cargo check -p mbv-core` passes.
- [x] 2.3 Replace the spawns at `player_runtime.rs:315` (`report_stopped_background`), `:343` (`report_start_background`), and `:430` (the inline `transition_to` spawn) with sends. Leave the synchronous real-quit path in `player_run_queue.rs` (`is_quit_shutdown()` branch) reporting inline as it does now. Verify no `thread::spawn` remains in the reporting path: `grep -n 'thread::spawn' crates/mbv-core/src/player_runtime.rs crates/mbv-core/src/player_run_queue.rs` shows only the run-loop spawn at `player_runtime.rs:629`.
- [x] 2.4 Replace the progress-join spawn in `report_stop_now_or_background` (`player_run_queue.rs:56`) with the progress-join `ReportJob` send, preserving the `mark_progress_sync_pending` fire-and-forget semantics documented at that site. Verify `cargo nextest run -p mbv-core` passes.
- [x] 2.5 Add a test that a transition enqueues the outgoing stopped-report before the incoming start-report, asserting the observed order at the worker rather than wall-clock timing. Verify it fails if the two sends are swapped.

## 3. Trimming allocator

- [x] 3.1 Add `mimalloc` (default features) to the workspace dependency table and to `crates/mbvd/Cargo.toml` and the root binary's dependencies. Verify `cargo check --workspace` passes.
- [x] 3.2 Declare `#[global_allocator] static GLOBAL: MiMalloc = MiMalloc;` in `crates/mbvd/src/main.rs` and in `src/main.rs` — the second is required because `src/local_daemon.rs` runs the same playback loop in-process for Stay-alive. Verify both binaries build and `mbvd --version` runs.

## 4. Gates

- [ ] 4.1 Run `cargo fmt`, `cargo clippy --workspace --all-targets`, `cargo nextest run --workspace`, `ast-grep scan`, and `make check-code-file-lines`; verify all pass with no new warnings and no governed file over 800 lines.
- [ ] 4.2 Sync the applied deltas into `openspec/specs/` (new `headless-playback-memory-footprint`, modified `video-feed-playback-buffering`) and archive the change. Verify `openspec validate` reports no errors.
- [ ] 4.3 Deploy the built `mbvd` to `music.local`, confirm over mpv IPC that `demuxer-max-back-bytes` reads the headless value on a live audio run, and post the observed value plus a starting RSS baseline to issue #656 for comparison during ordinary listening.
