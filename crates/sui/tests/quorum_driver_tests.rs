// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;
use std::time::Duration;
use sui_core::authority_aggregator::{AuthorityAggregator, AuthorityAggregatorBuilder};
use sui_core::authority_client::NetworkAuthorityClient;
use sui_core::quorum_driver::{QuorumDriverHandler, QuorumDriverMetrics};
use sui_node::SuiNodeHandle;
use sui_types::base_types::SuiAddress;
use sui_types::messages::{
    QuorumDriverRequest, QuorumDriverRequestType, QuorumDriverResponse, VerifiedTransaction,
};
use test_utils::authority::{spawn_test_authorities, test_authority_configs};
use test_utils::messages::make_transfer_sui_transaction;
use test_utils::objects::test_gas_objects;
use test_utils::test_account_keys;

async fn setup() -> (
    Vec<SuiNodeHandle>,
    AuthorityAggregator<NetworkAuthorityClient>,
    VerifiedTransaction,
) {
    let mut gas_objects = test_gas_objects();
    let configs = test_authority_configs();
    let handles = spawn_test_authorities(gas_objects.clone(), &configs).await;
    let committee_store = handles[0].with(|h| h.state().committee_store().clone());
    let (aggregator, _) = AuthorityAggregatorBuilder::from_network_config(&configs)
        .with_committee_store(committee_store)
        .build()
        .unwrap();
    let (sender, keypair) = test_account_keys().pop().unwrap();
    let tx = make_transfer_sui_transaction(
        gas_objects.pop().unwrap().compute_object_reference(),
        SuiAddress::default(),
        None,
        sender,
        &keypair,
    );
    (handles, aggregator, tx)
}

#[tokio::test]
async fn test_execute_transaction_immediate() {
    let (_handles, aggregator, tx) = setup().await;
    let digest = *tx.digest();

    let quorum_driver_handler =
        QuorumDriverHandler::new(Arc::new(aggregator), QuorumDriverMetrics::new_for_tests());
    let quorum_driver = quorum_driver_handler.clone_quorum_driver();
    let handle = tokio::task::spawn(async move {
        let (cert, effects) = quorum_driver_handler.subscribe().recv().await.unwrap();
        assert_eq!(*cert.digest(), digest);
        assert_eq!(effects.effects.transaction_digest, digest);
    });
    assert!(matches!(
        quorum_driver
            .execute_transaction(QuorumDriverRequest {
                transaction: tx,
                request_type: QuorumDriverRequestType::ImmediateReturn,
            })
            .await
            .unwrap(),
        QuorumDriverResponse::ImmediateReturn
    ));

    handle.await.unwrap();
}

#[tokio::test]
async fn test_execute_transaction_wait_for_cert() {
    let (_handles, aggregator, tx) = setup().await;
    let digest = *tx.digest();

    let quorum_driver_handler =
        QuorumDriverHandler::new(Arc::new(aggregator), QuorumDriverMetrics::new_for_tests());
    let quorum_driver = quorum_driver_handler.clone_quorum_driver();
    let handle = tokio::task::spawn(async move {
        let (cert, effects) = quorum_driver_handler.subscribe().recv().await.unwrap();
        assert_eq!(*cert.digest(), digest);
        assert_eq!(effects.effects.transaction_digest, digest);
    });
    if let QuorumDriverResponse::TxCert(cert) = quorum_driver
        .execute_transaction(QuorumDriverRequest {
            transaction: tx,
            request_type: QuorumDriverRequestType::WaitForTxCert,
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
    let (_handles, aggregator, tx) = setup().await;
    let digest = *tx.digest();

    let quorum_driver_handler =
        QuorumDriverHandler::new(Arc::new(aggregator), QuorumDriverMetrics::new_for_tests());
    let quorum_driver = quorum_driver_handler.clone_quorum_driver();
    let handle = tokio::task::spawn(async move {
        let (cert, effects) = quorum_driver_handler.subscribe().recv().await.unwrap();
        assert_eq!(*cert.digest(), digest);
        assert_eq!(effects.effects.transaction_digest, digest);
    });
    if let QuorumDriverResponse::EffectsCert(result) = quorum_driver
        .execute_transaction(QuorumDriverRequest {
            transaction: tx,
            request_type: QuorumDriverRequestType::WaitForEffectsCert,
        })
        .await
        .unwrap()
    {
        let (cert, effects) = *result;
        assert_eq!(*cert.digest(), digest);
        assert_eq!(effects.effects.transaction_digest, digest);
    } else {
        unreachable!();
    }

    handle.await.unwrap();
}

#[tokio::test]
async fn test_update_validators() {
    let (_handles, mut aggregator, tx) = setup().await;
    let arc_aggregator = Arc::new(aggregator.clone());
    let quorum_driver_handler =
        QuorumDriverHandler::new(arc_aggregator.clone(), QuorumDriverMetrics::new_for_tests());
    let quorum_driver = quorum_driver_handler.clone_quorum_driver();
    let quorum_driver_clone = quorum_driver.clone();
    let handle = tokio::task::spawn(async move {
        // Wait till the epoch/committee is updated.
        tokio::time::sleep(Duration::from_secs(3)).await;

        let result = quorum_driver
            .execute_transaction(QuorumDriverRequest {
                transaction: tx,
                request_type: QuorumDriverRequestType::WaitForEffectsCert,
            })
            .await;
        // This now will fail due to epoch mismatch.
        assert!(result.is_err());
    });

    // Update authority aggregator with a new epoch number, and let quorum driver know.
    aggregator.committee.epoch = 10;
    quorum_driver_clone
        .update_validators(Arc::new(aggregator))
        .await
        .unwrap();
    assert_eq!(
        quorum_driver_handler.clone_quorum_driver().current_epoch(),
        10
    );

    handle.await.unwrap();
}
