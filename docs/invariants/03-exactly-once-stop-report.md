# Invariant 3 — Exactly one `report_stopped` per item lifecycle

**Scope:** `StopReport::{NotSent, Sent, Accepted}` and `LoadState`
(`crates/mbv-core/src/player_run_state.rs`), `PlaybackRun::{on_end_file,
on_shutdown, report_stop_now_or_background}` (`player_run_events.rs`,
`player_run_queue.rs`), all `cmd_*` replacement paths
(`player_run_commands.rs`), and `SessionReporter::{report_stopped,
report_stopped_background, report_stopped_for_shutdown, has_session,
clear_session}` (`player_runtime.rs:210+`).

## The invariant

For every item that becomes current, **exactly one** `report_stopped` reaches
Emby for the *completed* item, and the reporter's session ids always name the
item the report is about:

1. `stop_report` starts `NotSent`, transitions to `Sent`/`Accepted` exactly
   once per item lifecycle, and resets **only** when the displaced file's
   drain completes (`on_end_file`'s `load_state.drain() → HitZero →
   stop_report.reset()`).
2. During a queue transition, `loadfile "replace"` displaces the current file
   and mpv emits a predictable EndFile for it; `LoadState::begin_single()`
   arms exactly one drain so that EndFile is swallowed, not reported.
3. `SessionReporter.ids` always name the item whose progress is being
   reported: `start_item`/`transition_to` publish new ids **before**
   `report_start`; `start_item`-for-feed and Emby→Feed transitions call
   `clear_session()` so subsequent reports are no-ops instead of stale writes.
4. No path reports a transition's *new* item as stopped, and no path reports
   the *old* item twice (once synchronously, once via EndFile/Shutdown).

## Why it matters

`report_stopped` is what moves the server resume point. Zero reports loses
resume; two reports for one lifecycle can overwrite the *new* item's `start`
with the *old* item's terminal position (or zero an audio item's position
twice, harmless but noisy). Cross-item id contamination is worse: reporting
item B's position under item A's session id corrupts A's resume point on the
server with B's clock — silent, persistent, and blamed on "Emby being
weird". The `load_state`/`stop_report` pair is a tiny two-phase commit across
the mpv event stream, and every replacement path must participate
identically.

## What breaks if it is violated

- **Double report on replace.** `cmd_replace_queue` sets
  `stop_report = mark_sent(report_stopped(...))` *and then* the displaced
  file's EndFile arrives. Without `LoadState::begin_single()` + drain-swallow,
  `on_end_file` reports the old item again — and worse, after the queue has
  been swapped, `last_valid_pos` may already belong to the new item's clock.
- **Lost report on quit.** `on_shutdown` reports only `if stop_report ==
  NotSent`. If a transition path wrongly pre-marks `Sent` (e.g.
  `accept_stopped_replacement`'s `if NotSent` guard misfires, or a reset is
  skipped), the real final position is never sent and resume rewinds.
- **Stale-id write on Emby→Feed.** If `clear_session()` is skipped when the
  new item is a Feed entry, the progress reporter keeps sending the feed
  position under the old Emby session id. `report_stopped_background` and
  the 10 s progress thread both key off `has_session()` — clearing is the
  only thing that silences them.
- **Shutdown races.** `report_stop_now_or_background` exists because a real
  quit must report synchronously (background thread may not run before exit)
  while an ordinary stop must not block the UI/mpv. Calling the wrong one
  either hangs teardown on the network or drops the final report on the
  floor. The `shutdown_report_timeout` budget split
  (`progress_join_budget` = half of quit timeout) composes with
  `App::teardown`'s outer bound — mis-set either and shutdown either
  overruns or starves the session-terminating call.

## How the code maintains it today

- **Arm/drain discipline:** every replacement path (`cmd_replace_queue`
  empty + non-empty, `cmd_load_new`, `cmd_submit_queue`,
  `replace_with_queue_items`, `accept_stopped_replacement`) sets
  `load_state = begin_single()` and documents that `stop_report` stays
  `Sent` until the drain resets it. `on_end_file` drains first, before any
  reason handling, and `return true`s (swallow) while pending.
- **Per-site `stop_report` choreography:** replace paths `mark_sent` the old
  item's report synchronously at swap time (so EndFile/Shutdown later see
  `Sent` and stay quiet); `cmd_load_new` sets `NotSent` because
  `transition_to` defers the old item's report to a background thread;
  `cmd_submit_queue` branches on `queue_len() > 0` so a first-ever submission
  doesn't report a nonexistent current item.
- **Id hygiene:** `start_item` writes `ids` before `report_start` (comment:
  "so the progress reporter thread never sends stale IDs"); `transition_to`
  fires `report_stopped_background` for the old ids *before*
  `get_playback_info` overwrites them; Feed branches call `clear_session()`;
  `QueueRemove` of the active slot clears reporter ids to stop stale
  progress until the transition completes.
- **Quit vs transition split:** `is_quit_shutdown()` gates sync-vs-background;
  `cancel_pending_quit()` clears both `quit_at` *and*
  `shutdown_report_timeout` so a cancelled quit can't silently keep the tight
  shutdown budget for the rest of the run (comment explains the exact
  degradation); the 2 s `quit_at` fallback in `run()` re-reports if nothing
  was sent.
- **Tests pin the trickiest corners:** `playlist_pos_does_not_clobber_…`,
  `queue_load_indices/location`, and the `player_tests_session*` suite cover
  drain, forced-slot, and shutdown-report interplay.

## Where it currently fails

1. **The contract is per-call-site folklore, not a type.** Five replacement
   paths must each set the right (`stop_report`, `load_state`) pair, and the
   correct pair differs per path (`begin_item_lifecycle`'s own comment admits
   "the caller must set `stop_report` and `load_state` itself because those
   differ per call site"). Nothing enforces that a sixth path does it; a new
   `cmd_*` that forgets `begin_single()` double-reports, one that forgets
   the reset never reports. This is the highest-value hardening target in
   the playback path: a single `begin_transition(stop_action)` helper owning
   both fields would make the illegal states unrepresentable.
2. **`LoadState` counts to one, always.** `begin_single()` hardcodes
   `Pending(1)` while `drain()` supports arbitrary counts — the generality
   is dead. If mpv ever emits two EndFiles for one `loadfile "replace"` (or
   two replaces race on the fast path), the second EndFile falls through to
   full end-of-track handling for the *new* item: spurious `mark_played`,
   spurious `TrackCompleted`, wrong `consume`. The `run()` loop processes
   commands and mpv events on one thread, which narrows but does not close
   this (a second `SubmitQueue` can arrive while the first drain is
   pending).
3. **`report_stopped_for_end_file(Quit)` re-enters shutdown-aware reporting
   from inside EndFile handling**, splitting the quit-report logic across
   `on_end_file` (Queue-origin Quit arm), `on_shutdown`, and the `quit_at`
   timeout in `run()`. Three reporters for one event, coordinated only by
   `stop_report` equality checks — correct today, fragile to any new
   early-return that skips the state update but not the send (or vice
   versa).
