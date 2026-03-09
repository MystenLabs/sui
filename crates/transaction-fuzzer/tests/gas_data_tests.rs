// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use proptest::arbitrary::*;
use proptest::test_runner::TestCaseError;
use sui_types::base_types::{ObjectID, ObjectRef, SuiAddress, dbg_addr};
use sui_types::crypto::{AccountKeyPair, KeypairTraits, get_key_pair};
use sui_types::digests::TransactionDigest;
use sui_types::error::SuiError;
use sui_types::object::{MoveObject, OBJECT_START_VERSION, Object, Owner};
use sui_types::programmable_transaction_builder::ProgrammableTransactionBuilder;
use sui_types::transaction::{GasData, TransactionData, TransactionKind};
use sui_types::utils::to_sender_signed_transaction;
use tracing::debug;
use transaction_fuzzer::GasDataGenConfig;
use transaction_fuzzer::GasDataWithObjects;
use transaction_fuzzer::executor::Executor;
use transaction_fuzzer::{run_proptest, run_proptest_with_fullnode};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn make_transfer_sui_pt() -> TransactionKind {
    let mut builder = ProgrammableTransactionBuilder::new();
    builder.transfer_sui(dbg_addr(2), None);
    TransactionKind::ProgrammableTransaction(builder.finish())
}

fn make_gas_coin(sender: SuiAddress, balance: u64) -> Object {
    Object::new_move(
        MoveObject::new_gas_coin(OBJECT_START_VERSION, ObjectID::random(), balance),
        Owner::AddressOwner(sender),
        TransactionDigest::genesis_marker(),
    )
}

fn make_gas_data(sender: SuiAddress, objects: &[Object], price: u64, budget: u64) -> GasData {
    GasData {
        payment: objects
            .iter()
            .map(|o| o.compute_object_reference())
            .collect(),
        owner: sender,
        price,
        budget,
    }
}

/// Results of running a gas scenario through all 4 execution modes.
struct AllModeResults {
    normal: Result<(), SuiError>,
    dry_run: Result<(), SuiError>,
    dev_inspect_skip: Result<(), SuiError>,
    dev_inspect_no_skip: Result<(), SuiError>,
}

/// Run a gas scenario through normal execution, dry-run, dev-inspect(skip=true),
/// and dev-inspect(skip=false). Returns results from each mode.
fn run_all_modes(
    executor: &mut Executor,
    sender: SuiAddress,
    sender_key: &AccountKeyPair,
    gas_data: GasData,
    objects: &[Object],
) -> AllModeResults {
    executor.add_objects(objects);

    let kind = make_transfer_sui_pt();
    let gas_refs: Vec<ObjectRef> = gas_data.payment.clone();
    let price = gas_data.price;
    let budget = gas_data.budget;

    // Normal execution (uses validator)
    let tx_data = TransactionData::new_with_gas_data(kind.clone(), sender, gas_data.clone());
    let tx = to_sender_signed_transaction(tx_data, sender_key);
    let normal = executor.execute_transaction(tx).map(|_| ());

    // For dry-run and dev-inspect, we need a separate fullnode executor since the
    // validator executor rejects these calls. Create one and insert the same objects.
    let mut fullnode = Executor::new_fullnode();
    fullnode.add_objects(objects);

    // Dry-run
    let tx_data = TransactionData::new_with_gas_data(kind.clone(), sender, gas_data);
    let dry_run = fullnode.dry_run_transaction(tx_data);

    // Dev-inspect with skip_checks=true (default)
    let dev_inspect_skip = fullnode.dev_inspect_transaction(
        sender,
        kind.clone(),
        Some(price),
        Some(budget),
        Some(sender),
        Some(gas_refs.clone()),
        Some(true),
    );

    // Dev-inspect with skip_checks=false
    let dev_inspect_no_skip = fullnode.dev_inspect_transaction(
        sender,
        kind,
        Some(price),
        Some(budget),
        Some(sender),
        Some(gas_refs),
        Some(false),
    );

    AllModeResults {
        normal,
        dry_run,
        dev_inspect_skip,
        dev_inspect_no_skip,
    }
}

fn assert_err_contains(result: &Result<(), SuiError>, expected_substring: &str, mode: &str) {
    let err = result
        .as_ref()
        .expect_err(&format!("{mode}: expected error, got Ok"));
    let err_str = format!("{err:?}");
    assert!(
        err_str.contains(expected_substring),
        "{mode}: expected error containing '{expected_substring}', got: {err_str}"
    );
}

// ---------------------------------------------------------------------------
// Proptest fuzz tests (original + extended to dry-run & dev-inspect)
// ---------------------------------------------------------------------------

/// Send transfer sui txn with provided random gas data and gas objects to an authority.
fn test_with_random_gas_data(
    gas_data_test: GasDataWithObjects,
    executor: &mut Executor,
) -> Result<(), TestCaseError> {
    let gas_data = gas_data_test.gas_data;
    let objects = gas_data_test.objects;
    let sender = gas_data_test.sender_key.public().into();

    // Insert the random gas objects into genesis.
    executor.add_objects(&objects);
    let pt = {
        let mut builder = ProgrammableTransactionBuilder::new();
        let recipient = dbg_addr(2);
        builder.transfer_sui(recipient, None);
        builder.finish()
    };
    let kind = TransactionKind::ProgrammableTransaction(pt);
    let tx_data = TransactionData::new_with_gas_data(kind, sender, gas_data);
    let tx = to_sender_signed_transaction(tx_data, &gas_data_test.sender_key);

    let result = executor.execute_transaction(tx);
    debug!("result: {:?}", result);
    Ok(())
}

fn test_with_random_gas_data_dry_run(
    gas_data_test: GasDataWithObjects,
    executor: &mut Executor,
) -> Result<(), TestCaseError> {
    let gas_data = gas_data_test.gas_data;
    let objects = gas_data_test.objects;
    let sender = gas_data_test.sender_key.public().into();

    executor.add_objects(&objects);
    let kind = make_transfer_sui_pt();
    let tx_data = TransactionData::new_with_gas_data(kind, sender, gas_data);

    let result = executor.dry_run_transaction(tx_data);
    debug!("dry_run result: {:?}", result);
    Ok(())
}

fn test_with_random_gas_data_dev_inspect(
    gas_data_test: GasDataWithObjects,
    executor: &mut Executor,
    skip_checks: bool,
) -> Result<(), TestCaseError> {
    let gas_data = gas_data_test.gas_data;
    let objects = gas_data_test.objects;
    let sender: SuiAddress = gas_data_test.sender_key.public().into();

    executor.add_objects(&objects);
    let kind = make_transfer_sui_pt();
    let gas_refs: Vec<ObjectRef> = gas_data.payment.clone();

    let result = executor.dev_inspect_transaction(
        sender,
        kind,
        Some(gas_data.price),
        Some(gas_data.budget),
        Some(gas_data.owner),
        Some(gas_refs),
        Some(skip_checks),
    );
    debug!("dev_inspect (skip={skip_checks}) result: {:?}", result);
    Ok(())
}

#[test]
#[cfg_attr(msim, ignore)]
fn test_gas_data_owned_or_immut() {
    let strategy = any_with::<GasDataWithObjects>(GasDataGenConfig::owned_by_sender_or_immut());
    run_proptest(1000, strategy, |gas_data_test, mut executor| {
        test_with_random_gas_data(gas_data_test, &mut executor)
    });
}

#[test]
#[cfg_attr(msim, ignore)]
fn test_gas_data_any_owner() {
    let strategy = any_with::<GasDataWithObjects>(GasDataGenConfig::any_owner());
    run_proptest(1000, strategy, |gas_data_test, mut executor| {
        test_with_random_gas_data(gas_data_test, &mut executor)
    });
}

#[test]
#[cfg_attr(msim, ignore)]
fn test_gas_data_dry_run_fuzz() {
    let strategy = any_with::<GasDataWithObjects>(GasDataGenConfig::owned_by_sender_or_immut());
    run_proptest_with_fullnode(1000, strategy, |gas_data_test, mut executor| {
        test_with_random_gas_data_dry_run(gas_data_test, &mut executor)
    });
}

#[test]
#[cfg_attr(msim, ignore)]
fn test_gas_data_dev_inspect_skip_checks_fuzz() {
    // Must use sender_owned_only() because skip_checks=true bypasses ownership
    // validation, and non-sender-owned gas objects (immutable, shared) would panic
    // during execution when the system tries to modify them.
    let strategy = any_with::<GasDataWithObjects>(GasDataGenConfig::sender_owned_only());
    run_proptest_with_fullnode(1000, strategy, |gas_data_test, mut executor| {
        test_with_random_gas_data_dev_inspect(gas_data_test, &mut executor, true)
    });
}

#[test]
#[cfg_attr(msim, ignore)]
fn test_gas_data_dev_inspect_no_skip_fuzz() {
    let strategy = any_with::<GasDataWithObjects>(GasDataGenConfig::owned_by_sender_or_immut());
    run_proptest_with_fullnode(1000, strategy, |gas_data_test, mut executor| {
        test_with_random_gas_data_dev_inspect(gas_data_test, &mut executor, false)
    });
}

// ---------------------------------------------------------------------------
// Deterministic edge case tests
// ---------------------------------------------------------------------------

/// Valid gas data succeeds in all execution modes.
#[test]
#[cfg_attr(msim, ignore)]
fn test_valid_gas_data_all_modes() {
    let (sender, sender_key): (SuiAddress, AccountKeyPair) = get_key_pair();
    let mut executor = Executor::new();
    let rgp = executor.get_reference_gas_price();
    let budget = rgp * 200_000; // well above min_transaction_cost
    let gas_obj = make_gas_coin(sender, budget);
    let gas_data = make_gas_data(sender, std::slice::from_ref(&gas_obj), rgp, budget);

    let results = run_all_modes(&mut executor, sender, &sender_key, gas_data, &[gas_obj]);

    assert!(results.normal.is_ok(), "normal: {:?}", results.normal);
    assert!(results.dry_run.is_ok(), "dry_run: {:?}", results.dry_run);
    assert!(
        results.dev_inspect_skip.is_ok(),
        "dev_inspect_skip: {:?}",
        results.dev_inspect_skip
    );
    assert!(
        results.dev_inspect_no_skip.is_ok(),
        "dev_inspect_no_skip: {:?}",
        results.dev_inspect_no_skip
    );
}

/// Gas price = 0 (below RGP) fails in all modes.
#[test]
#[cfg_attr(msim, ignore)]
fn test_gas_price_zero() {
    let (sender, sender_key): (SuiAddress, AccountKeyPair) = get_key_pair();
    let mut executor = Executor::new();
    let budget = 100_000_000;
    let gas_obj = make_gas_coin(sender, budget);
    let gas_data = make_gas_data(sender, std::slice::from_ref(&gas_obj), 0, budget);

    let results = run_all_modes(&mut executor, sender, &sender_key, gas_data, &[gas_obj]);

    assert_err_contains(&results.normal, "GasPriceUnderRGP", "normal");
    assert_err_contains(&results.dry_run, "GasPriceUnderRGP", "dry_run");
    assert_err_contains(
        &results.dev_inspect_skip,
        "GasPriceUnderRGP",
        "dev_inspect_skip",
    );
    assert_err_contains(
        &results.dev_inspect_no_skip,
        "GasPriceUnderRGP",
        "dev_inspect_no_skip",
    );
}

/// Gas price just below RGP fails in all modes.
#[test]
#[cfg_attr(msim, ignore)]
fn test_gas_price_below_rgp() {
    let (sender, sender_key): (SuiAddress, AccountKeyPair) = get_key_pair();
    let mut executor = Executor::new();
    let rgp = executor.get_reference_gas_price();
    let budget = 100_000_000;
    let gas_obj = make_gas_coin(sender, budget);
    let gas_data = make_gas_data(sender, std::slice::from_ref(&gas_obj), rgp - 1, budget);

    let results = run_all_modes(&mut executor, sender, &sender_key, gas_data, &[gas_obj]);

    assert_err_contains(&results.normal, "GasPriceUnderRGP", "normal");
    assert_err_contains(&results.dry_run, "GasPriceUnderRGP", "dry_run");
    assert_err_contains(
        &results.dev_inspect_skip,
        "GasPriceUnderRGP",
        "dev_inspect_skip",
    );
    assert_err_contains(
        &results.dev_inspect_no_skip,
        "GasPriceUnderRGP",
        "dev_inspect_no_skip",
    );
}

/// Gas budget of zero (below min_transaction_cost) fails in all modes.
#[test]
#[cfg_attr(msim, ignore)]
fn test_gas_budget_zero() {
    let (sender, sender_key): (SuiAddress, AccountKeyPair) = get_key_pair();
    let mut executor = Executor::new();
    let rgp = executor.get_reference_gas_price();
    let gas_obj = make_gas_coin(sender, 1_000_000_000);
    let gas_data = make_gas_data(sender, std::slice::from_ref(&gas_obj), rgp, 0);

    let results = run_all_modes(&mut executor, sender, &sender_key, gas_data, &[gas_obj]);

    assert_err_contains(&results.normal, "GasBudgetTooLow", "normal");
    assert_err_contains(&results.dry_run, "GasBudgetTooLow", "dry_run");
    assert_err_contains(
        &results.dev_inspect_skip,
        "GasBudgetTooLow",
        "dev_inspect_skip",
    );
    assert_err_contains(
        &results.dev_inspect_no_skip,
        "GasBudgetTooLow",
        "dev_inspect_no_skip",
    );
}

/// Gas budget exceeding max_tx_gas fails in all modes.
#[test]
#[cfg_attr(msim, ignore)]
fn test_gas_budget_exceeds_max() {
    let (sender, sender_key): (SuiAddress, AccountKeyPair) = get_key_pair();
    let mut executor = Executor::new();
    let rgp = executor.get_reference_gas_price();
    let max_budget = sui_protocol_config::ProtocolConfig::get_for_max_version_UNSAFE().max_tx_gas();
    let over_budget = max_budget + 1;
    let gas_obj = make_gas_coin(sender, over_budget);
    let gas_data = make_gas_data(sender, std::slice::from_ref(&gas_obj), rgp, over_budget);

    let results = run_all_modes(&mut executor, sender, &sender_key, gas_data, &[gas_obj]);

    assert_err_contains(&results.normal, "GasBudgetTooHigh", "normal");
    assert_err_contains(&results.dry_run, "GasBudgetTooHigh", "dry_run");
    assert_err_contains(
        &results.dev_inspect_skip,
        "GasBudgetTooHigh",
        "dev_inspect_skip",
    );
    assert_err_contains(
        &results.dev_inspect_no_skip,
        "GasBudgetTooHigh",
        "dev_inspect_no_skip",
    );
}

/// Gas coin balance < budget fails in all modes.
#[test]
#[cfg_attr(msim, ignore)]
fn test_gas_balance_insufficient() {
    let (sender, sender_key): (SuiAddress, AccountKeyPair) = get_key_pair();
    let mut executor = Executor::new();
    let rgp = executor.get_reference_gas_price();
    let budget = rgp * 200_000;
    // Gas coin has only 1 MIST -- far below budget
    let gas_obj = make_gas_coin(sender, 1);
    let gas_data = make_gas_data(sender, std::slice::from_ref(&gas_obj), rgp, budget);

    let results = run_all_modes(&mut executor, sender, &sender_key, gas_data, &[gas_obj]);

    assert_err_contains(&results.normal, "GasBalanceTooLow", "normal");
    assert_err_contains(&results.dry_run, "GasBalanceTooLow", "dry_run");
    assert_err_contains(
        &results.dev_inspect_skip,
        "GasBalanceTooLow",
        "dev_inspect_skip",
    );
    assert_err_contains(
        &results.dev_inspect_no_skip,
        "GasBalanceTooLow",
        "dev_inspect_no_skip",
    );
}
