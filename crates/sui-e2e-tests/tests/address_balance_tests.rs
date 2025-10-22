// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use move_core_types::{identifier::Identifier, u256::U256};
use sui_json_rpc_types::SuiTransactionBlockEffectsAPI;
use sui_keys::keystore::AccountKeystore;
use sui_macros::*;
use sui_protocol_config::{ProtocolConfig, ProtocolVersion};
use sui_sdk::wallet_context::WalletContext;
use sui_types::{
    SUI_FRAMEWORK_PACKAGE_ID,
    accumulator_metadata::AccumulatorOwner,
    accumulator_root::{AccumulatorValue, U128},
    balance::Balance,
    base_types::{ObjectRef, SuiAddress},
    digests::{ChainIdentifier, CheckpointDigest},
    effects::TransactionEffectsAPI,
    gas_coin::GAS,
    programmable_transaction_builder::ProgrammableTransactionBuilder,
    storage::ChildObjectResolver,
    supported_protocol_versions::SupportedProtocolVersions,
    transaction::{Argument, Command, TransactionData, TransactionKind},
};
use test_cluster::TestClusterBuilder;

async fn get_sender_and_gas(context: &mut WalletContext) -> (SuiAddress, ObjectRef) {
    let sender = context
        .config
        .keystore
        .addresses()
        .first()
        .cloned()
        .unwrap();

    let gas = context
        .gas_objects(sender)
        .await
        .unwrap()
        .pop()
        .unwrap()
        .1
        .object_ref();

    (sender, gas)
}

fn create_transaction_with_expiration(
    sender: SuiAddress,
    gas_coin: ObjectRef,
    rgp: u64,
    min_epoch: Option<u64>,
    max_epoch: Option<u64>,
    chain_id: ChainIdentifier,
    nonce: u32,
) -> TransactionData {
    use sui_types::transaction::{GasData, TransactionDataV1, TransactionExpiration};
    let mut builder = ProgrammableTransactionBuilder::new();
    let amount = builder.pure(1000u64).unwrap();
    let coin = builder.command(Command::SplitCoins(Argument::GasCoin, vec![amount]));
    let Argument::Result(coin_idx) = coin else {
        panic!("coin is not a result");
    };
    let coin = Argument::NestedResult(coin_idx, 0);
    builder.transfer_arg(sender, coin);
    let tx = TransactionKind::ProgrammableTransaction(builder.finish());
    TransactionData::V1(TransactionDataV1 {
        kind: tx,
        sender,
        gas_data: GasData {
            payment: vec![gas_coin],
            owner: sender,
            price: rgp,
            budget: 10000000,
        },
        expiration: TransactionExpiration::ValidDuring {
            min_epoch,
            max_epoch,
            min_timestamp_seconds: None,
            max_timestamp_seconds: None,
            chain: chain_id,
            nonce,
        },
    })
}

// Test protocol gating of accumulator root creation. This test can be deleted after the feature
// is released.
#[cfg_attr(not(msim), ignore)]
#[sim_test]
async fn test_accumulators_root_created() {
    let _guard = ProtocolConfig::apply_overrides_for_testing(|version, mut cfg| {
        if version >= ProtocolVersion::MAX {
            cfg.create_root_accumulator_object_for_testing();
            // for some reason all 4 nodes are not reliably submitting capability messages
            cfg.set_buffer_stake_for_protocol_upgrade_bps_for_testing(0);
        }
        if version == ProtocolVersion::MAX_ALLOWED {
            cfg.enable_accumulators_for_testing();
        }
        cfg
    });

    let test_cluster = TestClusterBuilder::new()
        .with_supported_protocol_versions(SupportedProtocolVersions::new_for_testing(
            ProtocolVersion::MAX.as_u64(),
            ProtocolVersion::MAX_ALLOWED.as_u64(),
        ))
        .build()
        .await;

    // accumulator root is not created yet.
    test_cluster.fullnode_handle.sui_node.with(|node| {
        let state = node.state();
        assert!(
            !state
                .load_epoch_store_one_call_per_task()
                .accumulator_root_exists()
        );
    });

    test_cluster.trigger_reconfiguration().await;

    // accumulator root was created at the end of previous epoch,
    // but we didn't upgrade to the next protocol version yet.
    test_cluster.fullnode_handle.sui_node.with(|node| {
        let state = node.state();
        assert!(
            state
                .load_epoch_store_one_call_per_task()
                .accumulator_root_exists()
        );
        assert_eq!(
            state
                .load_epoch_store_one_call_per_task()
                .protocol_config()
                .version,
            ProtocolVersion::MAX
        );
    });

    // now we can upgrade to the next protocol version.
    test_cluster.trigger_reconfiguration().await;

    test_cluster.fullnode_handle.sui_node.with(|node| {
        let state = node.state();
        assert_eq!(
            state
                .load_epoch_store_one_call_per_task()
                .protocol_config()
                .version,
            ProtocolVersion::MAX_ALLOWED
        );
    });
}

// Test protocol gating of address balances. This test can be deleted after the feature
// is released.
#[cfg_attr(not(msim), ignore)]
#[sim_test]
async fn test_accumulators_disabled() {
    let _guard = ProtocolConfig::apply_overrides_for_testing(|version, mut cfg| {
        if version >= ProtocolVersion::MAX {
            cfg.create_root_accumulator_object_for_testing();
            // for some reason all 4 nodes are not reliably submitting capability messages
            cfg.set_buffer_stake_for_protocol_upgrade_bps_for_testing(0);
        }
        if version == ProtocolVersion::MAX_ALLOWED {
            cfg.enable_accumulators_for_testing();
        }
        cfg
    });

    let mut test_cluster = TestClusterBuilder::new()
        .with_supported_protocol_versions(SupportedProtocolVersions::new_for_testing(
            ProtocolVersion::MAX.as_u64(),
            ProtocolVersion::MAX_ALLOWED.as_u64(),
        ))
        .build()
        .await;
    let rgp = test_cluster.get_reference_gas_price().await;

    let (sender, gas) = get_sender_and_gas(&mut test_cluster.wallet).await;

    let recipient = SuiAddress::random_for_testing_only();

    // Withdraw must be rejected at signing.
    let withdraw_tx = withdraw_from_balance_tx(1000, sender, gas, rgp);
    let withdraw_tx = test_cluster.wallet.sign_transaction(&withdraw_tx).await;
    test_cluster
        .wallet
        .execute_transaction_may_fail(withdraw_tx)
        .await
        .unwrap_err();

    // Transfer fails at execution time
    let tx = make_send_to_account_tx(1000, recipient, sender, gas, rgp);

    let signed = test_cluster.wallet.sign_transaction(&tx).await;
    let effects = test_cluster
        .wallet
        .execute_transaction_may_fail(signed)
        .await
        .unwrap()
        .effects
        .unwrap();
    let gas = effects.gas_object().reference.to_object_ref();
    let status = effects.status().clone();
    assert!(status.is_err());

    // we reconfigure, and create the accumulator root at the end of this epoch.
    // but because the root did not exist during this epoch, we don't upgrade to
    // the next protocol version yet.
    test_cluster.trigger_reconfiguration().await;

    // Withdraw must still be rejected at signing.
    let withdraw_tx = withdraw_from_balance_tx(1000, sender, gas, rgp);
    let withdraw_tx = test_cluster.wallet.sign_transaction(&withdraw_tx).await;
    test_cluster
        .wallet
        .execute_transaction_may_fail(withdraw_tx)
        .await
        .unwrap_err();

    // transfer fails at execution time
    let tx = make_send_to_account_tx(1000, recipient, sender, gas, rgp);

    let signed = test_cluster.wallet.sign_transaction(&tx).await;
    let effects = test_cluster
        .wallet
        .execute_transaction_may_fail(signed)
        .await
        .unwrap()
        .effects
        .unwrap();
    let gas = effects.gas_object().reference.to_object_ref();
    let status = effects.status().clone();
    assert!(status.is_err());

    // after one more reconfig, we can upgrade to the next protocol version.
    test_cluster.trigger_reconfiguration().await;

    let tx = make_send_to_account_tx(1000, sender, sender, gas, rgp);

    let gas = test_cluster
        .sign_and_execute_transaction(&tx)
        .await
        .effects
        .unwrap()
        .gas_object()
        .reference
        .to_object_ref();

    test_cluster.fullnode_handle.sui_node.with(|node| {
        let state = node.state();
        let child_object_resolver = state.get_child_object_resolver().as_ref();
        verify_accumulator_exists(child_object_resolver, sender, 1000);
    });

    // Withdraw can succeed now
    let withdraw_tx = withdraw_from_balance_tx(1000, sender, gas, rgp);
    test_cluster
        .sign_and_execute_transaction(&withdraw_tx)
        .await;

    test_cluster.fullnode_handle.sui_node.with(|node| {
        let state = node.state();
        let child_object_resolver = state.get_child_object_resolver().as_ref();

        let sui_coin_type = Balance::type_tag(GAS::type_tag());
        assert!(
            !AccumulatorValue::exists(child_object_resolver, None, sender, &sui_coin_type).unwrap(),
            "Accumulator value should have been removed"
        );
    });

    // ensure that no conservation failures are detected during reconfig.
    test_cluster.trigger_reconfiguration().await;
}

#[sim_test]
async fn test_deposits() {
    let _guard = ProtocolConfig::apply_overrides_for_testing(|_, mut cfg| {
        cfg.create_root_accumulator_object_for_testing();
        cfg.enable_accumulators_for_testing();
        cfg
    });

    let mut test_cluster = TestClusterBuilder::new().build().await;
    let rgp = test_cluster.get_reference_gas_price().await;
    let context = &mut test_cluster.wallet;

    let (sender, gas) = get_sender_and_gas(context).await;

    let recipient = SuiAddress::random_for_testing_only();

    let tx = make_send_to_account_tx(1000, recipient, sender, gas, rgp);

    test_cluster.sign_and_execute_transaction(&tx).await;

    test_cluster.fullnode_handle.sui_node.with(|node| {
        let state = node.state();
        let child_object_resolver = state.get_child_object_resolver().as_ref();
        verify_accumulator_exists(child_object_resolver, recipient, 1000);
    });

    // ensure that no conservation failures are detected during reconfig.
    test_cluster.trigger_reconfiguration().await;
}

fn verify_accumulator_exists(
    child_object_resolver: &dyn ChildObjectResolver,
    owner: SuiAddress,
    expected_balance: u64,
) {
    let sui_coin_type = Balance::type_tag(GAS::type_tag());

    assert!(
        AccumulatorValue::exists(child_object_resolver, None, owner, &sui_coin_type).unwrap(),
        "Accumulator value should have been created"
    );

    let accumulator_object =
        AccumulatorValue::load_object(child_object_resolver, None, owner, &sui_coin_type)
            .expect("read cannot fail")
            .expect("accumulator should exist");

    assert!(
        accumulator_object
            .data
            .try_as_move()
            .unwrap()
            .type_()
            .is_efficient_representation()
    );

    let accumulator_value =
        AccumulatorValue::load(child_object_resolver, None, owner, &sui_coin_type)
            .expect("read cannot fail")
            .expect("accumulator should exist");

    assert_eq!(
        accumulator_value,
        AccumulatorValue::U128(U128 {
            value: expected_balance as u128
        }),
        "Accumulator value should be 1000"
    );

    assert!(
        AccumulatorOwner::exists(child_object_resolver, None, owner).unwrap(),
        "Owner object should have been created"
    );

    let owner = AccumulatorOwner::load(child_object_resolver, None, owner)
        .expect("read cannot fail")
        .expect("owner must exist");

    assert!(
        owner
            .metadata_exists(child_object_resolver, None, &sui_coin_type)
            .unwrap(),
        "Metadata object should have been created"
    );

    let _metadata = owner
        .load_metadata(child_object_resolver, None, &sui_coin_type)
        .unwrap();
}

#[sim_test]
async fn test_deposit_and_withdraw() {
    let _guard = ProtocolConfig::apply_overrides_for_testing(|_, mut cfg| {
        cfg.create_root_accumulator_object_for_testing();
        cfg.enable_accumulators_for_testing();
        cfg
    });

    let mut test_cluster = TestClusterBuilder::new().build().await;
    let rgp = test_cluster.get_reference_gas_price().await;
    let context = &mut test_cluster.wallet;

    let (sender, gas) = get_sender_and_gas(context).await;

    let tx = make_send_to_account_tx(1000, sender, sender, gas, rgp);
    let res = test_cluster.sign_and_execute_transaction(&tx).await;

    test_cluster.fullnode_handle.sui_node.with(|node| {
        let state = node.state();
        let child_object_resolver = state.get_child_object_resolver().as_ref();
        verify_accumulator_exists(child_object_resolver, sender, 1000);
    });

    let gas = res.effects.unwrap().gas_object().reference.to_object_ref();

    let tx = withdraw_from_balance_tx(1000, sender, gas, rgp);
    test_cluster.sign_and_execute_transaction(&tx).await;

    test_cluster.fullnode_handle.sui_node.with(|node| {
        let state = node.state();
        let child_object_resolver = state.get_child_object_resolver().as_ref();
        let sui_coin_type = Balance::type_tag(GAS::type_tag());

        assert!(
            !AccumulatorValue::exists(child_object_resolver, None, sender, &sui_coin_type).unwrap(),
            "Accumulator value should have been removed"
        );
        assert!(
            !AccumulatorOwner::exists(child_object_resolver, None, sender).unwrap(),
            "Owner object should have been removed"
        );
    });

    // ensure that no conservation failures are detected during reconfig.
    test_cluster.trigger_reconfiguration().await;
}

#[sim_test]
async fn test_deposit_and_withdraw_with_larger_reservation() {
    let _guard = ProtocolConfig::apply_overrides_for_testing(|_, mut cfg| {
        cfg.create_root_accumulator_object_for_testing();
        cfg.enable_accumulators_for_testing();
        cfg
    });

    let mut test_cluster = TestClusterBuilder::new().build().await;
    let rgp = test_cluster.get_reference_gas_price().await;
    let context = &mut test_cluster.wallet;

    let (sender, gas) = get_sender_and_gas(context).await;

    let tx = make_send_to_account_tx(1000, sender, sender, gas, rgp);
    let res = test_cluster.sign_and_execute_transaction(&tx).await;
    let gas = res.effects.unwrap().gas_object().reference.to_object_ref();

    // Withdraw 800 with a reservation of 1000 (larger than actual withdrawal)
    let tx = withdraw_from_balance_tx_with_reservation(800, 1000, sender, gas, rgp);
    test_cluster.sign_and_execute_transaction(&tx).await;

    test_cluster.fullnode_handle.sui_node.with(|node| {
        let state = node.state();
        let child_object_resolver = state.get_child_object_resolver().as_ref();
        // Verify that the accumulator still exists, as the entire balance was not withdrawn
        verify_accumulator_exists(child_object_resolver, sender, 200);
    });

    // ensure that no conservation failures are detected during reconfig.
    test_cluster.trigger_reconfiguration().await;
}

#[sim_test]
async fn test_withdraw_non_existent_balance() {
    let _guard = ProtocolConfig::apply_overrides_for_testing(|_, mut cfg| {
        cfg.create_root_accumulator_object_for_testing();
        cfg.enable_accumulators_for_testing();
        cfg
    });

    let mut test_cluster = TestClusterBuilder::new().build().await;
    let rgp = test_cluster.get_reference_gas_price().await;
    let context = &mut test_cluster.wallet;

    let (sender, gas) = get_sender_and_gas(context).await;

    // Settlement transaction fails with EInvalidSplitAmount because
    let tx = withdraw_from_balance_tx(1000, sender, gas, rgp);
    let signed_tx = test_cluster.sign_transaction(&tx).await;
    let (effects, _) = test_cluster
        .execute_transaction_return_raw_effects(signed_tx)
        .await
        .unwrap();

    assert!(
        effects.status().is_err(),
        "Expected transaction to fail due to underflow"
    );

    // ensure that no conservation failures are detected during reconfig.
    test_cluster.trigger_reconfiguration().await;
}

#[sim_test]
async fn test_withdraw_underflow() {
    let _guard = ProtocolConfig::apply_overrides_for_testing(|_, mut cfg| {
        cfg.create_root_accumulator_object_for_testing();
        cfg.enable_accumulators_for_testing();
        cfg
    });

    let mut test_cluster = TestClusterBuilder::new().build().await;
    let rgp = test_cluster.get_reference_gas_price().await;
    let context = &mut test_cluster.wallet;

    let (sender, gas) = get_sender_and_gas(context).await;

    // send 1000 from our gas coin to our balance
    let tx = make_send_to_account_tx(1000, sender, sender, gas, rgp);
    let res = test_cluster.sign_and_execute_transaction(&tx).await;
    let gas = res.effects.unwrap().gas_object().reference.to_object_ref();

    // Withdraw 1001 from balance
    // Settlement transaction fails due to underflow (MovePrimitiveRuntimeError)
    let tx = withdraw_from_balance_tx(1001, sender, gas, rgp);
    let signed_tx = test_cluster.sign_transaction(&tx).await;
    let (effects, _) = test_cluster
        .execute_transaction_return_raw_effects(signed_tx)
        .await
        .unwrap();

    assert!(
        effects.status().is_err(),
        "Expected transaction to fail due to underflow"
    );

    // ensure that no conservation failures are detected during reconfig.
    test_cluster.trigger_reconfiguration().await;
}

fn withdraw_from_balance_tx(
    amount: u64,
    sender: SuiAddress,
    gas: ObjectRef,
    rgp: u64,
) -> TransactionData {
    withdraw_from_balance_tx_with_reservation(amount, amount, sender, gas, rgp)
}

fn withdraw_from_balance_tx_with_reservation(
    amount: u64,
    reservation_amount: u64,
    sender: SuiAddress,
    gas: ObjectRef,
    rgp: u64,
) -> TransactionData {
    let mut builder = ProgrammableTransactionBuilder::new();

    // Add withdraw reservation
    let withdraw_arg = sui_types::transaction::FundsWithdrawalArg::balance_from_sender(
        reservation_amount,
        sui_types::type_input::TypeInput::from(sui_types::gas_coin::GAS::type_tag()),
    );
    let withdraw_arg = builder.funds_withdrawal(withdraw_arg).unwrap();

    let amount_arg = builder.pure(U256::from(amount)).unwrap();

    let split_withdraw_arg = builder.programmable_move_call(
        SUI_FRAMEWORK_PACKAGE_ID,
        Identifier::new("funds_accumulator").unwrap(),
        Identifier::new("withdrawal_split").unwrap(),
        vec!["0x2::balance::Balance<0x2::sui::SUI>".parse().unwrap()],
        vec![withdraw_arg, amount_arg],
    );

    let coin = builder.programmable_move_call(
        SUI_FRAMEWORK_PACKAGE_ID,
        Identifier::new("coin").unwrap(),
        Identifier::new("redeem_funds").unwrap(),
        vec!["0x2::sui::SUI".parse().unwrap()],
        vec![split_withdraw_arg],
    );

    builder.transfer_arg(sender, coin);

    let tx = TransactionKind::ProgrammableTransaction(builder.finish());
    TransactionData::new(tx, sender, gas, 10000000, rgp)
}

fn make_send_to_account_tx(
    amount: u64,
    recipient: SuiAddress,
    sender: SuiAddress,
    gas: ObjectRef,
    rgp: u64,
) -> TransactionData {
    let mut builder = ProgrammableTransactionBuilder::new();

    let amount = builder.pure(amount).unwrap();

    let recipient_arg = builder.pure(recipient).unwrap();

    let coin = builder.command(Command::SplitCoins(Argument::GasCoin, vec![amount]));
    let Argument::Result(coin_idx) = coin else {
        panic!("coin is not a result");
    };

    let coin = Argument::NestedResult(coin_idx, 0);

    builder.programmable_move_call(
        SUI_FRAMEWORK_PACKAGE_ID,
        Identifier::new("coin").unwrap(),
        Identifier::new("send_funds").unwrap(),
        vec!["0x2::sui::SUI".parse().unwrap()],
        vec![coin, recipient_arg],
    );

    let tx = TransactionKind::ProgrammableTransaction(builder.finish());
    TransactionData::new(tx, sender, gas, 10000000, rgp)
}

#[sim_test]
async fn test_empty_gas_payment_with_address_balance() {
    let _guard = ProtocolConfig::apply_overrides_for_testing(|_, mut cfg| {
        cfg.enable_address_balance_gas_payments_for_testing();
        cfg
    });

    let mut test_cluster = TestClusterBuilder::new().build().await;
    let rgp = test_cluster.get_reference_gas_price().await;
    let context = &mut test_cluster.wallet;

    let (sender, _) = get_sender_and_gas(context).await;

    let chain_id = test_cluster.get_chain_identifier();

    let tx = create_address_balance_transaction_with_chain_id(sender, rgp, chain_id);

    let signed_tx = test_cluster.sign_transaction(&tx).await;
    let result = test_cluster
        .execute_transaction_return_raw_effects(signed_tx)
        .await;

    let err = result.expect_err("Expected transaction to fail with GasBalanceTooLow");
    let err_str = format!("{:?}", err);
    assert!(
        err_str.contains("GasBalanceTooLow"),
        "Expected GasBalanceTooLow because gas payments with address balance not implemented, but got: {:?}",
        err
    );

    // ensure that no conservation failures are detected during reconfig.
    test_cluster.trigger_reconfiguration().await;
}

fn create_address_balance_transaction_with_chain_id(
    sender: SuiAddress,
    rgp: u64,
    chain_id: ChainIdentifier,
) -> TransactionData {
    use sui_types::transaction::{GasData, TransactionDataV1, TransactionExpiration};

    let mut builder = ProgrammableTransactionBuilder::new();

    let amount = builder.pure(1000u64).unwrap();

    let coin = builder.command(Command::SplitCoins(Argument::GasCoin, vec![amount]));
    let Argument::Result(coin_idx) = coin else {
        panic!("coin is not a result");
    };

    let coin = Argument::NestedResult(coin_idx, 0);
    builder.transfer_arg(sender, coin);

    let tx = TransactionKind::ProgrammableTransaction(builder.finish());

    TransactionData::V1(TransactionDataV1 {
        kind: tx,
        sender,
        gas_data: GasData {
            payment: vec![], // Empty payment to trigger address balance usage
            owner: sender,
            price: rgp,
            budget: 1000000,
        },
        expiration: TransactionExpiration::ValidDuring {
            min_epoch: Some(0),
            max_epoch: Some(0),
            min_timestamp_seconds: None,
            max_timestamp_seconds: None,
            chain: chain_id,
            nonce: 12345,
        },
    })
}

fn create_regular_gas_transaction_with_current_epoch(
    sender: SuiAddress,
    gas_coin: ObjectRef,
    rgp: u64,
    current_epoch: u64,
    chain_id: ChainIdentifier,
) -> TransactionData {
    use sui_types::transaction::{GasData, TransactionDataV1, TransactionExpiration};

    let mut builder = ProgrammableTransactionBuilder::new();

    let amount = builder.pure(1000u64).unwrap();
    let coin = builder.command(Command::SplitCoins(Argument::GasCoin, vec![amount]));
    let Argument::Result(coin_idx) = coin else {
        panic!("coin is not a result");
    };

    let coin = Argument::NestedResult(coin_idx, 0);
    builder.transfer_arg(sender, coin);

    let tx = TransactionKind::ProgrammableTransaction(builder.finish());

    TransactionData::V1(TransactionDataV1 {
        kind: tx,
        sender,
        gas_data: GasData {
            payment: vec![gas_coin], // Normal gas payment txn
            owner: sender,
            price: rgp,
            budget: 10000000,
        },
        expiration: TransactionExpiration::ValidDuring {
            min_epoch: Some(current_epoch),
            max_epoch: Some(current_epoch),
            min_timestamp_seconds: None,
            max_timestamp_seconds: None,
            chain: chain_id,
            nonce: 12345,
        },
    })
}

#[sim_test]
async fn test_regular_gas_payment_with_valid_during_current_epoch() {
    let mut test_cluster = TestClusterBuilder::new().build().await;
    let rgp = test_cluster.get_reference_gas_price().await;
    let chain_id = test_cluster.get_chain_identifier();
    let context = &mut test_cluster.wallet;

    let (sender, gas_coin) = get_sender_and_gas(context).await;
    let current_epoch = 0;

    let tx = create_regular_gas_transaction_with_current_epoch(
        sender,
        gas_coin,
        rgp,
        current_epoch,
        chain_id,
    );

    let signed_tx = test_cluster.sign_transaction(&tx).await;
    let (effects, _) = test_cluster
        .execute_transaction_return_raw_effects(signed_tx)
        .await
        .unwrap();

    assert!(
        effects.status().is_ok(),
        "Transaction should execute successfully. Error: {:?}",
        effects.status()
    );
}

#[sim_test]
async fn test_transaction_expired_too_early() {
    let mut test_cluster = TestClusterBuilder::new().build().await;
    let rgp = test_cluster.get_reference_gas_price().await;
    let chain_id = test_cluster.get_chain_identifier();
    let context = &mut test_cluster.wallet;

    let (sender, gas_coin) = get_sender_and_gas(context).await;
    let future_epoch = 10;

    let tx = create_regular_gas_transaction_with_current_epoch(
        sender,
        gas_coin,
        rgp,
        future_epoch,
        chain_id,
    );

    let signed_tx = test_cluster.sign_transaction(&tx).await;
    let result = test_cluster
        .execute_transaction_return_raw_effects(signed_tx)
        .await;

    match result {
        Err(err) => {
            let err_str = format!("{:?}", err);
            assert!(
                err_str.contains("TransactionExpired"),
                "Expected TransactionExpired error, got: {:?}",
                err
            );
        }
        Ok(_) => panic!("Transaction should be rejected when epoch is too early"),
    }
}

#[sim_test]
async fn test_transaction_expired_too_late() {
    let mut test_cluster = TestClusterBuilder::new().build().await;
    let rgp = test_cluster.get_reference_gas_price().await;
    let chain_id = test_cluster.get_chain_identifier();
    let context = &mut test_cluster.wallet;

    let (sender, gas_coin) = get_sender_and_gas(context).await;

    let past_epoch = 0;

    // trigger epoch 1
    test_cluster.trigger_reconfiguration().await;

    let tx = create_regular_gas_transaction_with_current_epoch(
        sender, gas_coin, rgp, past_epoch, chain_id,
    );

    let signed_tx = test_cluster.sign_transaction(&tx).await;
    let result = test_cluster
        .execute_transaction_return_raw_effects(signed_tx)
        .await;

    match result {
        Err(err) => {
            let err_str = format!("{:?}", err);
            assert!(
                err_str.contains("TransactionExpired"),
                "Expected TransactionExpired error, got: {:?}",
                err
            );
        }
        Ok(_) => panic!("Transaction should be rejected when epoch is too late"),
    }
}

#[sim_test]
async fn test_transaction_invalid_chain_id() {
    let mut test_cluster = TestClusterBuilder::new().build().await;
    let rgp = test_cluster.get_reference_gas_price().await;
    let context = &mut test_cluster.wallet;

    let (sender, gas_coin) = get_sender_and_gas(context).await;
    let current_epoch = 0;

    let wrong_chain_id = ChainIdentifier::from(CheckpointDigest::default());

    let tx = create_regular_gas_transaction_with_current_epoch(
        sender,
        gas_coin,
        rgp,
        current_epoch,
        wrong_chain_id,
    );

    let signed_tx = test_cluster.sign_transaction(&tx).await;
    let result = test_cluster
        .execute_transaction_return_raw_effects(signed_tx)
        .await;

    match result {
        Err(err) => {
            let err_str = format!("{:?}", err);
            assert!(
                err_str.contains("InvalidChainId"),
                "Expected InvalidChainId error, got: {:?}",
                err
            );
        }
        Ok(_) => panic!("Transaction should be rejected with invalid chain ID"),
    }
}

#[sim_test]
async fn test_transaction_expiration_min_none_max_some() {
    let mut test_cluster = TestClusterBuilder::new().build().await;
    let rgp = test_cluster.get_reference_gas_price().await;
    let chain_id = test_cluster.get_chain_identifier();
    let context = &mut test_cluster.wallet;

    let (sender, gas_coin) = get_sender_and_gas(context).await;
    let current_epoch = 0;

    let tx = create_transaction_with_expiration(
        sender,
        gas_coin,
        rgp,
        None,
        Some(current_epoch + 5),
        chain_id,
        12345,
    );

    let signed_tx = test_cluster.sign_transaction(&tx).await;
    let result = test_cluster
        .execute_transaction_return_raw_effects(signed_tx)
        .await;

    let err = result.expect_err("Transaction should be rejected when only max_epoch is specified");
    let err_str = format!("{:?}", err);
    assert!(
        err_str.contains("Both min_epoch and max_epoch must be specified and equal"),
        "Expected validation error 'Both min_epoch and max_epoch must be specified and equal', got: {:?}",
        err
    );
}

#[sim_test]
async fn test_transaction_expiration_edge_cases() {
    let mut test_cluster = TestClusterBuilder::new().build().await;
    let rgp = test_cluster.get_reference_gas_price().await;
    let chain_id = test_cluster.get_chain_identifier();

    let (sender, gas_coin1) = get_sender_and_gas(&mut test_cluster.wallet).await;
    let current_epoch = 0;

    // Test case 1: min_epoch = max_epoch (single epoch window)
    let tx1 = create_transaction_with_expiration(
        sender,
        gas_coin1,
        rgp,
        Some(current_epoch), // min_epoch: Some(current)
        Some(current_epoch), // max_epoch: Some(current) - single epoch window
        chain_id,
        100,
    );

    let result1 = test_cluster
        .execute_transaction_return_raw_effects(test_cluster.sign_transaction(&tx1).await)
        .await;
    assert!(
        result1.is_ok(),
        "Single epoch window transaction should succeed"
    );

    // Test case 2: Transaction with min_epoch in the future should be rejected as not yet valid
    let (_, gas_coin2) = get_sender_and_gas(&mut test_cluster.wallet).await;
    let tx2 = create_transaction_with_expiration(
        sender,
        gas_coin2,
        rgp,
        Some(current_epoch + 1),
        Some(current_epoch + 1),
        chain_id,
        200,
    );

    let result2 = test_cluster
        .execute_transaction_return_raw_effects(test_cluster.sign_transaction(&tx2).await)
        .await;
    let err2 = result2.expect_err("Transaction should be rejected when min_epoch is in the future");
    let err_str2 = format!("{:?}", err2);
    assert!(
        err_str2.contains("TransactionExpired"),
        "Expected TransactionExpired for future min_epoch, got: {:?}",
        err2
    );

    // Test case 3: min_epoch: Some(past), max_epoch: Some(past) - expired
    let (_, gas_coin3) = get_sender_and_gas(&mut test_cluster.wallet).await;

    // First trigger an epoch change to make current_epoch "past"
    test_cluster.trigger_reconfiguration().await;

    let tx3 = create_transaction_with_expiration(
        sender,
        gas_coin3,
        rgp,
        Some(0), // min_epoch: Some(past)
        Some(0), // max_epoch: Some(past) - now expired
        chain_id,
        300,
    );

    let result3 = test_cluster
        .execute_transaction_return_raw_effects(test_cluster.sign_transaction(&tx3).await)
        .await;
    match result3 {
        Err(err) => {
            let err_str = format!("{:?}", err);
            assert!(
                err_str.contains("TransactionExpired"),
                "Expected TransactionExpired for past max_epoch, got: {:?}",
                err
            );
        }
        Ok(_) => panic!("Transaction should be rejected when max_epoch is in the past"),
    }
}
