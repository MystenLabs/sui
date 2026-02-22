// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#![allow(deprecated)] // We need to use rpc_client for JSON-RPC testing

use jsonrpsee::core::client::ClientT;
use jsonrpsee::rpc_params;
use sui_json_rpc_types::{
    Balance as RpcBalance, CoinPage, SuiData, SuiObjectDataOptions, SuiObjectResponse,
};
use sui_macros::*;
use sui_test_transaction_builder::{FundSource, TestTransactionBuilder};
use sui_types::{
    base_types::{FullObjectRef, ObjectID, SequenceNumber, SuiAddress},
    coin_reservation::{self, ParsedDigest, ParsedObjectRefWithdrawal},
    digests::{CheckpointDigest, ObjectDigest},
    effects::TransactionEffectsAPI,
    transaction::{Argument, Command, TransactionDataAPI, TransactionKind},
};
use test_cluster::addr_balance_test_env::{TestEnv, TestEnvBuilder, get_sui_accumulator_object_id};

#[sim_test]
async fn test_coin_reservation_validation() {
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
        assert!(err.to_string().contains("is less than requested"));
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

        assert!(
            err.to_string()
                .contains("Balance of gas object 100 is lower than the needed amount")
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
    // Explicitly disable coin reservations to test gating (they're on by default for devnet/localnet)
    let mut test_env = TestEnvBuilder::new()
        .with_proto_override_cb(Box::new(|_, mut cfg| {
            cfg.disable_coin_reservation_for_testing();
            cfg
        }))
        .build()
        .await;

    let sender = test_env.get_sender(0);

    // Verify transaction is rejected if coin reservation is not enabled (as input).
    {
        let coin_reservation = test_env.encode_coin_reservation(sender, 0, 1);

        let err = test_env
            .transfer_from_coin_to_address_balance(sender, coin_reservation, vec![(1, sender)])
            .await
            .unwrap_err();
        assert!(
            err.to_string()
                .contains("coin reservation backward compatibility layer is not enabled"),
            "Expected gating error for coin reservation in input, got: {}",
            err
        );
    }

    // Verify transaction is rejected if coin reservation is used as gas payment.
    {
        let coin_reservation = test_env.encode_coin_reservation(sender, 0, 5_000_000_000);

        let tx = test_env
            .tx_builder_with_gas(sender, coin_reservation)
            .build();
        let err = test_env.exec_tx_directly(tx).await.unwrap_err();
        assert!(
            err.to_string()
                .contains("coin reservation backward compatibility layer is not enabled"),
            "Expected gating error for coin reservation in gas payment, got: {}",
            err
        );
    }

    test_env.cluster.trigger_reconfiguration().await;
}

#[sim_test]
async fn test_valid_coin_reservation_transfers() {
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
        initial_sender_balance as u64 - gas_charge as u64 - 1
    );

    test_env.cluster.trigger_reconfiguration().await;
}

#[sim_test]
async fn test_gas_coin_callarg_with_coin_reservation_gas() {
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
    dbg!(&tx);

    // Send tx, should succeed.
    let (_, effects) = test_env.exec_tx_directly(tx).await.unwrap();
    dbg!(&effects);
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
    assert!(err.to_string().contains("is less than requested"));

    test_env.cluster.trigger_reconfiguration().await;
}

#[sim_test]
async fn test_deny_list_enforced_for_coin_reservation() {
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

#[sim_test]
async fn test_gas_coin_not_owned_by_gas_owner() {
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
            .contains(format!("is owned by {}, not sender {}", sender2, sender1).as_str())
    );

    test_env.cluster.trigger_reconfiguration().await;
}

#[sim_test]
async fn test_gas_payment_mix_of_owners() {
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
            .contains(format!("is owned by {}, not sender {}", sender2, sender1).as_str())
    );

    test_env.cluster.trigger_reconfiguration().await;
}

#[sim_test]
async fn test_rpc_get_object_returns_fake_coin() {
    // Test that the JSON-RPC getObject endpoint returns a fake coin object
    // when given a masked object ID representing an address balance.

    let mut test_env = TestEnvBuilder::new()
        .with_proto_override_cb(Box::new(|_, mut cfg| {
            cfg.enable_coin_reservation_for_testing();
            cfg
        }))
        .build()
        .await;

    let (sender, _) = test_env.get_sender_and_gas(0);
    let address_balance_amount = 1_000_000_000u64;

    // Fund sender's address balance
    test_env
        .fund_one_address_balance(sender, address_balance_amount)
        .await;

    // Get the fake coin object ref (masked ID)
    let fake_coin_ref = test_env.encode_coin_reservation(sender, 0, address_balance_amount);
    let masked_object_id = fake_coin_ref.0;

    // The masked ID should be different from the unmasked accumulator object ID
    let unmasked_id = coin_reservation::mask_or_unmask_id(masked_object_id, test_env.chain_id);
    assert_ne!(masked_object_id, unmasked_id);

    // Query the RPC endpoint with the masked object ID
    let params = rpc_params![
        masked_object_id,
        SuiObjectDataOptions::new().with_content().with_owner()
    ];
    let response: SuiObjectResponse = test_env
        .cluster
        .fullnode_handle
        .rpc_client
        .request("sui_getObject", params)
        .await
        .unwrap();

    // The response should contain the fake coin object
    let object_data = response.data.expect("Expected object data");
    assert_eq!(object_data.object_id, masked_object_id);

    // Verify the object is a coin and has the expected balance
    let content = object_data.content.expect("Expected content");
    let fields = content.try_into_move().expect("Expected move object");
    assert!(
        fields
            .type_
            .to_string()
            .contains("0x2::coin::Coin<0x2::sui::SUI>")
    );

    test_env.cluster.trigger_reconfiguration().await;
}

#[sim_test]
async fn test_rpc_get_coins_includes_fake_coins() {
    // Test that the JSON-RPC getCoins endpoint includes fake coins
    // representing address balances.

    let mut test_env = TestEnvBuilder::new()
        .with_proto_override_cb(Box::new(|_, mut cfg| {
            cfg.enable_coin_reservation_for_testing();
            cfg
        }))
        .build()
        .await;

    let (sender, _) = test_env.get_sender_and_gas(0);
    let address_balance_amount = 5_000_000_000u64;

    // Get the initial coin count
    let params = rpc_params![
        sender,
        Option::<String>::None,
        Option::<String>::None,
        Option::<usize>::None
    ];
    let initial_coins: CoinPage = test_env
        .cluster
        .fullnode_handle
        .rpc_client
        .request("suix_getCoins", params)
        .await
        .unwrap();
    let initial_coin_count = initial_coins.data.len();

    // Fund sender's address balance
    test_env
        .fund_one_address_balance(sender, address_balance_amount)
        .await;

    // Get the fake coin object ref
    let fake_coin_ref = test_env.encode_coin_reservation(sender, 0, address_balance_amount);
    let masked_object_id = fake_coin_ref.0;

    // Query the RPC endpoint for coins
    let params = rpc_params![
        sender,
        Option::<String>::None,
        Option::<String>::None,
        Option::<usize>::None
    ];
    let coins: CoinPage = test_env
        .cluster
        .fullnode_handle
        .rpc_client
        .request("suix_getCoins", params)
        .await
        .unwrap();

    // Should have one more coin than before (the fake coin)
    assert_eq!(
        coins.data.len(),
        initial_coin_count + 1,
        "Should have one additional fake coin"
    );

    // Find the fake coin in the list
    let fake_coin = coins
        .data
        .iter()
        .find(|c| c.coin_object_id == masked_object_id)
        .expect("Fake coin should be in the list");

    assert_eq!(fake_coin.balance, address_balance_amount);
    assert!(fake_coin.coin_type.contains("0x2::sui::SUI"));

    test_env.cluster.trigger_reconfiguration().await;
}

#[sim_test]
async fn test_rpc_get_balance_includes_address_balance() {
    // Test that the JSON-RPC getBalance endpoint includes address balance
    // in the total balance.

    let mut test_env = TestEnvBuilder::new()
        .with_proto_override_cb(Box::new(|_, mut cfg| {
            cfg.enable_coin_reservation_for_testing();
            cfg
        }))
        .build()
        .await;

    let (sender, _) = test_env.get_sender_and_gas(0);
    let address_balance_amount = 3_000_000_000u64;

    // Get the initial balance
    let params = rpc_params![sender, Option::<String>::None];
    let initial_balance: RpcBalance = test_env
        .cluster
        .fullnode_handle
        .rpc_client
        .request("suix_getBalance", params)
        .await
        .unwrap();
    let initial_total = initial_balance.total_balance;
    let initial_coin_count = initial_balance.coin_object_count;

    // Fund sender's address balance
    test_env
        .fund_one_address_balance(sender, address_balance_amount)
        .await;

    // Get the updated balance
    let params = rpc_params![sender, Option::<String>::None];
    let updated_balance: RpcBalance = test_env
        .cluster
        .fullnode_handle
        .rpc_client
        .request("suix_getBalance", params)
        .await
        .unwrap();

    // The total balance should be roughly the same (minus gas costs) since we're
    // just moving funds from coin to address balance. The key check is that the
    // address balance is included in the total.
    assert!(
        updated_balance.total_balance >= initial_total - 10_000_000,
        "Total balance should be roughly the same (allowing for gas costs). \
        Initial: {}, Updated: {}",
        initial_total,
        updated_balance.total_balance
    );

    // Coin count should have increased by 1 (the fake coin representing the address balance)
    assert_eq!(
        updated_balance.coin_object_count,
        initial_coin_count + 1,
        "Coin count should have increased by 1 (fake coin). \
        Initial: {}, Updated: {}",
        initial_coin_count,
        updated_balance.coin_object_count
    );

    // The funds_in_address_balance field should reflect the address balance
    assert_eq!(
        updated_balance.funds_in_address_balance, address_balance_amount as u128,
        "Address balance should be reported"
    );

    test_env.cluster.trigger_reconfiguration().await;
}

/// Helper function to fetch SUI coins using pagination with a specific page size.
/// Returns a list of (object_id, balance, digest) tuples in the order returned by the API.
async fn fetch_sui_coins_paginated(
    rpc_client: &impl ClientT,
    owner: SuiAddress,
    page_size: usize,
) -> Vec<(ObjectID, u64, ObjectDigest)> {
    let mut all_coins = Vec::new();
    let mut cursor: Option<String> = None;
    let mut iteration = 0;

    loop {
        iteration += 1;
        if iteration > 100 {
            panic!(
                "fetch_sui_coins_paginated: too many iterations ({}), likely infinite loop",
                iteration
            );
        }

        // suix_getCoins with None coin_type defaults to SUI
        let params = rpc_params![
            owner,
            Option::<String>::None,
            cursor.clone(),
            Some(page_size)
        ];
        let page: CoinPage = rpc_client.request("suix_getCoins", params).await.unwrap();

        for coin in &page.data {
            all_coins.push((coin.coin_object_id, coin.balance, coin.digest));
        }

        if !page.has_next_page {
            break;
        }
        cursor = page.next_cursor;
    }

    all_coins
}

/// Helper function to fetch ALL coins (all types) using pagination with a specific page size.
/// Uses suix_getAllCoins which returns coins of all types sorted by (type, inverted_balance, id).
async fn fetch_all_coins_paginated(
    rpc_client: &impl ClientT,
    owner: SuiAddress,
    page_size: usize,
) -> Vec<(ObjectID, u64, ObjectDigest)> {
    let mut all_coins = Vec::new();
    let mut cursor: Option<String> = None;
    let mut iteration = 0;

    loop {
        iteration += 1;
        if iteration > 100 {
            panic!(
                "fetch_all_coins_paginated: too many iterations ({}), likely infinite loop",
                iteration
            );
        }

        let params = rpc_params![owner, cursor.clone(), Some(page_size)];
        let page: CoinPage = rpc_client
            .request("suix_getAllCoins", params)
            .await
            .unwrap();

        for coin in &page.data {
            all_coins.push((coin.coin_object_id, coin.balance, coin.digest));
        }

        if !page.has_next_page {
            break;
        }
        cursor = page.next_cursor;
    }

    all_coins
}

/// Helper to verify pagination consistency for a sender with coins.
/// Tests that fetching coins with different page sizes always returns identical results.
/// For "multi_type" test, uses suix_getAllCoins; otherwise uses suix_getCoins (SUI only).
async fn verify_pagination_consistency(
    test_env: &TestEnv,
    sender: SuiAddress,
    expected_fake_position: &str,
) {
    let rpc_client = &test_env.cluster.fullnode_handle.rpc_client;

    // Use getAllCoins for multi-type tests, getCoins (SUI only) for single-type tests
    let use_all_coins = expected_fake_position == "multi_type";

    // Get baseline with large page size (effectively no pagination)
    let baseline = if use_all_coins {
        fetch_all_coins_paginated(rpc_client, sender, 100).await
    } else {
        fetch_sui_coins_paginated(rpc_client, sender, 100).await
    };
    let total_coins = baseline.len();

    assert!(
        total_coins >= 2,
        "Need at least 2 coins for meaningful pagination test, got {}",
        total_coins
    );

    // Test with various page sizes from 1 to total_coins + 2
    for page_size in 1..=total_coins + 2 {
        let paginated = if use_all_coins {
            fetch_all_coins_paginated(rpc_client, sender, page_size).await
        } else {
            fetch_sui_coins_paginated(rpc_client, sender, page_size).await
        };

        assert_eq!(
            paginated.len(),
            baseline.len(),
            "Page size {} returned different number of coins for {} fake coin position. \
            Expected {}, got {}",
            page_size,
            expected_fake_position,
            baseline.len(),
            paginated.len()
        );

        // Verify each coin matches in the same order
        for (i, (baseline_coin, paginated_coin)) in
            baseline.iter().zip(paginated.iter()).enumerate()
        {
            assert_eq!(
                baseline_coin, paginated_coin,
                "Page size {} returned different coin at position {} for {} fake coin position. \
                Expected {:?}, got {:?}",
                page_size, i, expected_fake_position, baseline_coin, paginated_coin
            );
        }
    }

    // Find all fake coins (one per coin type with address balance)
    let fake_coin_positions: Vec<usize> = baseline
        .iter()
        .enumerate()
        .filter(|(_, (_, _, digest))| ParsedDigest::is_coin_reservation_digest(digest))
        .map(|(i, _)| i)
        .collect();

    assert!(
        !fake_coin_positions.is_empty(),
        "No fake coins found in results for {} position test",
        expected_fake_position
    );

    // For position checks, we care about the first fake coin (SUI type for single-type tests)
    let first_fake_pos = fake_coin_positions[0];

    match expected_fake_position {
        "largest" => assert_eq!(
            first_fake_pos, 0,
            "Fake SUI coin should be at position 0 (largest), but was at {}",
            first_fake_pos
        ),
        "smallest" => {
            // The SUI fake coin should be the last coin (smallest balance)
            assert_eq!(
                first_fake_pos,
                total_coins - 1,
                "Fake SUI coin should be at last position (smallest), but was at {}",
                first_fake_pos
            );
        }
        "middle" => assert!(
            first_fake_pos > 0 && first_fake_pos < total_coins - 1,
            "Fake SUI coin should be in middle, but was at position {} of {}",
            first_fake_pos,
            total_coins
        ),
        "multi_type" => {
            assert!(
                fake_coin_positions.len() >= 2,
                "Expected multiple fake coins (one per type), but found {}",
                fake_coin_positions.len()
            );
        }
        _ => {}
    }
}

#[sim_test]
async fn test_rpc_get_coins_pagination_multi_type() {
    // Test pagination with multiple coin types: SUI real coins, SUI fake coin (address balance),
    // and a custom coin fake coin. Coins are sorted by (coin_type, inverted_balance, object_id),
    // so SUI coins (0x2::sui::SUI) come before custom coins lexicographically.

    let mut test_env = TestEnvBuilder::new()
        .with_proto_override_cb(Box::new(|_, mut cfg| {
            cfg.enable_coin_reservation_for_testing();
            cfg
        }))
        .build()
        .await;

    let (sender, mut all_gas) = test_env.get_sender_and_all_gas(0);
    let gas = all_gas.pop().unwrap();
    let coin_to_split = all_gas.pop().unwrap();

    // Create real SUI coins with small balances: 100, 200, 300, 400 mist
    let split_amounts = vec![100u64, 200, 300, 400];
    let tx = test_env
        .tx_builder_with_gas(sender, gas)
        .split_coin(coin_to_split, split_amounts.clone())
        .build();
    let (digest, effects) = test_env.exec_tx_directly(tx).await.unwrap();
    assert!(effects.status().is_ok(), "Split coin failed");
    test_env.cluster.wait_for_tx_settlement(&[digest]).await;

    // Fund SUI address balance (creates fake SUI coin)
    let sui_fake_balance = 250u64;
    test_env
        .fund_one_address_balance(sender, sui_fake_balance)
        .await;

    // Publish a custom coin and mint to address balance (creates fake custom coin)
    let custom_coin_balance = 5000u64;
    let (_, _coin_type) = test_env
        .publish_and_mint_trusted_coin(sender, custom_coin_balance)
        .await;

    // Verify pagination consistency across all coin types
    verify_pagination_consistency(&test_env, sender, "multi_type").await;

    test_env.cluster.trigger_reconfiguration().await;
}

#[sim_test]
async fn test_rpc_get_coins_pagination_fake_coin_largest() {
    // Test pagination when SUI fake coin has the LARGEST balance among SUI coins.
    // Also includes a custom coin to test multi-type pagination.

    let mut test_env = TestEnvBuilder::new()
        .with_proto_override_cb(Box::new(|_, mut cfg| {
            cfg.enable_coin_reservation_for_testing();
            cfg
        }))
        .build()
        .await;

    let (sender, mut all_gas) = test_env.get_sender_and_all_gas(0);
    let gas = all_gas.pop().unwrap();
    let coin_to_split = all_gas.pop().unwrap();

    // Create real SUI coins with small balances: 100, 200, 300, 400 mist
    let split_amounts = vec![100u64, 200, 300, 400];
    let tx = test_env
        .tx_builder_with_gas(sender, gas)
        .split_coin(coin_to_split, split_amounts.clone())
        .build();
    let (digest, effects) = test_env.exec_tx_directly(tx).await.unwrap();
    assert!(effects.status().is_ok(), "Split coin failed");
    test_env.cluster.wait_for_tx_settlement(&[digest]).await;

    // Get current SUI coins to find the max balance
    let params = rpc_params![
        sender,
        Option::<String>::None,
        Option::<String>::None,
        Option::<usize>::None
    ];
    let coins_before: CoinPage = test_env
        .cluster
        .fullnode_handle
        .rpc_client
        .request("suix_getCoins", params)
        .await
        .unwrap();
    let max_balance = coins_before.data.iter().map(|c| c.balance).max().unwrap();

    // Fund SUI address balance twice to create a fake coin larger than any real coin.
    let funding_amount = (max_balance as u128 * 6 / 10) as u64;

    // First funding
    test_env
        .fund_one_address_balance(sender, funding_amount)
        .await;

    // Swap in a fresh gas coin for the second funding
    let params = rpc_params![
        sender,
        Option::<String>::None,
        Option::<String>::None,
        Option::<usize>::None
    ];
    let coins_mid: CoinPage = test_env
        .cluster
        .fullnode_handle
        .rpc_client
        .request("suix_getCoins", params)
        .await
        .unwrap();

    let fresh_coin = coins_mid
        .data
        .iter()
        .find(|c| c.balance >= funding_amount + 1_000_000_000)
        .expect("Should have a coin with enough balance for second funding");

    test_env.gas_objects.get_mut(&sender).unwrap()[0] = (
        fresh_coin.coin_object_id,
        fresh_coin.version,
        fresh_coin.digest,
    );

    // Second funding
    test_env
        .fund_one_address_balance(sender, funding_amount)
        .await;

    // Publish a custom coin and mint to address balance
    let custom_coin_balance = 7500u64;
    let (_, _coin_type) = test_env
        .publish_and_mint_trusted_coin(sender, custom_coin_balance)
        .await;

    verify_pagination_consistency(&test_env, sender, "largest").await;

    test_env.cluster.trigger_reconfiguration().await;
}

#[sim_test]
async fn test_rpc_get_coins_pagination_fake_coin_smallest() {
    // Test pagination when SUI fake coin has the SMALLEST balance among SUI coins.
    // Also includes a custom coin to test multi-type pagination.

    let mut test_env = TestEnvBuilder::new()
        .with_proto_override_cb(Box::new(|_, mut cfg| {
            cfg.enable_coin_reservation_for_testing();
            cfg
        }))
        .build()
        .await;

    let (sender, mut all_gas) = test_env.get_sender_and_all_gas(0);
    let gas = all_gas.pop().unwrap();
    let coin_to_split = all_gas.pop().unwrap();

    // Create real SUI coins with balances: 1000, 2000, 3000, 4000 mist
    let split_amounts = vec![1000u64, 2000, 3000, 4000];
    let tx = test_env
        .tx_builder_with_gas(sender, gas)
        .split_coin(coin_to_split, split_amounts.clone())
        .build();
    let (digest, effects) = test_env.exec_tx_directly(tx).await.unwrap();
    assert!(effects.status().is_ok(), "Split coin failed");
    test_env.cluster.wait_for_tx_settlement(&[digest]).await;

    // Fund SUI address balance with amount smaller than smallest split coin
    let fake_balance = 50u64;
    test_env
        .fund_one_address_balance(sender, fake_balance)
        .await;

    // Publish a custom coin and mint to address balance
    let custom_coin_balance = 2500u64;
    let (_, _coin_type) = test_env
        .publish_and_mint_trusted_coin(sender, custom_coin_balance)
        .await;

    verify_pagination_consistency(&test_env, sender, "smallest").await;

    test_env.cluster.trigger_reconfiguration().await;
}

#[sim_test]
async fn test_rpc_get_coins_pagination_fake_coin_middle() {
    // Test pagination when SUI fake coin has a MIDDLE balance among SUI coins.
    // Also includes a custom coin to test multi-type pagination.

    let mut test_env = TestEnvBuilder::new()
        .with_proto_override_cb(Box::new(|_, mut cfg| {
            cfg.enable_coin_reservation_for_testing();
            cfg
        }))
        .build()
        .await;

    let (sender, mut all_gas) = test_env.get_sender_and_all_gas(0);
    let gas = all_gas.pop().unwrap();
    let coin_to_split = all_gas.pop().unwrap();

    // Create real SUI coins with balances: 100, 200, 400, 500 mist (gap at 300)
    let split_amounts = vec![100u64, 200, 400, 500];
    let tx = test_env
        .tx_builder_with_gas(sender, gas)
        .split_coin(coin_to_split, split_amounts.clone())
        .build();
    let (digest, effects) = test_env.exec_tx_directly(tx).await.unwrap();
    assert!(effects.status().is_ok(), "Split coin failed");
    test_env.cluster.wait_for_tx_settlement(&[digest]).await;

    // Fund SUI address balance with 300 - between 200 and 400
    let fake_balance = 300u64;
    test_env
        .fund_one_address_balance(sender, fake_balance)
        .await;

    // Publish a custom coin and mint to address balance
    let custom_coin_balance = 350u64;
    let (_, _coin_type) = test_env
        .publish_and_mint_trusted_coin(sender, custom_coin_balance)
        .await;

    verify_pagination_consistency(&test_env, sender, "middle").await;

    test_env.cluster.trigger_reconfiguration().await;
}
