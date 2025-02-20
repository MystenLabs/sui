// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{time::Duration, vec};

use sui_test_transaction_builder::TestTransactionBuilder;
use sui_types::executable_transaction::VerifiedExecutableTransaction;
use sui_types::object::Owner;
use sui_types::transaction::VerifiedTransaction;
use sui_types::{
    base_types::{ObjectID, SequenceNumber},
    crypto::deterministic_random_account_key,
    object::Object,
    storage::InputKey,
    transaction::{CallArg, ObjectArg},
    SUI_FRAMEWORK_PACKAGE_ID,
};
use tokio::time::Instant;
use tokio::{
    sync::mpsc::{error::TryRecvError, unbounded_channel, UnboundedReceiver},
    time::sleep,
};

use crate::{
    authority::{authority_tests::init_state_with_objects, AuthorityState},
    transaction_manager::{PendingCertificate, TransactionManager},
};

#[allow(clippy::disallowed_methods)] // allow unbounded_channel()
fn make_transaction_manager(
    state: &AuthorityState,
) -> (TransactionManager, UnboundedReceiver<PendingCertificate>) {
    // Create a new transaction manager instead of reusing the authority's, to examine
    // transaction_manager output from rx_ready_certificates.
    let (tx_ready_certificates, rx_ready_certificates) = unbounded_channel();
    let transaction_manager = TransactionManager::new(
        state.get_object_cache_reader().clone(),
        state.get_transaction_cache_reader().clone(),
        &state.epoch_store_for_testing(),
        tx_ready_certificates,
        state.metrics.clone(),
    );

    (transaction_manager, rx_ready_certificates)
}

fn make_transaction(gas_object: Object, input: Vec<CallArg>) -> VerifiedExecutableTransaction {
    // Use fake module, function, package and gas prices since they are irrelevant for testing
    // transaction manager.
    let rgp = 100;
    let (sender, keypair) = deterministic_random_account_key();
    let transaction =
        TestTransactionBuilder::new(sender, gas_object.compute_object_reference(), rgp)
            .move_call(SUI_FRAMEWORK_PACKAGE_ID, "counter", "assert_value", input)
            .build_and_sign(&keypair);
    VerifiedExecutableTransaction::new_system(VerifiedTransaction::new_unchecked(transaction), 0)
}

fn get_input_keys(objects: &[Object]) -> Vec<InputKey> {
    objects
        .iter()
        .map(|object| InputKey::VersionedObject {
            id: object.full_id(),
            version: object.version(),
        })
        .collect()
}

#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn transaction_manager_basics() {
    // Initialize an authority state.
    let (owner, _keypair) = deterministic_random_account_key();
    let gas_objects: Vec<Object> = (0..10)
        .map(|_| {
            let gas_object_id = ObjectID::random();
            Object::with_id_owner_for_testing(gas_object_id, owner)
        })
        .collect();
    let state = init_state_with_objects(gas_objects.clone()).await;

    // Create a new transaction manager instead of reusing the authority's, to examine
    // transaction_manager output from rx_ready_certificates.
    let (transaction_manager, mut rx_ready_certificates) = make_transaction_manager(&state);
    // TM should output no transaction.
    assert!(rx_ready_certificates
        .try_recv()
        .is_err_and(|err| err == TryRecvError::Empty));
    // TM should be empty at the beginning.
    transaction_manager.check_empty_for_testing();

    // Enqueue empty vec should not crash.
    transaction_manager.enqueue(vec![], &state.epoch_store_for_testing());
    // TM should output no transaction.
    assert!(rx_ready_certificates
        .try_recv()
        .is_err_and(|err| err == TryRecvError::Empty));

    // Enqueue a transaction with existing gas object, empty input.
    let transaction = make_transaction(gas_objects[0].clone(), vec![]);
    let tx_start_time = Instant::now();
    transaction_manager.enqueue(vec![transaction.clone()], &state.epoch_store_for_testing());
    // TM should output the transaction eventually.
    let pending_certificate = rx_ready_certificates.recv().await.unwrap();

    // Tests that pending certificate stats are recorded properly.
    assert!(pending_certificate.stats.enqueue_time >= tx_start_time);
    assert!(
        pending_certificate.stats.ready_time.unwrap() >= pending_certificate.stats.enqueue_time
    );

    assert_eq!(transaction_manager.inflight_queue_len(), 1);

    // Notify TM about transaction commit
    transaction_manager.notify_commit(
        transaction.digest(),
        vec![],
        &state.epoch_store_for_testing(),
    );

    // TM should be empty.
    transaction_manager.check_empty_for_testing();

    // Enqueue a transaction with a new gas object, empty input.
    let gas_object_new = Object::with_id_owner_version_for_testing(
        ObjectID::random(),
        0.into(),
        Owner::AddressOwner(owner),
    );
    let transaction = make_transaction(gas_object_new.clone(), vec![]);
    let tx_start_time = Instant::now();
    transaction_manager.enqueue(vec![transaction.clone()], &state.epoch_store_for_testing());
    // TM should output no transaction yet.
    sleep(Duration::from_secs(1)).await;
    assert!(rx_ready_certificates
        .try_recv()
        .is_err_and(|err| err == TryRecvError::Empty));

    assert_eq!(transaction_manager.inflight_queue_len(), 1);

    // Duplicated enqueue is allowed.
    transaction_manager.enqueue(vec![transaction.clone()], &state.epoch_store_for_testing());
    sleep(Duration::from_secs(1)).await;
    assert!(rx_ready_certificates
        .try_recv()
        .is_err_and(|err| err == TryRecvError::Empty));

    assert_eq!(transaction_manager.inflight_queue_len(), 1);

    // Notify TM about availability of the gas object.
    transaction_manager.objects_available(
        get_input_keys(&[gas_object_new]),
        &state.epoch_store_for_testing(),
    );
    // TM should output the transaction eventually.
    let pending_certificate = rx_ready_certificates.recv().await.unwrap();

    // Tests that pending certificate stats are recorded properly. The ready time should be
    // 2 seconds apart from the enqueue time.
    assert!(pending_certificate.stats.enqueue_time >= tx_start_time);
    assert!(
        pending_certificate.stats.ready_time.unwrap() - pending_certificate.stats.enqueue_time
            >= Duration::from_secs(2)
    );

    // Re-enqueue the same transaction should not result in another output.
    transaction_manager.enqueue(vec![transaction.clone()], &state.epoch_store_for_testing());
    sleep(Duration::from_secs(1)).await;
    assert!(rx_ready_certificates
        .try_recv()
        .is_err_and(|err| err == TryRecvError::Empty));

    // Notify TM about transaction commit
    transaction_manager.notify_commit(
        transaction.digest(),
        vec![],
        &state.epoch_store_for_testing(),
    );

    // TM should be empty at the end.
    transaction_manager.check_empty_for_testing();
}

// Tests when objects become available, correct set of transactions can be sent to execute.
// Specifically, we have following setup,
//         shared_object     shared_object_2
//       /    |    \     \    /
//    tx_0  tx_1  tx_2    tx_3
//     r      r     w      r
// And when shared_object is available, tx_0, tx_1, and tx_2 can be executed. And when
// shared_object_2 becomes available, tx_3 can be executed.
#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn transaction_manager_object_dependency() {
    // Initialize an authority state, with gas objects and a shared object.
    let (owner, _keypair) = deterministic_random_account_key();
    let gas_objects: Vec<Object> = (0..10)
        .map(|_| {
            let gas_object_id = ObjectID::random();
            Object::with_id_owner_for_testing(gas_object_id, owner)
        })
        .collect();
    let shared_object = Object::shared_for_testing();
    let initial_shared_version = shared_object.owner().start_version().unwrap();
    let shared_object_2 = Object::shared_for_testing();
    let initial_shared_version_2 = shared_object_2.owner().start_version().unwrap();

    let state = init_state_with_objects(
        [
            gas_objects.clone(),
            vec![shared_object.clone(), shared_object_2.clone()],
        ]
        .concat(),
    )
    .await;

    // Create a new transaction manager instead of reusing the authority's, to examine
    // transaction_manager output from rx_ready_certificates.
    let (transaction_manager, mut rx_ready_certificates) = make_transaction_manager(&state);
    // TM should output no transaction.
    assert!(rx_ready_certificates.try_recv().is_err());

    // Enqueue two transactions with the same shared object input in read-only mode.
    let shared_version = 1000.into();
    let shared_object_arg_read = ObjectArg::SharedObject {
        id: shared_object.id(),
        initial_shared_version,
        mutable: false,
    };
    let transaction_read_0 = make_transaction(
        gas_objects[0].clone(),
        vec![CallArg::Object(shared_object_arg_read)],
    );
    let transaction_read_1 = make_transaction(
        gas_objects[1].clone(),
        vec![CallArg::Object(shared_object_arg_read)],
    );
    state
        .epoch_store_for_testing()
        .set_shared_object_versions_for_testing(
            transaction_read_0.digest(),
            &[(
                (
                    shared_object.id(),
                    shared_object.owner().start_version().unwrap(),
                ),
                shared_version,
            )],
        )
        .unwrap();
    state
        .epoch_store_for_testing()
        .set_shared_object_versions_for_testing(
            transaction_read_1.digest(),
            &[(
                (
                    shared_object.id(),
                    shared_object.owner().start_version().unwrap(),
                ),
                shared_version,
            )],
        )
        .unwrap();

    // Enqueue one transaction with the same shared object in mutable mode.
    let shared_object_arg_default = ObjectArg::SharedObject {
        id: shared_object.id(),
        initial_shared_version,
        mutable: true,
    };
    let transaction_default = make_transaction(
        gas_objects[2].clone(),
        vec![CallArg::Object(shared_object_arg_default)],
    );
    state
        .epoch_store_for_testing()
        .set_shared_object_versions_for_testing(
            transaction_default.digest(),
            &[(
                (
                    shared_object.id(),
                    shared_object.owner().start_version().unwrap(),
                ),
                shared_version,
            )],
        )
        .unwrap();

    // Enqueue one transaction with two readonly shared object inputs, `shared_object` and `shared_object_2`.
    let shared_version_2 = 1000.into();
    let shared_object_arg_read_2 = ObjectArg::SharedObject {
        id: shared_object_2.id(),
        initial_shared_version: initial_shared_version_2,
        mutable: false,
    };
    let transaction_read_2 = make_transaction(
        gas_objects[3].clone(),
        vec![
            CallArg::Object(shared_object_arg_default),
            CallArg::Object(shared_object_arg_read_2),
        ],
    );
    state
        .epoch_store_for_testing()
        .set_shared_object_versions_for_testing(
            transaction_read_2.digest(),
            &[
                (
                    (
                        shared_object.id(),
                        shared_object.owner().start_version().unwrap(),
                    ),
                    shared_version,
                ),
                (
                    (
                        shared_object_2.id(),
                        shared_object_2.owner().start_version().unwrap(),
                    ),
                    shared_version_2,
                ),
            ],
        )
        .unwrap();

    transaction_manager.enqueue(
        vec![
            transaction_read_0.clone(),
            transaction_read_1.clone(),
            transaction_default.clone(),
            transaction_read_2.clone(),
        ],
        &state.epoch_store_for_testing(),
    );

    // TM should output no transaction yet.
    sleep(Duration::from_secs(1)).await;
    assert!(rx_ready_certificates.try_recv().is_err());

    assert_eq!(transaction_manager.inflight_queue_len(), 4);

    // Notify TM about availability of the first shared object.
    transaction_manager.objects_available(
        vec![InputKey::VersionedObject {
            id: shared_object.full_id(),
            version: shared_version,
        }],
        &state.epoch_store_for_testing(),
    );

    // TM should output the 3 transactions that are only waiting for this object.
    let tx_0 = rx_ready_certificates.recv().await.unwrap().certificate;
    let tx_1 = rx_ready_certificates.recv().await.unwrap().certificate;
    let tx_2 = rx_ready_certificates.recv().await.unwrap().certificate;
    {
        let mut want_digests = vec![
            transaction_read_0.digest(),
            transaction_read_1.digest(),
            transaction_default.digest(),
        ];
        want_digests.sort();
        let mut got_digests = vec![tx_0.digest(), tx_1.digest(), tx_2.digest()];
        got_digests.sort();
        assert_eq!(want_digests, got_digests);
    }

    sleep(Duration::from_secs(1)).await;
    assert!(rx_ready_certificates.try_recv().is_err());

    assert_eq!(transaction_manager.inflight_queue_len(), 4);

    // Notify TM about read-only transaction commit
    transaction_manager.notify_commit(tx_0.digest(), vec![], &state.epoch_store_for_testing());
    transaction_manager.notify_commit(tx_1.digest(), vec![], &state.epoch_store_for_testing());
    transaction_manager.notify_commit(tx_2.digest(), vec![], &state.epoch_store_for_testing());

    assert_eq!(transaction_manager.inflight_queue_len(), 1);

    // Make shared_object_2 available.
    transaction_manager.objects_available(
        vec![InputKey::VersionedObject {
            id: shared_object_2.full_id(),
            version: shared_version_2,
        }],
        &state.epoch_store_for_testing(),
    );

    // Now, the transaction waiting for both shared objects can be executed.
    let tx_3 = rx_ready_certificates.recv().await.unwrap().certificate;
    assert_eq!(transaction_read_2.digest(), tx_3.digest());

    sleep(Duration::from_secs(1)).await;
    assert!(rx_ready_certificates.try_recv().is_err());

    assert_eq!(transaction_manager.inflight_queue_len(), 1);

    // Notify TM about tx_3.
    transaction_manager.notify_commit(tx_3.digest(), vec![], &state.epoch_store_for_testing());

    transaction_manager.check_empty_for_testing();
}

#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn transaction_manager_receiving_notify_commit() {
    telemetry_subscribers::init_for_testing();
    // Initialize an authority state.
    let (owner, _keypair) = deterministic_random_account_key();
    let gas_objects: Vec<Object> = (0..10)
        .map(|_| {
            let gas_object_id = ObjectID::random();
            Object::with_id_owner_for_testing(gas_object_id, owner)
        })
        .collect();
    let state = init_state_with_objects(gas_objects.clone()).await;

    // Create a new transaction manager instead of reusing the authority's, to examine
    // transaction_manager output from rx_ready_certificates.
    let (transaction_manager, mut rx_ready_certificates) = make_transaction_manager(&state);
    // TM should output no transaction.
    assert!(rx_ready_certificates.try_recv().is_err());
    // TM should be empty at the beginning.
    transaction_manager.check_empty_for_testing();

    let obj_id = ObjectID::random();
    let object_arguments: Vec<_> = (0..10)
        .map(|i| {
            let object = Object::with_id_owner_version_for_testing(
                obj_id,
                i.into(),
                Owner::AddressOwner(owner),
            );
            // Every other transaction receives the object, and we create a run of multiple receives in
            // a row at the beginning to test that the TM doesn't get stuck in either configuration of:
            // ImmOrOwnedObject => Receiving,
            // Receiving => Receiving
            // Receiving => ImmOrOwnedObject
            // ImmOrOwnedObject => ImmOrOwnedObject is already tested as the default case on mainnet.
            let object_arg = if i % 2 == 0 || i == 3 {
                ObjectArg::Receiving(object.compute_object_reference())
            } else {
                ObjectArg::ImmOrOwnedObject(object.compute_object_reference())
            };
            let txn = make_transaction(gas_objects[0].clone(), vec![CallArg::Object(object_arg)]);
            (object, txn)
        })
        .collect();

    for (i, (_, txn)) in object_arguments.iter().enumerate() {
        // TM should output no transaction yet since waiting on receiving object or
        // ImmOrOwnedObject input.
        transaction_manager.enqueue(vec![txn.clone()], &state.epoch_store_for_testing());
        sleep(Duration::from_secs(1)).await;
        assert!(rx_ready_certificates.try_recv().is_err());
        assert_eq!(transaction_manager.inflight_queue_len(), i + 1);
    }

    // Start things off by notifying TM that the receiving object 0 is available.
    transaction_manager.objects_available(
        get_input_keys(&[object_arguments[0].0.clone()]),
        &state.epoch_store_for_testing(),
    );

    // Now start to unravel the rest of the transactions by notifying that each subsequent
    // transaction has been processed.
    for (i, (object, txn)) in object_arguments.iter().enumerate() {
        // TM should output the transaction eventually now that the receiving object has become
        // available.
        rx_ready_certificates.recv().await.unwrap();

        // Only one transaction at a time should become available though. So if we try to get
        // another one it should fail.
        sleep(Duration::from_secs(1)).await;
        assert!(rx_ready_certificates.try_recv().is_err());

        // Notify the TM that the transaction has been processed, and that it has written the
        // object at the next version.
        transaction_manager.notify_commit(
            txn.digest(),
            vec![InputKey::VersionedObject {
                id: object.full_id(),
                version: object.version().next(),
            }],
            &state.epoch_store_for_testing(),
        );

        // TM should now output another transaction to run since it the next version of that object
        // has become available.
        assert_eq!(
            transaction_manager.inflight_queue_len(),
            object_arguments.len() - i - 1
        );
    }

    // After everything TM should be empty.
    transaction_manager.check_empty_for_testing();
}

#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn transaction_manager_receiving_object_ready_notifications() {
    telemetry_subscribers::init_for_testing();
    // Initialize an authority state.
    let (owner, _keypair) = deterministic_random_account_key();
    let gas_objects: Vec<Object> = (0..10)
        .map(|_| {
            let gas_object_id = ObjectID::random();
            Object::with_id_owner_for_testing(gas_object_id, owner)
        })
        .collect();
    let state = init_state_with_objects(gas_objects.clone()).await;

    // Create a new transaction manager instead of reusing the authority's, to examine
    // transaction_manager output from rx_ready_certificates.
    let (transaction_manager, mut rx_ready_certificates) = make_transaction_manager(&state);
    // TM should output no transaction.
    assert!(rx_ready_certificates.try_recv().is_err());
    // TM should be empty at the beginning.
    transaction_manager.check_empty_for_testing();

    let obj_id = ObjectID::random();
    let receiving_object_new0 =
        Object::with_id_owner_version_for_testing(obj_id, 0.into(), Owner::AddressOwner(owner));
    let receiving_object_new1 =
        Object::with_id_owner_version_for_testing(obj_id, 1.into(), Owner::AddressOwner(owner));
    let receiving_object_arg0 =
        ObjectArg::Receiving(receiving_object_new0.compute_object_reference());
    let receive_object_transaction0 = make_transaction(
        gas_objects[0].clone(),
        vec![CallArg::Object(receiving_object_arg0)],
    );

    let receiving_object_arg1 =
        ObjectArg::Receiving(receiving_object_new1.compute_object_reference());
    let receive_object_transaction1 = make_transaction(
        gas_objects[0].clone(),
        vec![CallArg::Object(receiving_object_arg1)],
    );

    // TM should output no transaction yet since waiting on receiving object.
    transaction_manager.enqueue(
        vec![receive_object_transaction0.clone()],
        &state.epoch_store_for_testing(),
    );
    sleep(Duration::from_secs(1)).await;
    assert!(rx_ready_certificates.try_recv().is_err());
    assert_eq!(transaction_manager.inflight_queue_len(), 1);

    // TM should output no transaction yet since waiting on receiving object.
    transaction_manager.enqueue(
        vec![receive_object_transaction1.clone()],
        &state.epoch_store_for_testing(),
    );
    sleep(Duration::from_secs(1)).await;
    assert!(rx_ready_certificates.try_recv().is_err());
    assert_eq!(transaction_manager.inflight_queue_len(), 2);

    // Duplicate enqueue of receiving object is allowed.
    transaction_manager.enqueue(
        vec![receive_object_transaction0.clone()],
        &state.epoch_store_for_testing(),
    );
    sleep(Duration::from_secs(1)).await;
    assert!(rx_ready_certificates.try_recv().is_err());
    assert_eq!(transaction_manager.inflight_queue_len(), 2);

    // Notify TM that the receiving object 0 is available.
    transaction_manager.objects_available(
        get_input_keys(&[receiving_object_new0.clone()]),
        &state.epoch_store_for_testing(),
    );

    // TM should output the transaction eventually now that the receiving object has become
    // available.
    rx_ready_certificates.recv().await.unwrap();

    // Notify TM that the receiving object 0 is available.
    transaction_manager.objects_available(
        get_input_keys(&[receiving_object_new1.clone()]),
        &state.epoch_store_for_testing(),
    );

    // TM should output the transaction eventually now that the receiving object has become
    // available.
    rx_ready_certificates.recv().await.unwrap();
}

#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn transaction_manager_receiving_object_ready_notifications_multiple_of_same_receiving() {
    telemetry_subscribers::init_for_testing();
    // Initialize an authority state.
    let (owner, _keypair) = deterministic_random_account_key();
    let gas_objects: Vec<Object> = (0..10)
        .map(|_| {
            let gas_object_id = ObjectID::random();
            Object::with_id_owner_for_testing(gas_object_id, owner)
        })
        .collect();
    let state = init_state_with_objects(gas_objects.clone()).await;

    // Create a new transaction manager instead of reusing the authority's, to examine
    // transaction_manager output from rx_ready_certificates.
    let (transaction_manager, mut rx_ready_certificates) = make_transaction_manager(&state);
    // TM should output no transaction.
    assert!(rx_ready_certificates.try_recv().is_err());
    // TM should be empty at the beginning.
    transaction_manager.check_empty_for_testing();

    let obj_id = ObjectID::random();
    let receiving_object_new0 =
        Object::with_id_owner_version_for_testing(obj_id, 0.into(), Owner::AddressOwner(owner));
    let receiving_object_new1 =
        Object::with_id_owner_version_for_testing(obj_id, 1.into(), Owner::AddressOwner(owner));
    let receiving_object_arg0 =
        ObjectArg::Receiving(receiving_object_new0.compute_object_reference());
    let receive_object_transaction0 = make_transaction(
        gas_objects[0].clone(),
        vec![CallArg::Object(receiving_object_arg0)],
    );

    let receive_object_transaction01 = make_transaction(
        gas_objects[1].clone(),
        vec![CallArg::Object(receiving_object_arg0)],
    );

    let receiving_object_arg1 =
        ObjectArg::Receiving(receiving_object_new1.compute_object_reference());
    let receive_object_transaction1 = make_transaction(
        gas_objects[0].clone(),
        vec![CallArg::Object(receiving_object_arg1)],
    );

    // Enqueuing a transaction with a receiving object that is available at the time it is enqueued
    // should become immediately available.
    let gas_receiving_arg = ObjectArg::Receiving(gas_objects[3].compute_object_reference());
    let tx1 = make_transaction(
        gas_objects[0].clone(),
        vec![CallArg::Object(gas_receiving_arg)],
    );

    // TM should output no transaction yet since waiting on receiving object.
    transaction_manager.enqueue(
        vec![receive_object_transaction0.clone()],
        &state.epoch_store_for_testing(),
    );
    sleep(Duration::from_secs(1)).await;
    assert!(rx_ready_certificates.try_recv().is_err());
    assert_eq!(transaction_manager.inflight_queue_len(), 1);

    // TM should output no transaction yet since waiting on receiving object.
    transaction_manager.enqueue(
        vec![receive_object_transaction1.clone()],
        &state.epoch_store_for_testing(),
    );
    sleep(Duration::from_secs(1)).await;
    assert!(rx_ready_certificates.try_recv().is_err());
    assert_eq!(transaction_manager.inflight_queue_len(), 2);

    // Different transaction with a duplicate receiving object reference is allowed.
    // Both transaction's will be outputted once the receiving object is available.
    transaction_manager.enqueue(
        vec![receive_object_transaction01.clone()],
        &state.epoch_store_for_testing(),
    );
    sleep(Duration::from_secs(1)).await;
    assert!(rx_ready_certificates.try_recv().is_err());
    assert_eq!(transaction_manager.inflight_queue_len(), 3);

    // Notify TM that the receiving object 0 is available.
    transaction_manager.objects_available(
        get_input_keys(&[receiving_object_new0.clone()]),
        &state.epoch_store_for_testing(),
    );

    // TM should output both transactions depending on the receiving object now that the
    // transaction's receiving object has become available.
    rx_ready_certificates.recv().await.unwrap();

    rx_ready_certificates.recv().await.unwrap();

    // Only two transactions that were dependent on the receiving object should be output.
    assert!(rx_ready_certificates.try_recv().is_err());

    // Enqueue a transaction with a receiving object that is available at the time it is enqueued.
    // This should be immediately available.
    transaction_manager.enqueue(vec![tx1.clone()], &state.epoch_store_for_testing());
    sleep(Duration::from_secs(1)).await;
    rx_ready_certificates.recv().await.unwrap();

    // Notify TM that the receiving object 0 is available.
    transaction_manager.objects_available(
        get_input_keys(&[receiving_object_new1.clone()]),
        &state.epoch_store_for_testing(),
    );

    // TM should output the transaction eventually now that the receiving object has become
    // available.
    rx_ready_certificates.recv().await.unwrap();
}

#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn transaction_manager_receiving_object_ready_if_current_version_greater() {
    telemetry_subscribers::init_for_testing();
    // Initialize an authority state.
    let (owner, _keypair) = deterministic_random_account_key();
    let mut gas_objects: Vec<Object> = (0..10)
        .map(|_| {
            let gas_object_id = ObjectID::random();
            Object::with_id_owner_for_testing(gas_object_id, owner)
        })
        .collect();
    let receiving_object = Object::with_id_owner_version_for_testing(
        ObjectID::random(),
        10.into(),
        Owner::AddressOwner(owner),
    );
    gas_objects.push(receiving_object.clone());
    let state = init_state_with_objects(gas_objects.clone()).await;

    // Create a new transaction manager instead of reusing the authority's, to examine
    // transaction_manager output from rx_ready_certificates.
    let (transaction_manager, mut rx_ready_certificates) = make_transaction_manager(&state);
    // TM should output no transaction.
    assert!(rx_ready_certificates.try_recv().is_err());
    // TM should be empty at the beginning.
    transaction_manager.check_empty_for_testing();

    let receiving_object_new0 = Object::with_id_owner_version_for_testing(
        receiving_object.id(),
        0.into(),
        Owner::AddressOwner(owner),
    );
    let receiving_object_new1 = Object::with_id_owner_version_for_testing(
        receiving_object.id(),
        1.into(),
        Owner::AddressOwner(owner),
    );
    let receiving_object_arg0 =
        ObjectArg::Receiving(receiving_object_new0.compute_object_reference());
    let receive_object_transaction0 = make_transaction(
        gas_objects[0].clone(),
        vec![CallArg::Object(receiving_object_arg0)],
    );

    let receive_object_transaction01 = make_transaction(
        gas_objects[1].clone(),
        vec![CallArg::Object(receiving_object_arg0)],
    );

    let receiving_object_arg1 =
        ObjectArg::Receiving(receiving_object_new1.compute_object_reference());
    let receive_object_transaction1 = make_transaction(
        gas_objects[0].clone(),
        vec![CallArg::Object(receiving_object_arg1)],
    );

    // TM should output no transaction yet since waiting on receiving object.
    transaction_manager.enqueue(
        vec![receive_object_transaction0.clone()],
        &state.epoch_store_for_testing(),
    );
    transaction_manager.enqueue(
        vec![receive_object_transaction01.clone()],
        &state.epoch_store_for_testing(),
    );
    transaction_manager.enqueue(
        vec![receive_object_transaction1.clone()],
        &state.epoch_store_for_testing(),
    );
    sleep(Duration::from_secs(1)).await;
    rx_ready_certificates.recv().await.unwrap();
    rx_ready_certificates.recv().await.unwrap();
    rx_ready_certificates.recv().await.unwrap();
    assert!(rx_ready_certificates.try_recv().is_err());
}

// Tests transaction cancellation logic in transaction manager. Mainly tests that for cancelled transaction,
// transaction manager only waits for all non-shared objects to be available before outputting the transaction.
#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn transaction_manager_with_cancelled_transactions() {
    // Initialize an authority state, with gas objects and 3 shared objects.
    let (owner, _keypair) = deterministic_random_account_key();
    let gas_object = Object::with_id_owner_for_testing(ObjectID::random(), owner);
    let shared_object_1 = Object::shared_for_testing();
    let initial_shared_version_1 = shared_object_1.owner().start_version().unwrap();
    let shared_object_2 = Object::shared_for_testing();
    let initial_shared_version_2 = shared_object_2.owner().start_version().unwrap();
    let owned_object = Object::with_id_owner_for_testing(ObjectID::random(), owner);

    let state = init_state_with_objects(vec![
        gas_object.clone(),
        shared_object_1.clone(),
        shared_object_2.clone(),
        owned_object.clone(),
    ])
    .await;

    // Create a new transaction manager instead of reusing the authority's, to examine
    // transaction_manager output from rx_ready_certificates.
    let (transaction_manager, mut rx_ready_certificates) = make_transaction_manager(&state);
    // TM should output no transaction.
    assert!(rx_ready_certificates.try_recv().is_err());

    // Enqueue one transaction with 2 shared object inputs and 1 owned input.
    let shared_object_arg_1 = ObjectArg::SharedObject {
        id: shared_object_1.id(),
        initial_shared_version: initial_shared_version_1,
        mutable: true,
    };
    let shared_object_arg_2 = ObjectArg::SharedObject {
        id: shared_object_2.id(),
        initial_shared_version: initial_shared_version_2,
        mutable: true,
    };

    // Changes the desired owned object version to a higher version. We will make it available later.
    let owned_version = 2000.into();
    let mut owned_ref = owned_object.compute_object_reference();
    owned_ref.1 = owned_version;
    let owned_object_arg = ObjectArg::ImmOrOwnedObject(owned_ref);

    let cancelled_transaction = make_transaction(
        gas_object.clone(),
        vec![
            CallArg::Object(shared_object_arg_1),
            CallArg::Object(shared_object_arg_2),
            CallArg::Object(owned_object_arg),
        ],
    );
    state
        .epoch_store_for_testing()
        .set_shared_object_versions_for_testing(
            cancelled_transaction.digest(),
            &[
                (
                    (
                        shared_object_1.id(),
                        shared_object_1.owner().start_version().unwrap(),
                    ),
                    SequenceNumber::CANCELLED_READ,
                ),
                (
                    (
                        shared_object_2.id(),
                        shared_object_2.owner().start_version().unwrap(),
                    ),
                    SequenceNumber::CONGESTED,
                ),
            ],
        )
        .unwrap();

    transaction_manager.enqueue(
        vec![cancelled_transaction.clone()],
        &state.epoch_store_for_testing(),
    );

    // TM should output no transaction yet.
    sleep(Duration::from_secs(1)).await;
    assert!(rx_ready_certificates.try_recv().is_err());

    assert_eq!(transaction_manager.inflight_queue_len(), 1);

    // Notify TM about availability of the owned object.
    transaction_manager.objects_available(
        vec![InputKey::VersionedObject {
            id: owned_object.full_id(),
            version: owned_version,
        }],
        &state.epoch_store_for_testing(),
    );

    // TM should output the transaction as soon as the owned object is available.
    let available_txn = rx_ready_certificates.recv().await.unwrap().certificate;
    assert_eq!(available_txn.digest(), cancelled_transaction.digest());

    sleep(Duration::from_secs(1)).await;
    assert!(rx_ready_certificates.try_recv().is_err());

    assert_eq!(transaction_manager.inflight_queue_len(), 1);

    // Notify TM about read-only transaction commit
    transaction_manager.notify_commit(
        available_txn.digest(),
        vec![],
        &state.epoch_store_for_testing(),
    );

    assert_eq!(transaction_manager.inflight_queue_len(), 0);

    transaction_manager.check_empty_for_testing();
}
