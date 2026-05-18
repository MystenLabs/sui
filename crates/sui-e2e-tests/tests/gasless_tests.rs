// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::time::Duration;

use move_core_types::{
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
        self, Command, FundsWithdrawalArg, GasData, ObjectArg, TransactionData, TransactionDataV1,
        TransactionExpiration, TransactionKind,
    },
};
use test_cluster::addr_balance_test_env::{TestEnv, TestEnvBuilder};

async fn setup_gasless_env() -> TestEnv {
    TestEnvBuilder::new().build().await
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
    transaction::add_gasless_token_for_testing(coin_type.to_canonical_string(true), 0);
    coin_type
}

fn build_send_funds_ptb(amount: u64, coin_type: TypeTag, recipient: SuiAddress) -> TransactionKind {
    let mut builder = ProgrammableTransactionBuilder::new();
    let withdraw_arg = FundsWithdrawalArg::balance_from_sender(amount, coin_type.clone());
    let withdraw_arg = builder.funds_withdrawal(withdraw_arg).unwrap();
    let balance = builder.programmable_move_call(
        SUI_FRAMEWORK_PACKAGE_ID,
        Identifier::new("balance").unwrap(),
        Identifier::new("redeem_funds").unwrap(),
        vec![coin_type.clone()],
        vec![withdraw_arg],
    );
    let recipient_arg = builder.pure(recipient).unwrap();
    builder.programmable_move_call(
        SUI_FRAMEWORK_PACKAGE_ID,
        Identifier::new("balance").unwrap(),
        Identifier::new("send_funds").unwrap(),
        vec![coin_type],
        vec![balance, recipient_arg],
    );
    TransactionKind::ProgrammableTransaction(builder.finish())
}

fn assert_zero_gas(gas_summary: &GasCostSummary) {
    assert_eq!(gas_summary.computation_cost, 0);
    assert_eq!(gas_summary.storage_cost, 0);
    assert_eq!(gas_summary.storage_rebate, 0);
    assert_eq!(gas_summary.non_refundable_storage_fee, 0);
}

#[cfg_attr(not(msim), ignore)]
#[sim_test]
async fn test_gasless_transfer_success() {
    let mut test_env = setup_gasless_env().await;
    let sender = test_env.get_sender(1);
    let recipient = test_env.get_sender(2);

    let initial_funding = 10_000u64;
    let transfer_amount = 1000u64;

    let coin_type = setup_custom_coin(&mut test_env, &[(initial_funding, sender)]).await;
    assert_eq!(test_env.get_sui_balance_ab(sender), 0);

    let tx = test_env.create_gasless_transaction(
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
        "Gasless transfer should succeed: {:?}",
        effects.status()
    );
    assert_zero_gas(effects.gas_cost_summary());

    assert_eq!(
        test_env.get_balance_ab(sender, coin_type.clone()),
        initial_funding - transfer_amount
    );
    assert_eq!(
        test_env.get_balance_ab(recipient, coin_type),
        transfer_amount
    );

    test_env.trigger_reconfiguration().await;
}

#[cfg_attr(not(msim), ignore)]
#[sim_test]
async fn test_gasless_dryrun() {
    let mut test_env = setup_gasless_env().await;
    let sender = test_env.get_sender(1);
    let recipient = test_env.get_sender(2);

    let initial_funding = 10_000u64;
    let transfer_amount = 1000u64;

    let coin_type = setup_custom_coin(&mut test_env, &[(initial_funding, sender)]).await;

    // Verify dryrun works for gasless transactions.
    let tx_data = test_env.create_gasless_transaction(
        transfer_amount,
        coin_type.clone(),
        sender,
        recipient,
        0,
        0,
    );
    let result = test_env
        .cluster
        .fullnode_handle
        .sui_node
        .with_async(|node| async move { node.state().dry_exec_transaction(tx_data).await })
        .await;
    assert!(
        result.is_ok(),
        "Expected gasless dryrun to succeed, got: {:?}",
        result.unwrap_err()
    );

    // Verify simulate works for gasless transactions.
    let tx_data = test_env.create_gasless_transaction(
        transfer_amount,
        coin_type.clone(),
        sender,
        recipient,
        0,
        0,
    );
    let result = test_env.cluster.fullnode_handle.sui_node.with(|node| {
        node.state().simulate_transaction(
            tx_data,
            sui_types::transaction_executor::TransactionChecks::Enabled,
            true,
        )
    });
    if let Err(e) = result {
        panic!("Expected gasless simulate to succeed, got: {e}");
    }

    // Verify direct execution also works for the same transaction shape.
    let tx_data =
        test_env.create_gasless_transaction(transfer_amount, coin_type, sender, recipient, 1, 0);
    let (_, effects) = test_env.exec_tx_directly(tx_data).await.unwrap();
    assert!(effects.status().is_ok());
    assert_zero_gas(effects.gas_cost_summary());
}

#[cfg_attr(not(msim), ignore)]
#[sim_test]
async fn test_gasless_multi_recipient() {
    let mut test_env = setup_gasless_env().await;
    let sender = test_env.get_sender(1);
    let recipient1 = test_env.get_sender(2);
    let recipient2 = test_env.get_sender(3);

    let total_amount = 1000u64;
    let split_amount = 400u64;
    let remainder = total_amount - split_amount;

    let coin_type = setup_custom_coin(&mut test_env, &[(total_amount, sender)]).await;
    assert_eq!(test_env.get_sui_balance_ab(sender), 0);

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
        Identifier::new("send_funds").unwrap(),
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
        Identifier::new("send_funds").unwrap(),
        vec![coin_type.clone()],
        vec![balance2, recipient2_arg],
    );
    let tx_kind = TransactionKind::ProgrammableTransaction(builder.finish());
    let tx = test_env.gasless_transaction_data(tx_kind, sender, 0, 0);
    let (_, effects) = test_env.exec_tx_directly(tx).await.unwrap();

    assert!(
        effects.status().is_ok(),
        "Gasless multi-recipient transfer should succeed: {:?}",
        effects.status()
    );
    assert_zero_gas(effects.gas_cost_summary());

    assert_eq!(test_env.get_balance_ab(sender, coin_type.clone()), 0);
    assert_eq!(
        test_env.get_balance_ab(recipient1, coin_type.clone()),
        split_amount
    );
    assert_eq!(test_env.get_balance_ab(recipient2, coin_type), remainder);

    test_env.trigger_reconfiguration().await;
}

#[cfg_attr(not(msim), ignore)]
#[sim_test]
async fn test_gasless_disabled_rejects_transaction() {
    let mut test_env = TestEnvBuilder::new()
        .with_proto_override_cb(Box::new(|_, mut cfg| {
            cfg.disable_gasless_for_testing();
            cfg
        }))
        .build()
        .await;

    let sender = test_env.get_sender(1);
    let coin_type = setup_custom_coin(&mut test_env, &[(10_000, sender)]).await;
    assert_eq!(test_env.get_sui_balance_ab(sender), 0);

    let tx = test_env.create_gasless_transaction(1000, coin_type, sender, sender, 0, 0);
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
async fn test_gasless_paid_tx_still_works() {
    let mut test_env = setup_gasless_env().await;
    let sender = test_env.get_sender(0);
    let recipient = test_env.get_sender(1);

    test_env
        .fund_one_address_balance(sender, 10_000_000_000)
        .await;

    let tx_kind = build_send_funds_ptb(1000, GAS::type_tag(), recipient);
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

    let coin_type = setup_custom_coin(&mut test_env, &[(10_000, sender)]).await;
    let free_tx = test_env.create_gasless_transaction(1000, coin_type, sender, recipient, 1, 0);
    let (_, effects) = test_env.exec_tx_directly(free_tx).await.unwrap();
    assert!(effects.status().is_ok());
    assert_zero_gas(effects.gas_cost_summary());

    test_env.trigger_reconfiguration().await;
}

#[cfg_attr(not(msim), ignore)]
#[sim_test]
async fn test_gasless_computation_cap() {
    let mut test_env = TestEnvBuilder::new()
        .with_proto_override_cb(Box::new(|_, mut cfg| {
            cfg.set_gasless_max_computation_units_for_testing(1);
            cfg
        }))
        .build()
        .await;

    let sender = test_env.get_sender(1);
    let recipient = test_env.get_sender(2);
    let coin_type = setup_custom_coin(&mut test_env, &[(10_000, sender)]).await;
    assert_eq!(test_env.get_sui_balance_ab(sender), 0);

    let tx = test_env.create_gasless_transaction(100, coin_type, sender, recipient, 0, 0);
    let (_, effects) = test_env.exec_tx_directly(tx).await.unwrap();

    assert!(
        effects.status().is_err(),
        "Gasless should fail when computation exceeds cap"
    );
    assert_zero_gas(effects.gas_cost_summary());

    test_env.trigger_reconfiguration().await;
}

// drive_transaction computes amplification_factor = gas_price / rgp, which would
// reject gasless transactions (gas_price=0) with GasPriceUnderRGP. This test
// verifies the bypass for that check works through the full orchestrator path.
#[cfg_attr(not(msim), ignore)]
#[sim_test]
async fn test_gasless_drive_transaction() {
    let mut test_env = setup_gasless_env().await;
    let sender = test_env.get_sender(1);
    let recipient = test_env.get_sender(2);

    let coin_type = setup_custom_coin(&mut test_env, &[(5000, sender)]).await;
    assert_eq!(test_env.get_sui_balance_ab(sender), 0);

    let tx_data = test_env.create_gasless_transaction(1000, coin_type, sender, recipient, 0, 0);
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
        "Gasless transaction should succeed via drive_transaction: {:?}",
        result.err()
    );

    test_env.trigger_reconfiguration().await;
}

#[cfg_attr(not(msim), ignore)]
#[sim_test]
async fn test_gasless_split_coins_success() {
    let mut test_env = setup_gasless_env().await;
    let sender = test_env.get_sender(1);
    let recipient = test_env.get_sender(2);

    let coin_amount = 10_000u64;
    let send_amount = 3_000u64;

    let (_, coin_type, coin_refs) =
        setup_mintable_coin_env(&mut test_env, &[(coin_amount, sender)]).await;
    let coin_ref = coin_refs[0];

    // Build PTB:
    //   SplitCoins(coin, [send_amount]) → [split_coin]
    //   coin::send_funds(split_coin, recipient)
    //   coin::send_funds(coin, sender)
    let mut builder = ProgrammableTransactionBuilder::new();
    let coin_arg = builder.obj(ObjectArg::ImmOrOwnedObject(coin_ref)).unwrap();
    let amount_arg = builder.pure(send_amount).unwrap();
    let split_coin = builder.command(Command::SplitCoins(coin_arg, vec![amount_arg]));
    let recipient_arg = builder.pure(recipient).unwrap();
    builder.programmable_move_call(
        SUI_FRAMEWORK_PACKAGE_ID,
        Identifier::new("coin").unwrap(),
        Identifier::new("send_funds").unwrap(),
        vec![coin_type.clone()],
        vec![split_coin, recipient_arg],
    );
    let sender_arg = builder.pure(sender).unwrap();
    builder.programmable_move_call(
        SUI_FRAMEWORK_PACKAGE_ID,
        Identifier::new("coin").unwrap(),
        Identifier::new("send_funds").unwrap(),
        vec![coin_type.clone()],
        vec![coin_arg, sender_arg],
    );
    let tx_kind = TransactionKind::ProgrammableTransaction(builder.finish());
    let tx = test_env.gasless_transaction_data(tx_kind, sender, 0, 0);
    let (_, effects) = test_env.exec_tx_directly(tx).await.unwrap();

    assert!(
        effects.status().is_ok(),
        "Gasless SplitCoins should succeed when all coins are consumed: {:?}",
        effects.status()
    );
    assert_eq!(
        test_env.get_balance_ab(recipient, coin_type.clone()),
        send_amount
    );
    assert_eq!(
        test_env.get_balance_ab(sender, coin_type),
        coin_amount - send_amount
    );
    assert_eq!(effects.gas_cost_summary().net_gas_usage(), 0);

    test_env.trigger_reconfiguration().await;
}

#[cfg_attr(not(msim), ignore)]
#[sim_test]
async fn test_gasless_split_coins_leftover_fails() {
    let mut test_env = setup_gasless_env().await;
    let sender = test_env.get_sender(1);
    let recipient = test_env.get_sender(2);

    let coin_amount = 10_000u64;
    let send_amount = 3_000u64;

    let (_, coin_type, coin_refs) =
        setup_mintable_coin_env(&mut test_env, &[(coin_amount, sender)]).await;
    let coin_ref = coin_refs[0];

    // Build PTB that splits a coin but does NOT consume the split result,
    // leaving a written object that violates gasless post-execution checks.
    //   SplitCoins(coin, [send_amount]) → [split_coin]  (left over!)
    //   coin::send_funds(coin, recipient)
    let mut builder = ProgrammableTransactionBuilder::new();
    let coin_arg = builder.obj(ObjectArg::ImmOrOwnedObject(coin_ref)).unwrap();
    let amount_arg = builder.pure(send_amount).unwrap();
    builder.command(Command::SplitCoins(coin_arg, vec![amount_arg]));
    let recipient_arg = builder.pure(recipient).unwrap();
    builder.programmable_move_call(
        SUI_FRAMEWORK_PACKAGE_ID,
        Identifier::new("coin").unwrap(),
        Identifier::new("send_funds").unwrap(),
        vec![coin_type.clone()],
        vec![coin_arg, recipient_arg],
    );
    let tx_kind = TransactionKind::ProgrammableTransaction(builder.finish());
    let tx = test_env.gasless_transaction_data(tx_kind, sender, 0, 0);
    let (_, effects) = test_env.exec_tx_directly(tx).await.unwrap();

    assert!(
        effects.status().is_err(),
        "Gasless SplitCoins with leftover coin should fail, got: {:?}",
        effects.status()
    );

    test_env.trigger_reconfiguration().await;
}

#[cfg_attr(not(msim), ignore)]
#[sim_test]
async fn test_gasless_rejects_nonzero_budget() {
    let mut test_env = setup_gasless_env().await;
    let sender = test_env.get_sender(1);
    let recipient = test_env.get_sender(2);

    let coin_type = setup_custom_coin(&mut test_env, &[(10_000, sender)]).await;

    let tx_kind = build_send_funds_ptb(1000, coin_type, recipient);
    let tx = TransactionData::V1(TransactionDataV1 {
        kind: tx_kind,
        sender,
        gas_data: GasData {
            payment: vec![],
            owner: sender,
            price: 0,
            budget: 1000,
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
    let result = test_env.exec_tx_directly(tx).await;

    let err = result.unwrap_err();
    assert!(
        err.to_string().contains("gas_budget must be 0"),
        "Expected zero budget error, got: {err}"
    );

    test_env.trigger_reconfiguration().await;
}

/// Helper to set up a mintable coin, register it in the gasless token allowlist,
/// and mint coins to specified recipients. Returns (package_id, coin_type, coin_refs).
async fn setup_mintable_coin_env(
    test_env: &mut TestEnv,
    mints: &[(u64, SuiAddress)],
) -> (
    sui_types::base_types::ObjectID,
    TypeTag,
    Vec<sui_types::base_types::ObjectRef>,
) {
    let (publisher, package_id, coin_type, mut treasury_cap_ref) =
        test_env.setup_mintable_coin().await;

    transaction::add_gasless_token_for_testing(coin_type.to_canonical_string(true), 0);

    let mut coin_refs = Vec::new();
    for &(amount, recipient) in mints {
        let (new_tcap, coin_ref) = test_env
            .mint_coin(publisher, package_id, treasury_cap_ref, amount, recipient)
            .await;
        treasury_cap_ref = new_tcap;
        coin_refs.push(coin_ref);
    }
    (package_id, coin_type, coin_refs)
}

// ============================================================
// Coin<T> Input Tests
// ============================================================

#[cfg_attr(not(msim), ignore)]
#[sim_test]
async fn test_gasless_coin_input_success() {
    let mut test_env = setup_gasless_env().await;
    let sender = test_env.get_sender(1);
    let recipient = test_env.get_sender(2);

    let coin_amount = 5000u64;
    let (_, coin_type, coin_refs) =
        setup_mintable_coin_env(&mut test_env, &[(coin_amount, sender)]).await;
    let coin_ref = coin_refs[0];

    let sender_sui_before = test_env.get_sui_balance(sender).await;

    // Build PTB: coin::send_funds(coin, recipient)
    let mut builder = ProgrammableTransactionBuilder::new();
    let coin_arg = builder.obj(ObjectArg::ImmOrOwnedObject(coin_ref)).unwrap();
    let recipient_arg = builder.pure(recipient).unwrap();
    builder.programmable_move_call(
        SUI_FRAMEWORK_PACKAGE_ID,
        Identifier::new("coin").unwrap(),
        Identifier::new("send_funds").unwrap(),
        vec![coin_type.clone()],
        vec![coin_arg, recipient_arg],
    );
    let tx_kind = TransactionKind::ProgrammableTransaction(builder.finish());
    let tx = test_env.gasless_transaction_data(tx_kind, sender, 0, 0);
    let (_, effects) = test_env.exec_tx_directly(tx).await.unwrap();

    assert!(
        effects.status().is_ok(),
        "Gasless coin input should succeed: {:?}",
        effects.status()
    );

    // Verify recipient received funds
    assert_eq!(test_env.get_balance_ab(recipient, coin_type), coin_amount);

    // Storage rebate from destroyed coin should be absorbed, not returned to sender
    let gas_summary = effects.gas_cost_summary();
    assert_eq!(
        gas_summary.computation_cost, gas_summary.storage_rebate,
        "computation_cost should equal storage_rebate"
    );
    assert!(
        gas_summary.storage_rebate > 0,
        "storage_rebate should be positive when a coin is destroyed"
    );
    assert_eq!(gas_summary.net_gas_usage(), 0);

    let sender_sui_after = test_env.get_sui_balance(sender).await;
    assert_eq!(
        sender_sui_after, sender_sui_before,
        "Sender's SUI balance should not increase from storage rebate"
    );

    test_env.trigger_reconfiguration().await;
}

#[cfg_attr(not(msim), ignore)]
#[sim_test]
async fn test_gasless_coin_not_destroyed_fails() {
    let mut test_env = setup_gasless_env().await;
    let sender = test_env.get_sender(1);
    let recipient = test_env.get_sender(2);

    let (_, _coin_type, coin_refs) =
        setup_mintable_coin_env(&mut test_env, &[(5000, sender)]).await;
    let coin_ref = coin_refs[0];

    // Try to use TransferObjects (not whitelisted) with the coin
    let mut builder = ProgrammableTransactionBuilder::new();
    let coin_arg = builder.obj(ObjectArg::ImmOrOwnedObject(coin_ref)).unwrap();
    builder.transfer_arg(recipient, coin_arg);
    let tx_kind = TransactionKind::ProgrammableTransaction(builder.finish());

    let tx = test_env.gasless_transaction_data(tx_kind, sender, 0, 0);
    let result = test_env.exec_tx_directly(tx).await;

    let err = result.unwrap_err();
    assert!(
        err.to_string()
            .contains("is not supported in gasless transactions")
            || err
                .to_string()
                .contains("only support MoveCall, MergeCoins, and SplitCoins"),
        "Expected command whitelist error, got: {err}"
    );

    test_env.trigger_reconfiguration().await;
}

#[cfg_attr(not(msim), ignore)]
#[sim_test]
async fn test_gasless_rejects_non_coin_object_input() {
    let mut test_env = setup_gasless_env().await;

    // The TreasuryCap is a non-Coin object owned by the publisher.
    // Use it as an object input in a gasless tx to verify rejection.
    let (publisher, _package_id, coin_type, treasury_cap_ref) =
        test_env.setup_mintable_coin().await;
    transaction::add_gasless_token_for_testing(coin_type.to_canonical_string(true), 0);

    // Include TreasuryCap as an object input with an allowed gasless command.
    // The command validation passes, but object input check rejects the non-Coin.
    let mut builder = ProgrammableTransactionBuilder::new();
    let treasury_arg = builder
        .obj(ObjectArg::ImmOrOwnedObject(treasury_cap_ref))
        .unwrap();
    let recipient_arg = builder.pure(publisher).unwrap();
    builder.programmable_move_call(
        SUI_FRAMEWORK_PACKAGE_ID,
        Identifier::new("coin").unwrap(),
        Identifier::new("send_funds").unwrap(),
        vec![coin_type.clone()],
        vec![treasury_arg, recipient_arg],
    );
    let tx_kind = TransactionKind::ProgrammableTransaction(builder.finish());
    // Use publisher as sender since they own the TreasuryCap
    let tx = test_env.gasless_transaction_data(tx_kind, publisher, 0, 0);
    let result = test_env.exec_tx_directly(tx).await;

    let err = result.unwrap_err();
    assert!(
        err.to_string().contains("non-Coin object") || err.to_string().contains("Coin<T>"),
        "Expected non-Coin rejection, got: {err}"
    );

    test_env.trigger_reconfiguration().await;
}

#[cfg_attr(not(msim), ignore)]
#[sim_test]
async fn test_gasless_rejects_non_allowlisted_coin_type_input() {
    let mut test_env = setup_gasless_env().await;
    let sender = test_env.get_sender(1);
    let recipient = test_env.get_sender(2);

    // Setup mintable coin but do NOT register it in the fail point allowlist
    let (publisher, package_id, coin_type, treasury_cap_ref) = test_env.setup_mintable_coin().await;

    let (_, coin_ref) = test_env
        .mint_coin(publisher, package_id, treasury_cap_ref, 1000, sender)
        .await;

    let mut builder = ProgrammableTransactionBuilder::new();
    let coin_arg = builder.obj(ObjectArg::ImmOrOwnedObject(coin_ref)).unwrap();
    let recipient_arg = builder.pure(recipient).unwrap();
    builder.programmable_move_call(
        SUI_FRAMEWORK_PACKAGE_ID,
        Identifier::new("coin").unwrap(),
        Identifier::new("send_funds").unwrap(),
        vec![coin_type],
        vec![coin_arg, recipient_arg],
    );
    let tx_kind = TransactionKind::ProgrammableTransaction(builder.finish());
    let tx = test_env.gasless_transaction_data(tx_kind, sender, 0, 0);
    let result = test_env.exec_tx_directly(tx).await;

    let err = result.unwrap_err();
    assert!(
        err.to_string()
            .contains("is not currently allowed in gasless transactions")
            || err
                .to_string()
                .contains("is not supported in gasless transactions"),
        "Expected token allowlist or command allowlist error, got: {err}"
    );

    test_env.trigger_reconfiguration().await;
}

#[cfg_attr(not(msim), ignore)]
#[sim_test]
async fn test_gasless_coin_and_withdrawal_combined() {
    let mut test_env = setup_gasless_env().await;
    let sender = test_env.get_sender(1);
    let recipient = test_env.get_sender(2);

    let withdrawal_amount = 3000u64;
    let coin_amount = 2000u64;

    // Setup mintable coin package and mint a Coin<T> for sender
    let (publisher, package_id, coin_type, treasury_cap_ref) = test_env.setup_mintable_coin().await;
    let (_, coin_ref) = test_env
        .mint_coin(publisher, package_id, treasury_cap_ref, coin_amount, sender)
        .await;

    // Also set up a custom coin for address balance withdrawal
    let custom_coin_type = {
        let (_, ct) = test_env.setup_custom_coin().await;
        ct
    };
    let funder = test_env.get_sender(0);
    let tx = test_env
        .tx_builder(funder)
        .transfer_funds_to_address_balance(
            FundSource::address_fund_with_reservation(withdrawal_amount),
            vec![(withdrawal_amount, sender)],
            custom_coin_type.clone(),
        )
        .build();
    let (_, effects) = test_env.exec_tx_directly(tx).await.unwrap();
    assert!(effects.status().is_ok());

    // Register both coin types in the allowlist
    transaction::add_gasless_token_for_testing(coin_type.to_canonical_string(true), 0);
    transaction::add_gasless_token_for_testing(custom_coin_type.to_canonical_string(true), 0);

    // Build combined PTB: withdrawal(custom_coin) → redeem → send_funds,
    // coin input(mintable) → into_balance → send_funds
    let mut builder = ProgrammableTransactionBuilder::new();

    // Part 1: Withdrawal
    let withdraw_arg =
        FundsWithdrawalArg::balance_from_sender(withdrawal_amount, custom_coin_type.clone());
    let withdraw_arg = builder.funds_withdrawal(withdraw_arg).unwrap();
    let balance1 = builder.programmable_move_call(
        SUI_FRAMEWORK_PACKAGE_ID,
        Identifier::new("balance").unwrap(),
        Identifier::new("redeem_funds").unwrap(),
        vec![custom_coin_type.clone()],
        vec![withdraw_arg],
    );
    let recipient_arg1 = builder.pure(recipient).unwrap();
    builder.programmable_move_call(
        SUI_FRAMEWORK_PACKAGE_ID,
        Identifier::new("balance").unwrap(),
        Identifier::new("send_funds").unwrap(),
        vec![custom_coin_type],
        vec![balance1, recipient_arg1],
    );

    // Part 2: Coin input
    let coin_arg = builder.obj(ObjectArg::ImmOrOwnedObject(coin_ref)).unwrap();
    let recipient_arg2 = builder.pure(recipient).unwrap();
    builder.programmable_move_call(
        SUI_FRAMEWORK_PACKAGE_ID,
        Identifier::new("coin").unwrap(),
        Identifier::new("send_funds").unwrap(),
        vec![coin_type.clone()],
        vec![coin_arg, recipient_arg2],
    );

    let tx_kind = TransactionKind::ProgrammableTransaction(builder.finish());
    let tx = test_env.gasless_transaction_data(tx_kind, sender, 0, 0);
    let (_, effects) = test_env.exec_tx_directly(tx).await.unwrap();

    assert!(
        effects.status().is_ok(),
        "Gasless combined withdrawal + coin input should succeed: {:?}",
        effects.status()
    );

    assert_eq!(test_env.get_balance_ab(recipient, coin_type), coin_amount);

    test_env.trigger_reconfiguration().await;
}

#[cfg_attr(not(msim), ignore)]
#[sim_test]
async fn test_gasless_multiple_coin_inputs() {
    let mut test_env = setup_gasless_env().await;
    let sender = test_env.get_sender(1);
    let recipient = test_env.get_sender(2);

    let amount1 = 3000u64;
    let amount2 = 2000u64;

    let (_, coin_type, coin_refs) =
        setup_mintable_coin_env(&mut test_env, &[(amount1, sender), (amount2, sender)]).await;
    let coin1_ref = coin_refs[0];
    let coin2_ref = coin_refs[1];

    // Build PTB: MergeCoins(coin1, [coin2]), coin::send_funds(coin1, recipient)
    let mut builder = ProgrammableTransactionBuilder::new();
    let coin1_arg = builder.obj(ObjectArg::ImmOrOwnedObject(coin1_ref)).unwrap();
    let coin2_arg = builder.obj(ObjectArg::ImmOrOwnedObject(coin2_ref)).unwrap();
    builder.command(Command::MergeCoins(coin1_arg, vec![coin2_arg]));
    let recipient_arg = builder.pure(recipient).unwrap();
    builder.programmable_move_call(
        SUI_FRAMEWORK_PACKAGE_ID,
        Identifier::new("coin").unwrap(),
        Identifier::new("send_funds").unwrap(),
        vec![coin_type.clone()],
        vec![coin1_arg, recipient_arg],
    );
    let tx_kind = TransactionKind::ProgrammableTransaction(builder.finish());
    let tx = test_env.gasless_transaction_data(tx_kind, sender, 0, 0);
    let (_, effects) = test_env.exec_tx_directly(tx).await.unwrap();

    assert!(
        effects.status().is_ok(),
        "Gasless multiple coin inputs should succeed: {:?}",
        effects.status()
    );

    assert_eq!(
        test_env.get_balance_ab(recipient, coin_type),
        amount1 + amount2,
    );

    test_env.trigger_reconfiguration().await;
}

#[cfg_attr(not(msim), ignore)]
#[sim_test]
async fn test_gasless_coin_split_and_keep_change() {
    let mut test_env = setup_gasless_env().await;
    let sender = test_env.get_sender(1);
    let recipient = test_env.get_sender(2);

    let coin_amount = 10_000u64;
    let send_amount = 3_000u64;
    let change_amount = coin_amount - send_amount;

    let (_, coin_type, coin_refs) =
        setup_mintable_coin_env(&mut test_env, &[(coin_amount, sender)]).await;
    let coin_ref = coin_refs[0];

    // Build PTB:
    //   coin::into_balance(coin) → bal
    //   balance::split(&mut bal, send_amount) → part
    //   balance::send_funds(part, recipient)
    //   balance::send_funds(bal, sender)
    let mut builder = ProgrammableTransactionBuilder::new();
    let coin_arg = builder.obj(ObjectArg::ImmOrOwnedObject(coin_ref)).unwrap();
    let bal = builder.programmable_move_call(
        SUI_FRAMEWORK_PACKAGE_ID,
        Identifier::new("coin").unwrap(),
        Identifier::new("into_balance").unwrap(),
        vec![coin_type.clone()],
        vec![coin_arg],
    );
    let amount_arg = builder.pure(send_amount).unwrap();
    let part = builder.programmable_move_call(
        SUI_FRAMEWORK_PACKAGE_ID,
        Identifier::new("balance").unwrap(),
        Identifier::new("split").unwrap(),
        vec![coin_type.clone()],
        vec![bal, amount_arg],
    );
    let recipient_arg = builder.pure(recipient).unwrap();
    builder.programmable_move_call(
        SUI_FRAMEWORK_PACKAGE_ID,
        Identifier::new("balance").unwrap(),
        Identifier::new("send_funds").unwrap(),
        vec![coin_type.clone()],
        vec![part, recipient_arg],
    );
    let sender_arg = builder.pure(sender).unwrap();
    builder.programmable_move_call(
        SUI_FRAMEWORK_PACKAGE_ID,
        Identifier::new("balance").unwrap(),
        Identifier::new("send_funds").unwrap(),
        vec![coin_type.clone()],
        vec![bal, sender_arg],
    );
    let tx_kind = TransactionKind::ProgrammableTransaction(builder.finish());
    let tx = test_env.gasless_transaction_data(tx_kind, sender, 0, 0);
    let (_, effects) = test_env.exec_tx_directly(tx).await.unwrap();

    assert!(
        effects.status().is_ok(),
        "Gasless coin split should succeed: {:?}",
        effects.status()
    );

    assert_eq!(
        test_env.get_balance_ab(recipient, coin_type.clone()),
        send_amount
    );
    assert_eq!(test_env.get_balance_ab(sender, coin_type), change_amount);
    assert_eq!(effects.gas_cost_summary().net_gas_usage(), 0);

    test_env.trigger_reconfiguration().await;
}

#[cfg_attr(not(msim), ignore)]
#[sim_test]
async fn test_gasless_rate_limit_rejects() {
    let mut test_env = TestEnvBuilder::new()
        .with_proto_override_cb(Box::new(|_, mut cfg| {
            cfg.set_gasless_max_tps_for_testing(0);
            cfg
        }))
        .build()
        .await;

    let sender = test_env.get_sender(1);
    let recipient = test_env.get_sender(2);
    let coin_type = setup_custom_coin(&mut test_env, &[(10_000, sender)]).await;

    let tx_data = test_env.create_gasless_transaction(100, coin_type, sender, recipient, 0, 0);
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
            Some(Duration::from_secs(5)),
        )
        .await;

    assert!(result.is_err(), "Should fail due to rate limiting");
    let err = result.unwrap_err();
    let err_str = err.to_string();
    assert!(
        err_str.contains("ValidatorOverloaded") || err_str.contains("retry"),
        "Expected validator overloaded error, got: {err_str}"
    );

    test_env.trigger_reconfiguration().await;
}
