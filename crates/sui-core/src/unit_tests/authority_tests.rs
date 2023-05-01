// Copyright (c) 2021, Facebook, Inc. and its affiliates
// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::HashSet;
use std::fs;
use std::{convert::TryInto, env};

use bcs;
use futures::{stream::FuturesUnordered, StreamExt};
use move_binary_format::access::ModuleAccess;
use move_binary_format::{
    file_format::{self, AddressIdentifierIndex, IdentifierIndex, ModuleHandle},
    CompiledModule,
};
use move_core_types::identifier::IdentStr;
use move_core_types::language_storage::StructTag;
use move_core_types::parser::parse_type_tag;
use move_core_types::{
    account_address::AccountAddress, ident_str, identifier::Identifier, language_storage::TypeTag,
};
use rand::{
    distributions::{Distribution, Uniform},
    prelude::StdRng,
    Rng, SeedableRng,
};
use serde_json::json;

use sui_json_rpc_types::{
    SuiArgument, SuiExecutionResult, SuiExecutionStatus, SuiTransactionBlockEffectsAPI, SuiTypeTag,
};
use sui_macros::sim_test;
use sui_protocol_config::{ProtocolConfig, SupportedProtocolVersions};
use sui_types::dynamic_field::DynamicFieldType;
use sui_types::effects::TransactionEffects;
use sui_types::epoch_data::EpochData;
use sui_types::error::UserInputError;
use sui_types::execution_status::{ExecutionFailureStatus, ExecutionStatus};
use sui_types::gas::SuiCostTable;
use sui_types::gas_coin::GasCoin;
use sui_types::object::Data;
use sui_types::programmable_transaction_builder::ProgrammableTransactionBuilder;
use sui_types::sui_system_state::SuiSystemStateWrapper;
use sui_types::utils::{
    to_sender_signed_transaction, to_sender_signed_transaction_with_multi_signers,
};
use sui_types::{
    base_types::dbg_addr,
    crypto::{get_key_pair, Signature},
    crypto::{AccountKeyPair, AuthorityKeyPair, KeypairTraits},
    messages::VerifiedTransaction,
    object::{Owner, GAS_VALUE_FOR_TESTING, OBJECT_START_VERSION},
    MOVE_STDLIB_OBJECT_ID, SUI_FRAMEWORK_OBJECT_ID, SUI_SYSTEM_STATE_OBJECT_ID,
};
use sui_types::{SUI_CLOCK_OBJECT_ID, SUI_CLOCK_OBJECT_SHARED_VERSION};

use crate::authority::move_integration_tests::build_and_publish_test_package_with_upgrade_cap;
use crate::authority::test_authority_builder::TestAuthorityBuilder;
use crate::{
    authority_client::{AuthorityAPI, NetworkAuthorityClient},
    authority_server::AuthorityServer,
    test_utils::init_state_parameters_from_rng,
};

use super::*;

pub use crate::authority::authority_test_utils::*;

pub enum TestCallArg {
    Pure(Vec<u8>),
    Object(ObjectID),
    ObjVec(Vec<ObjectID>),
}

impl TestCallArg {
    pub async fn to_call_arg(
        self,
        builder: &mut ProgrammableTransactionBuilder,
        state: &AuthorityState,
    ) -> Argument {
        match self {
            Self::Pure(value) => builder.input(CallArg::Pure(value)).unwrap(),
            Self::Object(object_id) => builder
                .input(CallArg::Object(
                    Self::call_arg_from_id(object_id, state).await,
                ))
                .unwrap(),
            Self::ObjVec(vec) => {
                let mut refs = vec![];
                for object_id in vec {
                    refs.push(Self::call_arg_from_id(object_id, state).await)
                }
                builder.make_obj_vec(refs).unwrap()
            }
        }
    }

    async fn call_arg_from_id(object_id: ObjectID, state: &AuthorityState) -> ObjectArg {
        let object = state.get_object(&object_id).await.unwrap().unwrap();
        match &object.owner {
            Owner::AddressOwner(_) | Owner::ObjectOwner(_) | Owner::Immutable => {
                ObjectArg::ImmOrOwnedObject(object.compute_object_reference())
            }
            Owner::Shared {
                initial_shared_version,
            } => ObjectArg::SharedObject {
                id: object_id,
                initial_shared_version: *initial_shared_version,
                mutable: true,
            },
        }
    }
}

// TODO break this up into a cleaner set of components. It does a bit too much
// currently
async fn construct_shared_object_transaction_with_sequence_number(
    initial_shared_version_override: Option<SequenceNumber>,
) -> (
    Arc<AuthorityState>,
    Arc<AuthorityState>,
    VerifiedTransaction,
    ObjectID,
    ObjectID,
) {
    let (sender, keypair): (_, AccountKeyPair) = get_key_pair();

    // Initialize an authority with a (owned) gas object and a shared object.
    let gas_object_id = ObjectID::random();
    let (shared_object_id, shared_object) = {
        let (authority, package) =
            init_state_with_ids_and_object_basics(vec![(sender, gas_object_id)]).await;
        let effects = call_move_(
            &authority,
            None,
            &gas_object_id,
            &sender,
            &keypair,
            &package.0,
            "object_basics",
            "share",
            vec![],
            vec![],
            true,
        )
        .await
        .unwrap();
        effects.status().unwrap();
        let shared_object_id = effects.created()[0].0 .0;
        let mut shared_object = authority
            .get_object(&shared_object_id)
            .await
            .unwrap()
            .unwrap();
        if let Some(initial_shared_version) = initial_shared_version_override {
            shared_object
                .data
                .try_as_move_mut()
                .unwrap()
                .increment_version_to(initial_shared_version);
            shared_object.owner = Owner::Shared {
                initial_shared_version,
            };
        }
        shared_object.previous_transaction = TransactionDigest::genesis();
        (shared_object_id, shared_object)
    };
    let initial_shared_version = shared_object.version();

    // Make a sample transaction.
    let (validator, fullnode, package) =
        init_state_with_ids_and_object_basics_with_fullnode(vec![(sender, gas_object_id)]).await;
    validator.insert_genesis_object(shared_object.clone()).await;
    fullnode.insert_genesis_object(shared_object.clone()).await;
    let rgp = validator.reference_gas_price_for_testing().unwrap();
    let gas_object = validator.get_object(&gas_object_id).await.unwrap();
    let gas_object_ref = gas_object.unwrap().compute_object_reference();
    let data = TransactionData::new_move_call(
        sender,
        package.0,
        ident_str!("object_basics").to_owned(),
        ident_str!("set_value").to_owned(),
        /* type_args */ vec![],
        gas_object_ref,
        /* args */
        vec![
            CallArg::Object(ObjectArg::SharedObject {
                id: shared_object_id,
                initial_shared_version,
                mutable: true,
            }),
            CallArg::Pure(16u64.to_le_bytes().to_vec()),
        ],
        TEST_ONLY_GAS_UNIT_FOR_OBJECT_BASICS * rgp,
        rgp,
    )
    .unwrap();
    (
        validator,
        fullnode,
        to_sender_signed_transaction(data, &keypair),
        gas_object_id,
        shared_object_id,
    )
}

#[tokio::test]
async fn test_dry_run_transaction_block() {
    let (validator, fullnode, transaction, gas_object_id, shared_object_id) =
        construct_shared_object_transaction_with_sequence_number(None).await;
    let initial_shared_object_version = validator
        .get_object(&shared_object_id)
        .await
        .unwrap()
        .unwrap()
        .version();

    let transaction_digest = *transaction.digest();

    let (response, _, _, _) = fullnode
        .dry_exec_transaction(
            transaction.data().intent_message().value.clone(),
            transaction_digest,
        )
        .await
        .unwrap();
    assert_eq!(*response.effects.status(), SuiExecutionStatus::Success);
    let gas_usage = response.effects.gas_cost_summary();

    // Make sure that objects are not mutated after dry run.
    let gas_object_version = fullnode
        .get_object(&gas_object_id)
        .await
        .unwrap()
        .unwrap()
        .version();
    assert_eq!(gas_object_version, OBJECT_START_VERSION);
    let shared_object_version = fullnode
        .get_object(&shared_object_id)
        .await
        .unwrap()
        .unwrap()
        .version();
    assert_eq!(shared_object_version, initial_shared_object_version);

    let txn_data = &transaction.data().intent_message().value;
    let txn_data = TransactionData::new_with_gas_coins(
        txn_data.kind().clone(),
        txn_data.sender(),
        vec![],
        txn_data.gas_budget(),
        txn_data.gas_price(),
    );
    let (response, _, _, _) = fullnode
        .dry_exec_transaction(txn_data, transaction_digest)
        .await
        .unwrap();
    let gas_usage_no_gas = response.effects.gas_cost_summary();
    assert_eq!(*response.effects.status(), SuiExecutionStatus::Success);
    assert_eq!(gas_usage, gas_usage_no_gas);
}

#[tokio::test]
async fn test_dry_run_no_gas_big_transfer() {
    let (sender, sender_key): (_, AccountKeyPair) = get_key_pair();
    let recipient = dbg_addr(2);
    let gas_object_id = ObjectID::random();
    let (_, fullnode, _) =
        init_state_with_ids_and_object_basics_with_fullnode(vec![(sender, gas_object_id)]).await;

    let amount = 1_000_000_000u64;
    let mut builder = ProgrammableTransactionBuilder::new();
    builder.transfer_sui(recipient, Some(amount));
    let pt = builder.finish();
    let data = TransactionData::new_programmable(
        sender,
        vec![],
        pt,
        ProtocolConfig::get_for_max_version().max_tx_gas(),
        fullnode.reference_gas_price_for_testing().unwrap(),
    );

    let signed = to_sender_signed_transaction(data, &sender_key);

    let (dry_run_res, _, _, _) = fullnode
        .dry_exec_transaction(
            signed.data().intent_message().value.clone(),
            *signed.digest(),
        )
        .await
        .unwrap();
    assert_eq!(*dry_run_res.effects.status(), SuiExecutionStatus::Success);
}

#[tokio::test]
async fn test_dev_inspect_object_by_bytes() {
    let (sender, sender_key): (_, AccountKeyPair) = get_key_pair();
    let gas_object_id = ObjectID::random();
    let (validator, fullnode, object_basics) =
        init_state_with_ids_and_object_basics_with_fullnode(vec![(sender, gas_object_id)]).await;

    // test normal call
    let DevInspectResults {
        effects, results, ..
    } = call_dev_inspect(
        &fullnode,
        &sender,
        &object_basics.0,
        "object_basics",
        "create",
        vec![],
        vec![
            TestCallArg::Pure(bcs::to_bytes(&(16_u64)).unwrap()),
            TestCallArg::Pure(bcs::to_bytes(&sender).unwrap()),
        ],
    )
    .await
    .unwrap();
    assert_eq!(effects.created().len(), 1);
    // random gas is mutated
    assert_eq!(effects.mutated().len(), 1);
    assert!(effects.deleted().is_empty());
    assert!(effects.gas_cost_summary().computation_cost > 0);
    let mut results = results.unwrap();
    assert_eq!(results.len(), 1);
    let exec_results = results.pop().unwrap();
    let SuiExecutionResult {
        mutable_reference_outputs,
        return_values,
    } = exec_results;
    assert!(mutable_reference_outputs.is_empty());
    assert!(return_values.is_empty());
    let dev_inspect_gas_summary = effects.gas_cost_summary().clone();

    // actually make the call to make an object
    let effects = call_move_(
        &validator,
        Some(&fullnode),
        &gas_object_id,
        &sender,
        &sender_key,
        &object_basics.0,
        "object_basics",
        "create",
        vec![],
        vec![
            TestCallArg::Pure(bcs::to_bytes(&(16_u64)).unwrap()),
            TestCallArg::Pure(bcs::to_bytes(&sender).unwrap()),
        ],
        false,
    )
    .await
    .unwrap();
    let created_object_id = effects.created()[0].0 .0;
    let created_object = validator
        .get_object(&created_object_id)
        .await
        .unwrap()
        .unwrap();
    let created_object_bytes = created_object
        .data
        .try_as_move()
        .unwrap()
        .contents()
        .to_vec();
    // gas used should be the same
    assert_eq!(effects.gas_cost_summary(), &dev_inspect_gas_summary);

    // use the created object directly, via its bytes
    let DevInspectResults {
        effects, results, ..
    } = call_dev_inspect(
        &fullnode,
        &sender,
        &object_basics.0,
        "object_basics",
        "set_value",
        vec![],
        vec![
            TestCallArg::Pure(created_object_bytes),
            TestCallArg::Pure(bcs::to_bytes(&100_u64).unwrap()),
        ],
    )
    .await
    .unwrap();
    assert!(effects.created().is_empty());
    // the object is not marked as mutated, since it was passed in via bytes
    // but random gas is mutated
    assert_eq!(effects.mutated().len(), 1);
    assert!(effects.deleted().is_empty());
    assert!(effects.gas_cost_summary().computation_cost > 0);

    let mut results = results.unwrap();
    assert_eq!(results.len(), 1);
    let exec_results = results.pop().unwrap();
    let SuiExecutionResult {
        mutable_reference_outputs,
        return_values,
    } = exec_results;
    assert_eq!(mutable_reference_outputs.len(), 1);
    assert!(return_values.is_empty());
    let updated_reference_bytes = &mutable_reference_outputs[0].1;

    // make the same call with the object id
    let effects = call_move_(
        &validator,
        Some(&fullnode),
        &gas_object_id,
        &sender,
        &sender_key,
        &object_basics.0,
        "object_basics",
        "set_value",
        vec![],
        vec![
            TestCallArg::Object(created_object_id),
            TestCallArg::Pure(bcs::to_bytes(&100_u64).unwrap()),
        ],
        false,
    )
    .await
    .unwrap();
    assert!(effects.created().is_empty());
    assert_eq!(effects.mutated().len(), 2);
    assert!(effects.deleted().is_empty());
    assert!(effects.unwrapped_then_deleted().is_empty());

    // compare the bytes
    let updated_object = validator
        .get_object(&created_object_id)
        .await
        .unwrap()
        .unwrap();
    let updated_object_bytes = updated_object.data.try_as_move().unwrap().contents();
    assert_eq!(updated_object_bytes, updated_reference_bytes)
}

#[tokio::test]
async fn test_dev_inspect_unowned_object() {
    let (alice, alice_key): (_, AccountKeyPair) = get_key_pair();
    let alice_gas_id = ObjectID::random();
    let (validator, fullnode, object_basics) =
        init_state_with_ids_and_object_basics_with_fullnode(vec![(alice, alice_gas_id)]).await;
    let (bob, _bob_key): (_, AccountKeyPair) = get_key_pair();

    // make an object, send it to bob
    let effects = call_move_(
        &validator,
        Some(&fullnode),
        &alice_gas_id,
        &alice,
        &alice_key,
        &object_basics.0,
        "object_basics",
        "create",
        vec![],
        vec![
            TestCallArg::Pure(bcs::to_bytes(&(16_u64)).unwrap()),
            TestCallArg::Pure(bcs::to_bytes(&bob).unwrap()),
        ],
        false,
    )
    .await
    .unwrap();
    let created_object_id = effects.created()[0].0 .0;
    let created_object = validator
        .get_object(&created_object_id)
        .await
        .unwrap()
        .unwrap();
    assert!(alice != bob);
    assert_eq!(created_object.owner, Owner::AddressOwner(bob));

    // alice uses the object with dev inspect, despite not being the owner
    let DevInspectResults {
        effects, results, ..
    } = call_dev_inspect(
        &fullnode,
        &alice,
        &object_basics.0,
        "object_basics",
        "set_value",
        vec![],
        vec![
            TestCallArg::Object(created_object_id),
            TestCallArg::Pure(bcs::to_bytes(&100_u64).unwrap()),
        ],
    )
    .await
    .unwrap();
    assert!(effects.created().is_empty());
    // random gas and input object are mutated
    assert_eq!(effects.mutated().len(), 2);
    assert!(effects.deleted().is_empty());
    assert!(effects.gas_cost_summary().computation_cost > 0);

    let mut results = results.unwrap();
    assert_eq!(results.len(), 1);
    let exec_results = results.pop().unwrap();
    let SuiExecutionResult {
        mutable_reference_outputs,
        return_values,
    } = exec_results;
    assert_eq!(mutable_reference_outputs.len(), 1);
    assert!(return_values.is_empty());
}

#[tokio::test]
async fn test_dev_inspect_dynamic_field() {
    let (test_object1_bytes, test_object2_bytes) = {
        let (sender, sender_key): (_, AccountKeyPair) = get_key_pair();
        let gas_object_id = ObjectID::random();
        let (validator, fullnode, object_basics) =
            init_state_with_ids_and_object_basics_with_fullnode(vec![(sender, gas_object_id)])
                .await;
        macro_rules! mk_obj {
            () => {{
                let effects = call_move_(
                    &validator,
                    Some(&fullnode),
                    &gas_object_id,
                    &sender,
                    &sender_key,
                    &object_basics.0,
                    "object_basics",
                    "create",
                    vec![],
                    vec![
                        TestCallArg::Pure(bcs::to_bytes(&(16_u64)).unwrap()),
                        TestCallArg::Pure(bcs::to_bytes(&sender).unwrap()),
                    ],
                    false,
                )
                .await
                .unwrap();
                assert!(effects.status().is_ok(), "{:#?}", effects.status());
                let created_object_id = effects.created()[0].0 .0;
                let created_object = validator
                    .get_object(&created_object_id)
                    .await
                    .unwrap()
                    .unwrap();
                created_object
                    .data
                    .try_as_move()
                    .unwrap()
                    .contents()
                    .to_vec()
            }};
        }
        (mk_obj!(), mk_obj!())
    };

    let (sender, _sender_key): (_, AccountKeyPair) = get_key_pair();
    let gas_object_id = ObjectID::random();
    let (_validator, fullnode, object_basics) =
        init_state_with_ids_and_object_basics_with_fullnode(vec![(sender, gas_object_id)]).await;

    // add a dynamic field to itself
    let pt = ProgrammableTransaction {
        inputs: vec![
            CallArg::Pure(test_object1_bytes.clone()),
            CallArg::Pure(test_object1_bytes.clone()),
        ],
        commands: vec![Command::MoveCall(Box::new(ProgrammableMoveCall {
            package: object_basics.0,
            module: Identifier::new("object_basics").unwrap(),
            function: Identifier::new("add_ofield").unwrap(),
            type_arguments: vec![],
            arguments: vec![Argument::Input(0), Argument::Input(1)],
        }))],
    };
    let kind = TransactionKind::programmable(pt);
    let DevInspectResults { error, .. } = fullnode
        .dev_inspect_transaction_block(sender, kind, Some(1))
        .await
        .unwrap();
    // produces an error
    let err = error.unwrap();
    assert!(
        err.contains("kind: CircularObjectOwnership"),
        "unexpected error: {}",
        err
    );

    // add a dynamic field to an object
    let DevInspectResults {
        effects, results, ..
    } = call_dev_inspect(
        &fullnode,
        &sender,
        &object_basics.0,
        "object_basics",
        "add_ofield",
        vec![],
        vec![
            TestCallArg::Pure(test_object1_bytes.clone()),
            TestCallArg::Pure(test_object2_bytes.clone()),
        ],
    )
    .await
    .unwrap();
    let mut results = results.unwrap();
    assert_eq!(effects.created().len(), 1);
    // random gas is mutated
    assert_eq!(effects.mutated().len(), 1);
    // nothing is deleted
    assert!(effects.deleted().is_empty());
    assert!(effects.gas_cost_summary().computation_cost > 0);
    assert_eq!(results.len(), 1);
    let exec_results = results.pop().unwrap();
    let SuiExecutionResult {
        mutable_reference_outputs,
        return_values,
    } = exec_results;
    assert_eq!(mutable_reference_outputs.len(), 1);
    assert!(return_values.is_empty());
}

#[tokio::test]
async fn test_dev_inspect_return_values() {
    let (sender, sender_key): (_, AccountKeyPair) = get_key_pair();
    let gas_object_id = ObjectID::random();
    let (validator, fullnode, object_basics) =
        init_state_with_ids_and_object_basics_with_fullnode(vec![(sender, gas_object_id)]).await;

    // make an object
    let init_value = 16_u64;
    let effects = call_move_(
        &validator,
        Some(&fullnode),
        &gas_object_id,
        &sender,
        &sender_key,
        &object_basics.0,
        "object_basics",
        "create",
        vec![],
        vec![
            TestCallArg::Pure(bcs::to_bytes(&(init_value)).unwrap()),
            TestCallArg::Pure(bcs::to_bytes(&sender).unwrap()),
        ],
        false,
    )
    .await
    .unwrap();
    let created_object_id = effects.created()[0].0 .0;
    let created_object = validator
        .get_object(&created_object_id)
        .await
        .unwrap()
        .unwrap();
    let created_object_bytes = created_object
        .data
        .try_as_move()
        .unwrap()
        .contents()
        .to_vec();

    // mutably borrow a value from it's bytes
    let DevInspectResults { results, .. } = call_dev_inspect(
        &fullnode,
        &sender,
        &object_basics.0,
        "object_basics",
        "borrow_value_mut",
        vec![],
        vec![TestCallArg::Pure(created_object_bytes.clone())],
    )
    .await
    .unwrap();
    let mut results = results.unwrap();
    assert_eq!(results.len(), 1);
    let exec_results = results.pop().unwrap();
    let SuiExecutionResult {
        mutable_reference_outputs,
        mut return_values,
    } = exec_results;
    assert_eq!(mutable_reference_outputs.len(), 1);
    assert_eq!(return_values.len(), 1);
    let (return_value_1, return_type) = return_values.pop().unwrap();
    let deserialized_rv1: u64 = bcs::from_bytes(&return_value_1).unwrap();
    assert_eq!(init_value, deserialized_rv1);
    let type_tag: TypeTag = return_type.try_into().unwrap();
    assert!(matches!(type_tag, TypeTag::U64));

    // borrow a value from it's bytes
    let DevInspectResults { results, .. } = call_dev_inspect(
        &fullnode,
        &sender,
        &object_basics.0,
        "object_basics",
        "borrow_value",
        vec![],
        vec![TestCallArg::Pure(created_object_bytes.clone())],
    )
    .await
    .unwrap();
    let mut results = results.unwrap();
    assert_eq!(results.len(), 1);
    let exec_results = results.pop().unwrap();
    let SuiExecutionResult {
        mutable_reference_outputs,
        mut return_values,
    } = exec_results;
    assert!(mutable_reference_outputs.is_empty());
    assert_eq!(return_values.len(), 1);
    let (return_value_1, return_type) = return_values.pop().unwrap();
    let deserialized_rv1: u64 = bcs::from_bytes(&return_value_1).unwrap();
    assert_eq!(init_value, deserialized_rv1);
    let type_tag: TypeTag = return_type.try_into().unwrap();
    assert!(matches!(type_tag, TypeTag::U64));

    // read one value from it's bytes
    let DevInspectResults { results, .. } = call_dev_inspect(
        &fullnode,
        &sender,
        &object_basics.0,
        "object_basics",
        "get_value",
        vec![],
        vec![TestCallArg::Pure(created_object_bytes.clone())],
    )
    .await
    .unwrap();
    let mut results = results.unwrap();
    assert_eq!(results.len(), 1);
    let exec_results = results.pop().unwrap();
    let SuiExecutionResult {
        mutable_reference_outputs,
        mut return_values,
    } = exec_results;
    assert!(mutable_reference_outputs.is_empty());
    assert_eq!(return_values.len(), 1);
    let (return_value_1, return_type) = return_values.pop().unwrap();
    let deserialized_rv1: u64 = bcs::from_bytes(&return_value_1).unwrap();
    assert_eq!(init_value, deserialized_rv1);
    let type_tag: TypeTag = return_type.try_into().unwrap();
    assert!(matches!(type_tag, TypeTag::U64));

    // An unused value without drop is an error normally
    let effects = call_move_(
        &validator,
        Some(&fullnode),
        &gas_object_id,
        &sender,
        &sender_key,
        &object_basics.0,
        "object_basics",
        "wrap_object",
        vec![],
        vec![TestCallArg::Object(created_object_id)],
        false,
    )
    .await
    .unwrap();
    assert_eq!(
        effects.status(),
        &ExecutionStatus::Failure {
            error: ExecutionFailureStatus::UnusedValueWithoutDrop {
                result_idx: 0,
                secondary_idx: 0,
            },
            command: None,
        }
    );

    // An unused value without drop is not an error in dev inspect
    let DevInspectResults { results, .. } = call_dev_inspect(
        &fullnode,
        &sender,
        &object_basics.0,
        "object_basics",
        "wrap_object",
        vec![],
        vec![TestCallArg::Pure(created_object_bytes)],
    )
    .await
    .unwrap();
    let mut results = results.unwrap();
    assert_eq!(results.len(), 1);
    let exec_results = results.pop().unwrap();
    let SuiExecutionResult {
        mutable_reference_outputs,
        mut return_values,
    } = exec_results;
    assert!(mutable_reference_outputs.is_empty());
    assert_eq!(return_values.len(), 1);
    let (_return_value, return_type) = return_values.pop().unwrap();
    let expected_type = TypeTag::Struct(Box::new(StructTag {
        address: object_basics.0.into(),
        module: Identifier::new("object_basics").unwrap(),
        name: Identifier::new("Wrapper").unwrap(),
        type_params: vec![],
    }));
    let return_type: TypeTag = return_type.try_into().unwrap();
    assert_eq!(return_type, expected_type);
}

#[tokio::test]
async fn test_dev_inspect_gas_coin_argument() {
    let (validator, fullnode, _object_basics) =
        init_state_with_ids_and_object_basics_with_fullnode(vec![]).await;
    let epoch_store = validator.epoch_store_for_testing();
    let protocol_config = epoch_store.protocol_config();

    let sender = SuiAddress::random_for_testing_only();
    let recipient = SuiAddress::random_for_testing_only();
    let amount = 500;
    let pt = {
        let mut builder = ProgrammableTransactionBuilder::new();
        builder.pay_sui(vec![recipient], vec![amount]).unwrap();
        builder.finish()
    };
    let kind = TransactionKind::programmable(pt);
    let results = fullnode
        .dev_inspect_transaction_block(sender, kind, Some(1))
        .await
        .unwrap()
        .results
        .unwrap();
    assert_eq!(results.len(), 2);
    // Split results
    let SuiExecutionResult {
        mutable_reference_outputs,
        return_values,
    } = &results[0];
    // check argument is the gas coin updated
    assert_eq!(mutable_reference_outputs.len(), 1);
    let (arg, arg_value, arg_type) = &mutable_reference_outputs[0];
    assert_eq!(arg, &SuiArgument::GasCoin);
    check_coin_value(arg_value, arg_type, protocol_config.max_tx_gas() - amount);

    assert_eq!(return_values.len(), 1);
    let (ret_value, ret_type) = &return_values[0];
    check_coin_value(ret_value, ret_type, amount);

    // Transfer results
    let SuiExecutionResult {
        mutable_reference_outputs,
        return_values,
    } = &results[1];
    assert!(mutable_reference_outputs.is_empty());
    assert!(return_values.is_empty());
}

fn check_coin_value(actual_value: &[u8], actual_type: &SuiTypeTag, expected_value: u64) {
    let actual_type: TypeTag = actual_type.clone().try_into().unwrap();
    assert_eq!(actual_type, TypeTag::Struct(Box::new(GasCoin::type_())));
    let actual_coin: GasCoin = bcs::from_bytes(actual_value).unwrap();
    assert_eq!(actual_coin.value(), expected_value);
}

#[tokio::test]
async fn test_dev_inspect_uses_unbound_object() {
    let (sender, _sender_key): (_, AccountKeyPair) = get_key_pair();
    let gas_object_id = ObjectID::random();
    let (_validator, fullnode, object_basics) =
        init_state_with_ids_and_object_basics_with_fullnode(vec![(sender, gas_object_id)]).await;

    let pt = {
        let mut builder = ProgrammableTransactionBuilder::new();
        builder
            .move_call(
                object_basics.0,
                Identifier::new("object_basics").unwrap(),
                Identifier::new("freeze").unwrap(),
                vec![],
                vec![CallArg::Object(ObjectArg::ImmOrOwnedObject(
                    random_object_ref(),
                ))],
            )
            .unwrap();
        builder.finish()
    };
    let kind = TransactionKind::programmable(pt);

    let result = fullnode
        .dev_inspect_transaction_block(sender, kind, Some(1))
        .await;
    let Err(err) = result else { panic!() };
    assert!(err.to_string().contains("ObjectNotFound"));
}

#[tokio::test]
async fn test_dev_inspect_on_validator() {
    let (sender, _sender_key): (_, AccountKeyPair) = get_key_pair();
    let gas_object_id = ObjectID::random();
    let (validator, object_basics) =
        init_state_with_ids_and_object_basics(vec![(sender, gas_object_id)]).await;

    // test normal call
    let result = call_dev_inspect(
        &validator,
        &sender,
        &object_basics.0,
        "object_basics",
        "create",
        vec![],
        vec![
            TestCallArg::Pure(bcs::to_bytes(&(16_u64)).unwrap()),
            TestCallArg::Pure(bcs::to_bytes(&sender).unwrap()),
        ],
    )
    .await;
    assert!(result.is_err())
}

#[tokio::test]
async fn test_dry_run_on_validator() {
    let (validator, _fullnode, transaction, _gas_object_id, _shared_object_id) =
        construct_shared_object_transaction_with_sequence_number(None).await;
    let transaction_digest = *transaction.digest();
    let response = validator
        .dry_exec_transaction(
            transaction.data().intent_message().value.clone(),
            transaction_digest,
        )
        .await;
    assert!(response.is_err());
}

// Tests using a dynamic field that a is newer than the parent in dev inspect/dry run
#[tokio::test]
async fn test_dry_run_dev_inspect_dynamic_field_too_new() {
    let (sender, sender_key): (_, AccountKeyPair) = get_key_pair();
    let gas_object_id = ObjectID::random();
    let (validator, fullnode) = init_state_validator_with_fullnode().await;
    let (validator, object_basics) = publish_object_basics(validator).await;
    let (fullnode, _object_basics) = publish_object_basics(fullnode).await;
    let gas_object = Object::with_id_owner_for_testing(gas_object_id, sender);
    let gas_object_ref = gas_object.compute_object_reference();
    validator.insert_genesis_object(gas_object.clone()).await;
    fullnode.insert_genesis_object(gas_object).await;
    // create the parent
    let effects = call_move_(
        &validator,
        Some(&fullnode),
        &gas_object_id,
        &sender,
        &sender_key,
        &object_basics.0,
        "object_basics",
        "create",
        vec![],
        vec![
            TestCallArg::Pure(bcs::to_bytes(&(16_u64)).unwrap()),
            TestCallArg::Pure(bcs::to_bytes(&sender).unwrap()),
        ],
        false,
    )
    .await
    .unwrap();
    assert_eq!(effects.status(), &ExecutionStatus::Success);
    assert_eq!(effects.created().len(), 1);
    let parent = effects.created()[0].0;

    // create the child
    let effects = call_move_(
        &validator,
        Some(&fullnode),
        &gas_object_id,
        &sender,
        &sender_key,
        &object_basics.0,
        "object_basics",
        "create",
        vec![],
        vec![
            TestCallArg::Pure(bcs::to_bytes(&(32_u64)).unwrap()),
            TestCallArg::Pure(bcs::to_bytes(&sender).unwrap()),
        ],
        false,
    )
    .await
    .unwrap();
    assert_eq!(effects.status(), &ExecutionStatus::Success);
    assert_eq!(effects.created().len(), 1);
    let child = effects.created()[0].0;

    // add/wrap the child
    let effects = call_move_(
        &validator,
        Some(&fullnode),
        &gas_object_id,
        &sender,
        &sender_key,
        &object_basics.0,
        "object_basics",
        "add_field",
        vec![],
        vec![TestCallArg::Object(parent.0), TestCallArg::Object(child.0)],
        false,
    )
    .await
    .unwrap();
    assert_eq!(effects.status(), &ExecutionStatus::Success);
    assert_eq!(effects.created().len(), 1);
    let field = effects.created()[0].0;

    // make sure the parent was updated
    let new_parent = fullnode.get_object(&parent.0).await.unwrap().unwrap();
    assert!(parent.1 < new_parent.version());

    // delete the child, but using the old version of the parent
    let pt = ProgrammableTransaction {
        inputs: vec![CallArg::Object(ObjectArg::ImmOrOwnedObject(parent))],
        commands: vec![Command::MoveCall(Box::new(ProgrammableMoveCall {
            package: object_basics.0,
            module: Identifier::new("object_basics").unwrap(),
            function: Identifier::new("remove_field").unwrap(),
            type_arguments: vec![],
            arguments: vec![Argument::Input(0)],
        }))],
    };
    let kind = TransactionKind::programmable(pt.clone());
    // dev inspect
    let DevInspectResults { effects, .. } = fullnode
        .dev_inspect_transaction_block(sender, kind, Some(1))
        .await
        .unwrap();
    assert_eq!(effects.deleted().len(), 1);
    let deleted = &effects.deleted()[0];
    assert_eq!(field.0, deleted.object_id);
    assert_eq!(deleted.version, SequenceNumber::MAX);
    let rgp = fullnode.reference_gas_price_for_testing().unwrap();
    // dry run
    let data = TransactionData::new_programmable(
        sender,
        vec![gas_object_ref],
        pt,
        rgp * TEST_ONLY_GAS_UNIT_FOR_OBJECT_BASICS,
        rgp,
    );
    let transaction = to_sender_signed_transaction(data.clone(), &sender_key);
    let digest = *transaction.digest();
    let DryRunTransactionBlockResponse { effects, .. } =
        fullnode.dry_exec_transaction(data, digest).await.unwrap().0;
    assert_eq!(effects.deleted().len(), 1);
    let deleted = &effects.deleted()[0];
    assert_eq!(field.0, deleted.object_id);
    assert_eq!(deleted.version, SequenceNumber::MAX);
}

// tests using a gas coin with version MAX - 1
#[tokio::test]
async fn test_dry_run_dev_inspect_max_gas_version() {
    let (sender, sender_key): (_, AccountKeyPair) = get_key_pair();
    let gas_object_id = ObjectID::random();
    let (validator, fullnode) = init_state_validator_with_fullnode().await;
    let (validator, object_basics) = publish_object_basics(validator).await;
    let (fullnode, _object_basics) = publish_object_basics(fullnode).await;
    let gas_object = Object::with_id_owner_version_for_testing(
        gas_object_id,
        SequenceNumber::from_u64(SequenceNumber::MAX.value() - 1),
        sender,
    );
    let gas_object_ref = gas_object.compute_object_reference();
    validator.insert_genesis_object(gas_object.clone()).await;
    fullnode.insert_genesis_object(gas_object).await;
    let rgp = fullnode.reference_gas_price_for_testing().unwrap();
    let pt = ProgrammableTransaction {
        inputs: vec![
            CallArg::Pure(bcs::to_bytes(&(32_u64)).unwrap()),
            CallArg::Pure(bcs::to_bytes(&sender).unwrap()),
        ],
        commands: vec![Command::MoveCall(Box::new(ProgrammableMoveCall {
            package: object_basics.0,
            module: Identifier::new("object_basics").unwrap(),
            function: Identifier::new("create").unwrap(),
            type_arguments: vec![],
            arguments: vec![Argument::Input(0), Argument::Input(1)],
        }))],
    };
    let kind = TransactionKind::programmable(pt.clone());
    // dev inspect
    let DevInspectResults { effects, .. } = fullnode
        .dev_inspect_transaction_block(sender, kind, Some(1))
        .await
        .unwrap();
    assert_eq!(effects.status(), &SuiExecutionStatus::Success);

    // dry run
    let data = TransactionData::new_programmable(
        sender,
        vec![gas_object_ref],
        pt,
        rgp * TEST_ONLY_GAS_UNIT_FOR_OBJECT_BASICS,
        rgp,
    );
    let transaction = to_sender_signed_transaction(data.clone(), &sender_key);
    let digest = *transaction.digest();
    let DryRunTransactionBlockResponse { effects, .. } =
        fullnode.dry_exec_transaction(data, digest).await.unwrap().0;
    assert_eq!(effects.status(), &SuiExecutionStatus::Success);
}

#[tokio::test]
async fn test_handle_transfer_transaction_bad_signature() {
    let (sender, sender_key): (_, AccountKeyPair) = get_key_pair();
    let recipient = dbg_addr(2);
    let object_id = ObjectID::random();
    let gas_object_id = ObjectID::random();
    let authority_state =
        init_state_with_ids(vec![(sender, object_id), (sender, gas_object_id)]).await;
    let rgp = authority_state.reference_gas_price_for_testing().unwrap();
    let object = authority_state
        .get_object(&object_id)
        .await
        .unwrap()
        .unwrap();
    let gas_object = authority_state
        .get_object(&gas_object_id)
        .await
        .unwrap()
        .unwrap();
    let transfer_transaction = init_transfer_transaction(
        sender,
        &sender_key,
        recipient,
        object.compute_object_reference(),
        gas_object.compute_object_reference(),
        rgp * TEST_ONLY_GAS_UNIT_FOR_TRANSFER,
        rgp,
    );

    let consensus_address = "/ip4/127.0.0.1/tcp/0/http".parse().unwrap();

    let server = AuthorityServer::new_for_test(
        "/ip4/127.0.0.1/tcp/0/http".parse().unwrap(),
        authority_state.clone(),
        consensus_address,
    );
    let metrics = server.metrics.clone();

    let server_handle = server.spawn_for_test().await.unwrap();

    let client = NetworkAuthorityClient::connect(server_handle.address())
        .await
        .unwrap();

    let (_unknown_address, unknown_key): (_, AccountKeyPair) = get_key_pair();
    let mut bad_signature_transfer_transaction = transfer_transaction.clone().into_inner();
    *bad_signature_transfer_transaction
        .data_mut_for_testing()
        .tx_signatures_mut_for_testing() =
        vec![
            Signature::new_secure(transfer_transaction.data().intent_message(), &unknown_key)
                .into(),
        ];

    assert!(client
        .handle_transaction(bad_signature_transfer_transaction)
        .await
        .is_err());

    assert_eq!(metrics.signature_errors.get(), 1);

    let object = authority_state
        .get_object(&object_id)
        .await
        .unwrap()
        .unwrap();
    assert!(authority_state
        .get_transaction_lock(
            &object.compute_object_reference(),
            &authority_state.epoch_store_for_testing()
        )
        .await
        .unwrap()
        .is_none());

    assert!(authority_state
        .get_transaction_lock(
            &object.compute_object_reference(),
            &authority_state.epoch_store_for_testing()
        )
        .await
        .unwrap()
        .is_none());
}

#[tokio::test]
async fn test_handle_transfer_transaction_with_max_sequence_number() {
    let (sender, sender_key): (_, AccountKeyPair) = get_key_pair();
    let object_id: ObjectID = ObjectID::random();
    let gas_object_id = ObjectID::random();
    let recipient = dbg_addr(2);
    let authority_state = init_state_with_ids_and_versions(vec![
        (sender, object_id, SequenceNumber::MAX),
        (sender, gas_object_id, SequenceNumber::new()),
    ])
    .await;
    let rgp = authority_state.reference_gas_price_for_testing().unwrap();
    let epoch_store = authority_state.load_epoch_store_one_call_per_task();
    let object = authority_state
        .get_object(&object_id)
        .await
        .unwrap()
        .unwrap();
    let gas_object = authority_state
        .get_object(&gas_object_id)
        .await
        .unwrap()
        .unwrap();
    let transfer_transaction = init_transfer_transaction(
        sender,
        &sender_key,
        recipient,
        object.compute_object_reference(),
        gas_object.compute_object_reference(),
        rgp * TEST_ONLY_GAS_UNIT_FOR_TRANSFER,
        rgp,
    );
    let res = authority_state
        .handle_transaction(&epoch_store, transfer_transaction)
        .await;

    assert_eq!(
        UserInputError::try_from(res.unwrap_err()).unwrap(),
        UserInputError::InvalidSequenceNumber,
    );
}

#[tokio::test]
async fn test_handle_shared_object_with_max_sequence_number() {
    let (authority, _fullnode, transaction, _, _) =
        construct_shared_object_transaction_with_sequence_number(Some(SequenceNumber::MAX)).await;
    let epoch_store = authority.load_epoch_store_one_call_per_task();
    // Submit the transaction and assemble a certificate.
    let response = authority
        .handle_transaction(&epoch_store, transaction.clone())
        .await;
    assert_eq!(
        UserInputError::try_from(response.unwrap_err()).unwrap(),
        UserInputError::InvalidSequenceNumber,
    );
}

#[tokio::test]
async fn test_handle_transfer_transaction_unknown_sender() {
    let sender = dbg_addr(1);
    let (unknown_address, unknown_key) = get_key_pair();
    let object_id: ObjectID = ObjectID::random();
    let gas_object_id = ObjectID::random();
    let recipient = dbg_addr(2);
    let authority_state =
        init_state_with_ids(vec![(sender, object_id), (sender, gas_object_id)]).await;
    let rgp = authority_state.reference_gas_price_for_testing().unwrap();

    let epoch_store = authority_state.load_epoch_store_one_call_per_task();
    let object = authority_state
        .get_object(&object_id)
        .await
        .unwrap()
        .unwrap();
    let gas_object = authority_state
        .get_object(&gas_object_id)
        .await
        .unwrap()
        .unwrap();

    let unknown_sender_transfer_transaction = init_transfer_transaction(
        unknown_address,
        &unknown_key,
        recipient,
        object.compute_object_reference(),
        gas_object.compute_object_reference(),
        rgp * TEST_ONLY_GAS_UNIT_FOR_TRANSFER,
        rgp,
    );

    assert!(authority_state
        .handle_transaction(&epoch_store, unknown_sender_transfer_transaction)
        .await
        .is_err());

    let object = authority_state
        .get_object(&object_id)
        .await
        .unwrap()
        .unwrap();
    assert!(authority_state
        .get_transaction_lock(
            &object.compute_object_reference(),
            &authority_state.epoch_store_for_testing()
        )
        .await
        .unwrap()
        .is_none());

    assert!(authority_state
        .get_transaction_lock(
            &object.compute_object_reference(),
            &authority_state.epoch_store_for_testing()
        )
        .await
        .unwrap()
        .is_none());
}

#[tokio::test]
async fn test_handle_transfer_transaction_ok() {
    let (sender, sender_key): (_, AccountKeyPair) = get_key_pair();
    let recipient = dbg_addr(2);
    let object_id = ObjectID::random();
    let gas_object_id = ObjectID::random();
    let authority_state =
        init_state_with_ids(vec![(sender, object_id), (sender, gas_object_id)]).await;

    let rgp = authority_state.reference_gas_price_for_testing().unwrap();
    let epoch_store = authority_state.load_epoch_store_one_call_per_task();

    let object = authority_state
        .get_object(&object_id)
        .await
        .unwrap()
        .unwrap();
    let gas_object = authority_state
        .get_object(&gas_object_id)
        .await
        .unwrap()
        .unwrap();

    let before_object_version = object.version();
    let after_object_version =
        SequenceNumber::lamport_increment([object.version(), gas_object.version()]);

    assert!(before_object_version < after_object_version);

    let transfer_transaction = init_transfer_transaction(
        sender,
        &sender_key,
        recipient,
        object.compute_object_reference(),
        gas_object.compute_object_reference(),
        rgp * TEST_ONLY_GAS_UNIT_FOR_TRANSFER,
        rgp,
    );

    // Check the initial state of the locks
    assert!(authority_state
        .get_transaction_lock(
            &(object_id, before_object_version, object.digest()),
            &authority_state.epoch_store_for_testing()
        )
        .await
        .unwrap()
        .is_none());
    assert!(authority_state
        .get_transaction_lock(
            &(object_id, after_object_version, object.digest()),
            &authority_state.epoch_store_for_testing()
        )
        .await
        .is_err());

    let account_info = authority_state
        .handle_transaction(&epoch_store, transfer_transaction.clone())
        .await
        .unwrap();

    let pending_confirmation = authority_state
        .get_transaction_lock(
            &object.compute_object_reference(),
            &authority_state.epoch_store_for_testing(),
        )
        .await
        .unwrap()
        .unwrap();

    assert_eq!(
        &account_info.status.into_signed_for_testing(),
        pending_confirmation.auth_sig()
    );

    // Check the final state of the locks
    let Some(envelope) = authority_state.get_transaction_lock(
        &(object_id, before_object_version, object.digest()),
        &authority_state.epoch_store_for_testing(),
    ).await.unwrap() else {
        panic!("No verified envelope for transaction");
    };

    assert_eq!(
        envelope.data().intent_message().value,
        transfer_transaction.data().intent_message().value
    );
}

#[tokio::test]
async fn test_handle_sponsored_transaction() {
    let (sender, sender_key): (_, AccountKeyPair) = get_key_pair();
    let (sponsor, sponsor_key): (_, AccountKeyPair) = get_key_pair();
    let recipient = dbg_addr(2);
    let object_id = ObjectID::random();
    let gas_object_id = ObjectID::random();
    let authority_state =
        init_state_with_ids(vec![(sender, object_id), (sponsor, gas_object_id)]).await;
    let rgp = authority_state.reference_gas_price_for_testing().unwrap();
    let epoch_store = authority_state.load_epoch_store_one_call_per_task();

    let object = authority_state
        .get_object(&object_id)
        .await
        .unwrap()
        .unwrap();
    let gas_object = authority_state
        .get_object(&gas_object_id)
        .await
        .unwrap()
        .unwrap();

    let pt = {
        let mut builder = ProgrammableTransactionBuilder::new();
        builder
            .transfer_object(recipient, object.compute_object_reference())
            .unwrap();
        builder.finish()
    };
    let tx_kind = TransactionKind::programmable(pt);

    let data = TransactionData::new_with_gas_data(
        tx_kind.clone(),
        sender,
        GasData {
            payment: vec![gas_object.compute_object_reference()],
            owner: sponsor,
            price: rgp,
            budget: TEST_ONLY_GAS_UNIT_FOR_TRANSFER * rgp,
        },
    );
    let dual_signed_tx =
        to_sender_signed_transaction_with_multi_signers(data, vec![&sender_key, &sponsor_key]);

    authority_state
        .handle_transaction(&epoch_store, dual_signed_tx.clone())
        .await
        .unwrap();

    // Verify wrong gas owner gives error, using sender address
    let data = TransactionData::new_with_gas_data(
        tx_kind.clone(),
        sender,
        GasData {
            payment: vec![gas_object.compute_object_reference()],
            owner: sender, // <-- wrong
            price: rgp,
            budget: TEST_ONLY_GAS_UNIT_FOR_TRANSFER * rgp,
        },
    );
    let dual_signed_tx =
        to_sender_signed_transaction_with_multi_signers(data, vec![&sender_key, &sponsor_key]);

    let error = authority_state
        .handle_transaction(&epoch_store, dual_signed_tx.clone())
        .await
        .unwrap_err();

    assert!(
        matches!(
            UserInputError::try_from(error.clone()).unwrap(),
            UserInputError::IncorrectUserSignature { .. }
        ),
        "{}",
        error
    );

    // Verify wrong gas owner gives error, using another address
    let data = TransactionData::new_with_gas_data(
        tx_kind.clone(),
        sender,
        GasData {
            payment: vec![gas_object.compute_object_reference()],
            owner: dbg_addr(42), // <-- wrong
            price: rgp,
            budget: TEST_ONLY_GAS_UNIT_FOR_TRANSFER * rgp,
        },
    );
    let dual_signed_tx =
        to_sender_signed_transaction_with_multi_signers(data, vec![&sender_key, &sponsor_key]);
    let error = authority_state
        .handle_transaction(&epoch_store, dual_signed_tx.clone())
        .await
        .unwrap_err();

    assert!(
        matches!(
            UserInputError::try_from(error.clone()).unwrap(),
            UserInputError::IncorrectUserSignature { .. }
        ),
        "{}",
        error
    );

    // Sponsor sig is valid but it doesn't actually own the gas object
    let (third_party, third_party_key): (_, AccountKeyPair) = get_key_pair();
    let data = TransactionData::new_with_gas_data(
        tx_kind,
        sender,
        GasData {
            payment: vec![gas_object.compute_object_reference()],
            owner: third_party,
            price: rgp,
            budget: TEST_ONLY_GAS_UNIT_FOR_TRANSFER * rgp,
        },
    );
    let dual_signed_tx =
        to_sender_signed_transaction_with_multi_signers(data, vec![&sender_key, &third_party_key]);
    let error = authority_state
        .handle_transaction(&epoch_store, dual_signed_tx.clone())
        .await
        .unwrap_err();

    assert!(
        matches!(
            UserInputError::try_from(error.clone()).unwrap(),
            UserInputError::IncorrectUserSignature { .. }
        ),
        "{}",
        error
    );
}

#[tokio::test]
async fn test_transfer_package() {
    let (sender, sender_key): (_, AccountKeyPair) = get_key_pair();
    let recipient = dbg_addr(2);
    let object_id = ObjectID::random();
    let authority_state = init_state_with_ids(vec![(sender, object_id)]).await;
    let rgp = authority_state.reference_gas_price_for_testing().unwrap();
    let epoch_store = authority_state.load_epoch_store_one_call_per_task();
    let gas_object = authority_state
        .get_object(&object_id)
        .await
        .unwrap()
        .unwrap();
    let package_object_ref = authority_state
        .get_sui_system_package_object_ref()
        .await
        .unwrap();
    // We are trying to transfer the genesis package object, which is immutable.
    let transfer_transaction = init_transfer_transaction(
        sender,
        &sender_key,
        recipient,
        package_object_ref,
        gas_object.compute_object_reference(),
        rgp * TEST_ONLY_GAS_UNIT_FOR_TRANSFER,
        rgp,
    );
    authority_state
        .handle_transaction(&epoch_store, transfer_transaction.clone())
        .await
        .unwrap_err();
}

// This test attempts to use an immutable gas object to pay for gas.
// We expect it to fail early during transaction handle phase.
#[tokio::test]
async fn test_immutable_gas() {
    let (sender, sender_key): (_, AccountKeyPair) = get_key_pair();
    let recipient = dbg_addr(2);
    let mut_object_id = ObjectID::random();
    let authority_state = init_state_with_ids(vec![(sender, mut_object_id)]).await;

    let rgp = authority_state.reference_gas_price_for_testing().unwrap();
    let epoch_store = authority_state.load_epoch_store_one_call_per_task();
    let imm_object_id = ObjectID::random();
    let imm_object = Object::immutable_with_id_for_testing(imm_object_id);
    authority_state
        .insert_genesis_object(imm_object.clone())
        .await;
    let mut_object = authority_state
        .get_object(&mut_object_id)
        .await
        .unwrap()
        .unwrap();
    let transfer_transaction = init_transfer_transaction(
        sender,
        &sender_key,
        recipient,
        mut_object.compute_object_reference(),
        imm_object.compute_object_reference(),
        rgp * TEST_ONLY_GAS_UNIT_FOR_TRANSFER,
        rgp,
    );
    let result = authority_state
        .handle_transaction(&epoch_store, transfer_transaction.clone())
        .await;
    assert!(matches!(
        UserInputError::try_from(result.unwrap_err()).unwrap(),
        UserInputError::GasObjectNotOwnedObject { .. }
    ));
}

// This test attempts to use an immutable gas object to pay for gas.
// We expect it to fail early during transaction handle phase.
#[tokio::test]
async fn test_objected_owned_gas() {
    let (sender, sender_key): (_, AccountKeyPair) = get_key_pair();
    let recipient = dbg_addr(2);
    let parent_object_id = ObjectID::random();
    let authority_state = init_state_with_ids(vec![(sender, parent_object_id)]).await;
    let epoch_store = authority_state.load_epoch_store_one_call_per_task();
    let child_object_id = ObjectID::random();
    let child_object = Object::with_object_owner_for_testing(child_object_id, parent_object_id);
    authority_state
        .insert_genesis_object(child_object.clone())
        .await;
    let rgp = authority_state.reference_gas_price_for_testing().unwrap();
    let data = TransactionData::new_transfer_sui(
        recipient,
        sender,
        None,
        child_object.compute_object_reference(),
        rgp * TEST_ONLY_GAS_UNIT_FOR_TRANSFER,
        rgp,
    );

    let transaction = to_sender_signed_transaction(data, &sender_key);
    let result = authority_state
        .handle_transaction(&epoch_store, transaction)
        .await;
    assert!(matches!(
        UserInputError::try_from(result.unwrap_err()).unwrap(),
        UserInputError::GasObjectNotOwnedObject { .. }
    ));
}

/// Create a `CompiledModule` that depends on `m`
fn make_dependent_module(m: &CompiledModule) -> CompiledModule {
    let mut dependent_module = file_format::empty_module();
    dependent_module
        .identifiers
        .push(m.self_id().name().to_owned());
    dependent_module
        .address_identifiers
        .push(*m.self_id().address());
    dependent_module.module_handles.push(ModuleHandle {
        address: AddressIdentifierIndex((dependent_module.address_identifiers.len() - 1) as u16),
        name: IdentifierIndex((dependent_module.identifiers.len() - 1) as u16),
    });
    dependent_module
}

// Test that publishing a module that depends on an existing one works
#[tokio::test]
async fn test_publish_dependent_module_ok() {
    let (sender, sender_key): (_, AccountKeyPair) = get_key_pair();
    let gas_payment_object_id = ObjectID::random();
    let gas_payment_object = Object::with_id_owner_for_testing(gas_payment_object_id, sender);
    let gas_payment_object_ref = gas_payment_object.compute_object_reference();
    // create a genesis state that contains the gas object and genesis modules
    let genesis_module = match BuiltInFramework::genesis_objects().next().unwrap().data {
        Data::Package(m) => CompiledModule::deserialize_with_defaults(
            m.serialized_module_map().values().next().unwrap(),
        )
        .unwrap(),
        _ => unreachable!(),
    };
    // create a module that depends on a genesis module
    let dependent_module = make_dependent_module(&genesis_module);
    let dependent_module_bytes = {
        let mut bytes = Vec::new();
        dependent_module.serialize(&mut bytes).unwrap();
        bytes
    };

    let authority = init_state_with_objects(vec![gas_payment_object]).await;
    let rgp = authority.reference_gas_price_for_testing().unwrap();
    let data = TransactionData::new_module(
        sender,
        gas_payment_object_ref,
        vec![dependent_module_bytes],
        vec![ObjectID::from(*genesis_module.address())],
        rgp * TEST_ONLY_GAS_UNIT_FOR_PUBLISH,
        rgp,
    );
    let transaction = to_sender_signed_transaction(data, &sender_key);

    let dependent_module_id =
        TxContext::new(&sender, transaction.digest(), &EpochData::new_test()).fresh_id();

    // Object does not exist
    assert!(authority
        .get_object(&dependent_module_id)
        .await
        .unwrap()
        .is_none());
    let signed_effects = send_and_confirm_transaction(&authority, transaction)
        .await
        .unwrap()
        .1;
    signed_effects.into_data().status().unwrap();

    // check that the dependent module got published
    assert!(authority.get_object(&dependent_module_id).await.is_ok());
}

// Test that publishing a module with no dependencies works
#[tokio::test]
async fn test_publish_module_no_dependencies_ok() {
    let (sender, sender_key): (_, AccountKeyPair) = get_key_pair();
    let gas_payment_object_id = ObjectID::random();
    // Use the max budget to avoid running out of gas.
    let gas_balance = SuiCostTable::new_for_testing().max_gas_budget();
    let gas_payment_object =
        Object::with_id_owner_gas_for_testing(gas_payment_object_id, sender, gas_balance);
    let gas_payment_object_ref = gas_payment_object.compute_object_reference();
    let authority = init_state_with_objects(vec![gas_payment_object]).await;
    let rgp = authority.reference_gas_price_for_testing().unwrap();

    let module = file_format::empty_module();
    let mut module_bytes = Vec::new();
    module.serialize(&mut module_bytes).unwrap();
    let module_bytes = vec![module_bytes];
    let dependencies = vec![]; // no dependencies
    let data = TransactionData::new_module(
        sender,
        gas_payment_object_ref,
        module_bytes,
        dependencies,
        rgp * TEST_ONLY_GAS_UNIT_FOR_PUBLISH,
        rgp,
    );
    let transaction = to_sender_signed_transaction(data, &sender_key);
    let _module_object_id =
        TxContext::new(&sender, transaction.digest(), &EpochData::new_test()).fresh_id();
    let signed_effects = send_and_confirm_transaction(&authority, transaction)
        .await
        .unwrap()
        .1;
    signed_effects.into_data().status().unwrap();
}

#[tokio::test]
async fn test_publish_non_existing_dependent_module() {
    let (sender, sender_key): (_, AccountKeyPair) = get_key_pair();
    let gas_payment_object_id = ObjectID::random();
    let gas_payment_object = Object::with_id_owner_for_testing(gas_payment_object_id, sender);
    let gas_payment_object_ref = gas_payment_object.compute_object_reference();
    // create a genesis state that contains the gas object and genesis modules
    let genesis_module = match BuiltInFramework::genesis_objects().next().unwrap().data {
        Data::Package(m) => CompiledModule::deserialize_with_defaults(
            m.serialized_module_map().values().next().unwrap(),
        )
        .unwrap(),
        _ => unreachable!(),
    };
    // create a module that depends on a genesis module
    let mut dependent_module = make_dependent_module(&genesis_module);
    // Add another dependent module that points to a random address, hence does not exist on-chain.
    let not_on_chain = ObjectID::random();
    dependent_module
        .address_identifiers
        .push(AccountAddress::from(not_on_chain));
    dependent_module.module_handles.push(ModuleHandle {
        address: AddressIdentifierIndex((dependent_module.address_identifiers.len() - 1) as u16),
        name: IdentifierIndex(0),
    });
    let dependent_module_bytes = {
        let mut bytes = Vec::new();
        dependent_module.serialize(&mut bytes).unwrap();
        bytes
    };
    let authority = init_state_with_objects(vec![gas_payment_object]).await;
    let epoch_store = authority.load_epoch_store_one_call_per_task();

    let rgp = authority.reference_gas_price_for_testing().unwrap();
    let data = TransactionData::new_module(
        sender,
        gas_payment_object_ref,
        vec![dependent_module_bytes],
        vec![ObjectID::from(*genesis_module.address()), not_on_chain],
        rgp * TEST_ONLY_GAS_UNIT_FOR_PUBLISH,
        rgp,
    );
    let transaction = to_sender_signed_transaction(data, &sender_key);
    let response = authority
        .handle_transaction(&epoch_store, transaction)
        .await;
    assert!(std::string::ToString::to_string(&response.unwrap_err())
        .contains("DependentPackageNotFound"));
    // Check that gas was not charged.
    assert_eq!(
        authority
            .get_object(&gas_payment_object_id)
            .await
            .unwrap()
            .unwrap()
            .version(),
        gas_payment_object_ref.1
    );
}

// make sure that publishing a package above the size limit fails
#[tokio::test]
async fn test_package_size_limit() {
    let (sender, sender_key): (_, AccountKeyPair) = get_key_pair();
    let gas_payment_object_id = ObjectID::random();
    let gas_payment_object =
        Object::with_id_owner_gas_for_testing(gas_payment_object_id, sender, u64::MAX);
    let gas_payment_object_ref = gas_payment_object.compute_object_reference();
    let mut package = Vec::new();
    let mut modules_size = 0;
    // create a package larger than the max size; serialized modules is the largest contributor and
    // while other metadata is also contributing to the size it's easiest to construct object that's
    // too large by adding more module bytes
    let max_move_package_size = ProtocolConfig::get_for_min_version().max_move_package_size();
    while modules_size <= max_move_package_size {
        let mut module = file_format::empty_module();
        // generate unique name
        module.identifiers[0] =
            Identifier::new(format!("TestModule{:0>21000?}", modules_size)).unwrap();
        let module_bytes = {
            let mut bytes = Vec::new();
            module.serialize(&mut bytes).unwrap();
            bytes
        };
        modules_size += module_bytes.len() as u64;
        package.push(module_bytes);
    }

    let authority = init_state_with_objects(vec![gas_payment_object]).await;
    let rgp = authority.reference_gas_price_for_testing().unwrap();
    let data = TransactionData::new_module(
        sender,
        gas_payment_object_ref,
        package,
        vec![],
        rgp * TEST_ONLY_GAS_UNIT_FOR_PUBLISH,
        rgp,
    );
    let transaction = to_sender_signed_transaction(data, &sender_key);
    let signed_effects = send_and_confirm_transaction(&authority, transaction)
        .await
        .unwrap()
        .1;
    let ExecutionStatus::Failure { error, command: _ } = signed_effects.status() else {
        panic!("expected transaction to fail")
    };
    assert!(matches!(
        error,
        ExecutionFailureStatus::MovePackageTooBig { .. }
    ));
}

#[tokio::test]
async fn test_handle_move_transaction() {
    let (sender, sender_key): (_, AccountKeyPair) = get_key_pair();
    let gas_payment_object_id = ObjectID::random();
    let (authority_state, pkg_ref) =
        init_state_with_ids_and_object_basics(vec![(sender, gas_payment_object_id)]).await;

    let effects = create_move_object(
        &pkg_ref.0,
        &authority_state,
        &gas_payment_object_id,
        &sender,
        &sender_key,
    )
    .await
    .unwrap();

    assert!(effects.status().is_ok());
    assert_eq!(effects.created().len(), 1);
    assert_eq!(effects.mutated().len(), 1);

    let created_object_id = effects.created()[0].0 .0;
    // check that transaction actually created an object with the expected ID, owner
    let created_obj = authority_state
        .get_object(&created_object_id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(created_obj.owner, sender);
    assert_eq!(created_obj.id(), created_object_id);
}

#[sim_test]
async fn test_conflicting_transactions() {
    let (sender, sender_key): (_, AccountKeyPair) = get_key_pair();
    let recipient1 = dbg_addr(2);
    let recipient2 = dbg_addr(3);
    let object_id = ObjectID::random();
    let gas_object_id = ObjectID::random();
    let authority_state =
        init_state_with_ids(vec![(sender, object_id), (sender, gas_object_id)]).await;

    let rgp = authority_state.reference_gas_price_for_testing().unwrap();
    let epoch_store = authority_state.load_epoch_store_one_call_per_task();
    let object = authority_state
        .get_object(&object_id)
        .await
        .unwrap()
        .unwrap();
    let gas_object = authority_state
        .get_object(&gas_object_id)
        .await
        .unwrap()
        .unwrap();

    let tx1 = init_transfer_transaction(
        sender,
        &sender_key,
        recipient1,
        object.compute_object_reference(),
        gas_object.compute_object_reference(),
        rgp * TEST_ONLY_GAS_UNIT_FOR_TRANSFER,
        rgp,
    );

    let tx2 = init_transfer_transaction(
        sender,
        &sender_key,
        recipient2,
        object.compute_object_reference(),
        gas_object.compute_object_reference(),
        rgp * TEST_ONLY_GAS_UNIT_FOR_TRANSFER,
        rgp,
    );

    // repeatedly attempt to submit conflicting transactions at the same time, and verify that
    // exactly one succeeds in every case.
    //
    // Note: I verified that this test fails immediately if we remove the acquire_locks() call in
    // acquire_transaction_locks() and then add a sleep after we read the locks.
    for _ in 0..100 {
        let mut futures = FuturesUnordered::new();
        futures.push(authority_state.handle_transaction(&epoch_store, tx1.clone()));
        futures.push(authority_state.handle_transaction(&epoch_store, tx2.clone()));

        let first = futures.next().await.unwrap();
        let second = futures.next().await.unwrap();
        assert!(futures.next().await.is_none());

        // exactly one should fail.
        assert!(first.is_ok() != second.is_ok());

        let (ok, err) = if first.is_ok() {
            (first.unwrap(), second.unwrap_err())
        } else {
            (second.unwrap(), first.unwrap_err())
        };

        assert!(matches!(err, SuiError::ObjectLockConflict { .. }));

        let object_info = authority_state
            .handle_object_info_request(ObjectInfoRequest::latest_object_info_request(
                object.id(),
                None,
            ))
            .await
            .unwrap();
        let gas_info = authority_state
            .handle_object_info_request(ObjectInfoRequest::latest_object_info_request(
                gas_object.id(),
                None,
            ))
            .await
            .unwrap();

        assert_eq!(
            &ok.clone().status.into_signed_for_testing(),
            object_info
                .lock_for_debugging
                .expect("object should be locked")
                .auth_sig()
        );

        assert_eq!(
            &ok.clone().status.into_signed_for_testing(),
            gas_info
                .lock_for_debugging
                .expect("gas should be locked")
                .auth_sig()
        );

        authority_state.database.reset_locks_for_test(
            &[*tx1.digest(), *tx2.digest()],
            &[
                gas_object.compute_object_reference(),
                object.compute_object_reference(),
            ],
            &authority_state.epoch_store_for_testing(),
        );
    }
}

#[tokio::test]
async fn test_handle_transfer_transaction_double_spend() {
    let (sender, sender_key): (_, AccountKeyPair) = get_key_pair();
    let recipient = dbg_addr(2);
    let object_id = ObjectID::random();
    let gas_object_id = ObjectID::random();
    let authority_state =
        init_state_with_ids(vec![(sender, object_id), (sender, gas_object_id)]).await;

    let rgp = authority_state.reference_gas_price_for_testing().unwrap();
    let epoch_store = authority_state.load_epoch_store_one_call_per_task();
    let object = authority_state
        .get_object(&object_id)
        .await
        .unwrap()
        .unwrap();
    let gas_object = authority_state
        .get_object(&gas_object_id)
        .await
        .unwrap()
        .unwrap();
    let transfer_transaction = init_transfer_transaction(
        sender,
        &sender_key,
        recipient,
        object.compute_object_reference(),
        gas_object.compute_object_reference(),
        rgp * TEST_ONLY_GAS_UNIT_FOR_TRANSFER,
        rgp,
    );

    let signed_transaction = authority_state
        .handle_transaction(&epoch_store, transfer_transaction.clone())
        .await
        .unwrap();
    // calls to handlers are idempotent -- returns the same.
    let double_spend_signed_transaction = authority_state
        .handle_transaction(&epoch_store, transfer_transaction)
        .await
        .unwrap();
    // this is valid because our test authority should not change its certified transaction
    assert_eq!(signed_transaction, double_spend_signed_transaction);
}

#[tokio::test]
async fn test_handle_transfer_sui_with_amount_insufficient_gas() {
    let (sender, sender_key): (_, AccountKeyPair) = get_key_pair();
    let recipient = dbg_addr(2);
    let object_id = ObjectID::random();
    let authority_state = init_state_with_ids(vec![(sender, object_id)]).await;
    let rgp = authority_state.reference_gas_price_for_testing().unwrap();
    let object = authority_state
        .get_object(&object_id)
        .await
        .unwrap()
        .unwrap();
    let data = TransactionData::new_transfer_sui(
        recipient,
        sender,
        Some(GAS_VALUE_FOR_TESTING),
        object.compute_object_reference(),
        rgp * 2000,
        rgp,
    );
    let transaction = to_sender_signed_transaction(data, &sender_key);
    let result = send_and_confirm_transaction(&authority_state, transaction)
        .await
        .unwrap()
        .1
        .into_data();

    let ExecutionStatus::Failure { error, command } = result.status() else {
        panic!("expected transaction to fail")
    };
    assert_eq!(command, &Some(0));
    assert_eq!(error, &ExecutionFailureStatus::InsufficientCoinBalance)
}

#[tokio::test]
async fn test_missing_package() {
    let (sender, sender_key): (_, AccountKeyPair) = get_key_pair();
    let gas_object_id = ObjectID::random();
    let (authority_state, _object_basics) =
        init_state_with_ids_and_object_basics(vec![(sender, gas_object_id)]).await;
    let epoch_store = authority_state.load_epoch_store_one_call_per_task();
    let rgp = authority_state.reference_gas_price_for_testing().unwrap();
    let gas_object = authority_state
        .get_object(&gas_object_id)
        .await
        .unwrap()
        .unwrap();
    let non_existent_package = ObjectID::MAX;
    let gas_object_ref = gas_object.compute_object_reference();
    let data = TransactionData::new_move_call(
        sender,
        non_existent_package,
        ident_str!("object_basics").to_owned(),
        ident_str!("wrap").to_owned(),
        vec![],
        gas_object_ref,
        vec![],
        TEST_ONLY_GAS_UNIT_FOR_OBJECT_BASICS * rgp,
        rgp,
    )
    .unwrap();
    let transaction = to_sender_signed_transaction(data, &sender_key);
    let result = authority_state
        .handle_transaction(&epoch_store, transaction)
        .await;
    assert!(matches!(
        UserInputError::try_from(result.unwrap_err()).unwrap(),
        UserInputError::DependentPackageNotFound { .. }
    ));
}

#[tokio::test]
async fn test_type_argument_dependencies() {
    let (s1, s1_key): (_, AccountKeyPair) = get_key_pair();
    let (s2, s2_key): (_, AccountKeyPair) = get_key_pair();
    let (s3, s3_key): (_, AccountKeyPair) = get_key_pair();
    let gas1 = ObjectID::random();
    let gas2 = ObjectID::random();
    let gas3 = ObjectID::random();
    let (authority_state, (object_basics, _, _)) =
        init_state_with_ids_and_object_basics(vec![(s1, gas1), (s2, gas2), (s3, gas3)]).await;
    let epoch_store = authority_state.load_epoch_store_one_call_per_task();
    let rgp = authority_state.reference_gas_price_for_testing().unwrap();
    let gas1 = {
        let o = authority_state.get_object(&gas1).await.unwrap().unwrap();
        o.compute_object_reference()
    };
    let gas2 = {
        let o = authority_state.get_object(&gas2).await.unwrap().unwrap();
        o.compute_object_reference()
    };
    let gas3 = {
        let o = authority_state.get_object(&gas3).await.unwrap().unwrap();
        o.compute_object_reference()
    };
    // primitive type tag succeeds
    let data = TransactionData::new_move_call(
        s1,
        object_basics,
        ident_str!("object_basics").to_owned(),
        ident_str!("generic_test").to_owned(),
        vec![TypeTag::U64],
        gas1,
        vec![],
        TEST_ONLY_GAS_UNIT_FOR_OBJECT_BASICS * rgp,
        rgp,
    )
    .unwrap();
    let transaction = to_sender_signed_transaction(data, &s1_key);
    authority_state
        .handle_transaction(&epoch_store, transaction)
        .await
        .unwrap()
        .status
        .into_signed_for_testing();
    // obj type tag succeeds
    let data = TransactionData::new_move_call(
        s2,
        object_basics,
        ident_str!("object_basics").to_owned(),
        ident_str!("generic_test").to_owned(),
        vec![TypeTag::Struct(Box::new(StructTag {
            address: object_basics.into(),
            module: ident_str!("object_basics").to_owned(),
            name: ident_str!("Object").to_owned(),
            type_params: vec![],
        }))],
        gas2,
        vec![],
        TEST_ONLY_GAS_UNIT_FOR_OBJECT_BASICS * rgp,
        rgp,
    )
    .unwrap();
    let transaction = to_sender_signed_transaction(data, &s2_key);
    authority_state
        .handle_transaction(&epoch_store, transaction)
        .await
        .unwrap()
        .status
        .into_signed_for_testing();
    // missing package fails
    let data = TransactionData::new_move_call(
        s3,
        object_basics,
        ident_str!("object_basics").to_owned(),
        ident_str!("generic_test").to_owned(),
        vec![TypeTag::Struct(Box::new(StructTag {
            address: ObjectID::MAX.into(),
            module: ident_str!("object_basics").to_owned(),
            name: ident_str!("Object").to_owned(),
            type_params: vec![],
        }))],
        gas3,
        vec![],
        TEST_ONLY_GAS_UNIT_FOR_OBJECT_BASICS * rgp,
        rgp,
    )
    .unwrap();
    let transaction = to_sender_signed_transaction(data, &s3_key);
    let result = authority_state
        .handle_transaction(&epoch_store, transaction)
        .await;

    assert!(matches!(
        UserInputError::try_from(result.unwrap_err()).unwrap(),
        UserInputError::DependentPackageNotFound { .. }
    ));
}

#[tokio::test]
async fn test_handle_confirmation_transaction_receiver_equal_sender() {
    let (address, key) = get_key_pair();
    let object_id: ObjectID = ObjectID::random();
    let gas_object_id = ObjectID::random();
    let authority_state =
        init_state_with_ids(vec![(address, object_id), (address, gas_object_id)]).await;
    let object = authority_state
        .get_object(&object_id)
        .await
        .unwrap()
        .unwrap();
    let gas_object = authority_state
        .get_object(&gas_object_id)
        .await
        .unwrap()
        .unwrap();

    let certified_transfer_transaction = init_certified_transfer_transaction(
        address,
        &key,
        address,
        object.compute_object_reference(),
        gas_object.compute_object_reference(),
        &authority_state,
    );
    let signed_effects = authority_state
        .execute_certificate(
            &certified_transfer_transaction,
            &authority_state.epoch_store_for_testing(),
        )
        .await
        .unwrap();
    signed_effects.into_message().status().unwrap();
}

#[tokio::test]
async fn test_handle_confirmation_transaction_ok() {
    let (sender, sender_key): (_, AccountKeyPair) = get_key_pair();
    let recipient = dbg_addr(2);
    let object_id = ObjectID::random();
    let gas_object_id = ObjectID::random();
    let authority_state =
        init_state_with_ids(vec![(sender, object_id), (sender, gas_object_id)]).await;
    let object = authority_state
        .get_object(&object_id)
        .await
        .unwrap()
        .unwrap();
    let gas_object = authority_state
        .get_object(&gas_object_id)
        .await
        .unwrap()
        .unwrap();

    let next_sequence_number =
        SequenceNumber::lamport_increment([object.version(), gas_object.version()]);

    let certified_transfer_transaction = init_certified_transfer_transaction(
        sender,
        &sender_key,
        recipient,
        object.compute_object_reference(),
        gas_object.compute_object_reference(),
        &authority_state,
    );

    let old_account = authority_state
        .get_object(&object_id)
        .await
        .unwrap()
        .unwrap();

    let signed_effects = authority_state
        .execute_certificate(
            &certified_transfer_transaction.clone(),
            &authority_state.epoch_store_for_testing(),
        )
        .await
        .unwrap();
    signed_effects.into_message().status().unwrap();
    // Key check: the ownership has changed

    let new_account = authority_state
        .get_object(&object_id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(new_account.owner, recipient);
    assert_eq!(next_sequence_number, new_account.version());

    // Check locks are set and archived correctly
    assert!(authority_state
        .get_transaction_lock(
            &(object_id, 1.into(), old_account.digest()),
            &authority_state.epoch_store_for_testing()
        )
        .await
        .is_err());
    assert!(authority_state
        .get_transaction_lock(
            &(object_id, 2.into(), new_account.digest()),
            &authority_state.epoch_store_for_testing()
        )
        .await
        .expect("Exists")
        .is_none());
}

#[tokio::test]
async fn test_handle_confirmation_transaction_idempotent() {
    let (sender, sender_key): (_, AccountKeyPair) = get_key_pair();
    let recipient = dbg_addr(2);
    let object_id = ObjectID::random();
    let gas_object_id = ObjectID::random();
    let authority_state =
        init_state_with_ids(vec![(sender, object_id), (sender, gas_object_id)]).await;
    let object = authority_state
        .get_object(&object_id)
        .await
        .unwrap()
        .unwrap();
    let gas_object = authority_state
        .get_object(&gas_object_id)
        .await
        .unwrap()
        .unwrap();

    let certified_transfer_transaction = init_certified_transfer_transaction(
        sender,
        &sender_key,
        recipient,
        object.compute_object_reference(),
        gas_object.compute_object_reference(),
        &authority_state,
    );

    let signed_effects = authority_state
        .execute_certificate(
            &certified_transfer_transaction,
            &authority_state.epoch_store_for_testing(),
        )
        .await
        .unwrap();
    assert_eq!(signed_effects.data().status(), &ExecutionStatus::Success);

    let signed_effects2 = authority_state
        .execute_certificate(
            &certified_transfer_transaction,
            &authority_state.epoch_store_for_testing(),
        )
        .await
        .unwrap();
    assert_eq!(signed_effects2.data().status(), &ExecutionStatus::Success);

    // this is valid because we're checking the authority state does not change the certificate
    assert_eq!(signed_effects, signed_effects2);

    // Now check the transaction info request is also the same
    let info = authority_state
        .handle_transaction_info_request(TransactionInfoRequest {
            transaction_digest: *certified_transfer_transaction.digest(),
        })
        .await
        .unwrap();

    assert_eq!(
        info.status.into_effects_for_testing(),
        signed_effects.into_inner()
    );
}

#[tokio::test]
async fn test_move_call_mutable_object_not_mutated() {
    let (sender, sender_key): (_, AccountKeyPair) = get_key_pair();
    let gas_object_id = ObjectID::random();
    let (authority_state, pkg_ref) =
        init_state_with_ids_and_object_basics(vec![(sender, gas_object_id)]).await;

    let effects = create_move_object(
        &pkg_ref.0,
        &authority_state,
        &gas_object_id,
        &sender,
        &sender_key,
    )
    .await
    .unwrap();
    assert!(effects.status().is_ok());
    assert_eq!((effects.created().len(), effects.mutated().len()), (1, 1));
    let (new_object_id1, seq1, _) = effects.created()[0].0;

    let effects = create_move_object(
        &pkg_ref.0,
        &authority_state,
        &gas_object_id,
        &sender,
        &sender_key,
    )
    .await
    .unwrap();
    assert!(effects.status().is_ok());
    assert_eq!((effects.created().len(), effects.mutated().len()), (1, 1));
    let (new_object_id2, seq2, _) = effects.created()[0].0;

    let gas_version = authority_state
        .get_object(&gas_object_id)
        .await
        .unwrap()
        .unwrap()
        .version();

    let next_object_version = SequenceNumber::lamport_increment([gas_version, seq1, seq2]);

    let effects = call_move(
        &authority_state,
        &gas_object_id,
        &sender,
        &sender_key,
        &pkg_ref.0,
        "object_basics",
        "update",
        vec![],
        vec![
            TestCallArg::Object(new_object_id1),
            TestCallArg::Object(new_object_id2),
        ],
    )
    .await
    .unwrap();
    assert!(effects.status().is_ok());
    assert_eq!((effects.created().len(), effects.mutated().len()), (0, 3));
    // Verify that both objects' version increased, even though only one object was updated.
    assert_eq!(
        authority_state
            .get_object(&new_object_id1)
            .await
            .unwrap()
            .unwrap()
            .version(),
        next_object_version
    );
    assert_eq!(
        authority_state
            .get_object(&new_object_id2)
            .await
            .unwrap()
            .unwrap()
            .version(),
        next_object_version
    );
}

#[tokio::test]
async fn test_move_call_insufficient_gas() {
    // This test attempts to trigger a transaction execution that would fail due to insufficient gas.
    // We want to ensure that even though the transaction failed to execute, all objects
    // are mutated properly.
    let (sender, sender_key): (_, AccountKeyPair) = get_key_pair();
    let (recipient, recipient_key): (_, AccountKeyPair) = get_key_pair();
    let object_id = ObjectID::random();
    let gas_object_id1 = ObjectID::random();
    let gas_object_id2 = ObjectID::random();
    let authority_state = init_state_with_ids(vec![
        (sender, object_id),
        (sender, gas_object_id1),
        (recipient, gas_object_id2),
    ])
    .await;
    let rgp = authority_state.reference_gas_price_for_testing().unwrap();

    // First execute a transaction successfully to obtain the amount of gas needed for this
    // type of transaction.
    // After this transaction, object_id will be owned by recipient.
    let certified_transfer_transaction = init_certified_transfer_transaction(
        sender,
        &sender_key,
        recipient,
        authority_state
            .get_object(&object_id)
            .await
            .unwrap()
            .unwrap()
            .compute_object_reference(),
        authority_state
            .get_object(&gas_object_id1)
            .await
            .unwrap()
            .unwrap()
            .compute_object_reference(),
        &authority_state,
    );
    let effects = authority_state
        .execute_certificate(
            &certified_transfer_transaction,
            &authority_state.epoch_store_for_testing(),
        )
        .await
        .unwrap()
        .into_message();
    let gas_used = effects.gas_cost_summary().net_gas_usage() as u64;
    let kind_of_rebate_to_remove = effects.gas_cost_summary().storage_cost / 2;

    let obj_ref = authority_state
        .get_object(&object_id)
        .await
        .unwrap()
        .unwrap()
        .compute_object_reference();

    let gas_ref = authority_state
        .get_object(&gas_object_id2)
        .await
        .unwrap()
        .unwrap()
        .compute_object_reference();

    let next_object_version = SequenceNumber::lamport_increment([obj_ref.1, gas_ref.1]);

    let gas_used = if gas_used > kind_of_rebate_to_remove {
        if gas_used - kind_of_rebate_to_remove < 2000 {
            2000
        } else {
            gas_used - kind_of_rebate_to_remove
        }
    } else {
        2000
    };
    // Now we try to construct a transaction with a smaller gas budget than required.
    let data =
        TransactionData::new_transfer(sender, obj_ref, recipient, gas_ref, gas_used - 5, rgp);

    let transaction = to_sender_signed_transaction(data, &recipient_key);
    let tx_digest = *transaction.digest();
    let signed_effects = send_and_confirm_transaction(&authority_state, transaction)
        .await
        .unwrap()
        .1;
    let effects = signed_effects.into_data();
    assert!(effects.status().is_err());
    let obj = authority_state
        .get_object(&object_id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(obj.previous_transaction, tx_digest);
    assert_eq!(obj.version(), next_object_version);
    assert_eq!(obj.owner, recipient);
}

#[tokio::test]
async fn test_move_call_delete() {
    let (sender, sender_key): (_, AccountKeyPair) = get_key_pair();
    let gas_object_id = ObjectID::random();
    let (authority_state, pkg_ref) =
        init_state_with_ids_and_object_basics(vec![(sender, gas_object_id)]).await;

    let effects = create_move_object(
        &pkg_ref.0,
        &authority_state,
        &gas_object_id,
        &sender,
        &sender_key,
    )
    .await
    .unwrap();
    assert!(effects.status().is_ok());
    assert_eq!((effects.created().len(), effects.mutated().len()), (1, 1));
    let (new_object_id1, _seq1, _) = effects.created()[0].0;

    let effects = create_move_object(
        &pkg_ref.0,
        &authority_state,
        &gas_object_id,
        &sender,
        &sender_key,
    )
    .await
    .unwrap();
    assert!(effects.status().is_ok());
    assert_eq!((effects.created().len(), effects.mutated().len()), (1, 1));
    let (new_object_id2, _seq2, _) = effects.created()[0].0;

    let effects = call_move(
        &authority_state,
        &gas_object_id,
        &sender,
        &sender_key,
        &pkg_ref.0,
        "object_basics",
        "update",
        vec![],
        vec![
            TestCallArg::Object(new_object_id1),
            TestCallArg::Object(new_object_id2),
        ],
    )
    .await
    .unwrap();
    assert!(effects.status().is_ok());
    // All mutable objects will appear to be mutated, even if they are not.
    // obj1, obj2 and gas are all mutated here.
    assert_eq!((effects.created().len(), effects.mutated().len()), (0, 3));

    let effects = call_move(
        &authority_state,
        &gas_object_id,
        &sender,
        &sender_key,
        &pkg_ref.0,
        "object_basics",
        "delete",
        vec![],
        vec![TestCallArg::Object(new_object_id1)],
    )
    .await
    .unwrap();
    assert!(effects.status().is_ok());
    assert_eq!((effects.deleted().len(), effects.mutated().len()), (1, 1));
}

#[tokio::test]
async fn test_get_latest_parent_entry_genesis() {
    let authority_state = TestAuthorityBuilder::new().build().await;
    // There should not be any object with ID zero
    assert!(authority_state
        .get_object_or_tombstone(ObjectID::ZERO)
        .await
        .unwrap()
        .is_none());
}

#[tokio::test]
async fn test_get_latest_parent_entry() {
    let (sender, sender_key): (_, AccountKeyPair) = get_key_pair();
    let gas_object_id = ObjectID::random();
    let (authority_state, pkg_ref) =
        init_state_with_ids_and_object_basics(vec![(sender, gas_object_id)]).await;

    let effects = create_move_object(
        &pkg_ref.0,
        &authority_state,
        &gas_object_id,
        &sender,
        &sender_key,
    )
    .await
    .unwrap();
    let (new_object_id1, seq1, _) = effects.created()[0].0;

    let effects = create_move_object(
        &pkg_ref.0,
        &authority_state,
        &gas_object_id,
        &sender,
        &sender_key,
    )
    .await
    .unwrap();
    let (new_object_id2, seq2, _) = effects.created()[0].0;

    let update_version = SequenceNumber::lamport_increment([seq1, seq2, effects.gas_object().0 .1]);

    let effects = call_move(
        &authority_state,
        &gas_object_id,
        &sender,
        &sender_key,
        &pkg_ref.0,
        "object_basics",
        "update",
        vec![],
        vec![
            TestCallArg::Object(new_object_id1),
            TestCallArg::Object(new_object_id2),
        ],
    )
    .await
    .unwrap();

    // Check entry for object to be deleted is returned
    let obj_ref = authority_state
        .get_object_or_tombstone(new_object_id1)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(obj_ref.0, new_object_id1);
    assert_eq!(obj_ref.1, update_version);

    let delete_version = SequenceNumber::lamport_increment([obj_ref.1, effects.gas_object().0 .1]);

    let _effects = call_move(
        &authority_state,
        &gas_object_id,
        &sender,
        &sender_key,
        &pkg_ref.0,
        "object_basics",
        "delete",
        vec![],
        vec![TestCallArg::Object(new_object_id1)],
    )
    .await
    .unwrap();

    // Test get_latest_parent_entry function

    // The objects just after the gas object also returns None
    let mut x = gas_object_id.to_vec();
    let last_index = x.len() - 1;
    // Prevent overflow
    x[last_index] = u8::MAX - x[last_index];
    let unknown_object_id: ObjectID = x.try_into().unwrap();
    assert!(authority_state
        .get_object_or_tombstone(unknown_object_id)
        .await
        .unwrap()
        .is_none());

    // Check gas object is returned.
    let obj_ref = authority_state
        .get_object_or_tombstone(gas_object_id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(obj_ref.0, gas_object_id);
    assert_eq!(obj_ref.1, delete_version);

    // Check entry for deleted object is returned
    let obj_ref = authority_state
        .get_object_or_tombstone(new_object_id1)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(obj_ref.0, new_object_id1);
    assert_eq!(obj_ref.1, delete_version);
    assert_eq!(obj_ref.2, ObjectDigest::OBJECT_DIGEST_DELETED);
}

#[tokio::test]
async fn test_account_state_ok() {
    let sender = dbg_addr(1);
    let object_id = dbg_object_id(1);

    let authority_state = init_state_with_object_id(sender, object_id).await;
    authority_state
        .get_object(&object_id)
        .await
        .unwrap()
        .unwrap();
}

#[tokio::test]
async fn test_account_state_unknown_account() {
    let sender = dbg_addr(1);
    let unknown_address = dbg_object_id(99);
    let authority_state = init_state_with_object_id(sender, ObjectID::random()).await;
    assert!(authority_state
        .get_object(&unknown_address)
        .await
        .unwrap()
        .is_none());
}

#[tokio::test]
async fn test_authority_persist() {
    async fn init_state(
        genesis: &Genesis,
        authority_key: AuthorityKeyPair,
        store: Arc<AuthorityStore>,
    ) -> Arc<AuthorityState> {
        TestAuthorityBuilder::new()
            .with_genesis_and_keypair(genesis, &authority_key)
            .with_store(store)
            .build()
            .await
    }

    let seed = [1u8; 32];
    let (genesis, authority_key) = init_state_parameters_from_rng(&mut StdRng::from_seed(seed));
    let committee = genesis.committee().unwrap();

    // Create a random directory to store the DB
    let dir = env::temp_dir();
    let path = dir.join(format!("DB_{:?}", ObjectID::random()));
    fs::create_dir(&path).unwrap();

    // Create an authority
    let store =
        AuthorityStore::open_with_committee_for_testing(&path, None, &committee, &genesis, 0)
            .await
            .unwrap();
    let authority = init_state(&genesis, authority_key, store).await;

    // Create an object
    let recipient = dbg_addr(2);
    let object_id = ObjectID::random();
    let obj = Object::with_id_owner_for_testing(object_id, recipient);

    // Store an object
    authority.insert_genesis_object(obj).await;

    // Close the authority
    drop(authority);

    // TODO: The right fix is to invoke some function on DBMap and release the rocksdb arc references
    // being held in the background thread but this will suffice for now
    tokio::time::sleep(std::time::Duration::from_secs(1)).await;

    // Reopen the same authority with the same path
    let seed = [1u8; 32];
    let (genesis, authority_key) = init_state_parameters_from_rng(&mut StdRng::from_seed(seed));
    let committee = genesis.committee().unwrap();
    let store =
        AuthorityStore::open_with_committee_for_testing(&path, None, &committee, &genesis, 0)
            .await
            .unwrap();
    let authority2 = init_state(&genesis, authority_key, store).await;
    let obj2 = authority2.get_object(&object_id).await.unwrap().unwrap();

    // Check the object is present
    assert_eq!(obj2.id(), object_id);
    assert_eq!(obj2.owner, recipient);
}

#[tokio::test]
async fn test_idempotent_reversed_confirmation() {
    // In this test we exercise the case where an authority first receive the certificate,
    // and then receive the raw transaction latter. We should still ensure idempotent
    // response and be able to get back the same result.
    let recipient = dbg_addr(2);
    let (sender, sender_key): (_, AccountKeyPair) = get_key_pair();

    let object = Object::with_owner_for_testing(sender);
    let object_ref = object.compute_object_reference();
    let gas_object = Object::with_owner_for_testing(sender);
    let gas_object_ref = gas_object.compute_object_reference();
    let authority_state = init_state_with_objects([object, gas_object]).await;
    let epoch_store = authority_state.load_epoch_store_one_call_per_task();

    let certified_transfer_transaction = init_certified_transfer_transaction(
        sender,
        &sender_key,
        recipient,
        object_ref,
        gas_object_ref,
        &authority_state,
    );
    let result1 = authority_state
        .execute_certificate(
            &certified_transfer_transaction,
            &authority_state.epoch_store_for_testing(),
        )
        .await;
    assert!(result1.is_ok());
    let result2 = authority_state
        .handle_transaction(&epoch_store, certified_transfer_transaction.into_unsigned())
        .await;
    assert!(result2.is_ok());
    assert_eq!(
        result1.unwrap().into_message(),
        result2
            .unwrap()
            .status
            .into_effects_for_testing()
            .into_data()
    );
}

#[tokio::test]
async fn test_refusal_to_sign_consensus_commit_prologue() {
    // The system should refuse to handle sender-signed system transactions
    let (sender, sender_key): (_, AccountKeyPair) = get_key_pair();
    let gas_object_id = ObjectID::random();
    let gas_object = Object::with_id_owner_for_testing(gas_object_id, sender);
    let authority_state = init_state_with_objects(vec![gas_object.clone()]).await;
    let rgp = authority_state.reference_gas_price_for_testing().unwrap();
    let epoch_store = authority_state.load_epoch_store_one_call_per_task();

    let gas_ref = gas_object.compute_object_reference();
    let tx_data = TransactionData::new(
        TransactionKind::ConsensusCommitPrologue(ConsensusCommitPrologue {
            epoch: 0,
            round: 0,
            commit_timestamp_ms: 42,
        }),
        sender,
        gas_ref,
        TEST_ONLY_GAS_UNIT_FOR_GENERIC * rgp,
        rgp,
    );

    // Sender is able to sign it.
    let transaction = to_sender_signed_transaction(tx_data, &sender_key);

    // But the authority should refuse to handle it.
    assert!(matches!(
        authority_state
            .handle_transaction(&epoch_store, transaction)
            .await,
        Err(SuiError::InvalidSystemTransaction),
    ));
}

#[tokio::test]
async fn test_invalid_mutable_clock_parameter() {
    // User transactions that take the singleton Clock object at `0x6` by mutable reference will
    // fail to sign, to prevent transactions bottlenecking on it.
    let (sender, sender_key): (_, AccountKeyPair) = get_key_pair();
    let gas_object_id = ObjectID::random();
    let (authority_state, package_object_ref) =
        init_state_with_ids_and_object_basics(vec![(sender, gas_object_id)]).await;
    let epoch_store = authority_state.load_epoch_store_one_call_per_task();
    let gas_object = Object::with_id_owner_for_testing(gas_object_id, sender);
    let gas_ref = gas_object.compute_object_reference();

    let rgp = authority_state.reference_gas_price_for_testing().unwrap();
    let tx_data = TransactionData::new_move_call(
        sender,
        package_object_ref.0,
        ident_str!("object_basics").to_owned(),
        ident_str!("use_clock").to_owned(),
        /* type_args */ vec![],
        gas_ref,
        vec![CallArg::Object(ObjectArg::SharedObject {
            id: SUI_CLOCK_OBJECT_ID,
            initial_shared_version: SUI_CLOCK_OBJECT_SHARED_VERSION,
            mutable: true,
        })],
        TEST_ONLY_GAS_UNIT_FOR_OBJECT_BASICS * rgp,
        rgp,
    )
    .unwrap();

    let transaction = to_sender_signed_transaction(tx_data, &sender_key);

    let Err(e) = authority_state.handle_transaction(&epoch_store, transaction).await else {
        panic!("Expected handling transaction to fail");
    };

    assert_eq!(
        UserInputError::try_from(e).unwrap(),
        UserInputError::ImmutableParameterExpectedError {
            object_id: SUI_CLOCK_OBJECT_ID
        }
    );
}

#[tokio::test]
async fn test_valid_immutable_clock_parameter() {
    // User transactions can take an immutable reference of the singleton Clock.
    let (sender, sender_key): (_, AccountKeyPair) = get_key_pair();
    let gas_object_id = ObjectID::random();
    let (authority_state, package_object_ref) =
        init_state_with_ids_and_object_basics(vec![(sender, gas_object_id)]).await;
    let epoch_store = authority_state.load_epoch_store_one_call_per_task();
    let gas_object = Object::with_id_owner_for_testing(gas_object_id, sender);
    let gas_ref = gas_object.compute_object_reference();

    let rgp = authority_state.reference_gas_price_for_testing().unwrap();
    let tx_data = TransactionData::new_move_call(
        sender,
        package_object_ref.0,
        ident_str!("object_basics").to_owned(),
        ident_str!("use_clock").to_owned(),
        /* type_args */ vec![],
        gas_ref,
        vec![CallArg::Object(ObjectArg::SharedObject {
            id: SUI_CLOCK_OBJECT_ID,
            initial_shared_version: SUI_CLOCK_OBJECT_SHARED_VERSION,
            mutable: false,
        })],
        TEST_ONLY_GAS_UNIT_FOR_OBJECT_BASICS * rgp,
        rgp,
    )
    .unwrap();

    let transaction = to_sender_signed_transaction(tx_data, &sender_key);
    authority_state
        .handle_transaction(&epoch_store, transaction)
        .await
        .unwrap();
}

#[tokio::test]
async fn test_genesis_sui_system_state_object() {
    // This test verifies that we can read the genesis SuiSystemState object.
    // And its Move layout matches the definition in Rust (so that we can deserialize it).
    let authority_state = TestAuthorityBuilder::new().build().await;
    let wrapper = authority_state
        .get_object(&SUI_SYSTEM_STATE_OBJECT_ID)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(wrapper.version(), SequenceNumber::from(1));
    let move_object = wrapper.data.try_as_move().unwrap();
    let _sui_system_state =
        bcs::from_bytes::<SuiSystemStateWrapper>(move_object.contents()).unwrap();
    assert!(move_object.type_().is(&SuiSystemStateWrapper::type_()));
    let sui_system_state = authority_state
        .database
        .get_sui_system_state_object()
        .unwrap();
    assert_eq!(
        &sui_system_state.get_current_epoch_committee().committee,
        authority_state
            .epoch_store_for_testing()
            .committee()
            .as_ref()
    );
}

#[tokio::test]
async fn test_transfer_sui_no_amount() {
    let (sender, sender_key): (_, AccountKeyPair) = get_key_pair();
    let recipient = dbg_addr(2);
    let gas_object_id = ObjectID::random();
    let gas_object = Object::with_id_owner_for_testing(gas_object_id, sender);
    let init_balance = sui_types::gas::get_gas_balance(&gas_object).unwrap();
    let authority_state = init_state_with_objects(vec![gas_object.clone()]).await;

    let epoch_store = authority_state.load_epoch_store_one_call_per_task();
    let rgp = epoch_store.reference_gas_price();

    let gas_ref = gas_object.compute_object_reference();
    let tx_data = TransactionData::new_transfer_sui(
        recipient,
        sender,
        None,
        gas_ref,
        rgp * TEST_ONLY_GAS_UNIT_FOR_TRANSFER,
        rgp,
    );

    // Make sure transaction handling works as usual.
    let transaction = to_sender_signed_transaction(tx_data, &sender_key);
    authority_state
        .handle_transaction(&epoch_store, transaction.clone())
        .await
        .unwrap();

    let certificate = init_certified_transaction(transaction, &authority_state);
    let signed_effects = authority_state
        .execute_certificate(&certificate, &authority_state.epoch_store_for_testing())
        .await
        .unwrap();
    let effects = signed_effects.into_message();
    // Check that the transaction was successful, and the gas object is the only mutated object,
    // and got transferred. Also check on its version and new balance.
    assert!(effects.status().is_ok());
    assert!(effects.mutated_excluding_gas().is_empty());
    assert!(gas_ref.1 < effects.gas_object().0 .1);
    assert_eq!(effects.gas_object().1, Owner::AddressOwner(recipient));
    let new_balance = sui_types::gas::get_gas_balance(
        &authority_state
            .get_object(&gas_object_id)
            .await
            .unwrap()
            .unwrap(),
    )
    .unwrap();
    assert_eq!(
        new_balance as i64 + effects.gas_cost_summary().net_gas_usage(),
        init_balance as i64
    );
}

#[tokio::test]
async fn test_transfer_sui_with_amount() {
    let (sender, sender_key): (_, AccountKeyPair) = get_key_pair();
    let recipient = dbg_addr(2);
    let gas_object_id = ObjectID::random();
    let gas_object = Object::with_id_owner_for_testing(gas_object_id, sender);
    let init_balance = sui_types::gas::get_gas_balance(&gas_object).unwrap();
    let authority_state = init_state_with_objects(vec![gas_object.clone()]).await;
    let rgp = authority_state.reference_gas_price_for_testing().unwrap();

    let gas_ref = gas_object.compute_object_reference();
    let tx_data = TransactionData::new_transfer_sui(
        recipient,
        sender,
        Some(500),
        gas_ref,
        rgp * TEST_ONLY_GAS_UNIT_FOR_TRANSFER,
        rgp,
    );
    let transaction = to_sender_signed_transaction(tx_data, &sender_key);
    let certificate = init_certified_transaction(transaction, &authority_state);
    let signed_effects = authority_state
        .execute_certificate(&certificate, &authority_state.epoch_store_for_testing())
        .await
        .unwrap();
    let effects = signed_effects.into_message();
    // Check that the transaction was successful, the gas object remains in the original owner,
    // and an amount is split out and send to the recipient.
    assert!(effects.status().is_ok());
    assert!(effects.mutated_excluding_gas().is_empty());
    assert_eq!(effects.created().len(), 1);
    assert_eq!(effects.created()[0].1, Owner::AddressOwner(recipient));
    let new_gas = authority_state
        .get_object(&effects.created()[0].0 .0)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(sui_types::gas::get_gas_balance(&new_gas).unwrap(), 500);
    assert!(gas_ref.1 < effects.gas_object().0 .1);
    assert_eq!(effects.gas_object().1, Owner::AddressOwner(sender));
    let new_balance = sui_types::gas::get_gas_balance(
        &authority_state
            .get_object(&gas_object_id)
            .await
            .unwrap()
            .unwrap(),
    )
    .unwrap();
    assert_eq!(
        new_balance as i64 + effects.gas_cost_summary().net_gas_usage() + 500,
        init_balance as i64
    );
}

#[tokio::test]
async fn test_store_revert_transfer_sui() {
    // This test checks the correctness of revert_state_update in SuiDataStore.
    let (sender, sender_key): (_, AccountKeyPair) = get_key_pair();
    let (recipient, _sender_key): (_, AccountKeyPair) = get_key_pair();
    let gas_object_id = ObjectID::random();
    let gas_object = Object::with_id_owner_for_testing(gas_object_id, sender);
    let gas_object_ref = gas_object.compute_object_reference();
    let authority_state = init_state_with_objects(vec![gas_object.clone()]).await;
    let rgp = authority_state.reference_gas_price_for_testing().unwrap();

    let tx_data = TransactionData::new_transfer_sui(
        recipient,
        sender,
        None,
        gas_object.compute_object_reference(),
        rgp * TEST_ONLY_GAS_UNIT_FOR_TRANSFER,
        rgp,
    );

    let transaction = to_sender_signed_transaction(tx_data, &sender_key);
    let certificate = init_certified_transaction(transaction, &authority_state);
    let tx_digest = *certificate.digest();
    authority_state
        .execute_certificate(&certificate, &authority_state.epoch_store_for_testing())
        .await
        .unwrap();

    let db = &authority_state.database;
    db.revert_state_update(&tx_digest).await.unwrap();

    assert_eq!(
        db.get_object(&gas_object_id).unwrap().unwrap().owner,
        Owner::AddressOwner(sender),
    );
    assert_eq!(
        db.get_object_or_tombstone(gas_object_id).unwrap().unwrap(),
        gas_object_ref
    );
    // Transaction should not be deleted on revert in case it's needed
    // to execute a future state sync checkpoint.
    assert!(db.get_transaction_block(&tx_digest).unwrap().is_some());
    assert!(!db.as_ref().is_tx_already_executed(&tx_digest).unwrap());
}

#[tokio::test]
async fn test_store_revert_wrap_move_call() {
    let (sender, sender_key): (_, AccountKeyPair) = get_key_pair();
    let gas_object_id = ObjectID::random();
    let (authority_state, object_basics) =
        init_state_with_ids_and_object_basics(vec![(sender, gas_object_id)]).await;

    let rgp = authority_state.reference_gas_price_for_testing().unwrap();
    let create_effects = create_move_object(
        &object_basics.0,
        &authority_state,
        &gas_object_id,
        &sender,
        &sender_key,
    )
    .await
    .unwrap();

    assert!(create_effects.status().is_ok());
    assert_eq!(create_effects.created().len(), 1);

    let object_v0 = create_effects.created()[0].0;

    let wrap_txn = to_sender_signed_transaction(
        TransactionData::new_move_call(
            sender,
            object_basics.0,
            ident_str!("object_basics").to_owned(),
            ident_str!("wrap").to_owned(),
            vec![],
            create_effects.gas_object().0,
            vec![CallArg::Object(ObjectArg::ImmOrOwnedObject(object_v0))],
            TEST_ONLY_GAS_UNIT_FOR_OBJECT_BASICS * rgp,
            rgp,
        )
        .unwrap(),
        &sender_key,
    );

    let wrap_cert = init_certified_transaction(wrap_txn, &authority_state);
    let wrap_digest = *wrap_cert.digest();

    let wrap_effects = authority_state
        .execute_certificate(&wrap_cert, &authority_state.epoch_store_for_testing())
        .await
        .unwrap()
        .into_message();

    assert!(wrap_effects.status().is_ok());
    assert_eq!(wrap_effects.created().len(), 1);
    assert_eq!(wrap_effects.wrapped().len(), 1);
    assert_eq!(wrap_effects.wrapped()[0].0, object_v0.0);

    let wrapper_v0 = wrap_effects.created()[0].0;

    let db = &authority_state.database;
    db.revert_state_update(&wrap_digest).await.unwrap();

    // The wrapped object is unwrapped once again (accessible from storage).
    let object = db.get_object(&object_v0.0).unwrap().unwrap();
    assert_eq!(object.version(), object_v0.1);

    // The wrapper doesn't exist
    assert!(db.get_object(&wrapper_v0.0).unwrap().is_none());

    // The gas is uncharged
    let gas = db.get_object(&gas_object_id).unwrap().unwrap();
    assert_eq!(gas.version(), create_effects.gas_object().0 .1);
}

#[tokio::test]
async fn test_store_revert_unwrap_move_call() {
    let (sender, sender_key): (_, AccountKeyPair) = get_key_pair();
    let gas_object_id = ObjectID::random();
    let (authority_state, object_basics) =
        init_state_with_ids_and_object_basics(vec![(sender, gas_object_id)]).await;

    let rgp = authority_state.reference_gas_price_for_testing().unwrap();
    let create_effects = create_move_object(
        &object_basics.0,
        &authority_state,
        &gas_object_id,
        &sender,
        &sender_key,
    )
    .await
    .unwrap();

    assert!(create_effects.status().is_ok());
    assert_eq!(create_effects.created().len(), 1);

    let object_v0 = create_effects.created()[0].0;

    let wrap_effects = wrap_object(
        &object_basics.0,
        &authority_state,
        &object_v0.0,
        &gas_object_id,
        &sender,
        &sender_key,
    )
    .await
    .unwrap();

    assert!(wrap_effects.status().is_ok());
    assert_eq!(wrap_effects.created().len(), 1);
    assert_eq!(wrap_effects.wrapped().len(), 1);
    assert_eq!(wrap_effects.wrapped()[0].0, object_v0.0);

    let wrapper_v0 = wrap_effects.created()[0].0;

    let unwrap_txn = to_sender_signed_transaction(
        TransactionData::new_move_call(
            sender,
            object_basics.0,
            ident_str!("object_basics").to_owned(),
            ident_str!("unwrap").to_owned(),
            vec![],
            wrap_effects.gas_object().0,
            vec![CallArg::Object(ObjectArg::ImmOrOwnedObject(wrapper_v0))],
            TEST_ONLY_GAS_UNIT_FOR_OBJECT_BASICS * rgp,
            rgp,
        )
        .unwrap(),
        &sender_key,
    );

    let unwrap_cert = init_certified_transaction(unwrap_txn, &authority_state);
    let unwrap_digest = *unwrap_cert.digest();

    let unwrap_effects = authority_state
        .execute_certificate(&unwrap_cert, &authority_state.epoch_store_for_testing())
        .await
        .unwrap()
        .into_message();

    assert!(unwrap_effects.status().is_ok());
    assert_eq!(unwrap_effects.deleted().len(), 1);
    assert_eq!(unwrap_effects.deleted()[0].0, wrapper_v0.0);
    assert_eq!(unwrap_effects.unwrapped().len(), 1);
    assert_eq!(unwrap_effects.unwrapped()[0].0 .0, object_v0.0);

    let db = &authority_state.database;

    db.revert_state_update(&unwrap_digest).await.unwrap();

    // The unwrapped object is wrapped once again
    assert!(db.get_object(&object_v0.0).unwrap().is_none());

    // The wrapper exists
    let wrapper = db.get_object(&wrapper_v0.0).unwrap().unwrap();
    assert_eq!(wrapper.version(), wrapper_v0.1);

    // The gas is uncharged
    let gas = db.get_object(&gas_object_id).unwrap().unwrap();
    assert_eq!(gas.version(), wrap_effects.gas_object().0 .1);
}
#[tokio::test]
async fn test_store_get_dynamic_object() {
    let (_, fields) = create_and_retrieve_df_info(ident_str!("add_ofield")).await;
    assert_eq!(fields.len(), 1);
    assert_eq!(fields[0].type_, DynamicFieldType::DynamicObject);
}

#[tokio::test]
async fn test_store_get_dynamic_field() {
    let (_, fields) = create_and_retrieve_df_info(ident_str!("add_field")).await;

    assert_eq!(fields.len(), 1);
    assert!(matches!(fields[0].type_, DynamicFieldType::DynamicField));
    assert_eq!(json!(true), fields[0].name.value);
    assert_eq!(TypeTag::Bool, fields[0].name.type_)
}

async fn create_and_retrieve_df_info(function: &IdentStr) -> (SuiAddress, Vec<DynamicFieldInfo>) {
    let (sender, sender_key): (_, AccountKeyPair) = get_key_pair();
    let gas_object_id = ObjectID::random();
    let (authority_state, object_basics) =
        init_state_with_ids_and_object_basics(vec![(sender, gas_object_id)]).await;

    let rgp = authority_state.reference_gas_price_for_testing().unwrap();
    let create_outer_effects = create_move_object(
        &object_basics.0,
        &authority_state,
        &gas_object_id,
        &sender,
        &sender_key,
    )
    .await
    .unwrap();

    assert!(
        create_outer_effects.status().is_ok(),
        "{:?}",
        create_outer_effects
    );
    assert_eq!(create_outer_effects.created().len(), 1);

    let create_inner_effects = create_move_object(
        &object_basics.0,
        &authority_state,
        &gas_object_id,
        &sender,
        &sender_key,
    )
    .await
    .unwrap();

    assert!(create_inner_effects.status().is_ok());
    assert_eq!(create_inner_effects.created().len(), 1);

    let outer_v0 = create_outer_effects.created()[0].0;
    let inner_v0 = create_inner_effects.created()[0].0;

    let add_txn = to_sender_signed_transaction(
        TransactionData::new_move_call(
            sender,
            object_basics.0,
            ident_str!("object_basics").to_owned(),
            function.to_owned(),
            vec![],
            create_inner_effects.gas_object().0,
            vec![
                CallArg::Object(ObjectArg::ImmOrOwnedObject(outer_v0)),
                CallArg::Object(ObjectArg::ImmOrOwnedObject(inner_v0)),
            ],
            TEST_ONLY_GAS_UNIT_FOR_OBJECT_BASICS * rgp,
            rgp,
        )
        .unwrap(),
        &sender_key,
    );

    let add_cert = init_certified_transaction(add_txn, &authority_state);

    let add_effects = authority_state
        .try_execute_for_test(&add_cert)
        .await
        .unwrap()
        .0
        .into_message();

    assert!(add_effects.status().is_ok(), "{:?}", add_effects.status());
    assert_eq!(add_effects.created().len(), 1);

    (
        sender,
        authority_state
            .get_dynamic_fields(outer_v0.0, None, usize::MAX)
            .unwrap(),
    )
}

#[tokio::test]
async fn test_dynamic_field_struct_name_parsing() {
    let (_, fields) = create_and_retrieve_df_info(ident_str!("add_field_with_struct_name")).await;

    assert_eq!(fields.len(), 1);
    assert!(matches!(fields[0].type_, DynamicFieldType::DynamicField));
    assert_eq!(json!({"name_str": "Test Name"}), fields[0].name.value);
    assert_eq!(
        parse_type_tag("0x0::object_basics::Name").unwrap(),
        fields[0].name.type_
    )
}

#[tokio::test]
async fn test_dynamic_field_bytearray_name_parsing() {
    let (_, fields) =
        create_and_retrieve_df_info(ident_str!("add_field_with_bytearray_name")).await;

    assert_eq!(fields.len(), 1);
    assert!(matches!(fields[0].type_, DynamicFieldType::DynamicField));
    assert_eq!(parse_type_tag("vector<u8>").unwrap(), fields[0].name.type_);
    assert_eq!(json!("Test Name".as_bytes()), fields[0].name.value);
}

#[tokio::test]
async fn test_dynamic_field_address_name_parsing() {
    let (sender, fields) =
        create_and_retrieve_df_info(ident_str!("add_field_with_address_name")).await;

    assert_eq!(fields.len(), 1);
    assert!(matches!(fields[0].type_, DynamicFieldType::DynamicField));
    assert_eq!(parse_type_tag("address").unwrap(), fields[0].name.type_);
    assert_eq!(json!(sender), fields[0].name.value);
}

#[tokio::test]
async fn test_dynamic_object_field_struct_name_parsing() {
    let (_, fields) = create_and_retrieve_df_info(ident_str!("add_ofield_with_struct_name")).await;

    assert_eq!(fields.len(), 1);
    assert!(matches!(fields[0].type_, DynamicFieldType::DynamicObject));
    assert_eq!(json!({"name_str": "Test Name"}), fields[0].name.value);
    assert_eq!(
        parse_type_tag("0x0::object_basics::Name").unwrap(),
        fields[0].name.type_
    )
}

#[tokio::test]
async fn test_dynamic_object_field_bytearray_name_parsing() {
    let (_, fields) =
        create_and_retrieve_df_info(ident_str!("add_ofield_with_bytearray_name")).await;

    assert_eq!(fields.len(), 1);
    assert!(matches!(fields[0].type_, DynamicFieldType::DynamicObject));
    assert_eq!(parse_type_tag("vector<u8>").unwrap(), fields[0].name.type_);
    assert_eq!(json!("Test Name".as_bytes()), fields[0].name.value);
}

#[tokio::test]
async fn test_dynamic_object_field_address_name_parsing() {
    let (sender, fields) =
        create_and_retrieve_df_info(ident_str!("add_ofield_with_address_name")).await;

    assert_eq!(fields.len(), 1);
    assert!(matches!(fields[0].type_, DynamicFieldType::DynamicObject));
    assert_eq!(parse_type_tag("address").unwrap(), fields[0].name.type_);
    assert_eq!(json!(sender), fields[0].name.value);
}

#[tokio::test]
async fn test_store_revert_add_ofield() {
    let (sender, sender_key): (_, AccountKeyPair) = get_key_pair();
    let gas_object_id = ObjectID::random();
    let (authority_state, object_basics) =
        init_state_with_ids_and_object_basics(vec![(sender, gas_object_id)]).await;

    let rgp = authority_state.reference_gas_price_for_testing().unwrap();
    let create_outer_effects = create_move_object(
        &object_basics.0,
        &authority_state,
        &gas_object_id,
        &sender,
        &sender_key,
    )
    .await
    .unwrap();

    assert!(create_outer_effects.status().is_ok());
    assert_eq!(create_outer_effects.created().len(), 1);

    let create_inner_effects = create_move_object(
        &object_basics.0,
        &authority_state,
        &gas_object_id,
        &sender,
        &sender_key,
    )
    .await
    .unwrap();

    assert!(create_inner_effects.status().is_ok());
    assert_eq!(create_inner_effects.created().len(), 1);

    let outer_v0 = create_outer_effects.created()[0].0;
    let inner_v0 = create_inner_effects.created()[0].0;

    let add_txn = to_sender_signed_transaction(
        TransactionData::new_move_call(
            sender,
            object_basics.0,
            ident_str!("object_basics").to_owned(),
            ident_str!("add_ofield").to_owned(),
            vec![],
            create_inner_effects.gas_object().0,
            vec![
                CallArg::Object(ObjectArg::ImmOrOwnedObject(outer_v0)),
                CallArg::Object(ObjectArg::ImmOrOwnedObject(inner_v0)),
            ],
            TEST_ONLY_GAS_UNIT_FOR_OBJECT_BASICS * rgp,
            rgp,
        )
        .unwrap(),
        &sender_key,
    );

    let add_cert = init_certified_transaction(add_txn, &authority_state);
    let add_digest = *add_cert.digest();

    let add_effects = authority_state
        .execute_certificate(&add_cert, &authority_state.epoch_store_for_testing())
        .await
        .unwrap()
        .into_message();

    assert!(add_effects.status().is_ok());
    assert_eq!(add_effects.created().len(), 1);

    let field_v0 = add_effects.created()[0].0;
    let outer_v1 = find_by_id(add_effects.mutated(), outer_v0.0).unwrap();
    let inner_v1 = find_by_id(add_effects.mutated(), inner_v0.0).unwrap();

    let db = &authority_state.database;

    let outer = db.get_object(&outer_v0.0).unwrap().unwrap();
    assert_eq!(outer.version(), outer_v1.1);

    let field = db.get_object(&field_v0.0).unwrap().unwrap();
    assert_eq!(field.owner, Owner::ObjectOwner(outer_v0.0.into()));

    let inner = db.get_object(&inner_v0.0).unwrap().unwrap();
    assert_eq!(inner.version(), inner_v1.1);
    assert_eq!(inner.owner, Owner::ObjectOwner(field_v0.0.into()));

    db.revert_state_update(&add_digest).await.unwrap();

    let outer = db.get_object(&outer_v0.0).unwrap().unwrap();
    assert_eq!(outer.version(), outer_v0.1);

    // Field no longer exists
    assert!(db.get_object(&field_v0.0).unwrap().is_none());

    let inner = db.get_object(&inner_v0.0).unwrap().unwrap();
    assert_eq!(inner.version(), inner_v0.1);
    assert_eq!(inner.owner, Owner::AddressOwner(sender));
}

#[tokio::test]
async fn test_store_revert_remove_ofield() {
    let (sender, sender_key): (_, AccountKeyPair) = get_key_pair();
    let gas_object_id = ObjectID::random();
    let (authority_state, object_basics) =
        init_state_with_ids_and_object_basics(vec![(sender, gas_object_id)]).await;

    let rgp = authority_state.reference_gas_price_for_testing().unwrap();
    let create_outer_effects = create_move_object(
        &object_basics.0,
        &authority_state,
        &gas_object_id,
        &sender,
        &sender_key,
    )
    .await
    .unwrap();

    assert!(create_outer_effects.status().is_ok());
    assert_eq!(create_outer_effects.created().len(), 1);

    let create_inner_effects = create_move_object(
        &object_basics.0,
        &authority_state,
        &gas_object_id,
        &sender,
        &sender_key,
    )
    .await
    .unwrap();

    assert!(create_inner_effects.status().is_ok());
    assert_eq!(create_inner_effects.created().len(), 1);

    let outer_v0 = create_outer_effects.created()[0].0;
    let inner_v0 = create_inner_effects.created()[0].0;

    let add_effects = add_ofield(
        &object_basics.0,
        &authority_state,
        &outer_v0.0,
        &inner_v0.0,
        &gas_object_id,
        &sender,
        &sender_key,
    )
    .await
    .unwrap();

    assert!(add_effects.status().is_ok());
    assert_eq!(add_effects.created().len(), 1);

    let field_v0 = add_effects.created()[0].0;
    let outer_v1 = find_by_id(add_effects.mutated(), outer_v0.0).unwrap();
    let inner_v1 = find_by_id(add_effects.mutated(), inner_v0.0).unwrap();

    let remove_ofield_txn = to_sender_signed_transaction(
        TransactionData::new_move_call(
            sender,
            object_basics.0,
            ident_str!("object_basics").to_owned(),
            ident_str!("remove_ofield").to_owned(),
            vec![],
            add_effects.gas_object().0,
            vec![CallArg::Object(ObjectArg::ImmOrOwnedObject(outer_v1))],
            TEST_ONLY_GAS_UNIT_FOR_OBJECT_BASICS * rgp,
            rgp,
        )
        .unwrap(),
        &sender_key,
    );

    let remove_ofield_cert = init_certified_transaction(remove_ofield_txn, &authority_state);
    let remove_ofield_digest = *remove_ofield_cert.digest();

    let remove_effects = authority_state
        .execute_certificate(
            &remove_ofield_cert,
            &authority_state.epoch_store_for_testing(),
        )
        .await
        .unwrap()
        .into_message();

    assert!(remove_effects.status().is_ok());
    let outer_v2 = find_by_id(remove_effects.mutated(), outer_v0.0).unwrap();
    let inner_v2 = find_by_id(remove_effects.mutated(), inner_v0.0).unwrap();

    let db = &authority_state.database;

    let outer = db.get_object(&outer_v0.0).unwrap().unwrap();
    assert_eq!(outer.version(), outer_v2.1);

    let inner = db.get_object(&inner_v0.0).unwrap().unwrap();
    assert_eq!(inner.owner, Owner::AddressOwner(sender));
    assert_eq!(inner.version(), inner_v2.1);

    db.revert_state_update(&remove_ofield_digest).await.unwrap();

    let outer = db.get_object(&outer_v0.0).unwrap().unwrap();
    assert_eq!(outer.version(), outer_v1.1);

    let field = db.get_object(&field_v0.0).unwrap().unwrap();
    assert_eq!(field.owner, Owner::ObjectOwner(outer_v0.0.into()));

    let inner = db.get_object(&inner_v0.0).unwrap().unwrap();
    assert_eq!(inner.owner, Owner::ObjectOwner(field_v0.0.into()));
    assert_eq!(inner.version(), inner_v1.1);
}

#[tokio::test]
async fn test_iter_live_object_set() {
    let (sender, sender_key): (_, AccountKeyPair) = get_key_pair();
    let (receiver, _): (_, AccountKeyPair) = get_key_pair();
    let gas = ObjectID::random();
    let obj_id = ObjectID::random();
    let authority = init_state_with_ids(vec![(sender, gas), (sender, obj_id)]).await;
    let starting_live_set: HashSet<_> = authority
        .database
        .iter_live_object_set()
        .filter_map(|object| {
            let id = object.object_id();
            if id != gas && id != obj_id {
                Some(id)
            } else {
                None
            }
        })
        .collect();

    let gas_obj = authority.get_object(&gas).await.unwrap().unwrap();
    let obj = authority.get_object(&obj_id).await.unwrap().unwrap();

    let certified_transfer_transaction = init_certified_transfer_transaction(
        sender,
        &sender_key,
        receiver,
        obj.compute_object_reference(),
        gas_obj.compute_object_reference(),
        &authority,
    );
    authority
        .execute_certificate(
            &certified_transfer_transaction,
            &authority.epoch_store_for_testing(),
        )
        .await
        .unwrap();

    let (package, upgrade_cap) = build_and_publish_test_package_with_upgrade_cap(
        &authority,
        &sender,
        &sender_key,
        &gas,
        "object_wrapping",
        /* with_unpublished_deps */ false,
    )
    .await;

    // Create a Child object.
    let effects = call_move(
        &authority,
        &gas,
        &sender,
        &sender_key,
        &package.0,
        "object_wrapping",
        "create_child",
        vec![],
        vec![],
    )
    .await
    .unwrap();
    assert!(
        matches!(effects.status(), ExecutionStatus::Success { .. }),
        "{:?}",
        effects.status()
    );
    let child_object_ref = effects.created()[0].0;

    // Create a Parent object, by wrapping the child object.
    let effects = call_move(
        &authority,
        &gas,
        &sender,
        &sender_key,
        &package.0,
        "object_wrapping",
        "create_parent",
        vec![],
        vec![TestCallArg::Object(child_object_ref.0)],
    )
    .await
    .unwrap();
    assert!(
        matches!(effects.status(), ExecutionStatus::Success { .. }),
        "{:?}",
        effects.status()
    );
    // Child object is wrapped, Parent object is created().
    assert_eq!(
        (
            effects.created().len(),
            effects.deleted().len(),
            effects.wrapped().len()
        ),
        (1, 0, 1)
    );

    let parent_object_ref = effects.created()[0].0;

    // Extract the child out of the parent.
    let effects = call_move(
        &authority,
        &gas,
        &sender,
        &sender_key,
        &package.0,
        "object_wrapping",
        "extract_child",
        vec![],
        vec![TestCallArg::Object(parent_object_ref.0)],
    )
    .await
    .unwrap();
    assert!(
        matches!(effects.status(), ExecutionStatus::Success { .. }),
        "{:?}",
        effects.status()
    );

    // Make sure that version increments again when unwrapped.
    let child_object_ref = effects.unwrapped()[0].0;

    // Wrap the child to the parent again.
    let effects = call_move(
        &authority,
        &gas,
        &sender,
        &sender_key,
        &package.0,
        "object_wrapping",
        "set_child",
        vec![],
        vec![
            TestCallArg::Object(parent_object_ref.0),
            TestCallArg::Object(child_object_ref.0),
        ],
    )
    .await
    .unwrap();
    assert!(
        matches!(effects.status(), ExecutionStatus::Success { .. }),
        "{:?}",
        effects.status()
    );
    let parent_object_ref = effects.mutated_excluding_gas().first().unwrap().0;

    // Now delete the parent object, which will in turn delete the child object.
    let effects = call_move(
        &authority,
        &gas,
        &sender,
        &sender_key,
        &package.0,
        "object_wrapping",
        "delete_parent",
        vec![],
        vec![TestCallArg::Object(parent_object_ref.0)],
    )
    .await
    .unwrap();
    assert!(
        matches!(effects.status(), ExecutionStatus::Success { .. }),
        "{:?}",
        effects.status()
    );

    check_live_set(
        &authority,
        &starting_live_set,
        &[
            (package.0, package.1),
            (gas, SequenceNumber::from_u64(8)),
            (obj_id, SequenceNumber::from_u64(2)),
            (upgrade_cap.0, upgrade_cap.1),
        ],
    );
}

// helpers

#[cfg(test)]
fn check_live_set(
    authority: &AuthorityState,
    ignore: &HashSet<ObjectID>,
    expected_live_set: &[(ObjectID, SequenceNumber)],
) {
    let mut expected: Vec<_> = expected_live_set.into();
    expected.sort();

    let actual: Vec<_> = authority
        .database
        .iter_live_object_set()
        .filter_map(|object| {
            let id = object.object_id();
            if ignore.contains(&id) {
                None
            } else {
                Some((id, object.version()))
            }
        })
        .collect();

    assert_eq!(actual, expected);
}

#[cfg(test)]
pub fn find_by_id(fx: &[(ObjectRef, Owner)], id: ObjectID) -> Option<ObjectRef> {
    fx.iter().find_map(|(o, _)| (o.0 == id).then_some(*o))
}

#[cfg(test)]
pub async fn init_state_with_objects_and_object_basics<I: IntoIterator<Item = Object>>(
    objects: I,
) -> (Arc<AuthorityState>, ObjectRef) {
    let state = TestAuthorityBuilder::new().build().await;
    for obj in objects {
        state.insert_genesis_object(obj).await;
    }
    publish_object_basics(state).await
}

#[cfg(test)]
pub async fn init_state_with_ids_and_object_basics<
    I: IntoIterator<Item = (SuiAddress, ObjectID)>,
>(
    objects: I,
) -> (Arc<AuthorityState>, ObjectRef) {
    let state = TestAuthorityBuilder::new().build().await;
    for (address, object_id) in objects {
        let obj = Object::with_id_owner_for_testing(object_id, address);
        state.insert_genesis_object(obj).await;
    }
    publish_object_basics(state).await
}

async fn publish_object_basics(state: Arc<AuthorityState>) -> (Arc<AuthorityState>, ObjectRef) {
    use sui_move_build::BuildConfig;

    // add object_basics package object to genesis, since lots of test use it
    let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    path.push("src/unit_tests/data/object_basics");
    let modules: Vec<_> = BuildConfig::new_for_testing()
        .build(path)
        .unwrap()
        .get_modules()
        .cloned()
        .collect();
    let digest = TransactionDigest::genesis();
    let pkg = Object::new_package_for_testing(
        &modules,
        digest,
        BuiltInFramework::genesis_move_packages(),
    )
    .unwrap();
    let pkg_ref = pkg.compute_object_reference();
    state.insert_genesis_object(pkg).await;
    (state, pkg_ref)
}

#[cfg(test)]
pub async fn init_state_with_ids_and_object_basics_with_fullnode<
    I: IntoIterator<Item = (SuiAddress, ObjectID)>,
>(
    objects: I,
) -> (Arc<AuthorityState>, Arc<AuthorityState>, ObjectRef) {
    use sui_move_build::BuildConfig;

    let (validator, fullnode) = init_state_validator_with_fullnode().await;
    for (address, object_id) in objects {
        let obj = Object::with_id_owner_for_testing(object_id, address);
        validator.insert_genesis_object(obj.clone()).await;
        fullnode.insert_genesis_object(obj).await;
    }

    // add object_basics package object to genesis, since lots of test use it
    let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    path.push("src/unit_tests/data/object_basics");
    let modules: Vec<_> = BuildConfig::new_for_testing()
        .build(path)
        .unwrap()
        .get_modules()
        .cloned()
        .collect();
    let digest = TransactionDigest::genesis();
    let pkg = Object::new_package_for_testing(
        &modules,
        digest,
        BuiltInFramework::genesis_move_packages(),
    )
    .unwrap();
    let pkg_ref = pkg.compute_object_reference();
    validator.insert_genesis_object(pkg.clone()).await;
    fullnode.insert_genesis_object(pkg).await;
    (validator, fullnode, pkg_ref)
}

pub async fn call_move(
    authority: &AuthorityState,
    gas_object_id: &ObjectID,
    sender: &SuiAddress,
    sender_key: &AccountKeyPair,
    package: &ObjectID,
    module: &'_ str,
    function: &'_ str,
    type_args: Vec<TypeTag>,
    test_args: Vec<TestCallArg>,
) -> SuiResult<TransactionEffects> {
    call_move_(
        authority,
        None,
        gas_object_id,
        sender,
        sender_key,
        package,
        module,
        function,
        type_args,
        test_args,
        false, // no shared objects
    )
    .await
}

pub async fn call_move_(
    authority: &AuthorityState,
    fullnode: Option<&AuthorityState>,
    gas_object_id: &ObjectID,
    sender: &SuiAddress,
    sender_key: &AccountKeyPair,
    package: &ObjectID,
    module: &'_ str,
    function: &'_ str,
    type_args: Vec<TypeTag>,
    test_args: Vec<TestCallArg>,
    with_shared: bool, // Move call includes shared objects
) -> SuiResult<TransactionEffects> {
    let gas_object = authority.get_object(gas_object_id).await.unwrap();
    let gas_object_ref = gas_object.unwrap().compute_object_reference();
    let mut builder = ProgrammableTransactionBuilder::new();
    let mut args = vec![];
    for arg in test_args.into_iter() {
        args.push(arg.to_call_arg(&mut builder, authority).await);
    }
    let rgp = authority.reference_gas_price_for_testing().unwrap();
    builder.command(Command::move_call(
        *package,
        Identifier::new(module).unwrap(),
        Identifier::new(function).unwrap(),
        type_args,
        args,
    ));
    let data = TransactionData::new_programmable(
        *sender,
        vec![gas_object_ref],
        builder.finish(),
        rgp * TEST_ONLY_GAS_UNIT_FOR_OBJECT_BASICS * 10,
        rgp,
    );

    let transaction = to_sender_signed_transaction(data, sender_key);
    let signed_effects =
        send_and_confirm_transaction_(authority, fullnode, transaction, with_shared)
            .await?
            .1;
    Ok(signed_effects.into_data())
}

pub async fn execute_programmable_transaction(
    authority: &AuthorityState,
    gas_object_id: &ObjectID,
    sender: &SuiAddress,
    sender_key: &AccountKeyPair,
    pt: ProgrammableTransaction,
    gas_unit: u64,
) -> SuiResult<TransactionEffects> {
    execute_programmable_transaction_(
        authority,
        None,
        gas_object_id,
        sender,
        sender_key,
        pt,
        /* with_shared */ false,
        gas_unit,
    )
    .await
}

pub async fn execute_programmable_transaction_with_shared(
    authority: &AuthorityState,
    gas_object_id: &ObjectID,
    sender: &SuiAddress,
    sender_key: &AccountKeyPair,
    pt: ProgrammableTransaction,
    gas_unit: u64,
) -> SuiResult<TransactionEffects> {
    execute_programmable_transaction_(
        authority,
        None,
        gas_object_id,
        sender,
        sender_key,
        pt,
        /* with_shared */ true,
        gas_unit,
    )
    .await
}

async fn execute_programmable_transaction_(
    authority: &AuthorityState,
    fullnode: Option<&AuthorityState>,
    gas_object_id: &ObjectID,
    sender: &SuiAddress,
    sender_key: &AccountKeyPair,
    pt: ProgrammableTransaction,
    with_shared: bool, // Move call includes shared objects
    gas_unit: u64,
) -> SuiResult<TransactionEffects> {
    let rgp = authority.reference_gas_price_for_testing().unwrap();
    let gas_object = authority.get_object(gas_object_id).await.unwrap();
    let gas_object_ref = gas_object.unwrap().compute_object_reference();
    let data =
        TransactionData::new_programmable(*sender, vec![gas_object_ref], pt, rgp * gas_unit, rgp);

    let transaction = to_sender_signed_transaction(data, sender_key);
    let signed_effects =
        send_and_confirm_transaction_(authority, fullnode, transaction, with_shared)
            .await?
            .1;
    Ok(signed_effects.into_data())
}

async fn call_move_with_gas_coins(
    authority: &AuthorityState,
    fullnode: Option<&AuthorityState>,
    gas_object_ids: &[ObjectID],
    gas_budget: u64,
    sender: &SuiAddress,
    sender_key: &AccountKeyPair,
    package: &ObjectID,
    module: &'_ str,
    function: &'_ str,
    type_args: Vec<TypeTag>,
    test_args: Vec<TestCallArg>,
    with_shared: bool, // Move call includes shared objects
) -> SuiResult<TransactionEffects> {
    let mut gas_object_refs = vec![];
    for obj_id in gas_object_ids {
        let gas_object = authority.get_object(obj_id).await.unwrap();
        let gas_ref = gas_object.unwrap().compute_object_reference();
        gas_object_refs.push(gas_ref);
    }
    let mut builder = ProgrammableTransactionBuilder::new();
    let mut args = vec![];
    for arg in test_args.into_iter() {
        args.push(arg.to_call_arg(&mut builder, authority).await);
    }
    let rgp = authority.reference_gas_price_for_testing().unwrap();
    builder.command(Command::move_call(
        *package,
        Identifier::new(module).unwrap(),
        Identifier::new(function).unwrap(),
        type_args,
        args,
    ));
    let data = TransactionData::new_programmable(
        *sender,
        gas_object_refs,
        builder.finish(),
        gas_budget,
        rgp,
    );

    let transaction = to_sender_signed_transaction(data, sender_key);
    let signed_effects =
        send_and_confirm_transaction_(authority, fullnode, transaction, with_shared)
            .await?
            .1;
    Ok(signed_effects.into_data())
}

pub async fn create_move_object(
    package_id: &ObjectID,
    authority: &AuthorityState,
    gas_object_id: &ObjectID,
    sender: &SuiAddress,
    sender_key: &AccountKeyPair,
) -> SuiResult<TransactionEffects> {
    call_move(
        authority,
        gas_object_id,
        sender,
        sender_key,
        package_id,
        "object_basics",
        "create",
        vec![],
        vec![
            TestCallArg::Pure(bcs::to_bytes(&(16_u64)).unwrap()),
            TestCallArg::Pure(bcs::to_bytes(sender).unwrap()),
        ],
    )
    .await
}

async fn create_move_object_with_gas_coins(
    package_id: &ObjectID,
    authority: &AuthorityState,
    gas_object_ids: &[ObjectID],
    gas_budget: u64,
    sender: &SuiAddress,
    sender_key: &AccountKeyPair,
) -> SuiResult<TransactionEffects> {
    call_move_with_gas_coins(
        authority,
        None,
        gas_object_ids,
        gas_budget,
        sender,
        sender_key,
        package_id,
        "object_basics",
        "create",
        vec![],
        vec![
            TestCallArg::Pure(bcs::to_bytes(&(16_u64)).unwrap()),
            TestCallArg::Pure(bcs::to_bytes(sender).unwrap()),
        ],
        false,
    )
    .await
}

pub async fn wrap_object(
    package_id: &ObjectID,
    authority: &AuthorityState,
    object_id: &ObjectID,
    gas_object_id: &ObjectID,
    sender: &SuiAddress,
    sender_key: &AccountKeyPair,
) -> SuiResult<TransactionEffects> {
    call_move(
        authority,
        gas_object_id,
        sender,
        sender_key,
        package_id,
        "object_basics",
        "wrap",
        vec![],
        vec![TestCallArg::Object(*object_id)],
    )
    .await
}

pub async fn add_ofield(
    package_id: &ObjectID,
    authority: &AuthorityState,
    outer_object_id: &ObjectID,
    inner_object_id: &ObjectID,
    gas_object_id: &ObjectID,
    sender: &SuiAddress,
    sender_key: &AccountKeyPair,
) -> SuiResult<TransactionEffects> {
    call_move(
        authority,
        gas_object_id,
        sender,
        sender_key,
        package_id,
        "object_basics",
        "add_ofield",
        vec![],
        vec![
            TestCallArg::Object(*outer_object_id),
            TestCallArg::Object(*inner_object_id),
        ],
    )
    .await
}

pub async fn call_dev_inspect(
    authority: &AuthorityState,
    sender: &SuiAddress,
    package: &ObjectID,
    module: &str,
    function: &str,
    type_arguments: Vec<TypeTag>,
    test_args: Vec<TestCallArg>,
) -> Result<DevInspectResults, anyhow::Error> {
    let mut builder = ProgrammableTransactionBuilder::new();
    let mut arguments = Vec::with_capacity(test_args.len());
    for a in test_args {
        arguments.push(a.to_call_arg(&mut builder, authority).await)
    }

    builder.command(Command::move_call(
        *package,
        Identifier::new(module).unwrap(),
        Identifier::new(function).unwrap(),
        type_arguments,
        arguments,
    ));
    let kind = TransactionKind::programmable(builder.finish());
    authority
        .dev_inspect_transaction_block(*sender, kind, Some(1))
        .await
}

/// This function creates a transaction that calls a 0x02::object_basics::set_value function.
/// Usually we need to publish this package first, but in this test files we often don't do that.
/// Then the tx would fail with `VMVerificationOrDeserializationError` (Linker error, module not found),
/// but gas is still charged. Depending on what we want to test, this may be fine.
#[cfg(test)]
async fn make_test_transaction(
    sender: &SuiAddress,
    sender_key: &AccountKeyPair,
    shared_object_id: ObjectID,
    shared_object_initial_shared_version: SequenceNumber,
    gas_object_ref: &ObjectRef,
    authorities: &[&AuthorityState],
    arg_value: u64,
) -> VerifiedCertificate {
    // Make a sample transaction.
    let module = "object_basics";
    let function = "set_value";

    let rgp = authorities
        .get(0)
        .unwrap()
        .reference_gas_price_for_testing()
        .unwrap();
    let data = TransactionData::new_move_call(
        *sender,
        SUI_FRAMEWORK_OBJECT_ID,
        ident_str!(module).to_owned(),
        ident_str!(function).to_owned(),
        /* type_args */ vec![],
        *gas_object_ref,
        /* args */
        vec![
            CallArg::Object(ObjectArg::SharedObject {
                id: shared_object_id,
                initial_shared_version: shared_object_initial_shared_version,
                mutable: true,
            }),
            CallArg::Pure(arg_value.to_le_bytes().to_vec()),
        ],
        TEST_ONLY_GAS_UNIT_FOR_OBJECT_BASICS * rgp,
        rgp,
    )
    .unwrap();

    let transaction = to_sender_signed_transaction(data, sender_key);

    let committee = authorities[0].clone_committee_for_testing();
    let mut sigs = vec![];

    for authority in authorities {
        let epoch_store = authority.load_epoch_store_one_call_per_task();
        let response = authority
            .handle_transaction(&epoch_store, transaction.clone())
            .await
            .unwrap();
        let vote = response.status.into_signed_for_testing();
        sigs.push(vote.clone());
        if let Ok(cert) =
            CertifiedTransaction::new(transaction.clone().into_message(), sigs.clone(), &committee)
        {
            return cert.verify(&committee).unwrap();
        }
    }

    unreachable!("couldn't form cert")
}

async fn prepare_authority_and_shared_object_cert(
) -> (Arc<AuthorityState>, VerifiedCertificate, ObjectID) {
    let (sender, keypair): (_, AccountKeyPair) = get_key_pair();

    // Initialize an authority with a (owned) gas object and a shared object.
    let gas_object_id = ObjectID::random();
    let gas_object = Object::with_id_owner_for_testing(gas_object_id, sender);
    let gas_object_ref = gas_object.compute_object_reference();

    let shared_object_id = ObjectID::random();
    let shared_object = {
        let obj = MoveObject::new_gas_coin(OBJECT_START_VERSION, shared_object_id, 10);
        let owner = Owner::Shared {
            initial_shared_version: obj.version(),
        };
        Object::new_move(obj, owner, TransactionDigest::genesis())
    };
    let initial_shared_version = shared_object.version();

    let authority = init_state_with_objects(vec![gas_object, shared_object]).await;

    let certificate = make_test_transaction(
        &sender,
        &keypair,
        shared_object_id,
        initial_shared_version,
        &gas_object_ref,
        &[&authority],
        16,
    )
    .await;
    (authority, certificate, shared_object_id)
}

#[tokio::test(flavor = "current_thread", start_paused = true)]
#[should_panic]
async fn test_shared_object_transaction_shared_locks_not_set() {
    let (authority, certificate, _) = prepare_authority_and_shared_object_cert().await;

    // Executing the certificate now panics since it was not sequenced and shared locks are not set
    let _ = authority.try_execute_for_test(&certificate).await;
}

#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn test_shared_object_transaction_ok() {
    let (authority, certificate, shared_object_id) =
        prepare_authority_and_shared_object_cert().await;
    let transaction_digest = certificate.digest();

    // Sequence the certificate to assign a sequence number to the shared object.
    send_consensus(&authority, &certificate).await;

    // Verify shared locks are now set for the transaction.
    let shared_object_version = authority
        .epoch_store_for_testing()
        .get_shared_locks(transaction_digest)
        .expect("Reading shared locks should not fail")
        .into_iter()
        .find_map(|(object_id, version)| {
            if object_id == shared_object_id {
                Some(version)
            } else {
                None
            }
        })
        .expect("Shared object must be locked");
    assert_eq!(shared_object_version, OBJECT_START_VERSION);

    // Finally (Re-)execute the contract should succeed.
    authority.try_execute_for_test(&certificate).await.unwrap();

    // Ensure transaction effects are available.
    authority.notify_read_effects(&certificate).await.unwrap();

    // Ensure shared object sequence number increased.
    let shared_object_version = authority
        .get_object(&shared_object_id)
        .await
        .unwrap()
        .unwrap()
        .version();
    assert_eq!(shared_object_version, SequenceNumber::from(2));
}

#[tokio::test]
async fn test_consensus_message_processed() {
    telemetry_subscribers::init_for_testing();

    let (sender, keypair): (_, AccountKeyPair) = get_key_pair();

    let gas_object_id = ObjectID::random();
    let gas_object = Object::with_id_owner_for_testing(gas_object_id, sender);
    let mut gas_object_ref = gas_object.compute_object_reference();

    let shared_object_id = ObjectID::random();
    let shared_object = {
        use sui_types::object::MoveObject;
        let obj = MoveObject::new_gas_coin(OBJECT_START_VERSION, shared_object_id, 10);
        let owner = Owner::Shared {
            initial_shared_version: obj.version(),
        };
        Object::new_move(obj, owner, TransactionDigest::genesis())
    };
    let initial_shared_version = shared_object.version();

    let dir = tempfile::TempDir::new().unwrap();
    let network_config = sui_config::builder::ConfigBuilder::new(&dir)
        .committee_size(2.try_into().unwrap())
        .with_objects(vec![gas_object.clone(), shared_object.clone()])
        .build();
    let genesis = network_config.genesis;

    let sec1 = network_config.validator_configs[0]
        .protocol_key_pair()
        .copy();
    let sec2 = network_config.validator_configs[1]
        .protocol_key_pair()
        .copy();

    let authority1 = init_state_with_objects_and_committee(
        vec![gas_object.clone(), shared_object.clone()],
        &genesis,
        &sec1,
    )
    .await;
    let authority2 = init_state_with_objects_and_committee(
        vec![gas_object.clone(), shared_object.clone()],
        &genesis,
        &sec2,
    )
    .await;

    let seed = [1u8; 32];
    let mut rng = StdRng::from_seed(seed);
    for _ in 0..50 {
        let certificate = make_test_transaction(
            &sender,
            &keypair,
            shared_object_id,
            initial_shared_version,
            &gas_object_ref,
            &[&authority1, &authority2],
            Uniform::from(0..100000).sample(&mut rng),
        )
        .await;
        let transaction_digest = certificate.digest();

        // on authority1, we always sequence via consensus
        send_consensus(&authority1, &certificate).await;
        let (effects1, _execution_error_opt) =
            authority1.try_execute_for_test(&certificate).await.unwrap();

        // now, on authority2, we send 0 or 1 consensus messages, then we either sequence and execute via
        // effects or via handle_certificate, then send 0 or 1 consensus messages.
        let send_first = rng.gen_bool(0.5);
        if send_first {
            send_consensus(&authority2, &certificate).await;
        }

        let effects2 = if send_first && rng.gen_bool(0.5) {
            authority2
                .try_execute_for_test(&certificate)
                .await
                .unwrap()
                .0
                .into_message()
        } else {
            let epoch_store = authority2.epoch_store_for_testing();
            epoch_store
                .acquire_shared_locks_from_effects(
                    &VerifiedExecutableTransaction::new_from_certificate(certificate.clone()),
                    &effects1,
                    authority2.db(),
                )
                .await
                .unwrap();
            authority2.try_execute_for_test(&certificate).await.unwrap();
            authority2
                .database
                .get_executed_effects(transaction_digest)
                .unwrap()
                .unwrap()
        };

        assert_eq!(effects1.data(), &effects2);

        // If we didn't send consensus before handle_node_sync_certificate, we need to do it now.
        if !send_first {
            send_consensus(&authority2, &certificate).await;
        }

        // Sometimes send one more consensus message.
        if rng.gen_bool(0.5) {
            send_consensus(&authority2, &certificate).await;
        }

        // Update to the new gas object for new tx
        gas_object_ref = *effects1
            .data()
            .mutated()
            .iter()
            .map(|(objref, _)| objref)
            .find(|objref| objref.0 == gas_object_ref.0)
            .unwrap();
    }

    // verify the two validators are in sync.
    assert_eq!(
        authority1
            .epoch_store_for_testing()
            .get_next_object_version(&shared_object_id),
        authority2
            .epoch_store_for_testing()
            .get_next_object_version(&shared_object_id),
    );
}

#[test]
fn test_choose_next_system_packages() {
    telemetry_subscribers::init_for_testing();
    let o1 = random_object_ref();
    let o2 = random_object_ref();
    let o3 = random_object_ref();

    fn sort(mut v: Vec<ObjectRef>) -> Vec<ObjectRef> {
        v.sort();
        v
    }

    fn ver(v: u64) -> ProtocolVersion {
        ProtocolVersion::new(v)
    }

    macro_rules! make_capabilities {
        ($v: expr, $name: expr, $packages: expr) => {
            AuthorityCapabilities::new(
                $name,
                SupportedProtocolVersions::new_for_testing(1, $v),
                $packages,
            )
        };
    }

    let committee = Committee::new_simple_test_committee().0;
    let v = &committee.voting_rights;
    let mut protocol_config = ProtocolConfig::get_for_max_version();
    protocol_config.set_advance_to_highest_supported_protocol_version_for_testing(false);
    protocol_config.set_buffer_stake_for_protocol_upgrade_bps_for_testing(7500);

    // all validators agree on new system packages, but without a new protocol version, so no
    // upgrade.
    let capabilities = vec![
        make_capabilities!(1, v[0].0, vec![o1, o2]),
        make_capabilities!(1, v[1].0, vec![o1, o2]),
        make_capabilities!(1, v[2].0, vec![o1, o2]),
        make_capabilities!(1, v[3].0, vec![o1, o2]),
    ];

    assert_eq!(
        (ver(1), vec![]),
        AuthorityState::choose_protocol_version_and_system_packages(
            ProtocolVersion::MIN,
            &protocol_config,
            &committee,
            capabilities,
            protocol_config.buffer_stake_for_protocol_upgrade_bps(),
        )
    );

    // one validator disagrees, stake buffer means no upgrade
    let capabilities = vec![
        make_capabilities!(2, v[0].0, vec![o1, o2]),
        make_capabilities!(2, v[1].0, vec![o1, o2]),
        make_capabilities!(2, v[2].0, vec![o1, o2]),
        make_capabilities!(2, v[3].0, vec![o1, o3]),
    ];

    assert_eq!(
        (ver(1), vec![]),
        AuthorityState::choose_protocol_version_and_system_packages(
            ProtocolVersion::MIN,
            &protocol_config,
            &committee,
            capabilities.clone(),
            protocol_config.buffer_stake_for_protocol_upgrade_bps(),
        )
    );

    // Now 2f+1 is enough to upgrade
    protocol_config.set_buffer_stake_for_protocol_upgrade_bps_for_testing(0);

    assert_eq!(
        (ver(2), sort(vec![o1, o2])),
        AuthorityState::choose_protocol_version_and_system_packages(
            ProtocolVersion::MIN,
            &protocol_config,
            &committee,
            capabilities,
            protocol_config.buffer_stake_for_protocol_upgrade_bps(),
        )
    );

    // committee is split, can't upgrade even with 0 stake buffer
    let capabilities = vec![
        make_capabilities!(2, v[0].0, vec![o1, o2]),
        make_capabilities!(2, v[1].0, vec![o1, o2]),
        make_capabilities!(2, v[2].0, vec![o1, o3]),
        make_capabilities!(2, v[3].0, vec![o1, o3]),
    ];

    assert_eq!(
        (ver(1), vec![]),
        AuthorityState::choose_protocol_version_and_system_packages(
            ProtocolVersion::MIN,
            &protocol_config,
            &committee,
            capabilities,
            protocol_config.buffer_stake_for_protocol_upgrade_bps(),
        )
    );

    // all validators agree on packages, and a proto upgrade
    let capabilities = vec![
        make_capabilities!(2, v[0].0, vec![o1, o2]),
        make_capabilities!(2, v[1].0, vec![o1, o2]),
        make_capabilities!(2, v[2].0, vec![o1, o2]),
        make_capabilities!(2, v[3].0, vec![o1, o2]),
    ];

    assert_eq!(
        (ver(2), sort(vec![o1, o2])),
        AuthorityState::choose_protocol_version_and_system_packages(
            ProtocolVersion::MIN,
            &protocol_config,
            &committee,
            capabilities,
            protocol_config.buffer_stake_for_protocol_upgrade_bps(),
        )
    );

    // all validators agree on packages, but not protocol version.
    let capabilities = vec![
        make_capabilities!(1, v[0].0, vec![o1, o2]),
        make_capabilities!(1, v[1].0, vec![o1, o2]),
        make_capabilities!(2, v[2].0, vec![o1, o2]),
        make_capabilities!(2, v[3].0, vec![o1, o2]),
    ];

    assert_eq!(
        (ver(1), vec![]),
        AuthorityState::choose_protocol_version_and_system_packages(
            ProtocolVersion::MIN,
            &protocol_config,
            &committee,
            capabilities,
            protocol_config.buffer_stake_for_protocol_upgrade_bps(),
        )
    );

    // all validators support 3, but with this protocol config we cannot advance multiple
    // versions at once.
    let capabilities = vec![
        make_capabilities!(3, v[0].0, vec![o1, o2]),
        make_capabilities!(3, v[1].0, vec![o1, o2]),
        make_capabilities!(3, v[2].0, vec![o1, o2]),
        make_capabilities!(3, v[3].0, vec![o1, o2]),
    ];

    assert_eq!(
        (ver(2), sort(vec![o1, o2])),
        AuthorityState::choose_protocol_version_and_system_packages(
            ProtocolVersion::MIN,
            &protocol_config,
            &committee,
            capabilities,
            protocol_config.buffer_stake_for_protocol_upgrade_bps(),
        )
    );

    // one validator is having a problem with packages, so its vote does not count.
    let capabilities = vec![
        make_capabilities!(2, v[0].0, vec![]),
        make_capabilities!(1, v[1].0, vec![o1, o2]),
        make_capabilities!(2, v[2].0, vec![o1, o2]),
        make_capabilities!(2, v[3].0, vec![o1, o2]),
    ];

    assert_eq!(
        (ver(1), vec![]),
        AuthorityState::choose_protocol_version_and_system_packages(
            ProtocolVersion::MIN,
            &protocol_config,
            &committee,
            capabilities,
            protocol_config.buffer_stake_for_protocol_upgrade_bps(),
        )
    );

    protocol_config.set_advance_to_highest_supported_protocol_version_for_testing(true);

    // skip straight to version 3
    let capabilities = vec![
        make_capabilities!(3, v[0].0, vec![o1, o2]),
        make_capabilities!(3, v[1].0, vec![o1, o2]),
        make_capabilities!(3, v[2].0, vec![o1, o2]),
        make_capabilities!(3, v[3].0, vec![o1, o3]),
    ];

    assert_eq!(
        (ver(3), sort(vec![o1, o2])),
        AuthorityState::choose_protocol_version_and_system_packages(
            ProtocolVersion::MIN,
            &protocol_config,
            &committee,
            capabilities,
            protocol_config.buffer_stake_for_protocol_upgrade_bps(),
        )
    );

    let capabilities = vec![
        make_capabilities!(3, v[0].0, vec![o1, o2]),
        make_capabilities!(3, v[1].0, vec![o1, o2]),
        make_capabilities!(4, v[2].0, vec![o1, o2]),
        make_capabilities!(5, v[3].0, vec![o1, o2]),
    ];

    // packages are identical between all currently supported versions, so we can upgrade to
    // 3 which is the highest supported version
    assert_eq!(
        (ver(3), sort(vec![o1, o2])),
        AuthorityState::choose_protocol_version_and_system_packages(
            ProtocolVersion::MIN,
            &protocol_config,
            &committee,
            capabilities,
            protocol_config.buffer_stake_for_protocol_upgrade_bps(),
        )
    );

    let capabilities = vec![
        make_capabilities!(2, v[0].0, vec![]),
        make_capabilities!(2, v[1].0, vec![]),
        make_capabilities!(3, v[2].0, vec![o1, o2]),
        make_capabilities!(3, v[3].0, vec![o1, o3]),
    ];

    // Even though 2f+1 validators agree on version 2, we don't have an agreement about the
    // packages. In this situation it is likely that (v2, []) is a valid upgrade, but we don't have
    // a way to detect that. The upgrade simply won't happen until everyone moves to 3.
    assert_eq!(
        (ver(1), sort(vec![])),
        AuthorityState::choose_protocol_version_and_system_packages(
            ProtocolVersion::MIN,
            &protocol_config,
            &committee,
            capabilities,
            protocol_config.buffer_stake_for_protocol_upgrade_bps(),
        )
    );
}

#[tokio::test]
async fn test_gas_smashing() {
    // run a create move object transaction with a given set o gas coins and a budget
    async fn create_obj(
        sender: SuiAddress,
        sender_key: AccountKeyPair,
        gas_coins: Vec<Object>,
        gas_budget: u64,
    ) -> (Arc<AuthorityState>, TransactionEffects) {
        let object_ids: Vec<_> = gas_coins.iter().map(|obj| obj.id()).collect();
        let (authority_state, pkg_ref) = init_state_with_objects_and_object_basics(gas_coins).await;
        let effects = create_move_object_with_gas_coins(
            &pkg_ref.0,
            &authority_state,
            &object_ids,
            gas_budget,
            &sender,
            &sender_key,
        )
        .await
        .unwrap();
        (authority_state, effects)
    }

    // make a `coin_num` coins distributing `gas_amount` across them
    fn make_gas_coins(owner: SuiAddress, gas_amount: u64, coin_num: u64) -> Vec<Object> {
        let mut objects = vec![];
        let coin_balance = gas_amount / coin_num;
        for _ in 1..coin_num {
            let gas_object_id = ObjectID::random();
            objects.push(Object::with_id_owner_gas_for_testing(
                gas_object_id,
                owner,
                coin_balance,
            ));
        }
        // in case integer division dropped something, make a coin with whatever is left
        let amount_left = gas_amount - (coin_balance * (coin_num - 1));
        let gas_object_id = ObjectID::random();
        objects.push(Object::with_id_owner_gas_for_testing(
            gas_object_id,
            owner,
            amount_left,
        ));
        objects
    }

    // run an object creation transaction with the given amount of gas and coins
    async fn run_and_check(
        reference_gas_used: u64,
        coin_num: u64,
        budget: u64,
        success: bool,
    ) -> u64 {
        let (sender, sender_key): (_, AccountKeyPair) = get_key_pair();
        let gas_coins = make_gas_coins(sender, reference_gas_used, coin_num);
        let gas_coin_ids: Vec<_> = gas_coins.iter().map(|obj| obj.id()).collect();
        let (state, effects) = create_obj(sender, sender_key, gas_coins, budget).await;
        // check transaction
        if success {
            assert!(effects.status().is_ok());
        } else {
            assert!(effects.status().is_err());
        }
        // gas object in effects is first coin in vector of coins
        assert_eq!(gas_coin_ids[0], effects.gas_object().0 .0);
        // object is created on success and gas at position 0 mutated
        let created = usize::from(success);
        assert_eq!(
            (effects.created().len(), effects.mutated().len()),
            (created, 1)
        );
        // extra coin are deleted
        assert_eq!(effects.deleted().len() as u64, coin_num - 1);
        for gas_coin_id in &gas_coin_ids[1..] {
            assert!(effects
                .deleted()
                .iter()
                .any(|deleted| deleted.0 == *gas_coin_id));
        }
        // balance on first coin is correct
        let balance = sui_types::gas::get_gas_balance(
            &state.get_object(&gas_coin_ids[0]).await.unwrap().unwrap(),
        )
        .unwrap();
        let gas_used = effects.gas_cost_summary().gas_used();
        assert!(reference_gas_used > balance);
        assert_eq!(reference_gas_used, balance + gas_used);
        gas_used
    }

    // get the cost of the transaction so we can play with multiple gas coins
    // 100,000 should be enough money for that transaction.
    let gas_used = run_and_check(100_000_000, 1, 100_000_000, true).await;

    // add something to the gas used to account for multiple gas coins being charged for
    let reference_gas_used = gas_used + 1_000;
    let three_coin_gas = run_and_check(reference_gas_used, 3, reference_gas_used, true).await;
    run_and_check(reference_gas_used, 10, reference_gas_used - 100, true).await;

    // make less then required to succeed
    let reference_gas_used = gas_used - 1;
    run_and_check(reference_gas_used, 2, reference_gas_used - 10, false).await;
    run_and_check(reference_gas_used, 30, reference_gas_used, false).await;
    // use a small amount less than what 3 coins above reported (with success)
    run_and_check(three_coin_gas, 3, three_coin_gas - 1, false).await;
}

#[tokio::test]
async fn test_for_inc_201_dev_inspect() {
    use sui_move_build::BuildConfig;

    let (sender, _sender_key): (_, AccountKeyPair) = get_key_pair();
    let gas_object_id = ObjectID::random();
    let (_, fullnode, _) =
        init_state_with_ids_and_object_basics_with_fullnode(vec![(sender, gas_object_id)]).await;

    // Module bytes
    let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    path.push("src/unit_tests/data/publish_with_event");
    let modules = BuildConfig::new_for_testing()
        .build(path)
        .unwrap()
        .get_package_bytes(false);

    let mut builder = ProgrammableTransactionBuilder::new();
    builder.command(Command::Publish(
        modules,
        BuiltInFramework::all_package_ids(),
    ));
    let kind = TransactionKind::programmable(builder.finish());
    let DevInspectResults { events, .. } = fullnode
        .dev_inspect_transaction_block(sender, kind, Some(1))
        .await
        .unwrap();

    assert_eq!(1, events.data.len());
    assert_eq!(
        "PublishEvent".to_string(),
        events.data[0].type_.name.to_string()
    );
    assert_eq!(json!({"foo":"bar"}), events.data[0].parsed_json);
}

#[tokio::test]
async fn test_for_inc_201_dry_run() {
    use sui_move_build::BuildConfig;

    let (sender, sender_key): (_, AccountKeyPair) = get_key_pair();
    let gas_object_id = ObjectID::random();
    let (_, fullnode, _) =
        init_state_with_ids_and_object_basics_with_fullnode(vec![(sender, gas_object_id)]).await;

    // Module bytes
    let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    path.push("src/unit_tests/data/publish_with_event");
    let modules = BuildConfig::new_for_testing()
        .build(path)
        .unwrap()
        .get_package_bytes(false);

    let mut builder = ProgrammableTransactionBuilder::new();
    builder.publish_immutable(modules, BuiltInFramework::all_package_ids());
    let kind = TransactionKind::programmable(builder.finish());

    let txn_data = TransactionData::new_with_gas_coins(kind, sender, vec![], 50_000_000, 1);

    let signed = to_sender_signed_transaction(txn_data, &sender_key);
    let (DryRunTransactionBlockResponse { events, .. }, _, _, _) = fullnode
        .dry_exec_transaction(
            signed.data().intent_message().value.clone(),
            *signed.digest(),
        )
        .await
        .unwrap();

    assert_eq!(1, events.data.len());
    assert_eq!(
        "PublishEvent".to_string(),
        events.data[0].type_.name.to_string()
    );
    assert_eq!(json!({"foo":"bar"}), events.data[0].parsed_json);
}

#[tokio::test]
async fn test_publish_transitive_dependencies_ok() {
    use sui_move_build::BuildConfig;

    let (sender, key): (_, AccountKeyPair) = get_key_pair();
    let gas_id = ObjectID::random();
    let state = init_state_with_ids(vec![(sender, gas_id)]).await;
    let rgp = state.reference_gas_price_for_testing().unwrap();

    // Get gas object
    let gas_object = state.get_object(&gas_id).await.unwrap().unwrap();
    let gas_ref = gas_object.compute_object_reference();

    // Publish `package C`
    let mut package_c_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    package_c_path.extend(["src", "unit_tests", "data", "transitive_dependencies", "c"]);

    // Set `c` to 0x0 address so that compiler doesn't complain about
    // this being a non-zero address when publishing. We can't set the address
    // in the manifest either, because then we'll get a "Conflicting addresses"
    // if we try to set `c`'s address via `additional_named_addresses`.
    let mut build_config = BuildConfig::new_for_testing();
    build_config
        .config
        .additional_named_addresses
        .insert("c".to_string(), AccountAddress::ZERO);

    let modules = build_config
        .build(package_c_path)
        .unwrap()
        .get_package_bytes(/* with_unpublished_deps */ false);

    let mut builder = ProgrammableTransactionBuilder::new();
    builder.publish_immutable(modules, vec![]);
    let kind = TransactionKind::programmable(builder.finish());
    let txn_data = TransactionData::new_with_gas_coins(
        kind,
        sender,
        vec![gas_ref],
        rgp * TEST_ONLY_GAS_UNIT_FOR_PUBLISH,
        1,
    );
    let signed = to_sender_signed_transaction(txn_data, &key);
    let txn_effects = send_and_confirm_transaction(&state, signed)
        .await
        .unwrap()
        .1
        .into_data();
    let ((package_c_id, _, _), _) = txn_effects.created().first().unwrap();
    let gas_ref = txn_effects.gas_object().0;

    // Publish `package B`
    let mut package_b_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    package_b_path.extend(["src", "unit_tests", "data", "transitive_dependencies", "b"]);

    let mut build_config = BuildConfig::new_for_testing();
    build_config.config.additional_named_addresses.extend([
        ("b".to_string(), AccountAddress::ZERO),
        ("c".to_string(), (*package_c_id).into()),
    ]);

    let modules = build_config
        .build(package_b_path)
        .unwrap()
        .get_package_bytes(/* with_unpublished_deps */ false);

    let mut builder = ProgrammableTransactionBuilder::new();

    builder.publish_immutable(modules, vec![*package_c_id]); // Note: B depends on C

    let kind = TransactionKind::programmable(builder.finish());
    let txn_data = TransactionData::new_with_gas_coins(
        kind,
        sender,
        vec![gas_ref],
        rgp * TEST_ONLY_GAS_UNIT_FOR_PUBLISH,
        1,
    );
    let signed = to_sender_signed_transaction(txn_data, &key);
    let txn_effects = send_and_confirm_transaction(&state, signed)
        .await
        .unwrap()
        .1
        .into_data();
    let ((package_b_id, _, _), _) = txn_effects.created().first().unwrap();
    let gas_ref = txn_effects.gas_object().0;

    // Publish `package A`
    let mut package_a_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    package_a_path.extend(["src", "unit_tests", "data", "transitive_dependencies", "a"]);

    let mut build_config = BuildConfig::new_for_testing();
    build_config.config.additional_named_addresses.extend([
        ("a".to_string(), AccountAddress::ZERO),
        ("b".to_string(), (*package_b_id).into()),
        ("c".to_string(), (*package_c_id).into()),
    ]);

    let modules = build_config
        .build(package_a_path)
        .unwrap()
        .get_package_bytes(/* with_unpublished_deps */ false);

    let mut builder = ProgrammableTransactionBuilder::new();

    builder.publish_immutable(modules, vec![*package_b_id, *package_c_id]); // Note: A depends on B and C.

    let kind = TransactionKind::programmable(builder.finish());
    let txn_data = TransactionData::new_with_gas_coins(
        kind,
        sender,
        vec![gas_ref],
        rgp * TEST_ONLY_GAS_UNIT_FOR_PUBLISH,
        1,
    );
    let signed = to_sender_signed_transaction(txn_data, &key);
    let txn_effects = send_and_confirm_transaction(&state, signed)
        .await
        .unwrap()
        .1
        .into_data();
    let ((package_a_id, _, _), _) = txn_effects.created().first().unwrap();
    let gas_ref = txn_effects.gas_object().0;

    // Publish `package root`
    let mut package_root_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    package_root_path.extend([
        "src",
        "unit_tests",
        "data",
        "transitive_dependencies",
        "root",
    ]);

    let mut build_config = BuildConfig::new_for_testing();
    build_config.config.additional_named_addresses.extend([
        ("examples".to_string(), AccountAddress::ZERO),
        ("a".to_string(), (*package_a_id).into()),
        ("b".to_string(), (*package_b_id).into()),
        ("c".to_string(), (*package_c_id).into()),
    ]);

    let modules = build_config
        .build(package_root_path)
        .unwrap()
        .get_package_bytes(/* with_unpublished_deps */ false);

    let mut builder = ProgrammableTransactionBuilder::new();
    let mut deps = BuiltInFramework::all_package_ids();
    // Note: root depends on A, B, C.
    deps.extend([*package_a_id, *package_b_id, *package_c_id]);
    builder.publish_immutable(modules, deps);

    let kind = TransactionKind::programmable(builder.finish());
    let txn_data = TransactionData::new_with_gas_coins(
        kind,
        sender,
        vec![gas_ref],
        rgp * TEST_ONLY_GAS_UNIT_FOR_PUBLISH * 2,
        rgp,
    );
    let signed = to_sender_signed_transaction(txn_data, &key);

    let status = send_and_confirm_transaction(&state, signed)
        .await
        .unwrap()
        .1
        .into_data()
        .into_status();

    assert!(status.is_ok(), "Transaction failed: {:?}", status);
}

#[tokio::test]
async fn test_publish_missing_dependency() {
    use sui_move_build::BuildConfig;

    let (sender, key): (_, AccountKeyPair) = get_key_pair();
    let gas_id = ObjectID::random();
    let state = init_state_with_ids(vec![(sender, gas_id)]).await;

    // Get gas object
    let gas_object = state.get_object(&gas_id).await.unwrap().unwrap();
    let gas_ref = gas_object.compute_object_reference();

    // Module bytes
    let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    path.extend(["src", "unit_tests", "data", "object_basics"]);

    let modules = BuildConfig::new_for_testing()
        .build(path)
        .unwrap()
        .get_package_bytes(/* with_unpublished_deps */ false);

    let mut builder = ProgrammableTransactionBuilder::new();
    builder.publish_immutable(modules, vec![SUI_FRAMEWORK_OBJECT_ID]);
    let kind = TransactionKind::programmable(builder.finish());

    let txn_data = TransactionData::new_with_gas_coins(kind, sender, vec![gas_ref], 10000, 1);

    let signed = to_sender_signed_transaction(txn_data, &key);
    let (failure, _) = send_and_confirm_transaction(&state, signed)
        .await
        .unwrap()
        .1
        .into_data()
        .into_status()
        .unwrap_err();

    assert_eq!(
        ExecutionFailureStatus::PublishUpgradeMissingDependency,
        failure,
    );
}

#[tokio::test]
async fn test_publish_missing_transitive_dependency() {
    use sui_move_build::BuildConfig;

    let (sender, key): (_, AccountKeyPair) = get_key_pair();
    let gas_id = ObjectID::random();
    let state = init_state_with_ids(vec![(sender, gas_id)]).await;

    // Get gas object
    let gas_object = state.get_object(&gas_id).await.unwrap().unwrap();
    let gas_ref = gas_object.compute_object_reference();

    // Module bytes
    let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    path.extend(["src", "unit_tests", "data", "object_basics"]);

    let modules = BuildConfig::new_for_testing()
        .build(path)
        .unwrap()
        .get_package_bytes(/* with_unpublished_deps */ false);

    let mut builder = ProgrammableTransactionBuilder::new();
    builder.publish_immutable(modules, vec![MOVE_STDLIB_OBJECT_ID]);
    let kind = TransactionKind::programmable(builder.finish());

    let txn_data = TransactionData::new_with_gas_coins(kind, sender, vec![gas_ref], 10000, 1);

    let signed = to_sender_signed_transaction(txn_data, &key);
    let (failure, _) = send_and_confirm_transaction(&state, signed)
        .await
        .unwrap()
        .1
        .into_data()
        .into_status()
        .unwrap_err();

    assert_eq!(
        ExecutionFailureStatus::PublishUpgradeMissingDependency,
        failure,
    );
}

#[tokio::test]
async fn test_publish_not_a_package_dependency() {
    use sui_move_build::BuildConfig;

    let (sender, key): (_, AccountKeyPair) = get_key_pair();
    let gas_id = ObjectID::random();
    let state = init_state_with_ids(vec![(sender, gas_id)]).await;

    // Get gas object
    let gas_object = state.get_object(&gas_id).await.unwrap().unwrap();
    let gas_ref = gas_object.compute_object_reference();

    // Module bytes
    let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    path.extend(["src", "unit_tests", "data", "object_basics"]);

    let modules = BuildConfig::new_for_testing()
        .build(path)
        .unwrap()
        .get_package_bytes(/* with_unpublished_deps */ false);

    let mut builder = ProgrammableTransactionBuilder::new();
    let mut deps = BuiltInFramework::all_package_ids();
    // One of these things is not like the others
    deps.push(SUI_SYSTEM_STATE_OBJECT_ID);
    builder.publish_immutable(modules, deps);
    let kind = TransactionKind::programmable(builder.finish());

    let txn_data = TransactionData::new_with_gas_coins(kind, sender, vec![gas_ref], 10000, 1);

    let signed = to_sender_signed_transaction(txn_data, &key);
    let failure = send_and_confirm_transaction(&state, signed)
        .await
        .unwrap_err();

    assert_eq!(
        SuiError::UserInputError {
            error: UserInputError::MoveObjectAsPackage {
                object_id: SUI_SYSTEM_STATE_OBJECT_ID
            }
        },
        failure,
    )
}
