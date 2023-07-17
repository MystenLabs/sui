// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::ops::Deref;
use std::time::{Duration, SystemTime};
use sui_core::consensus_adapter::position_submit_certificate;
use sui_json_rpc_types::SuiTransactionBlockEffectsAPI;
use sui_macros::sim_test;
use sui_test_transaction_builder::{
    publish_basics_package, publish_basics_package_and_make_counter, TestTransactionBuilder,
};
use sui_types::effects::TransactionEffectsAPI;
use sui_types::event::Event;
use sui_types::execution_status::{CommandArgumentError, ExecutionFailureStatus, ExecutionStatus};
use sui_types::messages_grpc::ObjectInfoRequest;
use sui_types::transaction::{CallArg, ObjectArg};
use sui_types::SUI_CLOCK_OBJECT_ID;
use test_cluster::TestClusterBuilder;

/// Send a simple shared object transaction to Sui and ensures the client gets back a response.
#[sim_test]
async fn shared_object_transaction() {
    let test_cluster = TestClusterBuilder::new().build().await;
    let (sender, mut objects) = test_cluster.wallet.get_one_account().await.unwrap();
    let rgp = test_cluster.get_reference_gas_price().await;
    let transaction = TestTransactionBuilder::new(sender, objects.pop().unwrap(), rgp)
        .call_staking(
            objects.pop().unwrap(),
            test_cluster
                .swarm
                .active_validators()
                .next()
                .unwrap()
                .config
                .sui_address(),
        )
        .build();

    test_cluster
        .sign_and_execute_transaction(&transaction)
        .await;
}

/// End-to-end shared transaction test for a Sui validator. It does not test the client or wallet,
/// but tests the end-to-end flow from Sui to consensus.
#[sim_test]
async fn call_shared_object_contract() {
    let test_cluster = TestClusterBuilder::new().build().await;
    let (package, counter) = publish_basics_package_and_make_counter(&test_cluster.wallet).await;
    let package_id = package.0;
    let counter_id = counter.0;
    let counter_initial_shared_version = counter.1;
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
    let counter_creation_transaction = test_cluster
        .get_object_from_fullnode_store(&counter_id)
        .await
        .unwrap()
        .previous_transaction;

    // Send two read only transactions
    let (sender, objects) = test_cluster.wallet.get_one_account().await.unwrap();
    let rgp = test_cluster.get_reference_gas_price().await;
    let mut prev_assert_value_txs = Vec::new();
    for gas in objects {
        // Ensure the value of the counter is `0`.
        let transaction = TestTransactionBuilder::new(sender, gas, rgp)
            .move_call(
                package_id,
                "counter",
                "assert_value",
                vec![
                    CallArg::Object(counter_object_arg_imm),
                    CallArg::Pure(0u64.to_le_bytes().to_vec()),
                ],
            )
            .build();
        let effects = test_cluster
            .sign_and_execute_transaction(&transaction)
            .await
            .effects
            .unwrap();
        // Check that all reads must depend on the creation of the counter, but not to any previous reads.
        assert!(effects
            .dependencies()
            .contains(&counter_creation_transaction));
        assert!(prev_assert_value_txs
            .iter()
            .all(|tx| { !effects.dependencies().contains(tx) }));
        prev_assert_value_txs.push(*effects.transaction_digest());
    }

    // Make a transaction to increment the counter.
    let transaction = test_cluster
        .test_transaction_builder()
        .await
        .call_counter_increment(package_id, counter_id, counter_initial_shared_version)
        .build();
    let effects = test_cluster
        .sign_and_execute_transaction(&transaction)
        .await
        .effects
        .unwrap();
    let increment_transaction = *effects.transaction_digest();
    assert!(effects
        .dependencies()
        .contains(&counter_creation_transaction));
    // Previously executed assert_value transaction(s) are not a dependency because they took immutable reference to shared object
    assert!(prev_assert_value_txs
        .iter()
        .all(|tx| { !effects.dependencies().contains(tx) }));

    // assert_value can take both mutable and immutable references
    // it is allowed to pass mutable shared object arg to move call taking immutable reference
    let mut assert_value_mut_transaction = None;
    for imm in [true, false] {
        // Ensure the value of the counter is `1`.
        let transaction = test_cluster
            .test_transaction_builder()
            .await
            .move_call(
                package_id,
                "counter",
                "assert_value",
                vec![
                    CallArg::Object(if imm {
                        counter_object_arg_imm
                    } else {
                        counter_object_arg
                    }),
                    CallArg::Pure(1u64.to_le_bytes().to_vec()),
                ],
            )
            .build();
        let effects = test_cluster
            .sign_and_execute_transaction(&transaction)
            .await
            .effects
            .unwrap();
        assert!(effects.dependencies().contains(&increment_transaction));
        if let Some(prev) = assert_value_mut_transaction {
            assert!(effects.dependencies().contains(&prev));
        }
        assert_value_mut_transaction = Some(*effects.transaction_digest());
    }

    let assert_value_mut_transaction = assert_value_mut_transaction.unwrap();

    // And last check - attempt to send increment transaction with immutable reference
    let transaction = test_cluster
        .test_transaction_builder()
        .await
        .move_call(
            package_id,
            "counter",
            "increment",
            vec![CallArg::Object(counter_object_arg_imm)],
        )
        .build();
    let effects = test_cluster
        .wallet
        .execute_transaction_may_fail(test_cluster.wallet.sign_transaction(&transaction))
        .await
        .unwrap()
        .effects
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
        .into()
    );
    assert!(effects
        .dependencies()
        .contains(&assert_value_mut_transaction));
}

#[ignore("Disabled due to flakiness - re-enable when failure is fixed")]
#[sim_test]
async fn access_clock_object_test() {
    let test_cluster = TestClusterBuilder::new().build().await;
    let package_id = publish_basics_package(&test_cluster.wallet).await.0;

    let transaction = test_cluster.wallet.sign_transaction(
        &test_cluster
            .test_transaction_builder()
            .await
            .move_call(package_id, "clock", "get_time", vec![CallArg::CLOCK_IMM])
            .build(),
    );
    let digest = *transaction.digest();
    let start = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap();
    let (effects, events, objects) = test_cluster
        .execute_transaction_return_raw_effects(transaction)
        .await
        .unwrap();
    assert!(effects.status().is_ok());

    assert_eq!(
        objects.first().unwrap().compute_object_reference(),
        effects
            .input_shared_objects()
            .into_iter()
            .find(|((id, _, _), _)| id == &SUI_CLOCK_OBJECT_ID)
            .map(|(obj_ref, _)| obj_ref)
            .unwrap()
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
        let checkpoint = test_cluster
            .fullnode_handle
            .sui_node
            .with_async(|node| async {
                node.state()
                    .get_transaction_checkpoint(&digest, &node.state().epoch_store_for_testing())
                    .unwrap()
            })
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

#[sim_test]
async fn shared_object_sync() {
    let test_cluster = TestClusterBuilder::new().build().await;
    let package_id = publish_basics_package(&test_cluster.wallet).await.0;

    // Since we use submit_transaction_to_validators in this test, which does not go through fullnode,
    // we need to manage gas objects ourselves.
    let (sender, mut objects) = test_cluster.wallet.get_one_account().await.unwrap();
    let rgp = test_cluster.get_reference_gas_price().await;
    // Send a transaction to create a counter, to all but one authority.
    let create_counter_transaction = test_cluster.wallet.sign_transaction(
        &TestTransactionBuilder::new(sender, objects.pop().unwrap(), rgp)
            .call_counter_create(package_id)
            .build(),
    );
    let committee = test_cluster.committee().deref().clone();
    let validators = test_cluster.get_validator_pubkeys();
    let (slow_validators, fast_validators): (Vec<_>, Vec<_>) =
        validators.iter().partition(|name| {
            position_submit_certificate(&committee, name, create_counter_transaction.digest()) > 0
        });

    let (effects, _, _) = test_cluster
        .submit_transaction_to_validators(create_counter_transaction.clone(), &slow_validators)
        .await
        .unwrap();
    assert!(effects.status().is_ok());
    let ((counter_id, counter_initial_shared_version, _), _) = effects.created()[0];

    // Check that the counter object exists in at least one of the validators the transaction was
    // sent to.
    for validator in test_cluster.swarm.validator_node_handles() {
        if slow_validators.contains(&validator.state().name) {
            assert!(validator
                .state()
                .handle_object_info_request(ObjectInfoRequest::latest_object_info_request(
                    counter_id, None,
                ))
                .await
                .is_ok());
        }
    }

    // Check that the validator that wasn't sent the transaction is unaware of the counter object
    for validator in test_cluster.swarm.validator_node_handles() {
        if fast_validators.contains(&validator.state().name) {
            assert!(validator
                .state()
                .handle_object_info_request(ObjectInfoRequest::latest_object_info_request(
                    counter_id, None,
                ))
                .await
                .is_err());
        }
    }

    // Make a transaction to increment the counter.
    let increment_counter_transaction = test_cluster.wallet.sign_transaction(
        &TestTransactionBuilder::new(sender, objects.pop().unwrap(), rgp)
            .call_counter_increment(package_id, counter_id, counter_initial_shared_version)
            .build(),
    );

    // Let's submit the transaction to the original set of validators, except the first.
    let (effects, _, _) = test_cluster
        .submit_transaction_to_validators(increment_counter_transaction.clone(), &validators[1..])
        .await
        .unwrap();
    assert!(effects.status().is_ok());

    // Submit transactions to the out-of-date authority.
    // It will succeed because we share owned object certificates through narwhal
    let (effects, _, _) = test_cluster
        .submit_transaction_to_validators(increment_counter_transaction, &validators[0..1])
        .await
        .unwrap();
    assert!(effects.status().is_ok());
}

/// Send a simple shared object transaction to Sui and ensures the client gets back a response.
#[sim_test]
async fn replay_shared_object_transaction() {
    let test_cluster = TestClusterBuilder::new().build().await;
    let package_id = publish_basics_package(&test_cluster.wallet).await.0;

    // Send a transaction to create a counter (only to one authority) -- twice.
    let create_counter_transaction = test_cluster.wallet.sign_transaction(
        &test_cluster
            .test_transaction_builder()
            .await
            .call_counter_create(package_id)
            .build(),
    );

    let mut version = None;
    for _ in 0..2 {
        let effects = test_cluster
            .execute_transaction(create_counter_transaction.clone())
            .await
            .effects
            .unwrap();

        // Ensure the sequence number of the shared object did not change.
        let curr = effects.created()[0].reference.version;
        if let Some(prev) = version {
            assert_eq!(
                prev, curr,
                "SequenceNumber of shared object did not change."
            );
        }

        version = Some(curr);
    }
}
