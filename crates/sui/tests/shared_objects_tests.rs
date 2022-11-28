// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use sui_core::authority_client::AuthorityAPI;
use sui_types::messages::{
    CallArg, ExecutionStatus, ObjectArg, ObjectInfoRequest, ObjectInfoRequestKind,
};
use sui_types::object::OBJECT_START_VERSION;
use test_utils::authority::get_client;
use test_utils::transaction::{
    publish_counter_package, submit_shared_object_transaction, submit_single_owner_transaction,
};
use test_utils::{
    authority::{spawn_test_authorities, test_authority_configs},
    messages::{move_transaction, test_shared_object_transactions},
    objects::{test_gas_objects, test_shared_object},
};

use sui_macros::sim_test;

/// Send a simple shared object transaction to Sui and ensures the client gets back a response.
#[sim_test]
async fn shared_object_transaction() {
    let mut objects = test_gas_objects();
    objects.push(test_shared_object());

    // Get the authority configs and spawn them. Note that it is important to not drop
    // the handles (or the authorities will stop).
    let configs = test_authority_configs();
    let _handles = spawn_test_authorities(objects, &configs).await;

    // Make a test shared object certificate.
    let transaction = test_shared_object_transactions().pop().unwrap();

    // Submit the transaction. Note that this transaction is random and we do not expect
    // it to be successfully executed by the Move execution engine.
    let _effects = submit_shared_object_transaction(transaction, configs.validator_set())
        .await
        .unwrap();
}

/// Same as `shared_object_transaction` but every authorities submit the transaction.
#[sim_test]
async fn many_shared_object_transactions() {
    let mut objects = test_gas_objects();
    objects.push(test_shared_object());

    // Get the authority configs and spawn them. Note that it is important to not drop
    // the handles (or the authorities will stop).
    let configs = test_authority_configs();
    let _handles = spawn_test_authorities(objects, &configs).await;

    // Make a test shared object certificate.
    let transaction = test_shared_object_transactions().pop().unwrap();

    // Submit the transaction. Note that this transaction is random and we do not expect
    // it to be successfully executed by the Move execution engine.
    let _effects = submit_shared_object_transaction(transaction, configs.validator_set())
        .await
        .unwrap();
}

/// End-to-end shared transaction test for a Sui validator. It does not test the client, wallet,
/// or gateway but tests the end-to-end flow from Sui to consensus.
#[sim_test]
async fn call_shared_object_contract() {
    let mut gas_objects = test_gas_objects();

    // Get the authority configs and spawn them. Note that it is important to not drop
    // the handles (or the authorities will stop).
    let configs = test_authority_configs();
    let _handles = spawn_test_authorities(gas_objects.clone(), &configs).await;

    // Publish the move package to all authorities and get the new package ref.
    let package_ref =
        publish_counter_package(gas_objects.pop().unwrap(), configs.validator_set()).await;

    // Make a transaction to create a counter.
    let transaction = move_transaction(
        gas_objects.pop().unwrap(),
        "counter",
        "create",
        package_ref,
        /* arguments */ Vec::default(),
    );
    let effects = submit_single_owner_transaction(transaction, configs.validator_set()).await;
    assert!(matches!(effects.status, ExecutionStatus::Success { .. }));
    let ((counter_id, counter_initial_shared_version, _), _) = effects.created[0];
    let counter_object_arg = ObjectArg::SharedObject {
        id: counter_id,
        initial_shared_version: counter_initial_shared_version,
    };

    // Ensure the value of the counter is `0`.
    let transaction = move_transaction(
        gas_objects.pop().unwrap(),
        "counter",
        "assert_value",
        package_ref,
        vec![
            CallArg::Object(counter_object_arg),
            CallArg::Pure(0u64.to_le_bytes().to_vec()),
        ],
    );
    let effects = submit_shared_object_transaction(transaction, configs.validator_set())
        .await
        .unwrap();
    assert!(matches!(effects.status, ExecutionStatus::Success { .. }));

    // Make a transaction to increment the counter.
    let transaction = move_transaction(
        gas_objects.pop().unwrap(),
        "counter",
        "increment",
        package_ref,
        vec![CallArg::Object(counter_object_arg)],
    );
    let effects = submit_shared_object_transaction(transaction, configs.validator_set())
        .await
        .unwrap();
    assert!(matches!(effects.status, ExecutionStatus::Success { .. }));

    // Ensure the value of the counter is `1`.
    let transaction = move_transaction(
        gas_objects.pop().unwrap(),
        "counter",
        "assert_value",
        package_ref,
        vec![
            CallArg::Object(counter_object_arg),
            CallArg::Pure(1u64.to_le_bytes().to_vec()),
        ],
    );
    let effects = submit_shared_object_transaction(transaction, configs.validator_set())
        .await
        .unwrap();
    assert!(matches!(effects.status, ExecutionStatus::Success { .. }));
}

/// Same test as `call_shared_object_contract` but the clients submits many times the same
/// transaction (one copy per authority).
#[sim_test]
async fn shared_object_flood() {
    telemetry_subscribers::init_for_testing();
    let mut gas_objects = test_gas_objects();

    // Get the authority configs and spawn them. Note that it is important to not drop
    // the handles (or the authorities will stop).
    let configs = test_authority_configs();
    let _handles = spawn_test_authorities(gas_objects.clone(), &configs).await;

    // Publish the move package to all authorities and get the new package ref.
    let package_ref =
        publish_counter_package(gas_objects.pop().unwrap(), configs.validator_set()).await;

    // Make a transaction to create a counter.
    let transaction = move_transaction(
        gas_objects.pop().unwrap(),
        "counter",
        "create",
        package_ref,
        /* arguments */ Vec::default(),
    );
    let effects = submit_single_owner_transaction(transaction, configs.validator_set()).await;
    assert!(matches!(effects.status, ExecutionStatus::Success { .. }));
    let ((counter_id, counter_initial_shared_version, _), _) = effects.created[0];
    let counter_object_arg = ObjectArg::SharedObject {
        id: counter_id,
        initial_shared_version: counter_initial_shared_version,
    };

    // Ensure the value of the counter is `0`.
    let transaction = move_transaction(
        gas_objects.pop().unwrap(),
        "counter",
        "assert_value",
        package_ref,
        vec![
            CallArg::Object(counter_object_arg),
            CallArg::Pure(0u64.to_le_bytes().to_vec()),
        ],
    );
    let effects = submit_shared_object_transaction(transaction, configs.validator_set())
        .await
        .unwrap();
    assert!(matches!(effects.status, ExecutionStatus::Success { .. }));

    // Make a transaction to increment the counter.
    let transaction = move_transaction(
        gas_objects.pop().unwrap(),
        "counter",
        "increment",
        package_ref,
        vec![CallArg::Object(counter_object_arg)],
    );
    let effects = submit_shared_object_transaction(transaction, configs.validator_set())
        .await
        .unwrap();
    assert!(matches!(effects.status, ExecutionStatus::Success { .. }));

    // Ensure the value of the counter is `1`.
    let transaction = move_transaction(
        gas_objects.pop().unwrap(),
        "counter",
        "assert_value",
        package_ref,
        vec![
            CallArg::Object(counter_object_arg),
            CallArg::Pure(1u64.to_le_bytes().to_vec()),
        ],
    );
    let effects = submit_shared_object_transaction(transaction, configs.validator_set())
        .await
        .unwrap();
    assert!(matches!(effects.status, ExecutionStatus::Success { .. }));
}

#[sim_test]
async fn shared_object_sync() {
    telemetry_subscribers::init_for_testing();
    let mut gas_objects = test_gas_objects();

    // Get the authority configs and spawn them. Note that it is important to not drop
    // the handles (or the authorities will stop).
    let configs = test_authority_configs();
    let _handles = spawn_test_authorities(gas_objects.clone(), &configs).await;

    // Publish the move package to all authorities and get the new package ref.
    let package_ref =
        publish_counter_package(gas_objects.pop().unwrap(), configs.validator_set()).await;

    // Send a transaction to create a counter, but only to one authority.
    let create_counter_transaction = move_transaction(
        gas_objects.pop().unwrap(),
        "counter",
        "create",
        package_ref,
        /* arguments */ Vec::default(),
    );
    let effects = submit_single_owner_transaction(
        create_counter_transaction.clone(),
        // this is a bit fragile (see consensus adapter):
        // 2022-11-11 huitseeker: validator #2 is one of the two validators that submit this TX.
        // 2022-11-25 amnn: For reasons completely unrelated to this test, validator #2 is no
        //     longer one of the validators that submits this TX, but validator #1 is.
        &configs.validator_set()[0..1],
    )
    .await;
    assert!(matches!(effects.status, ExecutionStatus::Success { .. }));
    let ((counter_id, counter_initial_shared_version, _), _) = effects.created[0];
    let counter_object_arg = ObjectArg::SharedObject {
        id: counter_id,
        initial_shared_version: counter_initial_shared_version,
    };

    // Check that the counter object only exist in one validator, but not the rest.
    // count the number of validators that have the counter object.
    let mut provisioned_authorities = 0;
    for config in configs.validator_set() {
        provisioned_authorities += get_client(config)
            .handle_object_info_request(ObjectInfoRequest {
                object_id: counter_id,
                request_kind: ObjectInfoRequestKind::LatestObjectInfo(None),
            })
            .await
            .unwrap()
            .object()
            .map(|_x| 1)
            .unwrap_or_default();
    }
    assert_eq!(1, provisioned_authorities);

    // Make a transaction to increment the counter.
    let increment_counter_transaction = move_transaction(
        gas_objects.pop().unwrap(),
        "counter",
        "increment",
        package_ref,
        vec![CallArg::Object(counter_object_arg)],
    );

    // Let's submit the transaction to just one authority (including only one up-to-date).
    let effects = submit_shared_object_transaction(
        increment_counter_transaction.clone(),
        &configs.validator_set()[1..4],
    )
    .await
    .unwrap();
    assert!(matches!(effects.status, ExecutionStatus::Success { .. }));

    // Submit transactions to out-of-date authorities.
    // It will succeed because we share owned object certificates through narwhal
    let effects =
        submit_shared_object_transaction(increment_counter_transaction, configs.validator_set())
            .await
            .unwrap();
    assert!(matches!(effects.status, ExecutionStatus::Success { .. }));
}

/// Send a simple shared object transaction to Sui and ensures the client gets back a response.
#[sim_test]
async fn replay_shared_object_transaction() {
    let mut gas_objects = test_gas_objects();

    // Get the authority configs and spawn them. Note that it is important to not drop
    // the handles (or the authorities will stop).
    let configs = test_authority_configs();
    let _handles = spawn_test_authorities(gas_objects.clone(), &configs).await;

    // Publish the move package to all authorities and get the new package ref.
    let package_ref =
        publish_counter_package(gas_objects.pop().unwrap(), configs.validator_set()).await;

    // Send a transaction to create a counter (only to one authority) -- twice.
    let create_counter_transaction = move_transaction(
        gas_objects.pop().unwrap(),
        "counter",
        "create",
        package_ref,
        /* arguments */ Vec::default(),
    );
    for _ in 0..2 {
        let effects = submit_single_owner_transaction(
            create_counter_transaction.clone(),
            configs.validator_set(),
        )
        .await;
        assert!(matches!(effects.status, ExecutionStatus::Success { .. }));

        // Ensure the sequence number of the shared object did not change.
        let ((_, seq, _), _) = effects.created[0];
        assert_eq!(seq, OBJECT_START_VERSION);
    }
}
