## Context

See `proposal.md` — Why. Design-relevant current state:

- `init_mpv` (`crates/mbv-core/src/player_runtime.rs:482-483`) sets
  `demuxer-max-bytes=50M` and `demuxer-max-back-bytes=100M` as unconditional
  `with_initializer` options, before the `config.headless` branch that later sets
  `vo=null`, `force-window=no`, `audio-display=no`.
- `headless` is computed by `Player::headless_for`
  (`player_runtime_controller.rs:298`) as
  `audio_pipe_enabled || (!show_audio_window && is_audio)`. It is the flag the
  existing cover-art decision already keys off, and it is false for any run that
  paints a video window.
- `SessionReporter` spawns a detached thread per event:
  `report_stopped_background` (`player_runtime.rs:315`),
  `report_start_background` (`:343`), and the inline `transition_to` spawn
  (`:430`). `report_stop_now_or_background` (`player_run_queue.rs:56`) spawns a
  fourth to wait out the progress-thread join budget. Each does ureq/TLS work and
  exits.
- Neither binary declares a `#[global_allocator]`; both use system glibc malloc.
- Host of record: `music.local`, 1 GiB RAM + 1 GiB swap, ALSA loopback output.

## Goals / Non-Goals

**Goals:**
- Reduce the steady-state cache reservation of a headless run.
- Remove per-transition thread creation from the reporting path.
- Return freed pages to the OS during the session, by construction rather than
  by host configuration.

**Non-Goals:**
- Proving which layer retained the memory. The three changes are each correct
  independently. No upstream libmpv/FFmpeg report is attempted here.
- A synthetic soak or stress harness. The operator verifies by using the daemon
  normally; the original failure surfaced within a normal listening session, so
  a regression would surface the same way.
- Any change to a run with a video window, to source resolution, to queue
  ordering, or to the `--audio-only` admission rules.
- New user-facing configuration for cache sizes or allocator behaviour.

## Decisions

### Key cache sizing off `headless`, not a new `MpvRunConfig` field

`MpvRunConfig` has no audio-only field, and #656's notes observe as much. Adding
one would mean threading a new value through `submit_queue` and both daemon
paths for a value that `headless` already implies: a headless run has no video
window, so it has no consumer for a video-sized retained cache. Keying off the
existing flag keeps the change to one branch in `init_mpv` and cannot alter a
video run.

Consequence: the cache options move out of the unconditional
`with_initializer` block into `set_option` calls chosen by `config.headless`.
Both branches must set both properties explicitly — leaving one unset would let
a user `mpv.conf` supply it, which is exactly what the post-init property block
exists to prevent.

Budget: headless uses `demuxer-max-bytes=10M` with
`demuxer-max-back-bytes=10M` (20 MiB, versus 150 MiB today). Ten megabytes is
several minutes of any audio bitrate mbv plays; the retained cache exists for
backward seeks, which are cheap to re-fetch for audio.

Alternative rejected: lowering the budget globally. That would regress
`video-feed-playback-buffering`, whose 100 MiB window was provisioned
deliberately for high-bitrate video feeds.

### One long-lived reporter worker, FIFO, replacing four spawn sites

`SessionReporter` gains an `mpsc::Sender<ReportJob>` and a single worker thread
created with the reporter. The four call sites become sends. `ReportJob` carries
the already-snapshotted values those closures capture today (ids, positions,
runtime ticks, the optional ws flush, the progress `JoinHandle` plus its
budget), so nothing new is read under a lock from the worker.

FIFO delivery is a correctness improvement, not just a thread-count one: today
`report_stopped` for the outgoing item and `report_start` for the incoming one
race as independent detached threads, and Emby can observe them out of order. A
single queue makes stop-before-start the structural guarantee the new spec
requires.

The worker must not become a stall point. It performs the same blocking ureq
calls the detached threads did, and the progress-join job keeps the existing
`bounded::run_with_hard_bound` budget so a hung join cannot block the jobs
behind it. Playback never waits on the queue — sends stay non-blocking on an
unbounded channel, matching today's fire-and-forget semantics.

Shutdown: the real-quit path in `report_stop_now_or_background` already reports
synchronously and must keep doing so; it does not go through the worker. The
worker thread ends when the reporter is dropped and the sender closes.

Alternative rejected: a thread pool or an async runtime. Both are more machinery
than one consumer needs, and mbv-core has no async runtime today.

### mimalloc as `#[global_allocator]` in both playback binaries

The mechanism the evidence supports is freed pages not returning to the OS.
mimalloc returns them on a decay timer without an env var, so the mitigation
ships in the binary and cannot be lost to a reboot or a service-file edit — the
specific failure of the `MALLOC_ARENA_MAX` override that was tried and removed.

It goes in `crates/mbvd/src/main.rs` and `src/main.rs`. The second is not
optional: `src/local_daemon.rs` runs the same playback loop in-process for
Stay-alive, so a TUI left running as the Local daemon has the same exposure. A
global allocator can only be declared in a binary, so mbv-core is untouched.

Alternative rejected: `MALLOC_ARENA_MAX` / `mallopt(M_TRIM_THRESHOLD)` from
inside `main`. It is one knob against one glibc behaviour, guesses the arena
count, and does nothing on a musl or non-glibc target.

## Risks / Trade-offs

- A 10 MiB forward cache is smaller than the current 50 MiB, so a headless run
  over a slow network link has less slack before it rebuffers → the retained
  budget stays at 10 MiB rather than 0 so backward seeks still hit cache,. If rebuffering appears during normal
  listening, the forward figure is a one-line adjustment; the sizing decision is not load-bearing for
  the rest of the change.
- mimalloc is a new C dependency in the shipped daemon, affecting cross-builds
  and package size → it vendors and builds from source with no system library
  requirement, and it is confined to the two binary crates; reverting is
  deleting three lines per binary.
- Serialising reports through one worker could hide a slow server as reporting
  lag instead of parallel timeouts → each job keeps its own bounded budget, and
  failures are logged per job as they are today.
- The growth may prove unchanged, meaning the retained memory is libmpv/FFmpeg
  lifecycle state that neither the allocator nor the cache budget touches → all
  three changes remain correct and reduce the footprint anyway; the remaining
  work then moves to a new issue with #656's existing evidence attached.
- `MODIFIED` narrowing of `video-feed-playback-buffering` could read as a
  regression to a future reader → the modified requirement states the headless
  carve-out explicitly and names the capability that governs it.

## Migration Plan

No data or protocol migration. Deploy is the ordinary packaged-daemon path.
Rollback is per-decision and independent: revert the allocator lines, the
`init_mpv` branch, or the reporter worker without touching the others.

The temporary `MALLOC_ARENA_MAX` override on `music.local` has already been
removed and must stay removed, so the host runs the shipped configuration and
nothing else.
