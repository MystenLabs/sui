// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Mirrors the read-only subset of
//! `sui-e2e-tests/tests/rpc/client.rs`. The `Client` type from
//! `sui-rpc-api` is a thin wrapper around the gRPC stubs; these
//! tests verify that the rpc-node's HTTP listener answers via
//! that high-level adapter as well as via the raw gRPC clients
//! exercised elsewhere.
//!
//! Skipped:
//!
//! - `execute_transaction_transfer` — exercises the
//!   `TransactionExecutor` path that the binary doesn't yet wire
//!   up; covered by the same exclusion as
//!   `transaction_execution_service::*`.
//! - `get_checkpoint_artifacts` — needs
//!   `ProtocolConfig::apply_overrides_for_testing` to enable the
//!   artifacts digest field, which is process-global and
//!   conflicts with the shared per-test Simulacrum.

use sui_rpc_api::Client;
use sui_sdk_types::Address;
use sui_test_transaction_builder::TestTransactionBuilder;
use sui_types::utils::to_sender_signed_transaction;

use crate::cluster::LocalCluster;

#[tokio::test]
async fn get_object() {
    let cluster = LocalCluster::new().await.unwrap();
    let mut client = Client::new(cluster.grpc_url().to_string()).unwrap();

    let id: Address = "0x5".parse().unwrap();

    let _object = client.get_object(id.into()).await.unwrap();

    let _object = client
        .get_object_with_version(id.into(), 1.into())
        .await
        .unwrap();
}

#[tokio::test]
async fn get_full_checkpoint() {
    let cluster = LocalCluster::new().await.unwrap();

    // Submit a transfer so the latest checkpoint contains at
    // least one user transaction in addition to the genesis
    // entries.
    let (sender, keypair, gas) = cluster.funded_account(10_000_000_000).await.unwrap();
    let rgp = cluster.reference_gas_price().await;
    let tx_data = TestTransactionBuilder::new(sender, gas, rgp)
        .transfer_sui(Some(1), sender)
        .build();
    let signed = to_sender_signed_transaction(tx_data, &keypair);
    let (_effects, err) = cluster.execute_transaction(signed).await.unwrap();
    assert!(err.is_none(), "transfer should succeed: {err:?}");
    let checkpoint = cluster.create_checkpoint().await.unwrap();

    let mut client = Client::new(cluster.grpc_url().to_string()).unwrap();

    let latest = client.get_latest_checkpoint().await.unwrap().into_data();
    assert!(latest.sequence_number >= checkpoint.sequence_number);

    let _full = client
        .get_full_checkpoint(latest.sequence_number)
        .await
        .unwrap();
}
