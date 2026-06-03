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
//! - `subscription_service.rs` — needs a
//!   `SubscriptionService` handle on the rpc-api, which the
//!   binary doesn't construct either.
//! - `signature_verification_service.rs` — zkLogin test
//!   depends on `with_default_jwks` + epoch transitions with
//!   authenticator state updates that Simulacrum doesn't
//!   expose.
//! - `unchanged_loaded_runtime_objects.rs` — exercises a TTO
//!   shape that requires a custom on-disk Move package.
//! - `state_service::balance::test_address_balance*` — needs
//!   `ProtocolConfig::apply_overrides_for_testing` to enable
//!   accumulators, which is process-global and clashes with the
//!   shared Simulacrum instance.
//! - `state_service::balance::test_custom_coin_balance`,
//!   `state_service::list_owned_objects::test_indexing_with_tto`,
//!   and every `move_package_service` test that publishes one
//!   of the on-disk `crates/sui-e2e-tests/tests/rpc/data/*`
//!   Move packages — needs `sui-move-build` set up against
//!   that on-disk Move project layout.
//! - `client.rs::execute_transaction_transfer` and
//!   `get_checkpoint_artifacts` — same `TransactionExecutor`
//!   gap as `transaction_execution_service`, plus the artifact
//!   tests need post-`apply_overrides_for_testing` protocol
//!   features.
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

mod cluster;
mod v1alpha;
mod v2;
