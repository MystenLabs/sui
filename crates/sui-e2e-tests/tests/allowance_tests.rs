// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Minimal e2e allowance flow: issue an allowance, then consume it in a PTB.
//! Covers sign-time checks, adapter value creation, Move policy, and settlement.

use move_core_types::{identifier::Identifier, u256::U256};
use sui_macros::*;
use sui_simulator::has_mainnet_protocol_config_override;
use sui_types::{
    SUI_CLOCK_OBJECT_ID, SUI_CLOCK_OBJECT_SHARED_VERSION, SUI_FRAMEWORK_PACKAGE_ID,
    effects::TransactionEffectsAPI,
    gas_coin::GAS,
    object::Owner,
    programmable_transaction_builder::ProgrammableTransactionBuilder,
    transaction::{
        FundsWithdrawalArg, ObjectArg, SharedObjectMutability, TransactionData,
    },
};
use test_cluster::addr_balance_test_env::TestEnvBuilder;

const FUND: u64 = 5_000_000;
const SPEND: u64 = 1_000_000;

fn balance_sui_type() -> sui_types::TypeTag {
    "0x2::balance::Balance<0x2::sui::SUI>".parse().unwrap()
}

#[sim_test]
async fn test_allowance_issue_and_spend() {
    // Allowances are devnet-gated; under a mainnet protocol config the
    // withdrawal is rejected at signing by design.
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

    // Funder issues an allowance to the spender, capped at SPEND.
    let (_, funder_gas) = test_env.get_sender_and_gas(0);
    let mut builder = ProgrammableTransactionBuilder::new();
    let args = vec![
        builder.pure("".to_string()).unwrap(), // name
        builder.pure(spender).unwrap(),
        builder.pure(Some(U256::from(SPEND))).unwrap(), // lifetime_cap
        builder.pure(None::<u64>).unwrap(),             // start_timestamp_ms
        builder.pure(None::<u64>).unwrap(),             // expiration_timestamp_ms
        builder.pure(None::<u64>).unwrap(),             // rate_period_ms
        builder.pure(None::<U256>).unwrap(),            // rate_amount
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
    // creates the withdrawal, and `spend_balance` enforces policy and redeems.
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
        Identifier::new("spend_balance").unwrap(),
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

    // Settlement: the funder's address balance is down by exactly SPEND, and the
    // spender holds the redeemed coin.
    test_env.verify_accumulator_exists(funder, FUND - SPEND);
    effects
        .created()
        .iter()
        .find(|(_, owner)| matches!(owner, Owner::AddressOwner(a) if *a == spender))
        .expect("the spender receives the redeemed coin");

    // Reconfiguration runs the conservation checks over the accumulator.
    test_env.trigger_reconfiguration().await;
}
