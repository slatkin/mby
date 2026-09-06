# Invariant 4 — "Audiobookshelf-shaped" means both shapes, everywhere

**Scope:** every branch that asks "is this slot/item Audiobookshelf-owned?"
— `crates/mbv-core/src/playback_queue.rs` (300, 316, 497),
`crates/mbv-core/src/player_runtime_controller.rs`
(`submit_queue`, `queue_append`, cold-start projection),
`crates/mbv-core/src/player_run_queue.rs:506`,
`crates/mbv-core/src/player_run_commands.rs:326,433`,
`crates/mbv-core/src/player_run_events.rs:637`,
`crates/mbv-core/src/daemon_reconciliation.rs:173,199,234`,
`crates/mbv-core/src/config_state.rs:273`,
`src/app/audiobookshelf_service_actions.rs:44,161,175`.

## The invariant

`QueueItem` has **two** Audiobookshelf shapes — `Audiobookshelf` (episode)
and `AudiobookshelfBook` (book) — and the helpers say so explicitly:
`is_audiobookshelf_any()`, `required_service()` (both → `ServiceKind::
Audiobookshelf`), `admissible_for_owner_with_audiobookshelf()` (both gated).
**Every** site that decides ownership, admission, projection mode,
refresh-merge retention, service-teardown purging, session finalization, or
lifecycle close position must treat both shapes as Audiobookshelf-owned.
Matching only `Audiobookshelf(_)` / `is_audiobookshelf()` is never correct
unless the site names the episode-only reason in a comment.

## Why it matters

The two shapes share everything the invariant protects: they need an
`AudiobookshelfPlayerContext` to prepare, a server session lifecycle to
finalize, exclusion from Emby refresh-merge, exclusion from the mpv-playlist
projection (`active_file` mode), and purging on service replacement/removal.
A site that matches only the episode shape silently treats books as the
*default* kind (Emby/Feed path) — wrong admission, wrong projection, wrong
retention — with no compiler complaint, because both are just enum variants.

## What breaks if it is violated (all live today)

- **Book slots pruned by Emby refresh.** `merge_refresh` keeps
  `Feed | Audiobookshelf(_)` as-is (playback_queue.rs:497) but books fall
  through to the Emby lookup, which can never match (`group_fetched_items…`
  keys `Emby(id)` only; a book's `content_id` is `AudiobookshelfBook{…}`).
  Any non-active, non-`pending_sync` book is reported `pruned` on the next
  library refresh. Active books survive only via `should_protect_missing_slot`.
- **Book-only queues project as mpv-playlist instead of active-file.**
  `has_audiobookshelf_entries()` (playback_queue.rs:300),
  `Player::submit_queue` / `queue_append` admission, cold-start
  `active_file_projection`, `init_from_queue active_file`
  (player_run_queue.rs:506), `cmd_append_queue` (commands:326) and
  `cmd_submit_queue` routing (commands:433) all test episode-only. A
  book-only submission therefore skips the merged-timeline projection
  (`install_active_projection`: `merge-files`, extra sources, absolute seek)
  and takes the `mpv_url_for_queue_item` path, whose book arms are
  `unreachable!()` — fast-path submit of a book panics the player thread
  (caught by `run()`'s `catch_unwind` into a spurious `Stopped{error}`, but
  still a panic on a normal user action).
- **Service teardown leaks books.** `purge_queue` closures
  (daemon_reconciliation.rs:173,199), `finalize_active_audiobookshelf`
  (:234), `QueueState::without_audiobookshelf` (config_state.rs:273), and the
  app's `stop_active_audiobookshelf_playback` /
  `clear_audiobookshelf_queue_memory` (service_actions:44,161,175) all match
  episode-only. Removing/replacing the service while a book is queued leaves
  book slots in the live queue **and** the persisted `QueueState` (resurrects
  on restart), and an actively-playing book keeps its orphaned server session
  (never finalized within the teardown budget).
- **Book admission without a context.** `submit_queue`/`queue_append` reject
  episode items when `can_admit_audiobookshelf()` is false, but a book-only
  submission passes the gate and fails later inside `prepare_book_source`
  (`Unavailable`) — a confusing async `Stopped{error}` instead of the clean
  visible reject the episode path gets.
- **Book completion under-reported.** `provider_lifecycle_close_pos`
  (events:637) closes episode natural-ends at `runtime` (so `current_time >=
  duration` → `is_finished`) but closes books at `last_valid_pos`, which at
  EOF timing can sit a fraction under `duration` → finished books sync as
  unfinished.

## How the code maintains it (where it does)

The pattern for correctness already exists and just isn't applied uniformly:
`daemon_control_queue.rs:79` gates episodes vs books on separate transports;
`PlayerProxy::submit_queue` (remote) checks `is_audiobookshelf` and
`is_audiobookshelf_book` independently; `prepare_source`,
`ActiveItemLifecycle::for_item`, `mpv_url_for_queue_item`, and
`QueueState::emby_items` all enumerate both shapes; `has_non_emby_entries`
(`!matches!(Emby)`) and `required_service`/`admissible_*` are shape-complete
by construction. `reconcile_audiobookshelf_book_progress` matches books by
`library_item_id` only — the one place episode-only matching is correct, and
it says so.

## Cheapest strengthening (not done here)

- Replace the episode-only predicates at the sites above with
  `is_audiobookshelf_any()` (or match both variants); keep the two
  transport/capability gates (`abs-queue` vs `abs-book-queue`) split, as they
  are today.
- Regression tests: `merge_refresh` retains a non-active book;
  `purge_queue`/`without_audiobookshelf` drop books; book-only
  `submit_queue` without a context returns `false`; book-only cold start
  sets `active_file`.
- Dead code note: `is_audiobookshelf_slot` (playback_queue.rs:316) has no
  callers and is episode-only — delete it or make it `_any`; it exists only
  to be copied from.
