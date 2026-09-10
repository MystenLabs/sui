# Experiment: can causal sort be removed from CheckpointBuilder?

Branch: `mlogan-try-remove-causal-sort`

## Goal
Determine whether root effects handed to `CausalOrder::causal_sort_with_ccp` in
`CheckpointBuilder::resolve_checkpoint_transactions` are already causally sorted
(per `TransactionEffects::dependencies()`), so that the sort could be removed.

## Done
- [x] Added `CheckpointBuilder::assert_already_causally_sorted` in
      `crates/sui-core/src/checkpoints/mod.rs`, called immediately before
      `causal_sort_with_ccp`. Panics with `CAUSAL_SORT_VIOLATION:` if any tx's
      effects dependency (that is in the same root batch) appears later in the batch.

## Done (cont.)
- [x] `cargo check -p sui-core` clean
- [x] Seed search (2026-09-10):
      `scripts/simtest/seed-search.py test::test_composite_workload --exact --test simtest --no-build --num-seeds 200 --error-regex CAUSAL_SORT_VIOLATION`
      Note: the test name must include the `test::` module prefix when using `--exact`.

## Results
- 200/200 seeds passed, 0 `CAUSAL_SORT_VIOLATION` panics (seed range 1789058250001..200).
- Explicit effects dependencies are already respected by consensus handler output order
  in every batch handed to the checkpoint builder in this workload.

## RWLock edges (commit 2651b21836)
- Check moved into `CausalOrder::check_already_sorted` so it reuses `RWLockDependencyBuilder`.
  Reports "effects dependency" vs "rwlock edge" in the panic message. Unit test added.
- Seed search rerun with rebuilt binary, 200/200 passed, 0 violations of either class.

## Settlement scheduler (commit 2de3fc53d9)
- `SettlementScheduler::construct_and_execute_settlement` also calls `causal_sort_with_ccp`;
  the same `check_already_sorted` now runs before it. Both consumers must change together
  because the checkpoint builder recomputes settlement digests from the sorted order.
- Seed search on rebuilt binary (both assertions): 200/200 test::test_composite_workload passed, 0 violations.

## Broader coverage
- All 36 sui-benchmark simtests x 10 seeds (checkpoint-builder assertion only): 282/360 passed,
  0 failures before the run was cancelled to prioritise the settlement check.
- sui-e2e-tests sweep: not run.

## Why the property holds (see conversation notes)
- Shared inputs + RWLock edges: version assignment walks the commit in root order.
- Owned/received inputs: vote-time exact-version liveness check + votes cast at block
  acceptance + execution only after commit => producers are in strictly earlier commits.
  Post-consensus locking already relies on this (authority_per_epoch_store.rs ~1949).
