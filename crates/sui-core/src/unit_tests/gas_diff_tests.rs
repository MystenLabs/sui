// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Differential tests for gas charging behavior.
//!
//! Each test drives the `move_random` package (`loopy` / `storage_heavy` /
//! `always_abort`) through a specific error path and snapshots the resulting
//! `GasCostSummary` + `ExecutionStatus` + effects shape in a deterministic
//! text form. The goal is to compare behavior between `main` and this branch
//! (dario/gas_charger_squashed) by:
//!
//!   1. Running these tests on `main`, accepting the insta baseline.
//!   2. Running them on this branch — insta flags any snapshot that drifts.
//!
//! Object IDs and package addresses are intentionally excluded from the
//! snapshot strings because they are non-deterministic across runs.

use super::*;

use super::authority_tests::submit_and_execute;
use super::gas_tests::{make_gas_coins, publish_move_random_package, touch_gas_coins};
use crate::authority::test_authority_builder::TestAuthorityBuilder;
use insta::assert_snapshot;
use move_core_types::ident_str;
use sui_protocol_config::ProtocolConfig;
use sui_types::base_types::{ObjectID, SuiAddress};
use sui_types::crypto::{AccountKeyPair, get_key_pair};
use sui_types::execution_status::{ExecutionErrorKind, ExecutionStatus};
use sui_types::gas_coin::GasCoin;
use sui_types::transaction::{CallArg, TransactionData};
use sui_types::utils::to_sender_signed_transaction;

/// Run one transaction against `move_random::<entry>` and return a
/// deterministic, snapshot-friendly description of its outcome.
///
/// Includes only values that do not depend on random IDs or addresses:
///   - status classification (Success / Failure(<kind label>) with command index)
///   - full GasCostSummary (u64 fields)
///   - gas_object balance delta (final - initial)
///   - counts of mutated / created / deleted entries in the effects
///
/// Deliberately excludes Debug renderings of MoveAbort, object references,
/// package IDs, or anything else that varies run-to-run.
async fn run_transaction_for_diff(
    entry: &'static str,
    args: Vec<CallArg>,
    budget: u64,
    gas_price: u64,
    coin_num: u64,
) -> String {
    let (sender, sender_key): (SuiAddress, AccountKeyPair) = get_key_pair();

    // Ensure we have enough to cover `budget` in each coin setup.
    let gas_amount: u64 = if budget < 10_000_000_000 {
        10_000_000_000
    } else {
        budget
    };

    let gas_coins = make_gas_coins(sender, gas_amount, coin_num);
    let gas_coin_ids: Vec<_> = gas_coins.iter().map(|obj| obj.id()).collect();
    let authority_state = TestAuthorityBuilder::new().build().await;
    for obj in gas_coins {
        authority_state.insert_genesis_object(obj).await;
    }

    // Separate gas coin used only for publishing the test package.
    let publish_gas_id = ObjectID::random();
    let publish_gas = sui_types::object::Object::with_id_owner_gas_for_testing(
        publish_gas_id,
        sender,
        gas_amount,
    );
    authority_state.insert_genesis_object(publish_gas).await;

    // Touch the gas coins so storage_rebate is non-zero (matches check_oog_transaction).
    touch_gas_coins(
        &authority_state,
        sender,
        &sender_key,
        sender,
        &gas_coin_ids,
        publish_gas_id,
    )
    .await;

    let package =
        publish_move_random_package(&authority_state, &sender, &sender_key, &publish_gas_id).await;

    // Re-read the gas coins after `touch_gas_coins` so we use fresh versions.
    let mut gas_coin_refs = vec![];
    for coin_id in &gas_coin_ids {
        let coin_ref = authority_state
            .get_object(coin_id)
            .await
            .unwrap()
            .compute_object_reference();
        gas_coin_refs.push(coin_ref);
    }

    let module = ident_str!("move_random").to_owned();
    let function = ident_str!(entry).to_owned();
    let data = TransactionData::new_move_call_with_gas_coins(
        sender,
        package,
        module,
        function,
        vec![],
        gas_coin_refs,
        args,
        budget,
        gas_price,
    )
    .unwrap();
    let tx = to_sender_signed_transaction(data, &sender_key);

    // Capture the primary gas coin's pre-execution balance. `submit_and_execute`
    // consumes the transaction; we read from the store here.
    let primary_gas_before =
        GasCoin::try_from(&authority_state.get_object(&gas_coin_ids[0]).await.unwrap())
            .unwrap()
            .value();

    let effects = submit_and_execute(&authority_state, tx)
        .await
        .unwrap()
        .1
        .into_data();

    let status_str = render_status(effects.status());
    let summary = effects.gas_cost_summary();

    // The gas object in effects is the smashed primary coin, which may have
    // been reduced in value but is still mutated (not deleted).
    let primary_gas_after = match effects.gas_object() {
        Some((oref, _)) => authority_state
            .get_object(&oref.0)
            .await
            .and_then(|obj| GasCoin::try_from(&obj).ok().map(|c| c.value())),
        None => None,
    };
    let balance_delta: i64 = match primary_gas_after {
        Some(after) => after as i64 - primary_gas_before as i64,
        None => -(primary_gas_before as i64),
    };

    format!(
        "status: {status}\n\
         computation_cost: {cc}\n\
         storage_cost: {sc}\n\
         storage_rebate: {sr}\n\
         non_refundable_storage_fee: {nrsf}\n\
         gas_object_balance_change: {delta}\n\
         mutated_count: {m}\n\
         created_count: {c}\n\
         deleted_count: {d}\n",
        status = status_str,
        cc = summary.computation_cost,
        sc = summary.storage_cost,
        sr = summary.storage_rebate,
        nrsf = summary.non_refundable_storage_fee,
        delta = balance_delta,
        m = effects.mutated().len(),
        c = effects.created().len(),
        d = effects.deleted().len(),
    )
}

/// Classify `ExecutionStatus` into a short, deterministic label.
fn render_status(status: &ExecutionStatus) -> String {
    match status {
        ExecutionStatus::Success => "Success".to_string(),
        ExecutionStatus::Failure(f) => {
            let kind_label = match &f.error {
                ExecutionErrorKind::InsufficientGas => "InsufficientGas".to_string(),
                ExecutionErrorKind::MoveAbort(_, code) => format!("MoveAbort(code={code})"),
                ExecutionErrorKind::EffectsTooLarge { .. } => "EffectsTooLarge".to_string(),
                other => format!("{other:?}")
                    .split('{')
                    .next()
                    .unwrap_or("")
                    .trim()
                    .to_string(),
            };
            match f.command {
                Some(idx) => format!("Failure({kind_label}, command={idx})"),
                None => format!("Failure({kind_label})"),
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Happy path (should be identical across branches).
// ---------------------------------------------------------------------------

#[tokio::test]
async fn diff_happy_success_single_coin() {
    let out = run_transaction_for_diff(
        "storage_heavy",
        vec![
            CallArg::Pure(bcs::to_bytes(&5_u64).unwrap()),
            CallArg::Pure(bcs::to_bytes(&SuiAddress::ZERO).unwrap()),
        ],
        5_000_000,
        1000,
        1,
    )
    .await;
    assert_snapshot!(out);
}

#[tokio::test]
async fn diff_happy_success_multi_coin() {
    let out = run_transaction_for_diff(
        "storage_heavy",
        vec![
            CallArg::Pure(bcs::to_bytes(&5_u64).unwrap()),
            CallArg::Pure(bcs::to_bytes(&SuiAddress::ZERO).unwrap()),
        ],
        5_000_000,
        1000,
        5,
    )
    .await;
    assert_snapshot!(out);
}

// ---------------------------------------------------------------------------
// Computation-OOG paths (via `loopy`).
// ---------------------------------------------------------------------------

#[tokio::test]
async fn diff_oog_computation_storage_ok_single() {
    let budget = ProtocolConfig::get_for_max_version_UNSAFE().max_tx_gas();
    let out = run_transaction_for_diff("loopy", vec![], budget, 1000, 1).await;
    assert_snapshot!(out);
}

#[tokio::test]
async fn diff_oog_computation_storage_ok_multi() {
    let budget = ProtocolConfig::get_for_max_version_UNSAFE().max_tx_gas();
    let out = run_transaction_for_diff("loopy", vec![], budget, 1000, 5).await;
    assert_snapshot!(out);
}

#[tokio::test]
async fn diff_oog_computation_minimal_storage_oog() {
    // Budget = 5M units * 1000 rgp. Entire budget is consumed by computation so
    // even input-only storage can't be paid — hits the full-budget fallback.
    let out = run_transaction_for_diff("loopy", vec![], 5_000_000 * 1000, 1000, 1).await;
    assert_snapshot!(out);
}

// ---------------------------------------------------------------------------
// Storage-OOG paths (via `storage_heavy`).
// ---------------------------------------------------------------------------

#[tokio::test]
async fn diff_storage_oog_minimal_ok_single() {
    let out = run_transaction_for_diff(
        "storage_heavy",
        vec![
            CallArg::Pure(bcs::to_bytes(&100_u64).unwrap()),
            // Use ZERO so the address doesn't depend on the random keypair.
            CallArg::Pure(bcs::to_bytes(&SuiAddress::ZERO).unwrap()),
        ],
        1_100_000,
        1001,
        1,
    )
    .await;
    assert_snapshot!(out);
}

#[tokio::test]
async fn diff_storage_oog_minimal_ok_multi() {
    let out = run_transaction_for_diff(
        "storage_heavy",
        vec![
            CallArg::Pure(bcs::to_bytes(&100_u64).unwrap()),
            CallArg::Pure(bcs::to_bytes(&SuiAddress::ZERO).unwrap()),
        ],
        1_100_000,
        1001,
        5,
    )
    .await;
    assert_snapshot!(out);
}

#[tokio::test]
async fn diff_storage_oog_minimal_oog() {
    let out = run_transaction_for_diff(
        "storage_heavy",
        vec![
            CallArg::Pure(bcs::to_bytes(&100_u64).unwrap()),
            CallArg::Pure(bcs::to_bytes(&SuiAddress::ZERO).unwrap()),
        ],
        1_002_000,
        1001,
        1,
    )
    .await;
    assert_snapshot!(out);
}

// ---------------------------------------------------------------------------
// Move-abort paths (via `always_abort`).
// ---------------------------------------------------------------------------

#[tokio::test]
async fn diff_move_abort_generous_budget() {
    let out = run_transaction_for_diff("always_abort", vec![], 10_000_000_000, 1000, 1).await;
    assert_snapshot!(out);
}

#[tokio::test]
async fn diff_move_abort_tight_budget() {
    let out = run_transaction_for_diff("always_abort", vec![], 2_000_000, 1000, 1).await;
    assert_snapshot!(out);
}

#[tokio::test]
async fn diff_move_abort_multi_coin() {
    let out = run_transaction_for_diff("always_abort", vec![], 10_000_000_000, 1000, 5).await;
    assert_snapshot!(out);
}
