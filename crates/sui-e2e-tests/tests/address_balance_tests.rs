// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use move_core_types::{identifier::Identifier, u256::U256};
use sui_json_rpc_types::SuiTransactionBlockEffectsAPI;
use sui_macros::*;
use sui_protocol_config::{ProtocolConfig, ProtocolVersion};
use sui_types::{
    SUI_ACCUMULATOR_ROOT_OBJECT_ID, SUI_FRAMEWORK_PACKAGE_ID,
    accumulator_metadata::AccumulatorOwner,
    accumulator_root::AccumulatorValue,
    balance::Balance,
    base_types::{ObjectRef, SuiAddress},
    effects::{InputConsensusObject, TransactionEffectsAPI},
    gas_coin::GAS,
    programmable_transaction_builder::ProgrammableTransactionBuilder,
    supported_protocol_versions::SupportedProtocolVersions,
    transaction::{Argument, Command, TransactionData, TransactionKind},
};
use test_cluster::TestClusterBuilder;

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
        .with_num_validators(1)
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

    let test_cluster = TestClusterBuilder::new()
        .with_supported_protocol_versions(SupportedProtocolVersions::new_for_testing(
            ProtocolVersion::MAX.as_u64(),
            ProtocolVersion::MAX_ALLOWED.as_u64(),
        ))
        .build()
        .await;
    let rgp = test_cluster.get_reference_gas_price().await;
    let (sender, gas) = test_cluster.get_one_sender_and_gas().await;

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

    assert_eq!(test_cluster.get_address_balance(sender), 1000);

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

    let test_cluster = TestClusterBuilder::new().build().await;
    let rgp = test_cluster.get_reference_gas_price().await;
    let (sender, gas) = test_cluster.get_one_sender_and_gas().await;

    let recipient = SuiAddress::random_for_testing_only();

    let tx = make_send_to_account_tx(1000, recipient, sender, gas, rgp);

    let res = test_cluster.sign_and_execute_transaction(&tx).await;
    let gas = res.effects.unwrap().gas_object().reference.to_object_ref();

    tokio::time::sleep(std::time::Duration::from_secs(3)).await;

    let tx = make_send_to_account_tx(1000, recipient, sender, gas, rgp);

    test_cluster.sign_and_execute_transaction(&tx).await;

    assert_eq!(test_cluster.get_address_balance(recipient), 2000);

    test_cluster.fullnode_handle.sui_node.with(|node| {
        let state = node.state();
        let child_object_resolver = state.get_child_object_resolver().as_ref();

        let sui_coin_type = Balance::type_tag(GAS::type_tag());
        let accumulator_object =
            AccumulatorValue::load_object(child_object_resolver, None, recipient, &sui_coin_type)
                .expect("read cannot fail")
                .expect("accumulator should exist");
        let settlement_digest = accumulator_object.previous_transaction;
        let settlement_effects = state
            .get_transaction_cache_reader()
            .get_executed_effects(&settlement_digest)
            .expect("settlement digest should exist");
        let input_consensus_objects = settlement_effects.input_consensus_objects();
        input_consensus_objects.iter().find(|input_consensus_object| {
            matches!(input_consensus_object, InputConsensusObject::ReadOnly(obj_ref) if obj_ref.0 == SUI_ACCUMULATOR_ROOT_OBJECT_ID)
        }).expect("settlement should have accumulator root object as read-only input consensus object");
    });

    // ensure that no conservation failures are detected during reconfig.
    test_cluster.trigger_reconfiguration().await;
}

#[sim_test]
async fn test_multiple_settlement_txns() {
    let _guard = ProtocolConfig::apply_overrides_for_testing(|_, mut cfg| {
        cfg.enable_accumulators_for_testing();
        cfg.set_max_updates_per_settlement_txn_for_testing(3);
        cfg
    });

    let test_cluster = TestClusterBuilder::new().build().await;
    let rgp = test_cluster.get_reference_gas_price().await;
    let (sender, gas) = test_cluster.get_one_sender_and_gas().await;

    let recipient = SuiAddress::random_for_testing_only();

    let amounts_and_recipients = (0..20)
        .map(|_| (1u64, SuiAddress::random_for_testing_only()))
        .collect::<Vec<_>>();

    let tx = make_send_to_multi_account_tx(&amounts_and_recipients, sender, gas, rgp);

    let res = test_cluster.sign_and_execute_transaction(&tx).await;
    let gas = res.effects.unwrap().gas_object().reference.to_object_ref();

    tokio::time::sleep(std::time::Duration::from_secs(3)).await;

    let tx = make_send_to_account_tx(1000, recipient, sender, gas, rgp);

    test_cluster.sign_and_execute_transaction(&tx).await;

    for (amount, recipient) in amounts_and_recipients {
        assert_eq!(test_cluster.get_address_balance(recipient), amount);
    }

    // ensure that no conservation failures are detected during reconfig.
    test_cluster.trigger_reconfiguration().await;
}

#[sim_test]
async fn test_deposit_and_withdraw() {
    let _guard = ProtocolConfig::apply_overrides_for_testing(|_, mut cfg| {
        cfg.create_root_accumulator_object_for_testing();
        cfg.enable_accumulators_for_testing();
        cfg
    });

    let test_cluster = TestClusterBuilder::new().build().await;
    let rgp = test_cluster.get_reference_gas_price().await;
    let (sender, gas) = test_cluster.get_one_sender_and_gas().await;

    let tx = make_send_to_account_tx(1000, sender, sender, gas, rgp);
    let res = test_cluster.sign_and_execute_transaction(&tx).await;

    assert_eq!(test_cluster.get_address_balance(sender), 1000);

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

    let test_cluster = TestClusterBuilder::new().build().await;
    let rgp = test_cluster.get_reference_gas_price().await;
    let (sender, gas) = test_cluster.get_one_sender_and_gas().await;

    let tx = make_send_to_account_tx(1000, sender, sender, gas, rgp);
    let res = test_cluster.sign_and_execute_transaction(&tx).await;
    let gas = res.effects.unwrap().gas_object().reference.to_object_ref();

    // Withdraw 800 with a reservation of 1000 (larger than actual withdrawal)
    let tx = withdraw_from_balance_tx_with_reservation(800, 1000, sender, gas, rgp);
    test_cluster.sign_and_execute_transaction(&tx).await;

    assert_eq!(test_cluster.get_address_balance(sender), 200);

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

    let test_cluster = TestClusterBuilder::new().build().await;
    let rgp = test_cluster.get_reference_gas_price().await;
    let (sender, gas) = test_cluster.get_one_sender_and_gas().await;

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

    let test_cluster = TestClusterBuilder::new().build().await;
    let rgp = test_cluster.get_reference_gas_price().await;
    let (sender, gas) = test_cluster.get_one_sender_and_gas().await;

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
    make_send_to_multi_account_tx(&[(amount, recipient)], sender, gas, rgp)
}

fn make_send_to_multi_account_tx(
    amounts_and_recipients: &[(u64, SuiAddress)],
    sender: SuiAddress,
    gas: ObjectRef,
    rgp: u64,
) -> TransactionData {
    let mut builder = ProgrammableTransactionBuilder::new();

    for (amount, recipient) in amounts_and_recipients {
        let amount_arg = builder.pure(*amount).unwrap();
        let recipient_arg = builder.pure(recipient).unwrap();
        let coin = builder.command(Command::SplitCoins(Argument::GasCoin, vec![amount_arg]));

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
    }

    let tx = TransactionKind::ProgrammableTransaction(builder.finish());
    TransactionData::new(tx, sender, gas, 10000000, rgp)
}

#[sim_test]
async fn test_multiple_deposits_merged_in_effects() {
    let _guard = ProtocolConfig::apply_overrides_for_testing(|_, mut cfg| {
        cfg.create_root_accumulator_object_for_testing();
        cfg.enable_accumulators_for_testing();
        cfg
    });

    let test_cluster = TestClusterBuilder::new().build().await;
    let rgp = test_cluster.get_reference_gas_price().await;
    let initial_balance = 10_000;
    let sender = test_cluster
        .get_address_1_with_balance(initial_balance)
        .await;
    let gas = test_cluster
        .wallet
        .get_one_gas_object_owned_by_address(sender)
        .await
        .unwrap()
        .unwrap();

    let deposit_amounts = vec![1000u64, 2000u64, 3000u64];
    let withdraw_amounts = vec![500u64, 1500u64];

    let mut builder = ProgrammableTransactionBuilder::new();

    for amount in &deposit_amounts {
        let amount_arg = builder.pure(*amount).unwrap();
        let recipient_arg = builder.pure(sender).unwrap();
        let coin = builder.command(Command::SplitCoins(Argument::GasCoin, vec![amount_arg]));

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
    }

    for amount in &withdraw_amounts {
        let withdraw_arg = sui_types::transaction::FundsWithdrawalArg::balance_from_sender(
            *amount,
            sui_types::type_input::TypeInput::from(sui_types::gas_coin::GAS::type_tag()),
        );
        let withdraw_arg = builder.funds_withdrawal(withdraw_arg).unwrap();

        let amount_arg = builder.pure(U256::from(*amount)).unwrap();

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
    }

    let tx = TransactionKind::ProgrammableTransaction(builder.finish());
    let tx_data = TransactionData::new(tx, sender, gas, 10000000, rgp);

    let signed_tx = test_cluster.wallet.sign_transaction(&tx_data).await;
    let (effects, _) = test_cluster
        .execute_transaction_return_raw_effects(signed_tx)
        .await
        .unwrap();

    assert!(
        effects.status().is_ok(),
        "Transaction should succeed, got: {:?}",
        effects.status()
    );

    let acc_events = effects.accumulator_events();
    assert_eq!(
        acc_events.len(),
        1,
        "Should have exactly 1 accumulator event (merged), got: {}",
        acc_events.len()
    );

    let event = &acc_events[0];
    assert_eq!(
        event.write.address.address, sender,
        "Accumulator event should be for the sender"
    );

    let deposit_total: u64 = deposit_amounts.iter().sum();
    let withdraw_total: u64 = withdraw_amounts.iter().sum();
    let expected_net = deposit_total - withdraw_total;

    match &event.write.value {
        sui_types::effects::AccumulatorValue::Integer(value) => {
            assert_eq!(
                *value, expected_net,
                "Merged accumulator value should be {} (deposits {} - withdrawals {}), got {}",
                expected_net, deposit_total, withdraw_total, value
            );
        }
        _ => panic!("Expected Integer accumulator value"),
    }

    match &event.write.operation {
        sui_types::effects::AccumulatorOperation::Merge => {}
        _ => panic!("Expected Merge operation since deposits > withdrawals"),
    }

    let expected_final_balance = initial_balance + expected_net;
    assert_eq!(
        test_cluster.get_address_balance(sender),
        expected_final_balance
    );

    test_cluster.trigger_reconfiguration().await;
}
