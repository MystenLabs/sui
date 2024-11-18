// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;
use std::time::Duration;
use sui_core::authority_client::NetworkAuthorityClient;
use sui_core::transaction_orchestrator::TransactiondOrchestrator;
use sui_macros::sim_test;
use sui_storage::key_value_store::TransactionKeyValueStore;
use sui_storage::key_value_store_metrics::KeyValueStoreMetrics;
use sui_test_transaction_builder::{
    batch_make_transfer_transactions, make_staking_transaction, make_transfer_sui_transaction,
};
use sui_types::effects::TransactionEffectsAPI;
use sui_types::quorum_driver_types::{
    ExecuteTransactionRequestType, ExecuteTransactionRequestV3, ExecuteTransactionResponseV3,
    FinalizedEffects, IsTransactionExecutedLocally, QuorumDriverError,
};
use sui_types::transaction::Transaction;
use test_cluster::TestClusterBuilder;
use tokio::time::timeout;
use tracing::info;

fn make_socket_addr() -> std::net::SocketAddr {
    std::net::SocketAddr::new([127, 0, 0, 1].into(), 0)
}

#[sim_test]
async fn test_blocking_execution() -> Result<(), anyhow::Error> {
    let mut test_cluster = TestClusterBuilder::new().build().await;
    let context = &mut test_cluster.wallet;
    let handle = &test_cluster.fullnode_handle.sui_node;
    let orchestrator = handle.with(|n| n.transaction_orchestrator().as_ref().unwrap().clone());

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
        .submit_transaction_no_ticket(
            ExecuteTransactionRequestV3::new_v2(txn),
            Some(make_socket_addr()),
        )
        .await?;

    // Wait for data sync to catch up
    handle
        .state()
        .get_transaction_cache_reader()
        .notify_read_executed_effects(&[digest])
        .await;

    // Transaction Orchestrator proactivcely executes txn locally
    let txn = txns.swap_remove(0);
    let digest = *txn.digest();

    let (_, executed_locally) = execute_with_orchestrator(
        &orchestrator,
        txn,
        ExecuteTransactionRequestType::WaitForLocalExecution,
    )
    .await
    .unwrap_or_else(|e| panic!("Failed to execute transaction {:?}: {:?}", digest, e));

    assert!(executed_locally);

    let metrics = KeyValueStoreMetrics::new_for_tests();
    let kv_store = Arc::new(TransactionKeyValueStore::new(
        "rocksdb",
        metrics,
        handle.state(),
    ));

    assert!(handle
        .state()
        .get_executed_transaction_and_effects(digest, kv_store)
        .await
        .is_ok());

    Ok(())
}

#[sim_test]
async fn test_fullnode_wal_log() -> Result<(), anyhow::Error> {
    #[cfg(msim)]
    {
        use sui_core::authority::{init_checkpoint_timeout_config, CheckpointTimeoutConfig};
        init_checkpoint_timeout_config(CheckpointTimeoutConfig {
            warning_timeout: Duration::from_secs(2),
            panic_timeout: None,
        });
    }
    telemetry_subscribers::init_for_testing();
    let mut test_cluster = TestClusterBuilder::new()
        .with_epoch_duration_ms(600000)
        .build()
        .await;

    let handle = &test_cluster.fullnode_handle.sui_node;
    let orchestrator = handle.with(|n| n.transaction_orchestrator().as_ref().unwrap().clone());

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
            .execute_transaction_block(
                ExecuteTransactionRequestV3::new_v2(tx.clone()),
                ExecuteTransactionRequestType::WaitForEffectsCert,
                None,
            )
            .await
        {
            Ok((response, _)) => {
                info!(?tx_digest, "tx result: ok");
                result_tx.send(response.effects).await.unwrap();
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
) -> Result<(ExecuteTransactionResponseV3, IsTransactionExecutedLocally), QuorumDriverError> {
    orchestrator
        .execute_transaction_block(ExecuteTransactionRequestV3::new_v2(txn), request_type, None)
        .await
}

#[sim_test]
async fn execute_transaction_v3() -> Result<(), anyhow::Error> {
    let mut test_cluster = TestClusterBuilder::new().build().await;
    let context = &mut test_cluster.wallet;
    let handle = &test_cluster.fullnode_handle.sui_node;
    let orchestrator = handle.with(|n| n.transaction_orchestrator().as_ref().unwrap().clone());

    let txn_count = 1;
    let mut txns = batch_make_transfer_transactions(context, txn_count).await;
    assert!(
        txns.len() >= txn_count,
        "Expect at least {} txns. Do we generate enough gas objects during genesis?",
        txn_count,
    );

    // Quorum driver does not execute txn locally
    let txn = txns.swap_remove(0);

    let request = ExecuteTransactionRequestV3 {
        transaction: txn,
        include_events: true,
        include_input_objects: true,
        include_output_objects: true,
        include_auxiliary_data: false,
    };
    let response = orchestrator.execute_transaction_v3(request, None).await?;
    let fx = &response.effects.effects;

    let mut expected_input_objects = fx.modified_at_versions();
    expected_input_objects.sort_by_key(|&(id, _version)| id);
    let mut expected_output_objects = fx
        .all_changed_objects()
        .into_iter()
        .map(|(object_ref, _, _)| object_ref)
        .collect::<Vec<_>>();
    expected_output_objects.sort_by_key(|&(id, _version, _digest)| id);

    let mut actual_input_objects_received = response
        .input_objects
        .unwrap()
        .iter()
        .map(|object| (object.id(), object.version()))
        .collect::<Vec<_>>();
    actual_input_objects_received.sort_by_key(|&(id, _version)| id);
    assert_eq!(expected_input_objects, actual_input_objects_received);

    let mut actual_output_objects_received = response
        .output_objects
        .unwrap()
        .iter()
        .map(|object| (object.id(), object.version(), object.digest()))
        .collect::<Vec<_>>();
    actual_output_objects_received.sort_by_key(|&(id, _version, _digest)| id);
    assert_eq!(expected_output_objects, actual_output_objects_received);

    Ok(())
}

#[sim_test]
async fn execute_transaction_v3_staking_transaction() -> Result<(), anyhow::Error> {
    let mut test_cluster = TestClusterBuilder::new().build().await;
    let context = &mut test_cluster.wallet;
    let handle = &test_cluster.fullnode_handle.sui_node;
    let orchestrator = handle.with(|n| n.transaction_orchestrator().as_ref().unwrap().clone());

    let validator_address = context
        .get_client()
        .await?
        .governance_api()
        .get_latest_sui_system_state()
        .await?
        .active_validators
        .first()
        .unwrap()
        .sui_address;
    let transaction = make_staking_transaction(context, validator_address).await;

    let request = ExecuteTransactionRequestV3 {
        transaction,
        include_events: true,
        include_input_objects: true,
        include_output_objects: true,
        include_auxiliary_data: false,
    };
    let response = orchestrator.execute_transaction_v3(request, None).await?;
    let fx = &response.effects.effects;

    let mut expected_input_objects = fx.modified_at_versions();
    expected_input_objects.sort_by_key(|&(id, _version)| id);
    let mut expected_output_objects = fx
        .all_changed_objects()
        .into_iter()
        .map(|(object_ref, _, _)| object_ref)
        .collect::<Vec<_>>();
    expected_output_objects.sort_by_key(|&(id, _version, _digest)| id);

    let mut actual_input_objects_received = response
        .input_objects
        .unwrap()
        .iter()
        .map(|object| (object.id(), object.version()))
        .collect::<Vec<_>>();
    actual_input_objects_received.sort_by_key(|&(id, _version)| id);
    assert_eq!(expected_input_objects, actual_input_objects_received);

    let mut actual_output_objects_received = response
        .output_objects
        .unwrap()
        .iter()
        .map(|object| (object.id(), object.version(), object.digest()))
        .collect::<Vec<_>>();
    actual_output_objects_received.sort_by_key(|&(id, _version, _digest)| id);
    assert_eq!(expected_output_objects, actual_output_objects_received);

    Ok(())
}
