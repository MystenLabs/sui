// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::HashSet;
use std::time::Duration;

use move_core_types::{
    account_address::AccountAddress,
    identifier::Identifier,
    language_storage::{StructTag, TypeTag},
    u256::U256,
};
use sui_core::transaction_driver::SubmitTransactionOptions;
use sui_macros::*;
use sui_test_transaction_builder::FundSource;
use sui_types::{
    SUI_FRAMEWORK_PACKAGE_ID,
    base_types::SuiAddress,
    effects::TransactionEffectsAPI,
    gas::GasCostSummary,
    gas_coin::GAS,
    messages_grpc::SubmitTxRequest,
    programmable_transaction_builder::ProgrammableTransactionBuilder,
    transaction::{
        FundsWithdrawalArg, GasData, TransactionData, TransactionDataV1, TransactionExpiration,
        TransactionKind,
    },
    type_input::TypeInput,
};
use test_cluster::addr_balance_test_env::{TestEnv, TestEnvBuilder};

fn build_send_funds_ptb(amount: u64, recipient: SuiAddress) -> TransactionKind {
    let mut builder = ProgrammableTransactionBuilder::new();
    let withdraw_arg = FundsWithdrawalArg::balance_from_sender(amount, GAS::type_tag());
    let withdraw_arg = builder.funds_withdrawal(withdraw_arg).unwrap();
    let balance = builder.programmable_move_call(
        SUI_FRAMEWORK_PACKAGE_ID,
        Identifier::new("balance").unwrap(),
        Identifier::new("redeem_funds").unwrap(),
        vec!["0x2::sui::SUI".parse().unwrap()],
        vec![withdraw_arg],
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

async fn setup_custom_coin(test_env: &mut TestEnv, funding: &[(u64, SuiAddress)]) -> TypeTag {
    let total: u64 = funding.iter().map(|(amount, _)| amount).sum();
    let (publisher, coin_type) = test_env.setup_custom_coin().await;
    let tx = test_env
        .tx_builder(publisher)
        .transfer_funds_to_address_balance(
            FundSource::address_fund_with_reservation(total),
            funding.to_vec(),
            coin_type.clone(),
        )
        .build();
    let (_, effects) = test_env.exec_tx_directly(tx).await.unwrap();
    assert!(effects.status().is_ok());
    register_fail_point_arg("free_tier_extra_token_types", {
        let coin_type = coin_type.clone();
        move || {
            let mut extra = HashSet::new();
            extra.insert(TypeInput::from(coin_type.clone()));
            Some(extra)
        }
    });
    coin_type
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
    let sender = test_env.get_sender(1);
    let recipient = test_env.get_sender(2);

    let initial_funding = 10_000u64;
    let transfer_amount = 1000u64;

    let coin_type = setup_custom_coin(&mut test_env, &[(initial_funding, sender)]).await;
    assert_eq!(test_env.get_sui_balance(sender), 0);

    let tx = test_env.create_free_tier_transaction(
        transfer_amount,
        coin_type.clone(),
        sender,
        recipient,
        0,
        0,
    );
    let (_, effects) = test_env.exec_tx_directly(tx).await.unwrap();

    assert!(
        effects.status().is_ok(),
        "Free tier transfer should succeed: {:?}",
        effects.status()
    );
    assert_zero_gas(effects.gas_cost_summary());

    assert_eq!(
        test_env.get_balance(sender, coin_type.clone()),
        initial_funding - transfer_amount
    );
    assert_eq!(test_env.get_balance(recipient, coin_type), transfer_amount);

    test_env.trigger_reconfiguration().await;
}

#[cfg_attr(not(msim), ignore)]
#[sim_test]
async fn test_free_tier_multi_recipient() {
    let mut test_env = setup_free_tier_env().await;
    let sender = test_env.get_sender(1);
    let recipient1 = test_env.get_sender(2);
    let recipient2 = test_env.get_sender(3);

    let total_amount = 1000u64;
    let split_amount = 400u64;
    let remainder = total_amount - split_amount;

    let coin_type = setup_custom_coin(&mut test_env, &[(total_amount, sender)]).await;
    assert_eq!(test_env.get_sui_balance(sender), 0);

    let balance_type = TypeTag::Struct(Box::new(StructTag {
        address: *SUI_FRAMEWORK_PACKAGE_ID,
        module: Identifier::new("balance").unwrap(),
        name: Identifier::new("Balance").unwrap(),
        type_params: vec![coin_type.clone()],
    }));
    let mut builder = ProgrammableTransactionBuilder::new();
    let withdraw_arg = FundsWithdrawalArg::balance_from_sender(total_amount, coin_type.clone());
    let withdraw_arg = builder.funds_withdrawal(withdraw_arg).unwrap();
    // Split off split_amount for recipient1, remainder goes to recipient2
    let amount_arg = builder.pure(U256::from(split_amount)).unwrap();
    let split = builder.programmable_move_call(
        SUI_FRAMEWORK_PACKAGE_ID,
        Identifier::new("funds_accumulator").unwrap(),
        Identifier::new("withdrawal_split").unwrap(),
        vec![balance_type],
        vec![withdraw_arg, amount_arg],
    );
    let balance1 = builder.programmable_move_call(
        SUI_FRAMEWORK_PACKAGE_ID,
        Identifier::new("balance").unwrap(),
        Identifier::new("redeem_funds").unwrap(),
        vec![coin_type.clone()],
        vec![split],
    );
    let recipient1_arg = builder.pure(recipient1).unwrap();
    builder.programmable_move_call(
        SUI_FRAMEWORK_PACKAGE_ID,
        Identifier::new("balance").unwrap(),
        Identifier::new("gasless_send_funds").unwrap(),
        vec![coin_type.clone()],
        vec![balance1, recipient1_arg],
    );
    let balance2 = builder.programmable_move_call(
        SUI_FRAMEWORK_PACKAGE_ID,
        Identifier::new("balance").unwrap(),
        Identifier::new("redeem_funds").unwrap(),
        vec![coin_type.clone()],
        vec![withdraw_arg],
    );
    let recipient2_arg = builder.pure(recipient2).unwrap();
    builder.programmable_move_call(
        SUI_FRAMEWORK_PACKAGE_ID,
        Identifier::new("balance").unwrap(),
        Identifier::new("gasless_send_funds").unwrap(),
        vec![coin_type.clone()],
        vec![balance2, recipient2_arg],
    );
    let tx_kind = TransactionKind::ProgrammableTransaction(builder.finish());
    let tx = test_env.free_tier_transaction_data(tx_kind, sender, 0, 0);
    let (_, effects) = test_env.exec_tx_directly(tx).await.unwrap();

    assert!(
        effects.status().is_ok(),
        "Free tier multi-recipient transfer should succeed: {:?}",
        effects.status()
    );
    assert_zero_gas(effects.gas_cost_summary());

    assert_eq!(test_env.get_balance(sender, coin_type.clone()), 0);
    assert_eq!(
        test_env.get_balance(recipient1, coin_type.clone()),
        split_amount
    );
    assert_eq!(test_env.get_balance(recipient2, coin_type), remainder);

    test_env.trigger_reconfiguration().await;
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

    let sender = test_env.get_sender(1);
    let coin_type = setup_custom_coin(&mut test_env, &[(10_000, sender)]).await;
    assert_eq!(test_env.get_sui_balance(sender), 0);

    let tx = test_env.create_free_tier_transaction(1000, coin_type, sender, sender, 0, 0);
    let result = test_env.exec_tx_directly(tx).await;

    let err = result.unwrap_err();
    assert!(
        err.to_string().contains("Gas price"),
        "Expected gas price validation error, got: {err}"
    );

    test_env.trigger_reconfiguration().await;
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

    let tx_kind = build_send_funds_ptb(1000, recipient);
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

    let free_tx =
        test_env.create_free_tier_transaction(1000, GAS::type_tag(), sender, recipient, 1, 0);
    let (_, effects) = test_env.exec_tx_directly(free_tx).await.unwrap();
    assert!(effects.status().is_ok());
    assert_zero_gas(effects.gas_cost_summary());

    test_env.trigger_reconfiguration().await;
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
    let tx = test_env.free_tier_transaction_data(tx_kind, sender, 0, 0);
    let result = test_env.exec_tx_directly(tx).await;

    let err = result.unwrap_err();
    assert!(
        err.to_string()
            .contains("can only call balance::gasless_send_funds"),
        "Expected function whitelist error, got: {err}"
    );

    test_env.trigger_reconfiguration().await;
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
    let balance = builder.programmable_move_call(
        SUI_FRAMEWORK_PACKAGE_ID,
        Identifier::new("balance").unwrap(),
        Identifier::new("redeem_funds").unwrap(),
        vec!["0x2::sui::SUI".parse().unwrap()],
        vec![withdraw_arg],
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

    let tx = test_env.free_tier_transaction_data(tx_kind, sender, 0, 0);
    let result = test_env.exec_tx_directly(tx).await;

    let err = result.unwrap_err();
    assert!(
        err.to_string()
            .contains("can only call balance::gasless_send_funds")
            || err.to_string().contains("can only contain MoveCall"),
        "Expected function whitelist or MoveCall-only error, got: {err}"
    );

    test_env.trigger_reconfiguration().await;
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

    let sender = test_env.get_sender(1);
    let recipient = test_env.get_sender(2);
    let coin_type = setup_custom_coin(&mut test_env, &[(10_000, sender)]).await;
    assert_eq!(test_env.get_sui_balance(sender), 0);

    let tx = test_env.create_free_tier_transaction(100, coin_type, sender, recipient, 0, 0);
    let (_, effects) = test_env.exec_tx_directly(tx).await.unwrap();

    assert!(
        effects.status().is_err(),
        "Free tier should fail when computation exceeds cap"
    );
    assert_zero_gas(effects.gas_cost_summary());

    test_env.trigger_reconfiguration().await;
}

// drive_transaction computes amplification_factor = gas_price / rgp, which would
// reject free tier transactions (gas_price=0) with GasPriceUnderRGP. This test
// verifies the bypass for that check works through the full orchestrator path.
#[cfg_attr(not(msim), ignore)]
#[sim_test]
async fn test_free_tier_drive_transaction() {
    let mut test_env = setup_free_tier_env().await;
    let sender = test_env.get_sender(1);
    let recipient = test_env.get_sender(2);

    let coin_type = setup_custom_coin(&mut test_env, &[(5000, sender)]).await;
    assert_eq!(test_env.get_sui_balance(sender), 0);

    let tx_data = test_env.create_free_tier_transaction(1000, coin_type, sender, recipient, 0, 0);
    let signed_tx = test_env.cluster.wallet.sign_transaction(&tx_data).await;

    let orchestrator = test_env
        .cluster
        .fullnode_handle
        .sui_node
        .with(|n| n.transaction_orchestrator().as_ref().unwrap().clone());

    let result = orchestrator
        .transaction_driver()
        .drive_transaction(
            SubmitTxRequest::new_transaction(signed_tx),
            SubmitTransactionOptions {
                ..Default::default()
            },
            Some(Duration::from_secs(60)),
        )
        .await;

    assert!(
        result.is_ok(),
        "Free tier transaction should succeed via drive_transaction: {:?}",
        result.err()
    );

    test_env.trigger_reconfiguration().await;
}

#[cfg_attr(not(msim), ignore)]
#[sim_test]
async fn test_free_tier_rejects_non_sui_token() {
    let mut test_env = setup_free_tier_env().await;
    let sender = test_env.get_sender(0);
    let recipient = test_env.get_sender(1);

    test_env
        .fund_one_address_balance(sender, 10_000_000_000)
        .await;

    let fake_type = TypeTag::Struct(Box::new(StructTag {
        address: AccountAddress::from_hex_literal("0x123").unwrap(),
        module: Identifier::new("fake").unwrap(),
        name: Identifier::new("FAKE").unwrap(),
        type_params: vec![],
    }));

    let tx = test_env.create_free_tier_transaction(1000, fake_type, sender, recipient, 0, 0);
    let result = test_env.exec_tx_directly(tx).await;

    let err = result.unwrap_err();
    assert!(
        err.to_string()
            .contains("only support whitelisted token types"),
        "Expected token whitelist error, got: {err}"
    );

    test_env.trigger_reconfiguration().await;
}

#[cfg_attr(not(msim), ignore)]
#[sim_test]
async fn test_non_free_tier_rejects_gasless_send_funds() {
    let mut test_env = setup_free_tier_env().await;
    let sender = test_env.get_sender(0);
    let recipient = test_env.get_sender(1);

    test_env
        .fund_one_address_balance(sender, 10_000_000_000)
        .await;

    let mut builder = test_env.tx_builder(sender);
    let ptb = builder.ptb_builder_mut();
    let withdraw_arg = FundsWithdrawalArg::balance_from_sender(1000, GAS::type_tag());
    let withdraw_arg = ptb.funds_withdrawal(withdraw_arg).unwrap();
    let balance = ptb.programmable_move_call(
        SUI_FRAMEWORK_PACKAGE_ID,
        Identifier::new("balance").unwrap(),
        Identifier::new("redeem_funds").unwrap(),
        vec!["0x2::sui::SUI".parse().unwrap()],
        vec![withdraw_arg],
    );
    let recipient_arg = ptb.pure(recipient).unwrap();
    ptb.programmable_move_call(
        SUI_FRAMEWORK_PACKAGE_ID,
        Identifier::new("balance").unwrap(),
        Identifier::new("gasless_send_funds").unwrap(),
        vec!["0x2::sui::SUI".parse().unwrap()],
        vec![balance, recipient_arg],
    );
    let tx = builder.build();
    let result = test_env.exec_tx_directly(tx).await;

    let err = result.unwrap_err();
    assert!(
        err.to_string()
            .contains("gasless_send_funds can only be called from free tier transactions"),
        "Expected free-tier-only error, got: {err}"
    );

    test_env.trigger_reconfiguration().await;
}

#[cfg_attr(not(msim), ignore)]
#[sim_test]
async fn test_free_tier_custom_coin_transfer() {
    let mut test_env = setup_free_tier_env().await;
    let sender = test_env.get_sender(1);
    let recipient = test_env.get_sender(2);

    let coin_type = setup_custom_coin(&mut test_env, &[(5000, sender), (3000, recipient)]).await;
    assert_eq!(test_env.get_sui_balance(sender), 0);

    let transfer_amount = 500u64;
    let sender_before = test_env.get_balance(sender, coin_type.clone());
    let recipient_before = test_env.get_balance(recipient, coin_type.clone());
    let tx = test_env.create_free_tier_transaction(
        transfer_amount,
        coin_type.clone(),
        sender,
        recipient,
        0,
        0,
    );
    let (_, effects) = test_env.exec_tx_directly(tx).await.unwrap();

    assert!(
        effects.status().is_ok(),
        "Free tier custom coin transfer should succeed with 0 SUI: {:?}",
        effects.status()
    );
    assert_zero_gas(effects.gas_cost_summary());

    let sender_after = test_env.get_balance(sender, coin_type.clone());
    let recipient_after = test_env.get_balance(recipient, coin_type);
    assert_eq!(sender_after, sender_before - transfer_amount);
    assert_eq!(recipient_after, recipient_before + transfer_amount);

    test_env.trigger_reconfiguration().await;
}
