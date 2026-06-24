# Debugging: crash-recovery self-conflict checkpoint fork

Repro (matching seed-search artifact — run the binary directly, NOT `cargo simtest --test` which
builds a different artifact without `--package`):

```
MSIM_TEST_SEED=1782321375177 RUST_LOG=sui=debug,info \
SIMTEST_STATIC_INIT_MOVE=$PWD/examples/move/basics \
target/simulator/deps/simtest-f62f56c1988601e8 --test-threads 1 --no-capture \
--exact test::test_simulated_load_reconfig_with_crashes_and_delays
```
(cwd = crates/sui-benchmark to match seed-search)

Poison decision is now content-addressed (deterministic), so this is reproducible.

# iteration 1
- OBSERVATIONS
  - Fork is a SELF-CONFLICT: `checkpoints/mod.rs:1750 Checkpoint 210 was previously built with a
    different result: previously_computed 5VGSLuf... vs current 6FWarw7...` on node 3 (k#99f25ef6),
    epoch unknown yet.
  - Exactly one poison tx: `FsuFdYvEpRbt5zUCRZWLtFSy4o6AiZm9n5o9ANEu7EjW`, crashed 3x.
  - This is the SAME assertion class as the original Antithesis bug; the committed fix
    (drop-late + lock-preservation, 352037a1d6) does NOT prevent it.
  - seed-search log is RUST_LOG=error only -> lacks checkpoint-build detail. Need a debug rerun.
- HYPOTHESIS: On the first execution the poison tx is not yet in `crashed_transactions`, so its
  effects are committed and included in the locally-built+persisted checkpoint 210 (5VGSLuf). The
  crash fires after effects are committed. On restart the tx IS in `crashed_transactions` and is
  dropped, so checkpoint 210 rebuilds without it (6FWarw7) -> self-conflict. The drop-late fix does
  not help because the poison tx is itself a member of the already-persisted checkpoint, not merely
  affecting neighbors via locks.
- EXPERIMENT: Re-run with debug logging; confirm whether FsuFdYv's effects/digest are in cp210's
  pre-crash contents (5VGSLuf) and absent from the rebuild (6FWarw7), and locate where
  maybe_crash_for_testing fires relative to effects-commit and checkpoint-build.
- RESULTS (experiment_1): Hypothesis PARTIALLY REFUTED / REFINED.
  - Both cp210 builds contain "2 transactions" (SAME count) but different contents digests
    (pre-crash HE4HZ2Vd vs post-crash G7KdDfKw). So FsuFdYv is NOT directly added/removed from
    cp210; the divergence is in the 2 txs' contents.
  - FsuFdYv is an address-balance WITHDRAWAL tx. Checkpoints include an AccumulatorSettlement tx,
    built via "early/eager settlement" at SCHEDULING time (consensus handler), not at execution.
  - maybe_crash_for_testing fires at authority.rs:1503 — after the effects-cache check, BEFORE
    execution. So FsuFdYv never commits its own effects; the crash is at EXECUTION time.
  - FsuFdYv crashed twice (.616,.716) then was dropped (.816, crash_recovery.rs:441).

# iteration 2 — ROOT CAUSE
- ROOT CAUSE: An address-balance withdrawal that is poison contributes to the commit's
  AccumulatorSettlement at SCHEDULING time (eager settlement, consensus handler). On the first
  encounter the tx is not yet known-poison, so it is scheduled, its withdrawal is folded into the
  settlement, and checkpoint 210 is built+persisted WITH that settlement (HE4HZ2Vd). The poison
  crash then fires at EXECUTION time (after the checkpoint was built). On restart the tx is in
  crashed_transactions and is dropped at schedulables formation, so its withdrawal is excluded
  from the settlement and cp210 rebuilds differently (G7KdDfKw) -> self-conflict fatal.
- The asymmetry is structural: the DROP happens at scheduling time (before settlement), but the
  CRASH happens at execution time (after the checkpoint is built). The committed lock-preservation
  fix does not help because the diverging side effect is the eager accumulator settlement, not an
  owned-object lock. This is independent of (and pre-dates) that fix.
- FIX DIRECTION (for discussion): make the crash and the drop happen at the SAME pipeline stage so
  the poison tx never influences a built checkpoint in either timeline. Options:
  1. Move the poison crash to the consensus handler (decide via content-addressed
     should_poison_transaction before scheduling/settlement/checkpoint); crash there on first
     encounter, drop there on recovery. Loses "crash mid-execution" coverage.
  2. Exclude poison txs from checkpoint/settlement at scheduling time on BOTH encounters (keyed on
     content-addressed should_poison), while still letting the first encounter execute->crash for
     recovery coverage. Requires separating "scheduled for execution" from "included in checkpoint".
