// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;
use std::time::Duration;
use sui_core::authority_aggregator::{AuthorityAggregator, AuthorityAggregatorBuilder};
use sui_core::authority_client::NetworkAuthorityClient;
use sui_core::quorum_driver::{QuorumDriverHandler, QuorumDriverMetrics};
use sui_node::SuiNodeHandle;
use sui_types::base_types::SuiAddress;
use sui_types::crypto::AccountKeyPair;
use sui_types::error::SuiError;
use sui_types::messages::{
    QuorumDriverRequest, QuorumDriverRequestType, QuorumDriverResponse, VerifiedTransaction,
};
use sui_types::object::Object;
use test_utils::authority::{
    spawn_test_authorities, test_and_configure_authority_configs, test_authority_configs,
};
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
        assert_eq!(effects.data().transaction_digest, digest);
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
        assert_eq!(effects.data().transaction_digest, digest);
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
        assert_eq!(effects.data().transaction_digest, digest);
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
        assert_eq!(effects.data().transaction_digest, digest);
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

#[tokio::test]
async fn test_retry_on_object_locked() -> Result<(), anyhow::Error> {
    let mut gas_objects = test_gas_objects();
    let configs = test_and_configure_authority_configs(4);
    let handles = spawn_test_authorities(gas_objects.clone(), &configs).await;
    let committee_store = handles[0].with(|h| h.state().committee_store().clone());
    let (aggregator, _) = AuthorityAggregatorBuilder::from_network_config(&configs)
        .with_committee_store(committee_store)
        .build()
        .unwrap();
    let aggregator = Arc::new(aggregator);
    let quorum_driver_handler =
        QuorumDriverHandler::new(aggregator.clone(), QuorumDriverMetrics::new_for_tests());
    let quorum_driver = quorum_driver_handler.clone_quorum_driver();

    let (sender, keypair) = test_account_keys().pop().unwrap();
    let gas = gas_objects.pop().unwrap();
    let tx = make_tx(&gas, sender, &keypair);
    let names: Vec<_> = aggregator.authority_clients.keys().clone().collect();
    assert_eq!(names.len(), 4);
    let client0 = aggregator.clone_client(names[0]);
    let client1 = aggregator.clone_client(names[1]);
    let client2 = aggregator.clone_client(names[2]);

    // Case 1 - two validators lock the object with the same tx
    assert!(client0.handle_transaction(tx.clone()).await.is_ok());
    assert!(client1.handle_transaction(tx.clone()).await.is_ok());

    let tx2 = make_tx(&gas, sender, &keypair);
    let res = quorum_driver
        .execute_transaction(QuorumDriverRequest {
            transaction: tx2,
            request_type: QuorumDriverRequestType::WaitForEffectsCert,
        })
        .await;
    match res {
        // If aggregator gets two bad responses from 0 and 1 before getting two good responses from 2 and 3,
        // it will retry tx, but it will fail due to equivocaiton.
        Err(SuiError::QuorumFailedToProcessTransactionWithConflictingTransactionRetried {conflicting_tx_digest, conflicting_tx_retry_success}) => {
            assert_eq!(conflicting_tx_digest, *tx.digest());
            assert!(!conflicting_tx_retry_success);
        },
        // If aggregator gets two good responses from client 2 and 3 before two bad responses from 0 and 1,
        // tx will not be retried.
        Err(SuiError::QuorumFailedToProcessTransaction {..}) => (),
        _ => panic!("expect Err(SuiError::QuorumFailedToProcessTransactionWithConflictingTransactionRetried) or QuorumFailedToProcessTransaction but got {:?}", res),
    }

    // Case 2 - three validators lock the object with the same tx
    let gas = gas_objects.pop().unwrap();
    let tx = make_tx(&gas, sender, &keypair);

    assert!(client0.handle_transaction(tx.clone()).await.is_ok());
    assert!(client1.handle_transaction(tx.clone()).await.is_ok());
    assert!(client2.handle_transaction(tx.clone()).await.is_ok());

    let tx2 = make_tx(&gas, sender, &keypair);

    let res = quorum_driver
        .execute_transaction(QuorumDriverRequest {
            transaction: tx2,
            request_type: QuorumDriverRequestType::WaitForEffectsCert,
        })
        .await;
    // Aggregator gets three bad responses, and tries tx, which should succeed.
    if let Err(SuiError::QuorumFailedToProcessTransactionWithConflictingTransactionRetried {
        conflicting_tx_digest,
        conflicting_tx_retry_success,
    }) = res
    {
        assert_eq!(conflicting_tx_digest, *tx.digest());
        assert!(conflicting_tx_retry_success);
    } else {
        panic!("expect Err(SuiError::QuorumFailedToProcessTransactionWithConflictingTransactionRetried) but got {:?}", res)
    }

    // Case 3 - one validator locks the object
    let gas = gas_objects.pop().unwrap();
    let tx = make_tx(&gas, sender, &keypair);
    assert!(client0.handle_transaction(tx.clone()).await.is_ok());

    let tx2 = make_tx(&gas, sender, &keypair);

    let res = quorum_driver
        .execute_transaction(QuorumDriverRequest {
            transaction: tx2,
            request_type: QuorumDriverRequestType::WaitForEffectsCert,
        })
        .await;
    // Aggregator gets three good responses and execution succeeds.
    assert!(res.is_ok());

    // Case 4 - object is locked by 2 txes with weight 2 and 1 respectivefully. Then try to execute the third txn
    let gas = gas_objects.pop().unwrap();
    let tx = make_tx(&gas, sender, &keypair);
    let tx2 = make_tx(&gas, sender, &keypair);

    assert!(client0.handle_transaction(tx.clone()).await.is_ok());
    assert!(client1.handle_transaction(tx.clone()).await.is_ok());
    assert!(client2.handle_transaction(tx2.clone()).await.is_ok());

    let tx3 = make_tx(&gas, sender, &keypair);

    let res = quorum_driver
        .execute_transaction(QuorumDriverRequest {
            transaction: tx3,
            request_type: QuorumDriverRequestType::WaitForEffectsCert,
        })
        .await;
    match res {
        // If aggregator gets two bad responses from 0 and 1, it will retry tx, but it will fail due to equivocaiton.
        Err(SuiError::QuorumFailedToProcessTransactionWithConflictingTransactionRetried {conflicting_tx_digest, conflicting_tx_retry_success}) => {
            assert_eq!(conflicting_tx_digest, *tx.digest());
            assert!(!conflicting_tx_retry_success);
        },
        // If aggregator gets two bad responses of which one from 2, then no tx will be retried.
        Err(SuiError::QuorumFailedToProcessTransaction {..}) => (),
        _ => panic!("expect Err(SuiError::QuorumFailedToProcessTransactionWithConflictingTransactionRetried) or QuorumFailedToProcessTransaction but got {:?}", res),
    }

    // Case 5 - object is locked by 2 txes with weight 2 and 1, try to execute the lighter stake tx
    let gas = gas_objects.pop().unwrap();
    let tx = make_tx(&gas, sender, &keypair);
    let tx2 = make_tx(&gas, sender, &keypair);
    assert!(client0.handle_transaction(tx.clone()).await.is_ok());
    assert!(client1.handle_transaction(tx.clone()).await.is_ok());
    assert!(client2.handle_transaction(tx2.clone()).await.is_ok());
    println!("tx2: {:?}", tx2.digest());
    let res = quorum_driver
        .execute_transaction(QuorumDriverRequest {
            transaction: tx2,
            request_type: QuorumDriverRequestType::WaitForEffectsCert,
        })
        .await;
    match res {
        // if aggregator gets two bad responses from 0 and 1 first, it will try to retry tx (stake = 2), but that will fail
        Err(SuiError::QuorumFailedToProcessTransactionWithConflictingTransactionRetried {conflicting_tx_digest, conflicting_tx_retry_success}) => {
            assert_eq!(conflicting_tx_digest, *tx.digest());
            assert!(!conflicting_tx_retry_success);
        },
        // If aggregator gets two bad responses of which one from 2, then no tx will be retried.
        Err(SuiError::QuorumFailedToProcessTransaction {..}) => (),
        _ => panic!("expect Err(SuiError::QuorumFailedToProcessTransactionWithConflictingTransactionRetried) or QuorumFailedToProcessTransaction but got {:?}", res),
    }

    // Case 6 - object is locked by 2 txes with weight 2 and 1, try to execute the heavier stake tx
    let gas = gas_objects.pop().unwrap();
    let tx = make_tx(&gas, sender, &keypair);
    let tx2 = make_tx(&gas, sender, &keypair);

    assert!(client0.handle_transaction(tx.clone()).await.is_ok());
    assert!(client1.handle_transaction(tx.clone()).await.is_ok());
    assert!(client2.handle_transaction(tx2).await.is_ok());

    let res = quorum_driver
        .execute_transaction(QuorumDriverRequest {
            transaction: tx,
            request_type: QuorumDriverRequestType::WaitForEffectsCert,
        })
        .await;
    assert!(res.is_ok());

    // Case 7 - three validators lock the object, by different txes
    let gas = gas_objects.pop().unwrap();
    let tx = make_tx(&gas, sender, &keypair);
    let tx2 = make_tx(&gas, sender, &keypair);
    let tx3 = make_tx(&gas, sender, &keypair);
    assert!(client0.handle_transaction(tx).await.is_ok());
    assert!(client1.handle_transaction(tx2).await.is_ok());
    assert!(client2.handle_transaction(tx3).await.is_ok());

    let tx4 = make_tx(&gas, sender, &keypair);
    let res = quorum_driver
        .execute_transaction(QuorumDriverRequest {
            transaction: tx4,
            request_type: QuorumDriverRequestType::WaitForEffectsCert,
        })
        .await;
    if !matches!(res, Err(SuiError::QuorumFailedToProcessTransaction { .. })) {
        panic!(
            "expect Err(SuiError::QuorumFailedToProcessTransaction) but got {:?}",
            res
        )
    }

    Ok(())
}

fn make_tx(gas: &Object, sender: SuiAddress, keypair: &AccountKeyPair) -> VerifiedTransaction {
    make_transfer_sui_transaction(
        gas.compute_object_reference(),
        SuiAddress::random_for_testing_only(),
        None,
        sender,
        keypair,
    )
}
