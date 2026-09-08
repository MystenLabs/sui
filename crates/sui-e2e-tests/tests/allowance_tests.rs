// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! E2e allowance flows: issuance and spends, plus mid-flight staleness races.
//! Covers sign-time checks, adapter value creation, Move policy, and settlement.

use std::path::PathBuf;

use move_core_types::{identifier::Identifier, u256::U256};
use sui_macros::*;
use sui_simulator::has_mainnet_protocol_config_override;
use sui_types::{
    MOVE_STDLIB_PACKAGE_ID, SUI_CLOCK_OBJECT_ID, SUI_CLOCK_OBJECT_SHARED_VERSION,
    SUI_FRAMEWORK_PACKAGE_ID,
    base_types::{ObjectID, ObjectRef, SequenceNumber, SuiAddress},
    effects::TransactionEffectsAPI,
    execution_status::{ExecutionFailure, ExecutionFailureStatus, ExecutionStatus},
    gas_coin::GAS,
    object::Owner,
    programmable_transaction_builder::ProgrammableTransactionBuilder,
    transaction::{FundsWithdrawalArg, ObjectArg, SharedObjectMutability, TransactionData},
};
use test_cluster::addr_balance_test_env::{TestEnv, TestEnvBuilder};

const FUND: u64 = 5_000_000;
const SPEND: u64 = 1_000_000;

fn balance_sui_type() -> sui_types::TypeTag {
    "0x2::balance::Balance<0x2::sui::SUI>".parse().unwrap()
}

fn rate_limit_type() -> sui_types::TypeTag {
    "0x2::allowance::RateLimit".parse().unwrap()
}

fn allowance_test_code_path() -> PathBuf {
    let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    path.push("tests/allowance_test_code");
    path
}

/// Issues a plain allowance of `cap` from `funder` to `spender`.
/// Returns the shared allowance and the funder's refreshed gas.
async fn issue_allowance(
    test_env: &mut TestEnv,
    funder: SuiAddress,
    spender: SuiAddress,
    cap: u64,
) -> (ObjectID, SequenceNumber, ObjectRef) {
    let funder_gas = test_env.get_gas_for_sender(funder)[0];
    let mut builder = ProgrammableTransactionBuilder::new();
    let no_rate_limit = builder.programmable_move_call(
        MOVE_STDLIB_PACKAGE_ID,
        Identifier::new("option").unwrap(),
        Identifier::new("none").unwrap(),
        vec![rate_limit_type()],
        vec![],
    );
    let args = vec![
        builder.pure("".to_string()).unwrap(),
        builder.pure(spender).unwrap(),
        builder.pure(Some(U256::from(cap))).unwrap(),
        builder.pure(None::<u64>).unwrap(),
        builder.pure(Some(u64::MAX)).unwrap(),
        no_rate_limit,
    ];
    builder.programmable_move_call(
        SUI_FRAMEWORK_PACKAGE_ID,
        Identifier::new("allowance").unwrap(),
        Identifier::new("new").unwrap(),
        vec![balance_sui_type()],
        args,
    );
    let tx = TransactionData::new_programmable(
        funder,
        vec![funder_gas],
        builder.finish(),
        10_000_000,
        test_env.rgp,
    );
    let (_, effects) = test_env.exec_tx_directly(tx).await.unwrap();
    assert!(
        effects.status().is_ok(),
        "issuance failed: {:?}",
        effects.status()
    );
    let (allowance_ref, allowance_owner) = effects
        .created()
        .iter()
        .find(|(_, owner)| matches!(owner, Owner::Shared { .. }))
        .cloned()
        .expect("the allowance is created as a shared object");
    let Owner::Shared {
        initial_shared_version,
    } = allowance_owner
    else {
        unreachable!()
    };
    let funder_gas = effects.gas_object().expect("issuance paid gas").0;
    (allowance_ref.0, initial_shared_version, funder_gas)
}

/// The standard spend: redeem `amount` through the allowance, keep the coin.
fn build_spend_tx(
    rgp: u64,
    spender: SuiAddress,
    spender_gas: ObjectRef,
    funder: SuiAddress,
    allowance_id: ObjectID,
    initial_shared_version: SequenceNumber,
    amount: u64,
) -> TransactionData {
    let mut builder = ProgrammableTransactionBuilder::new();
    let allowance_arg = builder
        .obj(ObjectArg::SharedObject {
            id: allowance_id,
            initial_shared_version,
            mutability: SharedObjectMutability::Mutable,
        })
        .unwrap();
    let withdraw_arg = builder
        .funds_withdrawal(FundsWithdrawalArg::balance_from_allowance(
            amount,
            GAS::type_tag(),
            funder,
            allowance_id,
        ))
        .unwrap();
    let clock_arg = builder
        .obj(ObjectArg::SharedObject {
            id: SUI_CLOCK_OBJECT_ID,
            initial_shared_version: SUI_CLOCK_OBJECT_SHARED_VERSION,
            mutability: SharedObjectMutability::Immutable,
        })
        .unwrap();
    let spent_balance = builder.programmable_move_call(
        SUI_FRAMEWORK_PACKAGE_ID,
        Identifier::new("allowance").unwrap(),
        Identifier::new("balance_spend").unwrap(),
        vec!["0x2::sui::SUI".parse().unwrap()],
        vec![allowance_arg, withdraw_arg, clock_arg],
    );
    let coin = builder.programmable_move_call(
        SUI_FRAMEWORK_PACKAGE_ID,
        Identifier::new("coin").unwrap(),
        Identifier::new("from_balance").unwrap(),
        vec!["0x2::sui::SUI".parse().unwrap()],
        vec![spent_balance],
    );
    builder.transfer_arg(spender, coin);
    TransactionData::new_programmable(
        spender,
        vec![spender_gas],
        builder.finish(),
        10_000_000,
        rgp,
    )
}

#[sim_test]
async fn test_allowance_issue_and_spend() {
    if has_mainnet_protocol_config_override() {
        return;
    }
    // Genesis at the max protocol version (unsnapshotted) deploys the source-built
    // framework, which includes `sui::allowance`, and enables the flag.
    let mut test_env = TestEnvBuilder::new().build().await;

    let funder = test_env.get_sender(0);
    let (spender, spender_gas) = test_env.get_sender_and_gas(1);

    // The balance an allowance draws from lives in the accumulator, not a coin.
    test_env.fund_one_address_balance(funder, FUND).await;
    test_env.verify_accumulator_exists(funder, FUND);

    let (_, funder_gas) = test_env.get_sender_and_gas(0);
    let mut builder = ProgrammableTransactionBuilder::new();
    let no_rate_limit = builder.programmable_move_call(
        MOVE_STDLIB_PACKAGE_ID,
        Identifier::new("option").unwrap(),
        Identifier::new("none").unwrap(),
        vec![rate_limit_type()],
        vec![],
    );
    let args = vec![
        builder.pure("".to_string()).unwrap(), // name
        builder.pure(spender).unwrap(),
        builder.pure(Some(U256::from(SPEND))).unwrap(), // lifetime_cap
        builder.pure(None::<u64>).unwrap(),             // start_timestamp_ms
        builder.pure(Some(u64::MAX)).unwrap(),          // expiration_timestamp_ms
        no_rate_limit,
    ];
    builder.programmable_move_call(
        SUI_FRAMEWORK_PACKAGE_ID,
        Identifier::new("allowance").unwrap(),
        Identifier::new("new").unwrap(),
        vec![balance_sui_type()],
        args,
    );
    let tx = TransactionData::new_programmable(
        funder,
        vec![funder_gas],
        builder.finish(),
        10_000_000,
        test_env.rgp,
    );
    let (_, effects) = test_env.exec_tx_directly(tx).await.unwrap();
    assert!(
        effects.status().is_ok(),
        "issuing the allowance failed: {:?}",
        effects.status()
    );
    let (allowance_ref, allowance_owner) = effects
        .created()
        .iter()
        .find(|(_, owner)| matches!(owner, Owner::Shared { .. }))
        .cloned()
        .expect("the allowance is created as a shared object");
    let Owner::Shared {
        initial_shared_version,
    } = allowance_owner
    else {
        unreachable!()
    };
    let allowance_id = allowance_ref.0;

    // Spender consumes it: the input declares (funder, allowance), the adapter
    // creates the withdrawal, and `balance_spend` enforces policy and redeems.
    let mut builder = ProgrammableTransactionBuilder::new();
    let allowance_arg = builder
        .obj(ObjectArg::SharedObject {
            id: allowance_id,
            initial_shared_version,
            mutability: SharedObjectMutability::Mutable,
        })
        .unwrap();
    let withdraw_arg = builder
        .funds_withdrawal(FundsWithdrawalArg::balance_from_allowance(
            SPEND,
            GAS::type_tag(),
            funder,
            allowance_id,
        ))
        .unwrap();
    let clock_arg = builder
        .obj(ObjectArg::SharedObject {
            id: SUI_CLOCK_OBJECT_ID,
            initial_shared_version: SUI_CLOCK_OBJECT_SHARED_VERSION,
            mutability: SharedObjectMutability::Immutable,
        })
        .unwrap();
    let spent_balance = builder.programmable_move_call(
        SUI_FRAMEWORK_PACKAGE_ID,
        Identifier::new("allowance").unwrap(),
        Identifier::new("balance_spend").unwrap(),
        vec!["0x2::sui::SUI".parse().unwrap()],
        vec![allowance_arg, withdraw_arg, clock_arg],
    );
    let coin = builder.programmable_move_call(
        SUI_FRAMEWORK_PACKAGE_ID,
        Identifier::new("coin").unwrap(),
        Identifier::new("from_balance").unwrap(),
        vec!["0x2::sui::SUI".parse().unwrap()],
        vec![spent_balance],
    );
    builder.transfer_arg(spender, coin);
    let tx = TransactionData::new_programmable(
        spender,
        vec![spender_gas],
        builder.finish(),
        10_000_000,
        test_env.rgp,
    );
    let (_, effects) = test_env.exec_tx_directly(tx).await.unwrap();
    assert!(
        effects.status().is_ok(),
        "spending the allowance failed: {:?}",
        effects.status()
    );

    test_env.verify_accumulator_exists(funder, FUND - SPEND);
    effects
        .created()
        .iter()
        .find(|(_, owner)| matches!(owner, Owner::AddressOwner(a) if *a == spender))
        .expect("the spender receives the redeemed coin");

    // Reconfiguration runs the conservation checks over the accumulator.
    test_env.trigger_reconfiguration().await;
}

/// A spend admitted while the allowance is alive, with the revoke sequenced
/// ahead of it: the spend hits a deleted shared object and nothing is debited.
#[sim_test]
async fn test_allowance_revoked_mid_flight() {
    if has_mainnet_protocol_config_override() {
        return;
    }
    let mut test_env = TestEnvBuilder::new().build().await;

    let funder = test_env.get_sender(0);
    let (spender, spender_gas) = test_env.get_sender_and_gas(1);

    test_env.fund_one_address_balance(funder, FUND).await;
    test_env.verify_accumulator_exists(funder, FUND);

    let (_, funder_gas) = test_env.get_sender_and_gas(0);
    let mut builder = ProgrammableTransactionBuilder::new();
    let no_rate_limit = builder.programmable_move_call(
        MOVE_STDLIB_PACKAGE_ID,
        Identifier::new("option").unwrap(),
        Identifier::new("none").unwrap(),
        vec![rate_limit_type()],
        vec![],
    );
    let args = vec![
        builder.pure("mid-flight".to_string()).unwrap(),
        builder.pure(spender).unwrap(),
        builder.pure(Some(U256::from(SPEND))).unwrap(),
        builder.pure(None::<u64>).unwrap(),
        builder.pure(Some(u64::MAX)).unwrap(),
        no_rate_limit,
    ];
    builder.programmable_move_call(
        SUI_FRAMEWORK_PACKAGE_ID,
        Identifier::new("allowance").unwrap(),
        Identifier::new("new").unwrap(),
        vec![balance_sui_type()],
        args,
    );
    let tx = TransactionData::new_programmable(
        funder,
        vec![funder_gas],
        builder.finish(),
        10_000_000,
        test_env.rgp,
    );
    let (_, effects) = test_env.exec_tx_directly(tx).await.unwrap();
    assert!(
        effects.status().is_ok(),
        "issuance failed: {:?}",
        effects.status()
    );
    let (allowance_ref, allowance_owner) = effects
        .created()
        .iter()
        .find(|(_, owner)| matches!(owner, Owner::Shared { .. }))
        .cloned()
        .expect("the allowance is created as a shared object");
    let Owner::Shared {
        initial_shared_version,
    } = allowance_owner
    else {
        unreachable!()
    };
    let allowance_id = allowance_ref.0;
    let (cap_ref, _) = effects
        .created()
        .iter()
        .find(|(_, owner)| matches!(owner, Owner::AddressOwner(a) if *a == funder))
        .cloned()
        .expect("the revocation cap goes to the funder");
    let funder_gas = effects.gas_object().expect("issuance paid gas").0;

    let mut builder = ProgrammableTransactionBuilder::new();
    let allowance_arg = builder
        .obj(ObjectArg::SharedObject {
            id: allowance_id,
            initial_shared_version,
            mutability: SharedObjectMutability::Mutable,
        })
        .unwrap();
    let cap_arg = builder.obj(ObjectArg::ImmOrOwnedObject(cap_ref)).unwrap();
    builder.programmable_move_call(
        SUI_FRAMEWORK_PACKAGE_ID,
        Identifier::new("allowance").unwrap(),
        Identifier::new("revoke").unwrap(),
        vec![balance_sui_type()],
        vec![cap_arg, allowance_arg],
    );
    let revoke_tx = TransactionData::new_programmable(
        funder,
        vec![funder_gas],
        builder.finish(),
        10_000_000,
        test_env.rgp,
    );

    let mut builder = ProgrammableTransactionBuilder::new();
    let allowance_arg = builder
        .obj(ObjectArg::SharedObject {
            id: allowance_id,
            initial_shared_version,
            mutability: SharedObjectMutability::Mutable,
        })
        .unwrap();
    let withdraw_arg = builder
        .funds_withdrawal(FundsWithdrawalArg::balance_from_allowance(
            SPEND,
            GAS::type_tag(),
            funder,
            allowance_id,
        ))
        .unwrap();
    let clock_arg = builder
        .obj(ObjectArg::SharedObject {
            id: SUI_CLOCK_OBJECT_ID,
            initial_shared_version: SUI_CLOCK_OBJECT_SHARED_VERSION,
            mutability: SharedObjectMutability::Immutable,
        })
        .unwrap();
    let spent_balance = builder.programmable_move_call(
        SUI_FRAMEWORK_PACKAGE_ID,
        Identifier::new("allowance").unwrap(),
        Identifier::new("balance_spend").unwrap(),
        vec!["0x2::sui::SUI".parse().unwrap()],
        vec![allowance_arg, withdraw_arg, clock_arg],
    );
    let coin = builder.programmable_move_call(
        SUI_FRAMEWORK_PACKAGE_ID,
        Identifier::new("coin").unwrap(),
        Identifier::new("from_balance").unwrap(),
        vec!["0x2::sui::SUI".parse().unwrap()],
        vec![spent_balance],
    );
    builder.transfer_arg(spender, coin);
    let spend_tx = TransactionData::new_programmable(
        spender,
        vec![spender_gas],
        builder.finish(),
        10_000_000,
        test_env.rgp,
    );

    // Both are admitted while the allowance is alive; the bundle keeps
    // submission order, so the revoke lands first.
    let results = test_env
        .cluster
        .sign_and_execute_txns_in_soft_bundle(&[revoke_tx, spend_tx])
        .await
        .unwrap();
    let (_, revoke_effects) = &results[0];
    let (_, spend_effects) = &results[1];

    assert!(
        revoke_effects.status().is_ok(),
        "revoke failed: {:?}",
        revoke_effects.status()
    );
    assert!(
        revoke_effects.deleted().iter().any(|r| r.0 == allowance_id),
        "revoke deletes the allowance"
    );

    match spend_effects.status() {
        ExecutionStatus::Failure(ExecutionFailure {
            error: ExecutionFailureStatus::InputObjectDeleted,
            ..
        }) => (),
        other => panic!("expected InputObjectDeleted, got {other:?}"),
    }

    test_env.verify_accumulator_exists(funder, FUND);

    test_env.trigger_reconfiguration().await;
}

/// A spend admitted while the funder's balance covers it, with the funder's own
/// withdrawal sequenced ahead of it: the reservation can no longer be met, so
/// the spend fails before execution and nothing beyond gas moves.
#[sim_test]
async fn test_allowance_funder_drained_mid_flight() {
    if has_mainnet_protocol_config_override() {
        return;
    }
    let mut test_env = TestEnvBuilder::new().build().await;

    let funder = test_env.get_sender(0);
    let (spender, spender_gas) = test_env.get_sender_and_gas(1);

    test_env.fund_one_address_balance(funder, FUND).await;
    test_env.verify_accumulator_exists(funder, FUND);

    let (allowance_id, initial_shared_version, funder_gas) =
        issue_allowance(&mut test_env, funder, spender, SPEND).await;

    // The funder's own withdrawal, leaving one unit less than the spend needs.
    const DRAIN: u64 = FUND - SPEND + 1;
    let mut builder = ProgrammableTransactionBuilder::new();
    let withdraw_arg = builder
        .funds_withdrawal(FundsWithdrawalArg::balance_from_sender(
            DRAIN,
            GAS::type_tag(),
        ))
        .unwrap();
    let coin = builder.programmable_move_call(
        SUI_FRAMEWORK_PACKAGE_ID,
        Identifier::new("coin").unwrap(),
        Identifier::new("redeem_funds").unwrap(),
        vec!["0x2::sui::SUI".parse().unwrap()],
        vec![withdraw_arg],
    );
    builder.transfer_arg(funder, coin);
    let drain_tx = TransactionData::new_programmable(
        funder,
        vec![funder_gas],
        builder.finish(),
        10_000_000,
        test_env.rgp,
    );

    let spend_tx = build_spend_tx(
        test_env.rgp,
        spender,
        spender_gas,
        funder,
        allowance_id,
        initial_shared_version,
        SPEND,
    );

    let results = test_env
        .cluster
        .sign_and_execute_txns_in_soft_bundle(&[drain_tx, spend_tx])
        .await
        .unwrap();
    let (_, drain_effects) = &results[0];
    let (_, spend_effects) = &results[1];

    assert!(
        drain_effects.status().is_ok(),
        "drain failed: {:?}",
        drain_effects.status()
    );
    match spend_effects.status() {
        ExecutionStatus::Failure(ExecutionFailure {
            error: ExecutionFailureStatus::InsufficientFundsForWithdraw,
            ..
        }) => (),
        other => panic!("expected InsufficientFundsForWithdraw, got {other:?}"),
    }

    test_env.verify_accumulator_exists(funder, SPEND - 1);

    test_env.trigger_reconfiguration().await;
}

/// Two spends admitted against the same cap in one commit: sequencing applies
/// them in order, the first settles, and the second exceeds the cap in Move.
#[sim_test]
async fn test_allowance_cap_race_in_one_commit() {
    if has_mainnet_protocol_config_override() {
        return;
    }
    let mut test_env = TestEnvBuilder::new().build().await;

    let funder = test_env.get_sender(0);
    let (spender, spender_gas) = test_env.get_sender_and_all_gas(1);
    assert!(spender_gas.len() >= 2, "the spender needs two gas coins");

    test_env.fund_one_address_balance(funder, FUND).await;
    test_env.verify_accumulator_exists(funder, FUND);

    let (allowance_id, initial_shared_version, _) =
        issue_allowance(&mut test_env, funder, spender, SPEND).await;

    // Each fits the cap alone; together they exceed it.
    const PART: u64 = SPEND / 2 + 100_000;
    let spend_tx_1 = build_spend_tx(
        test_env.rgp,
        spender,
        spender_gas[0],
        funder,
        allowance_id,
        initial_shared_version,
        PART,
    );
    let spend_tx_2 = build_spend_tx(
        test_env.rgp,
        spender,
        spender_gas[1],
        funder,
        allowance_id,
        initial_shared_version,
        PART,
    );

    let results = test_env
        .cluster
        .sign_and_execute_txns_in_soft_bundle(&[spend_tx_1, spend_tx_2])
        .await
        .unwrap();
    let (_, first_effects) = &results[0];
    let (_, second_effects) = &results[1];

    assert!(
        first_effects.status().is_ok(),
        "first spend failed: {:?}",
        first_effects.status()
    );
    match second_effects.status() {
        ExecutionStatus::Failure(ExecutionFailure {
            error: ExecutionFailureStatus::MoveAbort(location, _),
            ..
        }) => {
            assert_eq!(location.function_name.as_deref(), Some("consume"));
        }
        other => panic!("expected a MoveAbort in consume, got {other:?}"),
    }

    test_env.verify_accumulator_exists(funder, FUND - PART);

    test_env.trigger_reconfiguration().await;
}

/// A spend admitted while the sender is the spender, with an app rotation
/// sequenced ahead of it: the allowance is alive but the sign-time spender
/// fact is stale, and Move's own check rejects the spend.
#[sim_test]
async fn test_allowance_spender_rotated_mid_flight() {
    if has_mainnet_protocol_config_override() {
        return;
    }
    let mut test_env = TestEnvBuilder::new().build().await;

    let funder = test_env.get_sender(0);
    let (spender, spender_gas) = test_env.get_sender_and_gas(1);
    let new_spender = test_env.get_sender(2);

    test_env.fund_one_address_balance(funder, FUND).await;
    test_env.verify_accumulator_exists(funder, FUND);

    let pkg = test_env
        .setup_test_package(allowance_test_code_path())
        .await;
    let app_type: sui_types::TypeTag = format!("{pkg}::allowance_app::APP").parse().unwrap();

    // The funder proposes an app-bound allowance and the app issues it.
    let funder_gas = test_env.get_gas_for_sender(funder)[0];
    let mut builder = ProgrammableTransactionBuilder::new();
    let no_rate_limit = builder.programmable_move_call(
        MOVE_STDLIB_PACKAGE_ID,
        Identifier::new("option").unwrap(),
        Identifier::new("none").unwrap(),
        vec![rate_limit_type()],
        vec![],
    );
    let args = vec![
        builder.pure("".to_string()).unwrap(),
        builder.pure(spender).unwrap(),
        builder.pure(Some(U256::from(SPEND))).unwrap(),
        builder.pure(None::<u64>).unwrap(),
        builder.pure(Some(u64::MAX)).unwrap(),
        no_rate_limit,
    ];
    let proposal = builder.programmable_move_call(
        SUI_FRAMEWORK_PACKAGE_ID,
        Identifier::new("allowance").unwrap(),
        Identifier::new("propose_for_app").unwrap(),
        vec![balance_sui_type(), app_type.clone()],
        args,
    );
    builder.programmable_move_call(
        pkg,
        Identifier::new("allowance_app").unwrap(),
        Identifier::new("issue").unwrap(),
        vec![balance_sui_type()],
        vec![proposal],
    );
    let tx = TransactionData::new_programmable(
        funder,
        vec![funder_gas],
        builder.finish(),
        10_000_000,
        test_env.rgp,
    );
    let (_, effects) = test_env.exec_tx_directly(tx).await.unwrap();
    assert!(
        effects.status().is_ok(),
        "app issuance failed: {:?}",
        effects.status()
    );
    let (allowance_ref, allowance_owner) = effects
        .created()
        .iter()
        .find(|(_, owner)| matches!(owner, Owner::Shared { .. }))
        .cloned()
        .expect("the allowance is created as a shared object");
    let Owner::Shared {
        initial_shared_version,
    } = allowance_owner
    else {
        unreachable!()
    };
    let allowance_id = allowance_ref.0;
    let funder_gas = effects.gas_object().expect("issuance paid gas").0;

    let mut builder = ProgrammableTransactionBuilder::new();
    let allowance_arg = builder
        .obj(ObjectArg::SharedObject {
            id: allowance_id,
            initial_shared_version,
            mutability: SharedObjectMutability::Mutable,
        })
        .unwrap();
    let new_spender_arg = builder.pure(new_spender).unwrap();
    builder.programmable_move_call(
        pkg,
        Identifier::new("allowance_app").unwrap(),
        Identifier::new("rotate").unwrap(),
        vec![balance_sui_type()],
        vec![allowance_arg, new_spender_arg],
    );
    let rotate_tx = TransactionData::new_programmable(
        funder,
        vec![funder_gas],
        builder.finish(),
        10_000_000,
        test_env.rgp,
    );

    // The old spender's spend, through the app path.
    let mut builder = ProgrammableTransactionBuilder::new();
    let allowance_arg = builder
        .obj(ObjectArg::SharedObject {
            id: allowance_id,
            initial_shared_version,
            mutability: SharedObjectMutability::Mutable,
        })
        .unwrap();
    let withdraw_arg = builder
        .funds_withdrawal(FundsWithdrawalArg::balance_from_allowance(
            SPEND,
            GAS::type_tag(),
            funder,
            allowance_id,
        ))
        .unwrap();
    let clock_arg = builder
        .obj(ObjectArg::SharedObject {
            id: SUI_CLOCK_OBJECT_ID,
            initial_shared_version: SUI_CLOCK_OBJECT_SHARED_VERSION,
            mutability: SharedObjectMutability::Immutable,
        })
        .unwrap();
    let spent_balance = builder.programmable_move_call(
        pkg,
        Identifier::new("allowance_app").unwrap(),
        Identifier::new("spend").unwrap(),
        vec!["0x2::sui::SUI".parse().unwrap()],
        vec![allowance_arg, withdraw_arg, clock_arg],
    );
    let coin = builder.programmable_move_call(
        SUI_FRAMEWORK_PACKAGE_ID,
        Identifier::new("coin").unwrap(),
        Identifier::new("from_balance").unwrap(),
        vec!["0x2::sui::SUI".parse().unwrap()],
        vec![spent_balance],
    );
    builder.transfer_arg(spender, coin);
    let spend_tx = TransactionData::new_programmable(
        spender,
        vec![spender_gas],
        builder.finish(),
        10_000_000,
        test_env.rgp,
    );

    // Both admitted while the old spender is current; the rotation lands first.
    let results = test_env
        .cluster
        .sign_and_execute_txns_in_soft_bundle(&[rotate_tx, spend_tx])
        .await
        .unwrap();
    let (_, rotate_effects) = &results[0];
    let (_, spend_effects) = &results[1];

    assert!(
        rotate_effects.status().is_ok(),
        "rotation failed: {:?}",
        rotate_effects.status()
    );
    match spend_effects.status() {
        ExecutionStatus::Failure(ExecutionFailure {
            error: ExecutionFailureStatus::MoveAbort(location, _),
            ..
        }) => {
            assert_eq!(location.function_name.as_deref(), Some("consume"));
        }
        other => panic!("expected a MoveAbort in consume, got {other:?}"),
    }

    test_env.verify_accumulator_exists(funder, FUND);

    test_env.trigger_reconfiguration().await;
}
