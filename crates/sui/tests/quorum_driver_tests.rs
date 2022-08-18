// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::time::Duration;
use sui_core::authority_aggregator::AuthorityAggregator;
use sui_core::authority_client::NetworkAuthorityClient;
use sui_node::SuiNode;
use sui_quorum_driver::{QuorumDriverHandler, QuorumDriverMetrics};
use sui_types::base_types::SuiAddress;
use sui_types::messages::{
    ExecuteTransactionRequest, ExecuteTransactionRequestType, ExecuteTransactionResponse,
    Transaction,
};
use test_utils::authority::{
    spawn_test_authorities, test_authority_aggregator, test_authority_configs,
};
use test_utils::messages::make_transfer_sui_transaction;
use test_utils::objects::test_gas_objects;
use test_utils::test_account_keys;

async fn setup() -> (
    Vec<SuiNode>,
    AuthorityAggregator<NetworkAuthorityClient>,
    Transaction,
) {
    let mut gas_objects = test_gas_objects();
    let configs = test_authority_configs();
    let handles = spawn_test_authorities(gas_objects.clone(), &configs).await;
    let clients = test_authority_aggregator(&configs, handles[0].state().epoch_store().clone());
    let (sender, keypair) = test_account_keys().pop().unwrap();
    let tx = make_transfer_sui_transaction(
        gas_objects.pop().unwrap().compute_object_reference(),
        SuiAddress::default(),
        None,
        sender,
        &keypair,
    );
    (handles, clients, tx)
}

#[tokio::test]
async fn test_execute_transaction_immediate() {
    let (_handles, clients, tx) = setup().await;
    let digest = *tx.digest();

    let quorum_driver_handler =
        QuorumDriverHandler::new(clients, QuorumDriverMetrics::new_for_tests());
    let quorum_driver = quorum_driver_handler.clone_quorum_driver();
    let handle = tokio::task::spawn(async move {
        let (cert, effects) = quorum_driver_handler.subscribe().recv().await.unwrap();
        assert_eq!(*cert.digest(), digest);
        assert_eq!(effects.effects().transaction_digest, digest);
    });
    assert!(matches!(
        quorum_driver
            .execute_transaction(ExecuteTransactionRequest {
                transaction: tx,
                request_type: ExecuteTransactionRequestType::ImmediateReturn,
            })
            .await
            .unwrap(),
        ExecuteTransactionResponse::ImmediateReturn
    ));

    handle.await.unwrap();
}

#[tokio::test]
async fn test_execute_transaction_wait_for_cert() {
    let (_handles, clients, tx) = setup().await;
    let digest = *tx.digest();

    let quorum_driver_handler =
        QuorumDriverHandler::new(clients, QuorumDriverMetrics::new_for_tests());
    let quorum_driver = quorum_driver_handler.clone_quorum_driver();
    let handle = tokio::task::spawn(async move {
        let (cert, effects) = quorum_driver_handler.subscribe().recv().await.unwrap();
        assert_eq!(*cert.digest(), digest);
        assert_eq!(effects.effects().transaction_digest, digest);
    });
    if let ExecuteTransactionResponse::TxCert(cert) = quorum_driver
        .execute_transaction(ExecuteTransactionRequest {
            transaction: tx,
            request_type: ExecuteTransactionRequestType::WaitForTxCert,
        })
        .await
        .unwrap()
    {
        assert_eq!(*cert.digest(), digest);
    } else {
        unreachable!();
    }

    handle.await.unwrap();
}

#[tokio::test]
async fn test_execute_transaction_wait_for_effects() {
    let (_handles, clients, tx) = setup().await;
    let digest = *tx.digest();

    let quorum_driver_handler =
        QuorumDriverHandler::new(clients, QuorumDriverMetrics::new_for_tests());
    let quorum_driver = quorum_driver_handler.clone_quorum_driver();
    let handle = tokio::task::spawn(async move {
        let (cert, effects) = quorum_driver_handler.subscribe().recv().await.unwrap();
        assert_eq!(*cert.digest(), digest);
        assert_eq!(effects.effects().transaction_digest, digest);
    });
    if let ExecuteTransactionResponse::EffectsCert(result) = quorum_driver
        .execute_transaction(ExecuteTransactionRequest {
            transaction: tx,
            request_type: ExecuteTransactionRequestType::WaitForEffectsCert,
        })
        .await
        .unwrap()
    {
        let (cert, effects) = *result;
        assert_eq!(*cert.digest(), digest);
        assert_eq!(effects.effects().transaction_digest, digest);
    } else {
        unreachable!();
    }

    handle.await.unwrap();
}

#[tokio::test]
async fn test_update_validators() {
    let (_handles, mut clients, tx) = setup().await;
    let quorum_driver_handler =
        QuorumDriverHandler::new(clients.clone(), QuorumDriverMetrics::new_for_tests());
    let quorum_driver = quorum_driver_handler.clone_quorum_driver();
    let handle = tokio::task::spawn(async move {
        // Wait till the epoch/committee is updated.
        tokio::time::sleep(Duration::from_secs(3)).await;

        let result = quorum_driver
            .execute_transaction(ExecuteTransactionRequest {
                transaction: tx,
                request_type: ExecuteTransactionRequestType::WaitForEffectsCert,
            })
            .await;
        // This now will fail due to epoch mismatch.
        assert!(result.is_err());
    });

    // Create a new authority aggregator with a new epoch number, and update the quorum driver.
    clients.committee.epoch = 10;
    quorum_driver_handler
        .update_validators(clients)
        .await
        .unwrap();

    handle.await.unwrap();
}
