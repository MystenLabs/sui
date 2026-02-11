// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use move_core_types::identifier::Identifier;
use sui_macros::*;
use sui_test_transaction_builder::{FundSource, TestTransactionBuilder};
use sui_types::{
    SUI_FRAMEWORK_PACKAGE_ID,
    base_types::{FullObjectRef, ObjectID, SequenceNumber, SuiAddress},
    coin_reservation::ParsedObjectRefWithdrawal,
    digests::CheckpointDigest,
    effects::TransactionEffectsAPI,
    gas_coin::GAS,
    programmable_transaction_builder::ProgrammableTransactionBuilder,
    transaction::{Argument, Command, ObjectArg, TransactionData},
};
use test_cluster::addr_balance_test_env::{TestEnvBuilder, get_sui_accumulator_object_id};

// TODO: test cases for backward compat layer
// - tx paying gas with coin reservation
// - gas smashing transaction with no commands
// - transaction with no commands, but non-gas coin reservations
// - transaction using GAS_COIN CallArg
// - add money to a fake coin
// - deduct money from a fake coin
// - transfer a fake coin away (with deduct/add money)
// - transfer a fake coin to oneself (with deduct/add money)
// - wrap a fake coin
// - wrong ChainID
// - non-SUI coin reservation in gas_payment
// - deny list is enforced for coin reservations
// - gas coin reservation not owned by gas_owner
// - gas payment with mix of real/fake coins.
//   - mix of owners
// - gas payment is not enough for budget

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
}

#[sim_test]
async fn test_coin_reservation_gating() {
    let mut test_env = TestEnvBuilder::new().build().await;

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

    // ensure both balances arrived at the recipient
    let recipient_balance = test_env.get_sui_balance(recipient);
    // 100 from coin transfer, 1 from coin reservation
    assert_eq!(recipient_balance, 100 + 1);
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

    let initial_sender_balance = test_env.get_sui_balance(sender);

    let tx = TestTransactionBuilder::new(sender, gas_payment, test_env.rgp)
        .transfer_sui_to_address_balance(FundSource::coin(transfer_payment), vec![(1, recipient)])
        .build();
    let (_, effects) = test_env.exec_tx_directly(tx).await.unwrap();

    assert!(effects.status().is_ok());
    let gas_charge = effects.gas_cost_summary().gas_used() as u128;

    // ensure both balances arrived at the recipient
    let recipient_balance = test_env.get_sui_balance(recipient);

    // 1 MIST transferred.
    assert_eq!(recipient_balance, 1);

    // Sender should have lost the gas charge and the 1 MIST transferred.
    let final_sender_balance = test_env.get_sui_balance(sender);
    assert_eq!(
        final_sender_balance,
        initial_sender_balance as u64 - gas_charge as u64 - 1
    );
}

#[sim_test]
async fn test_gas_coin_callarg_with_coin_reservation_gas() {
    // KEEP
    let mut test_env = TestEnvBuilder::new()
        .with_proto_override_cb(Box::new(|_, mut cfg| {
            cfg.enable_coin_reservation_for_testing();
            cfg
        }))
        .build()
        .await;

    let sender = test_env.get_sender(0);
    let budget = 5_000_000_000;
    test_env
        .fund_one_address_balance(sender, budget + 100)
        .await;

    let gas_reservation = test_env.encode_coin_reservation(sender, 0, budget);
    let recipient = SuiAddress::random_for_testing_only();

    // Use transfer_sui which internally does SplitCoins(GasCoin, [amount]) + TransferObjects.
    // GasCoin should work with coin reservation gas.
    let tx = TestTransactionBuilder::new(sender, gas_reservation, test_env.rgp)
        .transfer_sui(Some(100), recipient)
        .build();
    let (_, effects) = test_env.exec_tx_directly(tx).await.unwrap();
    assert!(effects.status().is_ok());
}

#[sim_test]
async fn test_wrong_chain_id() {
    // KEEP
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
}

#[sim_test]
async fn test_gas_payment_mix_of_real_and_fake_coins() {
    // KEEP
    let mut test_env = TestEnvBuilder::new()
        .with_proto_override_cb(Box::new(|_, mut cfg| {
            cfg.enable_coin_reservation_for_testing();
            cfg
        }))
        .build()
        .await;

    let (sender, _) = test_env.get_sender_and_gas(0);
    test_env
        .fund_one_address_balance(sender, 5_000_000_000)
        .await;
    let (sender, gas) = test_env.get_sender_and_gas(0);

    let coin_reservation = test_env.encode_coin_reservation(sender, 0, 5_000_000_000);

    // Gas payment is a mix of real coin and coin reservation.
    let ptb = ProgrammableTransactionBuilder::new();
    let pt = ptb.finish();

    let tx = TransactionData::new_programmable(
        sender,
        vec![gas, coin_reservation],
        pt,
        5_000_000_001,
        test_env.rgp,
    );
    let (_, effects) = test_env.exec_tx_directly(tx).await.unwrap();
    assert!(effects.status().is_ok());
    // TODO: verify balances
}

#[sim_test]
async fn test_gas_payment_mix_of_owners() {
    // KEEP (maybe?)
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
    let ptb = ProgrammableTransactionBuilder::new();
    let pt = ptb.finish();

    let tx = TransactionData::new_programmable(
        sender1,
        vec![gas1, coin_reservation_from_sender2],
        pt,
        5_000_000_000,
        test_env.rgp,
    );
    let err = test_env.exec_tx_directly(tx).await.unwrap_err();
    assert!(
        err.to_string()
            .contains(format!("is owned by {}, not sender {}", sender2, sender1).as_str())
    );
}
