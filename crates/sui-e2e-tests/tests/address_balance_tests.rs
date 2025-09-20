// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use move_core_types::identifier::Identifier;
use std::path::PathBuf;
use sui_json_rpc_types::SuiTransactionBlockEffectsAPI;
use sui_keys::keystore::AccountKeystore;
use sui_macros::*;
use sui_protocol_config::ProtocolConfig;
use sui_sdk::wallet_context::WalletContext;
use sui_test_transaction_builder::TestTransactionBuilder;
use sui_types::{
    accumulator_metadata::AccumulatorOwner,
    accumulator_root::{AccumulatorValue, U128},
    balance::Balance,
    base_types::{ObjectID, ObjectRef, SuiAddress},
    digests::{ChainIdentifier, CheckpointDigest},
    effects::TransactionEffectsAPI,
    gas::GasCostSummary,
    gas_coin::GAS,
    programmable_transaction_builder::ProgrammableTransactionBuilder,
    storage::ChildObjectResolver,
    transaction::{Argument, Command, TransactionData, TransactionKind},
    SUI_FRAMEWORK_PACKAGE_ID,
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
        "Accumulator value should be {expected_balance}"
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
async fn test_address_balance_gas() {
    let _guard = ProtocolConfig::apply_overrides_for_testing(|_, mut cfg| {
        cfg.enable_address_balance_gas_payments_for_testing();
        cfg
    });

    let mut test_cluster = TestClusterBuilder::new().build().await;
    let rgp = test_cluster.get_reference_gas_price().await;
    let context = &mut test_cluster.wallet;

    let (sender, gas) = get_sender_and_gas(context).await;
    let gas_package_id = setup_test_package(context).await;

    let deposit_tx = make_send_to_account_tx(10_000_000, sender, sender, gas, rgp);
    test_cluster.sign_and_execute_transaction(&deposit_tx).await;

    test_cluster.fullnode_handle.sui_node.with(|node| {
        let state = node.state();
        let child_object_resolver = state.get_child_object_resolver().as_ref();
        verify_accumulator_exists(child_object_resolver, sender, 10_000_000);
    });

    let chain_id = test_cluster.get_chain_identifier();

    let tx = create_test_transaction_address_balance(sender, gas_package_id, rgp, chain_id);

    let signed_tx = test_cluster.sign_transaction(&tx).await;
    let (effects, _) = test_cluster
        .execute_transaction_return_raw_effects(signed_tx)
        .await
        .expect("Transaction should succeed with address balance gas payment");

    assert!(
        effects.status().is_ok(),
        "Expected transaction to succeed with address balance gas payment, but got error: {:?}",
        effects.status()
    );

    let gas_summary = effects.gas_cost_summary();
    let gas_used = calculate_total_gas_cost(gas_summary);

    assert!(
        gas_used > 0,
        "Gas used should be greater than 0 with Move function calls, got: {}",
        gas_used
    );

    let expected_balance = 10_000_000 - gas_used;

    test_cluster.fullnode_handle.sui_node.with(|node| {
        let state = node.state();
        let child_object_resolver = state.get_child_object_resolver().as_ref();
        verify_accumulator_exists(child_object_resolver, sender, expected_balance);
    });

    test_cluster.trigger_reconfiguration().await;
}

async fn setup_test_package(context: &mut WalletContext) -> ObjectID {
    let mut move_test_code_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    move_test_code_path.push("tests/move_test_code");

    let (sender, gas_object) = context.get_one_gas_object().await.unwrap().unwrap();
    let gas_price = context.get_reference_gas_price().await.unwrap();
    let txn = context
        .sign_transaction(
            &TestTransactionBuilder::new(sender, gas_object, gas_price)
                .publish(move_test_code_path)
                .build(),
        )
        .await;
    let resp = context.execute_transaction_must_succeed(txn).await;
    let package_ref = resp.get_new_package_obj().unwrap();
    package_ref.0
}

fn calculate_total_gas_cost(gas_summary: &GasCostSummary) -> u64 {
    gas_summary.computation_cost + gas_summary.storage_cost + gas_summary.non_refundable_storage_fee
}

fn create_storage_test_transaction_kind(gas_test_package_id: ObjectID) -> TransactionKind {
    let mut builder = ProgrammableTransactionBuilder::new();
    let value = builder.pure(42u64).unwrap();
    builder.programmable_move_call(
        gas_test_package_id,
        Identifier::new("gas_test").unwrap(),
        Identifier::new("create_object_with_storage").unwrap(),
        vec![],
        vec![value],
    );
    TransactionKind::ProgrammableTransaction(builder.finish())
}

fn create_abort_test_transaction_kind(
    gas_test_package_id: ObjectID,
    should_abort: bool,
) -> TransactionKind {
    let mut builder = ProgrammableTransactionBuilder::new();
    let should_abort_arg = builder.pure(should_abort).unwrap();
    builder.programmable_move_call(
        gas_test_package_id,
        Identifier::new("gas_test").unwrap(),
        Identifier::new("abort_with_computation").unwrap(),
        vec![],
        vec![should_abort_arg],
    );
    TransactionKind::ProgrammableTransaction(builder.finish())
}

fn create_test_transaction_gas(
    sender: SuiAddress,
    gas_test_package_id: ObjectID,
    gas_coin: ObjectRef,
    rgp: u64,
) -> TransactionData {
    use sui_types::transaction::{GasData, TransactionDataV1, TransactionExpiration};

    let tx = create_storage_test_transaction_kind(gas_test_package_id);

    TransactionData::V1(TransactionDataV1 {
        kind: tx,
        sender,
        gas_data: GasData {
            payment: vec![gas_coin],
            owner: sender,
            price: rgp,
            budget: 10000000,
        },
        expiration: TransactionExpiration::None,
    })
}

fn create_test_transaction_address_balance(
    sender: SuiAddress,
    gas_test_package_id: ObjectID,
    rgp: u64,
    chain_id: ChainIdentifier,
) -> TransactionData {
    use sui_types::transaction::{GasData, TransactionDataV1, TransactionExpiration};

    let tx = create_storage_test_transaction_kind(gas_test_package_id);

    TransactionData::V1(TransactionDataV1 {
        kind: tx,
        sender,
        gas_data: GasData {
            payment: vec![],
            owner: sender,
            price: rgp,
            budget: 10000000,
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

fn create_abort_test_transaction_address_balance(
    sender: SuiAddress,
    gas_test_package_id: ObjectID,
    rgp: u64,
    chain_id: ChainIdentifier,
    should_abort: bool,
) -> TransactionData {
    use sui_types::transaction::{GasData, TransactionDataV1, TransactionExpiration};

    let tx = create_abort_test_transaction_kind(gas_test_package_id, should_abort);

    TransactionData::V1(TransactionDataV1 {
        kind: tx,
        sender,
        gas_data: GasData {
            payment: vec![],
            owner: sender,
            price: rgp,
            budget: 10000000,
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
            payment: vec![gas_coin],
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
                err_str.contains("InvalidExpirationChainId"),
                "Expected InvalidExpirationChainId error, got: {:?}",
                err
            );
        }
        Ok(_) => panic!("Transaction should be rejected with invalid chain ID"),
    }
}

#[sim_test]
async fn test_address_balance_gas_cost_parity() {
    let _guard = ProtocolConfig::apply_overrides_for_testing(|_, mut cfg| {
        cfg.enable_address_balance_gas_payments_for_testing();
        cfg
    });

    let mut test_cluster = TestClusterBuilder::new().build().await;

    let rgp = test_cluster.get_reference_gas_price().await;
    let chain_id = test_cluster.get_chain_identifier();

    let gas_test_package_id = setup_test_package(&mut test_cluster.wallet).await;

    let (sender, gas_for_deposit) = get_sender_and_gas(&mut test_cluster.wallet).await;

    let deposit_tx = make_send_to_account_tx(10_000_000, sender, sender, gas_for_deposit, rgp);
    test_cluster.sign_and_execute_transaction(&deposit_tx).await;

    let gas_coin = test_cluster
        .wallet
        .get_one_gas_object()
        .await
        .unwrap()
        .unwrap()
        .1;
    let coin_tx = create_test_transaction_gas(sender, gas_test_package_id, gas_coin, rgp);
    let coin_resp = test_cluster.sign_and_execute_transaction(&coin_tx).await;
    let coin_effects = coin_resp.effects.as_ref().unwrap();

    assert!(
        coin_effects.status().is_ok(),
        "Coin payment storage transaction should succeed, but got error: {:?}",
        coin_effects.status()
    );

    let address_balance_tx =
        create_test_transaction_address_balance(sender, gas_test_package_id, rgp, chain_id);
    let address_balance_resp = test_cluster
        .sign_and_execute_transaction(&address_balance_tx)
        .await;
    let address_balance_effects = address_balance_resp.effects.as_ref().unwrap();

    assert!(
        address_balance_effects.status().is_ok(),
        "Address balance payment storage transaction should succeed, but got error: {:?}",
        address_balance_effects.status()
    );

    let coin_gas_summary = coin_effects.gas_cost_summary();
    let address_balance_gas_summary = address_balance_effects.gas_cost_summary();

    assert!(
        coin_gas_summary.storage_cost > 0,
        "Coin payment should have storage costs for object creation"
    );
    assert!(
        address_balance_gas_summary.storage_cost > 0,
        "Address balance payment should have storage costs for object creation"
    );

    assert_eq!(
        coin_gas_summary.computation_cost, address_balance_gas_summary.computation_cost,
        "Computation costs should be identical for the same transaction"
    );

    test_cluster.trigger_reconfiguration().await;
}

#[sim_test]
async fn test_address_balance_gas_charged_on_move_abort() {
    let _guard = ProtocolConfig::apply_overrides_for_testing(|_, mut cfg| {
        cfg.enable_address_balance_gas_payments_for_testing();
        cfg
    });

    let mut test_cluster = TestClusterBuilder::new().build().await;
    let rgp = test_cluster.get_reference_gas_price().await;
    let chain_id = test_cluster.get_chain_identifier();

    let gas_test_package_id = setup_test_package(&mut test_cluster.wallet).await;

    let (sender, gas_for_deposit) = get_sender_and_gas(&mut test_cluster.wallet).await;

    let deposit_tx = make_send_to_account_tx(10_000_000, sender, sender, gas_for_deposit, rgp);
    test_cluster.sign_and_execute_transaction(&deposit_tx).await;

    test_cluster.fullnode_handle.sui_node.with(|node| {
        let state = node.state();
        let child_object_resolver = state.get_child_object_resolver().as_ref();
        verify_accumulator_exists(child_object_resolver, sender, 10_000_000);
    });

    let abort_tx = create_abort_test_transaction_address_balance(
        sender,
        gas_test_package_id,
        rgp,
        chain_id,
        true,
    );
    let signed_tx = test_cluster.sign_transaction(&abort_tx).await;
    let (effects, _) = test_cluster
        .execute_transaction_return_raw_effects(signed_tx)
        .await
        .expect("Transaction execution should succeed even with Move abort");

    assert!(
        effects.status().is_err(),
        "Expected transaction to fail due to Move abort"
    );

    let gas_summary = effects.gas_cost_summary();
    let gas_used = calculate_total_gas_cost(gas_summary);

    assert!(
        gas_used > 0,
        "Gas should still be charged even on Move abort, got: {}",
        gas_used
    );
    assert!(
        gas_summary.computation_cost > 0,
        "Computation cost should be charged for work done before abort"
    );

    let expected_balance = 10_000_000 - gas_used;

    test_cluster.fullnode_handle.sui_node.with(|node| {
        let state = node.state();
        let child_object_resolver = state.get_child_object_resolver().as_ref();
        verify_accumulator_exists(child_object_resolver, sender, expected_balance);
    });

    test_cluster.trigger_reconfiguration().await;
}
