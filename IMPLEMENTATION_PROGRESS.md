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

## Not yet covered
- The RWLock synthesized edges (`RWLockDependencyBuilder`): a consensus-object writer of
  version N+1 is sorted after readers of version N. This is *not* an effects dependency,
  so the assertion does not check it. Needs a second assertion before causal_sort can
  actually be removed.
- Other workloads / tests beyond test_composite_workload.
