// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::fmt;
use std::sync::Arc;

use super::*;

use crate::authority::authority_test_utils::{
    init_state_validator_with_fullnode, submit_and_execute,
};
use move_core_types::identifier::Identifier;
use move_core_types::language_storage::{StructTag, TypeTag};
use sui_json_rpc_types::SuiTransactionBlockEffectsAPI;
use sui_protocol_config::ProtocolConfig;
use sui_types::base_types::FullObjectRef;
use sui_types::collection_types::Table as TableType;
use sui_types::crypto::{AccountKeyPair, get_key_pair};
use sui_types::effects::TransactionEffectsAPI;
use sui_types::error::{SuiError, SuiErrorKind, UserInputError};
use sui_types::gas::GasCostSummary;
use sui_types::gas_coin::GasCoin;
use sui_types::object::{GAS_VALUE_FOR_TESTING, MoveObject, OBJECT_START_VERSION};
use sui_types::programmable_transaction_builder::ProgrammableTransactionBuilder;
use sui_types::transaction::Command;
use sui_types::transaction_executor::TransactionChecks;
use sui_types::utils::to_sender_signed_transaction;
use sui_types::{SUI_FRAMEWORK_ADDRESS, SUI_FRAMEWORK_PACKAGE_ID};

// =============================================================================
// Node entry points
// =============================================================================

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum NodeEntryPoint {
    Validator, // submit_and_execute() -> try_execute_executable_for_test() -> try_execute_immediately()
    DryRun,    // dry_exec_transaction()
    DevInspectSkipChecks, // dev_inspect_transaction_block(skip_checks=true)
    DevInspectFullChecks, // dev_inspect_transaction_block(skip_checks=false)
    SimulateFullChecks, // simulate_transaction(TransactionChecks::Enabled, mock_gas=false)
    SimulateFullChecksMockGas, // simulate_transaction(TransactionChecks::Enabled, mock_gas=true)
    SimulateSkipChecks, // simulate_transaction(TransactionChecks::Disabled, mock_gas=false)
    SimulateSkipChecksMockGas, // simulate_transaction(TransactionChecks::Disabled, mock_gas=true)
}

impl fmt::Display for NodeEntryPoint {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}", self)
    }
}

const TEST_GAS_BUDGET: u64 = 500_000_000;

// Different forms of dev inspect
const DEV_INSPECT_ENTRY_POINTS: &[NodeEntryPoint] = &[
    NodeEntryPoint::DevInspectSkipChecks,
    NodeEntryPoint::SimulateSkipChecks,
    NodeEntryPoint::SimulateSkipChecksMockGas,
];

// API that perform more rigorous transaction data checks
const CHECKED_ENTRY_POINTS: &[NodeEntryPoint] = &[
    NodeEntryPoint::Validator,
    NodeEntryPoint::DryRun,
    NodeEntryPoint::DevInspectFullChecks,
    NodeEntryPoint::SimulateFullChecks,
    NodeEntryPoint::SimulateFullChecksMockGas,
];

// =============================================================================
// Test environment setup
// =============================================================================

struct TestEnv {
    validator: Arc<AuthorityState>,
    fullnode: Arc<AuthorityState>,
    sender: SuiAddress,
    sender_key: AccountKeyPair,
    gas_object_ref: ObjectRef,
    gas_coin_refs: Vec<ObjectRef>,
    object_ref: ObjectRef,
    extra_object_ref: ObjectRef,
    other_owner_coin_ref: ObjectRef,
    rgp: u64,
}

// Makes a validator, a full node and the data to work with
async fn setup_test_env() -> TestEnv {
    let (validator, fullnode) = init_state_validator_with_fullnode().await;

    let (sender, sender_key): (_, AccountKeyPair) = get_key_pair();

    let gas_object_id = ObjectID::random();
    let gas_object =
        Object::with_id_owner_gas_for_testing(gas_object_id, sender, GAS_VALUE_FOR_TESTING);
    let gas_object_ref = gas_object.compute_object_reference();
    validator.insert_genesis_object(gas_object.clone()).await;
    fullnode.insert_genesis_object(gas_object).await;

    let object_id = ObjectID::random();
    let object = Object::with_id_owner_for_testing(object_id, sender);
    let object_ref = object.compute_object_reference();
    validator.insert_genesis_object(object.clone()).await;
    fullnode.insert_genesis_object(object).await;

    let extra_object_id = ObjectID::random();
    let extra_object = {
        let table = TableType {
            id: extra_object_id,
            size: 0,
        };
        let struct_tag = StructTag {
            address: SUI_FRAMEWORK_ADDRESS,
            module: Identifier::new("table").unwrap(),
            name: Identifier::new("Table").unwrap(),
            type_params: vec![TypeTag::U64, TypeTag::U64],
        };
        let move_obj = unsafe {
            MoveObject::new_from_execution_with_limit(
                struct_tag.into(),
                true,
                OBJECT_START_VERSION,
                bcs::to_bytes(&table).unwrap(),
                256,
            )
            .unwrap()
        };
        Object::new_move(
            move_obj,
            Owner::AddressOwner(sender),
            TransactionDigest::genesis_marker(),
        )
    };
    let extra_object_ref = extra_object.compute_object_reference();
    validator.insert_genesis_object(extra_object.clone()).await;
    fullnode.insert_genesis_object(extra_object).await;

    let mut gas_coin_refs = Vec::new();
    for _ in 0..3 {
        let id = ObjectID::random();
        let obj = Object::with_id_owner_gas_for_testing(id, sender, GAS_VALUE_FOR_TESTING);
        gas_coin_refs.push(obj.compute_object_reference());
        validator.insert_genesis_object(obj.clone()).await;
        fullnode.insert_genesis_object(obj).await;
    }

    let other_owner = SuiAddress::random_for_testing_only();
    let other_owner_coin_id = ObjectID::random();
    let other_owner_coin = Object::with_id_owner_gas_for_testing(
        other_owner_coin_id,
        other_owner,
        GAS_VALUE_FOR_TESTING,
    );
    let other_owner_coin_ref = other_owner_coin.compute_object_reference();
    validator
        .insert_genesis_object(other_owner_coin.clone())
        .await;
    fullnode.insert_genesis_object(other_owner_coin).await;

    let rgp = validator.reference_gas_price_for_testing().unwrap();

    TestEnv {
        validator,
        fullnode,
        sender,
        sender_key,
        gas_object_ref,
        gas_coin_refs,
        object_ref,
        extra_object_ref,
        other_owner_coin_ref,
        rgp,
    }
}

fn build_transfer(env: &TestEnv, budget: u64, gas_price: u64) -> TransactionData {
    let recipient = SuiAddress::random_for_testing_only();
    let pt = {
        let mut builder = ProgrammableTransactionBuilder::new();
        builder
            .transfer_object(recipient, FullObjectRef::from_fastpath_ref(env.object_ref))
            .unwrap();
        builder.finish()
    };
    let kind = TransactionKind::ProgrammableTransaction(pt);
    TransactionData::new(kind, env.sender, env.gas_object_ref, budget, gas_price)
}

// =============================================================================
// Running all entry points and collecting results
// =============================================================================

type EntryPointResult = (NodeEntryPoint, Result<GasCostSummary, SuiError>);

async fn run_all_entry_points(env: &TestEnv, data: TransactionData) -> Vec<EntryPointResult> {
    let sender = env.sender;
    let mut results: Vec<EntryPointResult> = Vec::new();

    // Path 1: Validator execute
    let tx = to_sender_signed_transaction(data.clone(), &env.sender_key);
    let mapped = match submit_and_execute(&env.validator, tx).await {
        Ok((_, effects)) => Ok(effects.data().gas_cost_summary().clone()),
        Err(e) => Err(e),
    };
    results.push((NodeEntryPoint::Validator, mapped));

    // Path 2: Fullnode dry_exec_transaction
    let mapped = match env.fullnode.dry_exec_transaction(data.clone()).await {
        Ok((_, _, effects, _)) => Ok(effects.gas_cost_summary().clone()),
        Err(e) => Err(e),
    };
    results.push((NodeEntryPoint::DryRun, mapped));

    // Path 3: Fullnode dev_inspect_transaction_block (skip_checks=true)
    {
        let dev_kind = data.clone().into_kind();
        let mapped = match env
            .fullnode
            .dev_inspect_transaction_block(
                sender,
                dev_kind,
                Some(data.gas_data().price),
                Some(data.gas_data().budget),
                None,
                Some(data.gas_data().payment.clone()),
                None,
                Some(true),
            )
            .await
        {
            Ok(r) => Ok(r.effects.gas_cost_summary().clone()),
            Err(e) => Err(e),
        };
        results.push((NodeEntryPoint::DevInspectSkipChecks, mapped));
    }

    // Path 4: Fullnode dev_inspect_transaction_block (skip_checks=false)
    {
        let dev_kind = data.clone().into_kind();
        let mapped = match env
            .fullnode
            .dev_inspect_transaction_block(
                sender,
                dev_kind,
                Some(data.gas_data().price),
                Some(data.gas_data().budget),
                None,
                Some(data.gas_data().payment.clone()),
                None,
                Some(false),
            )
            .await
        {
            Ok(r) => Ok(r.effects.gas_cost_summary().clone()),
            Err(e) => Err(e),
        };
        results.push((NodeEntryPoint::DevInspectFullChecks, mapped));
    }

    // Path 5-8: Fullnode simulate_transaction — all combos
    for (entry_point, checks, mock_gas) in [
        (
            NodeEntryPoint::SimulateFullChecks,
            TransactionChecks::Enabled,
            false,
        ),
        (
            NodeEntryPoint::SimulateFullChecksMockGas,
            TransactionChecks::Enabled,
            true,
        ),
        (
            NodeEntryPoint::SimulateSkipChecks,
            TransactionChecks::Disabled,
            false,
        ),
        (
            NodeEntryPoint::SimulateSkipChecksMockGas,
            TransactionChecks::Disabled,
            true,
        ),
    ] {
        let mapped = match env
            .fullnode
            .simulate_transaction(data.clone(), checks, mock_gas)
        {
            Ok(r) => Ok(r.effects.gas_cost_summary().clone()),
            Err(e) => Err(e),
        };
        results.push((entry_point, mapped));
    }

    results
}

fn assert_all_equal(results: &[EntryPointResult]) {
    assert!(!results.is_empty(), "no results to compare");
    let (first_entry_point, first_result) = &results[0];
    for (entry_point, result) in &results[1..] {
        assert_eq!(
            first_result, result,
            "mismatch between '{first_entry_point}' and '{entry_point}'"
        );
    }
}

fn find_result(
    results: &[EntryPointResult],
    entry_point: NodeEntryPoint,
) -> &Result<GasCostSummary, SuiError> {
    &results.iter().find(|(p, _)| *p == entry_point).unwrap().1
}

fn assert_err(
    results: &[EntryPointResult],
    entry_point: NodeEntryPoint,
    check: impl Fn(&SuiErrorKind) -> bool,
    expected_desc: &str,
) {
    match find_result(results, entry_point) {
        Err(e) => assert!(
            check(e.as_inner()),
            "'{entry_point}': expected {expected_desc}, got: {e:?}"
        ),
        Ok(gas) => panic!("'{entry_point}': expected {expected_desc}, got Ok({gas:?})"),
    }
}

fn assert_ok(results: &[EntryPointResult], entry_point: NodeEntryPoint) {
    let result = find_result(results, entry_point);
    assert!(
        result.is_ok(),
        "'{entry_point}': expected Ok, got: {result:?}"
    );
}

// =============================================================================
// Tests
// =============================================================================

#[tokio::test]
async fn test_simple_transfer_gas_all_paths() {
    let env = setup_test_env().await;
    let data = build_transfer(&env, TEST_GAS_BUDGET, env.rgp);
    let results = run_all_entry_points(&env, data).await;
    assert_all_equal(&results);
}

#[tokio::test]
async fn test_bad_gas_budget_all_paths() {
    let env = setup_test_env().await;
    let protocol_config = ProtocolConfig::get_for_max_version_UNSAFE();
    let min_budget = protocol_config.base_tx_cost_fixed() * env.rgp;
    let max_budget = protocol_config.max_tx_gas();

    for budget in [0, min_budget - 1, max_budget + 1, u64::MAX] {
        let mut data = build_transfer(&env, TEST_GAS_BUDGET, env.rgp);
        data.gas_data_mut().budget = budget;
        let results = run_all_entry_points(&env, data).await;
        assert_all_equal(&results);
    }
}

#[tokio::test]
async fn test_bad_gas_price_all_paths() {
    let env = setup_test_env().await;
    let protocol_config = ProtocolConfig::get_for_max_version_UNSAFE();
    let max_gas_price = protocol_config.max_gas_price();

    for price in [0, env.rgp - 1, max_gas_price, u64::MAX] {
        let mut data = build_transfer(&env, TEST_GAS_BUDGET, env.rgp);
        data.gas_data_mut().price = price;
        let results = run_all_entry_points(&env, data).await;
        assert_all_equal(&results);
    }
}

#[tokio::test]
async fn test_bad_gas_payment_all_paths() {
    let env = setup_test_env().await;

    // Empty payment: mock-gas paths succeed, others fail.
    // TODO: Needs address balance to plug in appropriately
    {
        let mut data = build_transfer(&env, TEST_GAS_BUDGET, env.rgp);
        data.gas_data_mut().payment = vec![];
        let results = run_all_entry_points(&env, data).await;
        let check_withdraw = |e: &SuiErrorKind| {
            matches!(
                e,
                SuiErrorKind::UserInputError {
                    error: UserInputError::InvalidWithdrawReservation { .. }
                }
            )
        };
        assert_err(
            &results,
            NodeEntryPoint::Validator,
            check_withdraw,
            "InvalidWithdrawReservation",
        );
        assert_ok(&results, NodeEntryPoint::DryRun);
        assert_ok(&results, NodeEntryPoint::DevInspectSkipChecks);
        assert_ok(&results, NodeEntryPoint::DevInspectFullChecks);
        assert_err(
            &results,
            NodeEntryPoint::SimulateFullChecks,
            check_withdraw,
            "InvalidWithdrawReservation",
        );
        assert_ok(&results, NodeEntryPoint::SimulateFullChecksMockGas);
        assert_err(
            &results,
            NodeEntryPoint::SimulateSkipChecks,
            check_withdraw,
            "InvalidWithdrawReservation",
        );
        assert_ok(&results, NodeEntryPoint::SimulateSkipChecksMockGas);
    }

    // Non-existent object ref
    {
        let fake_ref = (
            ObjectID::random(),
            SequenceNumber::new(),
            ObjectDigest::random(),
        );
        let mut data = build_transfer(&env, TEST_GAS_BUDGET, env.rgp);
        data.gas_data_mut().payment = vec![fake_ref];
        let results = run_all_entry_points(&env, data).await;
        assert_all_equal(&results);
    }

    // Non-gas-coin object (Table<u64, u64>) as payment
    {
        let mut data = build_transfer(&env, TEST_GAS_BUDGET, env.rgp);
        data.gas_data_mut().payment = vec![env.extra_object_ref];
        let results = run_all_entry_points(&env, data).await;
        assert_all_equal(&results);
    }

    // Duplicate gas object ref
    {
        let mut data = build_transfer(&env, TEST_GAS_BUDGET, env.rgp);
        data.gas_data_mut().payment = vec![env.gas_object_ref, env.gas_object_ref];
        let results = run_all_entry_points(&env, data).await;
        assert_all_equal(&results);
    }

    // Non-existent ref in multi-coin payment
    {
        let fake_ref = (
            ObjectID::random(),
            SequenceNumber::new(),
            ObjectDigest::random(),
        );
        let mut data = build_transfer(&env, TEST_GAS_BUDGET, env.rgp);
        data.gas_data_mut().payment = vec![env.gas_coin_refs[0], fake_ref, env.gas_coin_refs[1]];
        let results = run_all_entry_points(&env, data).await;
        assert_all_equal(&results);
    }

    // Non-gas-coin in multi-coin payment
    {
        let mut data = build_transfer(&env, TEST_GAS_BUDGET, env.rgp);
        data.gas_data_mut().payment = vec![
            env.gas_coin_refs[0],
            env.extra_object_ref,
            env.gas_coin_refs[1],
        ];
        let results = run_all_entry_points(&env, data).await;
        assert_all_equal(&results);
    }

    // Duplicate ref in multi-coin payment
    {
        let mut data = build_transfer(&env, TEST_GAS_BUDGET, env.rgp);
        data.gas_data_mut().payment = vec![
            env.gas_coin_refs[0],
            env.gas_coin_refs[1],
            env.gas_coin_refs[0],
        ];
        let results = run_all_entry_points(&env, data).await;
        assert_all_equal(&results);
    }

    // Gas coin owned by someone else as sole payment
    {
        let mut data = build_transfer(&env, TEST_GAS_BUDGET, env.rgp);
        data.gas_data_mut().payment = vec![env.other_owner_coin_ref];
        let results = run_all_entry_points(&env, data).await;
        for &entry_point in CHECKED_ENTRY_POINTS {
            assert_err(
                &results,
                entry_point,
                |e| {
                    matches!(
                        e,
                        SuiErrorKind::UserInputError {
                            error: UserInputError::IncorrectUserSignature { .. }
                        }
                    )
                },
                "IncorrectUserSignature",
            );
        }
        for &entry_point in DEV_INSPECT_ENTRY_POINTS {
            assert_ok(&results, entry_point);
        }
    }

    // Gas coin owned by someone else in multi-coin payment
    {
        let mut data = build_transfer(&env, TEST_GAS_BUDGET, env.rgp);
        data.gas_data_mut().payment = vec![
            env.gas_coin_refs[0],
            env.other_owner_coin_ref,
            env.gas_coin_refs[1],
        ];
        let results = run_all_entry_points(&env, data).await;
        for &entry_point in CHECKED_ENTRY_POINTS {
            assert_err(
                &results,
                entry_point,
                |e| {
                    matches!(
                        e,
                        SuiErrorKind::UserInputError {
                            error: UserInputError::IncorrectUserSignature { .. }
                        }
                    )
                },
                "IncorrectUserSignature",
            );
        }
        for &entry_point in DEV_INSPECT_ENTRY_POINTS {
            assert_ok(&results, entry_point);
        }
    }

    // Package object as sole payment.
    // All entry points return GasObjectNotOwnedObject { owner: Immutable }: even
    // dev-inspect paths that pass gas_ownership_checks=false still hit the ownership
    // check, because check_gas_balance in gas_v2.rs calls check_gas_objects
    // unconditionally.
    let package_ref = env
        .validator
        .get_object(&SUI_FRAMEWORK_PACKAGE_ID)
        .await
        .unwrap()
        .compute_object_reference();
    {
        let mut data = build_transfer(&env, TEST_GAS_BUDGET, env.rgp);
        data.gas_data_mut().payment = vec![package_ref];
        let results = run_all_entry_points(&env, data).await;
        assert_all_equal(&results);
    }

    // Package in multi-coin payment
    {
        let mut data = build_transfer(&env, TEST_GAS_BUDGET, env.rgp);
        data.gas_data_mut().payment = vec![env.gas_coin_refs[0], package_ref, env.gas_coin_refs[1]];
        let results = run_all_entry_points(&env, data).await;
        assert_all_equal(&results);
    }

    // Input object (being transferred) as sole gas payment
    {
        let mut data = build_transfer(&env, TEST_GAS_BUDGET, env.rgp);
        data.gas_data_mut().payment = vec![env.object_ref];
        let results = run_all_entry_points(&env, data).await;
        assert_all_equal(&results);
    }

    // Input object in multi-coin payment at position > 0
    {
        let mut data = build_transfer(&env, TEST_GAS_BUDGET, env.rgp);
        data.gas_data_mut().payment =
            vec![env.gas_coin_refs[0], env.object_ref, env.gas_coin_refs[1]];
        let results = run_all_entry_points(&env, data).await;
        assert_all_equal(&results);
    }
}

#[tokio::test]
async fn test_gas_coin_smash_with_pure_arg() {
    let env = setup_test_env().await;

    let smashed_coin_id = env.gas_coin_refs[1].0;
    let coin_bytes = bcs::to_bytes(&GasCoin::new(smashed_coin_id, GAS_VALUE_FOR_TESTING)).unwrap();

    let recipient = SuiAddress::random_for_testing_only();
    let mut builder = ProgrammableTransactionBuilder::new();
    let coin_arg = builder.input(CallArg::Pure(coin_bytes)).unwrap();
    let recipient_arg = builder.pure(recipient).unwrap();
    builder.command(Command::move_call(
        SUI_FRAMEWORK_PACKAGE_ID,
        Identifier::new("transfer").unwrap(),
        Identifier::new("public_transfer").unwrap(),
        vec![TypeTag::Struct(Box::new(GasCoin::type_()))],
        vec![coin_arg, recipient_arg],
    ));
    let pt = builder.finish();

    let payment = vec![
        env.gas_object_ref,
        env.gas_coin_refs[1],
        env.gas_coin_refs[2],
    ];

    for &entry_point in DEV_INSPECT_ENTRY_POINTS {
        let result = match entry_point {
            NodeEntryPoint::DevInspectSkipChecks => {
                let kind = TransactionKind::programmable(pt.clone());
                env.fullnode
                    .dev_inspect_transaction_block(
                        env.sender,
                        kind,
                        Some(env.rgp),
                        Some(TEST_GAS_BUDGET),
                        None,
                        Some(payment.clone()),
                        None,
                        Some(true),
                    )
                    .await
                    .map(|r| format!("{:?}", r.effects.status()))
                    .map_err(|e| format!("{e:?}"))
            }
            NodeEntryPoint::SimulateSkipChecks | NodeEntryPoint::SimulateSkipChecksMockGas => {
                let mock_gas = entry_point == NodeEntryPoint::SimulateSkipChecksMockGas;
                let mut data = TransactionData::new(
                    TransactionKind::programmable(pt.clone()),
                    env.sender,
                    payment[0],
                    TEST_GAS_BUDGET,
                    env.rgp,
                );
                data.gas_data_mut().payment = payment.clone();
                env.fullnode
                    .simulate_transaction(data, TransactionChecks::Disabled, mock_gas)
                    .map(|r| format!("{:?}", r.effects.status()))
                    .map_err(|e| format!("{e:?}"))
            }
            _ => unreachable!(),
        };
        assert_eq!(
            result.as_deref(),
            Ok("Success"),
            "'{entry_point}': expected Ok(\"Success\"), got {result:?}"
        );
    }
}
