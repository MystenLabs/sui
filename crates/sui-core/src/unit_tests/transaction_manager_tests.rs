// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{time::Duration, vec};

use sui_types::{
    base_types::ObjectID,
    crypto::deterministic_random_account_key,
    messages::{CallArg, ObjectArg, VerifiedExecutableTransaction, TEST_ONLY_GAS_UNIT_FOR_GENERIC},
    object::Object,
    SUI_FRAMEWORK_OBJECT_ID,
};
use test_utils::messages::move_transaction;
use tokio::{
    sync::mpsc::{unbounded_channel, UnboundedReceiver},
    time::sleep,
};

use crate::{
    authority::{
        authority_store::InputKey, authority_tests::init_state_with_objects, AuthorityState,
    },
    transaction_manager::TransactionManager,
};

#[allow(clippy::disallowed_methods)] // allow unbounded_channel()
fn make_transaction_manager(
    state: &AuthorityState,
) -> (
    TransactionManager,
    UnboundedReceiver<VerifiedExecutableTransaction>,
) {
    // Create a new transaction manager instead of reusing the authority's, to examine
    // transaction_manager output from rx_ready_certificates.
    let (tx_ready_certificates, rx_ready_certificates) = unbounded_channel();
    let transaction_manager = TransactionManager::new(
        state.database.clone(),
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
    let transaction = move_transaction(
        gas_object,
        "counter",
        "assert_value",
        SUI_FRAMEWORK_OBJECT_ID,
        input,
        rgp,
        rgp * TEST_ONLY_GAS_UNIT_FOR_GENERIC,
    );
    VerifiedExecutableTransaction::new_system(transaction, 0)
}

fn get_input_keys(objects: &[Object]) -> Vec<InputKey> {
    objects
        .iter()
        .map(|object| InputKey(object.id(), Some(object.version())))
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

    // Enqueue empty vec should not crash.
    transaction_manager
        .enqueue(vec![], &state.epoch_store_for_testing())
        .unwrap();
    // TM should output no transaction.
    assert!(rx_ready_certificates.try_recv().is_err());

    // Enqueue a transaction with existing gas object, empty input.
    let transaction = make_transaction(gas_objects[0].clone(), vec![]);
    transaction_manager
        .enqueue(vec![transaction], &state.epoch_store_for_testing())
        .unwrap();
    // TM should output the transaction eventually.
    rx_ready_certificates.recv().await.unwrap();

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

    // Duplicated enqueue is allowed.
    transaction_manager
        .enqueue(vec![transaction.clone()], &state.epoch_store_for_testing())
        .unwrap();
    sleep(Duration::from_secs(1)).await;
    assert!(rx_ready_certificates.try_recv().is_err());

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
    transaction_manager
        .certificate_executed(transaction.digest(), &state.epoch_store_for_testing());
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

    // Notify TM about availability of the shared object.
    transaction_manager.objects_available(
        vec![InputKey(shared_object.id(), Some(shared_version))],
        &state.epoch_store_for_testing(),
    );

    // TM should output the 2 read-only transactions eventually.
    let tx_0 = rx_ready_certificates.recv().await.unwrap();
    let tx_1 = rx_ready_certificates.recv().await.unwrap();
    let mut want_digests = vec![transaction_read_0.digest(), transaction_read_1.digest()];
    want_digests.sort();
    let mut got_digests = vec![tx_0.digest(), tx_1.digest()];
    got_digests.sort();
    assert_eq!(want_digests, got_digests);

    // TM should not output default-lock transaction yet.
    sleep(Duration::from_secs(1)).await;
    assert!(rx_ready_certificates.try_recv().is_err());

    // Notify TM about read-only transaction commit
    transaction_manager.certificate_executed(tx_0.digest(), &state.epoch_store_for_testing());
    transaction_manager.certificate_executed(tx_1.digest(), &state.epoch_store_for_testing());

    // TM should output the default-lock transaction eventually.
    let tx_2 = rx_ready_certificates.recv().await.unwrap();
    assert_eq!(tx_2.digest(), transaction_default.digest());
}
