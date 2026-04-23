// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::num::NonZeroUsize;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use sui_config::node::AuthorityOverloadConfig;
use sui_core::authority_client::{
    AuthorityAPI, make_network_authority_clients_with_network_config,
};
use sui_macros::{register_fail_point_if, sim_test};
use sui_network::default_mysten_network_config;
use sui_swarm_config::network_config_builder::ConfigBuilder;
use sui_test_transaction_builder::TestTransactionBuilder;
use sui_types::messages_grpc::SubmitTxRequest;
use sui_types::transaction::Transaction;
use test_cluster::TestClusterBuilder;

async fn make_transfer_tx_with_gas_price(
    cluster: &test_cluster::TestCluster,
    gas_price: u64,
) -> Transaction {
    let sender = cluster.get_address_0();
    let gas = cluster
        .wallet
        .get_one_gas_object_owned_by_address(sender)
        .await
        .unwrap()
        .unwrap();
    let tx_data = TestTransactionBuilder::new(sender, gas, gas_price)
        .transfer_sui(Some(1), sender)
        .build();
    cluster.wallet.sign_transaction(&tx_data).await
}

/// Verify that transactions flow through the admission queue successfully.
#[sim_test]
async fn test_admission_queue_basic() {
    let config = AuthorityOverloadConfig {
        admission_queue_enabled: true,
        admission_queue_bypass_fraction: 0.0,
        admission_queue_capacity_fraction: 0.001,
        ..Default::default()
    };

    let cluster = TestClusterBuilder::new()
        .with_authority_overload_config(config)
        .build()
        .await;

    let rgp = cluster.get_reference_gas_price().await;

    let tx = make_transfer_tx_with_gas_price(&cluster, rgp).await;
    let result = cluster.wallet.execute_transaction_may_fail(tx).await;
    assert!(result.is_ok(), "Transaction at RGP should succeed");

    let tx = make_transfer_tx_with_gas_price(&cluster, rgp * 10).await;
    let result = cluster.wallet.execute_transaction_may_fail(tx).await;
    assert!(result.is_ok(), "Transaction at 10x RGP should succeed");
}

/// Verify that:
/// - A low gas price tx is rejected when the queue is full.
/// - A high gas price tx evicts a low gas price entry and is accepted.
#[sim_test]
async fn test_admission_queue_eviction_and_rejection() {
    let overload_config = AuthorityOverloadConfig {
        admission_queue_enabled: true,
        admission_queue_bypass_fraction: 0.0,
        // capacity = 20_000 * 0.0001 = 2
        admission_queue_capacity_fraction: 0.0001,
        ..Default::default()
    };

    // Drain is disabled via an AtomicBool so we can re-enable it at the end.
    let drain_disabled = Arc::new(AtomicBool::new(true));

    // Build network config with overload settings, and extract the committee
    // for creating direct authority clients.
    let network_config = ConfigBuilder::new_with_temp_dir()
        .committee_size(NonZeroUsize::new(4).unwrap())
        .with_authority_overload_config(overload_config)
        .build();
    let committee = network_config.committee_with_network();

    let cluster = TestClusterBuilder::new()
        .set_network_config(network_config)
        .build()
        .await;

    // Register the drain-disable failpoint only on the target validator.
    let validator = cluster.all_validator_handles().into_iter().next().unwrap();
    let dd = drain_disabled.clone();
    validator.with(|_| {
        register_fail_point_if("admission_queue_disable_drain", move || {
            dd.load(Ordering::SeqCst)
        });
    });

    // Create a direct gRPC client to the target validator.
    let local_clients = make_network_authority_clients_with_network_config(
        &committee,
        &default_mysten_network_config(),
    );
    let target_name = validator.with(|node| node.state().name);
    let auth_client = local_clients.get(&target_name).unwrap();

    let rgp = cluster.get_reference_gas_price().await;

    // Build transactions with different gas prices.
    let tx1 = make_transfer_tx_with_gas_price(&cluster, rgp).await;
    let tx2 = make_transfer_tx_with_gas_price(&cluster, rgp).await;
    let tx_low = make_transfer_tx_with_gas_price(&cluster, rgp).await;
    let tx_high = make_transfer_tx_with_gas_price(&cluster, rgp * 10).await;

    // Fill the queue (capacity 2). submit_transaction blocks waiting for the
    // consensus position when accepted (draining is disabled), so we spawn them.
    let fill1 = tokio::spawn({
        let c = auth_client.clone();
        async move {
            c.submit_transaction(SubmitTxRequest::new_transaction(tx1), None)
                .await
        }
    });
    let fill2 = tokio::spawn({
        let c = auth_client.clone();
        async move {
            c.submit_transaction(SubmitTxRequest::new_transaction(tx2), None)
                .await
        }
    });

    // Wait for fills to reach the queue.
    tokio::time::sleep(std::time::Duration::from_secs(1)).await;

    // Queue is full. A low gas price tx should be rejected immediately.
    let r_low = auth_client
        .submit_transaction(SubmitTxRequest::new_transaction(tx_low), None)
        .await;
    let err = r_low.expect_err("Low gas price tx should be rejected when queue is full");
    assert!(
        err.to_string().contains("outbid"),
        "Expected outbid error, got: {err}"
    );

    // A high gas price tx should evict an RGP entry and be accepted.
    // It will block (drainer disabled), so spawn it.
    let high = tokio::spawn({
        let c = auth_client.clone();
        async move {
            c.submit_transaction(SubmitTxRequest::new_transaction(tx_high), None)
                .await
        }
    });

    // Re-enable draining so all accepted transactions complete.
    drain_disabled.store(false, Ordering::SeqCst);

    // Await all spawned tasks and verify results.
    // One of fill1/fill2 was evicted by the high-gas tx (its oneshot was dropped),
    // so one fill should fail and the other should succeed.
    let r_fill1 = fill1.await.unwrap();
    let r_fill2 = fill2.await.unwrap();
    let r_high = high.await.unwrap();

    // The high-gas tx should have been accepted and submitted successfully.
    assert!(
        r_high.is_ok(),
        "High gas price tx should succeed after drain re-enabled: {:?}",
        r_high.err()
    );

    // Exactly one of the two fills should have been evicted (error) by the high-gas tx.
    let fill_successes = [&r_fill1, &r_fill2].iter().filter(|r| r.is_ok()).count();
    assert_eq!(
        fill_successes, 1,
        "Expected one fill to succeed and one to be evicted. fill1: {:?}, fill2: {:?}",
        r_fill1, r_fill2
    );
}

/// Verify that the admission queue is properly recreated on epoch change.
#[sim_test]
async fn test_admission_queue_epoch_boundary_cleanup() {
    let config = AuthorityOverloadConfig {
        admission_queue_enabled: true,
        admission_queue_bypass_fraction: 0.0,
        admission_queue_capacity_fraction: 0.001,
        ..Default::default()
    };

    let cluster = TestClusterBuilder::new()
        .with_authority_overload_config(config)
        .build()
        .await;

    let rgp = cluster.get_reference_gas_price().await;

    let tx = make_transfer_tx_with_gas_price(&cluster, rgp).await;
    let result = cluster.wallet.execute_transaction_may_fail(tx).await;
    assert!(result.is_ok(), "Transaction should succeed in epoch 0");

    cluster.trigger_reconfiguration().await;

    let tx = make_transfer_tx_with_gas_price(&cluster, rgp).await;
    let result = cluster.wallet.execute_transaction_may_fail(tx).await;
    assert!(
        result.is_ok(),
        "Transaction should succeed after epoch change: {:?}",
        result.err()
    );

    cluster.trigger_reconfiguration().await;

    let tx = make_transfer_tx_with_gas_price(&cluster, rgp).await;
    let result = cluster.wallet.execute_transaction_may_fail(tx).await;
    assert!(
        result.is_ok(),
        "Transaction should succeed after second epoch change: {:?}",
        result.err()
    );
}

/// Verify that epoch change with a pending admission queue entry does not
/// leave the system in a broken state. Submits a transaction to a validator
/// with draining disabled, triggers reconfiguration, re-enables draining,
/// and verifies:
/// - The pending transaction succeeds (retried in the new epoch).
/// - The system is healthy in the new epoch.
/// - A second reconfiguration completes successfully.
#[sim_test]
async fn test_admission_queue_reconfig_with_pending_entries() {
    let overload_config = AuthorityOverloadConfig {
        admission_queue_enabled: true,
        admission_queue_bypass_fraction: 0.0,
        admission_queue_capacity_fraction: 0.0001,
        ..Default::default()
    };

    let drain_disabled = Arc::new(AtomicBool::new(true));

    let network_config = ConfigBuilder::new_with_temp_dir()
        .committee_size(NonZeroUsize::new(4).unwrap())
        .with_authority_overload_config(overload_config)
        .build();
    let committee = network_config.committee_with_network();

    let cluster = TestClusterBuilder::new()
        .set_network_config(network_config)
        .build()
        .await;

    let validator = cluster.all_validator_handles().into_iter().next().unwrap();

    // Register drain-disable failpoint on the target validator.
    let dd = drain_disabled.clone();
    validator.with(|_| {
        register_fail_point_if("admission_queue_disable_drain", move || {
            dd.load(Ordering::SeqCst)
        });
    });

    let local_clients = make_network_authority_clients_with_network_config(
        &committee,
        &default_mysten_network_config(),
    );
    let target_name = validator.with(|node| node.state().name);
    let auth_client = local_clients.get(&target_name).unwrap();

    let rgp = cluster.get_reference_gas_price().await;
    let tx1 = make_transfer_tx_with_gas_price(&cluster, rgp).await;

    // Submit a fill via the validator's gRPC interface. With draining disabled,
    // the RPC will block waiting for the consensus position.
    let fill = tokio::spawn({
        let c = auth_client.clone();
        async move {
            c.submit_transaction(SubmitTxRequest::new_transaction(tx1), None)
                .await
        }
    });

    // Give the fill time to reach the queue.
    tokio::time::sleep(std::time::Duration::from_secs(1)).await;

    // Trigger reconfiguration while the fill is pending.
    cluster.trigger_reconfiguration().await;

    // Re-enable draining so the new epoch's queue works normally.
    drain_disabled.store(false, Ordering::SeqCst);

    // The fill was pending when reconfig replaced the queue. The RPC handler's
    // retry loop catches ValidatorHaltedAtEpochEnd and resubmits in the new
    // epoch, so the fill succeeds.
    let r_fill = fill.await.unwrap();
    assert!(
        r_fill.is_ok(),
        "fill should succeed via retry in new epoch: {:?}",
        r_fill,
    );

    // Verify the system is healthy in the new epoch.
    let tx = make_transfer_tx_with_gas_price(&cluster, rgp).await;
    let result = cluster.wallet.execute_transaction_may_fail(tx).await;
    assert!(
        result.is_ok(),
        "Transaction should succeed in new epoch: {:?}",
        result.err()
    );

    // A second reconfiguration should succeed (no stale state).
    cluster.trigger_reconfiguration().await;

    let tx = make_transfer_tx_with_gas_price(&cluster, rgp).await;
    let result = cluster.wallet.execute_transaction_may_fail(tx).await;
    assert!(
        result.is_ok(),
        "Transaction should succeed after second reconfig: {:?}",
        result.err()
    );
}
