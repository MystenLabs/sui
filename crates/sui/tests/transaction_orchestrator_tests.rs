// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use prometheus::Registry;
use sui_core::authority_client::NetworkAuthorityClient;
use sui_core::transaction_orchestrator::TransactiondOrchestrator;
use sui_node::SuiNode;
use sui_types::base_types::TransactionDigest;
use sui_types::messages::{
    ExecuteTransactionRequest, ExecuteTransactionRequestType, ExecuteTransactionResponse,
    QuorumDriverRequest, QuorumDriverRequestType, VerifiedTransaction,
};
use test_utils::messages::make_transactions_with_wallet_context;
use test_utils::network::TestClusterBuilder;
use test_utils::transaction::{wait_for_all_txes, wait_for_tx};

#[tokio::test]
async fn test_blocking_execution() -> Result<(), anyhow::Error> {
    let mut test_cluster = TestClusterBuilder::new().build().await?;
    let context = &mut test_cluster.wallet;
    let node = &test_cluster.fullnode_handle.sui_node;

    let active = node.active();

    // Disable node sync process
    active.cancel_node_sync_process_for_tests().await;

    let net = active.agg_aggregator();
    let orchestrator = TransactiondOrchestrator::new(net, node.state(), &Registry::new());

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
    .await;

    if let ExecuteTransactionResponse::EffectsCert(result) = res {
        let (_, _, executed_locally) = *result;
        assert!(executed_locally);
    };

    assert!(node.state().get_transaction(digest).await.is_ok());

    Ok(())
}

#[tokio::test]
async fn test_non_blocking_execution() -> Result<(), anyhow::Error> {
    let mut test_cluster = TestClusterBuilder::new().build().await?;
    let context = &mut test_cluster.wallet;
    let node = &test_cluster.fullnode_handle.sui_node;

    let active = node.active();

    // Disable node sync process
    active.cancel_node_sync_process_for_tests().await;

    let net = active.agg_aggregator();
    let orchestrator = TransactiondOrchestrator::new(net, node.state(), &Registry::new());

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

async fn node_knows_txes(node: &SuiNode, digests: &Vec<TransactionDigest>) {
    for digest in digests {
        assert!(node.state().get_transaction(*digest).await.is_ok());
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
