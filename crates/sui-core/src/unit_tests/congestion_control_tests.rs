// Copyright (c) 2021, Facebook, Inc. and its affiliates
// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use sui_types::{
    base_types::{ObjectID, ObjectRef, SequenceNumber, SuiAddress},
    crypto::{get_key_pair, AccountKeyPair},
    effects::TransactionEffects,
    execution_status::{CommandArgumentError, ExecutionFailureStatus},
    object::Object,
    programmable_transaction_builder::ProgrammableTransactionBuilder,
    transaction::{ProgrammableTransaction, Transaction},
};

use crate::authority::authority_test_utils::execute_sequenced_certificate_to_effects;
use crate::{
    authority::{
        authority_tests::{
            build_programmable_transaction, certify_shared_obj_transaction_no_execution,
            enqueue_all_and_execute_all, execute_programmable_transaction,
            execute_programmable_transaction_with_shared,
        },
        move_integration_tests::build_and_publish_test_package,
        test_authority_builder::TestAuthorityBuilder,
        AuthorityState,
    },
    move_call,
};
use move_core_types::ident_str;
use sui_protocol_config::{Chain, PerObjectCongestionControlMode, ProtocolConfig, ProtocolVersion};
use sui_types::base_types::TransactionDigest;
use sui_types::committee::EpochId;
use sui_types::effects::TransactionEffectsAPI;
use sui_types::error::{ExecutionError, SuiError};
use sui_types::execution_status::ExecutionFailureStatus::{
    InputObjectDeleted, SharedObjectOperationNotAllowed,
};
use sui_types::transaction::{ObjectArg, VerifiedCertificate};

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
    shared_object_1: &ObjectRef,
    shared_object_2: &ObjectRef,
    owned_object: &ObjectRef,
) -> TransactionEffects {
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
    execute_programmable_transaction_with_shared(
        authority_state,
        gas_object_id,
        &sender,
        &sender_key,
        pt,
        TEST_ONLY_GAS_UNIT_FOR_CONGESTED_TX,
    )
    .await
    .unwrap()
}

#[tokio::test]
async fn test_congestion_control_execution_cancellation() {
    telemetry_subscribers::init_for_testing();
    let (sender, sender_key): (_, AccountKeyPair) = get_key_pair();

    let mut protocol_config =
        ProtocolConfig::get_for_version(ProtocolVersion::max(), Chain::Unknown);
    protocol_config
        .set_per_object_congestion_control_mode(PerObjectCongestionControlMode::TotalGasBudget);
    protocol_config.set_max_accumulated_txn_cost_per_object_in_checkpoint(
        TEST_ONLY_GAS_PRICE * TEST_ONLY_GAS_UNIT_FOR_CONGESTED_TX - 1,
    );
    protocol_config.set_max_deferral_rounds_for_congestion_control(0);
    let authority_state = TestAuthorityBuilder::new()
        .with_reference_gas_price(TEST_ONLY_GAS_PRICE)
        .with_protocol_config(protocol_config)
        .build()
        .await;

    let mut gas_object_ids = vec![];
    for _ in 0..20 {
        let gas_object_id = ObjectID::random();
        let gas_object = Object::with_id_owner_for_testing(gas_object_id, sender);
        authority_state.insert_genesis_object(gas_object).await;
        gas_object_ids.push(gas_object_id);
    }

    let package = build_and_publish_test_package(
        &authority_state,
        &sender,
        &sender_key,
        &gas_object_ids[0],
        "congestion_control",
        false,
    )
    .await;

    let shared_object_1_initial_ref = create_shared_object(
        &authority_state,
        &package,
        &sender,
        &sender_key,
        &gas_object_ids[0],
    )
    .await;

    let shared_object_2_initial_ref = create_shared_object(
        &authority_state,
        &package,
        &sender,
        &sender_key,
        &gas_object_ids[0],
    )
    .await;

    let owned_object = create_owned_object(
        &authority_state,
        &package,
        &sender,
        &sender_key,
        &gas_object_ids[0],
    )
    .await;

    println!(
        "Created objects {:?}\n {:?}\n {:?}\n",
        shared_object_1_initial_ref, shared_object_2_initial_ref, owned_object
    );

    let effects = update_objects(
        &authority_state,
        &package,
        &sender,
        &sender_key,
        &gas_object_ids[0],
        &shared_object_1_initial_ref,
        &shared_object_2_initial_ref,
        &owned_object,
    )
    .await;

    println!("ZZZZZZ {:?}", effects);
}
