// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::authority::authority_store::ObjectLockStatus;
use crate::authority::authority_test_utils::{
    assign_shared_object_versions, assign_versions_and_schedule,
};
use crate::authority::shared_object_version_manager::Schedulable;
use crate::authority::test_authority_builder::TestAuthorityBuilder;
use crate::authority::{AuthorityState, ExecutionEnv};
use crate::authority_client::AuthorityAPI;
use crate::authority_server::{ValidatorService, ValidatorServiceMetrics};
use crate::checkpoints::CheckpointStore;
use crate::consensus_adapter::ConsensusAdapter;
use crate::consensus_adapter::ConsensusAdapterMetrics;
use crate::consensus_adapter::{ConnectionMonitorStatusForTests, MockConsensusClient};
use crate::safe_client::SafeClient;
use crate::test_authority_clients::LocalAuthorityClient;
use crate::test_utils::{make_transfer_object_move_transaction, make_transfer_object_transaction};
use crate::unit_test_utils::{
    init_local_authorities, init_local_authorities_with_overload_thresholds,
};
use sui_protocol_config::ProtocolConfig;

use sui_types::error::SuiErrorKind;
use sui_types::executable_transaction::VerifiedExecutableTransaction;
use sui_types::messages_grpc::VerifiedObjectInfoResponse;
use sui_types::transaction::VerifiedTransaction;

use std::sync::Arc;
use std::time::Duration;

use itertools::Itertools;
use move_core_types::{account_address::AccountAddress, ident_str};
use sui_config::node::AuthorityOverloadConfig;
use sui_test_transaction_builder::TestTransactionBuilder;
use sui_types::base_types::{ObjectID, ObjectRef, SuiAddress};
use sui_types::crypto::{AccountKeyPair, Signature, Signer, get_key_pair};
use sui_types::effects::{TransactionEffects, TransactionEffectsAPI};
use sui_types::messages_grpc::{LayoutGenerationOption, ObjectInfoRequest};
use sui_types::object::{Object, Owner};
use sui_types::transaction::{
    CallArg, TEST_ONLY_GAS_UNIT_FOR_HEAVY_COMPUTATION_STORAGE, Transaction, TransactionData,
};
use sui_types::utils::to_sender_signed_transaction;
use tokio::time::sleep;

/// Creates a Move transaction that creates an object_basics object.
fn create_object_move_transaction(
    src: SuiAddress,
    secret: &dyn Signer<Signature>,
    dest: SuiAddress,
    value: u64,
    package_id: ObjectID,
    gas_object_ref: ObjectRef,
    gas_price: u64,
) -> Transaction {
    let arguments = vec![
        CallArg::Pure(value.to_le_bytes().to_vec()),
        CallArg::Pure(bcs::to_bytes(&AccountAddress::from(dest)).unwrap()),
    ];

    to_sender_signed_transaction(
        TransactionData::new_move_call(
            src,
            package_id,
            ident_str!("object_basics").to_owned(),
            ident_str!("create").to_owned(),
            Vec::new(),
            gas_object_ref,
            arguments,
            TEST_ONLY_GAS_UNIT_FOR_HEAVY_COMPUTATION_STORAGE * gas_price,
            gas_price,
        )
        .unwrap(),
        secret,
    )
}

/// Gets the latest object reference from an authority.
async fn get_latest_ref<A>(authority: Arc<SafeClient<A>>, object_id: ObjectID) -> ObjectRef
where
    A: AuthorityAPI + Send + Sync + Clone + 'static,
{
    if let Ok(VerifiedObjectInfoResponse { object }) = authority
        .handle_object_info_request(ObjectInfoRequest::latest_object_info_request(
            object_id,
            LayoutGenerationOption::None,
        ))
        .await
    {
        return object.compute_object_reference();
    }
    panic!("Object not found!");
}

/*
TODO: Re-enable after we have checkpoint v2.
#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn pending_exec_notify_ready_certificates() {
    use telemetry_subscribers::init_for_testing;
    init_for_testing();

    let setup = checkpoint_tests_setup(20, Duration::from_millis(200), true).await;

    let TestSetup {
        committee: _committee,
        authorities,
        mut transactions,
        aggregator,
    } = setup;

    let authority_state = authorities[0].authority.clone();
    let mut ready_certificates_stream = authority_state.ready_certificates_stream().await.unwrap();

    // TODO: duplicated with checkpoint_driver/tests.rs
    // Start active part of authority.
    for inner_state in authorities.clone() {
        let inner_agg = aggregator.clone();
        let active_state = Arc::new(
            ActiveAuthority::new_with_ephemeral_storage_for_test(
                inner_state.authority.clone(),
                inner_agg,
            )
            .unwrap(),
        );
        let _active_handle = active_state
            .spawn_checkpoint_process(CheckpointMetrics::new_for_tests())
            .await;
    }

    let sender_aggregator = aggregator.clone();
    let _end_of_sending_join = tokio::task::spawn(async move {
        let mut certs = Vec::new();
        while let Some(t) = transactions.pop() {
            let (_cert, effects) = sender_aggregator
                .execute_transaction_block(&t)
                .await
                .expect("All ok.");

            // Check whether this is a success?
            assert!(matches!(
                effects.data().status,
                ExecutionStatus::Success { .. }
            ));
            println!("Execute at {:?}", tokio::time::Instant::now());

            certs.push(_cert);

            // Add some delay between transactions
            tokio::time::sleep(Duration::from_secs(27)).await;
        }

        certs
    });

    // Wait for all the sending to happen.
    let certs = _end_of_sending_join.await.expect("all ok");

    // Clear effects so their executions will happen below.
    authority_state
        .database
        .perpetual_tables
        .effects
        .clear()
        .expect("Clearing effects failed!");

    // Insert the certificates
    authority_state
        .enqueue_certificates_for_execution(certs.clone())
        .await
        .expect("Storage is ok");

    tokio::task::yield_now().await;

    // Wait to get back the certificates
    wait_for_certs(&mut ready_certificates_stream, &certs).await;

    // Should have no certificate any more.
    wait_for_certs(&mut ready_certificates_stream, &vec![]).await;
}

#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn pending_exec_full() {
    use telemetry_subscribers::init_for_testing;
    init_for_testing();

    let setup = checkpoint_tests_setup(20, Duration::from_millis(200), true).await;

    let TestSetup {
        committee: _committee,
        authorities,
        mut transactions,
        aggregator,
    } = setup;

    let authority_state = authorities[0].authority.clone();

    // Start active part of authority.
    for inner_state in authorities.clone() {
        let inner_agg = aggregator.clone();
        let _active_handle = tokio::task::spawn(async move {
            let active_state = Arc::new(
                ActiveAuthority::new_with_ephemeral_storage_for_test(
                    inner_state.authority.clone(),
                    inner_agg,
                )
                .unwrap(),
            );
            let batch_state = inner_state.authority.clone();
            tokio::task::spawn(async move {
                batch_state
                    .run_batch_service(1, Duration::from_secs(1))
                    .await
            });
            active_state
                .spawn_checkpoint_process(CheckpointMetrics::new_for_tests())
                .await;
        });
    }

    let sender_aggregator = aggregator.clone();
    let _end_of_sending_join = tokio::task::spawn(async move {
        let mut certs = Vec::new();
        while let Some(t) = transactions.pop() {
            let (_cert, effects) = sender_aggregator
                .execute_transaction_block(&t)
                .await
                .expect("All ok.");

            // Check whether this is a success?
            assert!(matches!(
                effects.data().status,
                ExecutionStatus::Success { .. }
            ));
            println!("Execute at {:?}", tokio::time::Instant::now());

            certs.push(_cert);

            // Add some delay between transactions
            tokio::time::sleep(Duration::from_secs(27)).await;
        }

        certs
    });

    // Wait for all the sending to happen.
    let certs = _end_of_sending_join.await.expect("all ok");

    // Insert the certificates
    authority_state
        .enqueue_certificates_for_execution(certs.clone())
        .await
        .expect("Storage is ok");

    // Wait for execution.
    for cert in certs {
        wait_for_tx(*cert.digest(), authority_state.clone()).await;
    }
}

 */

// Helper to create a VerifiedExecutableTransaction.
// This uses CertificateProof::Consensus to simulate consensus-certified transactions.
fn create_executable_transaction(
    authority_clients: &[Arc<SafeClient<LocalAuthorityClient>>],
    txn: &Transaction,
) -> VerifiedExecutableTransaction {
    // Get the epoch from the first authority
    let state = &authority_clients[0].authority_client().state;
    let epoch = state.epoch_store_for_testing().epoch();

    // Verify the transaction and create executable with consensus proof
    let verified_tx = VerifiedTransaction::new_unchecked(txn.clone());
    VerifiedExecutableTransaction::new_from_consensus(verified_tx, epoch)
}

// Helper to execute an owned object transaction on authorities.
// Creates a VerifiedExecutableTransaction and executes on first three authorities.
async fn execute_owned_on_first_three_authorities(
    authority_clients: &[Arc<SafeClient<LocalAuthorityClient>>],
    txn: &Transaction,
) -> (VerifiedExecutableTransaction, TransactionEffects) {
    // Create executable transaction with MFP-style proof
    let executable = create_executable_transaction(authority_clients, txn);

    // Execute on first three authorities
    for client in authority_clients.iter().take(3) {
        let state = &client.authority_client().state;
        state.execution_scheduler().enqueue(
            vec![(executable.clone().into(), ExecutionEnv::new())],
            &state.epoch_store_for_testing(),
        );
    }

    // Wait for execution on the third authority and return effects
    let effects = authority_clients[2]
        .authority_client()
        .state
        .get_transaction_cache_reader()
        .notify_read_executed_effects("", &[*executable.digest()])
        .await
        .pop()
        .unwrap();

    (executable, effects)
}

// Helper to execute a shared object transaction via consensus.
pub async fn do_executable_with_shared_objects(
    authority: &AuthorityState,
    executable: &VerifiedExecutableTransaction,
) -> TransactionEffects {
    assign_versions_and_schedule(authority, executable).await;
    authority
        .get_transaction_cache_reader()
        .notify_read_executed_effects("", &[*executable.digest()])
        .await
        .pop()
        .unwrap()
}

// Helper to execute a shared object transaction on authorities.
// Creates a VerifiedExecutableTransaction and executes via consensus on first three authorities.
async fn execute_shared_on_first_three_authorities(
    authority_clients: &[Arc<SafeClient<LocalAuthorityClient>>],
    txn: &Transaction,
) -> (VerifiedExecutableTransaction, TransactionEffects) {
    // Create executable transaction with MFP-style proof
    let executable = create_executable_transaction(authority_clients, txn);

    // Execute via consensus on first three authorities
    do_executable_with_shared_objects(&authority_clients[0].authority_client().state, &executable)
        .await;
    do_executable_with_shared_objects(&authority_clients[1].authority_client().state, &executable)
        .await;
    let effects = do_executable_with_shared_objects(
        &authority_clients[2].authority_client().state,
        &executable,
    )
    .await;
    (executable, effects)
}

#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn test_execution_with_dependencies() {
    telemetry_subscribers::init_for_testing();

    // Disable randomness, it can't be constructed with fake authorities in this test anyway.
    let _guard = ProtocolConfig::apply_overrides_for_testing(|_, mut config| {
        config.set_random_beacon_for_testing(false);
        config
    });

    // ---- Initialize a network with three accounts, each with 10 gas objects.

    const NUM_ACCOUNTS: usize = 3;
    let accounts: Vec<(_, AccountKeyPair)> =
        (0..NUM_ACCOUNTS).map(|_| get_key_pair()).collect_vec();

    const NUM_GAS_OBJECTS_PER_ACCOUNT: usize = 10;
    let gas_objects = (0..NUM_ACCOUNTS)
        .map(|i| {
            (0..NUM_GAS_OBJECTS_PER_ACCOUNT)
                .map(|_| Object::with_owner_for_testing(accounts[i].0))
                .collect_vec()
        })
        .collect_vec();
    let all_gas_objects = gas_objects.clone().into_iter().flatten().collect_vec();

    let (aggregator, authorities, _genesis, package) =
        init_local_authorities(4, all_gas_objects.clone()).await;
    let authority_clients: Vec<_> = authorities
        .iter()
        .map(|a| aggregator.authority_clients[&a.name].clone())
        .collect();
    let rgp = authorities
        .first()
        .unwrap()
        .reference_gas_price_for_testing()
        .unwrap();

    // ---- Create an owned object and a shared counter.

    let mut executed_owned_certs = Vec::new();
    let mut executed_shared_certs = Vec::new();

    // Initialize an object owned by 1st account.
    let (addr1, key1): &(_, AccountKeyPair) = &accounts[0];
    let gas_ref = get_latest_ref(authority_clients[0].clone(), gas_objects[0][0].id()).await;
    let tx1 = create_object_move_transaction(*addr1, key1, *addr1, 100, package, gas_ref, rgp);
    let (executable, effects1) =
        execute_owned_on_first_three_authorities(&authority_clients, &tx1).await;
    executed_owned_certs.push(executable);
    let mut owned_object_ref = effects1.created()[0].0;

    // Initialize a shared counter, re-using gas_ref_0 so it has to execute after tx1.
    let gas_ref = get_latest_ref(authority_clients[0].clone(), gas_objects[0][0].id()).await;
    let tx2 = TestTransactionBuilder::new(*addr1, gas_ref, rgp)
        .call_counter_create(package)
        .build_and_sign(key1);
    let (executable, effects2) =
        execute_owned_on_first_three_authorities(&authority_clients, &tx2).await;
    executed_owned_certs.push(executable);
    let (mut shared_counter_ref, owner) = effects2.created()[0].clone();
    let shared_counter_initial_version = if let Owner::Shared {
        initial_shared_version,
    } = owner
    {
        // Because the gas object used has version 2, the initial lamport timestamp of the shared
        // counter is 3.
        assert_eq!(initial_shared_version.value(), 3);
        initial_shared_version
    } else {
        panic!("Not a shared object! {:?} {:?}", shared_counter_ref, owner);
    };

    // ---- Execute transactions with dependencies on first 3 nodes in the dependency order.

    // In each iteration, creates an owned and a shared transaction that depends on previous input
    // and gas objects.
    for i in 0..100 {
        let source_index = i % NUM_ACCOUNTS;
        let (source_addr, source_key) = &accounts[source_index];

        let gas_ref = get_latest_ref(
            authority_clients[source_index].clone(),
            gas_objects[source_index][i * 3 % NUM_GAS_OBJECTS_PER_ACCOUNT].id(),
        )
        .await;
        let (dest_addr, _) = &accounts[(i + 1) % NUM_ACCOUNTS];
        let owned_tx = make_transfer_object_move_transaction(
            *source_addr,
            source_key,
            *dest_addr,
            owned_object_ref,
            package,
            gas_ref,
            TEST_ONLY_GAS_UNIT_FOR_HEAVY_COMPUTATION_STORAGE,
            rgp,
        );
        let (executable, effects) =
            execute_owned_on_first_three_authorities(&authority_clients, &owned_tx).await;
        executed_owned_certs.push(executable);
        owned_object_ref = effects.mutated_excluding_gas().first().unwrap().0;

        let gas_ref = get_latest_ref(
            authority_clients[source_index].clone(),
            gas_objects[source_index][i * 7 % NUM_GAS_OBJECTS_PER_ACCOUNT].id(),
        )
        .await;
        let shared_tx = TestTransactionBuilder::new(*source_addr, gas_ref, rgp)
            .call_counter_increment(
                package,
                shared_counter_ref.0,
                shared_counter_initial_version,
            )
            .build_and_sign(source_key);
        let (executable, effects) =
            execute_shared_on_first_three_authorities(&authority_clients, &shared_tx).await;
        executed_shared_certs.push(executable);
        shared_counter_ref = effects.mutated_excluding_gas().first().unwrap().0;
    }

    // ---- Execute transactions in reverse dependency order on the last authority.

    // Assign shared object versions in the executed order.

    let mut executables_with_env = Vec::new();
    for executable in executed_shared_certs.iter() {
        let assigned_versions = assign_shared_object_versions(&authorities[3], executable).await;
        executables_with_env.push((
            Schedulable::Transaction(executable.clone()),
            ExecutionEnv::new().with_assigned_versions(assigned_versions),
        ));
    }

    // Enqueue executables out of dependency order for executions.
    for (executable, env) in executables_with_env.iter().rev() {
        authorities[3].execution_scheduler().enqueue(
            vec![(executable.clone(), env.clone())],
            &authorities[3].epoch_store_for_testing(),
        );
    }
    for executable in executed_owned_certs.iter().rev() {
        authorities[3].execution_scheduler().enqueue(
            vec![(executable.clone().into(), ExecutionEnv::new())],
            &authorities[3].epoch_store_for_testing(),
        );
    }

    // All certs should get executed eventually.
    let digests: Vec<_> = executed_shared_certs
        .iter()
        .chain(executed_owned_certs.iter())
        .map(|cert| *cert.digest())
        .collect();
    authorities[3]
        .get_transaction_cache_reader()
        .notify_read_executed_effects("", &digests)
        .await;
}

fn create_executable_for_test(
    authority_clients: &[Arc<SafeClient<LocalAuthorityClient>>],
    txn: &Transaction,
) -> VerifiedExecutableTransaction {
    create_executable_transaction(authority_clients, txn)
}

#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn test_per_object_overload() {
    telemetry_subscribers::init_for_testing();

    // Disable randomness, it can't be constructed with fake authorities in this test anyway.
    let _guard = ProtocolConfig::apply_overrides_for_testing(|_, mut config| {
        config.set_random_beacon_for_testing(false);
        config
    });

    // Initialize a network with 1 account and gas objects.
    let (addr, key): (_, AccountKeyPair) = get_key_pair();
    // Use a small threshold for testing to avoid creating too many objects
    const TEST_PER_OBJECT_QUEUE_LENGTH: usize = 20;
    const NUM_GAS_OBJECTS_PER_ACCOUNT: usize = TEST_PER_OBJECT_QUEUE_LENGTH + 10; // Some buffer
    let gas_objects = (0..NUM_GAS_OBJECTS_PER_ACCOUNT)
        .map(|_| Object::with_owner_for_testing(addr))
        .collect_vec();
    let (aggregator, authorities, _genesis, package) =
        init_local_authorities_with_overload_thresholds(
            4,
            gas_objects.clone(),
            AuthorityOverloadConfig {
                max_transaction_manager_per_object_queue_length: TEST_PER_OBJECT_QUEUE_LENGTH,
                ..Default::default()
            },
        )
        .await;
    let rgp = authorities
        .first()
        .unwrap()
        .reference_gas_price_for_testing()
        .unwrap();
    let authority_clients: Vec<_> = authorities
        .iter()
        .map(|a| aggregator.authority_clients[&a.name].clone())
        .collect();

    // Create a shared counter.
    let gas_ref = get_latest_ref(authority_clients[0].clone(), gas_objects[0].id()).await;
    let create_counter_txn = TestTransactionBuilder::new(addr, gas_ref, rgp)
        .call_counter_create(package)
        .build_and_sign(&key);
    let create_counter_executable =
        create_executable_for_test(&authority_clients, &create_counter_txn);
    for authority in authorities.iter().take(3) {
        assign_versions_and_schedule(authority, &create_counter_executable).await;
    }
    for authority in authorities.iter().take(3) {
        authority
            .get_transaction_cache_reader()
            .notify_read_executed_effects("", &[*create_counter_executable.digest()])
            .await
            .pop()
            .unwrap();
    }

    // Executing this transaction on the last authority should succeed.
    // The executable was already created, so we just need to execute it.
    assign_versions_and_schedule(&authorities[3], &create_counter_executable).await;
    let create_counter_effects = authorities[3]
        .get_transaction_cache_reader()
        .notify_read_executed_effects("", &[*create_counter_executable.digest()])
        .await
        .pop()
        .unwrap();
    let (shared_counter_ref, owner) = create_counter_effects.created()[0].clone();
    let Owner::Shared {
        initial_shared_version: shared_counter_initial_version,
    } = owner
    else {
        panic!("Not a shared object! {:?} {:?}", shared_counter_ref, owner);
    };

    // Stop execution on the last authority, to simulate having a backlog.
    authorities[3].shutdown_execution_for_test();
    // Make sure execution driver has exited.
    sleep(Duration::from_secs(1)).await;

    // Sign and try execute 1000 txns on the first three authorities. And enqueue them on the last authority.
    // First shared counter txn has input object available on authority 3. So to overload authority 3, 1 more
    // txn is needed.
    let num_txns = authorities[3]
        .overload_config()
        .max_transaction_manager_per_object_queue_length
        + 1;
    for gas_object in gas_objects.iter().take(num_txns) {
        let gas_ref = get_latest_ref(authority_clients[0].clone(), gas_object.id()).await;
        let shared_txn = TestTransactionBuilder::new(addr, gas_ref, rgp)
            .call_counter_increment(
                package,
                shared_counter_ref.0,
                shared_counter_initial_version,
            )
            .build_and_sign(&key);
        let shared_executable = create_executable_for_test(&authority_clients, &shared_txn);
        for authority in authorities.iter().take(3) {
            assign_versions_and_schedule(authority, &shared_executable).await;
        }
        assign_versions_and_schedule(&authorities[3], &shared_executable).await;
    }
    // Give enough time to schedule the transactions.
    sleep(Duration::from_secs(3)).await;

    // Trying to sign a new transaction would now fail.
    let gas_ref = get_latest_ref(authority_clients[0].clone(), gas_objects[num_txns].id()).await;
    let shared_txn = TestTransactionBuilder::new(addr, gas_ref, rgp)
        .call_counter_increment(
            package,
            shared_counter_ref.0,
            shared_counter_initial_version,
        )
        .build_and_sign(&key);
    let res = authorities[3]
        .execution_scheduler()
        .check_execution_overload(authorities[3].overload_config(), shared_txn.data());
    let message = format!("{res:?}");
    assert!(
        message.contains("TooManyTransactionsPendingOnObject"),
        "{}",
        message
    );
}

#[tokio::test]
async fn test_txn_age_overload() {
    telemetry_subscribers::init_for_testing();

    // Disable randomness, it can't be constructed with fake authorities in this test anyway.
    let _guard = ProtocolConfig::apply_overrides_for_testing(|_, mut config| {
        config.set_random_beacon_for_testing(false);
        config
    });

    // Initialize a network with 1 account and 3 gas objects.
    let (addr, key): (_, AccountKeyPair) = get_key_pair();
    let gas_objects = (0..3)
        .map(|_| Object::with_owner_for_testing(addr))
        .collect_vec();
    let (aggregator, authorities, _genesis, package) =
        init_local_authorities_with_overload_thresholds(
            4,
            gas_objects.clone(),
            AuthorityOverloadConfig {
                max_txn_age_in_queue: Duration::from_secs(5),
                ..Default::default()
            },
        )
        .await;
    let rgp = authorities
        .first()
        .unwrap()
        .reference_gas_price_for_testing()
        .unwrap();
    let authority_clients: Vec<_> = authorities
        .iter()
        .map(|a| aggregator.authority_clients[&a.name].clone())
        .collect();

    // Create a shared counter.
    let gas_ref = get_latest_ref(authority_clients[0].clone(), gas_objects[0].id()).await;
    let create_counter_txn = TestTransactionBuilder::new(addr, gas_ref, rgp)
        .call_counter_create(package)
        .build_and_sign(&key);
    let create_counter_executable =
        create_executable_for_test(&authority_clients, &create_counter_txn);
    for authority in authorities.iter().take(3) {
        assign_versions_and_schedule(authority, &create_counter_executable).await;
    }
    for authority in authorities.iter().take(3) {
        authority
            .get_transaction_cache_reader()
            .notify_read_executed_effects("", &[*create_counter_executable.digest()])
            .await
            .pop()
            .unwrap();
    }

    // Executing this transaction on the last authority should succeed.
    // The executable was already created, so we just need to execute it.
    assign_versions_and_schedule(&authorities[3], &create_counter_executable).await;
    let create_counter_effects = authorities[3]
        .get_transaction_cache_reader()
        .notify_read_executed_effects("", &[*create_counter_executable.digest()])
        .await
        .pop()
        .unwrap();
    let (shared_counter_ref, owner) = create_counter_effects.created()[0].clone();
    let Owner::Shared {
        initial_shared_version: shared_counter_initial_version,
    } = owner
    else {
        panic!("Not a shared object! {:?} {:?}", shared_counter_ref, owner);
    };

    // Stop execution on the last authority, to simulate having a backlog.
    authorities[3].shutdown_execution_for_test();
    // Make sure execution driver has exited.
    sleep(Duration::from_secs(1)).await;

    // Sign and try execute 2 txns on the first three authorities. And enqueue them on the last authority.
    // First shared counter txn has input object available on authority 3. So to put a txn in the queue, we
    // will need another txn.
    for gas_object in gas_objects.iter().take(2) {
        let gas_ref = get_latest_ref(authority_clients[0].clone(), gas_object.id()).await;
        let shared_txn = TestTransactionBuilder::new(addr, gas_ref, rgp)
            .call_counter_increment(
                package,
                shared_counter_ref.0,
                shared_counter_initial_version,
            )
            .build_and_sign(&key);
        let shared_executable = create_executable_for_test(&authority_clients, &shared_txn);
        for authority in authorities.iter().take(3) {
            assign_versions_and_schedule(authority, &shared_executable).await;
        }
        assign_versions_and_schedule(&authorities[3], &shared_executable).await;
    }

    // Sleep for 6 seconds to make sure the transaction is old enough since our threshold is 5.
    tokio::time::sleep(Duration::from_secs(6)).await;

    // Trying to sign a new transaction would now fail.
    let gas_ref = get_latest_ref(authority_clients[0].clone(), gas_objects[2].id()).await;
    let shared_txn = TestTransactionBuilder::new(addr, gas_ref, rgp)
        .call_counter_increment(
            package,
            shared_counter_ref.0,
            shared_counter_initial_version,
        )
        .build_and_sign(&key);
    let res = authorities[3]
        .execution_scheduler()
        .check_execution_overload(authorities[3].overload_config(), shared_txn.data());
    let message = format!("{res:?}");
    assert!(
        message.contains("TooOldTransactionPendingOnObject"),
        "{}",
        message
    );
}

// Tests that when validator is in load shedding mode, it can pushback txn signing correctly.
#[tokio::test]
async fn test_authority_txn_signing_pushback() {
    telemetry_subscribers::init_for_testing();

    // Create one sender, one recipient address, and 2 gas objects.
    let (sender, sender_key): (_, AccountKeyPair) = get_key_pair();
    let (recipient, _): (_, AccountKeyPair) = get_key_pair();
    let gas_object1 = Object::with_owner_for_testing(sender);
    let gas_object2 = Object::with_owner_for_testing(sender);

    // Initialize an AuthorityState. Disable overload monitor by setting max_load_shedding_percentage to 0;
    // Set check_system_overload_at_signing to true.
    let overload_config = AuthorityOverloadConfig {
        check_system_overload_at_signing: true,
        max_load_shedding_percentage: 0,
        ..Default::default()
    };
    let authority_state = TestAuthorityBuilder::new()
        .with_authority_overload_config(overload_config)
        .build()
        .await;
    authority_state
        .insert_genesis_objects(&[gas_object1.clone(), gas_object2.clone()])
        .await;

    // Create a validator service around the `authority_state`.
    let epoch_store = authority_state.epoch_store_for_testing();
    let consensus_adapter = Arc::new(ConsensusAdapter::new(
        Arc::new(MockConsensusClient::new()),
        CheckpointStore::new_for_tests(),
        authority_state.name,
        Arc::new(ConnectionMonitorStatusForTests {}),
        100_000,
        100_000,
        None,
        None,
        ConsensusAdapterMetrics::new_test(),
        epoch_store.protocol_config().clone(),
    ));
    let validator_service = Arc::new(ValidatorService::new_for_tests(
        authority_state.clone(),
        consensus_adapter,
        Arc::new(ValidatorServiceMetrics::new_for_tests()),
    ));

    // First, create a transaction to transfer `gas_object1` to `recipient`.
    let rgp = authority_state.reference_gas_price_for_testing().unwrap();
    let tx = make_transfer_object_transaction(
        gas_object1.compute_object_reference(),
        gas_object2.compute_object_reference(),
        sender,
        &sender_key,
        recipient,
        rgp,
    );

    // Manually make the authority into overload state and reject 100% of traffic.
    authority_state.overload_info.set_overload(100);

    // Transaction should be rejected with ValidatorOverloadedRetryAfter error.
    // Overload is checked early, before object locking.
    let result = validator_service.handle_transaction_for_testing_with_overload_check(tx.clone());
    assert!(matches!(
        result.unwrap_err().into_inner(),
        SuiErrorKind::ValidatorOverloadedRetryAfter { .. }
    ));

    // Verify that objects are NOT locked when overload error is returned early.
    let lock = authority_state
        .get_transaction_lock(&gas_object1.compute_object_reference(), &epoch_store)
        .await
        .unwrap();
    assert!(
        lock.is_none(),
        "Objects should not be locked when overload is triggered early"
    );

    // Clear the authority overload status.
    authority_state.overload_info.clear_overload();

    // Now the transaction can be successfully processed.
    let result = validator_service.handle_transaction_for_testing_with_overload_check(tx.clone());
    assert!(result.is_ok());

    // Verify the object is now locked by the transaction.
    // We use get_lock() instead of get_transaction_lock() because handle_vote_transaction
    // uses sign=false, which doesn't store the signed transaction.
    let lock_status = authority_state
        .get_object_cache_reader()
        .get_lock(gas_object1.compute_object_reference(), &epoch_store)
        .unwrap();
    assert!(
        matches!(lock_status, ObjectLockStatus::LockedToTx { locked_by_tx } if locked_by_tx.tx_digest == *tx.digest()),
        "Object should be locked to the transaction after successful processing"
    );
}
