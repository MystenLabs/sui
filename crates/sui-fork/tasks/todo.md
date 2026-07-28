# TODO: Replace `LiveState` with `object_version_by_checkpoint` — COMPLETE

Plan: see tasks/plan.md

Phase order differs from the plan: the plan put the read path first, which would
have left the tree broken between commits (reads switched to rows the write path
did not yet produce). Writing first, dual-writing both authorities, kept every
commit green. Phase 3 merged into Phase 2 for the same reason — splitting the
deletion out would have committed a state full of dead-code warnings.

## Phase 1: Write path (dual-write) — 76aab4b392
- [x] `LocalStore` takes the fork checkpoint; `ServiceManager` passes
      `metadata.forked_at_checkpoint`
- [x] `executing_checkpoint()` — in-flight checkpoint is highest persisted + 1,
      floored at the fork checkpoint
- [x] `stage_object_version_at_checkpoint` / `stage_restored_object_version`
- [x] `save_live_object_if_current` + `save_indexed_live_object` stage a restore
      floor at the fork checkpoint, in the same batch as the object row
- [x] `apply_local_object_diff` stages ordinary rows at the executing
      checkpoint, in the same batch, removals before writes

## Phase 2+3: Read path and deletion — 3e8ea61af1
- [x] `get_latest_object_status` resolves through
      `get_object_version_at_checkpoint`, bounded at the executing checkpoint
- [x] Raw-`objects` fallback dropped; `apply_local_object_diff` is the only
      tombstone writer, so every tombstone now has an index row beside it
- [x] `src/live_state.rs` deleted, with the `LocalStore` field, the
      `ServiceManager` open, and `highest_tombstone_at_or_before`
- [x] Legacy-data-dir rejection dropped at the user's request

## Phase 4: Verification — dc2673d0f2
- [x] Exact-version fetch must not displace established currency; verified
      non-vacuous by mutation (writing a row there fails the test with the older
      version served as current)
- [x] Pre-fork materialization lands at the fork checkpoint exactly
- [x] Local execution lands at the executing checkpoint

## Phase 5: Docs and lint
- [x] `design/storage.md`: rewrote the authority section around the three write
      rules, narrowed Known gaps to the pending-buffer case, dropped the
      two-database and reconciliation gaps
- [x] `design/storage_diagrams.md`: ownership, three-way read, and write
      diagrams
- [x] Stale "live-state"/"live pointer" terminology across comments
- [x] `cargo fmt` + `cargo xclippy -p sui-fork` clean
- [x] 107 tests pass, 2 skipped

## Not done — deferred deliberately

- **Task 6 as written in the plan** (assert the execution-path row and the
  indexer-written row agree) is covered only indirectly, by
  `local_execution_is_recorded_at_the_executing_checkpoint` plus the existing
  end-to-end tests passing. A direct assertion needs a test that runs the
  indexer and reads the CF back at a known checkpoint.
- **The restore-fallback audit** (plan risk 2): no gRPC path currently exposes a
  pre-fork checkpoint-pinned object read, so the trap is unreachable today and
  recorded rather than guarded.
