// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use move_core_types::{identifier::Identifier, u256::U256};
use sui_macros::*;
use sui_types::{
    SUI_FRAMEWORK_PACKAGE_ID,
    base_types::SuiAddress,
    effects::TransactionEffectsAPI,
    gas::GasCostSummary,
    gas_coin::GAS,
    programmable_transaction_builder::ProgrammableTransactionBuilder,
    transaction::{
        FundsWithdrawalArg, GasData, TransactionData, TransactionDataV1, TransactionExpiration,
        TransactionKind,
    },
};
use test_cluster::addr_balance_test_env::{TestEnv, TestEnvBuilder, get_sui_balance};

fn create_free_tier_transaction(
    tx_kind: TransactionKind,
    sender: SuiAddress,
    rgp: u64,
    chain_id: sui_types::digests::ChainIdentifier,
    nonce: u32,
    epoch: u64,
) -> TransactionData {
    TransactionData::V1(TransactionDataV1 {
        kind: tx_kind,
        sender,
        gas_data: GasData {
            payment: vec![],
            owner: sender,
            price: rgp,
            budget: 0,
        },
        expiration: TransactionExpiration::ValidDuring {
            min_epoch: Some(epoch),
            max_epoch: Some(epoch),
            min_timestamp: None,
            max_timestamp: None,
            chain: chain_id,
            nonce,
        },
    })
}

fn build_gasless_send_funds_ptb(amount: u64, recipient: SuiAddress) -> TransactionKind {
    let mut builder = ProgrammableTransactionBuilder::new();
    let withdraw_arg = FundsWithdrawalArg::balance_from_sender(amount, GAS::type_tag());
    let withdraw_arg = builder.funds_withdrawal(withdraw_arg).unwrap();
    let amount_arg = builder.pure(U256::from(amount)).unwrap();
    let split = builder.programmable_move_call(
        SUI_FRAMEWORK_PACKAGE_ID,
        Identifier::new("funds_accumulator").unwrap(),
        Identifier::new("withdrawal_split").unwrap(),
        vec!["0x2::balance::Balance<0x2::sui::SUI>".parse().unwrap()],
        vec![withdraw_arg, amount_arg],
    );
    let balance = builder.programmable_move_call(
        SUI_FRAMEWORK_PACKAGE_ID,
        Identifier::new("balance").unwrap(),
        Identifier::new("redeem_funds").unwrap(),
        vec!["0x2::sui::SUI".parse().unwrap()],
        vec![split],
    );
    let recipient_arg = builder.pure(recipient).unwrap();
    builder.programmable_move_call(
        SUI_FRAMEWORK_PACKAGE_ID,
        Identifier::new("balance").unwrap(),
        Identifier::new("gasless_send_funds").unwrap(),
        vec!["0x2::sui::SUI".parse().unwrap()],
        vec![balance, recipient_arg],
    );
    TransactionKind::ProgrammableTransaction(builder.finish())
}

fn build_send_funds_ptb(amount: u64, recipient: SuiAddress) -> TransactionKind {
    let mut builder = ProgrammableTransactionBuilder::new();
    let withdraw_arg = FundsWithdrawalArg::balance_from_sender(amount, GAS::type_tag());
    let withdraw_arg = builder.funds_withdrawal(withdraw_arg).unwrap();
    let amount_arg = builder.pure(U256::from(amount)).unwrap();
    let split = builder.programmable_move_call(
        SUI_FRAMEWORK_PACKAGE_ID,
        Identifier::new("funds_accumulator").unwrap(),
        Identifier::new("withdrawal_split").unwrap(),
        vec!["0x2::balance::Balance<0x2::sui::SUI>".parse().unwrap()],
        vec![withdraw_arg, amount_arg],
    );
    let balance = builder.programmable_move_call(
        SUI_FRAMEWORK_PACKAGE_ID,
        Identifier::new("balance").unwrap(),
        Identifier::new("redeem_funds").unwrap(),
        vec!["0x2::sui::SUI".parse().unwrap()],
        vec![split],
    );
    let recipient_arg = builder.pure(recipient).unwrap();
    builder.programmable_move_call(
        SUI_FRAMEWORK_PACKAGE_ID,
        Identifier::new("balance").unwrap(),
        Identifier::new("send_funds").unwrap(),
        vec!["0x2::sui::SUI".parse().unwrap()],
        vec![balance, recipient_arg],
    );
    TransactionKind::ProgrammableTransaction(builder.finish())
}

async fn setup_free_tier_env() -> TestEnv {
    TestEnvBuilder::new()
        .with_proto_override_cb(Box::new(|_, mut cfg| {
            cfg.enable_free_tier_for_testing();
            cfg
        }))
        .build()
        .await
}

fn db_sui_balance(test_env: &TestEnv, owner: SuiAddress) -> u64 {
    test_env.cluster.fullnode_handle.sui_node.with(move |node| {
        let state = node.state();
        let resolver = state.get_child_object_resolver();
        get_sui_balance(resolver.as_ref(), owner)
    })
}

fn assert_zero_gas(gas_summary: &GasCostSummary) {
    assert_eq!(gas_summary.computation_cost, 0);
    assert_eq!(gas_summary.storage_cost, 0);
    assert_eq!(gas_summary.storage_rebate, 0);
    assert_eq!(gas_summary.non_refundable_storage_fee, 0);
}

#[cfg_attr(not(msim), ignore)]
#[sim_test]
async fn test_free_tier_transfer_success() {
    let mut test_env = setup_free_tier_env().await;
    let sender = test_env.get_sender(0);
    let recipient = test_env.get_sender(1);

    test_env
        .fund_one_address_balance(sender, 10_000_000_000)
        .await;

    let tx_kind = build_gasless_send_funds_ptb(1000, recipient);
    let tx = create_free_tier_transaction(tx_kind, sender, test_env.rgp, test_env.chain_id, 0, 0);
    let result = test_env.exec_tx_directly(tx).await;

    let (_, effects) = result.unwrap();

    assert!(
        effects.status().is_ok(),
        "Free tier transfer should succeed: {:?}",
        effects.status()
    );
    assert_zero_gas(effects.gas_cost_summary());
}

#[cfg_attr(not(msim), ignore)]
#[sim_test]
async fn test_free_tier_disabled_rejects_transaction() {
    let mut test_env = TestEnvBuilder::new()
        .with_proto_override_cb(Box::new(|_, mut cfg| {
            cfg.enable_address_balance_gas_payments_for_testing();
            cfg.disable_free_tier_for_testing();
            cfg
        }))
        .build()
        .await;

    let sender = test_env.get_sender(0);
    test_env
        .fund_one_address_balance(sender, 10_000_000_000)
        .await;

    let tx_kind = build_gasless_send_funds_ptb(1000, sender);
    let tx = create_free_tier_transaction(tx_kind, sender, test_env.rgp, test_env.chain_id, 0, 0);
    let result = test_env.exec_tx_directly(tx).await;

    assert!(
        result.is_err(),
        "Free tier should be rejected when disabled"
    );
}

#[cfg_attr(not(msim), ignore)]
#[sim_test]
async fn test_free_tier_gas_never_charged_on_failure() {
    let mut test_env = setup_free_tier_env().await;
    let sender = test_env.get_sender(0);

    test_env.fund_one_address_balance(sender, 100).await;

    let sender_balance_before = db_sui_balance(&test_env, sender);

    let tx_kind = build_gasless_send_funds_ptb(1000, sender);
    let tx = create_free_tier_transaction(tx_kind, sender, test_env.rgp, test_env.chain_id, 0, 0);
    let result = test_env.exec_tx_directly(tx).await;

    assert!(
        result.is_err(),
        "Should be rejected at signing (insufficient withdrawal balance)"
    );

    let sender_balance_after = db_sui_balance(&test_env, sender);
    assert_eq!(sender_balance_after, sender_balance_before);
}

#[cfg_attr(not(msim), ignore)]
#[sim_test]
async fn test_free_tier_conservation_check() {
    let mut test_env = setup_free_tier_env().await;
    let sender = test_env.get_sender(0);
    let recipient = test_env.get_sender(1);

    test_env
        .fund_one_address_balance(sender, 5_000_000_000)
        .await;
    test_env
        .fund_one_address_balance(recipient, 1_000_000_000)
        .await;

    let sender_before = db_sui_balance(&test_env, sender);
    let recipient_before = db_sui_balance(&test_env, recipient);
    let total_before = sender_before + recipient_before;

    let transfer_amount = 1_000_000_000u64;
    let tx_kind = build_gasless_send_funds_ptb(transfer_amount, recipient);
    let tx = create_free_tier_transaction(tx_kind, sender, test_env.rgp, test_env.chain_id, 0, 0);
    let (_, effects) = test_env.exec_tx_directly(tx).await.unwrap();

    assert!(effects.status().is_ok());
    assert_zero_gas(effects.gas_cost_summary());

    let sender_after = db_sui_balance(&test_env, sender);
    let recipient_after = db_sui_balance(&test_env, recipient);
    let total_after = sender_after + recipient_after;

    assert_eq!(total_before, total_after);
    assert_eq!(sender_after, sender_before - transfer_amount);
    assert_eq!(recipient_after, recipient_before + transfer_amount);
}

#[cfg_attr(not(msim), ignore)]
#[sim_test]
async fn test_free_tier_paid_tx_still_works() {
    let mut test_env = setup_free_tier_env().await;
    let sender = test_env.get_sender(0);
    let recipient = test_env.get_sender(1);

    test_env
        .fund_one_address_balance(sender, 10_000_000_000)
        .await;

    let tx_kind = build_gasless_send_funds_ptb(1000, recipient);
    let paid_tx = TransactionData::V1(TransactionDataV1 {
        kind: tx_kind,
        sender,
        gas_data: GasData {
            payment: vec![],
            owner: sender,
            price: test_env.rgp,
            budget: 10_000_000,
        },
        expiration: TransactionExpiration::ValidDuring {
            min_epoch: Some(0),
            max_epoch: Some(0),
            min_timestamp: None,
            max_timestamp: None,
            chain: test_env.chain_id,
            nonce: 0,
        },
    });
    let (_, effects) = test_env.exec_tx_directly(paid_tx).await.unwrap();
    assert!(effects.status().is_ok());
    let gas_summary = effects.gas_cost_summary();
    assert!(
        gas_summary.computation_cost > 0,
        "Paid tx should have nonzero gas"
    );

    let tx_kind = build_gasless_send_funds_ptb(1000, recipient);
    let free_tx =
        create_free_tier_transaction(tx_kind, sender, test_env.rgp, test_env.chain_id, 1, 0);
    let (_, effects) = test_env.exec_tx_directly(free_tx).await.unwrap();
    assert!(effects.status().is_ok());
    assert_zero_gas(effects.gas_cost_summary());
}

#[cfg_attr(not(msim), ignore)]
#[sim_test]
async fn test_free_tier_rejects_regular_send_funds() {
    let mut test_env = setup_free_tier_env().await;
    let sender = test_env.get_sender(0);
    let recipient = test_env.get_sender(1);

    test_env
        .fund_one_address_balance(sender, 10_000_000_000)
        .await;

    let tx_kind = build_send_funds_ptb(1000, recipient);
    let tx = create_free_tier_transaction(tx_kind, sender, test_env.rgp, test_env.chain_id, 0, 0);
    let result = test_env.exec_tx_directly(tx).await;

    assert!(
        result.is_err(),
        "Free tier should reject balance::send_funds - only gasless_send_funds allowed"
    );
}

#[cfg_attr(not(msim), ignore)]
#[sim_test]
async fn test_free_tier_rejects_transfer_objects() {
    let mut test_env = setup_free_tier_env().await;
    let sender = test_env.get_sender(0);
    let recipient = test_env.get_sender(1);

    test_env
        .fund_one_address_balance(sender, 10_000_000_000)
        .await;

    let mut builder = ProgrammableTransactionBuilder::new();
    let withdraw_arg = FundsWithdrawalArg::balance_from_sender(1000, GAS::type_tag());
    let withdraw_arg = builder.funds_withdrawal(withdraw_arg).unwrap();
    let amount_arg = builder.pure(U256::from(1000u64)).unwrap();
    let split = builder.programmable_move_call(
        SUI_FRAMEWORK_PACKAGE_ID,
        Identifier::new("funds_accumulator").unwrap(),
        Identifier::new("withdrawal_split").unwrap(),
        vec!["0x2::balance::Balance<0x2::sui::SUI>".parse().unwrap()],
        vec![withdraw_arg, amount_arg],
    );
    let balance = builder.programmable_move_call(
        SUI_FRAMEWORK_PACKAGE_ID,
        Identifier::new("balance").unwrap(),
        Identifier::new("redeem_funds").unwrap(),
        vec!["0x2::sui::SUI".parse().unwrap()],
        vec![split],
    );
    let coin = builder.programmable_move_call(
        SUI_FRAMEWORK_PACKAGE_ID,
        Identifier::new("coin").unwrap(),
        Identifier::new("from_balance").unwrap(),
        vec!["0x2::sui::SUI".parse().unwrap()],
        vec![balance],
    );
    builder.transfer_arg(recipient, coin);
    let tx_kind = TransactionKind::ProgrammableTransaction(builder.finish());

    let tx = create_free_tier_transaction(tx_kind, sender, test_env.rgp, test_env.chain_id, 0, 0);
    let result = test_env.exec_tx_directly(tx).await;

    assert!(
        result.is_err(),
        "Free tier should reject TransferObjects command"
    );
}

#[cfg_attr(not(msim), ignore)]
#[sim_test]
async fn test_free_tier_computation_cap() {
    let mut test_env = TestEnvBuilder::new()
        .with_proto_override_cb(Box::new(|_, mut cfg| {
            cfg.enable_free_tier_for_testing();
            cfg.set_free_tier_max_computation_units_for_testing(1);
            cfg
        }))
        .build()
        .await;

    let sender = test_env.get_sender(0);
    let recipient = test_env.get_sender(1);
    test_env
        .fund_one_address_balance(sender, 10_000_000_000)
        .await;

    let tx_kind = build_gasless_send_funds_ptb(100, recipient);
    let tx = create_free_tier_transaction(tx_kind, sender, test_env.rgp, test_env.chain_id, 0, 0);
    let (_, effects) = test_env.exec_tx_directly(tx).await.unwrap();

    assert!(
        effects.status().is_err(),
        "Free tier should fail when computation exceeds cap"
    );
    assert_zero_gas(effects.gas_cost_summary());
}

#[cfg_attr(not(msim), ignore)]
#[sim_test]
async fn test_free_tier_load_shedding_on_overload() {
    let mut test_env = setup_free_tier_env().await;
    let sender = test_env.get_sender(0);
    let recipient = test_env.get_sender(1);

    test_env
        .fund_one_address_balance(sender, 10_000_000_000)
        .await;

    let tx_kind = build_gasless_send_funds_ptb(100, recipient);
    let tx = create_free_tier_transaction(tx_kind, sender, test_env.rgp, test_env.chain_id, 0, 0);
    let (_, effects) = test_env.exec_tx_directly(tx).await.unwrap();
    assert!(
        effects.status().is_ok(),
        "Free tier should succeed when not overloaded"
    );

    for handle in test_env.cluster.all_validator_handles() {
        handle.with(|node| {
            node.state().overload_info.set_overload(20);
        });
    }

    let tx_kind = build_gasless_send_funds_ptb(100, recipient);
    let tx = create_free_tier_transaction(tx_kind, sender, test_env.rgp, test_env.chain_id, 1, 0);
    let result = test_env.exec_tx_directly(tx).await;
    assert!(
        result.is_err(),
        "Free tier should be rejected when overloaded (20% base * 10x multiplier = 100% shed)"
    );

    for handle in test_env.cluster.all_validator_handles() {
        handle.with(|node| {
            node.state().overload_info.clear_overload();
        });
    }

    let tx_kind = build_gasless_send_funds_ptb(100, recipient);
    let tx = create_free_tier_transaction(tx_kind, sender, test_env.rgp, test_env.chain_id, 2, 0);
    let (_, effects) = test_env.exec_tx_directly(tx).await.unwrap();
    assert!(
        effects.status().is_ok(),
        "Free tier should succeed after overload clears"
    );
}
