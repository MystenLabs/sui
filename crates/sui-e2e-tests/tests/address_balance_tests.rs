// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use move_core_types::identifier::Identifier;
use sui_json_rpc_api::CoinReadApiClient;
use sui_json_rpc_types::{SuiTransactionBlockEffectsAPI, SuiTransactionBlockResponse};
use sui_keys::keystore::AccountKeystore;
use sui_macros::*;
use sui_protocol_config::ProtocolConfig;
use sui_sdk::wallet_context::WalletContext;
use sui_test_transaction_builder::TestTransactionBuilder;
use sui_types::{
    accumulator_metadata::AccumulatorOwner,
    accumulator_root::{AccumulatorValue, U128},
    balance::Balance,
    base_types::{ObjectID, ObjectRef, SequenceNumber, SuiAddress},
    coin_reservation,
    effects::TransactionEffectsAPI,
    gas_coin::GAS,
    programmable_transaction_builder::ProgrammableTransactionBuilder,
    storage::ChildObjectResolver,
    transaction::{Argument, Command, ObjectArg, TransactionData, TransactionKind},
    SUI_FRAMEWORK_PACKAGE_ID,
};
use test_cluster::{TestCluster, TestClusterBuilder};

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

#[sim_test]
async fn test_deposits() {
    let _guard = ProtocolConfig::apply_overrides_for_testing(|_, mut cfg| {
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

    assert!(accumulator_object
        .data
        .try_as_move()
        .unwrap()
        .type_()
        .is_efficient_representation());

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
    builder.funds_withdrawal(withdraw_arg).unwrap();

    let amount = builder.pure(amount).unwrap();

    let balance = builder.programmable_move_call(
        SUI_FRAMEWORK_PACKAGE_ID,
        Identifier::new("balance").unwrap(),
        Identifier::new("withdraw_from_account").unwrap(),
        vec!["0x2::sui::SUI".parse().unwrap()],
        vec![amount],
    );

    let coin = builder.programmable_move_call(
        SUI_FRAMEWORK_PACKAGE_ID,
        Identifier::new("coin").unwrap(),
        Identifier::new("from_balance").unwrap(),
        vec!["0x2::sui::SUI".parse().unwrap()],
        vec![balance],
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

    let balance = builder.programmable_move_call(
        SUI_FRAMEWORK_PACKAGE_ID,
        Identifier::new("coin").unwrap(),
        Identifier::new("into_balance").unwrap(),
        vec!["0x2::sui::SUI".parse().unwrap()],
        vec![coin],
    );

    builder.programmable_move_call(
        SUI_FRAMEWORK_PACKAGE_ID,
        Identifier::new("balance").unwrap(),
        Identifier::new("send_to_account").unwrap(),
        vec!["0x2::sui::SUI".parse().unwrap()],
        vec![balance, recipient_arg],
    );

    let tx = TransactionKind::ProgrammableTransaction(builder.finish());
    TransactionData::new(tx, sender, gas, 10000000, rgp)
}

#[sim_test]
async fn test_coin_reservation() {
    let _guard = ProtocolConfig::apply_overrides_for_testing(|_, mut cfg| {
        cfg.enable_accumulators_for_testing();
        cfg.enable_coin_reservation_for_testing();
        cfg
    });

    let mut test_cluster = TestClusterBuilder::new()
        .with_num_validators(1)
        .build()
        .await;
    let rgp = test_cluster.get_reference_gas_price().await;

    let (sender, gas) = get_sender_and_gas(&mut test_cluster.wallet).await;

    // send 1000 from our gas coin to our balance
    let tx = make_send_to_account_tx(1000, sender, sender, gas, rgp);
    let res = test_cluster.sign_and_execute_transaction(&tx).await;
    let gas = res.effects.unwrap().gas_object().reference.to_object_ref();

    // look up the sui coin metadata object
    let sui_metadata_id = test_cluster
        .fullnode_handle
        .rpc_client
        .get_coin_metadata("0x2::sui::SUI".to_string())
        .await
        .unwrap()
        .unwrap()
        .id
        .unwrap();

    // Verify transaction is rejected if it reserves more than the available balance
    let coin_reservation =
        coin_reservation::encode_object_ref(sui_metadata_id, SequenceNumber::new(), 0, 1001)
            .unwrap();

    let err = try_coin_reservation_tx(&mut test_cluster, coin_reservation, sender, gas)
        .await
        .unwrap_err();
    assert!(err.to_string().contains("is less than requested"));

    // Verify transaction is rejected if it uses a bogus coin metadata id.
    let random_id = ObjectID::random();
    let coin_reservation =
        coin_reservation::encode_object_ref(random_id, SequenceNumber::new(), 0, 1001).unwrap();

    let err = try_coin_reservation_tx(&mut test_cluster, coin_reservation, sender, gas)
        .await
        .unwrap_err();
    assert!(err
        .to_string()
        .contains(format!("object id {} not found", random_id).as_str()));

    // Verify transaction is rejected if it is not valid in the current epoch.
    let coin_reservation =
        coin_reservation::encode_object_ref(sui_metadata_id, SequenceNumber::new(), 1, 100)
            .unwrap();

    let err = try_coin_reservation_tx(&mut test_cluster, coin_reservation, sender, gas)
        .await
        .unwrap_err();
    assert!(err
        .to_string()
        .contains("Transaction not valid during this epoch"));

    // Verify that transaction is rejected if the coin metadata is address owned
    let publish_coin = TestTransactionBuilder::new(sender, gas, rgp)
        .publish_examples("coin")
        .build();
    let publish_coin_res = test_cluster
        .sign_and_execute_transaction(&publish_coin)
        .await;
    let gas = publish_coin_res
        .effects
        .as_ref()
        .unwrap()
        .gas_object()
        .reference
        .to_object_ref();

    let coin_package_ref = publish_coin_res.get_new_package_obj().unwrap();
    let coin_metadata_id = test_cluster
        .fullnode_handle
        .rpc_client
        .get_coin_metadata(format!(
            "{}::owned_metadata_coin::OWNED_METADATA_COIN",
            coin_package_ref.0
        ))
        .await
        .unwrap()
        .unwrap()
        .id
        .unwrap();

    let coin_reservation =
        coin_reservation::encode_object_ref(coin_metadata_id, SequenceNumber::new(), 0, 100)
            .unwrap();
    let err = try_coin_reservation_tx(&mut test_cluster, coin_reservation, sender, gas)
        .await
        .unwrap_err();
    dbg!(&err);
    assert!(err
        .to_string()
        .contains(format!("object id {} must be shared or immutable", coin_metadata_id).as_str()));

    // Finally, test a valid transfer
    let coin_reservation =
        coin_reservation::encode_object_ref(sui_metadata_id, SequenceNumber::new(), 0, 100)
            .unwrap();

    let res = try_coin_reservation_tx(&mut test_cluster, coin_reservation, sender, gas)
        .await
        .unwrap();
    assert!(res.effects.unwrap().status().is_ok());
}

async fn try_coin_reservation_tx(
    test_cluster: &mut TestCluster,
    coin_reservation: ObjectRef,
    sender: SuiAddress,
    gas: ObjectRef,
) -> anyhow::Result<SuiTransactionBlockResponse> {
    let rgp = test_cluster.get_reference_gas_price().await;
    // transfer the coin reservation obj ref back to a regular coin
    let mut builder = ProgrammableTransactionBuilder::new();

    let coin_res_arg = builder
        .obj(ObjectArg::ImmOrOwnedObject(coin_reservation))
        .unwrap();

    let recipient_arg = builder.pure(sender).unwrap();
    builder.command(Command::TransferObjects(vec![coin_res_arg], recipient_arg));

    let tx = TransactionKind::ProgrammableTransaction(builder.finish());
    let tx = TransactionData::new(tx, sender, gas, 10000000, rgp);

    let signed_tx = test_cluster.wallet.sign_transaction(&tx).await;
    test_cluster
        .wallet
        .execute_transaction_may_fail(signed_tx)
        .await
}
