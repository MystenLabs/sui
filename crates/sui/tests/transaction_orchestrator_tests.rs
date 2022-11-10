// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use prometheus::Registry;
use sui::client_commands::WalletContext;
use sui_core::authority_client::NetworkAuthorityClient;
use sui_core::transaction_orchestrator::TransactiondOrchestrator;
use sui_keys::keystore::AccountKeystore;
use sui_node::SuiNode;
use sui_types::base_types::{ObjectID, ObjectRef, SuiAddress, TransactionDigest};
use sui_types::messages::{
    ExecuteTransactionRequest, ExecuteTransactionRequestType, ExecuteTransactionResponse,
    QuorumDriverRequest, QuorumDriverRequestType, VerifiedTransaction,
};
use test_utils::messages::{
    make_counter_increment_transaction_with_wallet_context, make_transactions_with_wallet_context,
};
use test_utils::network::{start_a_fullnode_with_handle, TestClusterBuilder};
use test_utils::transaction::{
    increment_counter, publish_basics_package_and_make_counter, wait_for_all_txes, wait_for_tx,
};
use tracing::info;

#[tokio::test]
async fn test_blocking_execution() -> Result<(), anyhow::Error> {
    let mut test_cluster = TestClusterBuilder::new().build().await?;
    let context = &mut test_cluster.wallet;
    let node = &test_cluster.fullnode_handle.as_ref().unwrap().sui_node;

    let active = node.active();

    // Disable node sync process
    active.cancel_node_sync_process_for_tests().await;

    let net = active.agg_aggregator();
    let node_sync_handle = active.clone().node_sync_handle();
    let orchestrator =
        TransactiondOrchestrator::new(net, node.state(), node_sync_handle, &Registry::new());

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
        .execute_transaction(QuorumDriverRequest {
            transaction: txn,
            request_type: QuorumDriverRequestType::WaitForEffectsCert,
        })
        .await
        .unwrap_or_else(|e| panic!("Failed to execute transaction {:?}: {:?}", digest, e));
    // Since node sync is turned off, this node does not know about this txn
    assert!(node.state().get_transaction(digest).await.is_err());

    // Transaction Orchestrator proactivcely executes txn locally
    let txn = txns.swap_remove(0);
    let digest = *txn.digest();

    let res = execute_with_orchestrator(
        &orchestrator,
        txn,
        ExecuteTransactionRequestType::WaitForLocalExecution,
    )
    .await;

    if let ExecuteTransactionResponse::EffectsCert(result) = res {
        let (_, _, executed_locally) = *result;
        assert!(executed_locally);
    };

    // This node knows about this txn even though node sync is toggled off.
    assert!(node.state().get_transaction(digest).await.is_ok());

    Ok(())
}

#[tokio::test]
async fn test_non_blocking_execution() -> Result<(), anyhow::Error> {
    let mut test_cluster = TestClusterBuilder::new().build().await?;
    let context = &mut test_cluster.wallet;
    let node = &test_cluster.fullnode_handle.as_ref().unwrap().sui_node;

    let active = node.active();

    // Disable node sync process
    active.cancel_node_sync_process_for_tests().await;

    let net = active.agg_aggregator();
    let node_sync_handle = active.clone().node_sync_handle();
    let orchestrator =
        TransactiondOrchestrator::new(net, node.state(), node_sync_handle, &Registry::new());

    let txn_count = 4;
    let mut txns = make_transactions_with_wallet_context(context, txn_count).await;
    assert!(
        txns.len() >= txn_count,
        "Expect at least {} txns. Do we generate enough gas objects during genesis?",
        txn_count,
    );

    // Test ImmediateReturn and WaitForTxCert eventually are executed too
    let txn = txns.swap_remove(0);
    let digest1 = *txn.digest();

    execute_with_orchestrator(
        &orchestrator,
        txn,
        ExecuteTransactionRequestType::ImmediateReturn,
    )
    .await;

    let txn = txns.swap_remove(0);
    let digest2 = *txn.digest();
    execute_with_orchestrator(
        &orchestrator,
        txn,
        ExecuteTransactionRequestType::WaitForTxCert,
    )
    .await;

    let txn = txns.swap_remove(0);
    let digest3 = *txn.digest();
    execute_with_orchestrator(
        &orchestrator,
        txn,
        ExecuteTransactionRequestType::WaitForEffectsCert,
    )
    .await;

    let digests = vec![digest1, digest2, digest3];
    wait_for_all_txes(digests.clone(), node.state().clone()).await;
    node_knows_txes(node, &digests).await;

    Ok(())
}

#[tokio::test]
async fn test_local_execution_with_missing_parents() -> Result<(), anyhow::Error> {
    telemetry_subscribers::init_for_testing();
    let mut test_cluster = TestClusterBuilder::new().build().await?;
    let context = &mut test_cluster.wallet;

    let fullnode_handle =
        start_a_fullnode_with_handle(&test_cluster.swarm, None, None, false).await?;
    // Note this node is different from the one connected with WalletContext
    let node = &fullnode_handle.sui_node;
    let wallet_context_node = &test_cluster.fullnode_handle.as_ref().unwrap().sui_node;

    let active = node.active();

    // Disable node sync process
    active.cancel_node_sync_process_for_tests().await;

    let net = active.agg_aggregator();
    let node_sync_handle = active.clone().node_sync_handle();
    let orchestrator =
        TransactiondOrchestrator::new(net, node.state(), node_sync_handle, &Registry::new());

    let signer = context.config.keystore.addresses().get(0).cloned().unwrap();
    let (pkg_ref, counter_id) = publish_basics_package_and_make_counter(context, signer).await;
    let counter_shared_at = counter_id.1;

    // 0. Execute transaction through Quorum Driver
    info!("Execute with a Quorum Driver");
    let digests0 = increment(context, &signer, counter_id.0, 20, pkg_ref).await;
    // Since the node sync process is disabled, the node does not know about these txns
    tokio::time::sleep(tokio::time::Duration::from_secs(3)).await;
    node_does_not_know_txes(node, &digests0).await;

    let tx0 = make_counter_increment_transaction_with_wallet_context(
        context,
        signer,
        counter_id.0,
        counter_shared_at,
        None,
    )
    .await;
    let digest0 = *tx0.digest();
    // Then we use this node's Quorum Driver to submit transaction.
    orchestrator
        .quorum_driver()
        .execute_transaction(QuorumDriverRequest {
            transaction: tx0,
            request_type: QuorumDriverRequestType::WaitForTxCert,
        })
        .await
        .unwrap_or_else(|e| panic!("Failed to execute transaction {:?}: {:?}", digest0, e));

    // Even though tx0 is **not** executed from the Orchestrator,
    // it subscribes to the Quorum Driver's effects queue,
    // receives the finalized transactions and executes them.
    // Wait for the async execution to finish
    wait_for_tx(digest0, node.state().clone()).await;
    node_knows_txes(node, &digests0).await;
    node_knows_txes(node, &vec![digest0]).await;

    // 1. Execute with Orchestrator, WaitForLocalExecution
    info!("Execute with Orchestrator, WaitForLocalExecution");

    // We wait until the wallet context node knows about digest0 so it can pick the right gas
    wait_for_tx(digest0, wallet_context_node.state().clone()).await;
    let digests1 = increment(context, &signer, counter_id.0, 20, pkg_ref).await;

    let tx1 = make_counter_increment_transaction_with_wallet_context(
        context,
        signer,
        counter_id.0,
        counter_shared_at,
        None,
    )
    .await;
    let digest1 = *tx1.digest();
    // WaitForLocalExecution synchronuously executes all previous txns
    let res = execute_with_orchestrator(
        &orchestrator,
        tx1,
        ExecuteTransactionRequestType::WaitForLocalExecution,
    )
    .await;
    if let ExecuteTransactionResponse::EffectsCert(result) = res {
        let (_, _, executed_locally) = *result;
        assert!(executed_locally);
    };
    node_knows_txes(node, &digests1).await;
    node_knows_txes(node, &vec![digest1]).await;

    // 2. Execute with Orchestrator, ImmediateReturn
    info!("Execute with Orchestrator, ImmediateReturn");

    // We wait until the wallet context node knows about digest1 so it can pick the right gas
    wait_for_tx(digest1, wallet_context_node.state().clone()).await;
    let digests2 = increment(context, &signer, counter_id.0, 20, pkg_ref).await;
    node_does_not_know_txes(node, &digests2).await;

    let tx2 = make_counter_increment_transaction_with_wallet_context(
        context,
        signer,
        counter_id.0,
        counter_shared_at,
        None,
    )
    .await;
    // ImmediateReturn does not wait for execution results. But the execution asynchronuously triggers
    // all dependencies to be executed as well.
    let digest2 = *tx2.digest();
    execute_with_orchestrator(
        &orchestrator,
        tx2,
        ExecuteTransactionRequestType::ImmediateReturn,
    )
    .await;

    // Wait for the async execution to finish
    wait_for_tx(digest2, node.state().clone()).await;
    node_knows_txes(node, &digests2).await;

    Ok(())
}

async fn increment(
    context: &WalletContext,
    signer: &SuiAddress,
    counter_id: ObjectID,
    delta: usize,
    pkg_ref: ObjectRef,
) -> Vec<TransactionDigest> {
    let mut digests = Vec::with_capacity(delta);
    for _ in 0..delta {
        let digest = increment_counter(context, *signer, None, pkg_ref, counter_id)
            .await
            .0
            .transaction_digest;
        digests.push(digest);
    }
    digests
}

async fn node_knows_txes(node: &SuiNode, digests: &Vec<TransactionDigest>) {
    for digest in digests {
        assert!(node.state().get_transaction(*digest).await.is_ok());
    }
}

async fn node_does_not_know_txes(node: &SuiNode, digests: &Vec<TransactionDigest>) {
    for digest in digests {
        assert!(node.state().get_transaction(*digest).await.is_err());
    }
}

async fn execute_with_orchestrator(
    orchestrator: &TransactiondOrchestrator<NetworkAuthorityClient>,
    txn: VerifiedTransaction,
    request_type: ExecuteTransactionRequestType,
) -> ExecuteTransactionResponse {
    let digest = *txn.digest();
    orchestrator
        .execute_transaction(ExecuteTransactionRequest {
            transaction: txn.into(),
            request_type,
        })
        .await
        .unwrap_or_else(|e| panic!("Failed to execute transaction {:?}: {:?}", digest, e))
}
