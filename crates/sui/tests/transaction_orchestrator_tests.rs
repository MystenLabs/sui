// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use prometheus::Registry;
use sui_core::authority_client::NetworkAuthorityClient;
use sui_core::quorum_driver::QuorumDriver;
use sui_core::transaction_orchestrator::TransactiondOrchestrator;
use sui_json_rpc_types::SuiObjectRead;
use sui_node::SuiNode;
use sui_types::messages::{
    ExecuteTransactionRequest, ExecuteTransactionRequestType, ExecuteTransactionResponse,
    Transaction,
};
use test_utils::messages::{
    // make_counter_create_transaction_with_wallet_context,
    make_counter_increment_transaction_with_wallet_context,
    make_transactions_with_wallet_context,
};
use test_utils::network::setup_network_and_wallet;
use test_utils::transaction::{
    increment_counter, publish_basics_package_and_make_counter, transfer_sui, wait_for_all_txes,
    wait_for_tx,
};

#[tokio::test]
async fn test_local_execution_basic() -> Result<(), anyhow::Error> {
    let (swarm, mut context, _) = setup_network_and_wallet().await?;

    let config = swarm.config().generate_fullnode_config();
    let node = SuiNode::start(&config, Registry::new()).await?;
    let active = node.active();

    // Disable node sync process
    active.cancel_node_sync_process_for_tests().await;

    // FIXME the clone?
    let net = (*active.net()).clone();
    let node_sync_state = active.node_sync_state.clone();
    let orchestrator = TransactiondOrchestrator::new(net, node_sync_state, &Registry::new());

    let mut txns = make_transactions_with_wallet_context(&mut context, 4).await;
    assert!(
        txns.len() >= 4,
        "Expect at least 4 txns. Do we generate enough gas objects during genesis?"
    );

    // Quorum driver does not execute txn locally
    let txn = txns.swap_remove(0);
    let digest = *txn.digest();
    let res = orchestrator
        .quorum_driver()
        .execute_transaction(ExecuteTransactionRequest {
            transaction: txn,
            request_type: ExecuteTransactionRequestType::WaitForEffectsCert,
        })
        .await
        .unwrap_or_else(|e| panic!("Failed to execute transaction {:?}: {:?}", digest, e));
    // Since node sync is turned off, this node does not know about this txn
    assert!(node.state().get_transaction(digest).await.is_err());

    // Transaction Orchestrator proactivcely executes txn locally
    let txn = txns.swap_remove(0);
    let digest = *txn.digest();
    let (res, executed_locally) = orchestrator
        .execute_transaction(ExecuteTransactionRequest {
            transaction: txn,
            request_type: ExecuteTransactionRequestType::WaitForEffectsCert,
        })
        .await
        .unwrap_or_else(|e| panic!("Failed to execute transaction {:?}: {:?}", digest, e));
    assert!(executed_locally.unwrap());
    matches!(res, ExecuteTransactionResponse::EffectsCert(..));
    // This node knows about this txn even though node sync is toggled off.
    assert!(node.state().get_transaction(digest).await.is_ok());

    // Test ImmediateReturn and WaitForTxCert eventually are executed too
    let txn = txns.swap_remove(0);
    let digest1 = *txn.digest();
    orchestrator
        .quorum_driver()
        .execute_transaction(ExecuteTransactionRequest {
            transaction: txn,
            request_type: ExecuteTransactionRequestType::ImmediateReturn,
        })
        .await
        .unwrap_or_else(|e| panic!("Failed to execute transaction {:?}: {:?}", digest, e));

    let txn = txns.swap_remove(0);
    let digest2 = *txn.digest();
    orchestrator
        .quorum_driver()
        .execute_transaction(ExecuteTransactionRequest {
            transaction: txn,
            request_type: ExecuteTransactionRequestType::WaitForTxCert,
        })
        .await
        .unwrap_or_else(|e| panic!("Failed to execute transaction {:?}: {:?}", digest, e));

    wait_for_all_txes(vec![digest1, digest2], node.state().clone()).await;
    assert!(node.state().get_transaction(digest1).await.is_ok());
    assert!(node.state().get_transaction(digest2).await.is_ok());

    Ok(())
}

#[tokio::test]
async fn test_local_execution_with_missing_parents() -> Result<(), anyhow::Error> {
    let (swarm, mut context, _) = setup_network_and_wallet().await?;

    let config = swarm.config().generate_fullnode_config();
    let node = SuiNode::start(&config, Registry::new()).await?;
    let active = node.active();

    // Disable node sync process
    active.cancel_node_sync_process_for_tests().await;

    // FIXME the clone?
    let net = (*active.net()).clone();
    let node_sync_state = active.node_sync_state.clone();
    let orchestrator = TransactiondOrchestrator::new(net, node_sync_state, &Registry::new());

    // Signer is the 2nd address in keystore (index: 1)
    let signer = context.keystore.addresses().get(0).cloned().unwrap();
    let (pkg_ref, counter_id) = publish_basics_package_and_make_counter(&context, signer).await;

    // Construct a dependency graph:
    // tx1 -> tx2 -> tx3 -------> tx5
    //                            /\
    //                            ||
    //               tx4  ---------

    let digest1 = increment_counter(&context, signer, None, pkg_ref, counter_id)
        .await
        .certificate
        .transaction_digest;
    let digest2 = increment_counter(&context, signer, None, pkg_ref, counter_id)
        .await
        .certificate
        .transaction_digest;

    let tx_3 =
        make_counter_increment_transaction_with_wallet_context(&context, signer, counter_id, None)
            .await;
    let digest3 = *tx_3.digest();
    orchestrator
        .quorum_driver()
        .execute_transaction(ExecuteTransactionRequest {
            transaction: tx_3,
            request_type: ExecuteTransactionRequestType::WaitForTxCert,
        })
        .await
        .unwrap_or_else(|e| panic!("Failed to execute transaction {:?}: {:?}", digest3, e));

    // The node does not know about these txns
    tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;
    assert!(node.state().get_transaction(digest1).await.is_err());
    assert!(node.state().get_transaction(digest2).await.is_err());
    assert!(node.state().get_transaction(digest3).await.is_err());
    // assert!(node.state().get_transaction(digest4).await.is_err());
    // assert!(node.state().get_transaction(digest5).await.is_err());

    // let new_gas_ref = match context.get_object_ref(sent_obj_id).await.unwrap() {
    //     SuiObjectRead::Exists(obj) => obj.reference,
    //     other => panic!("Failed to get a new gas for following use: {:?}", other)
    // }.to_object_ref();

    let tx_4 = make_counter_increment_transaction_with_wallet_context(
        // &context, signer, counter_id, Some(new_gas_ref)
        &context, signer, counter_id, None,
    )
    .await;
    let digest4 = *tx_4.digest();
    let (res, executed_locally) = orchestrator
        .execute_transaction(ExecuteTransactionRequest {
            transaction: tx_4,
            request_type: ExecuteTransactionRequestType::WaitForEffectsCert,
        })
        .await
        .unwrap_or_else(|e| panic!("Failed to execute transaction {:?}: {:?}", digest4, e));
    assert!(executed_locally.unwrap());
    matches!(res, ExecuteTransactionResponse::EffectsCert(..));

    assert!(node.state().get_transaction(digest1).await.is_ok());
    assert!(node.state().get_transaction(digest2).await.is_ok());
    assert!(node.state().get_transaction(digest3).await.is_ok());
    // assert!(node.state().get_transaction(digest4).await.is_ok());
    // assert!(node.state().get_transaction(digest5).await.is_ok());
    assert!(node.state().get_transaction(digest4).await.is_ok());

    let digest5 = increment_counter(&context, signer, None, pkg_ref, counter_id)
        .await
        .certificate
        .transaction_digest;
    let digest6 = increment_counter(&context, signer, None, pkg_ref, counter_id)
        .await
        .certificate
        .transaction_digest;

    let tx_7 =
        make_counter_increment_transaction_with_wallet_context(&context, signer, counter_id, None)
            .await;
    let digest7 = *tx_7.digest();
    orchestrator
        .execute_transaction(ExecuteTransactionRequest {
            transaction: tx_7,
            request_type: ExecuteTransactionRequestType::ImmediateReturn,
        })
        .await
        .unwrap_or_else(|e| panic!("Failed to execute transaction {:?}: {:?}", digest7, e));

    wait_for_all_txes(vec![digest5, digest6, digest7], node.state().clone()).await;
    assert!(node.state().get_transaction(digest5).await.is_ok());
    assert!(node.state().get_transaction(digest6).await.is_ok());
    assert!(node.state().get_transaction(digest7).await.is_ok());

    Ok(())
}

// async fn execute_for_tx_cert(
//     quorum_driver: &std::sync::Arc<QuorumDriver<NetworkAuthorityClient>>,
//     txn: Transaction,
// ) {
//     let txn_digest = *txn.digest();
//     quorum_driver
//         .execute_transaction(ExecuteTransactionRequest {
//             transaction: txn,
//             request_type: ExecuteTransactionRequestType::WaitForTxCert,
//         })
//         .await
//         .unwrap_or_else(|e| panic!("Failed to execute transaction {:?}: {:?}", txn_digest, e));
// }
