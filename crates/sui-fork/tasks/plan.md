# Implementation Plan: Replace `LiveState` with `object_version_by_checkpoint`

## Overview

Delete the fork-owned `live_state` RocksDB and the `ForkLiveState` pointer, and serve
current-version authority from the rpc-store's own `object_version_by_checkpoint` CF
instead.

The CF lives in the same database as the `objects` rows it describes, so a row and the
authority that makes it current commit in one batch. That retires both write-ordering rules
in `design/storage.md`
("rows before pointers", "removals before writes") and the startup-reconciliation gap
listed under Known gaps, because there is no longer a second database to reconcile
against. The fork's embedded indexer already runs the `object_version_by_checkpoint`
pipeline for post-fork checkpoints (`sui-rpc-store/src/indexer/mod.rs:379, 431-438`), so
`LiveState` is a parallel mechanism maintaining something the store maintains anyway.

Behaviour visible over RPC should not change: the fork answers the same questions from a
different authority source.

## Semantics we are committing to

The CF is keyed `(ObjectID, checkpoint) -> version`, where the checkpoint is *the
checkpoint in which the object changed to that version* — not "a checkpoint at which the
version was live". Liveness is inferred by a reverse floor scan: "V changed at C₁, and no
row exists in (C₁, C]" implies V is live at C. That inference is sound only when the change
history is complete over the scanned range, which is exactly the assumption the fork's
sparse store violates. Every rule below follows from protecting that assumption.

**Write rule 1 — lazy pre-fork materialization writes a restore floor.** When a read
resolves an object through GraphQL pinned at the fork checkpoint, we have learned its live
version *as of the fork checkpoint*. Record `store_restored(id, fork_cp, V)`. The
`from_restore` flag is what lets a later read tell "existed before the window, live at V"
from "created later", which is precisely the distinction `LiveState`'s three-way pointer
hand-rolls today.

**Write rule 2 — post-fork execution writes ordinary rows.** Changes produced by local
execution are keyed by the checkpoint executing them. This range is genuinely complete: the
fork executed every one of those checkpoints, so the floor-scan inference holds.

**Write rule 3 — exact-version fetches write no index row at all.** A fetch of `A@5` is
evidence about one point in history and is *not* evidence about what is live at any
checkpoint. Keying it at `fork_cp` would falsely claim v5 is live at the fork point. Keying
it at v5's creation checkpoint is subtler but equally wrong: it asserts "v5 from that
checkpoint onward until the next recorded change", and a sparse store cannot rule out an
unfetched later change, so the floor scan would return v5 with full confidence. The
`objects` row alone serves the version-keyed read that was actually asked. This mirrors
today's discipline exactly — an exact-version fetch populates rows and must never touch the
live pointer.

**Read rule — bounded reads through the version-level API.** Resolve current state with
`get_object_version_at_checkpoint(id, local_tip)`, never the unbounded
`get_latest_object_version`, which is a reverse prefix scan carrying the same sparse-history
hazard that motivated `LiveState` in the first place. And call the *version*-level API
rather than the composed `get_object_at_checkpoint`, which collapses both "tombstone" and
"no row" to `None` (`schema/object_version_by_checkpoint.rs:214-223`) — the fork needs those
distinguished, since a tombstone means never fall back and absence means do.

## Architecture Decisions

- **`get_latest_object_status` keeps its signature and its three-way contract.** Only its
  body changes: the `live_state.get(id)` lookup becomes a bounded
  `get_object_version_at_checkpoint(id, tip)` plus an `objects`-row inspection to classify
  live vs tombstone. Callers (`save_live_object_if_current`, the indexed-save path,
  `ForkStore`'s object reads) are untouched. `Status` and `TombstoneKind` survive as-is.

- **Post-fork rows are written synchronously by execution, and the indexer rewrites the
  same keys.** The executor needs read-your-writes for the next transaction's inputs, and
  the indexer only runs after sealing, so we cannot wait for it. Both writers produce
  `(id, checkpoint) -> version` for the same change, so the write is idempotent — but this
  is the one place the plan knowingly departs from the "canonical written synchronously,
  derived left to the indexer" split in `design/storage.md`, and Task 6 verifies the two
  agree rather than assuming it.

- **Pre-fork checkpoint-pinned reads must bypass the restore fallback.** When the floor scan
  finds nothing at or below C, the schema takes the object's earliest row and, if
  `from_restore` is set, returns that version regardless of how far below the anchor C sits
  (`schema:192-202`). For a stock node that is a documented bounded-availability
  approximation. For the fork it reports fork-point state as state-at-C, wrong for any
  object that changed in between — and unlike a stock node the fork could answer correctly
  by asking GraphQL pinned at C. Task 5 establishes whether any path can reach this.

- **`live_state/` disappears from the data dir.** Existing fork data directories become
  unreadable under the new code. Since `fork_metadata.json` is already validated on open, we
  extend that check to reject a data dir containing a `live_state/` directory with a message
  telling the user to re-fork. No migration path: fork data dirs are cheap to recreate and a
  migration would have to reconstruct authority we deliberately no longer store.

- **No new pruning configuration.** The fork configures none today, which is what makes this
  safe: the schema doc notes pruning retracts checkpoint-pinned entries below a superseding
  checkpoint (`schema:30-34`), which would eat restore-floor rows. Recorded as a constraint
  in the design doc rather than defended against in code.

## Task List

### Phase 1: Read path on the new authority
- Task 1: rewrite `LocalStore::get_latest_object_status` against
  `get_object_version_at_checkpoint`, keeping the signature and three-way contract
- Task 2: add the `local_tip` accessor the bounded read needs, and confirm its value during
  in-flight execution

### Checkpoint: lib compiles, reads resolve without consulting `live_state`

### Phase 2: Write path
- Task 3: lazy materialization and seed/inventory saves write `store_restored` rows at the
  fork checkpoint (replacing `set_live`)
- Task 4: `apply_local_object_diff` writes ordinary rows at the executing checkpoint, in the
  same batch as the object rows (replacing `apply_checkpoint`)

### Checkpoint: `cargo check -p sui-fork` green; no `live_state` references outside its module

### Phase 3: Delete the second database
- Task 5: delete `src/live_state.rs`, its `mod` declaration, the `LocalStore` field and
  constructor parameter; drop the DB open from `ServiceManager`; extend the
  `fork_metadata.json` open check to reject a stale `live_state/` directory

### Checkpoint: one RocksDB instance; data-dir layout matches `design/storage.md`

### Phase 4: Verification
- Task 6: assert the execution-path row and the indexer-written row agree for the same
  change (the double-writer decision above)
- Task 7: port the existing `LiveState` tests to the new authority — in particular the
  wrapped-then-recreated case, the tombstone-never-resurrects case, and
  `test_rpc_latest_read_ignores_stale_cached_history`
- Task 8: new test — exact-version fetch of a system package below its live version must not
  change what a latest read returns (the counterexample that motivated this design review)

### Checkpoint: `SUI_SKIP_SIMTESTS=1 cargo nextest run -p sui-fork` green

### Phase 5: Docs and lint
- Task 9: rewrite the `LiveState` section of `design/storage.md`; update the data-dir layout,
  the three-way read diagram in `design/storage_diagrams.md`, and the Known gaps entries that
  this change retires (write orderings, startup reconciliation)
- Task 10: `cargo fmt` + `cargo xclippy -p sui-fork`

## Risks and Mitigations

**The executing checkpoint number may not match what the indexer assigns.** Write rule 2
depends on the fork keying rows by the same checkpoint the indexer will later key them by.
If they disagree, the floor scan sees two rows for one change at different checkpoints and
the later one wins — silently. Task 2 confirms the numbering before any write-path work
lands; if it cannot be confirmed cheaply, Phase 2 stops and we reconsider disabling the
indexer's pipeline instead.

**The restore fallback is reachable from any checkpoint-pinned read below the fork point.**
Task 5 audits for such a path; if one exists it needs an explicit guard before this lands.

**Crash windows shrink but do not close.** Atomicity fixes row-versus-authority. The
memory-only `PendingCheckpointBuffer` remains exactly as it is, so a crash mid-publication
still loses an unsealed checkpoint, and the Known gaps entry has to be narrowed to cover
that alone.

## Open Questions

- Does the executor know the in-flight checkpoint sequence number at
  `apply_local_object_diff` time, and does it equal the sealed checkpoint's number?
  **Blocks Phase 2.**
- Should the fork disable the indexer's `object_version_by_checkpoint` pipeline rather than
  accept an idempotent double write? Cleaner in principle, but the pipeline carries the
  restore-anchor handling we want, and disabling one pipeline changes the publication
  watermark logic that execution blocks on.
- Does any current gRPC path expose a pre-fork checkpoint-pinned object read? If not, the
  restore-fallback trap is a constraint to document rather than code to write.
