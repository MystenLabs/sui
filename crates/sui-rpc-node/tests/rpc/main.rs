// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Integration tests for the `sui-rpc-node` binary. Each module
//! below runs against a [`cluster::LocalCluster`] — a single-process
//! harness pairing a [`simulacrum::Simulacrum`] with an in-process
//! [`sui_rpc_node`] service. Tests assert on what the rpc-api
//! surface returns after the indexer has caught up to a synthetic
//! checkpoint.
//!
//! Layout mirrors `crates/sui-e2e-tests/tests/rpc/`: per-service
//! submodules under [`v2`] hold one test file per gRPC method.
//! Tests that need on-chain effects drive them through Simulacrum
//! (`LocalCluster::execute_transaction` /
//! `LocalCluster::create_checkpoint`) and read them back via the
//! rpc-api gRPC clients.
//!
//! # Tests intentionally not ported from `sui-e2e-tests`
//!
//! - `transaction_execution_service::*` (every test in the
//!   module) — needs a `TransactionExecutor` impl wired into
//!   `RpcService::with_executor`. The binary doesn't yet set
//!   one up; see `crates/sui-rpc-node/src/rpc.rs`'s TODO.
//! - `client.rs::execute_transaction_transfer` — same
//!   `TransactionExecutor` gap as
//!   `transaction_execution_service`.
//! - `client.rs::get_checkpoint_artifacts` — needs
//!   `ProtocolConfig::apply_overrides_for_testing` to enable
//!   the artifacts digest field; that override is
//!   process-global and would conflict with the per-test
//!   Simulacrum instance.
//! - `subscription_service.rs` — needs a
//!   `SubscriptionService` handle on the rpc-api, which the
//!   binary doesn't construct (the rpc-store is read-only and
//!   doesn't generate the executor-side subscription events).
//! - `signature_verification_service.rs` — the zkLogin test
//!   depends on `with_default_jwks` + epoch transitions with
//!   authenticator state updates that Simulacrum doesn't
//!   expose.
//! - `ledger_service::get_epoch::get_epoch_protocol_config_exposes_gasless_allowlist`
//!   and `state_service::balance::test_address_balance*` —
//!   both call `ProtocolConfig::apply_overrides_for_testing`
//!   (gasless allowlist / accumulator enablement). Those
//!   overrides are process-global and would clash with other
//!   tests sharing this binary's Simulacrum.
//! - `state_service::balance::test_balance_apis` — the e2e
//!   version asserts on `TestClusterBuilder`'s fixed
//!   `INITIAL_SUI_BALANCE` (150 Peta MIST) grant. Simulacrum's
//!   `funded_account` takes the amount as a parameter, so the
//!   port asserts the requested amount instead.
//! - `unchanged_loaded_runtime_objects::test_unchanged_loaded_runtime_objects`
//!   — depends on `stake_with_validator(&test_cluster)` (no
//!   multi-validator concept in Simulacrum) and on a hard-coded
//!   `validator_set` object address that's specific to
//!   `TestClusterBuilder`'s setup. The three TTO tests in that
//!   file are ported.
//!
//! # Tests intentionally not ported from `sui-indexer-alt-e2e-tests`
//!
//! Only one e2e test remains unported.
//!
//! - `consistent_store_list_owned_objects_tests::test_coin_balance_change_cleanup`
//!   pins `Simulacrum::new_with_protocol_version(rng, 27)` to
//!   reproduce an Effects V1 indexing bug. Our `LocalCluster`
//!   doesn't expose protocol-version pinning, and the
//!   regression itself lives in
//!   `sui-core::transaction_outputs` — the rpc-store reader
//!   paths see what the upstream effects builder produces
//!   either way, so the bug doesn't surface here.

mod client;
mod cluster;
mod v1alpha;
mod v2;
