// Copyright (c) 2021, Facebook, Inc. and its affiliates
// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::authority::shared_object_congestion_tracker::SharedObjectCongestionTracker;
use crate::{
    authority::{
        authority_tests::{
            build_programmable_transaction, certify_shared_obj_transaction_no_execution,
            execute_programmable_transaction, send_and_confirm_transaction_,
        },
        move_integration_tests::build_and_publish_test_package,
        test_authority_builder::TestAuthorityBuilder,
        AuthorityState,
    },
    move_call,
};
use move_core_types::ident_str;
use std::sync::Arc;
use sui_macros::{register_fail_point_arg, sim_test};
use sui_protocol_config::{Chain, PerObjectCongestionControlMode, ProtocolConfig, ProtocolVersion};
use sui_types::base_types::ConsensusObjectSequenceKey;
use sui_types::digests::TransactionDigest;
use sui_types::effects::{InputSharedObject, TransactionEffectsAPI};
use sui_types::executable_transaction::VerifiedExecutableTransaction;
use sui_types::transaction::{ObjectArg, Transaction};
use sui_types::{
    base_types::{ObjectID, ObjectRef, SequenceNumber, SuiAddress},
    crypto::{get_key_pair, AccountKeyPair},
    effects::TransactionEffects,
    execution_status::{CongestedObjects, ExecutionFailureStatus, ExecutionStatus},
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
        protocol_config.set_per_object_congestion_control_mode_for_testing(
            PerObjectCongestionControlMode::TotalGasBudget,
        );

        // Set shared object congestion control such that it only allows 1 transaction to go through.
        let max_accumulated_txn_cost_per_object_in_commit =
            TEST_ONLY_GAS_PRICE * TEST_ONLY_GAS_UNIT;
        protocol_config.set_max_accumulated_txn_cost_per_object_in_narwhal_commit_for_testing(
            max_accumulated_txn_cost_per_object_in_commit,
        );
        protocol_config.set_max_accumulated_txn_cost_per_object_in_mysticeti_commit_for_testing(
            max_accumulated_txn_cost_per_object_in_commit,
        );

        // Set max deferral rounds to 0 to testr cancellation. All deferred transactions will be cancelled.
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

// Creates a transaction that touchs the shared objects `shared_object_1` and `shared_object_2`, and `owned_object`,
// and executes the transaction in `authority_state`. Returns the transaction and the effects of the execution.
async fn update_objects(
    authority_state: &AuthorityState,
    package: &ObjectRef,
    sender: &SuiAddress,
    sender_key: &AccountKeyPair,
    gas_object_id: &ObjectID,
    shared_object_1: &ConsensusObjectSequenceKey,
    shared_object_2: &ConsensusObjectSequenceKey,
    owned_object: &ObjectRef,
) -> (Transaction, TransactionEffects) {
    let mut txn_builder = ProgrammableTransactionBuilder::new();
    let arg1 = txn_builder
        .obj(ObjectArg::SharedObject {
            id: shared_object_1.0,
            initial_shared_version: shared_object_1.1,
            mutable: true,
        })
        .unwrap();
    let arg2 = txn_builder
        .obj(ObjectArg::SharedObject {
            id: shared_object_2.0,
            initial_shared_version: shared_object_2.1,
            mutable: true,
        })
        .unwrap();
    let arg3 = txn_builder
        .obj(ObjectArg::ImmOrOwnedObject(*owned_object))
        .unwrap();
    move_call! {
        txn_builder,
        (package.0)::congestion_control::increment(arg1, arg2, arg3)
    };
    let pt = txn_builder.finish();
    let transaction = build_programmable_transaction(
        authority_state,
        gas_object_id,
        sender,
        sender_key,
        pt,
        TEST_ONLY_GAS_UNIT,
    )
    .await
    .unwrap();

    let execution_effects =
        send_and_confirm_transaction_(authority_state, None, transaction.clone(), true)
            .await
            .unwrap()
            .1
            .into_data();
    (transaction, execution_effects)
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
    register_fail_point_arg("initial_congestion_tracker", move || {
        Some(SharedObjectCongestionTracker::new(
            [(shared_object_1.0, 10)],
            PerObjectCongestionControlMode::TotalGasBudget,
            Some(
                test_setup
                    .protocol_config
                    .max_accumulated_txn_cost_per_object_in_mysticeti_commit(),
            ),
            Some(1000), // Not used.
            None,       // Not used.
            0,          // Disable overage.
            0,
        ))
    });

    // Runs a transaction that touches shared_object_1, shared_object_2 and an owned object.
    let (congested_tx, effects) = update_objects(
        &authority_state,
        &test_setup.package,
        &test_setup.sender,
        &test_setup.sender_key,
        &test_setup.gas_object_id,
        &(shared_object_1.0, shared_object_1.1),
        &(shared_object_2.0, shared_object_2.1),
        &authority_state
            .get_object(&owned_object.0)
            .await
            .unwrap()
            .compute_object_reference(),
    )
    .await;

    // Transaction should be cancelled with `shared_object_1` as the congested object.
    assert_eq!(
        effects.status(),
        &ExecutionStatus::Failure {
            error: ExecutionFailureStatus::ExecutionCancelledDueToSharedObjectCongestion {
                congested_objects: CongestedObjects(vec![shared_object_1.0]),
            },
            command: None
        }
    );

    // Tests shared object versions in effects are set correctly.
    assert_eq!(
        effects.input_shared_objects(),
        vec![
            InputSharedObject::Cancelled(shared_object_1.0, SequenceNumber::CONGESTED),
            InputSharedObject::Cancelled(shared_object_2.0, SequenceNumber::CANCELLED_READ)
        ]
    );

    // Run the same transaction in `authority_state_2`, but using the above effects for the execution.
    let cert = certify_shared_obj_transaction_no_execution(&authority_state_2, congested_tx)
        .await
        .unwrap();
    authority_state_2
        .epoch_store_for_testing()
        .acquire_shared_version_assignments_from_effects(
            &VerifiedExecutableTransaction::new_from_certificate(cert.clone()),
            &effects,
            authority_state_2.get_object_cache_reader().as_ref(),
        )
        .unwrap();
    let (effects_2, execution_error) = authority_state_2.try_execute_for_test(&cert).await.unwrap();

    // Should result in the same cancellation.
    assert_eq!(
        execution_error.unwrap().to_execution_status().0,
        ExecutionFailureStatus::ExecutionCancelledDueToSharedObjectCongestion {
            congested_objects: CongestedObjects(vec![shared_object_1.0]),
        }
    );
    assert_eq!(&effects, effects_2.data())
}
