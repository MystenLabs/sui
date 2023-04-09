// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use prometheus::Registry;
use sui_core::authority_client::NetworkAuthorityClient;
use sui_core::transaction_orchestrator::TransactiondOrchestrator;
use sui_macros::sim_test;
use sui_types::crypto::{get_key_pair, AccountKeyPair};
use sui_types::messages::{
    ExecuteTransactionRequest, ExecuteTransactionRequestType, ExecuteTransactionResponse,
    FinalizedEffects, TransactionData, VerifiedTransaction, TEST_ONLY_GAS_UNIT_FOR_TRANSFER,
};
use sui_types::object::generate_test_gas_objects_with_owner;
use sui_types::quorum_driver_types::QuorumDriverError;
use sui_types::utils::to_sender_signed_transaction;
use test_utils::authority::{
    spawn_fullnode, spawn_test_authorities, test_authority_configs,
    test_authority_configs_with_objects,
};
use test_utils::messages::make_transactions_with_wallet_context;
use test_utils::network::wait_for_nodes_transition_to_epoch;
use test_utils::network::TestClusterBuilder;
use test_utils::transaction::wait_for_tx;
use tracing::info;

#[sim_test]
async fn test_blocking_execution() -> Result<(), anyhow::Error> {
    let mut test_cluster = TestClusterBuilder::new().build().await?;
    let context = &mut test_cluster.wallet;
    let node = &test_cluster.fullnode_handle.sui_node;

    let temp_dir = tempfile::tempdir().unwrap();
    let reconfig_channel = node.subscribe_to_epoch_change();
    let orchestrator = TransactiondOrchestrator::new_with_network_clients(
        node.state(),
        reconfig_channel,
        temp_dir.path(),
        &Registry::new(),
    )
    .await
    .unwrap();

    let txn_count = 4;
    let mut txns = make_transactions_with_wallet_context(context, txn_count).await;
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
    wait_for_tx(digest, node.state().clone()).await;

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

    assert!(node
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
        .await?;

    let node = &test_cluster.fullnode_handle.sui_node;

    let temp_dir = tempfile::tempdir().unwrap();
    let reconfig_channel = node.subscribe_to_epoch_change();
    tokio::task::yield_now().await;
    let orchestrator = TransactiondOrchestrator::new_with_network_clients(
        node.state(),
        reconfig_channel,
        temp_dir.path(),
        &Registry::new(),
    )
    .await
    .unwrap();

    let txn_count = 2;
    let context = &mut test_cluster.wallet;
    let mut txns = make_transactions_with_wallet_context(context, txn_count).await;
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

    let validator_addresses = test_cluster.get_validator_addresses();
    assert_eq!(validator_addresses.len(), 4);

    // Stop 2 validators and we lose quorum
    test_cluster.stop_validator(validator_addresses[0]);
    test_cluster.stop_validator(validator_addresses[1]);

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
    let pending_txes = orchestrator.load_all_pending_transactions();
    assert_eq!(pending_txes, vec![txn.clone()]);

    // Bring up 1 validator, we obtain quorum again and tx should succeed
    test_cluster.start_validator(validator_addresses[0]).await;
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
    let config = test_authority_configs();
    let authorities = spawn_test_authorities(&config).await;
    let fullnode = spawn_fullnode(&config, None).await;
    let epoch = fullnode.with(|node| {
        node.transaction_orchestrator()
            .unwrap()
            .quorum_driver()
            .current_epoch()
    });
    assert_eq!(epoch, 0);

    for handle in &authorities {
        handle
            .with_async(|node| async { node.close_epoch_for_testing().await.unwrap() })
            .await;
    }

    // Wait for all nodes to reach the next epoch.
    wait_for_nodes_transition_to_epoch(authorities.iter().chain(std::iter::once(&fullnode)), 1)
        .await;

    // Give it some time for the update to happen
    tokio::time::sleep(tokio::time::Duration::from_secs(3)).await;
    fullnode.with(|node| {
        let epoch = node
            .transaction_orchestrator()
            .unwrap()
            .quorum_driver()
            .current_epoch();
        assert_eq!(epoch, 1);
        assert_eq!(
            node.clone_authority_aggregator().unwrap().committee.epoch,
            1
        );
    });
}

#[sim_test]
async fn test_tx_across_epoch_boundaries() {
    telemetry_subscribers::init_for_testing();
    let total_tx_cnt = 1;
    let (sender, keypair) = get_key_pair::<AccountKeyPair>();
    let gas_objects = generate_test_gas_objects_with_owner(1, sender);
    let (result_tx, mut result_rx) = tokio::sync::mpsc::channel::<FinalizedEffects>(total_tx_cnt);

    let (config, mut gas_objects) = test_authority_configs_with_objects(gas_objects);
    let authorities = spawn_test_authorities(&config).await;
    let fullnode = spawn_fullnode(&config, None).await;
    let rgp = authorities[0]
        .with(|node| node.state().reference_gas_price_for_testing())
        .unwrap();
    let gas_object = gas_objects.swap_remove(0);
    let data = TransactionData::new_transfer_sui(
        get_key_pair::<AccountKeyPair>().0,
        sender,
        None,
        gas_object.compute_object_reference(),
        rgp * TEST_ONLY_GAS_UNIT_FOR_TRANSFER,
        rgp,
    );
    let tx = to_sender_signed_transaction(data, &keypair);

    // We first let 2 validators stop accepting user cert
    // to make sure QD does not get quorum until reconfig
    for handle in authorities.iter().take(2) {
        handle
            .with_async(|node| async { node.close_epoch_for_testing().await.unwrap() })
            .await;
    }

    // Spawn a task that fire the transaction through TransactionOrchestrator
    // across the epoch boundary.
    fullnode
        .with_async(|node| async {
            let to = node.transaction_orchestrator().unwrap();
            let tx_digest = *tx.digest();
            info!(?tx_digest, "Submitting tx");
            let tx = tx.into_inner();
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
        })
        .await;

    info!("Asking remaining validators to change epoch");
    // Ask the remaining 2 validators to close epoch
    for handle in authorities.iter().skip(2) {
        handle
            .with_async(|node| async { node.close_epoch_for_testing().await.unwrap() })
            .await;
    }

    // Wait for all nodes to reach the next epoch.
    info!("Now waiting for all nodes including fullnode to finish epoch change");
    wait_for_nodes_transition_to_epoch(authorities.iter(), 1).await;
    info!("Validators finished epoch change");
    wait_for_nodes_transition_to_epoch(std::iter::once(&fullnode), 1).await;
    info!("All nodes including fullnode finished");

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
    txn: VerifiedTransaction,
    request_type: ExecuteTransactionRequestType,
) -> Result<ExecuteTransactionResponse, QuorumDriverError> {
    orchestrator
        .execute_transaction_block(ExecuteTransactionRequest {
            transaction: txn.into(),
            request_type,
        })
        .await
}
