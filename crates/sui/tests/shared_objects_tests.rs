// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use futures::{stream, StreamExt};
use std::time::{Duration, SystemTime};
use sui_core::authority_client::AuthorityAPI;
use sui_core::consensus_adapter::position_submit_certificate;
use sui_types::messages::{
    CallArg, EntryArgumentError, EntryArgumentErrorKind, ExecutionFailureStatus, ExecutionStatus,
    ObjectArg, ObjectInfoRequest, TransactionEffectsAPI,
};
use test_utils::authority::get_client;
use test_utils::transaction::{
    publish_counter_package, submit_shared_object_transaction, submit_single_owner_transaction,
};
use test_utils::{
    authority::{spawn_test_authorities, test_authority_configs},
    messages::{move_transaction, test_shared_object_transactions},
};

use sui_macros::sim_test;
use sui_types::event::Event;
use sui_types::object::{generate_test_gas_objects, Object};
use sui_types::{SUI_CLOCK_OBJECT_ID, SUI_CLOCK_OBJECT_SHARED_VERSION};

/// Send a simple shared object transaction to Sui and ensures the client gets back a response.
#[sim_test]
async fn shared_object_transaction() {
    let mut objects = generate_test_gas_objects();
    objects.push(Object::shared_for_testing());

    // Get the authority configs and spawn them. Note that it is important to not drop
    // the handles (or the authorities will stop).
    let configs = test_authority_configs();
    let _handles = spawn_test_authorities(objects, &configs).await;

    // Make a test shared object certificate.
    let transaction = test_shared_object_transactions().pop().unwrap();

    // Submit the transaction. Note that this transaction is random and we do not expect
    // it to be successfully executed by the Move execution engine.
    let _effects = submit_shared_object_transaction(transaction, &configs.validator_set())
        .await
        .unwrap();
}

/// Same as `shared_object_transaction` but every authorities submit the transaction.
#[sim_test]
async fn many_shared_object_transactions() {
    let mut objects = generate_test_gas_objects();
    objects.push(Object::shared_for_testing());

    // Get the authority configs and spawn them. Note that it is important to not drop
    // the handles (or the authorities will stop).
    let configs = test_authority_configs();
    let _handles = spawn_test_authorities(objects, &configs).await;

    // Make a test shared object certificate.
    let transaction = test_shared_object_transactions().pop().unwrap();

    // Submit the transaction. Note that this transaction is random and we do not expect
    // it to be successfully executed by the Move execution engine.
    let _effects = submit_shared_object_transaction(transaction, &configs.validator_set())
        .await
        .unwrap();
}

/// End-to-end shared transaction test for a Sui validator. It does not test the client or wallet,
/// but tests the end-to-end flow from Sui to consensus.
#[sim_test]
async fn call_shared_object_contract() {
    let mut gas_objects = generate_test_gas_objects();

    // Get the authority configs and spawn them. Note that it is important to not drop
    // the handles (or the authorities will stop).
    let configs = test_authority_configs();
    let _handles = spawn_test_authorities(gas_objects.clone(), &configs).await;

    // Publish the move package to all authorities and get its package ID.
    let package_id = publish_counter_package(gas_objects.pop().unwrap(), &configs.validator_set())
        .await
        .0;

    // Make a transaction to create a counter.
    let transaction = move_transaction(
        gas_objects.pop().unwrap(),
        "counter",
        "create",
        package_id,
        /* arguments */ Vec::default(),
    );
    let (effects, _) = submit_single_owner_transaction(transaction, &configs.validator_set()).await;
    assert!(matches!(effects.status(), ExecutionStatus::Success { .. }));
    let counter_creation_transaction = *effects.transaction_digest();
    let ((counter_id, counter_initial_shared_version, _), _) = effects.created()[0];
    let counter_object_arg = ObjectArg::SharedObject {
        id: counter_id,
        initial_shared_version: counter_initial_shared_version,
        mutable: true,
    };
    let counter_object_arg_imm = ObjectArg::SharedObject {
        id: counter_id,
        initial_shared_version: counter_initial_shared_version,
        mutable: false,
    };

    // Send two read only transactions
    for _ in 0..2 {
        // Ensure the value of the counter is `0`.
        let transaction = move_transaction(
            gas_objects.pop().unwrap(),
            "counter",
            "assert_value",
            package_id,
            vec![
                CallArg::Object(counter_object_arg_imm),
                CallArg::Pure(0u64.to_le_bytes().to_vec()),
            ],
        );
        let (effects, _) = submit_shared_object_transaction(transaction, &configs.validator_set())
            .await
            .unwrap();
        assert!(matches!(effects.status(), ExecutionStatus::Success { .. }));
        // Only gas object transaction and counter creation are dependencies
        // Note that this assert would fail for second transaction
        // if they send counter_object_arg instead of counter_object_arg_imm
        assert_eq!(effects.dependencies().len(), 2);
        assert!(effects
            .dependencies()
            .contains(&counter_creation_transaction));
    }

    // Make a transaction to increment the counter.
    let transaction = move_transaction(
        gas_objects.pop().unwrap(),
        "counter",
        "increment",
        package_id,
        vec![CallArg::Object(counter_object_arg)],
    );
    let (effects, _) = submit_shared_object_transaction(transaction, &configs.validator_set())
        .await
        .unwrap();
    let increment_transaction = *effects.transaction_digest();
    assert!(matches!(effects.status(), ExecutionStatus::Success { .. }));
    // Again - only gas object transaction and counter creation are dependencies
    // Previously executed assert_value transaction(s) are not a dependency because they took immutable reference to shared object
    assert_eq!(effects.dependencies().len(), 2);
    assert!(effects
        .dependencies()
        .contains(&counter_creation_transaction));

    // assert_value can take both mutable and immutable references
    // it is allowed to pass mutable shared object arg to move call taking immutable reference
    let mut assert_value_mut_transaction = None;
    for imm in [true, false] {
        // Ensure the value of the counter is `1`.
        let transaction = move_transaction(
            gas_objects.pop().unwrap(),
            "counter",
            "assert_value",
            package_id,
            vec![
                CallArg::Object(if imm {
                    counter_object_arg_imm
                } else {
                    counter_object_arg
                }),
                CallArg::Pure(1u64.to_le_bytes().to_vec()),
            ],
        );
        let (effects, _) = submit_shared_object_transaction(transaction, &configs.validator_set())
            .await
            .unwrap();
        assert!(matches!(effects.status(), ExecutionStatus::Success { .. }));
        // Gas object transaction and increment transaction are dependencies
        assert_eq!(effects.dependencies().len(), 2);
        assert!(effects.dependencies().contains(&increment_transaction));
        assert_value_mut_transaction = Some(*effects.transaction_digest());
    }

    let assert_value_mut_transaction = assert_value_mut_transaction.unwrap();

    // And last check - attempt to send increment transaction with immutable reference
    let transaction = move_transaction(
        gas_objects.pop().unwrap(),
        "counter",
        "increment",
        package_id,
        vec![CallArg::Object(counter_object_arg_imm)],
    );
    let (effects, _) = submit_shared_object_transaction(transaction, &configs.validator_set())
        .await
        .unwrap();
    // Transaction fails
    assert!(matches!(
        effects.status(),
        ExecutionStatus::Failure {
            error: ExecutionFailureStatus::EntryArgumentError(EntryArgumentError {
                kind: EntryArgumentErrorKind::ObjectMutabilityMismatch,
                ..
            }),
            ..
        }
    ));
    assert_eq!(effects.dependencies().len(), 2);
    assert!(effects
        .dependencies()
        .contains(&assert_value_mut_transaction));
}

#[sim_test]
async fn access_clock_object_test() {
    let mut gas_objects = generate_test_gas_objects();

    // Get the authority configs and spawn them. Note that it is important to not drop
    // the handles (or the authorities will stop).
    let configs = test_authority_configs();
    let handles = spawn_test_authorities(gas_objects.clone(), &configs).await;

    // Publish the move package to all authorities and get its package ID.
    let package_id = publish_counter_package(gas_objects.pop().unwrap(), &configs.validator_set())
        .await
        .0;

    let clock_object_arg = ObjectArg::SharedObject {
        id: SUI_CLOCK_OBJECT_ID,
        initial_shared_version: SUI_CLOCK_OBJECT_SHARED_VERSION,
        mutable: false,
    };

    let transaction = move_transaction(
        gas_objects.pop().unwrap(),
        "clock",
        "get_time",
        package_id,
        vec![CallArg::Object(clock_object_arg)],
    );
    let digest = *transaction.digest();
    let start = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap();
    let (effects, events) = submit_shared_object_transaction(transaction, &configs.validator_set())
        .await
        .unwrap();
    let finish = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap();
    assert!(matches!(effects.status(), ExecutionStatus::Success { .. }));

    assert_eq!(2, events.data.len());
    let event = events.data.get(1).unwrap();
    let Event::MoveEvent { contents, .. } = event else { panic!("Expected move event, got {:?}", event) };

    use serde::{Deserialize, Serialize};
    #[derive(Serialize, Deserialize)]
    struct TimeEvent {
        timestamp_ms: u64,
    }
    let event = bcs::from_bytes::<TimeEvent>(contents).unwrap();

    // Some sanity checks on the timestamp that we got
    assert!(event.timestamp_ms >= start.as_millis() as u64);
    assert!(event.timestamp_ms <= finish.as_millis() as u64);

    let mut attempt = 0;
    #[allow(clippy::never_loop)] // seem to be a bug in clippy with let else statement
    loop {
        let checkpoint = handles
            .get(0)
            .unwrap()
            .with_async(|node| async { node.state().get_transaction_checkpoint(&digest).unwrap() })
            .await;
        let Some(checkpoint) = checkpoint else {
            attempt += 1;
            if attempt > 30 {
                panic!("Could not get transaction checkpoint");
            }
            tokio::time::sleep(Duration::from_secs(1)).await;
            continue;
        };

        // Timestamp that we have read in a smart contract
        // should match timestamp of the checkpoint where transaction is included
        assert_eq!(checkpoint.summary.timestamp_ms, event.timestamp_ms);
        break;
    }
}

/// Same test as `call_shared_object_contract` but the clients submits many times the same
/// transaction (one copy per authority).
#[sim_test]
async fn shared_object_flood() {
    telemetry_subscribers::init_for_testing();
    let mut gas_objects = generate_test_gas_objects();

    // Get the authority configs and spawn them. Note that it is important to not drop
    // the handles (or the authorities will stop).
    let configs = test_authority_configs();
    let _handles = spawn_test_authorities(gas_objects.clone(), &configs).await;

    // Publish the move package to all authorities and get its package ID.
    let package_id = publish_counter_package(gas_objects.pop().unwrap(), &configs.validator_set())
        .await
        .0;

    // Make a transaction to create a counter.
    let transaction = move_transaction(
        gas_objects.pop().unwrap(),
        "counter",
        "create",
        package_id,
        /* arguments */ Vec::default(),
    );
    let (effects, _) = submit_single_owner_transaction(transaction, &configs.validator_set()).await;
    assert!(matches!(effects.status(), ExecutionStatus::Success { .. }));
    let ((counter_id, counter_initial_shared_version, _), _) = effects.created()[0];
    let counter_object_arg = ObjectArg::SharedObject {
        id: counter_id,
        initial_shared_version: counter_initial_shared_version,
        mutable: true,
    };

    // Ensure the value of the counter is `0`.
    let transaction = move_transaction(
        gas_objects.pop().unwrap(),
        "counter",
        "assert_value",
        package_id,
        vec![
            CallArg::Object(counter_object_arg),
            CallArg::Pure(0u64.to_le_bytes().to_vec()),
        ],
    );
    let (effects, _) = submit_shared_object_transaction(transaction, &configs.validator_set())
        .await
        .unwrap();
    assert!(matches!(effects.status(), ExecutionStatus::Success { .. }));

    // Make a transaction to increment the counter.
    let transaction = move_transaction(
        gas_objects.pop().unwrap(),
        "counter",
        "increment",
        package_id,
        vec![CallArg::Object(counter_object_arg)],
    );
    let (effects, _) = submit_shared_object_transaction(transaction, &configs.validator_set())
        .await
        .unwrap();
    assert!(matches!(effects.status(), ExecutionStatus::Success { .. }));

    // Ensure the value of the counter is `1`.
    let transaction = move_transaction(
        gas_objects.pop().unwrap(),
        "counter",
        "assert_value",
        package_id,
        vec![
            CallArg::Object(counter_object_arg),
            CallArg::Pure(1u64.to_le_bytes().to_vec()),
        ],
    );
    let (effects, _) = submit_shared_object_transaction(transaction, &configs.validator_set())
        .await
        .unwrap();
    assert!(matches!(effects.status(), ExecutionStatus::Success { .. }));
}

#[sim_test]
async fn shared_object_sync() {
    telemetry_subscribers::init_for_testing();
    let mut gas_objects = generate_test_gas_objects();

    // Get the authority configs and spawn them. Note that it is important to not drop
    // the handles (or the authorities will stop).
    let configs = test_authority_configs();
    let _handles = spawn_test_authorities(gas_objects.clone(), &configs).await;

    // Publish the move package to all authorities and get its package ID.
    let package_id = publish_counter_package(gas_objects.pop().unwrap(), &configs.validator_set())
        .await
        .0;

    // Send a transaction to create a counter, to all but one authority.
    let create_counter_transaction = move_transaction(
        gas_objects.pop().unwrap(),
        "counter",
        "create",
        package_id,
        /* arguments */ Vec::default(),
    );

    let (slow_validators, fast_validators): (Vec<_>, Vec<_>) =
        configs.validator_set().iter().cloned().partition(|info| {
            position_submit_certificate(
                &configs.committee(),
                &info.protocol_key(),
                create_counter_transaction.digest(),
            ) > 0
        });

    let (effects, _) = submit_single_owner_transaction(
        create_counter_transaction.clone(),
        //&configs.validator_set()[1..],
        &slow_validators,
    )
    .await;
    assert!(matches!(effects.status(), ExecutionStatus::Success { .. }));
    let ((counter_id, counter_initial_shared_version, _), _) = effects.created()[0];
    let counter_object_arg = ObjectArg::SharedObject {
        id: counter_id,
        initial_shared_version: counter_initial_shared_version,
        mutable: true,
    };

    // Check that the counter object exists in at least one of the validators the transaction was
    // sent to.
    let validator_set = configs.validator_set();
    let has_counter = stream::iter(&validator_set[1..]).any(|config| async move {
        get_client(config)
            .handle_object_info_request(ObjectInfoRequest::latest_object_info_request(
                counter_id, None,
            ))
            .await
            .is_ok()
    });

    assert!(has_counter.await);

    // Check that the validator that wasn't sent the transaction is unaware of the counter object
    assert!(get_client(&fast_validators[0])
        .handle_object_info_request(ObjectInfoRequest::latest_object_info_request(
            counter_id, None,
        ))
        .await
        .is_err());

    // Make a transaction to increment the counter.
    let increment_counter_transaction = move_transaction(
        gas_objects.pop().unwrap(),
        "counter",
        "increment",
        package_id,
        vec![CallArg::Object(counter_object_arg)],
    );

    // Let's submit the transaction to the original set of validators.
    let (effects, _) = submit_shared_object_transaction(
        increment_counter_transaction.clone(),
        &configs.validator_set()[1..],
    )
    .await
    .unwrap();
    assert!(matches!(effects.status(), ExecutionStatus::Success { .. }));

    // Submit transactions to the out-of-date authority.
    // It will succeed because we share owned object certificates through narwhal
    let (effects, _) = submit_shared_object_transaction(
        increment_counter_transaction,
        &configs.validator_set()[0..1],
    )
    .await
    .unwrap();
    assert!(matches!(effects.status(), ExecutionStatus::Success { .. }));
}

/// Send a simple shared object transaction to Sui and ensures the client gets back a response.
#[sim_test]
async fn replay_shared_object_transaction() {
    let mut gas_objects = generate_test_gas_objects();

    // Get the authority configs and spawn them. Note that it is important to not drop
    // the handles (or the authorities will stop).
    let configs = test_authority_configs();
    let _handles = spawn_test_authorities(gas_objects.clone(), &configs).await;

    // Publish the move package to all authorities and get its package ID
    let package_id = publish_counter_package(gas_objects.pop().unwrap(), &configs.validator_set())
        .await
        .0;

    // Send a transaction to create a counter (only to one authority) -- twice.
    let create_counter_transaction = move_transaction(
        gas_objects.pop().unwrap(),
        "counter",
        "create",
        package_id,
        /* arguments */ Vec::default(),
    );

    let mut version = None;
    for _ in 0..2 {
        let (effects, _) = submit_single_owner_transaction(
            create_counter_transaction.clone(),
            &configs.validator_set(),
        )
        .await;
        assert!(matches!(effects.status(), ExecutionStatus::Success { .. }));

        // Ensure the sequence number of the shared object did not change.
        let ((_, curr, _), _) = effects.created()[0];
        if let Some(prev) = version {
            assert_eq!(
                prev, curr,
                "SequenceNumber of shared object did not change."
            );
        }

        version = Some(curr);
    }
}
