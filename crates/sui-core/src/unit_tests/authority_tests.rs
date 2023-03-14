// Copyright (c) 2021, Facebook, Inc. and its affiliates
// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use super::*;
use crate::authority::move_integration_tests::build_and_publish_test_package_with_upgrade_cap;
use crate::consensus_handler::SequencedConsensusTransaction;
use crate::{
    authority_client::{AuthorityAPI, NetworkAuthorityClient},
    authority_server::AuthorityServer,
    checkpoints::CheckpointServiceNoop,
    test_utils::init_state_parameters_from_rng,
};
use bcs;
use futures::{stream::FuturesUnordered, StreamExt};
use move_binary_format::access::ModuleAccess;
use move_binary_format::{
    file_format::{self, AddressIdentifierIndex, IdentifierIndex, ModuleHandle},
    CompiledModule,
};
use move_core_types::identifier::IdentStr;
use move_core_types::language_storage::StructTag;
use move_core_types::{
    account_address::AccountAddress, ident_str, identifier::Identifier, language_storage::TypeTag,
};
use rand::{
    distributions::{Distribution, Uniform},
    prelude::StdRng,
    Rng, SeedableRng,
};
use serde_json::json;
use std::collections::HashSet;
use std::fs;
use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};
use sui_json_rpc_types::{
    SuiArgument, SuiExecutionResult, SuiExecutionStatus, SuiGasCostSummary,
    SuiTransactionEffectsAPI, SuiTypeTag,
};
use sui_types::error::UserInputError;
use sui_types::gas_coin::GasCoin;
use sui_types::programmable_transaction_builder::ProgrammableTransactionBuilder;
use sui_types::utils::{
    make_committee_key, mock_certified_checkpoint, to_sender_signed_transaction,
    to_sender_signed_transaction_with_multi_signers,
};
use sui_types::{SUI_CLOCK_OBJECT_ID, SUI_CLOCK_OBJECT_SHARED_VERSION, SUI_FRAMEWORK_OBJECT_ID};

use crate::epoch::epoch_metrics::EpochMetrics;
use move_core_types::parser::parse_type_tag;
use std::{convert::TryInto, env};
use sui_macros::sim_test;
use sui_protocol_config::{ProtocolConfig, SupportedProtocolVersions};
use sui_types::dynamic_field::DynamicFieldType;
use sui_types::epoch_data::EpochData;
use sui_types::object::Data;
use sui_types::sui_system_state::epoch_start_sui_system_state::EpochStartSystemState;
use sui_types::sui_system_state::SuiSystemStateWrapper;
use sui_types::{
    base_types::dbg_addr,
    crypto::{get_key_pair, Signature},
    crypto::{AccountKeyPair, AuthorityKeyPair, KeypairTraits},
    messages::TransactionExpiration,
    messages::VerifiedTransaction,
    object::{Owner, GAS_VALUE_FOR_TESTING, OBJECT_START_VERSION},
    SUI_SYSTEM_STATE_OBJECT_ID,
};
use tracing::info;

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

const MAX_GAS: u64 = 10000;

// Only relevant in a ser/de context : the `CertifiedTransaction` for a transaction is not unique
fn compare_certified_transactions(o1: &CertifiedTransaction, o2: &CertifiedTransaction) {
    assert_eq!(o1.digest(), o2.digest());
    // in this ser/de context it's relevant to compare signatures
    assert_eq!(
        o1.auth_sig().signature.as_ref(),
        o2.auth_sig().signature.as_ref()
    );
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
    fullnode.insert_genesis_object(shared_object).await;
    let gas_object = validator.get_object(&gas_object_id).await.unwrap();
    let gas_object_ref = gas_object.unwrap().compute_object_reference();
    let data = TransactionData::new_move_call_with_dummy_gas_price(
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
        MAX_GAS,
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

pub fn create_genesis_module_packages() -> Vec<Object> {
    let sui_modules = sui_framework::get_sui_framework();
    let std_modules = sui_framework::get_move_stdlib();
    let (std_move_pkg, _) = sui_framework::make_std_sui_move_pkgs();
    vec![
        Object::new_package_for_testing(std_modules, TransactionDigest::genesis(), &[]).unwrap(),
        Object::new_package_for_testing(sui_modules, TransactionDigest::genesis(), [&std_move_pkg])
            .unwrap(),
    ]
}

#[tokio::test]
async fn test_dry_run_transaction() {
    let (validator, fullnode, transaction, gas_object_id, shared_object_id) =
        construct_shared_object_transaction_with_sequence_number(None).await;
    let initial_shared_object_version = validator
        .get_object(&shared_object_id)
        .await
        .unwrap()
        .unwrap()
        .version();

    let transaction_digest = *transaction.digest();

    let response = fullnode
        .dry_exec_transaction(
            transaction.data().intent_message.value.clone(),
            transaction_digest,
        )
        .await
        .unwrap();
    assert_eq!(*response.effects.status(), SuiExecutionStatus::Success);
    let gas_usage = response.effects.gas_used();

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

    let txn_data = &transaction.data().intent_message.value;
    let txn_data = TransactionData::new_with_gas_coins(
        txn_data.kind().clone(),
        txn_data.sender(),
        vec![],
        txn_data.gas_budget(),
        txn_data.gas_price(),
    );
    let response = fullnode
        .dry_exec_transaction(txn_data, transaction_digest)
        .await
        .unwrap();
    let gas_usage_no_gas = response.effects.gas_used();
    assert_eq!(*response.effects.status(), SuiExecutionStatus::Success);
    assert_eq!(gas_usage, gas_usage_no_gas);
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
    assert!(effects.gas_used().computation_cost > 0);
    let mut results = results.unwrap();
    assert_eq!(results.len(), 1);
    let exec_results = results.pop().unwrap();
    let SuiExecutionResult {
        mutable_reference_outputs,
        return_values,
    } = exec_results;
    assert!(mutable_reference_outputs.is_empty());
    assert!(return_values.is_empty());
    let dev_inspect_gas_summary = effects.gas_used().clone();

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
    let actual_gas_used: SuiGasCostSummary = effects.gas_cost_summary().clone().into();
    assert_eq!(actual_gas_used, dev_inspect_gas_summary);

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
    assert!(effects.gas_used().computation_cost > 0);

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
    assert!(effects.gas_used().computation_cost > 0);

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
    let DevInspectResults { results, .. } = fullnode
        .dev_inspect_transaction(sender, kind, Some(1))
        .await
        .unwrap();
    // produces an error
    let err = results.unwrap_err();
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
    assert!(effects.gas_used().computation_cost > 0);
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
        .dev_inspect_transaction(sender, kind, Some(1))
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
        .dev_inspect_transaction(sender, kind, Some(1))
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
            transaction.data().intent_message.value.clone(),
            transaction_digest,
        )
        .await;
    assert!(response.is_err());
}

#[tokio::test]
async fn test_handle_transfer_transaction_bad_signature() {
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
    let transfer_transaction = init_transfer_transaction(
        sender,
        &sender_key,
        recipient,
        object.compute_object_reference(),
        gas_object.compute_object_reference(),
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
    bad_signature_transfer_transaction
        .data_mut_for_testing()
        .tx_signatures =
        vec![
            Signature::new_secure(&transfer_transaction.data().intent_message, &unknown_key).into(),
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
    let sender = get_new_address::<AccountKeyPair>();
    let (unknown_address, unknown_key) = get_key_pair();
    let object_id: ObjectID = ObjectID::random();
    let gas_object_id = ObjectID::random();
    let recipient = dbg_addr(2);
    let authority_state =
        init_state_with_ids(vec![(sender, object_id), (sender, gas_object_id)]).await;
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
async fn test_upgrade_module_is_feature_gated() {
    let (sender, sender_key): (_, AccountKeyPair) = get_key_pair();
    let gas_object_id = ObjectID::random();
    let gas_object = Object::with_id_owner_gas_for_testing(gas_object_id, sender, 10000);
    let authority_state = init_state().await;
    authority_state.insert_genesis_object(gas_object).await;

    let pt = {
        let mut builder = ProgrammableTransactionBuilder::new();
        // Data doesn't matter here. We hit the feature flag before checking it.
        let arg = builder.pure(1).unwrap();
        builder.upgrade(arg, vec![], vec![vec![]]);
        builder.finish()
    };

    let TransactionEffects::V1(effects) = execute_programmable_transaction(
        &authority_state,
        &gas_object_id,
        &sender,
        &sender_key,
        pt,
    )
    .await
    .unwrap();
    let (failure_status, _) = effects.status.unwrap_err();
    assert_eq!(
        failure_status,
        ExecutionFailureStatus::FeatureNotYetSupported
    );
}

/* FIXME: This tests the submission of out of transaction certs, but modifies object sequence numbers manually
   and leaves the authority in an inconsistent state. We should re-code it in a proper way.

#[test]
fn test_handle_transfer_transaction_bad_sequence_number() {
    let (sender, sender_key): (_, AccountKeyPair) = get_key_pair();
    let object_id: ObjectID = random_object_id();
    let recipient = Address::Sui(dbg_addr(2));
    let authority_state = init_state_with_object(sender, object_id);
    let transfer_transaction = init_transfer_transaction(sender, &sender_key, recipient, object_id);

    let mut sequence_number_state = authority_state;
    let sequence_number_state_sender_account =
        sequence_number_state.objects.get_mut(&object_id).unwrap();
    sequence_number_state_sender_account.version() =
        sequence_number_state_sender_account
            .version()
            .increment()
            .unwrap();
    assert!(sequence_number_state
        .handle_transfer_transaction(transfer_transaction)
        .is_err());

        let object = sequence_number_state.objects.get(&object_id).unwrap();
        assert!(sequence_number_state.get_transaction_lock(object.id, object.version()).unwrap().is_none());
}
*/

#[tokio::test]
async fn test_handle_transfer_transaction_ok() {
    let (sender, sender_key): (_, AccountKeyPair) = get_key_pair();
    let recipient = dbg_addr(2);
    let object_id = ObjectID::random();
    let gas_object_id = ObjectID::random();
    let authority_state =
        init_state_with_ids(vec![(sender, object_id), (sender, gas_object_id)]).await;
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
        envelope.data().intent_message.value,
        transfer_transaction.data().intent_message.value
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
            price: DUMMY_GAS_PRICE,
            budget: 10000,
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
            price: DUMMY_GAS_PRICE,
            budget: 10000,
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
            price: DUMMY_GAS_PRICE,
            budget: 10000,
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
            price: DUMMY_GAS_PRICE,
            budget: 10000,
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
    let epoch_store = authority_state.load_epoch_store_one_call_per_task();
    let gas_object = authority_state
        .get_object(&object_id)
        .await
        .unwrap()
        .unwrap();
    let package_object_ref = authority_state.get_framework_object_ref().await.unwrap();
    // We are trying to transfer the genesis package object, which is immutable.
    let transfer_transaction = init_transfer_transaction(
        sender,
        &sender_key,
        recipient,
        package_object_ref,
        gas_object.compute_object_reference(),
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
    let data = TransactionData::new_transfer_sui_with_dummy_gas_price(
        recipient,
        sender,
        None,
        child_object.compute_object_reference(),
        10000,
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

pub async fn send_and_confirm_transaction(
    authority: &AuthorityState,
    transaction: VerifiedTransaction,
) -> Result<(CertifiedTransaction, SignedTransactionEffects), SuiError> {
    send_and_confirm_transaction_(
        authority,
        None, /* no fullnode_key_pair */
        transaction,
        false, /* no shared objects */
    )
    .await
}

pub async fn send_and_confirm_transaction_(
    authority: &AuthorityState,
    fullnode: Option<&AuthorityState>,
    transaction: VerifiedTransaction,
    with_shared: bool, // transaction includes shared objects
) -> Result<(CertifiedTransaction, SignedTransactionEffects), SuiError> {
    // Make the initial request
    let epoch_store = authority.load_epoch_store_one_call_per_task();
    let response = authority
        .handle_transaction(&epoch_store, transaction.clone())
        .await?;
    let vote = response.status.into_signed_for_testing();

    // Collect signatures from a quorum of authorities
    let committee = authority.clone_committee_for_testing();
    let certificate =
        CertifiedTransaction::new(transaction.into_message(), vec![vote.clone()], &committee)
            .unwrap()
            .verify(&committee)
            .unwrap();

    if with_shared {
        send_consensus(authority, &certificate).await;
    }

    // Submit the confirmation. *Now* execution actually happens, and it should fail when we try to look up our dummy module.
    // we unfortunately don't get a very descriptive error message, but we can at least see that something went wrong inside the VM
    let result = authority.try_execute_for_test(&certificate).await?;
    if let Some(fullnode) = fullnode {
        fullnode.try_execute_for_test(&certificate).await?;
    }
    Ok((certificate.into_inner(), result.into_inner()))
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
    let genesis_module_objects = create_genesis_module_packages();
    let genesis_module = match &genesis_module_objects[0].data {
        Data::Package(m) => {
            CompiledModule::deserialize(m.serialized_module_map().values().next().unwrap()).unwrap()
        }
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

    let data = TransactionData::new_module_with_dummy_gas_price(
        sender,
        gas_payment_object_ref,
        vec![dependent_module_bytes],
        vec![ObjectID::from(*genesis_module.address())],
        MAX_GAS,
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
    let gas_balance = MAX_GAS;
    let gas_payment_object =
        Object::with_id_owner_gas_for_testing(gas_payment_object_id, sender, gas_balance);
    let gas_payment_object_ref = gas_payment_object.compute_object_reference();
    let authority = init_state_with_objects(vec![gas_payment_object]).await;

    let module = file_format::empty_module();
    let mut module_bytes = Vec::new();
    module.serialize(&mut module_bytes).unwrap();
    let module_bytes = vec![module_bytes];
    let dependencies = vec![]; // no dependencies
    let data = TransactionData::new_module_with_dummy_gas_price(
        sender,
        gas_payment_object_ref,
        module_bytes,
        dependencies,
        MAX_GAS,
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
    let genesis_module_objects = create_genesis_module_packages();
    let genesis_module = match &genesis_module_objects[0].data {
        Data::Package(m) => {
            CompiledModule::deserialize(m.serialized_module_map().values().next().unwrap()).unwrap()
        }
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

    let data = TransactionData::new_module_with_dummy_gas_price(
        sender,
        gas_payment_object_ref,
        vec![dependent_module_bytes],
        vec![ObjectID::from(*genesis_module.address()), not_on_chain],
        MAX_GAS,
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
    let data = TransactionData::new_module_with_dummy_gas_price(
        sender,
        gas_payment_object_ref,
        package,
        vec![],
        MAX_GAS,
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
    );

    let tx2 = init_transfer_transaction(
        sender,
        &sender_key,
        recipient2,
        object.compute_object_reference(),
        gas_object.compute_object_reference(),
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
    let object = authority_state
        .get_object(&object_id)
        .await
        .unwrap()
        .unwrap();
    let data = TransactionData::new_transfer_sui_with_dummy_gas_price(
        recipient,
        sender,
        Some(GAS_VALUE_FOR_TESTING),
        object.compute_object_reference(),
        200,
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
async fn test_transaction_expiration() {
    let (sender, sender_key): (_, AccountKeyPair) = get_key_pair();
    let recipient = dbg_addr(2);
    let object_id = ObjectID::random();
    let authority_state = init_state_with_ids(vec![(sender, object_id)]).await;

    let mut committee = authority_state
        .epoch_store_for_testing()
        .committee()
        .to_owned();
    committee.epoch = 1;
    let system_state = EpochStartSystemState::new_for_testing_with_epoch(1);

    authority_state
        .reconfigure(
            &authority_state.epoch_store_for_testing(),
            SupportedProtocolVersions::SYSTEM_DEFAULT,
            committee,
            EpochStartConfiguration::new_v1(system_state, Default::default()),
        )
        .await
        .unwrap();

    let object = authority_state
        .get_object(&object_id)
        .await
        .unwrap()
        .unwrap();
    let mut data = TransactionData::new_transfer_sui_with_dummy_gas_price(
        recipient,
        sender,
        Some(1),
        object.compute_object_reference(),
        MAX_GAS,
    );

    // Expired transaction returns an error
    let epoch_store = authority_state.load_epoch_store_one_call_per_task();
    let mut expired_data = data.clone();

    *expired_data.expiration_mut() = TransactionExpiration::Epoch(0);
    let expired_transaction = to_sender_signed_transaction(expired_data, &sender_key);
    let result = authority_state
        .handle_transaction(&epoch_store, expired_transaction)
        .await;

    assert!(matches!(result.unwrap_err(), SuiError::TransactionExpired));

    // Non expired transaction signed without issue
    *data.expiration_mut() = TransactionExpiration::Epoch(10);
    let transaction = to_sender_signed_transaction(data, &sender_key);
    authority_state
        .handle_transaction(&epoch_store, transaction)
        .await
        .unwrap();
}

#[tokio::test]
async fn test_missing_package() {
    let (sender, sender_key): (_, AccountKeyPair) = get_key_pair();
    let gas_object_id = ObjectID::random();
    let (authority_state, _object_basics) =
        init_state_with_ids_and_object_basics(vec![(sender, gas_object_id)]).await;
    let epoch_store = authority_state.load_epoch_store_one_call_per_task();
    let gas_object = authority_state
        .get_object(&gas_object_id)
        .await
        .unwrap()
        .unwrap();
    let non_existent_package = ObjectID::MAX;
    let gas_object_ref = gas_object.compute_object_reference();
    let data = TransactionData::new_move_call_with_dummy_gas_price(
        sender,
        non_existent_package,
        ident_str!("object_basics").to_owned(),
        ident_str!("wrap").to_owned(),
        vec![],
        gas_object_ref,
        vec![],
        MAX_GAS,
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
    let data = TransactionData::new_move_call_with_dummy_gas_price(
        s1,
        object_basics,
        ident_str!("object_basics").to_owned(),
        ident_str!("generic_test").to_owned(),
        vec![TypeTag::U64],
        gas1,
        vec![],
        MAX_GAS,
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
    let data = TransactionData::new_move_call_with_dummy_gas_price(
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
        MAX_GAS,
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
    let data = TransactionData::new_move_call_with_dummy_gas_price(
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
        MAX_GAS,
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
    let account = authority_state
        .get_object(&object_id)
        .await
        .unwrap()
        .unwrap();

    assert!(authority_state
        .parent(&(object_id, account.version(), account.digest()))
        .await
        .is_some());
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
    let opt_cert = {
        let refx = authority_state
            .parent(&(object_id, new_account.version(), new_account.digest()))
            .await
            .unwrap();
        authority_state
            .get_certified_transaction(&refx, &authority_state.epoch_store_for_testing())
            .unwrap()
    };
    if let Some(certified_transaction) = opt_cert {
        // valid since our test authority should not update its certificate set
        compare_certified_transactions(&certified_transaction, &certified_transfer_transaction);
    } else {
        panic!("parent certificate not avaailable from the authority!");
    }

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

    // Check that all the parents are returned.
    assert_eq!(
        authority_state
            .get_parent_iterator(object_id, None)
            .await
            .unwrap()
            .count(),
        2
    );
}

struct LimitedPoll<F: Future> {
    inner: Pin<Box<F>>,
    count: u64,
    limit: u64,
}

impl<F: Future> LimitedPoll<F> {
    fn new(limit: u64, inner: F) -> Self {
        Self {
            inner: Box::pin(inner),
            count: 0,
            limit,
        }
    }
}

impl<F: Future> Future for LimitedPoll<F> {
    type Output = Option<F::Output>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        if self.count >= self.limit {
            return Poll::Ready(None);
        }
        self.count += 1;
        match self.inner.as_mut().poll(cx) {
            Poll::Pending => Poll::Pending,
            Poll::Ready(val) => Poll::Ready(Some(val)),
        }
    }
}

#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn test_handle_certificate_with_shared_object_interrupted_retry() {
    telemetry_subscribers::init_for_testing();

    let (sender, sender_key): (_, AccountKeyPair) = get_key_pair();
    let gas_object_id = ObjectID::random();

    // We repeatedly timeout certs after a variety of delays, using LimitedPoll to ensure that we
    // interrupt the future at every point at which it is possible to be interrupted.
    // When .await points are added, this test will automatically exercise them.
    // The loop below terminates after all await points have been checked.
    let delays: Vec<_> = (1..100).collect();

    let mut objects: Vec<_> = delays
        .iter()
        .map(|_| (sender, ObjectID::random()))
        .collect();
    objects.push((sender, gas_object_id));

    let authority_state = Arc::new(init_state_with_ids(objects.clone()).await);

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

    authority_state.insert_genesis_object(shared_object).await;

    let mut interrupted_count = 0;
    for limit in &delays {
        info!("Testing with poll limit {}", limit);
        let gas_object = authority_state
            .get_object(&gas_object_id)
            .await
            .unwrap()
            .unwrap();

        // The tested certificate must contain shared objects, background:
        // https://github.com/MystenLabs/sui/pull/4579
        let shared_object_cert = make_test_transaction(
            &sender,
            &sender_key,
            shared_object_id,
            initial_shared_version,
            &gas_object.compute_object_reference(),
            &[&authority_state],
            16,
        )
        .await;

        // Send the shared_object_cert to consensus without execution, because it is necessary
        // to prepare the state for the explicit interrupted execution later.
        send_consensus_no_execution(&authority_state, &shared_object_cert).await;

        let clone1 = shared_object_cert.clone();
        let state1 = authority_state.clone();

        let res = Box::pin(LimitedPoll::new(*limit, async move {
            state1.try_execute_for_test(&clone1).await.unwrap();
        }))
        .await;
        if res.is_some() {
            info!(?limit, "limit was high enough that future completed");
            break;
        }
        interrupted_count += 1;

        let epoch_store = authority_state.epoch_store_for_testing();
        let g = epoch_store
            .acquire_tx_guard(&VerifiedExecutableTransaction::new_from_certificate(
                shared_object_cert.clone(),
            ))
            .await
            .unwrap();

        // assert that the tx was dropped mid-stream due to the timeout.
        assert_eq!(g.retry_num(), 1);
        std::mem::drop(g);

        // Now run the tx to completion. Interrupted tx should be retriable via TransactionManager.
        // Must manually enqueue the cert to transaction manager because send_consensus_no_execution
        // explicitly doesn't do so.
        authority_state
            .transaction_manager()
            .enqueue_certificates(vec![shared_object_cert.clone()], &epoch_store)
            .unwrap();
        authority_state
            .execute_certificate(
                &shared_object_cert,
                &authority_state.epoch_store_for_testing(),
            )
            .await
            .unwrap();
    }

    // ensure we tested something
    assert!(interrupted_count >= 1);
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
    assert!(signed_effects.data().status().is_ok());

    let signed_effects2 = authority_state
        .execute_certificate(
            &certified_transfer_transaction,
            &authority_state.epoch_store_for_testing(),
        )
        .await
        .unwrap();
    assert!(signed_effects2.data().status().is_ok());

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

// skipped because it violates SUI conservation checks
#[ignore]
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
    let gas_used = effects.gas_cost_summary().gas_used();

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

    // Now we try to construct a transaction with a smaller gas budget than required.
    let data = TransactionData::new_transfer_with_dummy_gas_price(
        sender,
        obj_ref,
        recipient,
        gas_ref,
        gas_used - 5,
    );

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
    let authority_state = init_state().await;
    // There should not be any object with ID zero
    assert!(authority_state
        .get_latest_parent_entry(ObjectID::ZERO)
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
    let (obj_ref, tx) = authority_state
        .get_latest_parent_entry(new_object_id1)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(obj_ref.0, new_object_id1);
    assert_eq!(obj_ref.1, update_version);
    assert_eq!(*effects.transaction_digest(), tx);

    let delete_version = SequenceNumber::lamport_increment([obj_ref.1, effects.gas_object().0 .1]);

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

    // Test get_latest_parent_entry function

    // The objects just after the gas object also returns None
    let mut x = gas_object_id.to_vec();
    let last_index = x.len() - 1;
    // Prevent overflow
    x[last_index] = u8::MAX - x[last_index];
    let unknown_object_id: ObjectID = x.try_into().unwrap();
    assert!(authority_state
        .get_latest_parent_entry(unknown_object_id)
        .await
        .unwrap()
        .is_none());

    // Check gas object is returned.
    let (obj_ref, tx) = authority_state
        .get_latest_parent_entry(gas_object_id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(obj_ref.0, gas_object_id);
    assert_eq!(obj_ref.1, delete_version);
    assert_eq!(*effects.transaction_digest(), tx);

    // Check entry for deleted object is returned
    let (obj_ref, tx) = authority_state
        .get_latest_parent_entry(new_object_id1)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(obj_ref.0, new_object_id1);
    assert_eq!(obj_ref.1, delete_version);
    assert_eq!(obj_ref.2, ObjectDigest::OBJECT_DIGEST_DELETED);
    assert_eq!(*effects.transaction_digest(), tx);
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
        committee: Committee,
        authority_key: AuthorityKeyPair,
        store: Arc<AuthorityStore>,
    ) -> Arc<AuthorityState> {
        let name = authority_key.public().into();
        let secrete = Arc::pin(authority_key);
        let dir = env::temp_dir();
        let epoch_path = dir.join(format!("DB_{:?}", nondeterministic!(ObjectID::random())));
        fs::create_dir(&epoch_path).unwrap();
        let committee_store = Arc::new(CommitteeStore::new(epoch_path, &committee, None));

        let epoch_store_path = dir.join(format!("DB_{:?}", ObjectID::random()));
        fs::create_dir(&epoch_store_path).unwrap();
        let registry = Registry::new();
        let cache_metrics = Arc::new(ResolverMetrics::new(&registry));
        let verified_cert_cache_metrics = VerifiedCertificateCacheMetrics::new(&registry);
        let epoch_store = AuthorityPerEpochStore::new(
            name,
            Arc::new(committee),
            &epoch_store_path,
            None,
            EpochMetrics::new(&registry),
            EpochStartConfiguration::new_for_testing(),
            store.clone(),
            cache_metrics,
            verified_cert_cache_metrics,
        );

        let checkpoint_store_path = dir.join(format!("DB_{:?}", ObjectID::random()));
        fs::create_dir(&checkpoint_store_path).unwrap();
        let checkpoint_store = CheckpointStore::new(&checkpoint_store_path);

        AuthorityState::new(
            name,
            secrete,
            SupportedProtocolVersions::SYSTEM_DEFAULT,
            store,
            epoch_store,
            committee_store,
            None,
            None,
            checkpoint_store,
            &registry,
            AuthorityStorePruningConfig::default(),
            &[], // no genesis objects
            10000,
            &DBCheckpointConfig::default(),
        )
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
    let store = Arc::new(
        AuthorityStore::open_with_committee_for_testing(&path, None, &committee, &genesis, 0)
            .await
            .unwrap(),
    );
    let authority = init_state(committee, authority_key, store).await;

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
    let store = Arc::new(
        AuthorityStore::open_with_committee_for_testing(&path, None, &committee, &genesis, 0)
            .await
            .unwrap(),
    );
    let authority2 = init_state(committee, authority_key, store).await;
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
    let epoch_store = authority_state.load_epoch_store_one_call_per_task();

    let gas_ref = gas_object.compute_object_reference();
    let tx_data = TransactionData::new_with_dummy_gas_price(
        TransactionKind::ConsensusCommitPrologue(ConsensusCommitPrologue {
            epoch: 0,
            round: 0,
            commit_timestamp_ms: 42,
        }),
        sender,
        gas_ref,
        MAX_GAS,
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

    let tx_data = TransactionData::new_move_call_with_dummy_gas_price(
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
        MAX_GAS,
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

    let tx_data = TransactionData::new_move_call_with_dummy_gas_price(
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
        MAX_GAS,
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
    let authority_state = init_state().await;
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
        authority_state.epoch_store_for_testing().committee()
    );
}

#[cfg(msim)]
#[sim_test]
async fn test_sui_system_state_nop_upgrade() {
    use sui_adapter::programmable_transactions;
    use sui_types::sui_system_state::SUI_SYSTEM_STATE_TESTING_VERSION1;
    use sui_types::{MOVE_STDLIB_ADDRESS, SUI_SYSTEM_STATE_OBJECT_SHARED_VERSION};

    let authority_state = init_state().await;

    let protocol_config = ProtocolConfig::get_for_version(ProtocolVersion::MIN);
    let native_functions =
        sui_framework::natives::all_natives(MOVE_STDLIB_ADDRESS, SUI_FRAMEWORK_ADDRESS);
    let move_vm = adapter::new_move_vm(native_functions.clone(), &protocol_config)
        .expect("We defined natives to not fail here");
    let mut temporary_store = TemporaryStore::new(
        authority_state.database.clone(),
        InputObjects::new(vec![(
            InputObjectKind::SharedMoveObject {
                id: SUI_SYSTEM_STATE_OBJECT_ID,
                initial_shared_version: SUI_SYSTEM_STATE_OBJECT_SHARED_VERSION,
                mutable: true,
            },
            authority_state
                .get_object(&SUI_SYSTEM_STATE_OBJECT_ID)
                .await
                .unwrap()
                .unwrap(),
        )]),
        TransactionDigest::genesis(),
        &protocol_config,
    );
    let system_object_arg = CallArg::Object(ObjectArg::SharedObject {
        id: SUI_SYSTEM_STATE_OBJECT_ID,
        initial_shared_version: SUI_SYSTEM_STATE_OBJECT_SHARED_VERSION,
        mutable: true,
    });
    let new_protocol_version = ProtocolVersion::MIN.as_u64() + 1;
    let new_system_state_version = SUI_SYSTEM_STATE_TESTING_VERSION1;

    let pt = {
        let mut builder = ProgrammableTransactionBuilder::new();
        builder
            .move_call(
                SUI_FRAMEWORK_ADDRESS.into(),
                ident_str!("sui_system").to_owned(),
                ident_str!("advance_epoch").to_owned(),
                vec![],
                vec![
                    system_object_arg,
                    CallArg::Pure(bcs::to_bytes(&1u64).unwrap()),
                    CallArg::Pure(bcs::to_bytes(&new_protocol_version).unwrap()),
                    CallArg::Pure(bcs::to_bytes(&0u64).unwrap()),
                    CallArg::Pure(bcs::to_bytes(&0u64).unwrap()),
                    CallArg::Pure(bcs::to_bytes(&0u64).unwrap()),
                    CallArg::Pure(bcs::to_bytes(&0u64).unwrap()),
                    CallArg::Pure(bcs::to_bytes(&0u64).unwrap()),
                    CallArg::Pure(bcs::to_bytes(&0u64).unwrap()),
                    CallArg::Pure(bcs::to_bytes(&new_system_state_version).unwrap()), // Upgrade sui system state, set new version to 1.
                ],
            )
            .unwrap();
        builder.finish()
    };
    programmable_transactions::execution::execute::<_, _, execution_mode::Normal>(
        &protocol_config,
        &move_vm,
        &mut temporary_store,
        &mut TxContext::new(
            &SuiAddress::default(),
            &TransactionDigest::genesis(),
            &EpochData::new(0, 0, CheckpointDigest::default()),
        ),
        &mut SuiGasStatus::new_unmetered(),
        None,
        pt,
    )
    .unwrap();
    let inner = temporary_store.into_inner();
    // Make sure that the new version is set, and that we can still read the inner object.
    assert_eq!(
        inner.get_sui_system_state_wrapper_object().unwrap().version,
        new_system_state_version
    );
    let inner_state = inner.get_sui_system_state_object().unwrap();
    assert_eq!(inner_state.version(), new_system_state_version);
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

    let gas_ref = gas_object.compute_object_reference();
    let tx_data = TransactionData::new_transfer_sui_with_dummy_gas_price(
        recipient, sender, None, gas_ref, MAX_GAS,
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

    let gas_ref = gas_object.compute_object_reference();
    let tx_data = TransactionData::new_transfer_sui_with_dummy_gas_price(
        recipient,
        sender,
        Some(500),
        gas_ref,
        MAX_GAS,
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

    let tx_data = TransactionData::new_transfer_sui_with_dummy_gas_price(
        recipient,
        sender,
        None,
        gas_object.compute_object_reference(),
        MAX_GAS,
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
        db.get_latest_parent_entry(gas_object_id).unwrap().unwrap(),
        (gas_object_ref, TransactionDigest::genesis()),
    );
    assert!(db.get_transaction(&tx_digest).unwrap().is_none());
    assert!(!db.as_ref().is_tx_already_executed(&tx_digest).unwrap());
}

#[tokio::test]
async fn test_store_revert_wrap_move_call() {
    let (sender, sender_key): (_, AccountKeyPair) = get_key_pair();
    let gas_object_id = ObjectID::random();
    let (authority_state, object_basics) =
        init_state_with_ids_and_object_basics(vec![(sender, gas_object_id)]).await;

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
        TransactionData::new_move_call_with_dummy_gas_price(
            sender,
            object_basics.0,
            ident_str!("object_basics").to_owned(),
            ident_str!("wrap").to_owned(),
            vec![],
            create_effects.gas_object().0,
            vec![CallArg::Object(ObjectArg::ImmOrOwnedObject(object_v0))],
            MAX_GAS,
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
        TransactionData::new_move_call_with_dummy_gas_price(
            sender,
            object_basics.0,
            ident_str!("object_basics").to_owned(),
            ident_str!("unwrap").to_owned(),
            vec![],
            wrap_effects.gas_object().0,
            vec![CallArg::Object(ObjectArg::ImmOrOwnedObject(wrapper_v0))],
            MAX_GAS,
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
        TransactionData::new_move_call_with_dummy_gas_price(
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
            MAX_GAS,
        )
        .unwrap(),
        &sender_key,
    );

    let add_cert = init_certified_transaction(add_txn, &authority_state);

    let add_effects = authority_state
        .try_execute_for_test(&add_cert)
        .await
        .unwrap()
        .into_message();

    assert!(add_effects.status().is_ok());
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
        TransactionData::new_move_call_with_dummy_gas_price(
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
            MAX_GAS,
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
        TransactionData::new_move_call_with_dummy_gas_price(
            sender,
            object_basics.0,
            ident_str!("object_basics").to_owned(),
            ident_str!("remove_ofield").to_owned(),
            vec![],
            add_effects.gas_object().0,
            vec![CallArg::Object(ObjectArg::ImmOrOwnedObject(outer_v1))],
            MAX_GAS,
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
        .filter_map(|(id, _, _)| {
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
        .filter_map(|(id, v, _)| {
            if ignore.contains(&id) {
                None
            } else {
                Some((id, v))
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
pub async fn init_state() -> Arc<AuthorityState> {
    let dir = tempfile::TempDir::new().unwrap();
    let network_config = sui_config::builder::ConfigBuilder::new(&dir).build();
    let genesis = network_config.genesis;
    let keypair = network_config.validator_configs[0]
        .protocol_key_pair()
        .copy();

    init_state_with_committee(&genesis, &keypair).await
}

#[cfg(test)]
pub async fn init_state_validator_with_fullnode() -> (Arc<AuthorityState>, Arc<AuthorityState>) {
    use sui_types::crypto::get_authority_key_pair;

    let dir = tempfile::TempDir::new().unwrap();
    let network_config = sui_config::builder::ConfigBuilder::new(&dir).build();
    let genesis = network_config.genesis;
    let keypair = network_config.validator_configs[0]
        .protocol_key_pair()
        .copy();

    let validator = init_state_with_committee(&genesis, &keypair).await;
    let fullnode_key_pair = get_authority_key_pair().1;
    let fullnode = init_state_with_committee(&genesis, &fullnode_key_pair).await;
    (validator, fullnode)
}

#[cfg(test)]
pub async fn init_state_with_committee(
    genesis: &Genesis,
    authority_key: &AuthorityKeyPair,
) -> Arc<AuthorityState> {
    AuthorityState::new_for_testing(genesis.committee().unwrap(), authority_key, None, genesis)
        .await
}

#[cfg(test)]
pub async fn init_state_with_ids<I: IntoIterator<Item = (SuiAddress, ObjectID)>>(
    objects: I,
) -> Arc<AuthorityState> {
    let state = init_state().await;
    for (address, object_id) in objects {
        let obj = Object::with_id_owner_for_testing(object_id, address);
        state.insert_genesis_object(obj).await;
    }
    state
}

#[cfg(test)]
pub async fn init_state_with_objects_and_object_basics<I: IntoIterator<Item = Object>>(
    objects: I,
) -> (Arc<AuthorityState>, ObjectRef) {
    let state = init_state().await;
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
    let state = init_state().await;
    for (address, object_id) in objects {
        let obj = Object::with_id_owner_for_testing(object_id, address);
        state.insert_genesis_object(obj).await;
    }
    publish_object_basics(state).await
}

async fn publish_object_basics(state: Arc<AuthorityState>) -> (Arc<AuthorityState>, ObjectRef) {
    use sui_framework_build::compiled_package::BuildConfig;

    // add object_basics package object to genesis, since lots of test use it
    let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    path.push("src/unit_tests/data/object_basics");
    let modules: Vec<_> = BuildConfig::new_for_testing()
        .build(path)
        .unwrap()
        .get_modules()
        .into_iter()
        .cloned()
        .collect();
    let digest = TransactionDigest::genesis();
    let (std_move_pkg, sui_move_pkg) = sui_framework::make_std_sui_move_pkgs();
    let pkg =
        Object::new_package_for_testing(modules, digest, [&std_move_pkg, &sui_move_pkg]).unwrap();
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
    use sui_framework_build::compiled_package::BuildConfig;

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
        .into_iter()
        .cloned()
        .collect();
    let digest = TransactionDigest::genesis();
    let (std_move_pkg, sui_move_pkg) = sui_framework::make_std_sui_move_pkgs();
    let pkg =
        Object::new_package_for_testing(modules, digest, [&std_move_pkg, &sui_move_pkg]).unwrap();
    let pkg_ref = pkg.compute_object_reference();
    validator.insert_genesis_object(pkg.clone()).await;
    fullnode.insert_genesis_object(pkg).await;
    (validator, fullnode, pkg_ref)
}

#[cfg(test)]
pub async fn init_state_with_ids_and_versions<
    I: IntoIterator<Item = (SuiAddress, ObjectID, SequenceNumber)>,
>(
    objects: I,
) -> Arc<AuthorityState> {
    let state = init_state().await;
    for (address, object_id, version) in objects {
        let obj = Object::with_id_owner_version_for_testing(object_id, version, address);
        state.insert_genesis_object(obj).await;
    }
    state
}

pub async fn init_state_with_objects<I: IntoIterator<Item = Object>>(
    objects: I,
) -> Arc<AuthorityState> {
    let dir = tempfile::TempDir::new().unwrap();
    let network_config = sui_config::builder::ConfigBuilder::new(&dir).build();
    let genesis = network_config.genesis;
    let keypair = network_config.validator_configs[0]
        .protocol_key_pair()
        .copy();
    init_state_with_objects_and_committee(objects, &genesis, &keypair).await
}

pub async fn init_state_with_objects_and_committee<I: IntoIterator<Item = Object>>(
    objects: I,
    genesis: &Genesis,
    authority_key: &AuthorityKeyPair,
) -> Arc<AuthorityState> {
    let state = init_state_with_committee(genesis, authority_key).await;
    for o in objects {
        state.insert_genesis_object(o).await;
    }
    state
}

#[cfg(test)]
pub async fn init_state_with_object_id(
    address: SuiAddress,
    object: ObjectID,
) -> Arc<AuthorityState> {
    init_state_with_ids(std::iter::once((address, object))).await
}

#[cfg(test)]
pub fn init_transfer_transaction(
    sender: SuiAddress,
    secret: &AccountKeyPair,
    recipient: SuiAddress,
    object_ref: ObjectRef,
    gas_object_ref: ObjectRef,
) -> VerifiedTransaction {
    let data = TransactionData::new_transfer_with_dummy_gas_price(
        recipient,
        object_ref,
        sender,
        gas_object_ref,
        10000,
    );
    to_sender_signed_transaction(data, secret)
}

#[cfg(test)]
fn init_certified_transfer_transaction(
    sender: SuiAddress,
    secret: &AccountKeyPair,
    recipient: SuiAddress,
    object_ref: ObjectRef,
    gas_object_ref: ObjectRef,
    authority_state: &AuthorityState,
) -> VerifiedCertificate {
    let transfer_transaction =
        init_transfer_transaction(sender, secret, recipient, object_ref, gas_object_ref);
    init_certified_transaction(transfer_transaction, authority_state)
}

#[cfg(test)]
fn init_certified_transaction(
    transaction: VerifiedTransaction,
    authority_state: &AuthorityState,
) -> VerifiedCertificate {
    let vote = VerifiedSignedTransaction::new(
        0,
        transaction.clone(),
        authority_state.name,
        &*authority_state.secret,
    );
    let epoch_store = authority_state.epoch_store_for_testing();
    CertifiedTransaction::new(
        transaction.into_message(),
        vec![vote.auth_sig().clone()],
        epoch_store.committee(),
    )
    .unwrap()
    .verify(epoch_store.committee())
    .unwrap()
}

#[cfg(test)]
pub(crate) async fn send_consensus(authority: &AuthorityState, cert: &VerifiedCertificate) {
    let transaction = SequencedConsensusTransaction::new_test(
        ConsensusTransaction::new_certificate_message(&authority.name, cert.clone().into_inner()),
    );

    if let Ok(transaction) = authority
        .epoch_store_for_testing()
        .verify_consensus_transaction(transaction, &authority.metrics.skipped_consensus_txns)
    {
        authority
            .epoch_store_for_testing()
            .handle_consensus_transaction(
                transaction,
                &Arc::new(CheckpointServiceNoop {}),
                authority.transaction_manager(),
                authority.db(),
            )
            .await
            .unwrap();
    } else {
        warn!("Failed to verify certificate: {:?}", cert);
    }
}

#[cfg(test)]
pub(crate) async fn send_consensus_no_execution(
    authority: &AuthorityState,
    cert: &VerifiedCertificate,
) {
    let transaction = SequencedConsensusTransaction::new_test(
        ConsensusTransaction::new_certificate_message(&authority.name, cert.clone().into_inner()),
    );

    if let Ok(transaction) = authority
        .epoch_store_for_testing()
        .verify_consensus_transaction(transaction, &authority.metrics.skipped_consensus_txns)
    {
        // Call process_consensus_transaction() instead of handle_consensus_transaction(), to avoid actually executing cert.
        // This allows testing cert execution independently.
        authority
            .epoch_store_for_testing()
            .process_consensus_transaction(
                transaction,
                &Arc::new(CheckpointServiceNoop {}),
                &authority.db(),
            )
            .await
            .unwrap();
    } else {
        warn!("Failed to verify certificate: {:?}", cert);
    }
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
    builder.command(Command::move_call(
        *package,
        Identifier::new(module).unwrap(),
        Identifier::new(function).unwrap(),
        type_args,
        args,
    ));
    let data = TransactionData::new_programmable_with_dummy_gas_price(
        *sender,
        vec![gas_object_ref],
        builder.finish(),
        MAX_GAS,
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
) -> SuiResult<TransactionEffects> {
    execute_programmable_transaction_(
        authority,
        None,
        gas_object_id,
        sender,
        sender_key,
        pt,
        /* with_shared */ false,
    )
    .await
}

pub async fn execute_programmable_transaction_(
    authority: &AuthorityState,
    fullnode: Option<&AuthorityState>,
    gas_object_id: &ObjectID,
    sender: &SuiAddress,
    sender_key: &AccountKeyPair,
    pt: ProgrammableTransaction,
    with_shared: bool, // Move call includes shared objects
) -> SuiResult<TransactionEffects> {
    let gas_object = authority.get_object(gas_object_id).await.unwrap();
    let gas_object_ref = gas_object.unwrap().compute_object_reference();
    let data = TransactionData::new_programmable_with_dummy_gas_price(
        *sender,
        vec![gas_object_ref],
        pt,
        MAX_GAS,
    );

    let transaction = to_sender_signed_transaction(data, sender_key);
    let signed_effects =
        send_and_confirm_transaction_(authority, fullnode, transaction, with_shared)
            .await?
            .1;
    Ok(signed_effects.into_data())
}

pub async fn call_move_with_gas_coins(
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
        DUMMY_GAS_PRICE,
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

pub async fn create_move_object_with_gas_coins(
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
        .dev_inspect_transaction(*sender, kind, Some(1))
        .await
}

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

    let data = TransactionData::new_move_call_with_dummy_gas_price(
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
        MAX_GAS,
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
        let effects1 = authority1.try_execute_for_test(&certificate).await.unwrap();

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

#[tokio::test]
async fn test_tallying_rule_score_updates() {
    let seed = [1u8; 32];
    let mut rng = StdRng::from_seed(seed);
    let (authorities, committee) = make_committee_key(&mut rng);
    let auth_0_name = authorities[0].public().into();
    let auth_1_name = authorities[1].public().into();
    let auth_2_name = authorities[2].public().into();
    let auth_3_name = authorities[3].public().into();
    let dir = env::temp_dir();
    let path = dir.join(format!("DB_{:?}", ObjectID::random()));
    fs::create_dir(&path).unwrap();
    let registry = Registry::new();
    let metrics = EpochMetrics::new(&registry);

    let network_config = sui_config::builder::ConfigBuilder::new(&dir)
        .rng(rng)
        .build();
    let genesis = network_config.genesis;
    let store = Arc::new(
        AuthorityStore::open_with_committee_for_testing(&path, None, &committee, &genesis, 0)
            .await
            .unwrap(),
    );

    let cache_metrics = Arc::new(ResolverMetrics::new(&registry));
    let verified_cert_cache_metrics = VerifiedCertificateCacheMetrics::new(&registry);
    let epoch_store = AuthorityPerEpochStore::new(
        auth_0_name,
        Arc::new(committee.clone()),
        &path,
        None,
        metrics.clone(),
        EpochStartConfiguration::new_for_testing(),
        store,
        cache_metrics,
        verified_cert_cache_metrics,
    );

    let get_stored_seq_num_and_counter = |auth_name: &AuthorityName| {
        epoch_store
            .get_num_certified_checkpoint_sigs_by(auth_name)
            .unwrap()
    };

    // Only include auth_[0..3] in this certified checkpoint.
    let ckpt_1 = mock_certified_checkpoint(authorities[0..3].iter(), committee.clone(), 1);

    assert!(epoch_store
        .record_certified_checkpoint_signatures(&ckpt_1)
        .is_ok());

    assert_eq!(
        get_stored_seq_num_and_counter(&auth_0_name),
        Some((Some(1), 1))
    );
    assert_eq!(
        get_stored_seq_num_and_counter(&auth_1_name),
        Some((Some(1), 1))
    );
    assert_eq!(
        get_stored_seq_num_and_counter(&auth_2_name),
        Some((Some(1), 1))
    );
    assert_eq!(get_stored_seq_num_and_counter(&auth_3_name), None);

    // Only include auth_1, auth_2 and auth_3 in this certified checkpoint.
    let ckpt_2 = mock_certified_checkpoint(authorities[1..].iter(), committee.clone(), 2);

    assert!(epoch_store
        .record_certified_checkpoint_signatures(&ckpt_2)
        .is_ok());

    assert_eq!(
        get_stored_seq_num_and_counter(&auth_0_name),
        Some((Some(1), 1))
    );
    assert_eq!(
        get_stored_seq_num_and_counter(&auth_1_name),
        Some((Some(2), 2))
    );
    assert_eq!(
        get_stored_seq_num_and_counter(&auth_2_name),
        Some((Some(2), 2))
    );
    assert_eq!(
        get_stored_seq_num_and_counter(&auth_3_name),
        Some((Some(2), 1))
    );

    // Check idempotency.
    // Call the record function again with the same checkpoint and the stored
    // values shouldn't change.
    assert!(epoch_store
        .record_certified_checkpoint_signatures(&ckpt_2)
        .is_ok());

    assert_eq!(
        get_stored_seq_num_and_counter(&auth_0_name),
        Some((Some(1), 1))
    );
    assert_eq!(
        get_stored_seq_num_and_counter(&auth_1_name),
        Some((Some(2), 2))
    );
    assert_eq!(
        get_stored_seq_num_and_counter(&auth_2_name),
        Some((Some(2), 2))
    );
    assert_eq!(
        get_stored_seq_num_and_counter(&auth_3_name),
        Some((Some(2), 1))
    );

    // Check that the metrics are correctly set.
    let get_auth_score_metric = |auth_name: &AuthorityName| {
        metrics
            .tallying_rule_scores
            .get_metric_with_label_values(&[
                &format!("{:?}", auth_name.concise()),
                &committee.epoch().to_string(),
            ])
            .unwrap()
            .get()
    };
    assert_eq!(get_auth_score_metric(&auth_0_name), 1);
    assert_eq!(get_auth_score_metric(&auth_1_name), 2);
    assert_eq!(get_auth_score_metric(&auth_2_name), 2);
    assert_eq!(get_auth_score_metric(&auth_3_name), 1);
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
            &committee,
            &protocol_config,
            capabilities
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
            &committee,
            &protocol_config,
            capabilities.clone(),
        )
    );

    // Now 2f+1 is enough to upgrade
    protocol_config.set_buffer_stake_for_protocol_upgrade_bps_for_testing(0);

    assert_eq!(
        (ver(2), sort(vec![o1, o2])),
        AuthorityState::choose_protocol_version_and_system_packages(
            ProtocolVersion::MIN,
            &committee,
            &protocol_config,
            capabilities
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
            &committee,
            &protocol_config,
            capabilities,
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
            &committee,
            &protocol_config,
            capabilities
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
            &committee,
            &protocol_config,
            capabilities
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
            &committee,
            &protocol_config,
            capabilities
        )
    );
}

// skipped because it violates SUI conservation checks
#[ignore]
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

    // 1. get the cost of the transaction so we can play with multiple gas coins
    // 100,000 should be enough money for that transaction.
    let gas_used = run_and_check(100_000, 1, 100_000, true).await;

    // add something to the gas used to account for multiple gas coins being charged for
    let reference_gas_used = gas_used + 1_000;
    let three_coin_gas = run_and_check(reference_gas_used, 3, reference_gas_used, true).await;
    run_and_check(reference_gas_used, 10, reference_gas_used - 100, true).await;

    // make less then required to succeed
    let reference_gas_used = gas_used - 10;
    run_and_check(reference_gas_used, 2, reference_gas_used - 10, false).await;
    run_and_check(reference_gas_used, 30, reference_gas_used, false).await;
    // use a small amount less than what 3 coins above reported (with success)
    run_and_check(three_coin_gas, 3, three_coin_gas - 1, false).await;
}
