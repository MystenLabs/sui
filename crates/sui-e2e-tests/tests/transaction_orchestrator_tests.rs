// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use prometheus::Registry;
use std::time::Duration;
use sui_core::authority::EffectsNotifyRead;
use sui_core::authority_client::NetworkAuthorityClient;
use sui_core::transaction_orchestrator::TransactiondOrchestrator;
use sui_macros::sim_test;
use sui_test_transaction_builder::{
    batch_make_transfer_transactions, make_transfer_sui_transaction,
};
use sui_types::quorum_driver_types::{
    ExecuteTransactionRequest, ExecuteTransactionRequestType, ExecuteTransactionResponse,
    FinalizedEffects, QuorumDriverError,
};
use sui_types::transaction::Transaction;
use test_cluster::TestClusterBuilder;
use tokio::time::timeout;
use tracing::info;

#[sim_test]
async fn test_blocking_execution() -> Result<(), anyhow::Error> {
    let mut test_cluster = TestClusterBuilder::new().build().await;
    let context = &mut test_cluster.wallet;
    let handle = &test_cluster.fullnode_handle.sui_node;

    let temp_dir = tempfile::tempdir().unwrap();
    let registry = Registry::new();
    // Start orchestrator inside container so that it will be properly shutdown.
    let orchestrator = handle
        .with(|node| {
            TransactiondOrchestrator::new_with_network_clients(
                node.state(),
                node.subscribe_to_epoch_change(),
                temp_dir.path(),
                &registry,
            )
        })
        .unwrap();

    let txn_count = 4;
    let mut txns = batch_make_transfer_transactions(context, txn_count).await;
    assert!(
        txns.len() >= txn_count,
        "Expect at least {} txns. Do we generate enough gas objects during genesis?",
        txn_count,
    );

    // Quorum driver does not execute txn locally
    let txn = txns.swap_remove(0);
    let digest = *txn.digest();
    orchestrator
        .quorum_driver()
        .submit_transaction_no_ticket(txn)
        .await?;

    // Wait for data sync to catch up
    handle
        .state()
        .db()
        .notify_read_executed_effects(vec![digest])
        .await
        .unwrap();

    // Transaction Orchestrator proactivcely executes txn locally
    let txn = txns.swap_remove(0);
    let digest = *txn.digest();

    let res = execute_with_orchestrator(
        &orchestrator,
        txn,
        ExecuteTransactionRequestType::WaitForLocalExecution,
    )
    .await
    .unwrap_or_else(|e| panic!("Failed to execute transaction {:?}: {:?}", digest, e));

    let ExecuteTransactionResponse::EffectsCert(result) = res;
    let (_, _, executed_locally) = *result;
    assert!(executed_locally);

    assert!(handle
        .state()
        .get_executed_transaction_and_effects(digest)
        .await
        .is_ok());

    Ok(())
}

#[sim_test]
async fn test_fullnode_wal_log() -> Result<(), anyhow::Error> {
    telemetry_subscribers::init_for_testing();
    let mut test_cluster = TestClusterBuilder::new()
        .with_epoch_duration_ms(600000)
        .build()
        .await;

    let handle = &test_cluster.fullnode_handle.sui_node;

    let temp_dir = tempfile::tempdir().unwrap();
    tokio::task::yield_now().await;
    let registry = Registry::new();
    // Start orchestrator inside container so that it will be properly shutdown.
    let orchestrator = handle
        .with(|node| {
            TransactiondOrchestrator::new_with_network_clients(
                node.state(),
                node.subscribe_to_epoch_change(),
                temp_dir.path(),
                &registry,
            )
        })
        .unwrap();

    let txn_count = 2;
    let context = &mut test_cluster.wallet;
    let mut txns = batch_make_transfer_transactions(context, txn_count).await;
    assert!(
        txns.len() >= txn_count,
        "Expect at least {} txns. Do we generate enough gas objects during genesis?",
        txn_count,
    );
    // As a comparison, we first verify a tx can go through
    let txn = txns.swap_remove(0);
    let digest = *txn.digest();
    execute_with_orchestrator(
        &orchestrator,
        txn,
        ExecuteTransactionRequestType::WaitForLocalExecution,
    )
    .await
    .unwrap_or_else(|e| panic!("Failed to execute transaction {:?}: {:?}", digest, e));

    let validator_addresses = test_cluster.get_validator_pubkeys();
    assert_eq!(validator_addresses.len(), 4);

    // Stop 2 validators and we lose quorum
    test_cluster.stop_node(&validator_addresses[0]);
    test_cluster.stop_node(&validator_addresses[1]);

    let txn = txns.swap_remove(0);
    // Expect tx to fail
    execute_with_orchestrator(
        &orchestrator,
        txn.clone(),
        ExecuteTransactionRequestType::WaitForLocalExecution,
    )
    .await
    .unwrap_err();

    // Because the tx did not go through, we expect to see it in the WAL log
    let pending_txes: Vec<_> = orchestrator
        .load_all_pending_transactions()
        .into_iter()
        .map(|t| t.into_inner())
        .collect();
    assert_eq!(pending_txes, vec![txn.clone()]);

    // Bring up 1 validator, we obtain quorum again and tx should succeed
    test_cluster.start_node(&validator_addresses[0]).await;
    tokio::task::yield_now().await;
    execute_with_orchestrator(
        &orchestrator,
        txn,
        ExecuteTransactionRequestType::WaitForLocalExecution,
    )
    .await
    .unwrap();

    // TODO: wal erasing is done in the loop handling effects, so may have some delay.
    // However, once the refactoring is completed the wal removal will be done before
    // response is returned and we will not need the sleep.
    tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;
    // The tx should be erased in wal log.
    let pending_txes = orchestrator.load_all_pending_transactions();
    assert!(pending_txes.is_empty());

    Ok(())
}

#[sim_test]
async fn test_transaction_orchestrator_reconfig() {
    telemetry_subscribers::init_for_testing();
    let test_cluster = TestClusterBuilder::new().build().await;
    let epoch = test_cluster.fullnode_handle.sui_node.with(|node| {
        node.transaction_orchestrator()
            .unwrap()
            .quorum_driver()
            .current_epoch()
    });
    assert_eq!(epoch, 0);

    test_cluster.trigger_reconfiguration().await;

    // After epoch change on a fullnode, there could be a delay before the transaction orchestrator
    // updates its committee (happens asynchronously after receiving a reconfig message). Use a timeout
    // to make the test more reliable.
    timeout(Duration::from_secs(5), async {
        loop {
            let epoch = test_cluster.fullnode_handle.sui_node.with(|node| {
                node.transaction_orchestrator()
                    .unwrap()
                    .quorum_driver()
                    .current_epoch()
            });
            if epoch == 1 {
                break;
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    })
    .await
    .unwrap();

    assert_eq!(
        test_cluster.fullnode_handle.sui_node.with(|node| node
            .clone_authority_aggregator()
            .unwrap()
            .committee
            .epoch),
        1
    );
}

#[sim_test]
async fn test_tx_across_epoch_boundaries() {
    telemetry_subscribers::init_for_testing();
    let total_tx_cnt = 1;
    let (result_tx, mut result_rx) = tokio::sync::mpsc::channel::<FinalizedEffects>(total_tx_cnt);

    let test_cluster = TestClusterBuilder::new().build().await;
    let tx = make_transfer_sui_transaction(&test_cluster.wallet, None, None).await;
    let authorities = test_cluster.swarm.validator_node_handles();

    // We first let 2 validators stop accepting user cert
    // to make sure QD does not get quorum until reconfig
    for handle in authorities.iter().take(2) {
        handle
            .with_async(|node| async { node.close_epoch_for_testing().await.unwrap() })
            .await;
    }

    // Spawn a task that fire the transaction through TransactionOrchestrator
    // across the epoch boundary.
    let to = test_cluster
        .fullnode_handle
        .sui_node
        .with(|node| node.transaction_orchestrator().unwrap());

    let tx_digest = *tx.digest();
    info!(?tx_digest, "Submitting tx");
    tokio::task::spawn(async move {
        match to
            .execute_transaction_block(ExecuteTransactionRequest {
                transaction: tx.clone(),
                request_type: ExecuteTransactionRequestType::WaitForEffectsCert,
            })
            .await
        {
            Ok(ExecuteTransactionResponse::EffectsCert(res)) => {
                info!(?tx_digest, "tx result: ok");
                let (effects_cert, _, _) = *res;
                result_tx.send(effects_cert).await.unwrap();
            }
            Err(QuorumDriverError::TimeoutBeforeFinality) => {
                info!(?tx_digest, "tx result: timeout and will retry")
            }
            Err(other) => panic!("unexpected error: {:?}", other),
        }
    });

    info!("Asking remaining validators to change epoch");
    // Ask the remaining 2 validators to close epoch
    for handle in authorities.iter().skip(2) {
        handle
            .with_async(|node| async { node.close_epoch_for_testing().await.unwrap() })
            .await;
    }

    // Wait for the network to reach the next epoch.
    test_cluster.wait_for_epoch(Some(1)).await;

    // The transaction must finalize in epoch 1
    let start = std::time::Instant::now();
    match tokio::time::timeout(tokio::time::Duration::from_secs(15), result_rx.recv()).await {
        Ok(Some(effects_cert)) if effects_cert.epoch() == 1 => (),
        other => panic!("unexpected error: {:?}", other),
    }
    info!("test completed in {:?}", start.elapsed());
}

async fn execute_with_orchestrator(
    orchestrator: &TransactiondOrchestrator<NetworkAuthorityClient>,
    txn: Transaction,
    request_type: ExecuteTransactionRequestType,
) -> Result<ExecuteTransactionResponse, QuorumDriverError> {
    orchestrator
        .execute_transaction_block(ExecuteTransactionRequest {
            transaction: txn,
            request_type,
        })
        .await
}
