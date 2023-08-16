// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{sync::Arc, time::Duration, vec};

use sui_test_transaction_builder::TestTransactionBuilder;
use sui_types::executable_transaction::VerifiedExecutableTransaction;
use sui_types::transaction::VerifiedTransaction;
use sui_types::{
    base_types::ObjectID,
    crypto::deterministic_random_account_key,
    digests::TransactionEffectsDigest,
    object::Object,
    transaction::{CallArg, ObjectArg},
    SUI_FRAMEWORK_PACKAGE_ID,
};
use tokio::{
    sync::mpsc::{unbounded_channel, UnboundedReceiver},
    sync::Semaphore,
    time::sleep,
};

use crate::{
    authority::{
        authority_store::InputKey, authority_tests::init_state_with_objects, AuthorityState,
    },
    execution_driver::ExecutionDispatcher,
    transaction_manager::TransactionManager,
};

#[allow(clippy::disallowed_methods)] // allow unbounded_channel()
fn make_transaction_manager(
    state: &AuthorityState,
) -> (
    TransactionManager,
    UnboundedReceiver<(
        VerifiedExecutableTransaction,
        Option<TransactionEffectsDigest>,
    )>,
) {
    // Create a new transaction manager instead of reusing the authority's, to examine
    // transaction_manager output from rx_ready_certificates.
    let (tx_ready_certificates, rx_ready_certificates) = unbounded_channel();
    // no permits so that we don't try to spawn execution tasks in txn manager test
    let execution_limit = Arc::new(Semaphore::new(0));
    let execution_dispatcher = Arc::new(ExecutionDispatcher::new(
        tx_ready_certificates,
        execution_limit,
        state.metrics.clone(),
    ));
    let transaction_manager = TransactionManager::new(
        state.database.clone(),
        &state.epoch_store_for_testing(),
        execution_dispatcher,
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
            id: object.id(),
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
    assert!(rx_ready_certificates.try_recv().is_err());
    // TM should be empty at the beginning.
    transaction_manager.check_empty_for_testing();

    // Enqueue empty vec should not crash.
    transaction_manager
        .enqueue(vec![], &state.epoch_store_for_testing())
        .unwrap();
    // TM should output no transaction.
    assert!(rx_ready_certificates.try_recv().is_err());

    // Enqueue a transaction with existing gas object, empty input.
    let transaction = make_transaction(gas_objects[0].clone(), vec![]);
    transaction_manager
        .enqueue(vec![transaction.clone()], &state.epoch_store_for_testing())
        .unwrap();
    // TM should output the transaction eventually.
    rx_ready_certificates.recv().await.unwrap();

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
    let gas_object_new =
        Object::with_id_owner_version_for_testing(ObjectID::random(), 0.into(), owner);
    let transaction = make_transaction(gas_object_new.clone(), vec![]);
    transaction_manager
        .enqueue(vec![transaction.clone()], &state.epoch_store_for_testing())
        .unwrap();
    // TM should output no transaction yet.
    sleep(Duration::from_secs(1)).await;
    assert!(rx_ready_certificates.try_recv().is_err());

    assert_eq!(transaction_manager.inflight_queue_len(), 1);

    // Duplicated enqueue is allowed.
    transaction_manager
        .enqueue(vec![transaction.clone()], &state.epoch_store_for_testing())
        .unwrap();
    sleep(Duration::from_secs(1)).await;
    assert!(rx_ready_certificates.try_recv().is_err());

    assert_eq!(transaction_manager.inflight_queue_len(), 1);

    // Notify TM about availability of the gas object.
    transaction_manager.objects_available(
        get_input_keys(&vec![gas_object_new]),
        &state.epoch_store_for_testing(),
    );
    // TM should output the transaction eventually.
    rx_ready_certificates.recv().await.unwrap();

    // Re-enqueue the same transaction should not result in another output.
    transaction_manager
        .enqueue(vec![transaction.clone()], &state.epoch_store_for_testing())
        .unwrap();
    sleep(Duration::from_secs(1)).await;
    assert!(rx_ready_certificates.try_recv().is_err());

    // Notify TM about transaction commit
    transaction_manager.notify_commit(
        transaction.digest(),
        vec![],
        &state.epoch_store_for_testing(),
    );

    // TM should be empty at the end.
    transaction_manager.check_empty_for_testing();
}

#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn transaction_manager_read_lock() {
    // Initialize an authority state, with gas objects and a shared object.
    let (owner, _keypair) = deterministic_random_account_key();
    let gas_objects: Vec<Object> = (0..10)
        .map(|_| {
            let gas_object_id = ObjectID::random();
            Object::with_id_owner_for_testing(gas_object_id, owner)
        })
        .collect();
    let shared_object = Object::shared_for_testing();

    let state =
        init_state_with_objects([gas_objects.clone(), vec![shared_object.clone()]].concat()).await;

    // Create a new transaction manager instead of reusing the authority's, to examine
    // transaction_manager output from rx_ready_certificates.
    let (transaction_manager, mut rx_ready_certificates) = make_transaction_manager(&state);
    // TM should output no transaction.
    assert!(rx_ready_certificates.try_recv().is_err());

    // Enqueue two transactions with the same shared object input in read-only mode.
    let shared_version = 1000.into();
    let shared_object_arg_read = ObjectArg::SharedObject {
        id: shared_object.id(),
        initial_shared_version: 0.into(),
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
            &vec![(shared_object.id(), shared_version)],
        )
        .unwrap();
    state
        .epoch_store_for_testing()
        .set_shared_object_versions_for_testing(
            transaction_read_1.digest(),
            &vec![(shared_object.id(), shared_version)],
        )
        .unwrap();

    // Enqueue one transaction with default lock on the same shared object and version.
    let shared_object_arg_default = ObjectArg::SharedObject {
        id: shared_object.id(),
        initial_shared_version: 0.into(),
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
            &vec![(shared_object.id(), shared_version)],
        )
        .unwrap();

    transaction_manager
        .enqueue(
            vec![
                transaction_read_0.clone(),
                transaction_read_1.clone(),
                transaction_default.clone(),
            ],
            &state.epoch_store_for_testing(),
        )
        .unwrap();

    // TM should output no transaction yet.
    sleep(Duration::from_secs(1)).await;
    assert!(rx_ready_certificates.try_recv().is_err());

    assert_eq!(transaction_manager.inflight_queue_len(), 3);

    // Notify TM about availability of the shared object.
    transaction_manager.objects_available(
        vec![InputKey::VersionedObject {
            id: shared_object.id(),
            version: shared_version,
        }],
        &state.epoch_store_for_testing(),
    );

    // TM should output the 2 read-only transactions eventually.
    let tx_0 = rx_ready_certificates.recv().await.unwrap().0;
    let tx_1 = rx_ready_certificates.recv().await.unwrap().0;
    let mut want_digests = vec![transaction_read_0.digest(), transaction_read_1.digest()];
    want_digests.sort();
    let mut got_digests = vec![tx_0.digest(), tx_1.digest()];
    got_digests.sort();
    assert_eq!(want_digests, got_digests);

    // TM should not output default-lock transaction yet.
    sleep(Duration::from_secs(1)).await;
    assert!(rx_ready_certificates.try_recv().is_err());

    assert_eq!(transaction_manager.inflight_queue_len(), 3);

    // Notify TM about read-only transaction commit
    transaction_manager.notify_commit(tx_0.digest(), vec![], &state.epoch_store_for_testing());
    transaction_manager.notify_commit(tx_1.digest(), vec![], &state.epoch_store_for_testing());

    // TM should output the default-lock transaction eventually.
    let tx_2 = rx_ready_certificates.recv().await.unwrap().0;
    assert_eq!(tx_2.digest(), transaction_default.digest());

    assert_eq!(transaction_manager.inflight_queue_len(), 1);

    // Notify TM about default-lock transaction commit
    transaction_manager.notify_commit(tx_2.digest(), vec![], &state.epoch_store_for_testing());
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
            let object = Object::with_id_owner_version_for_testing(obj_id, i.into(), owner);
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
        transaction_manager
            .enqueue(vec![txn.clone()], &state.epoch_store_for_testing())
            .unwrap();
        sleep(Duration::from_secs(1)).await;
        assert!(rx_ready_certificates.try_recv().is_err());
        assert_eq!(transaction_manager.inflight_queue_len(), i + 1);
    }

    // Start things off by notifying TM that the receiving object 0 is available.
    transaction_manager.objects_available(
        get_input_keys(&vec![object_arguments[0].0.clone()]),
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
                id: object.id(),
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
    let receiving_object_new0 = Object::with_id_owner_version_for_testing(obj_id, 0.into(), owner);
    let receiving_object_new1 = Object::with_id_owner_version_for_testing(obj_id, 1.into(), owner);
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
    transaction_manager
        .enqueue(
            vec![receive_object_transaction0.clone()],
            &state.epoch_store_for_testing(),
        )
        .unwrap();
    sleep(Duration::from_secs(1)).await;
    assert!(rx_ready_certificates.try_recv().is_err());
    assert_eq!(transaction_manager.inflight_queue_len(), 1);

    // TM should output no transaction yet since waiting on receiving object.
    transaction_manager
        .enqueue(
            vec![receive_object_transaction1.clone()],
            &state.epoch_store_for_testing(),
        )
        .unwrap();
    sleep(Duration::from_secs(1)).await;
    assert!(rx_ready_certificates.try_recv().is_err());
    assert_eq!(transaction_manager.inflight_queue_len(), 2);

    // Duplicate enqueue of receiving object is allowed.
    transaction_manager
        .enqueue(
            vec![receive_object_transaction0.clone()],
            &state.epoch_store_for_testing(),
        )
        .unwrap();
    sleep(Duration::from_secs(1)).await;
    assert!(rx_ready_certificates.try_recv().is_err());
    assert_eq!(transaction_manager.inflight_queue_len(), 2);

    // Notify TM that the receiving object 0 is available.
    transaction_manager.objects_available(
        get_input_keys(&vec![receiving_object_new0.clone()]),
        &state.epoch_store_for_testing(),
    );

    // TM should output the transaction eventually now that the receiving object has become
    // available.
    rx_ready_certificates.recv().await.unwrap();

    // Notify TM that the receiving object 0 is available.
    transaction_manager.objects_available(
        get_input_keys(&vec![receiving_object_new1.clone()]),
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
    let receiving_object_new0 = Object::with_id_owner_version_for_testing(obj_id, 0.into(), owner);
    let receiving_object_new1 = Object::with_id_owner_version_for_testing(obj_id, 1.into(), owner);
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
    transaction_manager
        .enqueue(
            vec![receive_object_transaction0.clone()],
            &state.epoch_store_for_testing(),
        )
        .unwrap();
    sleep(Duration::from_secs(1)).await;
    assert!(rx_ready_certificates.try_recv().is_err());
    assert_eq!(transaction_manager.inflight_queue_len(), 1);

    // TM should output no transaction yet since waiting on receiving object.
    transaction_manager
        .enqueue(
            vec![receive_object_transaction1.clone()],
            &state.epoch_store_for_testing(),
        )
        .unwrap();
    sleep(Duration::from_secs(1)).await;
    assert!(rx_ready_certificates.try_recv().is_err());
    assert_eq!(transaction_manager.inflight_queue_len(), 2);

    // Different transaciton with a duplicate receiving object reference is allowed.
    // Both transaction's will be outputted once the receiving object is available.
    transaction_manager
        .enqueue(
            vec![receive_object_transaction01.clone()],
            &state.epoch_store_for_testing(),
        )
        .unwrap();
    sleep(Duration::from_secs(1)).await;
    assert!(rx_ready_certificates.try_recv().is_err());
    assert_eq!(transaction_manager.inflight_queue_len(), 3);

    // Notify TM that the receiving object 0 is available.
    transaction_manager.objects_available(
        get_input_keys(&vec![receiving_object_new0.clone()]),
        &state.epoch_store_for_testing(),
    );

    // TM should output both transactions dependening on the receiving object now that the
    // transaction's receiving object has become available.
    rx_ready_certificates.recv().await.unwrap();

    rx_ready_certificates.recv().await.unwrap();

    // Only two transactions that were dependent on the receiving object should be output.
    assert!(rx_ready_certificates.try_recv().is_err());

    // Enqueue a transaction with a receiving object that is available at the time it is enqueued.
    // This should be immediately available.
    transaction_manager
        .enqueue(vec![tx1.clone()], &state.epoch_store_for_testing())
        .unwrap();
    sleep(Duration::from_secs(1)).await;
    rx_ready_certificates.recv().await.unwrap();

    // Notify TM that the receiving object 0 is available.
    transaction_manager.objects_available(
        get_input_keys(&vec![receiving_object_new1.clone()]),
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
    let receiving_object =
        Object::with_id_owner_version_for_testing(ObjectID::random(), 10.into(), owner);
    gas_objects.push(receiving_object.clone());
    let state = init_state_with_objects(gas_objects.clone()).await;

    // Create a new transaction manager instead of reusing the authority's, to examine
    // transaction_manager output from rx_ready_certificates.
    let (transaction_manager, mut rx_ready_certificates) = make_transaction_manager(&state);
    // TM should output no transaction.
    assert!(rx_ready_certificates.try_recv().is_err());
    // TM should be empty at the beginning.
    transaction_manager.check_empty_for_testing();

    let receiving_object_new0 =
        Object::with_id_owner_version_for_testing(receiving_object.id(), 0.into(), owner);
    let receiving_object_new1 =
        Object::with_id_owner_version_for_testing(receiving_object.id(), 1.into(), owner);
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
    transaction_manager
        .enqueue(
            vec![receive_object_transaction0.clone()],
            &state.epoch_store_for_testing(),
        )
        .unwrap();
    transaction_manager
        .enqueue(
            vec![receive_object_transaction01.clone()],
            &state.epoch_store_for_testing(),
        )
        .unwrap();
    transaction_manager
        .enqueue(
            vec![receive_object_transaction1.clone()],
            &state.epoch_store_for_testing(),
        )
        .unwrap();
    sleep(Duration::from_secs(1)).await;
    rx_ready_certificates.recv().await.unwrap();
    rx_ready_certificates.recv().await.unwrap();
    rx_ready_certificates.recv().await.unwrap();
    assert!(rx_ready_certificates.try_recv().is_err());
}
