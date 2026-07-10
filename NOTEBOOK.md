# Debugging: fuzz_dynamic_committee "Transaction executed but checkpoint wait timed out"

Failing CI job: https://github.com/MystenLabs/sui/actions/runs/29061189525/job/86293168290?pr=27071
- Test: sui-e2e-tests::dynamic_committee_tests fuzz_dynamic_committee
- CI seed: MSIM_TEST_SEED=5947745660912475448 (derived from merge commit 528aa32a850f1d38...)
- Failed all 4 nextest retries in CI (deterministic in CI).
- Error: panic at test-cluster/src/lib.rs:990 — `execute_transaction_and_wait_for_checkpoint` returned
  `CheckpointTimeout`: transaction (request_add_stake) executed successfully with effects, but did not
  appear on the fullnode checkpoint subscription stream within 30s (simulated time).
- Response headers at failure time: x-sui-checkpoint-height: 89, x-sui-lowest-available-checkpoint: 89, epoch 0.

# iteration 1
- OBSERVATIONS
  - Recreated the EXACT CI tree: merge of PR head 9445d6f09 into main 71ea70f10; `git diff` vs CI merge
    commit 528aa32a85... is empty.
  - Ran exact repro: `MSIM_TEST_SEED=5947745660912475448 RUST_LOG=sui=debug,info cargo simtest --test dynamic_committee_tests -E 'test(=fuzz_dynamic_committee)' --no-capture` (experiment_1.log)
  - Result: PASSED locally (110.5s). Same seed + same code passes on arm64 macOS but fails on x86_64
    Linux CI. msim schedule diverges across platforms; deterministic within a platform.
- HYPOTHESIS: The checkpoint pipeline stall is real but timing/schedule-dependent; even in passing runs
  there may be an unusually long gap between checkpoints near the failure point (checkpoint ~89, epoch 0).
  Need to inspect the passing log for checkpoint cadence, then find a locally-failing seed via seed-search.
- EXPERIMENT: (a) Analyze experiment_1.log checkpoint timing; (b) run scripts/simtest/seed-search.py to
  find a seed that fails locally.
- RESULTS:
  - (a) Local passing run is totally healthy: 246 certified checkpoints over ~52s sim time, max gap 1.2s.
    At sim 02:30:14 (the moment CI's failing tx executed) local was at seq 87 vs CI's 89 — the two
    schedules track closely until CI hard-stalls right there. The CI stall is at the ~15th of 20 staking
    ops, ~7 sim-seconds before the reconfiguration point (locally at 02:30:21). Fullnode's embedded
    rpc-store index also keeps up locally (max 0.5s lag per checkpoint).
  - (b) seed-search: 58 random seeds all PASSED locally (stopped early). The failure likely requires
    CI's machine geometry, not just an unlucky seed.

# iteration 2
- OBSERVATIONS (code reading, no run)
  - PR 27071's retry machinery (`check_system_object_available` → `retry_request` →
    `wait_for_system_object_and_reenqueue`) has NO callers in this PR ([1/6] of a series). It is dead
    code; the PR is behaviorally inert for this test. The failure is a latent main issue; the PR only
    changed the merge SHA → seed → schedule.
  - The 30s wait is client-side: sui-rust-sdk `execute_transaction_and_wait_for_checkpoint` subscribes
    to the fullnode checkpoint stream, executes, fast-paths via `get_transaction` (reads the fullnode
    index), else waits on the stream.
  - Fullnode subscription delivery is GATED on the embedded rpc-store index:
    `SubscriptionService::handle_checkpoint` awaits `wait_until_indexed` (poll 10ms, up to 10s PER
    CHECKPOINT, WARN-level only — invisible in CI's RUST_LOG=error) in the single event loop; subscriber
    registrations queue behind it and a subscriber registered after checkpoint N is delivered misses N
    forever. `get_transaction` reads the same index. So an index stall ≥~30s (or missed-checkpoint +
    index lag combos) reproduces the exact CI symptom while validators keep making checkpoints.
  - Cross-platform divergence source found: `num_cpus::get()` is NOT virtualized by msim and feeds
    indexer pipeline channel sizes/adaptive concurrency (sui-indexer-alt-framework pipeline/mod.rs,
    sequential/mod.rs, concurrent/mod.rs), execution_driver.rs semaphore, authority.rs post-processing
    semaphore. Different host core count → different channel capacities → different msim schedule.
    Explains why the same seed passes on this 10-core mac but fails on the CI runner.
- HYPOTHESIS: With the CI runner's CPU count, the CI seed reproduces the failure locally (schedule is
  determined by seed + num_cpus-derived sizes). Then we can observe which component stalls.
- EXPERIMENT: Add CLAUDE_FAKE_NUM_CPUS env override at all 11 num_cpus::get() sites reachable in the
  sim (indexer pipeline x9, execution_driver x1, authority x1). Run the CI seed with
  CLAUDE_FAKE_NUM_CPUS in {16, 32, 4, 64, 8}.
- RESULTS: TBD
