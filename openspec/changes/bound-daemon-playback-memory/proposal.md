## Why

`mbvd` on `music.local` (1 GiB RAM + 1 GiB swap) grew without plateau during
long audio playback until it exhausted both RAM and swap and took the host
down (#656). Investigation has established the shape of the growth — roughly
+6.5 MiB per track transition, anonymous/private memory, released only at
process exit — and Heaptrack has ruled out a Rust-owned leak (1.45 MiB leaked,
45 MiB peak tracked heap against a 64 MiB RSS rise over the same ten
transitions). Those two facts together mean the resident growth is freed-but-
unreturned pages, not live objects; a temporary `MALLOC_ARENA_MAX=1` override
flattened the slope, which is the confirming half of the same observation.

This change stops the investigation loop and lands the three corrections that
are each defensible on their own terms, each of which reduces the footprint whether or not it turns out to be
the dominant layer. Verification is ordinary daily listening on `music.local`:
the original failure announced itself within a normal session, so a regression
would too.

## What Changes

- Headless playback runs get an audio-sized demuxer cache budget instead of the
  video-sized one. Today `init_mpv` sets `demuxer-max-bytes=50M` and
  `demuxer-max-back-bytes=100M` for every run, so a 1 GiB headless host
  reserves a 150 MiB steady-state cache to play MP3s. Playback with a video
  window keeps the existing budget unchanged.
- `SessionReporter` stops spawning detached threads per track transition.
  Emby report-start / report-stopped / progress-join work moves onto one
  long-lived FIFO worker thread fed by a channel. This removes the per-track
  thread churn that gives glibc a reason to open and retain a new malloc arena
  on every track, and makes stop-before-start ordering explicit instead of a
  race between detached threads.
- Both long-lived playback binaries (`mbvd` and the `mbv` binary that hosts the
  Local daemon) install a trimming global allocator, so freed pages return to
  the OS on a decay timer rather than accumulating as glibc arena high-water.
  This replaces the rejected `MALLOC_ARENA_MAX` service-environment override:
  same effect, compiled in, no host state to lose on reboot.

## Capabilities

### New Capabilities
- `headless-playback-memory-footprint`: bounds the resident memory a headless
  Player owner may accumulate across track transitions, and the cache budget it
  reserves for audio playback.

### Modified Capabilities
- `video-feed-playback-buffering`: the 100MiB retained cache window and 50MiB
  forward limit are currently specified as applying to a Player owner's
  playback cache unconditionally. They are narrowed to runs with a video
  window; headless runs are governed by the new capability.

## Impact

- `crates/mbv-core/src/player_runtime.rs` — `init_mpv` cache options; the three
  per-transition `thread::spawn` sites in `SessionReporter`.
- `crates/mbv-core/src/player_run_queue.rs` — the progress-join spawn in
  `report_stop_now_or_background`.
- `src/main.rs`, `crates/mbvd/src/main.rs` — global allocator declaration.
- New workspace dependency: a trimming allocator crate (`mimalloc`).
- No protocol, persistence, config-file, or UI surface changes.
