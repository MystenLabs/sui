// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::quorum_driver::reconfig_observer::DummyReconfigObserver;
use crate::quorum_driver::{AuthorityAggregator, QuorumDriverHandlerBuilder};
use crate::test_authority_clients::LocalAuthorityClient;
use crate::test_utils::make_transfer_sui_transaction;
use crate::{quorum_driver::QuorumDriverMetrics, test_utils::init_local_authorities};
use mysten_common::sync::notify_read::{NotifyRead, Registration};
use std::sync::Arc;
use std::time::Duration;
use sui_types::base_types::SuiAddress;
use sui_types::base_types::TransactionDigest;
use sui_types::crypto::{deterministic_random_account_key, get_key_pair, AccountKeyPair};
use sui_types::effects::TransactionEffectsAPI;
use sui_types::messages::VerifiedTransaction;
use sui_types::object::{generate_test_gas_objects, Object};
use sui_types::quorum_driver_types::{QuorumDriverError, QuorumDriverResponse, QuorumDriverResult};

async fn setup() -> (
    AuthorityAggregator<LocalAuthorityClient>,
    VerifiedTransaction,
) {
    let (sender, keypair): (_, AccountKeyPair) = get_key_pair();
    let gas_object = Object::with_owner_for_testing(sender);
    let (aggregator, authorities, genesis, _) =
        init_local_authorities(4, vec![gas_object.clone()]).await;
    let rgp = authorities
        .get(0)
        .unwrap()
        .reference_gas_price_for_testing()
        .unwrap();
    let gas_object = genesis
        .objects()
        .iter()
        .find(|o| o.id() == gas_object.id())
        .unwrap();

    let tx = make_tx(gas_object, sender, &keypair, rgp);
    (aggregator, tx)
}

fn make_tx(
    gas: &Object,
    sender: SuiAddress,
    keypair: &AccountKeyPair,
    gas_price: u64,
) -> VerifiedTransaction {
    make_transfer_sui_transaction(
        gas.compute_object_reference(),
        SuiAddress::random_for_testing_only(),
        None,
        sender,
        keypair,
        gas_price,
    )
}

#[tokio::test]
async fn test_quorum_driver_submit_transaction() {
    let (aggregator, tx) = setup().await;
    let digest = *tx.digest();

    let quorum_driver_handler = Arc::new(
        QuorumDriverHandlerBuilder::new(
            Arc::new(aggregator),
            Arc::new(QuorumDriverMetrics::new_for_tests()),
        )
        .with_reconfig_observer(Arc::new(DummyReconfigObserver {}))
        .start(),
    );
    // Test submit_transaction
    let qd_clone = quorum_driver_handler.clone();
    let handle = tokio::task::spawn(async move {
        let (tx, QuorumDriverResponse { effects_cert, .. }) = qd_clone
            .subscribe_to_effects()
            .recv()
            .await
            .unwrap()
            .unwrap();
        assert_eq!(tx.digest(), &digest);
        assert_eq!(*effects_cert.data().transaction_digest(), digest);
    });
    let ticket = quorum_driver_handler.submit_transaction(tx).await.unwrap();
    verify_ticket_response(ticket, &digest).await;

    handle.await.unwrap();
}

#[tokio::test]
async fn test_quorum_driver_submit_transaction_no_ticket() {
    let (aggregator, tx) = setup().await;
    let digest = *tx.digest();

    let quorum_driver_handler = Arc::new(
        QuorumDriverHandlerBuilder::new(
            Arc::new(aggregator),
            Arc::new(QuorumDriverMetrics::new_for_tests()),
        )
        .with_reconfig_observer(Arc::new(DummyReconfigObserver {}))
        .start(),
    );
    let qd_clone = quorum_driver_handler.clone();
    let handle = tokio::task::spawn(async move {
        let (tx, QuorumDriverResponse { effects_cert, .. }) = qd_clone
            .subscribe_to_effects()
            .recv()
            .await
            .unwrap()
            .unwrap();
        assert_eq!(tx.digest(), &digest);
        assert_eq!(*effects_cert.data().transaction_digest(), digest);
    });
    quorum_driver_handler
        .submit_transaction_no_ticket(tx)
        .await
        .unwrap();
    handle.await.unwrap();
}

async fn verify_ticket_response<'a>(
    ticket: Registration<'a, TransactionDigest, QuorumDriverResult>,
    tx_digest: &TransactionDigest,
) {
    let QuorumDriverResponse { effects_cert, .. } = ticket.await.unwrap();
    assert_eq!(effects_cert.data().transaction_digest(), tx_digest);
}

#[tokio::test]
async fn test_quorum_driver_with_given_notify_read() {
    let (aggregator, tx) = setup().await;
    let digest = *tx.digest();
    let notifier = Arc::new(NotifyRead::new());

    let quorum_driver_handler = Arc::new(
        QuorumDriverHandlerBuilder::new(
            Arc::new(aggregator),
            Arc::new(QuorumDriverMetrics::new_for_tests()),
        )
        .with_notifier(notifier.clone())
        .with_reconfig_observer(Arc::new(DummyReconfigObserver {}))
        .start(),
    );

    let qd_clone = quorum_driver_handler.clone();
    let handle = tokio::task::spawn(async move {
        let (tx, QuorumDriverResponse { effects_cert, .. }) = qd_clone
            .subscribe_to_effects()
            .recv()
            .await
            .unwrap()
            .unwrap();
        assert_eq!(tx.digest(), &digest);
        assert_eq!(*effects_cert.data().transaction_digest(), digest);
    });
    let ticket1 = notifier.register_one(&digest);
    let ticket2 = quorum_driver_handler.submit_transaction(tx).await.unwrap();
    verify_ticket_response(ticket1, &digest).await;
    verify_ticket_response(ticket2, &digest).await;

    handle.await.unwrap();
}

// TODO: add other cases for mismatched validator/client epoch
#[tokio::test]
async fn test_quorum_driver_update_validators_and_max_retry_times() {
    telemetry_subscribers::init_for_testing();
    let (mut aggregator, tx) = setup().await;
    let arc_aggregator = Arc::new(aggregator.clone());

    let quorum_driver_handler = Arc::new(
        QuorumDriverHandlerBuilder::new(
            arc_aggregator.clone(),
            Arc::new(QuorumDriverMetrics::new_for_tests()),
        )
        .with_reconfig_observer(Arc::new(DummyReconfigObserver {}))
        .with_max_retry_times(3)
        .start(),
    );

    let quorum_driver = quorum_driver_handler.clone_quorum_driver();
    let quorum_driver_clone = quorum_driver.clone();
    let handle = tokio::task::spawn(async move {
        // Wait till the epoch/committee is updated.
        tokio::time::sleep(Duration::from_secs(3)).await;

        // This now will fail due to server/client epoch mismatch:
        // server's epoch is 0 but client's is 10
        // This error should not happen in practice for benign validators and a working client
        let ticket = quorum_driver.submit_transaction(tx).await.unwrap();
        // We have a timeout here to make the test fail fast if fails
        match tokio::time::timeout(Duration::from_secs(20), ticket).await {
            Ok(Err(QuorumDriverError::FailedWithTransientErrorAfterMaximumAttempts { total_attempts })) => assert_eq!(total_attempts, 4),
            _ => panic!("The transaction should err on SafeClient epoch check mismatch, be retried 3 times and raise QuorumDriverError::FailedWithTransientErrorAfterMaximumAttempts error"),
        };
    });

    // Update authority aggregator with a new epoch number, and let quorum driver know.
    aggregator.committee.epoch = 10;
    quorum_driver_clone
        .update_validators(Arc::new(aggregator))
        .await;
    assert_eq!(
        quorum_driver_handler.clone_quorum_driver().current_epoch(),
        10
    );

    handle.await.unwrap();
}

#[tokio::test]
async fn test_quorum_driver_object_locked() -> Result<(), anyhow::Error> {
    let gas_objects = generate_test_gas_objects();
    let (sender, keypair): (SuiAddress, AccountKeyPair) = deterministic_random_account_key();

    let (aggregator, authorities, genesis, _) =
        init_local_authorities(4, gas_objects.clone()).await;
    let rgp = authorities
        .get(0)
        .unwrap()
        .reference_gas_price_for_testing()
        .unwrap();

    let mut gas_objects = gas_objects
        .into_iter()
        .map(|o| {
            genesis
                .objects()
                .iter()
                .find(|go| go.id() == o.id())
                .unwrap()
                .to_owned()
        })
        .collect::<Vec<_>>();

    let aggregator = Arc::new(aggregator);

    let quorum_driver_handler = Arc::new(
        QuorumDriverHandlerBuilder::new(
            aggregator.clone(),
            Arc::new(QuorumDriverMetrics::new_for_tests()),
        )
        .with_reconfig_observer(Arc::new(DummyReconfigObserver {}))
        .start(),
    );

    let quorum_driver = quorum_driver_handler.clone_quorum_driver();

    let gas = gas_objects.pop().unwrap();
    let tx = make_tx(&gas, sender, &keypair, rgp);
    let names: Vec<_> = aggregator.authority_clients.keys().clone().collect();
    assert_eq!(names.len(), 4);
    let client0 = aggregator.clone_client(names[0]);
    let client1 = aggregator.clone_client(names[1]);
    let client2 = aggregator.clone_client(names[2]);

    println!("Case 0 - two validators lock the object with the same tx");
    assert!(client0.handle_transaction(tx.clone()).await.is_ok());
    assert!(client1.handle_transaction(tx.clone()).await.is_ok());

    let tx2 = make_tx(&gas, sender, &keypair, rgp);
    let res = quorum_driver.submit_transaction(tx2).await.unwrap().await;

    // Aggregator waits for all responses when it sees a conflicting tx and because
    // there are not enough retryable errors to push the original tx or the most staked
    // conflicting tx >= 2f+1 stake. Neither transaction can be retried due to client
    // double spend and this is a fatal error.
    if let Err(QuorumDriverError::ObjectsDoubleUsed {
        conflicting_txes,
        retried_tx,
        retried_tx_success,
    }) = res
    {
        assert_eq!(retried_tx, None);
        assert_eq!(retried_tx_success, None);
        assert_eq!(conflicting_txes.len(), 1);
        assert_eq!(conflicting_txes.iter().next().unwrap().0, tx.digest());
    } else {
        panic!(
            "expect Err(QuorumDriverError::ObjectsDoubleUsed) but got {:?}",
            res
        );
    }

    println!("Case 1 - three validators lock the object with the same tx");
    let gas = gas_objects.pop().unwrap();
    let tx = make_tx(&gas, sender, &keypair, rgp);

    assert!(client0.handle_transaction(tx.clone()).await.is_ok());
    assert!(client1.handle_transaction(tx.clone()).await.is_ok());
    assert!(client2.handle_transaction(tx.clone()).await.is_ok());

    let tx2 = make_tx(&gas, sender, &keypair, rgp);

    let res = quorum_driver.submit_transaction(tx2).await.unwrap().await;
    // Aggregator gets three bad responses, and tries tx, which should succeed.
    if let Err(QuorumDriverError::ObjectsDoubleUsed {
        conflicting_txes,
        retried_tx,
        retried_tx_success,
    }) = res
    {
        assert_eq!(retried_tx, Some(*tx.digest()));
        assert_eq!(retried_tx_success, Some(true));
        assert_eq!(conflicting_txes.len(), 1);
        assert_eq!(conflicting_txes.iter().next().unwrap().0, tx.digest());
    } else {
        panic!(
            "expect Err(QuorumDriverError::ObjectsDoubleUsed) but got {:?}",
            res
        )
    }

    println!("Case 2 - one validator locks the object");
    let gas = gas_objects.pop().unwrap();
    let tx = make_tx(&gas, sender, &keypair, rgp);
    assert!(client0.handle_transaction(tx.clone()).await.is_ok());

    let tx2 = make_tx(&gas, sender, &keypair, rgp);
    let tx2_digest = *tx2.digest();

    let res = quorum_driver
        .submit_transaction(tx2)
        .await
        .unwrap()
        .await
        .unwrap();

    // Aggregator gets three good responses and execution succeeds.
    let QuorumDriverResponse { effects_cert, .. } = res;
    assert_eq!(*effects_cert.transaction_digest(), tx2_digest);

    println!("Case 3 - object is locked by 2 txes with weight 2 and 1 respectivefully. Then try to execute the third txn");
    let gas = gas_objects.pop().unwrap();
    let tx = make_tx(&gas, sender, &keypair, rgp);
    let tx2 = make_tx(&gas, sender, &keypair, rgp);

    assert!(client0.handle_transaction(tx.clone()).await.is_ok());
    assert!(client1.handle_transaction(tx.clone()).await.is_ok());
    assert!(client2.handle_transaction(tx2.clone()).await.is_ok());

    let tx3 = make_tx(&gas, sender, &keypair, rgp);

    let res = quorum_driver.submit_transaction(tx3).await.unwrap().await;

    if let Err(QuorumDriverError::ObjectsDoubleUsed {
        conflicting_txes,
        retried_tx,
        retried_tx_success,
    }) = res
    {
        assert_eq!(retried_tx, None);
        assert_eq!(retried_tx_success, None);
        assert_eq!(conflicting_txes.len(), 2);
        assert_eq!(conflicting_txes.get(tx.digest()).unwrap().1, 5000);
        assert_eq!(conflicting_txes.get(tx2.digest()).unwrap().1, 2500);
    } else {
        panic!(
            "expect Err(QuorumDriverError::ObjectsDoubleUsed) but got {:?}",
            res
        )
    }

    println!("Case 4 - object is locked by 2 txes with weight 2 and 1, try to execute the lighter stake tx");
    let gas = gas_objects.pop().unwrap();
    let tx = make_tx(&gas, sender, &keypair, rgp);
    let tx2 = make_tx(&gas, sender, &keypair, rgp);
    assert!(client0.handle_transaction(tx.clone()).await.is_ok());
    assert!(client1.handle_transaction(tx.clone()).await.is_ok());
    assert!(client2.handle_transaction(tx2.clone()).await.is_ok());
    let res = quorum_driver.submit_transaction(tx2).await.unwrap().await;

    if let Err(QuorumDriverError::ObjectsDoubleUsed {
        conflicting_txes,
        retried_tx,
        retried_tx_success,
    }) = res
    {
        assert_eq!(retried_tx, None);
        assert_eq!(retried_tx_success, None);
        assert_eq!(conflicting_txes.len(), 1);
        assert_eq!(conflicting_txes.get(tx.digest()).unwrap().1, 5000);
    } else {
        panic!(
            "expect Err(QuorumDriverError::ObjectsDoubleUsed) but got {:?}",
            res
        )
    }

    println!("Case 5 - object is locked by 2 txes with weight 2 and 1, try to execute the heavier stake tx");
    let gas = gas_objects.pop().unwrap();
    let tx = make_tx(&gas, sender, &keypair, rgp);
    let tx_digest = *tx.digest();
    let tx2 = make_tx(&gas, sender, &keypair, rgp);

    assert!(client0.handle_transaction(tx.clone()).await.is_ok());
    assert!(client1.handle_transaction(tx.clone()).await.is_ok());
    assert!(client2.handle_transaction(tx2).await.is_ok());

    let res = quorum_driver
        .submit_transaction(tx)
        .await
        .unwrap()
        .await
        .unwrap();

    let QuorumDriverResponse { effects_cert, .. } = res;
    assert_eq!(*effects_cert.transaction_digest(), tx_digest);

    println!("Case 6 - three validators lock the object, by different txes");
    let gas = gas_objects.pop().unwrap();
    let tx = make_tx(&gas, sender, &keypair, rgp);
    let tx2 = make_tx(&gas, sender, &keypair, rgp);
    let tx3 = make_tx(&gas, sender, &keypair, rgp);
    assert!(client0.handle_transaction(tx.clone()).await.is_ok());
    assert!(client1.handle_transaction(tx2.clone()).await.is_ok());
    assert!(client2.handle_transaction(tx3.clone()).await.is_ok());

    let tx4 = make_tx(&gas, sender, &keypair, rgp);
    let res = quorum_driver
        .submit_transaction(tx4.clone())
        .await
        .unwrap()
        .await;

    if let Err(QuorumDriverError::ObjectsDoubleUsed {
        conflicting_txes,
        retried_tx,
        retried_tx_success,
    }) = res
    {
        assert_eq!(retried_tx, None);
        assert_eq!(retried_tx_success, None);
        assert_eq!(conflicting_txes.len(), 3);
        assert_eq!(conflicting_txes.get(tx.digest()).unwrap().1, 2500);
        assert_eq!(conflicting_txes.get(tx2.digest()).unwrap().1, 2500);
        assert_eq!(conflicting_txes.get(tx3.digest()).unwrap().1, 2500);
    } else {
        panic!(
            "expect Err(QuorumDriverError::ObjectsDoubleUsed) but got {:?}",
            res
        )
    }

    Ok(())
}
