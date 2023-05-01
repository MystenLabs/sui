// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use futures::{stream, StreamExt};
use std::time::{Duration, SystemTime};
use sui_core::authority_client::AuthorityAPI;
use sui_core::consensus_adapter::position_submit_certificate;
use sui_types::messages::{CallArg, ObjectArg, ObjectInfoRequest, TEST_ONLY_GAS_UNIT_FOR_GENERIC};
use test_utils::authority::get_client;
use test_utils::transaction::{
    publish_counter_package, submit_shared_object_transaction, submit_single_owner_transaction,
};
use test_utils::{
    authority::{spawn_test_authorities, test_authority_configs_with_objects},
    messages::{move_transaction, test_shared_object_transactions},
};

use sui_macros::sim_test;
use sui_types::effects::TransactionEffectsAPI;
use sui_types::event::Event;
use sui_types::execution_status::{CommandArgumentError, ExecutionFailureStatus, ExecutionStatus};
use sui_types::object::{generate_test_gas_objects, Object};
use sui_types::{SUI_CLOCK_OBJECT_ID, SUI_CLOCK_OBJECT_SHARED_VERSION};

/// Send a simple shared object transaction to Sui and ensures the client gets back a response.
#[sim_test]
async fn shared_object_transaction() {
    let gas_objects = generate_test_gas_objects();
    let mut objects = vec![Object::shared_for_testing()];
    objects.extend(gas_objects.into_iter());

    // Get the authority configs and spawn them. Note that it is important to not drop
    // the handles (or the authorities will stop).
    let (configs, objects) = test_authority_configs_with_objects(objects);
    let rgp = configs.genesis.reference_gas_price();
    let mut objects = objects.into_iter();
    let shared_object = objects.next().unwrap();
    let gas_objects: Vec<_> = objects.collect();
    let _handles = spawn_test_authorities(&configs).await;

    // Make a test shared object certificate.
    let transaction = test_shared_object_transactions(Some(shared_object), Some(gas_objects), rgp)
        .pop()
        .unwrap();

    // Submit the transaction. Note that this transaction is random and we do not expect
    // it to be successfully executed by the Move execution engine.
    let _effects = submit_shared_object_transaction(transaction, &configs.net_addresses())
        .await
        .unwrap();
}

/// Same as `shared_object_transaction` but every authorities submit the transaction.
#[sim_test]
async fn many_shared_object_transactions() {
    let gas_objects = generate_test_gas_objects();
    let mut objects = vec![Object::shared_for_testing()];
    objects.extend(gas_objects.into_iter());

    // Get the authority configs and spawn them. Note that it is important to not drop
    // the handles (or the authorities will stop).
    let (configs, objects) = test_authority_configs_with_objects(objects);
    let rgp = configs.genesis.reference_gas_price();
    let mut objects = objects.into_iter();
    let shared_object = objects.next().unwrap();
    let gas_objects: Vec<_> = objects.collect();

    let _handles = spawn_test_authorities(&configs).await;

    // Make a test shared object certificate.
    let transaction = test_shared_object_transactions(Some(shared_object), Some(gas_objects), rgp)
        .pop()
        .unwrap();

    // Submit the transaction. Note that this transaction is random and we do not expect
    // it to be successfully executed by the Move execution engine.
    let _effects = submit_shared_object_transaction(transaction, &configs.net_addresses())
        .await
        .unwrap();
}

/// End-to-end shared transaction test for a Sui validator. It does not test the client or wallet,
/// but tests the end-to-end flow from Sui to consensus.
#[sim_test]
async fn call_shared_object_contract() {
    let gas_objects = generate_test_gas_objects();

    // Get the authority configs and spawn them. Note that it is important to not drop
    // the handles (or the authorities will stop).
    let (configs, mut gas_objects) = test_authority_configs_with_objects(gas_objects);
    let rgp = configs.genesis.reference_gas_price();
    let _handles = spawn_test_authorities(&configs).await;

    // Publish the move package to all authorities and get its package ID.
    let package_id =
        publish_counter_package(gas_objects.pop().unwrap(), &configs.net_addresses(), rgp)
            .await
            .0;

    // Make a transaction to create a counter.
    let transaction = move_transaction(
        gas_objects.pop().unwrap(),
        "counter",
        "create",
        package_id,
        /* arguments */ Vec::default(),
        rgp * TEST_ONLY_GAS_UNIT_FOR_GENERIC,
        rgp,
    );
    let (effects, _, _) =
        submit_single_owner_transaction(transaction, &configs.net_addresses()).await;
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
            rgp * TEST_ONLY_GAS_UNIT_FOR_GENERIC,
            rgp,
        );
        let (effects, _, _) =
            submit_shared_object_transaction(transaction, &configs.net_addresses())
                .await
                .unwrap();
        assert!(matches!(effects.status(), ExecutionStatus::Success { .. }));
        // Only genesis, assert_value, and counter creation are dependencies
        // Note that this assert would fail for second transaction
        // if they send counter_object_arg instead of counter_object_arg_imm
        assert_eq!(effects.dependencies().len(), 3);
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
        rgp * TEST_ONLY_GAS_UNIT_FOR_GENERIC,
        rgp,
    );
    let (effects, _, _) = submit_shared_object_transaction(transaction, &configs.net_addresses())
        .await
        .unwrap();
    let increment_transaction = *effects.transaction_digest();
    assert!(matches!(effects.status(), ExecutionStatus::Success { .. }));
    // Again - only genesis, increment, and counter creation are dependencies
    // Previously executed assert_value transaction(s) are not a dependency because they took immutable reference to shared object
    assert_eq!(effects.dependencies().len(), 3);
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
            rgp * TEST_ONLY_GAS_UNIT_FOR_GENERIC,
            rgp,
        );
        let (effects, _, _) =
            submit_shared_object_transaction(transaction, &configs.net_addresses())
                .await
                .unwrap();
        assert!(matches!(effects.status(), ExecutionStatus::Success { .. }));
        // Genesis, assert_value, and increment transaction are dependencies
        assert_eq!(effects.dependencies().len(), 3);
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
        rgp * TEST_ONLY_GAS_UNIT_FOR_GENERIC,
        rgp,
    );
    let (effects, _, _) = submit_shared_object_transaction(transaction, &configs.net_addresses())
        .await
        .unwrap();
    // Transaction fails
    assert_eq!(
        effects.status(),
        &ExecutionStatus::Failure {
            error: ExecutionFailureStatus::CommandArgumentError {
                arg_idx: 0,
                kind: CommandArgumentError::InvalidObjectByMutRef,
            },
            command: Some(0),
        }
    );
    assert_eq!(effects.dependencies().len(), 3);
    assert!(effects
        .dependencies()
        .contains(&assert_value_mut_transaction));
}

#[sim_test]
async fn access_clock_object_test() {
    let gas_objects = generate_test_gas_objects();

    // Get the authority configs and spawn them. Note that it is important to not drop
    // the handles (or the authorities will stop).
    let (configs, mut gas_objects) = test_authority_configs_with_objects(gas_objects);
    let rgp = configs.genesis.reference_gas_price();
    let handles = spawn_test_authorities(&configs).await;

    // Publish the move package to all authorities and get its package ID.
    let package_id =
        publish_counter_package(gas_objects.pop().unwrap(), &configs.net_addresses(), rgp)
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
        rgp * TEST_ONLY_GAS_UNIT_FOR_GENERIC,
        rgp,
    );
    let digest = *transaction.digest();
    let start = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap();
    let (effects, events, objects) =
        submit_shared_object_transaction(transaction, &configs.net_addresses())
            .await
            .unwrap();

    assert_eq!(
        objects.first().unwrap().compute_object_reference(),
        effects
            .shared_objects()
            .iter()
            .find(|(id, _, _)| *id == SUI_CLOCK_OBJECT_ID)
            .unwrap()
            .clone()
    );

    let finish = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap();
    assert!(matches!(effects.status(), ExecutionStatus::Success { .. }));

    assert_eq!(1, events.data.len());
    let event = events.data.get(0).unwrap();
    let Event { contents, .. } = event;

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
        assert_eq!(checkpoint.timestamp_ms, event.timestamp_ms);
        break;
    }
}

/// Same test as `call_shared_object_contract` but the clients submits many times the same
/// transaction (one copy per authority).
#[sim_test]
async fn shared_object_flood() {
    telemetry_subscribers::init_for_testing();
    let gas_objects = generate_test_gas_objects();

    // Get the authority configs and spawn them. Note that it is important to not drop
    // the handles (or the authorities will stop).
    let (configs, mut gas_objects) = test_authority_configs_with_objects(gas_objects);
    let rgp = configs.genesis.reference_gas_price();
    let _handles = spawn_test_authorities(&configs).await;

    // Publish the move package to all authorities and get its package ID.
    let package_id =
        publish_counter_package(gas_objects.pop().unwrap(), &configs.net_addresses(), rgp)
            .await
            .0;

    // Make a transaction to create a counter.
    let transaction = move_transaction(
        gas_objects.pop().unwrap(),
        "counter",
        "create",
        package_id,
        /* arguments */ Vec::default(),
        rgp * TEST_ONLY_GAS_UNIT_FOR_GENERIC,
        rgp,
    );
    let (effects, _, _) =
        submit_single_owner_transaction(transaction, &configs.net_addresses()).await;
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
        rgp * TEST_ONLY_GAS_UNIT_FOR_GENERIC,
        rgp,
    );
    let (effects, _, _) = submit_shared_object_transaction(transaction, &configs.net_addresses())
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
        rgp * TEST_ONLY_GAS_UNIT_FOR_GENERIC,
        rgp,
    );
    let (effects, _, _) = submit_shared_object_transaction(transaction, &configs.net_addresses())
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
        rgp * TEST_ONLY_GAS_UNIT_FOR_GENERIC,
        rgp,
    );
    let (effects, _, _) = submit_shared_object_transaction(transaction, &configs.net_addresses())
        .await
        .unwrap();
    assert!(matches!(effects.status(), ExecutionStatus::Success { .. }));
}

#[sim_test]
async fn shared_object_sync() {
    telemetry_subscribers::init_for_testing();
    let gas_objects = generate_test_gas_objects();

    // Get the authority configs and spawn them. Note that it is important to not drop
    // the handles (or the authorities will stop).
    let (configs, mut gas_objects) = test_authority_configs_with_objects(gas_objects);
    let rgp = configs.genesis.reference_gas_price();
    let _handles = spawn_test_authorities(&configs).await;

    // Publish the move package to all authorities and get its package ID.
    let package_id =
        publish_counter_package(gas_objects.pop().unwrap(), &configs.net_addresses(), rgp)
            .await
            .0;

    // Send a transaction to create a counter, to all but one authority.
    let create_counter_transaction = move_transaction(
        gas_objects.pop().unwrap(),
        "counter",
        "create",
        package_id,
        /* arguments */ Vec::default(),
        rgp * TEST_ONLY_GAS_UNIT_FOR_GENERIC,
        rgp,
    );

    let committee = configs.committee_with_network();
    let (slow_validators, fast_validators): (Vec<_>, Vec<_>) = committee
        .network_metadata
        .into_iter()
        .partition(|(name, _net)| {
            position_submit_certificate(
                &committee.committee,
                name,
                create_counter_transaction.digest(),
            ) > 0
        });

    let (effects, _, _) = submit_single_owner_transaction(
        create_counter_transaction.clone(),
        &slow_validators
            .iter()
            .map(|(_, net)| net.network_address.clone())
            .collect::<Vec<_>>(),
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
    let has_counter = stream::iter(&slow_validators).any(|(_, net)| async move {
        get_client(&net.network_address)
            .handle_object_info_request(ObjectInfoRequest::latest_object_info_request(
                counter_id, None,
            ))
            .await
            .is_ok()
    });

    assert!(has_counter.await);

    // Check that the validator that wasn't sent the transaction is unaware of the counter object
    assert!(get_client(&fast_validators[0].1.network_address)
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
        rgp * TEST_ONLY_GAS_UNIT_FOR_GENERIC,
        rgp,
    );

    // Let's submit the transaction to the original set of validators.
    let (effects, _, _) = submit_shared_object_transaction(
        increment_counter_transaction.clone(),
        &configs.net_addresses()[1..],
    )
    .await
    .unwrap();
    assert!(matches!(effects.status(), ExecutionStatus::Success { .. }));

    // Submit transactions to the out-of-date authority.
    // It will succeed because we share owned object certificates through narwhal
    let (effects, _, _) = submit_shared_object_transaction(
        increment_counter_transaction,
        &configs.net_addresses()[0..1],
    )
    .await
    .unwrap();
    assert!(matches!(effects.status(), ExecutionStatus::Success { .. }));
}

/// Send a simple shared object transaction to Sui and ensures the client gets back a response.
#[sim_test]
async fn replay_shared_object_transaction() {
    let gas_objects = generate_test_gas_objects();

    // Get the authority configs and spawn them. Note that it is important to not drop
    // the handles (or the authorities will stop).
    let (configs, mut gas_objects) = test_authority_configs_with_objects(gas_objects);
    let rgp = configs.genesis.reference_gas_price();
    let _handles = spawn_test_authorities(&configs).await;

    // Publish the move package to all authorities and get its package ID
    let package_id =
        publish_counter_package(gas_objects.pop().unwrap(), &configs.net_addresses(), rgp)
            .await
            .0;

    // Send a transaction to create a counter (only to one authority) -- twice.
    let create_counter_transaction = move_transaction(
        gas_objects.pop().unwrap(),
        "counter",
        "create",
        package_id,
        /* arguments */ Vec::default(),
        rgp * TEST_ONLY_GAS_UNIT_FOR_GENERIC,
        rgp,
    );

    let mut version = None;
    for _ in 0..2 {
        let (effects, _, _) = submit_single_owner_transaction(
            create_counter_transaction.clone(),
            &configs.net_addresses(),
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
