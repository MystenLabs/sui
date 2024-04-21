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
pub const TEST_ONLY_GAS_UNIT_FOR_SUCCESSFUL_TX: u64 = 10_000;
pub const TEST_ONLY_GAS_UNIT_FOR_CONGESTED_TX: u64 = 90_000;

async fn create_shared_object(
    authority_state: &AuthorityState,
    package: &ObjectRef,
    sender: &SuiAddress,
    sender_key: &AccountKeyPair,
    gas_object_id: &ObjectID,
) -> ObjectRef {
    let mut builder = ProgrammableTransactionBuilder::new();
    move_call! {
        builder,
        (package.0)::congestion_control::create_shared()
    };
    let pt = builder.finish();

    let create_shared_object_effects = execute_programmable_transaction(
        authority_state,
        gas_object_id,
        sender,
        sender_key,
        pt,
        TEST_ONLY_GAS_UNIT_FOR_SUCCESSFUL_TX,
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

async fn create_owned_object(
    authority_state: &AuthorityState,
    package: &ObjectRef,
    sender: &SuiAddress,
    sender_key: &AccountKeyPair,
    gas_object_id: &ObjectID,
) -> ObjectRef {
    let mut builder = ProgrammableTransactionBuilder::new();
    move_call! {
        builder,
        (package.0)::congestion_control::create_owned()
    };
    let pt = builder.finish();

    let create_owned_object_effects = execute_programmable_transaction(
        authority_state,
        gas_object_id,
        sender,
        sender_key,
        pt,
        TEST_ONLY_GAS_UNIT_FOR_SUCCESSFUL_TX,
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

async fn update_objects(
    authority_state: &AuthorityState,
    package: &ObjectRef,
    sender: &SuiAddress,
    sender_key: &AccountKeyPair,
    gas_object_id: &ObjectID,
    shared_object_1: &(ObjectID, SequenceNumber),
    shared_object_2: &(ObjectID, SequenceNumber),
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
        TEST_ONLY_GAS_UNIT_FOR_CONGESTED_TX,
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

struct TestSetup {
    setup_authority_state: Arc<AuthorityState>,
    protocol_config: ProtocolConfig,
    sender: SuiAddress,
    sender_key: AccountKeyPair,
    package: ObjectRef,
    gas_object_id: ObjectID,
    shared_object_1: (ObjectID, SequenceNumber),
    shared_object_2: (ObjectID, SequenceNumber),
    owned_object: (ObjectID, SequenceNumber),
}

impl TestSetup {
    async fn new() -> Self {
        let (sender, sender_key): (_, AccountKeyPair) = get_key_pair();

        let mut protocol_config =
            ProtocolConfig::get_for_version(ProtocolVersion::max(), Chain::Unknown);
        protocol_config
            .set_per_object_congestion_control_mode(PerObjectCongestionControlMode::TotalGasBudget);
        let max_accumulated_txn_cost_per_object_in_checkpoint =
            TEST_ONLY_GAS_PRICE * TEST_ONLY_GAS_UNIT_FOR_CONGESTED_TX;
        protocol_config.set_max_accumulated_txn_cost_per_object_in_checkpoint(
            max_accumulated_txn_cost_per_object_in_checkpoint,
        );
        protocol_config.set_max_deferral_rounds_for_congestion_control(0);

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

        let owned_object_ref = create_owned_object(
            &setup_authority_state,
            &package,
            &sender,
            &sender_key,
            &gas_object_id,
        )
        .await;

        let shared_object_1_initial_ref = create_shared_object(
            &setup_authority_state,
            &package,
            &sender,
            &sender_key,
            &gas_object_id,
        )
        .await;

        let shared_object_2_initial_ref = create_shared_object(
            &setup_authority_state,
            &package,
            &sender,
            &sender_key,
            &gas_object_id,
        )
        .await;

        Self {
            setup_authority_state,
            protocol_config,
            sender,
            sender_key,
            package,
            gas_object_id,
            shared_object_1: (shared_object_1_initial_ref.0, shared_object_1_initial_ref.1),
            shared_object_2: (shared_object_2_initial_ref.0, shared_object_2_initial_ref.1),
            owned_object: (owned_object_ref.0, owned_object_ref.1),
        }
    }

    fn convert_to_genesis_obj(obj: Object) -> Object {
        let mut genesis_obj = obj.clone();
        genesis_obj.previous_transaction = TransactionDigest::genesis_marker();
        genesis_obj
    }

    async fn create_genesis_objects(&self) -> Vec<Object> {
        let mut genesis_objects = Vec::new();

        genesis_objects.push(TestSetup::convert_to_genesis_obj(
            self.setup_authority_state
                .get_object(&self.shared_object_1.0)
                .await
                .unwrap()
                .unwrap(),
        ));
        genesis_objects.push(TestSetup::convert_to_genesis_obj(
            self.setup_authority_state
                .get_object(&self.shared_object_2.0)
                .await
                .unwrap()
                .unwrap(),
        ));
        genesis_objects.push(TestSetup::convert_to_genesis_obj(
            self.setup_authority_state
                .get_object(&self.owned_object.0)
                .await
                .unwrap()
                .unwrap(),
        ));
        genesis_objects.push(TestSetup::convert_to_genesis_obj(
            self.setup_authority_state
                .get_object(&self.package.0)
                .await
                .unwrap()
                .unwrap(),
        ));
        genesis_objects.push(TestSetup::convert_to_genesis_obj(
            self.setup_authority_state
                .get_object(&self.gas_object_id)
                .await
                .unwrap()
                .unwrap(),
        ));
        genesis_objects
    }
}

#[sim_test]
async fn test_congestion_control_execution_cancellation() {
    telemetry_subscribers::init_for_testing();
    let test_setup = TestSetup::new().await;
    let genesis_objects = test_setup.create_genesis_objects().await;

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

    register_fail_point_arg("initial_congestion_tracker", move || {
        Some(
            SharedObjectCongestionTracker::new_with_initial_value_for_test(&[(
                test_setup.shared_object_1.0,
                10,
            )]),
        )
    });

    let (congested_tx, effects) = update_objects(
        &authority_state,
        &test_setup.package,
        &test_setup.sender,
        &test_setup.sender_key,
        &test_setup.gas_object_id,
        &test_setup.shared_object_1,
        &test_setup.shared_object_2,
        &authority_state
            .get_object(&test_setup.owned_object.0)
            .await
            .unwrap()
            .unwrap()
            .compute_object_reference(),
    )
    .await;

    assert_eq!(
        effects.status(),
        &ExecutionStatus::Failure {
            error: ExecutionFailureStatus::ExecutionCancelledDueToSharedObjectCongestion {
                congested_objects: CongestedObjects(vec![test_setup.shared_object_1.0]),
            },
            command: None
        }
    );

    assert_eq!(
        effects.input_shared_objects(),
        vec![
            InputSharedObject::Cancelled(test_setup.shared_object_1.0, SequenceNumber::CONGESTED),
            InputSharedObject::Cancelled(
                test_setup.shared_object_2.0,
                SequenceNumber::CANCELLED_READ
            )
        ]
    );

    let cert = certify_shared_obj_transaction_no_execution(&authority_state_2, congested_tx)
        .await
        .unwrap();
    authority_state_2
        .epoch_store_for_testing()
        .acquire_shared_locks_from_effects(
            &VerifiedExecutableTransaction::new_from_certificate(cert.clone()),
            &effects,
            authority_state_2.get_cache_reader().as_ref(),
        )
        .await
        .unwrap();
    let (effects_2, execution_error) = authority_state_2.try_execute_for_test(&cert).await.unwrap();
    assert_eq!(
        execution_error.unwrap().to_execution_status().0,
        ExecutionFailureStatus::ExecutionCancelledDueToSharedObjectCongestion {
            congested_objects: CongestedObjects(vec![test_setup.shared_object_1.0]),
        }
    );
    assert_eq!(&effects, effects_2.data())
}
