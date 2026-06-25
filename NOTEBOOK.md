# Debugging: crash-recovery self-conflict fork at checkpoint 216

Repro (matching the seed-search build artifact — run the binary directly; `cargo simtest --test`
without `--package` builds a DIFFERENT artifact and does not reproduce):

```
cargo simtest build --tests --package sui-benchmark
cd crates/sui-benchmark
MSIM_TEST_SEED=1782341427004 RUST_LOG=<...> \
SIMTEST_STATIC_INIT_MOVE=$PWD/../../examples/move/basics \
../../target/simulator/deps/simtest-<hash> --test-threads 1 --no-capture \
--exact test::test_simulated_load_reconfig_with_crashes_and_delays
```

Rules: logging-only changes (`info!("CLAUDE: ...")`), no functional changes. Commit logging per
experiment. Current crash design under test: poison = opt-in marker (0xdeadbeef, attached by the
composite `RequestCrash` op) + content-addressed; dropped at try_execute (is_crashed); crashed txs
filtered from checkpoint roots + settlement (is_crashed); exempted from shared-object version
assignment (is_crashed). CRASH_PROB currently 0.02.

# iteration 1
- OBSERVATIONS
  - Self-conflict on node 6: checkpoints/mod.rs "Checkpoint 216 was previously built with a
    different result", previously_computed=369Pb7m (contents FVASy), current=6xto8v (contents 8piTGS).
  - cp216 has 6 txs in both builds; two differ:
    - tx1: 7CLWj4sv (persisted) vs BGczxy (rebuild) — DIFFERENT transaction.
    - tx5 (9mMxcMaN, same digest): effects 3dko (persisted) vs AhwR (rebuild) — same tx, different
      effects => read different input (shared) versions.
  - Poison tx is 5xJ2pF; it produced NO effects (only "Panic while executing", no
    write_transaction_outputs) — consistent with "crashes before producing outputs".
  - 5xJ2pF is NOT in cp216 (neither build).
- USER GUIDANCE: a poison tx that crashes prevents a checkpoint that depends on it from being built,
  so drop-on-recovery should not be able to cause a divergent checkpoint. => the version-exemption
  theory is suspect; find the real source. Do NOT call should_poison in production.
- HYPOTHESIS (to test by logging, not by code change): cp216's two builds differ because the
  version assignment for cp216's consensus commit differs between the first build and the rebuild.
  Need to see, for cp216's commit, the assigned versions of tx1 and tx5 (and whether 5xJ2pF is in
  the same commit and its is_dropped status) on each build.
- EXPERIMENT: re-run seed 1782341427004 with the existing DEBUG "Assigned versions from consensus
  processing" log (authority_per_epoch_store) + the CLAUDE write_checkpoint log enabled, and compare
  the version assignment for cp216's commit across the persisted build vs the rebuild. (Logging
  only; the "Assigned versions" log already exists.)
- RESULT: INVALID experiment. The verbose-logging run did NOT fork, while the same functional code
  forked under RUST_LOG=error. A deterministic msim test cannot change outcome from logging, so the
  failure is NOT a deterministic checkpoint-content bug.

# iteration 2 — non-determinism, then narrowed to shared log file
- OBSERVATION: seed 1782341427004 forks at seed-search concurrency 12 but PASSES at concurrency 1
  (2x), same --package binary and env. => the fork is concurrency/timing-triggered (a race), i.e.
  real (non-simulated) state leaking across concurrent test PROCESSES.
- USER HYPOTHESIS (most likely): concurrent test runs reuse the SAME panic-log file, so one test's
  crashed digests leak into another -> non-deterministic is_crashed -> the is_crashed-keyed
  checkpoint-content decisions diverge -> fork. (Single-threaded file IO itself is not
  non-deterministic.)
- EXPERIMENT: added eprintln logging of the panic-log path on read (load_crashed_transactions).
- RESULT: panic-log paths are UNIQUE per test process (each .tmp dir maps to exactly one seed log).
  Reads are consistent with writes (each validator writes its poison tx and reads back exactly that
  set). Shared/inconsistent log file => RULED OUT.

# iteration 3 — deterministic repro found (NEW deterministic repro)
- OBSERVATION: with a single consistent --package binary, seed 1782341427002 FAILS deterministically
  (both concurrency 1 and 12) with `notify_read.rs:212 debug_fatal "checkpoint builder is stuck"`.
  The earlier "004 passes@1 / fails@12" was a STALE-BINARY artifact (should_poison detour + rebuilds
  in between). So the failure IS deterministic given a consistent binary; no real non-determinism.
- REPRO: seed-search --package --num-seeds 1 --seed-start 1782341427001 (=> seed ...002).

# iteration 4 — builder stuck on a shared-version dependent of the dropped tx
- OBSERVATION (added eprintln of stuck keys at notify_read:212): builder stuck on 8Lkmq + 4dAn1
  (NOT the poison tx). Poison tx = FNBp9xzG.
- OBSERVATION (added eprintln of assigned_versions): FNBp9xzG is assigned shared object 0xdf3d:
  read v5, WRITE v11 (it advances the version). 8Lkmq is (hypothesised) assigned to read 0xdf3d@11,
  which FNBp9xzG never writes (dropped) => 8Lkmq cannot execute => builder stuck.
- OBSERVATION (added eprintln of crashed_set size): crashed set IS populated (size 1) in some
  version-assign calls on recovery — so the epoch-store plumbing works. BUT FNBp9xzG's assignment is
  IDENTICAL (0xdf3d read5 write11, NOT CANCELLED_READ = u64::MAX+1) in both size=0 AND size=1 calls.
  => is_crashed_transaction(FNBp9xzG) returns FALSE even when crashed_set_size==1.
- HYPOTHESIS: the crashed set present at version-assignment time does NOT actually contain FNBp9xzG
  (its one element is a different digest, or a representation mismatch), so the is_crashed-keyed
  exemption never matches the poison tx.
- EXPERIMENT 4: log the crashed set CONTENTS (not just size) at version assignment.
- RESULT: the crashed set DOES contain FNBp9xzG (82 calls show set={FNBp9xzG}); plumbing is fine.
  Yet FNBp9xzG still advances 0xdf3d 5->11 (not CANCELLED_READ). Refined hypothesis: FNBp9xzG's OWN
  commit is version-assigned only while the set is empty (before the crash) and is NOT re-assigned
  on recovery (commit already processed/idempotent); the set={FNBp9xzG} calls are OTHER commits.
  So the is_crashed exemption is never consulted for FNBp9xzG's commit.

# iteration 5
- HYPOTHESIS: is_dropped(FNBp9xzG) is false at the (single, pre-crash) assignment of FNBp9xzG's
  commit, because the commit is assigned before FNBp9xzG crashes and not re-assigned afterwards.
- EXPERIMENT 5: log, per tx at assignment, its digest + is_dropped + the crashed-set membership, so
  we directly see is_dropped for FNBp9xzG's commit and the set at that moment.
