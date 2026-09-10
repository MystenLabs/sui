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

## Remaining
- [ ] `cargo check -p sui-core`
- [ ] Seed search: `scripts/simtest/seed-search.py test_composite_workload --package sui-benchmark --error-regex CAUSAL_SORT_VIOLATION`
- [ ] Record results below

## Results
(pending)
