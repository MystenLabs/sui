# Future: Extend shared test cluster to cli_tests

## Context

The shell tests now support an external shared cluster via `SUI_TEST_CLUSTER_CONFIG_DIR`.
The same approach can eliminate ~41 TestCluster startups across ~134 cli_tests.

## Current state of cli_tests

File: `crates/sui/tests/cli_tests.rs` (~5000+ lines, ~134 test functions)

cli_tests use TestCluster more deeply than shell_tests:
- `test_cluster.get_address_0/1/2()` — pre-funded genesis addresses
- `test_cluster.wallet` / `wallet_mut()` — WalletContext for signing/execution
- `test_cluster.get_reference_gas_price()` — gas price via direct API
- `context.grpc_client()` — gRPC client from WalletContext
- `context.sign_transaction()` / `execute_transaction_may_fail()` — direct execution

## Approach

### 1. Create a helper that returns a WalletContext from the shared cluster

```rust
/// If SUI_TEST_CLUSTER_CONFIG_DIR is set, load WalletContext from a copy of the
/// shared config. Fund via faucet. Otherwise, fall back to per-test TestCluster.
async fn get_test_context() -> (Option<TestCluster>, WalletContext) { ... }
```

When using the external cluster:
- Copy client.yaml + keystore from SUI_TEST_CLUSTER_CONFIG_DIR to a temp dir
- Load WalletContext from the copy
- Request faucet gas
- Return (None, wallet_context)

When no external cluster:
- Create TestCluster as today
- Return (Some(test_cluster), test_cluster.wallet)

### 2. API migration guide

| Current TestCluster API | Replacement with WalletContext |
|---|---|
| `test_cluster.get_address_0()` | `context.active_address()` |
| `test_cluster.get_reference_gas_price()` | `context.get_reference_gas_price().await` |
| `test_cluster.wallet.sign_transaction()` | Works as-is (WalletContext has keystore) |
| `context.grpc_client()` | Works as-is (WalletContext has RPC URL) |

### 3. Tests with custom cluster configs

Some tests may configure TestCluster specially (specific epoch duration, multiple validators,
custom genesis, etc.). These cannot use the shared cluster and should keep creating their own.
Identify and exclude these during migration.

## CI changes

The `SUI_TEST_CLUSTER_CONFIG_DIR` env var is already set in the `test` and `windows-cli-tests`
CI jobs. No additional CI changes needed — cli_tests will automatically use the shared cluster
when the env var is present.

## Key files

- `crates/sui/tests/cli_tests.rs` — main file to modify
- `crates/sui/tests/shell_tests.rs` — reference implementation of shared cluster pattern
- `crates/sui-sdk/src/wallet_context.rs` — WalletContext API reference
