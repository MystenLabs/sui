// Copyright (c) 2021, Facebook, Inc. and its affiliates
// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::authority::authority_test_utils::certify_shared_obj_transaction_no_execution;
use crate::authority::shared_object_congestion_tracker::SharedObjectCongestionTracker;
use crate::authority::{AuthorityState, ExecutionEnv};
use crate::consensus_test_utils;
use crate::consensus_test_utils::TestConsensusCommit;
use crate::{
    authority::{
        authority_tests::{build_programmable_transaction, execute_programmable_transaction},
        move_integration_tests::build_and_publish_test_package,
        test_authority_builder::TestAuthorityBuilder,
    },
    move_call,
};
use move_core_types::ident_str;
use std::sync::Arc;
use sui_macros::{register_fail_point_arg, sim_test};
use sui_protocol_config::{
    Chain, ExecutionTimeEstimateParams, PerObjectCongestionControlMode, ProtocolConfig,
    ProtocolVersion,
};
use sui_types::digests::TransactionDigest;
use sui_types::effects::{InputConsensusObject, TransactionEffectsAPI};
use sui_types::executable_transaction::VerifiedExecutableTransaction;
use sui_types::messages_consensus::ConsensusTransaction;
use sui_types::transaction::{CertifiedTransaction, VerifiedTransaction};
use sui_types::transaction::{ObjectArg, SharedObjectMutability};
use sui_types::{
    base_types::{ObjectID, ObjectRef, SequenceNumber, SuiAddress},
    crypto::{AccountKeyPair, get_key_pair},
    execution_status::{CongestedObjects, ExecutionFailureStatus},
    object::Object,
    programmable_transaction_builder::ProgrammableTransactionBuilder,
};

pub const TEST_ONLY_GAS_PRICE: u64 = 1000;
pub const TEST_ONLY_GAS_UNIT: u64 = 10_000;

// Note that TestSetup is currently purposely created for test_congestion_control_execution_cancellation.
struct TestSetup {
    setup_authority_state: Arc<AuthorityState>,
    protocol_config: ProtocolConfig,
    sender: SuiAddress,
    sender_key: AccountKeyPair,
    package: ObjectRef,
    gas_object_id: ObjectID,
}

impl TestSetup {
    async fn new() -> Self {
        let (sender, sender_key): (_, AccountKeyPair) = get_key_pair();

        let mut protocol_config =
            ProtocolConfig::get_for_version(ProtocolVersion::max(), Chain::Unknown);
        // Use ExecutionTimeEstimate mode with parameters that will cause congestion
        protocol_config.set_per_object_congestion_control_mode_for_testing(
            PerObjectCongestionControlMode::ExecutionTimeEstimate(ExecutionTimeEstimateParams {
                target_utilization: 0, // 0% utilization means budget of 0, so any cost will exceed it
                allowed_txn_cost_overage_burst_limit_us: 0,
                max_estimate_us: u64::MAX,
                randomness_scalar: 100,
                stored_observations_num_included_checkpoints: 10,
                stored_observations_limit: u64::MAX,
                stake_weighted_median_threshold: 0,
                default_none_duration_for_new_keys: false,
                observations_chunk_size: None,
            }),
        );

        // Set max deferral rounds to 0 to test cancellation. All deferred transactions will be cancelled.
        protocol_config.set_max_deferral_rounds_for_congestion_control_for_testing(0);

        let setup_authority_state = TestAuthorityBuilder::new()
            .with_reference_gas_price(TEST_ONLY_GAS_PRICE)
            .with_protocol_config(protocol_config.clone())
            .build()
            .await;

        let gas_object_id = ObjectID::random();
        let gas_object = Object::with_id_owner_for_testing(gas_object_id, sender);
        setup_authority_state
            .insert_genesis_object(gas_object.clone())
            .await;

        let package = build_and_publish_test_package(
            &setup_authority_state,
            &sender,
            &sender_key,
            &gas_object_id,
            "congestion_control",
            false,
        )
        .await;

        Self {
            setup_authority_state,
            protocol_config,
            sender,
            sender_key,
            package,
            gas_object_id,
        }
    }

    // Creates a shared object in `setup_authority_state` and returns the object reference.
    async fn create_shared_object(&self) -> ObjectRef {
        let mut builder = ProgrammableTransactionBuilder::new();
        move_call! {
            builder,
            (self.package.0)::congestion_control::create_shared()
        };
        let pt = builder.finish();

        let create_shared_object_effects = execute_programmable_transaction(
            &self.setup_authority_state,
            &self.gas_object_id,
            &self.sender,
            &self.sender_key,
            pt,
            TEST_ONLY_GAS_UNIT,
        )
        .await
        .unwrap();
        assert!(
            create_shared_object_effects.status().is_ok(),
            "Execution error {:?}",
            create_shared_object_effects.status()
        );
        assert_eq!(create_shared_object_effects.created().len(), 1);
        create_shared_object_effects.created()[0].0
    }

    // Creates an owned object in `setup_authority_state` and returns the object reference.
    async fn create_owned_object(&self) -> ObjectRef {
        let mut builder = ProgrammableTransactionBuilder::new();
        move_call! {
            builder,
            (self.package.0)::congestion_control::create_owned()
        };
        let pt = builder.finish();

        let create_owned_object_effects = execute_programmable_transaction(
            &self.setup_authority_state,
            &self.gas_object_id,
            &self.sender,
            &self.sender_key,
            pt,
            TEST_ONLY_GAS_UNIT,
        )
        .await
        .unwrap();
        assert!(
            create_owned_object_effects.status().is_ok(),
            "Execution error {:?}",
            create_owned_object_effects.status()
        );
        assert_eq!(create_owned_object_effects.created().len(), 1);
        create_owned_object_effects.created()[0].0
    }

    // Converts an object to a genesis object by setting its previous_transaction to a genesis marker.
    fn convert_to_genesis_obj(obj: Object) -> Object {
        let mut genesis_obj = obj.clone();
        genesis_obj.previous_transaction = TransactionDigest::genesis_marker();
        genesis_obj
    }

    // Returns a list of objects that can be used as genesis object for a brand new authority state,
    // including the gas object, the package object, and the objects passed in `objects`.
    async fn create_genesis_objects_for_new_authority_state(
        &self,
        objects: &[ObjectID],
    ) -> Vec<Object> {
        let mut genesis_objects = Vec::new();
        genesis_objects.push(TestSetup::convert_to_genesis_obj(
            self.setup_authority_state
                .get_object(&self.package.0)
                .await
                .unwrap(),
        ));
        genesis_objects.push(TestSetup::convert_to_genesis_obj(
            self.setup_authority_state
                .get_object(&self.gas_object_id)
                .await
                .unwrap(),
        ));

        for obj in objects {
            genesis_objects.push(TestSetup::convert_to_genesis_obj(
                self.setup_authority_state.get_object(obj).await.unwrap(),
            ));
        }
        genesis_objects
    }
}

// Tests execution aspect of cancelled transaction due to shared object congestion. Mainly tests that
//   1. Cancelled transaction should return correct error status.
//   2. Executing cancelled transaction with effects should result in the same transaction cancellation.
#[sim_test]
async fn test_congestion_control_execution_cancellation() {
    telemetry_subscribers::init_for_testing();

    // Creates a authority state with 2 shared object and 1 owned object. We use this setup
    // to initialize two more authority states: one tests cancellation execution, and one tests
    // executing cancelled transaction from effect.
    let test_setup = TestSetup::new().await;
    let shared_object_1 = test_setup.create_shared_object().await;
    let shared_object_2 = test_setup.create_shared_object().await;
    let owned_object = test_setup.create_owned_object().await;

    // Gets objects that can be used as genesis objects for new authority states.
    let genesis_objects = test_setup
        .create_genesis_objects_for_new_authority_state(&[
            shared_object_1.0,
            shared_object_2.0,
            owned_object.0,
        ])
        .await;

    // Creates two authority states with the same genesis objects for the actual test.
    let authority_state = TestAuthorityBuilder::new()
        .with_reference_gas_price(TEST_ONLY_GAS_PRICE)
        .with_protocol_config(test_setup.protocol_config.clone())
        .build()
        .await;
    authority_state
        .insert_genesis_objects(&genesis_objects)
        .await;
    let authority_state_2 = TestAuthorityBuilder::new()
        .with_reference_gas_price(TEST_ONLY_GAS_PRICE)
        .with_protocol_config(test_setup.protocol_config.clone())
        .build()
        .await;
    authority_state_2
        .insert_genesis_objects(&genesis_objects)
        .await;

    // Initialize shared object queue so that any transaction touches shared_object_1 should result in congestion and cancellation.
    // Set initial cost of 10 for shared_object_1, which with 0% target_utilization and 0 burst limit
    // will exceed the budget and cause congestion.
    register_fail_point_arg("initial_congestion_tracker", move || {
        Some(SharedObjectCongestionTracker::new(
            [(shared_object_1.0, 10)],
            ExecutionTimeEstimateParams {
                target_utilization: 0,
                allowed_txn_cost_overage_burst_limit_us: 0,
                max_estimate_us: u64::MAX,
                randomness_scalar: 100,
                stored_observations_num_included_checkpoints: 10,
                stored_observations_limit: u64::MAX,
                stake_weighted_median_threshold: 0,
                default_none_duration_for_new_keys: false,
                observations_chunk_size: None,
            },
            false,
        ))
    });

    // Set up ConsensusHandler for the authority_state
    let consensus_setup =
        consensus_test_utils::setup_consensus_handler_for_testing(&authority_state).await;
    let mut consensus_handler = consensus_setup.consensus_handler;
    let captured_transactions = consensus_setup.captured_transactions;

    // Create a transaction that touches shared_object_1, shared_object_2 and an owned object.
    let mut txn_builder = ProgrammableTransactionBuilder::new();
    let arg1 = txn_builder
        .obj(ObjectArg::SharedObject {
            id: shared_object_1.0,
            initial_shared_version: shared_object_1.1,
            mutability: SharedObjectMutability::Mutable,
        })
        .unwrap();
    let arg2 = txn_builder
        .obj(ObjectArg::SharedObject {
            id: shared_object_2.0,
            initial_shared_version: shared_object_2.1,
            mutability: SharedObjectMutability::Mutable,
        })
        .unwrap();
    let owned_object_ref = authority_state
        .get_object(&owned_object.0)
        .await
        .unwrap()
        .compute_object_reference();
    let arg3 = txn_builder
        .obj(ObjectArg::ImmOrOwnedObject(owned_object_ref))
        .unwrap();
    move_call! {
        txn_builder,
        (test_setup.package.0)::congestion_control::increment(arg1, arg2, arg3)
    };
    let pt = txn_builder.finish();
    let congested_tx = build_programmable_transaction(
        &authority_state,
        &test_setup.gas_object_id,
        &test_setup.sender,
        &test_setup.sender_key,
        pt,
        TEST_ONLY_GAS_UNIT,
    )
    .await
    .unwrap();

    let verified_tx_2 = VerifiedTransaction::new_unchecked(congested_tx.clone());

    let epoch_store_2 = authority_state_2.load_epoch_store_one_call_per_task();
    let response = authority_state_2
        .handle_transaction(&epoch_store_2, verified_tx_2.clone())
        .await
        .unwrap();
    let vote = response.status.into_signed_for_testing();

    let committee = authority_state.clone_committee_for_testing();
    let cert = CertifiedTransaction::new(verified_tx_2.into_message(), vec![vote], &committee)
        .unwrap()
        .try_into_verified_for_testing(&committee, &Default::default())
        .unwrap();

    let consensus_transactions = vec![ConsensusTransaction::new_certificate_message(
        &authority_state.name,
        cert.clone().into(),
    )];
    let commit = TestConsensusCommit::new(consensus_transactions, 1, 0, 0);

    consensus_handler.handle_consensus_commit(commit).await;

    // Wait for captured transactions to be available
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;

    // Get the captured transactions
    let (scheduled_txns, assigned_tx_and_versions) = {
        let mut captured = captured_transactions.lock();
        assert!(
            !captured.is_empty(),
            "Expected transactions to be scheduled"
        );
        let (scheduled_txns, assigned_tx_and_versions, _) = captured.remove(0);
        (scheduled_txns, assigned_tx_and_versions)
    };

    // Both prologue and the cancelled transaction should be scheduled
    // The cancelled transaction will abort during execution
    assert_eq!(
        scheduled_txns.len(),
        3,
        "Expected prologue + cancelled transaction + settlement"
    );

    // Now execute the cancelled transaction to get the effects
    // Find the assigned versions for our specific transaction
    let cert_key = cert.key();
    let assigned_versions = assigned_tx_and_versions
        .into_map()
        .get(&cert_key)
        .expect("Transaction should have assigned versions")
        .clone();

    let execution_env = ExecutionEnv::new().with_assigned_versions(assigned_versions);
    let (effects, execution_error) = authority_state
        .try_execute_for_test(&cert, execution_env)
        .await;

    // Transaction should be cancelled with `shared_object_1` as the congested object.
    assert!(execution_error.is_some());
    assert_eq!(
        execution_error.unwrap().to_execution_status().0,
        ExecutionFailureStatus::ExecutionCancelledDueToSharedObjectCongestion {
            congested_objects: CongestedObjects(vec![shared_object_1.0]),
        }
    );
    let effects = effects.data();

    // Tests consensus object versions in effects are set correctly.
    assert_eq!(
        effects.input_consensus_objects(),
        vec![
            InputConsensusObject::Cancelled(shared_object_1.0, SequenceNumber::CONGESTED),
            InputConsensusObject::Cancelled(shared_object_2.0, SequenceNumber::CANCELLED_READ)
        ]
    );

    // Run the same transaction in `authority_state_2`, but using the above effects for the execution.
    let (cert, _) = certify_shared_obj_transaction_no_execution(&authority_state_2, congested_tx)
        .await
        .unwrap();
    let assigned_versions = authority_state_2
        .epoch_store_for_testing()
        .acquire_shared_version_assignments_from_effects(
            &VerifiedExecutableTransaction::new_from_certificate(cert.clone()),
            effects,
            None,
            authority_state_2.get_object_cache_reader().as_ref(),
        )
        .unwrap();
    let execution_env = ExecutionEnv::new().with_assigned_versions(assigned_versions);
    let (effects_2, execution_error) = authority_state_2
        .try_execute_for_test(&cert, execution_env)
        .await;

    // Should result in the same cancellation.
    assert_eq!(
        execution_error.unwrap().to_execution_status().0,
        ExecutionFailureStatus::ExecutionCancelledDueToSharedObjectCongestion {
            congested_objects: CongestedObjects(vec![shared_object_1.0]),
        }
    );
    assert_eq!(effects, effects_2.data());
}
