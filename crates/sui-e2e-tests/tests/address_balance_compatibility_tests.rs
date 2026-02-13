// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use sui_macros::*;
use sui_test_transaction_builder::{FundSource, TestTransactionBuilder};
use sui_types::{
    base_types::{FullObjectRef, ObjectID, SequenceNumber, SuiAddress},
    coin_reservation::ParsedObjectRefWithdrawal,
    digests::CheckpointDigest,
    effects::TransactionEffectsAPI,
};
use test_cluster::addr_balance_test_env::{TestEnvBuilder, get_sui_accumulator_object_id};

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

    test_env
        .cluster
        .wait_for_tx_settlement(std::slice::from_ref(effects.transaction_digest()))
        .await;

    // ensure both balances arrived at the recipient
    let recipient_balance = test_env.get_sui_balance(recipient).await;
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
}

#[sim_test]
async fn test_gas_coin_callarg_with_coin_reservation_gas() {
    // TODO: This test requires GasCoin materialization to work with coin reservations.
    // Currently, when gas is paid via address balance (coin reservation), no actual coin object
    // exists to load. The adapter needs to create a synthetic/virtual coin object from the
    // address balance with balance = reservation_amount - gas_budget.
    // See: gas_charger.rs (smash_gas, gas_coin), context.rs (gas budget subtraction in new())

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

    let initial_sender_balance = test_env.get_sui_balance_ab(sender);

    // Use transfer_sui which internally does SplitCoins(GasCoin, [amount]) + TransferObjects.
    // GasCoin should work with coin reservation gas.
    let tx = TestTransactionBuilder::new(sender, gas_reservation, test_env.rgp)
        .transfer_sui(Some(100), recipient)
        .build();
    let (_, effects) = test_env.exec_tx_directly(tx).await.unwrap();
    assert!(effects.status().is_ok());

    let gas_charge = effects.gas_cost_summary().gas_used();

    // Verify the sender's address balance is decreased by the amount of the gas charges + transfer.
    let final_sender_balance = test_env.get_sui_balance_ab(sender);
    assert_eq!(
        final_sender_balance,
        initial_sender_balance - gas_charge - 100
    );

    // Verify the recipient receives the 100 MIST transfer.
    let recipient_balance = test_env.get_sui_balance_ab(recipient);
    assert_eq!(recipient_balance, 100);
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

    // Send tx, should succeed.
    let (_, effects) = test_env.exec_tx_directly(tx).await.unwrap();
    assert!(effects.status().is_ok());

    // Assert that the sender received a new coin with balance 100.
    let created = effects.created();
    assert_eq!(created.len(), 1, "Expected one created object");
    let new_coin_id = created[0].0.0;
    let new_coin_balance = test_env.get_coin_balance(new_coin_id).await;
    assert_eq!(new_coin_balance, 100);
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
}

#[sim_test]
async fn test_gas_smash_into_fake_coin() {
    // TODO: This test requires fixes to gas smashing with mixed payment methods.
    // When fake coin is first and real coin is second, there's an overflow error.
    // When real coin is first and fake coin is second, the real coin is mutated
    // rather than deleted/smashed into address balance.
    //
    // Original requirements:
    // - build a transaction with 0 commands.
    // - for the gas payment, make the first coin a coin reservation, i.e. fake coin.
    // - make the second gas payment a real coin.
    // - send tx, should succeed
    // - verify the sender's address balance is increased by the amount of the real coin,
    //   and the real coin is deleted.
}

#[sim_test]
async fn test_gas_smash_multiple_fake_coins() {
    // TODO: This test requires fixes to gas smashing with mixed payment methods.
    // See test_gas_smash_into_fake_coin for details.
    //
    // Original requirements:
    // - build a transaction with 0 commands.
    // - for the gas payment, make the first 2 coins coin reservations, i.e. fake coins.
    // - make the third gas payment a real coin.
    // - send tx, should succeed
    // - verify the sender's address balance is increased by the amount of the real coin,
    //   and the real coin is deleted.
    // - verify the 2 fake coins are deleted.
}

#[sim_test]
async fn test_gas_smash_from_fake_coin() {
    // TODO: This test requires fixes to gas smashing with mixed payment methods.
    // See test_gas_smash_into_fake_coin for details.
    //
    // Original requirements:
    // - build a transaction with 0 commands.
    // - make the first gas coin a real coin.
    // - make the second gas coin a coin reservation, i.e. fake coin.
    // - send tx, should succeed
    // - verify the sender's address balance is decreased by the amount of the fake coin,
    //   and the real coin increases by the value of the fake coin, minus the gas charge.
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
}
