// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use move_core_types::ident_str;
use sui_macros::*;
use sui_simulator::has_mainnet_protocol_config_override;
use sui_test_transaction_builder::{FundSource, TestTransactionBuilder};
use sui_types::{
    base_types::{FullObjectRef, ObjectID, ObjectRef, SequenceNumber, SuiAddress},
    coin_reservation::ParsedObjectRefWithdrawal,
    digests::CheckpointDigest,
    effects::TransactionEffectsAPI,
    programmable_transaction_builder::ProgrammableTransactionBuilder,
    transaction::{
        Argument, CallArg, Command, GasData, ObjectArg, TransactionData, TransactionDataAPI,
        TransactionDataV1, TransactionExpiration, TransactionKind,
    },
};
use test_cluster::addr_balance_test_env::{TestEnv, TestEnvBuilder, get_sui_accumulator_object_id};

#[sim_test]
async fn test_coin_reservation_validation() {
    if has_mainnet_protocol_config_override() {
        return;
    }
    let mut test_env = TestEnvBuilder::new()
        .with_proto_override_cb(Box::new(|_, mut cfg| {
            cfg.enable_coin_reservation_for_testing();
            cfg
        }))
        .build()
        .await;

    let (sender1, _) = test_env.get_sender_and_gas(0);
    let (sender2, _) = test_env.get_sender_and_gas(1);

    // send 1000 gas from the gas coins to the balances
    test_env.fund_one_address_balance(sender1, 1000).await;

    // refresh the gas object
    let (sender1, gas1) = test_env.get_sender_and_gas(0);

    // Verify transaction is rejected if it reserves more than the available balance
    {
        let coin_res = test_env.encode_coin_reservation(sender1, 0, 1001);

        let err = test_env
            .transfer_from_coin_to_address_balance(sender1, coin_res, vec![(1, sender1)])
            .await
            .unwrap_err();
        assert!(err.to_string().contains("Insufficient address balance"));
    }

    // Verify transaction is rejected if it uses a bogus accumulator object id.
    {
        let random_id = ObjectID::random();
        let coin_res = ParsedObjectRefWithdrawal::new(random_id, 0, 1001)
            .encode(SequenceNumber::new(), test_env.chain_id);

        let err = test_env
            .transfer_from_coin_to_address_balance(sender1, coin_res, vec![(1, sender1)])
            .await
            .unwrap_err();

        assert!(
            err.to_string()
                .contains(format!("object id {} not found", random_id).as_str())
        );
    }

    // Verify transaction is rejected if it is not valid in the current epoch.
    {
        let coin_res = test_env.encode_coin_reservation(sender1, 1, 100);

        let err = test_env
            .transfer_from_coin_to_address_balance(sender1, coin_res, vec![(1, sender1)])
            .await
            .unwrap_err();
        assert!(err.to_string().contains("Transaction Expired"));
    }

    // Verify transaction is rejected if the reservation amount is zero.
    {
        let coin_res = test_env.encode_coin_reservation(sender1, 0, 0);

        let err = test_env
            .transfer_from_coin_to_address_balance(sender1, coin_res, vec![(1, sender1)])
            .await
            .unwrap_err();
        assert!(
            err.to_string()
                .contains("reservation amount must be non-zero")
        );
    }

    // Verify the transaction is rejected if the accumulator object is not owned by the sender.
    {
        let coin_res = test_env.encode_coin_reservation(sender1, 0, 100);

        let recipient = SuiAddress::random_for_testing_only();
        let err = test_env
            .transfer_from_coin_to_address_balance(sender2, coin_res, vec![(1, recipient)])
            .await
            .unwrap_err();
        assert!(
            err.to_string()
                .contains(format!("is owned by {}, not sender {}", sender1, sender2).as_str())
        );
    }

    // Verify that invalid epoch for coin reservation in gas is rejected.
    {
        let coin_reservation_gas = test_env.encode_coin_reservation(sender1, 3, 10000000);

        let tx = test_env
            .tx_builder_with_gas(sender1, coin_reservation_gas)
            .transfer_sui_to_address_balance(FundSource::coin(gas1), vec![(1, sender1)])
            .build();
        let err = test_env.exec_tx_directly(tx).await.unwrap_err();

        assert!(err.to_string().contains("Transaction Expired"));
    }

    // Verify that zero amount for coin reservation in gas is rejected.
    {
        let coin_reservation_gas = test_env.encode_coin_reservation(sender1, 0, 0);

        let tx = test_env
            .tx_builder_with_gas(sender1, coin_reservation_gas)
            .transfer_sui_to_address_balance(FundSource::coin(gas1), vec![(1, sender1)])
            .build();
        let err = test_env.exec_tx_directly(tx).await.unwrap_err();

        assert!(
            err.to_string()
                .contains("reservation amount must be non-zero")
        );
    }

    // Verify gas budget is enforced with coin reservations.
    {
        let coin_reservation_gas = test_env.encode_coin_reservation(sender1, 0, 100);

        let tx = test_env
            .tx_builder_with_gas(sender1, coin_reservation_gas)
            .transfer_sui_to_address_balance(FundSource::coin(gas1), vec![(1, sender1)])
            .build();
        let err = test_env.exec_tx_directly(tx).await.unwrap_err();

        let err_str = err.to_string();
        assert!(
            err_str.contains("Balance of gas object 100 is lower than the needed amount"),
            "Expected 'Balance of gas object 100 is lower than the needed amount' but got: {}",
            err_str
        );
    }

    // Verify that total reservation limit is enforced for coin reservations, including gas reservations.
    {
        // 1 gas reservation
        let gas_reservation = test_env.encode_coin_reservation(sender1, 0, 10000000);

        // plus 1 regular reservation
        let mut tx_builder = TestTransactionBuilder::new(sender1, gas_reservation, test_env.rgp)
            .transfer_sui_to_address_balance(
                FundSource::address_fund_with_reservation(1),
                vec![(1, sender1)],
            );

        // plus 9 coin reservations
        for _ in 0..9 {
            let random_object_id = ObjectID::random();

            let coin = ParsedObjectRefWithdrawal::new(random_object_id, 0, 100)
                .encode(SequenceNumber::new(), test_env.chain_id);

            tx_builder = tx_builder.transfer(FullObjectRef::from_fastpath_ref(coin), sender1);
        }

        let tx = tx_builder.build();

        let err = test_env.exec_tx_directly(tx).await.unwrap_err();

        assert!(
            err.to_string()
                .contains("Maximum number of balance withdraw reservations is 10")
        );
    }

    // Verify that non-SUI coin reservations cannot be used as gas.
    {
        // Publish a trusted coin and mint some to the sender's address balance.
        let (_, coin_type) = test_env
            .publish_and_mint_trusted_coin(sender1, 10_000_000_000)
            .await;

        // Make a coin reservation for the non-SUI coin.
        let coin_reservation =
            test_env.encode_coin_reservation_for_type(sender1, 0, 10_000_000_000, coin_type);

        // Attempt to use it as gas - should be rejected since gas must be SUI.
        let tx = test_env
            .tx_builder_with_gas(sender1, coin_reservation)
            .build();
        let err = test_env.exec_tx_directly(tx).await.unwrap_err();
        assert!(
            err.to_string()
                .contains("Gas object is not an owned object with owner")
        );
    }

    test_env.cluster.trigger_reconfiguration().await;
}

#[sim_test]
async fn test_coin_reservation_gating() {
    if has_mainnet_protocol_config_override() {
        return;
    }
    // Explicitly disable coin reservation flags to test the gating logic
    let mut test_env = TestEnvBuilder::new()
        .with_proto_override_cb(Box::new(|_, mut cfg| {
            cfg.disable_coin_reservation_for_testing();
            cfg
        }))
        .build()
        .await;

    let sender = test_env.get_sender(0);

    // Verify transaction is rejected if coin reservation is not enabled.
    {
        let coin_reservation = test_env.encode_coin_reservation(sender, 0, 1);

        let err = test_env
            .transfer_from_coin_to_address_balance(sender, coin_reservation, vec![(1, sender)])
            .await
            .unwrap_err();
        assert!(
            err.to_string()
                .contains("coin reservation backward compatibility layer is not enabled")
        );
    }

    test_env.cluster.trigger_reconfiguration().await;
}

#[sim_test]
async fn test_valid_coin_reservation_transfers() {
    if has_mainnet_protocol_config_override() {
        return;
    }
    let mut test_env = TestEnvBuilder::new()
        .with_proto_override_cb(Box::new(|_, mut cfg| {
            cfg.enable_coin_reservation_for_testing();
            cfg
        }))
        .build()
        .await;

    let (sender, _) = test_env.get_sender_and_gas(0);

    test_env.fund_one_address_balance(sender, 1000).await;

    let coin_reservation = test_env.encode_coin_reservation(sender, 0, 100);

    let recipient = SuiAddress::random_for_testing_only();

    // Transfer the entire "coin" to the recipient
    let (_, effects) = test_env
        .transfer_coin_to_address_balance(sender, coin_reservation, recipient)
        .await
        .unwrap();
    assert!(effects.status().is_ok());

    // Transfer a portion of the coin reservation to the recipient
    let (_, effects) = test_env
        .transfer_from_coin_to_address_balance(sender, coin_reservation, vec![(1, recipient)])
        .await
        .unwrap();
    assert!(effects.status().is_ok());

    test_env
        .cluster
        .wait_for_tx_settlement(std::slice::from_ref(effects.transaction_digest()))
        .await;

    // ensure both balances arrived at the recipient
    let recipient_balance = test_env.get_sui_balance(recipient).await;
    // 100 from coin transfer, 1 from coin reservation
    assert_eq!(recipient_balance, 100 + 1);

    test_env.cluster.trigger_reconfiguration().await;
}

#[sim_test]
async fn test_valid_coin_reservation_gas_payments() {
    if has_mainnet_protocol_config_override() {
        return;
    }
    let mut test_env = TestEnvBuilder::new()
        .with_proto_override_cb(Box::new(|_, mut cfg| {
            cfg.enable_coin_reservation_for_testing();
            cfg
        }))
        .build()
        .await;

    let sender = test_env.get_sender(0);

    let budget = 5000000000;

    test_env
        .fund_one_address_balance(sender, budget + 100)
        .await;

    let transfer_payment = test_env.encode_coin_reservation(sender, 0, 1);
    let gas_payment = test_env.encode_coin_reservation(sender, 0, budget);

    let recipient = SuiAddress::random_for_testing_only();

    let initial_sender_balance = test_env.get_sui_balance_ab(sender);

    let tx = TestTransactionBuilder::new(sender, gas_payment, test_env.rgp)
        .transfer_sui_to_address_balance(FundSource::coin(transfer_payment), vec![(1, recipient)])
        .build();
    let (_, effects) = test_env.exec_tx_directly(tx).await.unwrap();

    assert!(effects.status().is_ok());
    let gas_charge = effects.gas_cost_summary().gas_used() as u128;

    // ensure both balances arrived at the recipient
    let recipient_balance = test_env.get_sui_balance_ab(recipient);

    // 1 MIST transferred.
    assert_eq!(recipient_balance, 1);

    // Sender should have lost the gas charge and the 1 MIST transferred.
    let final_sender_balance = test_env.get_sui_balance_ab(sender);
    assert_eq!(
        final_sender_balance,
        initial_sender_balance - gas_charge as u64 - 1
    );

    test_env.cluster.trigger_reconfiguration().await;
}

#[sim_test]
async fn test_gas_coin_callarg_with_coin_reservation_gas() {
    if has_mainnet_protocol_config_override() {
        return;
    }
    // Tests GasCoin materialization when gas is paid purely via coin reservation.
    // The coin reservation must include both the gas budget AND any amount the
    // transaction wants to use via GasCoin. The materialized GasCoin will have
    // balance = reservation_amount - gas_budget.

    let mut test_env = TestEnvBuilder::new()
        .with_proto_override_cb(Box::new(|_, mut cfg| {
            cfg.enable_coin_reservation_for_testing();
            cfg
        }))
        .build()
        .await;

    let sender = test_env.get_sender(0);
    let budget = 5_000_000_000u64;
    let available_for_gas_coin = 100u64;

    // Fund sender with enough for two transactions (the failing one still consumes gas)
    test_env
        .fund_one_address_balance(sender, 2 * budget + available_for_gas_coin)
        .await;

    // First test: exceeding the materialized GasCoin's available balance should fail.
    // The reservation includes budget + 100, so the materialized GasCoin has 100 mist.
    {
        let gas_reservation =
            test_env.encode_coin_reservation(sender, 0, budget + available_for_gas_coin);
        let recipient = SuiAddress::random_for_testing_only();

        // Try to transfer 200 mist, but only 100 is available in the materialized GasCoin.
        let excessive_transfer = 200u64;
        assert!(excessive_transfer > available_for_gas_coin);

        let tx = TestTransactionBuilder::new(sender, gas_reservation, test_env.rgp)
            .transfer_sui(Some(excessive_transfer), recipient)
            .build();
        let (_, effects) = test_env.exec_tx_directly(tx).await.unwrap();
        assert!(format!("{:?}", effects.status()).contains("InsufficientCoinBalance"));
    }

    // Second test: transferring exactly the available amount should succeed.
    {
        let gas_reservation =
            test_env.encode_coin_reservation(sender, 0, budget + available_for_gas_coin);
        let recipient = SuiAddress::random_for_testing_only();

        let initial_sender_balance = test_env.get_sui_balance_ab(sender);

        // Use transfer_sui which internally does SplitCoins(GasCoin, [amount]) + TransferObjects.
        let tx = TestTransactionBuilder::new(sender, gas_reservation, test_env.rgp)
            .transfer_sui(Some(available_for_gas_coin), recipient)
            .build();
        let (_, effects) = test_env.exec_tx_directly(tx).await.unwrap();
        assert!(
            effects.status().is_ok(),
            "Transaction failed: {:?}",
            effects.status()
        );

        let gas_charge = effects.gas_cost_summary().gas_used();

        // Verify the sender's address balance is decreased by gas charges + transfer.
        let final_sender_balance = test_env.get_sui_balance_ab(sender);
        assert_eq!(
            final_sender_balance,
            initial_sender_balance - gas_charge - available_for_gas_coin
        );

        // Verify the recipient received a Coin object with the transfer amount.
        let created = effects.created();
        assert_eq!(created.len(), 1, "Expected exactly one created object");
        let created_coin_id = created[0].0.0;
        let coin_balance = test_env.get_coin_balance(created_coin_id).await;
        assert_eq!(coin_balance, available_for_gas_coin);
    }

    test_env.cluster.trigger_reconfiguration().await;
}

#[sim_test]
async fn test_gas_coin_by_ref_with_coin_reservation_gas() {
    if has_mainnet_protocol_config_override() {
        return;
    }
    // Tests GasCoin materialization when GasCoin is used by reference (not consumed).
    // Uses transfer_sui_to_address_balance which splits from GasCoin and sends to
    // address balance, leaving the GasCoin still alive (not transferred by value).

    let mut test_env = TestEnvBuilder::new()
        .with_proto_override_cb(Box::new(|_, mut cfg| {
            cfg.enable_coin_reservation_for_testing();
            cfg
        }))
        .build()
        .await;

    let sender = test_env.get_sender(0);
    let budget = 5_000_000_000;
    let transfer_amount = 100u64;

    // Fund sender with enough for gas budget + transfer amount
    test_env
        .fund_one_address_balance(sender, budget + transfer_amount)
        .await;

    // The gas reservation must include the transfer amount
    let gas_reservation = test_env.encode_coin_reservation(sender, 0, budget + transfer_amount);
    let recipient = SuiAddress::random_for_testing_only();

    let initial_sender_balance = test_env.get_sui_balance_ab(sender);

    // Use transfer_sui_to_address_balance which does:
    // - SplitCoins(GasCoin, [amount]) -> Balance
    // - send_funds(Balance, recipient)
    // The GasCoin is borrowed mutably but not consumed by value.
    let tx = TestTransactionBuilder::new(sender, gas_reservation, test_env.rgp)
        .transfer_sui_to_address_balance(
            FundSource::Coin(gas_reservation),
            vec![(transfer_amount, recipient)],
        )
        .build();
    let (_, effects) = test_env.exec_tx_directly(tx).await.unwrap();
    assert!(
        effects.status().is_ok(),
        "Transaction failed: {:?}",
        effects.status()
    );

    let gas_charge = effects.gas_cost_summary().gas_used();

    // Verify the sender's address balance is decreased by gas charges + transfer
    let final_sender_balance = test_env.get_sui_balance_ab(sender);
    assert_eq!(
        final_sender_balance,
        initial_sender_balance - gas_charge - transfer_amount
    );

    // Verify the recipient's address balance received the transfer
    let recipient_balance = test_env.get_sui_balance_ab(recipient);
    assert_eq!(recipient_balance, transfer_amount);

    test_env.cluster.trigger_reconfiguration().await;
}

#[sim_test]
async fn test_gas_coin_callarg_with_mixed_gas() {
    if has_mainnet_protocol_config_override() {
        return;
    }
    // Test that GasCoin arg works when gas is [real, fake].
    // The real coin becomes the smashed gas coin, so GasCoin should work.

    let mut test_env = TestEnvBuilder::new()
        .with_proto_override_cb(Box::new(|_, mut cfg| {
            cfg.enable_coin_reservation_for_testing();
            cfg
        }))
        .build()
        .await;

    let (sender, _) = test_env.get_sender_and_gas(0);

    // Fund address balance so coin reservation works
    test_env
        .fund_one_address_balance(sender, 10_000_000_000)
        .await;

    // Get a real coin to use as the first gas payment
    let (_, mut all_gas) = test_env.get_sender_and_all_gas(0);
    let real_coin = all_gas.pop().unwrap();
    let real_coin_balance = test_env.get_coin_balance(real_coin.0).await;

    // Create a coin reservation (fake coin) to use as the second gas payment
    let fake_coin_amount = 5_000_000_000u64;
    let fake_coin = test_env.encode_coin_reservation(sender, 0, fake_coin_amount);

    let initial_ab_balance = test_env.get_sui_balance_ab(sender);
    let recipient = SuiAddress::random_for_testing_only();
    let transfer_amount = 100u64;

    // Use transfer_sui which internally does SplitCoins(GasCoin, [amount]) + TransferObjects.
    // With [real, fake] gas, the real coin is the smashed gas coin, so GasCoin should work.
    let tx = test_env
        .tx_builder_with_gas_objects(sender, vec![real_coin, fake_coin])
        .transfer_sui(Some(transfer_amount), recipient)
        .build();

    // Verify that Argument::GasCoin is present in the transaction commands
    let TransactionKind::ProgrammableTransaction(pt) = tx.kind() else {
        panic!("Expected ProgrammableTransaction");
    };
    let has_gas_coin_arg = pt
        .commands
        .iter()
        .any(|cmd| matches!(cmd, Command::SplitCoins(Argument::GasCoin, _)));
    assert!(has_gas_coin_arg, "Transaction should use Argument::GasCoin");

    let (_, effects) = test_env.exec_tx_directly(tx).await.unwrap();
    assert!(effects.status().is_ok());

    let gas_charge = effects.gas_cost_summary().gas_used();

    // The real coin should still exist (it's the gas coin, mutated not deleted)
    assert!(effects.deleted().is_empty(), "No coins should be deleted");

    // The real coin balance should be:
    // original + fake_coin_amount - gas_charge - transfer_amount
    let final_real_coin_balance = test_env.get_coin_balance(real_coin.0).await;
    assert_eq!(
        final_real_coin_balance,
        real_coin_balance + fake_coin_amount - gas_charge - transfer_amount
    );

    // The sender's address balance should have decreased by the fake coin amount
    let final_ab_balance = test_env.get_sui_balance_ab(sender);
    assert_eq!(final_ab_balance, initial_ab_balance - fake_coin_amount);

    // The recipient should have received a new coin with the transfer amount
    let recipient_balance = test_env.get_sui_balance(recipient).await;
    assert_eq!(recipient_balance, transfer_amount);

    test_env.cluster.trigger_reconfiguration().await;
}

#[sim_test]
async fn test_add_money_to_fake_coin() {
    if has_mainnet_protocol_config_override() {
        return;
    }
    let mut test_env = TestEnvBuilder::new()
        .with_proto_override_cb(Box::new(|_, mut cfg| {
            cfg.enable_coin_reservation_for_testing();
            cfg
        }))
        .build()
        .await;

    let (sender, _) = test_env.get_sender_and_gas(0);
    test_env.fund_one_address_balance(sender, 1000).await;

    let (_, mut all_gas) = test_env.get_sender_and_all_gas(0);
    let gas = all_gas.pop().unwrap();
    let real_coin = all_gas.pop().unwrap();

    let coin_reservation = test_env.encode_coin_reservation(sender, 0, 100);

    let initial_balance = test_env.get_sui_balance_ab(sender);
    let real_coin_balance = test_env.get_coin_balance(real_coin.0).await;

    // Merge `real_coin` into `coin_reservation` (fake coin).
    let tx = test_env
        .tx_builder_with_gas(sender, gas)
        .merge_coins(coin_reservation, vec![real_coin])
        .build();
    let (_, effects) = test_env.exec_tx_directly(tx).await.unwrap();
    assert!(effects.status().is_ok());

    // Verify the sender's address balance is increased by the amount of `real_coin`.
    let final_balance = test_env.get_sui_balance_ab(sender);
    assert_eq!(final_balance, initial_balance + real_coin_balance);

    test_env.cluster.trigger_reconfiguration().await;
}

#[sim_test]
async fn test_split_from_fake_coin() {
    if has_mainnet_protocol_config_override() {
        return;
    }
    let mut test_env = TestEnvBuilder::new()
        .with_proto_override_cb(Box::new(|_, mut cfg| {
            cfg.enable_coin_reservation_for_testing();
            cfg
        }))
        .build()
        .await;

    let (sender, _) = test_env.get_sender_and_gas(0);
    test_env.fund_one_address_balance(sender, 1000).await;

    let coin_reservation = test_env.encode_coin_reservation(sender, 0, 100);

    let tx = test_env
        .tx_builder(sender)
        .split_coin(coin_reservation, vec![100])
        .build();

    // Send tx, should succeed.
    let (_, effects) = test_env.exec_tx_directly(tx).await.unwrap();
    assert!(effects.status().is_ok());

    // Assert that the sender received a new coin with balance 100.
    let created = effects.created();
    assert_eq!(created.len(), 1, "Expected one created object");
    let new_coin_id = created[0].0.0;
    let new_coin_balance = test_env.get_coin_balance(new_coin_id).await;
    assert_eq!(new_coin_balance, 100);

    test_env.cluster.trigger_reconfiguration().await;
}

#[sim_test]
async fn test_fake_coin_conversion_with_references_in_ptbs() {
    if has_mainnet_protocol_config_override() {
        return;
    }
    // The synthesized withdrawal-conversion command injects `&mut TxContext`. With references
    // allowed in PTBs, the memory-safety checkers root TxContext borrows at the borrowing
    // command's position in the command list, and the synthesized command is exactly the case
    // where that position differs from `Command::idx`. This is the only path that produces
    // synthesized commands, and it is unreachable from the adapter transactional tests (the
    // simulator never rewrites coin reservations), so it is covered here.
    let mut test_env = TestEnvBuilder::new()
        .with_proto_override_cb(Box::new(|_, mut cfg| {
            cfg.enable_coin_reservation_for_testing();
            cfg.set_allow_references_in_ptbs_for_testing(true);
            cfg
        }))
        .build()
        .await;

    let (sender, _) = test_env.get_sender_and_gas(0);
    test_env.fund_one_address_balance(sender, 1000).await;

    let coin_reservation = test_env.encode_coin_reservation(sender, 0, 100);

    let tx = test_env
        .tx_builder(sender)
        .split_coin(coin_reservation, vec![100])
        .build();

    let (_, effects) = test_env.exec_tx_directly(tx).await.unwrap();
    assert!(effects.status().is_ok());

    // Assert that the sender received a new coin with balance 100.
    let created = effects.created();
    assert_eq!(created.len(), 1, "Expected one created object");
    let new_coin_id = created[0].0.0;
    let new_coin_balance = test_env.get_coin_balance(new_coin_id).await;
    assert_eq!(new_coin_balance, 100);

    test_env.cluster.trigger_reconfiguration().await;
}

#[sim_test]
async fn test_coin_reservation_enforced_when_not_used() {
    if has_mainnet_protocol_config_override() {
        return;
    }
    let mut test_env = TestEnvBuilder::new()
        .with_proto_override_cb(Box::new(|_, mut cfg| {
            cfg.enable_coin_reservation_for_testing();
            cfg
        }))
        .build()
        .await;

    let sender = test_env.get_sender(0);
    test_env.fund_one_address_balance(sender, 1000).await;

    // Use a coin reservation that is greater than the sender's address balance.
    let coin_reservation = test_env.encode_coin_reservation(sender, 0, 2000);

    // Build tx with 0 commands, using the oversized coin reservation as gas.
    let tx = test_env
        .tx_builder_with_gas(sender, coin_reservation)
        .build();

    // Send tx, assert it fails due to insufficient balance.
    let err = test_env.exec_tx_directly(tx).await.unwrap_err();
    assert!(err.to_string().contains("Insufficient address balance"));

    test_env.cluster.trigger_reconfiguration().await;
}

#[sim_test]
async fn test_deny_list_enforced_for_coin_reservation() {
    if has_mainnet_protocol_config_override() {
        return;
    }
    // See existing deny list tests:
    // - crates/sui-e2e-tests/tests/per_epoch_config_stress_tests.rs (uses deny_list_v2_add/remove)
    // - crates/sui-e2e-tests/tests/rpc/v2/state_service/get_coin_info.rs::test_get_coin_info_regulated_coin
    //
    // Regulated coin modules:
    // - crates/sui-e2e-tests/tests/move_test_code/sources/regulated_coin.move
    // - crates/sui-e2e-tests/tests/rpc/data/regulated_coin/sources/regulated_coin.move
}

#[sim_test]
async fn test_wrong_chain_id() {
    if has_mainnet_protocol_config_override() {
        return;
    }
    let mut test_env = TestEnvBuilder::new()
        .with_proto_override_cb(Box::new(|_, mut cfg| {
            cfg.enable_coin_reservation_for_testing();
            cfg
        }))
        .build()
        .await;

    let (sender, _) = test_env.get_sender_and_gas(0);
    test_env.fund_one_address_balance(sender, 1000).await;

    // Encode a coin reservation with a wrong chain identifier. Unmasking with the
    // correct chain ID will produce a different (nonexistent) object ID.
    let accumulator_obj_id = get_sui_accumulator_object_id(sender);
    let wrong_chain_id = CheckpointDigest::from([42u8; 32]).into();
    let coin_res = ParsedObjectRefWithdrawal::new(accumulator_obj_id, 0, 100)
        .encode(SequenceNumber::new(), wrong_chain_id);

    let err = test_env
        .transfer_from_coin_to_address_balance(sender, coin_res, vec![(1, sender)])
        .await
        .unwrap_err();
    assert!(err.to_string().contains("not found"));

    test_env.cluster.trigger_reconfiguration().await;
}

#[sim_test]
async fn test_gas_smash_into_fake_coin() {
    if has_mainnet_protocol_config_override() {
        return;
    }
    // Test gas smashing where the first coin is a fake coin (coin reservation)
    // and the second coin is a real coin. The real coin should be smashed into
    // the address balance.

    let mut test_env = TestEnvBuilder::new()
        .with_proto_override_cb(Box::new(|_, mut cfg| {
            cfg.enable_coin_reservation_for_testing();
            cfg
        }))
        .build()
        .await;

    let (sender, _) = test_env.get_sender_and_gas(0);

    // Fund address balance so coin reservation works
    test_env
        .fund_one_address_balance(sender, 10_000_000_000)
        .await;

    // Get a real coin to use as the second gas payment
    let (_, mut all_gas) = test_env.get_sender_and_all_gas(0);
    let real_coin = all_gas.pop().unwrap();
    let real_coin_balance = test_env.get_coin_balance(real_coin.0).await;

    // Create a coin reservation (fake coin) to use as the first gas payment
    let fake_coin = test_env.encode_coin_reservation(sender, 0, 5_000_000_000);

    let initial_balance = test_env.get_sui_balance_ab(sender);

    // Build transaction with fake coin first, real coin second
    let tx = test_env
        .tx_builder_with_gas_objects(sender, vec![fake_coin, real_coin])
        .build();
    let (_, effects) = test_env.exec_tx_directly(tx).await.unwrap();
    assert!(effects.status().is_ok());

    let gas_charge = effects.gas_cost_summary().gas_used();

    // The real coin should be deleted (smashed into address balance)
    assert_eq!(effects.deleted().len(), 1, "Real coin should be deleted");
    assert_eq!(
        effects.deleted()[0].0,
        real_coin.0,
        "Deleted object should be the real coin"
    );

    // The sender's address balance should have increased by the real coin amount,
    // minus the gas charge (which was deducted from address balance)
    let final_balance = test_env.get_sui_balance_ab(sender);
    assert_eq!(
        final_balance,
        initial_balance + real_coin_balance - gas_charge
    );

    test_env.cluster.trigger_reconfiguration().await;
}

#[sim_test]
async fn test_gas_smash_multiple_fake_coins() {
    if has_mainnet_protocol_config_override() {
        return;
    }
    // Test gas smashing where the first two coins are fake coins (coin reservations)
    // and the third coin is a real coin. The real coin should be smashed into
    // the address balance.

    let mut test_env = TestEnvBuilder::new()
        .with_proto_override_cb(Box::new(|_, mut cfg| {
            cfg.enable_coin_reservation_for_testing();
            cfg
        }))
        .build()
        .await;

    let (sender, _) = test_env.get_sender_and_gas(0);

    // Fund address balance so coin reservations work
    test_env
        .fund_one_address_balance(sender, 10_000_000_000)
        .await;

    // Get a real coin to use as the third gas payment
    let (_, mut all_gas) = test_env.get_sender_and_all_gas(0);
    let real_coin = all_gas.pop().unwrap();
    let real_coin_balance = test_env.get_coin_balance(real_coin.0).await;

    // Create two coin reservations (fake coins) to use as first and second gas payments.
    // Both use epoch 0 since we're in epoch 0 - the second parameter is epoch, not sequence.
    let fake_coin1 = test_env.encode_coin_reservation(sender, 0, 2_000_000_000);
    let fake_coin2 = test_env.encode_coin_reservation(sender, 0, 3_000_000_000);

    let initial_balance = test_env.get_sui_balance_ab(sender);

    // Build transaction with two fake coins first, then real coin
    let tx = test_env
        .tx_builder_with_gas_objects(sender, vec![fake_coin1, fake_coin2, real_coin])
        .build();
    let (_, effects) = test_env.exec_tx_directly(tx).await.unwrap();
    assert!(effects.status().is_ok());

    let gas_charge = effects.gas_cost_summary().gas_used();

    // The real coin should be deleted (smashed into address balance)
    assert_eq!(effects.deleted().len(), 1, "Real coin should be deleted");
    assert_eq!(
        effects.deleted()[0].0,
        real_coin.0,
        "Deleted object should be the real coin"
    );

    // The sender's address balance should have increased by the real coin amount,
    // minus the gas charge (which was deducted from address balance)
    let final_balance = test_env.get_sui_balance_ab(sender);
    assert_eq!(
        final_balance,
        initial_balance + real_coin_balance - gas_charge
    );

    test_env.cluster.trigger_reconfiguration().await;
}

#[sim_test]
async fn test_gas_smash_from_fake_coin() {
    if has_mainnet_protocol_config_override() {
        return;
    }
    // Test gas smashing where the first coin is a real coin and the second coin
    // is a fake coin (coin reservation). The fake coin value should be withdrawn
    // from address balance and smashed into the real coin.

    let mut test_env = TestEnvBuilder::new()
        .with_proto_override_cb(Box::new(|_, mut cfg| {
            cfg.enable_coin_reservation_for_testing();
            cfg
        }))
        .build()
        .await;

    let (sender, _) = test_env.get_sender_and_gas(0);

    // Fund address balance so coin reservation works
    test_env
        .fund_one_address_balance(sender, 10_000_000_000)
        .await;

    // Get a real coin to use as the first gas payment
    let (_, mut all_gas) = test_env.get_sender_and_all_gas(0);
    let real_coin = all_gas.pop().unwrap();
    let real_coin_balance = test_env.get_coin_balance(real_coin.0).await;

    // Create a coin reservation (fake coin) to use as the second gas payment
    let fake_coin_amount = 5_000_000_000u64;
    let fake_coin = test_env.encode_coin_reservation(sender, 0, fake_coin_amount);

    let initial_balance = test_env.get_sui_balance_ab(sender);

    // Build transaction with real coin first, fake coin second
    let tx = test_env
        .tx_builder_with_gas_objects(sender, vec![real_coin, fake_coin])
        .build();
    let (_, effects) = test_env.exec_tx_directly(tx).await.unwrap();
    assert!(effects.status().is_ok());

    let gas_charge = effects.gas_cost_summary().gas_used();

    // No coins should be deleted - the real coin is mutated (receives the fake coin value)
    assert!(effects.deleted().is_empty(), "No coins should be deleted");

    // The real coin should be mutated (increased by fake coin amount, minus gas)
    let final_real_coin_balance = test_env.get_coin_balance(real_coin.0).await;
    assert_eq!(
        final_real_coin_balance,
        real_coin_balance + fake_coin_amount - gas_charge
    );

    // The sender's address balance should have decreased by the fake coin amount
    let final_balance = test_env.get_sui_balance_ab(sender);
    assert_eq!(final_balance, initial_balance - fake_coin_amount);

    test_env.cluster.trigger_reconfiguration().await;
}

/// Regression test: smashing two real gas coins must conserve SUI and produce the
/// correct final balance. One coin is deleted (smashed into the other) so this
/// exercises the deleted-input path of collect_storage_and_rebate, which is now
/// placed after the IFFW early-return check.
#[sim_test]
async fn test_gas_smash_two_real_coins() {
    if has_mainnet_protocol_config_override() {
        return;
    }

    let mut test_env = TestEnvBuilder::new().build().await;

    let (sender, mut all_gas) = test_env.get_sender_and_all_gas(0);
    assert!(all_gas.len() >= 2, "need ≥2 gas coins");

    let coin1 = all_gas.remove(0);
    let coin2 = all_gas.remove(0);
    let coin1_balance = test_env.get_coin_balance(coin1.0).await;
    let coin2_balance = test_env.get_coin_balance(coin2.0).await;

    let tx = test_env
        .tx_builder_with_gas_objects(sender, vec![coin1, coin2])
        .build();
    let (_, effects) = test_env.exec_tx_directly(tx).await.unwrap();
    assert!(
        effects.status().is_ok(),
        "Transaction failed: {:?}",
        effects.status()
    );

    let gas_used = effects.gas_cost_summary().gas_used();

    // coin2 is deleted (smashed into coin1)
    assert_eq!(effects.deleted().len(), 1);
    assert_eq!(effects.deleted()[0].0, coin2.0);

    // coin1 holds the combined balance minus gas
    let final_balance = test_env.get_coin_balance(coin1.0).await;
    assert_eq!(final_balance, coin1_balance + coin2_balance - gas_used);

    test_env.cluster.trigger_reconfiguration().await;
}

#[sim_test]
async fn test_gas_coin_not_owned_by_gas_owner() {
    if has_mainnet_protocol_config_override() {
        return;
    }
    // Send a transaction using a coin reservation that is not owned by the sender.
    // Verify tx is rejected.

    let mut test_env = TestEnvBuilder::new()
        .with_proto_override_cb(Box::new(|_, mut cfg| {
            cfg.enable_coin_reservation_for_testing();
            cfg
        }))
        .build()
        .await;

    let sender1 = test_env.get_sender(0);
    let sender2 = test_env.get_sender(1);

    // Fund sender2's address balance
    test_env
        .fund_one_address_balance(sender2, 10_000_000_000)
        .await;

    // Create a coin reservation from sender2's address balance
    let coin_reservation_from_sender2 = test_env.encode_coin_reservation(sender2, 0, 5_000_000_000);

    // sender1 tries to use sender2's coin reservation as gas
    let tx = test_env
        .tx_builder_with_gas(sender1, coin_reservation_from_sender2)
        .build();
    let err = test_env.exec_tx_directly(tx).await.unwrap_err();
    assert!(
        err.to_string()
            .contains("Gas object is not an owned object with owner"),
        "Expected ownership error, got: {}",
        err
    );

    test_env.cluster.trigger_reconfiguration().await;
}

#[sim_test]
async fn test_gas_payment_mix_of_owners() {
    if has_mainnet_protocol_config_override() {
        return;
    }
    let mut test_env = TestEnvBuilder::new()
        .with_proto_override_cb(Box::new(|_, mut cfg| {
            cfg.enable_coin_reservation_for_testing();
            cfg
        }))
        .build()
        .await;

    let sender2 = test_env.get_sender(1);

    // Fund sender2's address balance so the accumulator exists
    test_env
        .fund_one_address_balance(sender2, 5_000_000_000)
        .await;
    let (sender1, gas1) = test_env.get_sender_and_gas(0);

    // Create a coin reservation from sender2's accumulator
    let coin_reservation_from_sender2 = test_env.encode_coin_reservation(sender2, 0, 5_000_000_000);

    // sender1 is the sender, gas includes sender1's real coin + sender2's coin reservation.
    // The coin reservation ownership check uses self.sender() = sender1.
    let tx = test_env
        .tx_builder_with_gas_objects(sender1, vec![gas1, coin_reservation_from_sender2])
        .build();
    let err = test_env.exec_tx_directly(tx).await.unwrap_err();
    assert!(
        err.to_string()
            .contains("Gas object is not an owned object with owner"),
        "Expected ownership error, got: {}",
        err
    );

    test_env.cluster.trigger_reconfiguration().await;
}

#[sim_test]
async fn test_coin_reservation_rejected_in_sponsored_transaction() {
    if has_mainnet_protocol_config_override() {
        return;
    }
    use shared_crypto::intent::Intent;
    use sui_keys::keystore::AccountKeystore;
    use sui_types::transaction::{GasData, ProgrammableTransaction, Transaction, TransactionData};

    let mut test_env = TestEnvBuilder::new()
        .with_proto_override_cb(Box::new(|_, mut cfg| {
            cfg.enable_coin_reservation_for_testing();
            cfg
        }))
        .build()
        .await;

    let sender = test_env.get_sender(0);
    let sponsor = test_env.get_sender(1);

    test_env
        .fund_one_address_balance(sender, 10_000_000_000)
        .await;

    let coin_reservation = test_env.encode_coin_reservation(sender, 0, 5_000_000_000);

    let tx_data = TransactionData::new_with_gas_data(
        TransactionKind::ProgrammableTransaction(ProgrammableTransaction {
            inputs: vec![],
            commands: vec![],
        }),
        sender,
        GasData {
            payment: vec![coin_reservation],
            owner: sponsor,
            price: test_env.rgp,
            budget: 5_000_000_000,
        },
    );

    let sender_sig = test_env
        .cluster
        .wallet
        .config
        .keystore
        .sign_secure(&sender, &tx_data, Intent::sui_transaction())
        .await
        .unwrap();
    let sponsor_sig = test_env
        .cluster
        .wallet
        .config
        .keystore
        .sign_secure(&sponsor, &tx_data, Intent::sui_transaction())
        .await
        .unwrap();
    let tx = Transaction::from_data(tx_data, vec![sender_sig, sponsor_sig]);

    let err = test_env
        .cluster
        .execute_transaction_directly(&tx)
        .await
        .unwrap_err();
    assert!(
        err.to_string()
            .contains("Gas object is not an owned object with owner"),
        "Expected sponsored coin reservation rejection, got: {}",
        err
    );

    test_env.cluster.trigger_reconfiguration().await;
}

#[sim_test]
async fn test_merge_coin_into_gas_coin_with_coin_reservation() {
    if has_mainnet_protocol_config_override() {
        return;
    }
    // Tests merging a real coin into the ephemeral GasCoin when gas is paid via
    // coin reservation. The GasCoin starts with (reservation - budget) balance.
    // After merging, the GasCoin should hold the merged coin's value plus whatever
    // was available from the reservation. Then transfer GasCoin to a recipient.

    let mut test_env = TestEnvBuilder::new()
        .with_proto_override_cb(Box::new(|_, mut cfg| {
            cfg.enable_coin_reservation_for_testing();
            cfg
        }))
        .build()
        .await;

    let sender = test_env.get_sender(0);
    let budget = 5_000_000_000u64;

    test_env.fund_one_address_balance(sender, budget).await;

    // Get a real coin to merge into GasCoin
    let (_, mut all_gas) = test_env.get_sender_and_all_gas(0);
    let real_coin = all_gas.pop().unwrap();
    let real_coin_balance = test_env.get_coin_balance(real_coin.0).await;

    let initial_sender_balance = test_env.get_sui_balance_ab(sender);

    // Gas reservation covers exactly the budget — GasCoin starts with 0 available
    let gas_reservation = test_env.encode_coin_reservation(sender, 0, budget);

    let recipient = SuiAddress::random_for_testing_only();

    // Build: MergeCoins(GasCoin, [real_coin]) then TransferObjects([GasCoin], recipient)
    let mut builder = TestTransactionBuilder::new(sender, gas_reservation, test_env.rgp);
    let coin_arg = builder
        .ptb_builder_mut()
        .obj(ObjectArg::ImmOrOwnedObject(real_coin))
        .unwrap();
    builder
        .ptb_builder_mut()
        .command(Command::MergeCoins(Argument::GasCoin, vec![coin_arg]));
    let rec_arg = builder.ptb_builder_mut().pure(recipient).unwrap();
    builder
        .ptb_builder_mut()
        .command(Command::TransferObjects(vec![Argument::GasCoin], rec_arg));
    let tx = builder.build();

    let (_, effects) = test_env.exec_tx_directly(tx).await.unwrap();
    assert!(
        effects.status().is_ok(),
        "Transaction failed: {:?}",
        effects.status()
    );

    let gas_charge = effects.gas_cost_summary().gas_used();

    // When the ephemeral GasCoin is transferred away, gas charges are redirected to the
    // coin itself. The coin keeps: real_coin_balance + (budget - gas_charge).
    let created = effects.created();
    assert_eq!(created.len(), 1, "Expected exactly one created object");
    let created_coin_id = created[0].0.0;
    let coin_balance = test_env.get_coin_balance(created_coin_id).await;
    assert_eq!(coin_balance, real_coin_balance + budget - gas_charge);

    // Sender's address balance is debited by the full reservation amount (the budget),
    // since gas charges are paid from the coin, not the address balance.
    let final_sender_balance = test_env.get_sui_balance_ab(sender);
    assert_eq!(final_sender_balance, initial_sender_balance - budget);

    test_env.cluster.trigger_reconfiguration().await;
}

#[sim_test]
async fn test_coin_reservation_with_shared_object() {
    if has_mainnet_protocol_config_override() {
        return;
    }
    let mut test_env = TestEnvBuilder::new().build().await;

    let (sender, _) = test_env.get_sender_and_gas(0);

    test_env.fund_one_address_balance(sender, 1000).await;

    let (sender, gas) = test_env.get_sender_and_gas(0);
    let coin_reservation = test_env.encode_coin_reservation(sender, 0, 100);

    let recipient = SuiAddress::random_for_testing_only();

    let mut builder = TestTransactionBuilder::new(sender, gas, test_env.rgp);
    let coin_arg = builder
        .ptb_builder_mut()
        .obj(ObjectArg::ImmOrOwnedObject(coin_reservation))
        .unwrap();
    builder = builder.move_call(
        sui_types::SUI_FRAMEWORK_PACKAGE_ID,
        "clock",
        "timestamp_ms",
        vec![CallArg::CLOCK_IMM],
    );
    let balance = builder.ptb_builder_mut().programmable_move_call(
        sui_types::SUI_FRAMEWORK_PACKAGE_ID,
        ident_str!("coin").to_owned(),
        ident_str!("into_balance").to_owned(),
        vec![sui_types::gas_coin::GAS::type_tag()],
        vec![coin_arg],
    );
    let recipient_arg = builder.ptb_builder_mut().pure(recipient).unwrap();
    builder.ptb_builder_mut().programmable_move_call(
        sui_types::SUI_FRAMEWORK_PACKAGE_ID,
        ident_str!("balance").to_owned(),
        ident_str!("send_funds").to_owned(),
        vec![sui_types::gas_coin::GAS::type_tag()],
        vec![balance, recipient_arg],
    );
    let tx = builder.build();

    let (_, effects) = test_env.exec_tx_directly(tx).await.unwrap();
    assert!(effects.status().is_ok());

    test_env.cluster.trigger_reconfiguration().await;
}

#[sim_test]
async fn test_coin_reservation_split_without_move() {
    if has_mainnet_protocol_config_override() {
        return;
    }
    // Split some amount from a coin reservation and transfer the split-off coin
    // to a recipient, but leave the original coin reservation unmoved (not transferred
    // by value).

    let mut test_env = TestEnvBuilder::new().build().await;

    let (sender, _) = test_env.get_sender_and_gas(0);

    test_env.fund_one_address_balance(sender, 1000).await;

    let (sender, gas) = test_env.get_sender_and_gas(0);
    let reservation_amount = 100u64;
    let split_amount = 40u64;
    let coin_reservation = test_env.encode_coin_reservation(sender, 0, reservation_amount);

    let initial_sender_balance = test_env.get_sui_balance_ab(sender);

    let recipient = SuiAddress::random_for_testing_only();

    // Build PTB: SplitCoins(coin_reservation, [split_amount]) -> TransferObjects([split], recipient)
    // The original coin_reservation is not consumed by value.
    let mut builder = TestTransactionBuilder::new(sender, gas, test_env.rgp);
    let coin_arg = builder
        .ptb_builder_mut()
        .obj(ObjectArg::ImmOrOwnedObject(coin_reservation))
        .unwrap();
    let split_amount_arg = builder.ptb_builder_mut().pure(split_amount).unwrap();
    let split_result = builder
        .ptb_builder_mut()
        .command(Command::SplitCoins(coin_arg, vec![split_amount_arg]));
    let Argument::Result(split_idx) = split_result else {
        panic!("Expected Result argument from SplitCoins");
    };
    let recipient_arg = builder.ptb_builder_mut().pure(recipient).unwrap();
    builder.ptb_builder_mut().command(Command::TransferObjects(
        vec![Argument::NestedResult(split_idx, 0)],
        recipient_arg,
    ));
    let tx = builder.build();

    let (_, effects) = test_env.exec_tx_directly(tx).await.unwrap();
    assert!(
        effects.status().is_ok(),
        "Transaction failed: {:?}",
        effects.status()
    );

    // Recipient should have a new coin with the split amount
    let created = effects.created();
    assert_eq!(created.len(), 1, "Expected exactly one created object");
    let split_coin_id = created[0].0.0;
    let split_coin_balance = test_env.get_coin_balance(split_coin_id).await;
    assert_eq!(split_coin_balance, split_amount);

    // The reservation withdraws reservation_amount, but only split_amount leaves
    // as a real coin. The unused portion (reservation_amount - split_amount) returns
    // to address balance. Net debit = split_amount.
    let final_sender_balance = test_env.get_sui_balance_ab(sender);
    assert_eq!(final_sender_balance, initial_sender_balance - split_amount);

    test_env.cluster.trigger_reconfiguration().await;
}

#[sim_test]
async fn test_mix_coin_reservations_real_coins_and_shared_object() {
    if has_mainnet_protocol_config_override() {
        return;
    }
    // Mix a real coin and a coin reservation in a single PTB that also touches
    // a shared object (Clock). Merge the real coin into the coin reservation,
    // then split a portion off and transfer it.

    let mut test_env = TestEnvBuilder::new().build().await;

    let (sender, _) = test_env.get_sender_and_gas(0);

    test_env.fund_one_address_balance(sender, 1000).await;

    let (_, mut all_gas) = test_env.get_sender_and_all_gas(0);
    let gas = all_gas.pop().unwrap();
    let real_coin = all_gas.pop().unwrap();
    let real_coin_balance = test_env.get_coin_balance(real_coin.0).await;

    let sender = test_env.get_sender(0);
    let reservation_amount = 200u64;
    let coin_reservation = test_env.encode_coin_reservation(sender, 0, reservation_amount);

    let initial_sender_balance = test_env.get_sui_balance_ab(sender);

    let recipient = SuiAddress::random_for_testing_only();
    let transfer_amount = 50u64;

    // Build PTB:
    // 1. clock::timestamp_ms(Clock) — touch a shared object
    // 2. MergeCoins(coin_reservation, [real_coin])
    // 3. SplitCoins(coin_reservation, [transfer_amount])
    // 4. TransferObjects([split], recipient)
    let mut builder = TestTransactionBuilder::new(sender, gas, test_env.rgp);
    let coin_res_arg = builder
        .ptb_builder_mut()
        .obj(ObjectArg::ImmOrOwnedObject(coin_reservation))
        .unwrap();
    let real_coin_arg = builder
        .ptb_builder_mut()
        .obj(ObjectArg::ImmOrOwnedObject(real_coin))
        .unwrap();

    // Touch shared object
    builder = builder.move_call(
        sui_types::SUI_FRAMEWORK_PACKAGE_ID,
        "clock",
        "timestamp_ms",
        vec![CallArg::CLOCK_IMM],
    );

    // Merge real coin into coin reservation
    builder
        .ptb_builder_mut()
        .command(Command::MergeCoins(coin_res_arg, vec![real_coin_arg]));

    // Split a portion off the merged result
    let amount_arg = builder.ptb_builder_mut().pure(transfer_amount).unwrap();
    let split_result = builder
        .ptb_builder_mut()
        .command(Command::SplitCoins(coin_res_arg, vec![amount_arg]));
    let Argument::Result(split_idx) = split_result else {
        panic!("Expected Result argument from SplitCoins");
    };

    // Transfer the split coin to recipient
    let recipient_arg = builder.ptb_builder_mut().pure(recipient).unwrap();
    builder.ptb_builder_mut().command(Command::TransferObjects(
        vec![Argument::NestedResult(split_idx, 0)],
        recipient_arg,
    ));
    let tx = builder.build();

    let (_, effects) = test_env.exec_tx_directly(tx).await.unwrap();
    assert!(
        effects.status().is_ok(),
        "Transaction failed: {:?}",
        effects.status()
    );

    // Recipient should have received a coin with transfer_amount
    let created = effects.created();
    assert_eq!(created.len(), 1, "Expected exactly one created object");
    let new_coin_id = created[0].0.0;
    let new_coin_balance = test_env.get_coin_balance(new_coin_id).await;
    assert_eq!(new_coin_balance, transfer_amount);

    // The real coin should be deleted (merged into the coin reservation)
    assert_eq!(effects.deleted().len(), 1);
    assert_eq!(effects.deleted()[0].0, real_coin.0);

    // Sender's address balance: the real coin is merged into the fake coin, crediting
    // real_coin_balance to address balance. The split removes transfer_amount. The
    // reservation's unused portion (reservation_amount - transfer_amount) stays in
    // address balance. Net change = +real_coin_balance - transfer_amount.
    let final_sender_balance = test_env.get_sui_balance_ab(sender);
    assert_eq!(
        final_sender_balance,
        initial_sender_balance + real_coin_balance - transfer_amount
    );

    test_env.cluster.trigger_reconfiguration().await;
}

/// Regression test: gas smashing must not underflow the address balance on IFFW. TX1 drains the
/// AB to 0; TX2's gas payment mixes two real coins with a coin reservation and fires IFFW. With
/// the fix, smashing is skipped (no underflow at settlement, no coins merged/deleted).
#[sim_test]
async fn test_gas_smash_no_ab_underflow_on_iffw() {
    if has_mainnet_protocol_config_override() {
        return;
    }

    let mut test_env = TestEnvBuilder::new()
        .with_proto_override_cb(Box::new(|_, mut cfg| {
            cfg.enable_coin_reservation_for_testing();
            cfg
        }))
        .build()
        .await;

    let sender = test_env.get_sender(0);

    // Fund the AB; both per-tx reservations must be ≤ this to pass signing validation.
    let initial_ab = 20_000_000u64;
    test_env.fund_one_address_balance(sender, initial_ab).await;

    // Refresh gas list after funding (funding consumes one coin).
    let mut all_gas = test_env.get_gas_for_sender(sender);
    assert!(all_gas.len() >= 3, "need ≥3 gas coins");

    // TX1: withdraw all AB (pays gas from a real coin so the full initial_ab is freed).
    let gas_for_tx1 = all_gas.remove(0);
    let dummy = SuiAddress::random_for_testing_only();
    let tx1 = test_env
        .tx_builder_with_gas(sender, gas_for_tx1)
        .transfer_sui_to_address_balance(
            FundSource::address_fund_with_reservation(initial_ab),
            vec![(initial_ab, dummy)],
        )
        .build();

    // TX2: gas_data.payment = [real_coin_a, real_coin_b, coin_reservation].
    // Using two real coins deliberately exercises the case where smashing would normally
    // delete real_coin_b — the fix must leave it intact.
    // Reservation = initial_ab / 2 passes per-tx signing validation (≤ initial_ab) but
    // exceeds the post-TX1 balance of 0, triggering IFFW.
    let real_coin_a = all_gas.remove(0);
    let real_coin_b = all_gas.remove(0);
    let reservation = initial_ab / 2;
    let fake_coin = test_env.encode_coin_reservation(sender, 0, reservation);
    let tx2 = test_env
        .tx_builder_with_gas_objects(sender, vec![real_coin_a, real_coin_b, fake_coin])
        .build();

    let tx1_digest = tx1.digest();
    let tx2_digest = tx2.digest();

    let mut effects = test_env
        .cluster
        .sign_and_execute_txns_in_soft_bundle(&[tx1, tx2])
        .await
        .unwrap();

    let tx2_effects = effects.pop().unwrap().1;
    let tx1_effects = effects.pop().unwrap().1;

    assert!(
        tx1_effects.status().is_ok(),
        "TX1 should succeed: {:?}",
        tx1_effects.status()
    );
    let status_str = format!("{:?}", tx2_effects.status());
    assert!(
        status_str.contains("InsufficientFundsForWithdraw"),
        "TX2 should fail with InsufficientFundsForWithdraw, got: {status_str}"
    );

    // Wait for settlement.  Without the fix the settlement transaction aborts trying
    // to split `reservation` from a 0-balance AB, crashing the node.
    test_env
        .cluster
        .wait_for_tx_settlement(&[tx1_digest, tx2_digest])
        .await;

    // AB must not have been touched by the failed TX2.
    let final_ab = test_env.get_sui_balance_ab(sender);
    assert_eq!(final_ab, 0, "AB should stay 0; got {final_ab}");

    test_env.cluster.trigger_reconfiguration().await;
}

/// Regression test: same shape as test_gas_smash_no_ab_underflow_on_iffw, but the
/// two real coins each hold only 1 MIST. Total real-coin gas after smashing is 2
/// MIST — far below the gas budget. With AB entries dropped on IFFW the gas
/// charger falls back to charging the real coins, and `deduct_gas` asserts
/// balance >= charge. If gas charging is not also skipped for this case the
/// node panics during settlement.
#[sim_test]
async fn test_gas_smash_no_underflow_on_iffw_tiny_real_coins() {
    if has_mainnet_protocol_config_override() {
        return;
    }

    let mut test_env = TestEnvBuilder::new()
        .with_proto_override_cb(Box::new(|_, mut cfg| {
            cfg.enable_coin_reservation_for_testing();
            cfg
        }))
        .build()
        .await;

    let sender = test_env.get_sender(0);

    // AB must be large enough that the per-tx signing validation accepts
    // TX2's reservation (≤ AB) and total gas balance (real coins + reservation)
    // covers the default gas budget (≈ 5_000_000_000 MIST).
    let initial_ab = 10_000_000_000u64;
    test_env.fund_one_address_balance(sender, initial_ab).await;

    let all_gas = test_env.get_gas_for_sender(sender);
    assert!(all_gas.len() >= 3, "need ≥3 gas coins");

    // Pre-step: split two 1-MIST coins off a normal gas coin and transfer them
    // back to sender. These tiny coins will be TX2's real-coin gas payments.
    let splitter_gas = all_gas[0];
    let mut split_builder = ProgrammableTransactionBuilder::new();
    let one_a = split_builder.pure(1u64).unwrap();
    let one_b = split_builder.pure(1u64).unwrap();
    let split_result =
        split_builder.command(Command::SplitCoins(Argument::GasCoin, vec![one_a, one_b]));
    let Argument::Result(split_idx) = split_result else {
        panic!("SplitCoins should return Argument::Result");
    };
    let recipient_arg = split_builder.pure(sender).unwrap();
    split_builder.command(Command::TransferObjects(
        vec![
            Argument::NestedResult(split_idx, 0),
            Argument::NestedResult(split_idx, 1),
        ],
        recipient_arg,
    ));
    let split_pt = split_builder.finish();
    let split_tx = TransactionData::new_programmable(
        sender,
        vec![splitter_gas],
        split_pt,
        test_env.rgp * 5_000_000,
        test_env.rgp,
    );
    let (_, split_effects) = test_env.exec_tx_directly(split_tx).await.unwrap();
    assert!(
        split_effects.status().is_ok(),
        "tiny-coin split failed: {:?}",
        split_effects.status()
    );
    let tiny_coins: Vec<_> = split_effects
        .created()
        .into_iter()
        .filter(
            |(_, owner)| matches!(owner, sui_types::object::Owner::AddressOwner(a) if *a == sender),
        )
        .map(|(obj_ref, _)| obj_ref)
        .collect();
    assert_eq!(
        tiny_coins.len(),
        2,
        "expected exactly two created tiny coins"
    );
    let tiny_coin_a = tiny_coins[0];
    let tiny_coin_b = tiny_coins[1];
    assert_eq!(test_env.get_coin_balance(tiny_coin_a.0).await, 1);
    assert_eq!(test_env.get_coin_balance(tiny_coin_b.0).await, 1);

    // Refresh gas list; splitter_gas was mutated and tiny coins are now also in
    // the wallet's gas-object list. Pick a separate coin to pay for TX1.
    let all_gas = test_env.get_gas_for_sender(sender);
    let gas_for_tx1 = *all_gas
        .iter()
        .find(|g| g.0 != splitter_gas.0 && g.0 != tiny_coin_a.0 && g.0 != tiny_coin_b.0)
        .expect("need a non-tiny coin to pay TX1's gas");

    // TX1: drain the AB to 0 (gas comes from a real coin, so the full
    // initial_ab transfers out).
    let dummy = SuiAddress::random_for_testing_only();
    let tx1 = test_env
        .tx_builder_with_gas(sender, gas_for_tx1)
        .transfer_sui_to_address_balance(
            FundSource::address_fund_with_reservation(initial_ab),
            vec![(initial_ab, dummy)],
        )
        .build();

    // TX2: gas_data.payment = [tiny_coin_a (1 MIST), tiny_coin_b (1 MIST),
    // coin_reservation]. After TX1 drains the AB the reservation withdraw
    // fails with IFFW.  reservation = initial_ab / 2 passes the per-tx signing
    // check (≤ initial_ab) and reservation + 2 MIST > default budget.
    let reservation = initial_ab / 2;
    let fake_coin = test_env.encode_coin_reservation(sender, 0, reservation);
    let tx2 = test_env
        .tx_builder_with_gas_objects(sender, vec![tiny_coin_a, tiny_coin_b, fake_coin])
        .build();

    let tx1_digest = tx1.digest();
    let tx2_digest = tx2.digest();

    let mut effects = test_env
        .cluster
        .sign_and_execute_txns_in_soft_bundle(&[tx1, tx2])
        .await
        .unwrap();

    let tx2_effects = effects.pop().unwrap().1;
    let tx1_effects = effects.pop().unwrap().1;

    assert!(
        tx1_effects.status().is_ok(),
        "TX1 should succeed: {:?}",
        tx1_effects.status()
    );
    let status_str = format!("{:?}", tx2_effects.status());
    assert!(
        status_str.contains("InsufficientFundsForWithdraw"),
        "TX2 should fail with InsufficientFundsForWithdraw, got: {status_str}"
    );

    // Settlement must complete. Without an additional fix, gas charging tries
    // to deduct gas from a 2-MIST coin and panics in deduct_gas's
    // `balance >= charge` assertion, crashing the node.
    test_env
        .cluster
        .wait_for_tx_settlement(&[tx1_digest, tx2_digest])
        .await;

    let final_ab = test_env.get_sui_balance_ab(sender);
    assert_eq!(final_ab, 0, "AB should stay 0; got {final_ab}");

    test_env.cluster.trigger_reconfiguration().await;
}

/// Regression test: AB-as-smash-target on IFFW with non-genesis coins.
///
/// TX2 has gas_data.payment = [coin_reservation, real_coin_a, real_coin_b].
/// The coin_reservation is first, so the AB is the smash target.
/// After TX1 drains the AB, TX2 fails with IFFW.
///
/// `real_coin_a` and `real_coin_b` are split off a genesis gas coin so they
/// carry a non-zero `storage_rebate`, which exercises the conservation check
/// more thoroughly.
///
/// With the skip-all fix, smashing is entirely skipped for IFFW transactions
/// that involve any AB payment. The real coins are NOT deleted; they retain
/// their original balances. The AB stays at 0 and settlement completes without
/// any panic.
#[sim_test]
async fn test_gas_smash_ab_target_iffw() {
    if has_mainnet_protocol_config_override() {
        return;
    }

    let mut test_env = TestEnvBuilder::new()
        .with_proto_override_cb(Box::new(|_, mut cfg| {
            cfg.enable_coin_reservation_for_testing();
            cfg
        }))
        .build()
        .await;

    let sender = test_env.get_sender(0);

    // initial_ab must cover the reservation (≤ AB at signing time), and the
    // reservation alone needs to be ≥ default gas budget (5_000_000_000) so
    // signing-time gas-balance validation passes.
    let initial_ab = 10_000_000_000u64;
    test_env.fund_one_address_balance(sender, initial_ab).await;

    let all_gas = test_env.get_gas_for_sender(sender);
    assert!(all_gas.len() >= 2, "need ≥2 gas coins");

    // Pre-step: split two coins off a genesis gas coin so they have non-zero
    // `storage_rebate` (genesis-allocated coins have `storage_rebate = 0` and
    // would hide rebate-accounting bugs).  Split coins are sized large enough
    // that any incidental gas charge won't take them below zero.
    let splitter_gas = all_gas[0];
    let split_amount = 1_000_000_000u64;
    let mut split_builder = ProgrammableTransactionBuilder::new();
    let amt_a = split_builder.pure(split_amount).unwrap();
    let amt_b = split_builder.pure(split_amount).unwrap();
    let split_result =
        split_builder.command(Command::SplitCoins(Argument::GasCoin, vec![amt_a, amt_b]));
    let Argument::Result(split_idx) = split_result else {
        panic!("SplitCoins should return Argument::Result");
    };
    let recipient_arg = split_builder.pure(sender).unwrap();
    split_builder.command(Command::TransferObjects(
        vec![
            Argument::NestedResult(split_idx, 0),
            Argument::NestedResult(split_idx, 1),
        ],
        recipient_arg,
    ));
    let split_pt = split_builder.finish();
    let split_tx = TransactionData::new_programmable(
        sender,
        vec![splitter_gas],
        split_pt,
        test_env.rgp * 5_000_000,
        test_env.rgp,
    );
    let (_, split_effects) = test_env.exec_tx_directly(split_tx).await.unwrap();
    assert!(
        split_effects.status().is_ok(),
        "coin split failed: {:?}",
        split_effects.status()
    );
    let split_coins: Vec<_> = split_effects
        .created()
        .into_iter()
        .filter(
            |(_, owner)| matches!(owner, sui_types::object::Owner::AddressOwner(a) if *a == sender),
        )
        .map(|(obj_ref, _)| obj_ref)
        .collect();
    assert_eq!(split_coins.len(), 2, "expected exactly two split coins");
    let real_coin_a = split_coins[0];
    let real_coin_b = split_coins[1];
    let real_coin_a_balance = test_env.get_coin_balance(real_coin_a.0).await;
    let real_coin_b_balance = test_env.get_coin_balance(real_coin_b.0).await;
    assert_eq!(real_coin_a_balance, split_amount);
    assert_eq!(real_coin_b_balance, split_amount);

    // Re-fetch the wallet's gas list; the splitter coin's version changed.
    let all_gas = test_env.get_gas_for_sender(sender);
    let gas_for_tx1 = *all_gas
        .iter()
        .find(|g| g.0 != splitter_gas.0 && g.0 != real_coin_a.0 && g.0 != real_coin_b.0)
        .expect("need a non-split coin to pay TX1's gas");

    // TX1: drain the AB to 0 (gas comes from a real coin).
    let dummy = SuiAddress::random_for_testing_only();
    let tx1 = test_env
        .tx_builder_with_gas(sender, gas_for_tx1)
        .transfer_sui_to_address_balance(
            FundSource::address_fund_with_reservation(initial_ab),
            vec![(initial_ab, dummy)],
        )
        .build();

    // TX2: gas_data.payment = [coin_reservation, real_coin_a, real_coin_b].
    // Reservation first → AB is the smash target; real coins are smashed into it.
    let reservation = initial_ab / 2;
    let fake_coin = test_env.encode_coin_reservation(sender, 0, reservation);
    let tx2 = test_env
        .tx_builder_with_gas_objects(sender, vec![fake_coin, real_coin_a, real_coin_b])
        .build();

    let tx1_digest = tx1.digest();
    let tx2_digest = tx2.digest();

    let mut effects = test_env
        .cluster
        .sign_and_execute_txns_in_soft_bundle(&[tx1, tx2])
        .await
        .unwrap();

    let tx2_effects = effects.pop().unwrap().1;
    let tx1_effects = effects.pop().unwrap().1;

    assert!(
        tx1_effects.status().is_ok(),
        "TX1 should succeed: {:?}",
        tx1_effects.status()
    );
    let status_str = format!("{:?}", tx2_effects.status());
    assert!(
        status_str.contains("InsufficientFundsForWithdraw"),
        "TX2 should fail with InsufficientFundsForWithdraw, got: {status_str}"
    );

    test_env
        .cluster
        .wait_for_tx_settlement(&[tx1_digest, tx2_digest])
        .await;

    // Smashing is skipped entirely for IFFW: no coins are deleted.
    assert!(
        tx2_effects.deleted().is_empty(),
        "no coins should be deleted when smashing is skipped: {:?}",
        tx2_effects.deleted()
    );

    // Gas summary is zero: no computation or storage charges.
    let cost = tx2_effects.gas_cost_summary();
    assert_eq!(
        cost.computation_cost, 0,
        "IFFW computation_cost should be 0; got {cost:?}"
    );
    assert_eq!(
        cost.storage_cost, 0,
        "IFFW storage_cost should be 0; got {cost:?}"
    );
    assert_eq!(
        cost.storage_rebate, 0,
        "IFFW storage_rebate should be 0; got {cost:?}"
    );
    assert_eq!(
        cost.non_refundable_storage_fee, 0,
        "IFFW non_refundable_storage_fee should be 0; got {cost:?}"
    );

    // Even though smashing is skipped, the gas coins are still inputs and must
    // have their versions bumped so locks advance.
    let mutated_ids: std::collections::BTreeMap<ObjectID, SequenceNumber> = tx2_effects
        .mutated()
        .into_iter()
        .map(|(obj_ref, _owner)| (obj_ref.0, obj_ref.1))
        .collect();
    let mutated_real_a = mutated_ids
        .get(&real_coin_a.0)
        .expect("real_coin_a must appear in mutated() with a bumped version");
    assert!(
        *mutated_real_a > real_coin_a.1,
        "real_coin_a version must advance: input={:?}, mutated={mutated_real_a:?}",
        real_coin_a.1
    );
    let mutated_real_b = mutated_ids
        .get(&real_coin_b.0)
        .expect("real_coin_b must appear in mutated() with a bumped version");
    assert!(
        *mutated_real_b > real_coin_b.1,
        "real_coin_b version must advance: input={:?}, mutated={mutated_real_b:?}",
        real_coin_b.1
    );

    // Both real coins retain their original balances.
    assert_eq!(
        test_env.get_coin_balance(real_coin_a.0).await,
        real_coin_a_balance,
        "real_coin_a balance must be unchanged"
    );
    assert_eq!(
        test_env.get_coin_balance(real_coin_b.0).await,
        real_coin_b_balance,
        "real_coin_b balance must be unchanged"
    );

    // AB stays at 0 — no deposit from smashing since smashing was skipped.
    let final_ab = test_env.get_sui_balance_ab(sender);
    assert_eq!(final_ab, 0, "AB should stay 0; got {final_ab}");

    test_env.cluster.trigger_reconfiguration().await;
}

/// Shared IFFW smash scenario. The caller supplies a closure that builds TX2's
/// `gas_data.payment` vector and returns the subset of real coins that should
/// appear in `mutated()` with bumped versions.
///
/// `total_reservation_amount` is the AB budget the closure may distribute across
/// one or more coin reservations. Each individual reservation should be at least
/// the default gas budget (5_000_000_000) so signing-time validation passes.
async fn iffw_smash_scenario<F>(build_payment: F)
where
    F: FnOnce(&TestEnv, SuiAddress, u64, ObjectRef, ObjectRef) -> (Vec<ObjectRef>, Vec<ObjectRef>),
{
    if has_mainnet_protocol_config_override() {
        return;
    }

    let mut test_env = TestEnvBuilder::new()
        .with_proto_override_cb(Box::new(|_, mut cfg| {
            cfg.enable_coin_reservation_for_testing();
            cfg
        }))
        .build()
        .await;

    let sender = test_env.get_sender(0);

    // Sized to support up to two reservations of 7.5B each (≥ default gas budget),
    // and still leave `initial_ab / 2` headroom against the AB at signing time.
    let initial_ab = 30_000_000_000u64;
    test_env.fund_one_address_balance(sender, initial_ab).await;

    let all_gas = test_env.get_gas_for_sender(sender);
    assert!(all_gas.len() >= 2, "need ≥2 gas coins");

    // Pre-step: split two coins off a genesis gas coin so they have non-zero
    // `storage_rebate` (genesis-allocated coins have `storage_rebate = 0`).
    let splitter_gas = all_gas[0];
    let split_amount = 1_000_000_000u64;
    let mut split_builder = ProgrammableTransactionBuilder::new();
    let amt_a = split_builder.pure(split_amount).unwrap();
    let amt_b = split_builder.pure(split_amount).unwrap();
    let split_result =
        split_builder.command(Command::SplitCoins(Argument::GasCoin, vec![amt_a, amt_b]));
    let Argument::Result(split_idx) = split_result else {
        panic!("SplitCoins should return Argument::Result");
    };
    let recipient_arg = split_builder.pure(sender).unwrap();
    split_builder.command(Command::TransferObjects(
        vec![
            Argument::NestedResult(split_idx, 0),
            Argument::NestedResult(split_idx, 1),
        ],
        recipient_arg,
    ));
    let split_pt = split_builder.finish();
    let split_tx = TransactionData::new_programmable(
        sender,
        vec![splitter_gas],
        split_pt,
        test_env.rgp * 5_000_000,
        test_env.rgp,
    );
    let (_, split_effects) = test_env.exec_tx_directly(split_tx).await.unwrap();
    assert!(
        split_effects.status().is_ok(),
        "coin split failed: {:?}",
        split_effects.status()
    );
    let split_coins: Vec<_> = split_effects
        .created()
        .into_iter()
        .filter(
            |(_, owner)| matches!(owner, sui_types::object::Owner::AddressOwner(a) if *a == sender),
        )
        .map(|(obj_ref, _)| obj_ref)
        .collect();
    assert_eq!(split_coins.len(), 2, "expected exactly two split coins");
    let real_coin_a = split_coins[0];
    let real_coin_b = split_coins[1];
    let real_coin_a_balance = test_env.get_coin_balance(real_coin_a.0).await;
    let real_coin_b_balance = test_env.get_coin_balance(real_coin_b.0).await;
    assert_eq!(real_coin_a_balance, split_amount);
    assert_eq!(real_coin_b_balance, split_amount);

    // Re-fetch the wallet's gas list; the splitter coin's version changed.
    let all_gas = test_env.get_gas_for_sender(sender);
    let gas_for_tx1 = *all_gas
        .iter()
        .find(|g| g.0 != splitter_gas.0 && g.0 != real_coin_a.0 && g.0 != real_coin_b.0)
        .expect("need a non-split coin to pay TX1's gas");

    // TX1: drain the AB to 0 (gas comes from a real coin).
    let dummy = SuiAddress::random_for_testing_only();
    let tx1 = test_env
        .tx_builder_with_gas(sender, gas_for_tx1)
        .transfer_sui_to_address_balance(
            FundSource::address_fund_with_reservation(initial_ab),
            vec![(initial_ab, dummy)],
        )
        .build();

    let total_reservation_amount = initial_ab / 2;
    let (payment, real_coins_used) = build_payment(
        &test_env,
        sender,
        total_reservation_amount,
        real_coin_a,
        real_coin_b,
    );
    let tx2 = test_env
        .tx_builder_with_gas_objects(sender, payment)
        .build();

    let tx1_digest = tx1.digest();
    let tx2_digest = tx2.digest();

    let mut effects = test_env
        .cluster
        .sign_and_execute_txns_in_soft_bundle(&[tx1, tx2])
        .await
        .unwrap();

    let tx2_effects = effects.pop().unwrap().1;
    let tx1_effects = effects.pop().unwrap().1;

    assert!(
        tx1_effects.status().is_ok(),
        "TX1 should succeed: {:?}",
        tx1_effects.status()
    );
    let status_str = format!("{:?}", tx2_effects.status());
    assert!(
        status_str.contains("InsufficientFundsForWithdraw"),
        "TX2 should fail with InsufficientFundsForWithdraw, got: {status_str}"
    );

    test_env
        .cluster
        .wait_for_tx_settlement(&[tx1_digest, tx2_digest])
        .await;

    // Smashing is skipped entirely for IFFW: no coins are deleted.
    assert!(
        tx2_effects.deleted().is_empty(),
        "no coins should be deleted when smashing is skipped: {:?}",
        tx2_effects.deleted()
    );

    // Gas summary is zero across all fields.
    let cost = tx2_effects.gas_cost_summary();
    assert_eq!(
        cost.computation_cost, 0,
        "IFFW computation_cost should be 0; got {cost:?}"
    );
    assert_eq!(
        cost.storage_cost, 0,
        "IFFW storage_cost should be 0; got {cost:?}"
    );
    assert_eq!(
        cost.storage_rebate, 0,
        "IFFW storage_rebate should be 0; got {cost:?}"
    );
    assert_eq!(
        cost.non_refundable_storage_fee, 0,
        "IFFW non_refundable_storage_fee should be 0; got {cost:?}"
    );

    // Every real coin used as payment must appear in mutated() at a bumped version.
    let mutated_ids: std::collections::BTreeMap<ObjectID, SequenceNumber> = tx2_effects
        .mutated()
        .into_iter()
        .map(|(obj_ref, _owner)| (obj_ref.0, obj_ref.1))
        .collect();
    for real_coin in &real_coins_used {
        let mutated_version = mutated_ids
            .get(&real_coin.0)
            .unwrap_or_else(|| panic!("real coin {:?} must appear in mutated()", real_coin.0));
        assert!(
            *mutated_version > real_coin.1,
            "real coin {:?} version must advance: input={:?}, mutated={mutated_version:?}",
            real_coin.0,
            real_coin.1
        );
    }

    // Real coin balances retained.
    for real_coin in &real_coins_used {
        let expected = if real_coin.0 == real_coin_a.0 {
            real_coin_a_balance
        } else if real_coin.0 == real_coin_b.0 {
            real_coin_b_balance
        } else {
            panic!("unexpected real coin in payment: {:?}", real_coin.0);
        };
        assert_eq!(
            test_env.get_coin_balance(real_coin.0).await,
            expected,
            "real coin {:?} balance must be unchanged",
            real_coin.0
        );
    }

    // AB stays at 0 — no deposit from smashing since smashing was skipped.
    let final_ab = test_env.get_sui_balance_ab(sender);
    assert_eq!(final_ab, 0, "AB should stay 0; got {final_ab}");

    test_env.cluster.trigger_reconfiguration().await;
}

/// IFFW with the smash target as a real gas coin: payment = [real_coin_a, reservation].
/// The reservation sits behind a real coin, so the real coin is the smash target.
/// The trailing reservation still triggers IFFW once the AB is drained.
#[sim_test]
async fn test_gas_smash_coin_target_iffw() {
    iffw_smash_scenario(
        |env, sender, reservation_amount, real_coin_a, _real_coin_b| {
            let fake = env.encode_coin_reservation(sender, 0, reservation_amount);
            (vec![real_coin_a, fake], vec![real_coin_a])
        },
    )
    .await
}

/// IFFW with the smash target as a reservation and a single trailing real coin:
/// payment = [reservation, real_coin_a]. AB is the target, the real coin is a
/// non-target — minimal variant of `test_gas_smash_ab_target_iffw` (one real coin).
#[sim_test]
async fn test_gas_smash_coin_nontarget_iffw() {
    iffw_smash_scenario(
        |env, sender, reservation_amount, real_coin_a, _real_coin_b| {
            let fake = env.encode_coin_reservation(sender, 0, reservation_amount);
            (vec![fake, real_coin_a], vec![real_coin_a])
        },
    )
    .await
}

/// IFFW with reservations and real coins interleaved:
/// payment = [reservation_a, real_coin_a, reservation_b, real_coin_b].
/// Two reservations both draw from the drained AB; both real coins are non-targets
/// at different positions in the payment vector and must still be mutated.
#[sim_test]
async fn test_gas_smash_interspersed_iffw() {
    iffw_smash_scenario(
        |env, sender, reservation_amount, real_coin_a, real_coin_b| {
            let half = reservation_amount / 2;
            let fake_a = env.encode_coin_reservation(sender, 0, half);
            let fake_b = env.encode_coin_reservation(sender, 0, half);
            (
                vec![fake_a, real_coin_a, fake_b, real_coin_b],
                vec![real_coin_a, real_coin_b],
            )
        },
    )
    .await
}

fn build_fake_coin_reservation_pt(
    chain_id: sui_types::digests::ChainIdentifier,
) -> TransactionKind {
    let bogus_accumulator_id = ObjectID::random();
    let fake_coin_res = ParsedObjectRefWithdrawal::new(bogus_accumulator_id, 0, 100)
        .encode(SequenceNumber::new(), chain_id);

    let mut pt_builder = ProgrammableTransactionBuilder::new();
    let coin_arg = pt_builder
        .obj(ObjectArg::ImmOrOwnedObject(fake_coin_res))
        .unwrap();
    let recipient_arg = pt_builder
        .pure(SuiAddress::random_for_testing_only())
        .unwrap();
    pt_builder.command(Command::TransferObjects(vec![coin_arg], recipient_arg));
    TransactionKind::ProgrammableTransaction(pt_builder.finish())
}

#[sim_test]
async fn test_fake_coin_reservation_dry_run_does_not_panic() {
    if has_mainnet_protocol_config_override() {
        return;
    }
    let test_env = TestEnvBuilder::new()
        .with_proto_override_cb(Box::new(|_, mut cfg| {
            cfg.enable_coin_reservation_for_testing();
            cfg
        }))
        .build()
        .await;

    let (sender, gas) = test_env.get_sender_and_gas(0);
    let kind = build_fake_coin_reservation_pt(test_env.chain_id);
    let tx_data = TransactionData::V1(TransactionDataV1 {
        kind,
        sender,
        gas_data: GasData {
            payment: vec![gas],
            owner: sender,
            price: test_env.rgp,
            budget: 5_000_000_000,
        },
        expiration: TransactionExpiration::None,
    });

    let state = test_env
        .cluster
        .fullnode_handle
        .sui_node
        .with(|node| node.state().clone());
    let join = tokio::task::spawn(async move { state.dry_exec_transaction(tx_data).await });

    match join.await {
        Ok(Ok(_)) => panic!("dry-run with fake coin reservation should have errored"),
        Ok(Err(e)) => {
            let msg = e.to_string();
            assert!(
                msg.contains("InvalidWithdrawReservation") || msg.contains("not found"),
                "unexpected error: {msg}"
            );
        }
        Err(join_err) if join_err.is_panic() => {
            panic!(
                "dryRunTransactionBlock panicked on fake coin reservation: {:?}",
                join_err
            );
        }
        Err(e) => panic!("unexpected join error: {e:?}"),
    }
}

#[sim_test]
async fn test_fake_coin_reservation_dev_inspect_does_not_panic() {
    if has_mainnet_protocol_config_override() {
        return;
    }
    let test_env = TestEnvBuilder::new()
        .with_proto_override_cb(Box::new(|_, mut cfg| {
            cfg.enable_coin_reservation_for_testing();
            cfg
        }))
        .build()
        .await;

    let sender = test_env.get_sender(0);
    let tx_kind = build_fake_coin_reservation_pt(test_env.chain_id);

    let state = test_env
        .cluster
        .fullnode_handle
        .sui_node
        .with(|node| node.state().clone());
    let join = tokio::task::spawn(async move {
        state
            .dev_inspect_transaction_block(
                sender,
                tx_kind,
                None,
                None,
                None,
                None,
                None,
                /* skip_checks */ Some(true),
            )
            .await
    });

    match join.await {
        Ok(Ok(_)) => panic!("dev-inspect with fake coin reservation should have errored"),
        Ok(Err(e)) => {
            let msg = e.to_string();
            assert!(
                msg.contains("InvalidWithdrawReservation") || msg.contains("not found"),
                "unexpected error: {msg}"
            );
        }
        Err(join_err) if join_err.is_panic() => {
            panic!(
                "devInspectTransactionBlock panicked on fake coin reservation: {:?}",
                join_err
            );
        }
        Err(e) => panic!("unexpected join error: {e:?}"),
    }
}

#[sim_test]
async fn test_fake_coin_reservation_dry_run_safe_when_flag_disabled() {
    if has_mainnet_protocol_config_override() {
        return;
    }
    let test_env = TestEnvBuilder::new()
        .with_proto_override_cb(Box::new(|_, mut cfg| {
            cfg.disable_coin_reservation_for_testing();
            cfg
        }))
        .build()
        .await;

    let (sender, gas) = test_env.get_sender_and_gas(0);
    let kind = build_fake_coin_reservation_pt(test_env.chain_id);
    let tx_data = TransactionData::V1(TransactionDataV1 {
        kind,
        sender,
        gas_data: GasData {
            payment: vec![gas],
            owner: sender,
            price: test_env.rgp,
            budget: 5_000_000_000,
        },
        expiration: TransactionExpiration::None,
    });

    let state = test_env
        .cluster
        .fullnode_handle
        .sui_node
        .with(|node| node.state().clone());
    let join = tokio::task::spawn(async move { state.dry_exec_transaction(tx_data).await });

    match join.await {
        Ok(Ok(_)) => panic!("dry-run with fake coin reservation should have errored"),
        Ok(Err(e)) => {
            assert!(
                e.to_string()
                    .contains("coin reservation backward compatibility layer is not enabled"),
                "expected gating rejection, got: {e}"
            );
        }
        Err(join_err) if join_err.is_panic() => {
            panic!(
                "dryRunTransactionBlock panicked with coin reservation flag disabled: {:?}",
                join_err
            );
        }
        Err(e) => panic!("unexpected join error: {e:?}"),
    }
}

#[sim_test]
async fn test_fake_coin_reservation_dev_inspect_safe_when_flag_disabled() {
    if has_mainnet_protocol_config_override() {
        return;
    }
    let test_env = TestEnvBuilder::new()
        .with_proto_override_cb(Box::new(|_, mut cfg| {
            cfg.disable_coin_reservation_for_testing();
            cfg
        }))
        .build()
        .await;

    let sender = test_env.get_sender(0);
    let tx_kind = build_fake_coin_reservation_pt(test_env.chain_id);

    let state = test_env
        .cluster
        .fullnode_handle
        .sui_node
        .with(|node| node.state().clone());
    let join = tokio::task::spawn(async move {
        state
            .dev_inspect_transaction_block(
                sender,
                tx_kind,
                None,
                None,
                None,
                None,
                None,
                /* skip_checks */ Some(true),
            )
            .await
    });

    match join.await {
        Ok(Ok(_)) => panic!("dev-inspect with fake coin reservation should have errored"),
        Ok(Err(e)) => {
            assert!(
                e.to_string()
                    .contains("coin reservation backward compatibility layer is not enabled"),
                "expected gating rejection, got: {e}"
            );
        }
        Err(join_err) if join_err.is_panic() => {
            panic!(
                "devInspectTransactionBlock panicked with coin reservation flag disabled: {:?}",
                join_err
            );
        }
        Err(e) => panic!("unexpected join error: {e:?}"),
    }
}
