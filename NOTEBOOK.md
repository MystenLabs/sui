# Debugging test_composite_workload flakiness

## Reproduction
- Seed: `1769969825141`
- Command: `MSIM_TEST_SEED=1769969825141 SUI_PROTOCOL_CONFIG_CHAIN_OVERRIDE=mainnet RUST_LOG=sui=debug,info cargo simtest --test simtest -E 'test(=test::test_composite_workload)' --no-capture`
- Failure: `assertion failed: metrics_sum.insufficient_funds_count > 2`

## Iteration 1

### OBSERVATIONS
- The test expects `insufficient_funds_count > 2` but only gets 2
- The scheduler logs show 6 unique transaction digests with `status=InsufficientFunds` at the validator level
- But the client metrics only count 2 `insufficient_funds_count`
- The test code checks `effects.is_cancelled()` BEFORE `effects.is_insufficient_funds()`:
  ```rust
  if effects.is_cancelled() {
      metrics.record_cancellation(op_set);
  } else if effects.is_insufficient_funds() {
      metrics.record_insufficient_funds(op_set);
  }
  ```
- `is_cancelled()` looks for `ExecutionCancelledDueToSharedObjectCongestion`
- `is_insufficient_funds()` looks for `InsufficientFundsForWithdraw`
- The failure only happens with `SUI_PROTOCOL_CONFIG_CHAIN_OVERRIDE=mainnet`
- Without mainnet config, 200 seeds passed with no failures

### HYPOTHESIS
The mainnet protocol config has different congestion control settings that cause transactions which would fail with InsufficientFundsForWithdraw to instead be cancelled due to shared object congestion. Since the check order in the test code checks for cancellation first, these transactions are counted as cancellations rather than insufficient funds failures.

### EXPERIMENT
Add logging to understand:
1. What execution error is returned for transactions that have insufficient funds
2. Whether there's a difference in congestion control behavior between mainnet and default configs
