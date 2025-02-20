// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::quorum_driver::reconfig_observer::DummyReconfigObserver;
use crate::quorum_driver::{
    AuthorityAggregator, AuthorityAggregatorUpdatable as _, QuorumDriverHandlerBuilder,
};
use crate::test_authority_clients::LocalAuthorityClient;
use crate::test_authority_clients::LocalAuthorityClientFaultConfig;
use crate::test_utils::make_transfer_sui_transaction;
use crate::{quorum_driver::QuorumDriverMetrics, unit_test_utils::init_local_authorities};
use mysten_common::sync::notify_read::{NotifyRead, Registration};
use std::net::SocketAddr;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;
use sui_macros::{register_fail_point, sim_test};
use sui_types::base_types::SuiAddress;
use sui_types::base_types::TransactionDigest;
use sui_types::crypto::{deterministic_random_account_key, get_key_pair, AccountKeyPair};
use sui_types::effects::TransactionEffectsAPI;
use sui_types::object::{generate_test_gas_objects, Object};
use sui_types::quorum_driver_types::{
    ExecuteTransactionRequestV3, QuorumDriverError, QuorumDriverResponse, QuorumDriverResult,
};
use sui_types::transaction::Transaction;
use tokio::time::timeout;

async fn setup() -> (AuthorityAggregator<LocalAuthorityClient>, Transaction) {
    let (sender, keypair): (_, AccountKeyPair) = get_key_pair();
    let gas_object = Object::with_owner_for_testing(sender);
    let (aggregator, authorities, genesis, _) =
        init_local_authorities(4, vec![gas_object.clone()]).await;
    let rgp = authorities
        .first()
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
) -> Transaction {
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
    let ticket = quorum_driver_handler
        .submit_transaction(ExecuteTransactionRequestV3::new_v2(tx))
        .await
        .unwrap();
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
        .submit_transaction_no_ticket(
            ExecuteTransactionRequestV3::new_v2(tx),
            Some(SocketAddr::new([127, 0, 0, 1].into(), 0)),
        )
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
    let ticket2 = quorum_driver_handler
        .submit_transaction(ExecuteTransactionRequestV3::new_v2(tx))
        .await
        .unwrap();
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
        let ticket = quorum_driver
            .submit_transaction(ExecuteTransactionRequestV3::new_v2(tx))
            .await
            .unwrap();
        // We have a timeout here to make the test fail fast if fails
        match tokio::time::timeout(Duration::from_secs(20), ticket).await {
            Ok(Err(QuorumDriverError::FailedWithTransientErrorAfterMaximumAttempts { total_attempts })) => assert_eq!(total_attempts, 4),
            _ => panic!("The transaction should err on SafeClient epoch check mismatch, be retried 3 times and raise QuorumDriverError::FailedWithTransientErrorAfterMaximumAttempts error"),
        };
    });

    // Update authority aggregator with a new epoch number, and let quorum driver know.
    let mut committee = aggregator.clone_inner_committee_test_only();
    committee.epoch = 10;
    aggregator.committee = Arc::new(committee);
    quorum_driver_clone.update_authority_aggregator(Arc::new(aggregator));
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
    let client_ip = SocketAddr::new([127, 0, 0, 1].into(), 0);

    let (aggregator, authorities, genesis, _) =
        init_local_authorities(4, gas_objects.clone()).await;
    let rgp = authorities
        .first()
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
    let client0 = aggregator.clone_client_test_only(names[0]);
    let client1 = aggregator.clone_client_test_only(names[1]);
    let client2 = aggregator.clone_client_test_only(names[2]);

    println!("Case 0 - two validators lock the object with the same tx");
    assert!(client0
        .handle_transaction(tx.clone(), Some(client_ip))
        .await
        .is_ok());
    assert!(client1
        .handle_transaction(tx.clone(), Some(client_ip))
        .await
        .is_ok());

    let tx2 = make_tx(&gas, sender, &keypair, rgp);
    let res = quorum_driver
        .submit_transaction(ExecuteTransactionRequestV3::new_v2(tx2))
        .await
        .unwrap()
        .await;

    // Aggregator waits for all responses when it sees a conflicting tx and because
    // there are not enough retryable errors to push the original tx or the most staked
    // conflicting tx >= 2f+1 stake. Neither transaction can be retried due to client
    // double spend and this is a fatal error.
    if let Err(QuorumDriverError::ObjectsDoubleUsed { conflicting_txes }) = res {
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

    assert!(client0
        .handle_transaction(tx.clone(), Some(client_ip))
        .await
        .is_ok());
    assert!(client1
        .handle_transaction(tx.clone(), Some(client_ip))
        .await
        .is_ok(),);
    assert!(client2
        .handle_transaction(tx.clone(), Some(client_ip))
        .await
        .is_ok(),);

    let tx2 = make_tx(&gas, sender, &keypair, rgp);

    let res = quorum_driver
        .submit_transaction(ExecuteTransactionRequestV3::new_v2(tx2))
        .await
        .unwrap()
        .await;
    // Aggregator gets three bad responses, and tries tx, which should succeed.
    if let Err(QuorumDriverError::ObjectsDoubleUsed { conflicting_txes }) = res {
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
    assert!(client0
        .handle_transaction(tx.clone(), Some(client_ip))
        .await
        .is_ok());

    let tx2 = make_tx(&gas, sender, &keypair, rgp);
    let tx2_digest = *tx2.digest();

    let res = quorum_driver
        .submit_transaction(ExecuteTransactionRequestV3::new_v2(tx2))
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

    assert!(client0
        .handle_transaction(tx.clone(), Some(client_ip))
        .await
        .is_ok());
    assert!(client1
        .handle_transaction(tx.clone(), Some(client_ip))
        .await
        .is_ok());
    assert!(client2
        .handle_transaction(tx2.clone(), Some(client_ip))
        .await
        .is_ok());

    let tx3 = make_tx(&gas, sender, &keypair, rgp);

    let res = quorum_driver
        .submit_transaction(ExecuteTransactionRequestV3::new_v2(tx3))
        .await
        .unwrap()
        .await;

    if let Err(QuorumDriverError::ObjectsDoubleUsed { conflicting_txes }) = res {
        assert_eq!(conflicting_txes.len(), 2);
        let tx_stake = conflicting_txes.get(tx.digest()).unwrap().1;
        assert!(tx_stake == 2500 || tx_stake == 5000);
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
    assert!(client0
        .handle_transaction(tx.clone(), Some(client_ip))
        .await
        .is_ok());
    assert!(client1
        .handle_transaction(tx.clone(), Some(client_ip))
        .await
        .is_ok());
    assert!(client2
        .handle_transaction(tx2.clone(), Some(client_ip))
        .await
        .is_ok());
    let res = quorum_driver
        .submit_transaction(ExecuteTransactionRequestV3::new_v2(tx2))
        .await
        .unwrap()
        .await;

    if let Err(QuorumDriverError::ObjectsDoubleUsed { conflicting_txes }) = res {
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

    assert!(client0
        .handle_transaction(tx.clone(), Some(client_ip))
        .await
        .is_ok());
    assert!(client1
        .handle_transaction(tx.clone(), Some(client_ip))
        .await
        .is_ok());
    assert!(client2
        .handle_transaction(tx2, Some(client_ip))
        .await
        .is_ok());

    let res = quorum_driver
        .submit_transaction(ExecuteTransactionRequestV3::new_v2(tx))
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
    assert!(client0
        .handle_transaction(tx.clone(), Some(client_ip))
        .await
        .is_ok());
    assert!(client1
        .handle_transaction(tx2.clone(), Some(client_ip))
        .await
        .is_ok());
    assert!(client2
        .handle_transaction(tx3.clone(), Some(client_ip))
        .await
        .is_ok());

    let tx4 = make_tx(&gas, sender, &keypair, rgp);
    let res = quorum_driver
        .submit_transaction(ExecuteTransactionRequestV3::new_v2(tx4.clone()))
        .await
        .unwrap()
        .await;

    if let Err(QuorumDriverError::ObjectsDoubleUsed { conflicting_txes }) = res {
        assert!(conflicting_txes.len() == 3 || conflicting_txes.len() == 2);
        assert!(conflicting_txes
            .iter()
            .all(|(digest, (_objs, stake))| (*stake == 2500)
                && (digest == tx.digest() || digest == tx2.digest() || digest == tx3.digest())));
    } else {
        panic!(
            "expect Err(QuorumDriverError::ObjectsDoubleUsed) but got {:?}",
            res
        )
    }

    Ok(())
}

// Tests that quorum driver can continuously retry txn with SystemOverloadedRetryAfter error.
#[sim_test]
async fn test_quorum_driver_handling_overload_and_retry() {
    telemetry_subscribers::init_for_testing();

    // Setup
    let (sender, keypair): (_, AccountKeyPair) = get_key_pair();
    let gas_object = Object::with_owner_for_testing(sender);
    let (mut aggregator, authorities, genesis, _) =
        init_local_authorities(4, vec![gas_object.clone()]).await;

    // Make local authority client to always return SystemOverloadedRetryAfter error.
    let fault_config = LocalAuthorityClientFaultConfig {
        overload_retry_after_handle_transaction: Some(Duration::from_secs(30)),
        ..Default::default()
    };
    let mut clients = aggregator.clone_inner_clients_test_only();
    for client in &mut clients.values_mut() {
        client.authority_client_mut().fault_config = fault_config;
    }
    let clients = clients.into_iter().map(|(k, v)| (k, Arc::new(v))).collect();
    aggregator.authority_clients = Arc::new(clients);

    // Create a transaction for the test.
    let rgp = authorities
        .first()
        .unwrap()
        .reference_gas_price_for_testing()
        .unwrap();
    let gas_object = genesis
        .objects()
        .iter()
        .find(|o| o.id() == gas_object.id())
        .unwrap();
    let tx = make_tx(gas_object, sender, &keypair, rgp);

    // Use a fail point to count the number of retries to test that the quorum backoff logic
    // respects above `overload_retry_after_handle_transaction`.
    let retry_count: Arc<AtomicUsize> = Arc::new(AtomicUsize::new(0));
    let retry_count_clone = retry_count.clone();
    register_fail_point("count_retry_times", move || {
        retry_count_clone.fetch_add(1, Ordering::SeqCst);
    });

    // Create a quorum driver with max_retry_times = 0.
    let arc_aggregator = Arc::new(aggregator.clone());
    let quorum_driver_handler = Arc::new(
        QuorumDriverHandlerBuilder::new(
            arc_aggregator.clone(),
            Arc::new(QuorumDriverMetrics::new_for_tests()),
        )
        .with_reconfig_observer(Arc::new(DummyReconfigObserver {}))
        .with_max_retry_times(0)
        .start(),
    );

    // Submit the transaction, and check that it shouldn't return, and the number of retries is within
    // 300s timeout / 30s retry after duration = 10 times.
    let ticket = quorum_driver_handler
        .submit_transaction(ExecuteTransactionRequestV3::new_v2(tx))
        .await
        .unwrap();
    match timeout(Duration::from_secs(300), ticket).await {
        Ok(result) => panic!("Process transaction should timeout! {:?}", result),
        Err(_) => {
            assert!(retry_count.load(Ordering::SeqCst) <= 10);
            println!("Waiting for txn timed out! This is desired behavior.")
        }
    }
}
