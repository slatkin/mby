## 1. Projection type

- [ ] 1.1 Add `PlayheadProjection` + `PlayheadConfidence` (`Confirmed | Predicted(PredictionReason)`) and `PredictionReason` (`Relocated | ItemSelected`) to `src/app/types_playback.rs`, folding in the roles of `QueueCursorPush` (scope + Follow/Reanchor) and `PendingActiveIdx` (Shift/Jump). Verify `cargo check -p mbv` compiles the new type with a unit assertion that a `Predicted(Relocated)` projection reports progress and `Predicted(ItemSelected)` reports zero.
- [ ] 1.2 Replace `App::pending_active_idx` and `App::queue_cursor_pushed` with the single `playhead: PlayheadProjection` field in `src/app/app_struct.rs`; update `construct.rs` and `src/app/tests.rs` stub init. Verify `cargo check -p mbv` fails only at the arm/read sites addressed in groups 2–3.

## 2. Convert the arm sites (compiler-enumerated)

- [ ] 2.1 Convert the playhead-follow arm sites — `player_event.rs` (mpv advance, next-up auto-advance, `UnifiedQueueUpdated`), `run_loop_events.rs`, `run_loop_events_session.rs` — to set `playhead` with `Predicted(Relocated)` scoped to the playback scope, preserving the existing `queue_cursor_held_by_user()` yield. Verify the retargeted `player_event` / run-loop queue tests pass.
- [ ] 2.2 Convert the authoritative re-anchor arm sites — `queue_scope.rs` (scope switch, `replace_playback_queue`, `replace_direct_remote_queue`), `mouse_gestures.rs` (wheel scroll), `shell_queue.rs` `PlayNow` — to an authoritative re-anchor on their scope. Verify `tests_queue_reorder.rs` and the `shell_queue` scope-aware / reanchor tests pass.
- [ ] 2.3 Convert the item-selected arm site — `action.rs` `QueuePlayCursor` jump path — to `Predicted(ItemSelected)`; convert the queue-edit arm sites — `queue_actions.rs` remove and move — to `Predicted(Relocated)`. Verify the play-cursor stale-progress test and `active_index_prediction_survives_same_length_move_until_player_ack` pass.

## 3. Reconcile as a tick step; readers stop mutating

- [ ] 3.1 Add `App::reconcile_playhead()` that drops a prediction when `status.current_idx` + `status.queue_len` match it; call it from `src/app/shell_run.rs` immediately after `handle_player_event` drains `player_rx`. Verify a new test: a player event tick clears a matching prediction, and a bare layout tick (no event) does not.
- [ ] 3.2 Make `effective_playback_state` take `&self` and read the projection instead of running the reconciliation match; update its callers. Verify `cargo check -p mbv` and the indicator/playback-state tests pass.
- [ ] 3.3 Make `sync_queue`'s cursor decision read `playhead` (scope match + Follow-yields / Reanchor-wins) without clearing it — clearing is now `reconcile_playhead`'s job for predictions and consumption-on-apply for re-anchors. Verify the `shell_queue` push-consumption tests pass unchanged in intent.

## 4. Scope accessor pass (separate commit, droppable)

- [ ] 4.1 Rename `visible_queue_scope` → `viewed_queue_scope`, `playback_target_queue_scope` → `playing_queue_scope`; derive `queue_scope_is_playback` from the latter; document `displayed_queue*` / `playback_queue*` as viewed vs playing. Verify `cargo check -p mbv` and `cargo clippy --workspace --all-targets` are clean.

## 5. Docs and gates

- [ ] 5.1 Add a "Playhead" entry under `## Queue` in `CONTEXT.md`. Verify it names the projection concept (active scope + slot + confidence) and lists an *Avoid* line.
- [ ] 5.2 Sync the `queue-canonical-list` delta into `openspec/specs/queue-canonical-list/spec.md` and run `openspec validate --changes consolidate-queue-playhead-state`. Verify it passes.
- [ ] 5.3 Full gate: `cargo nextest run -p mbv`, `cargo clippy --workspace --all-targets`, `cargo fmt --all -- --check`, `make check-code-file-lines`. Verify all pass (the pre-existing unrelated `shell_tv_workspace::tests::tv_breakpoint_resize_round_trip_keeps_selected_series` flake excepted).
