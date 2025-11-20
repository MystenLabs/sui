// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use move_core_types::{identifier::Identifier, u256::U256};
use shared_crypto::intent::Intent;
use sui_json_rpc_types::SuiTransactionBlockEffectsAPI;
use sui_keys::keystore::AccountKeystore;
use sui_macros::*;
use sui_protocol_config::ProtocolConfig;
use sui_test_transaction_builder::publish_package;
use sui_types::{
    SUI_FRAMEWORK_PACKAGE_ID,
    base_types::{ObjectID, ObjectRef, SuiAddress},
    digests::ChainIdentifier,
    effects::TransactionEffectsAPI,
    gas::GasCostSummary,
    gas_coin::GAS,
    programmable_transaction_builder::ProgrammableTransactionBuilder,
    transaction::{
        FundsWithdrawalArg, GasData, TransactionData, TransactionDataAPI, TransactionDataV1,
        TransactionExpiration, TransactionKind,
    },
};
use test_cluster::{TestCluster, TestClusterBuilder};

async fn setup_test_cluster() -> (TestCluster, ObjectID) {
    let _guard = ProtocolConfig::apply_overrides_for_testing(|_, mut cfg| {
        cfg.enable_address_balance_gas_payments_for_testing();
        cfg
    });

    let test_cluster = TestClusterBuilder::new()
        .with_num_validators(1)
        .build()
        .await;

    let mut path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    path.push("tests/move_test_code");
    let (gas_test_package_id, _, _) = publish_package(&test_cluster.wallet, path).await;

    (test_cluster, gas_test_package_id)
}

async fn execute_txn_and_check(
    test_cluster: &TestCluster,
    tx: &TransactionData,
    expect_success: bool,
) -> sui_types::effects::TransactionEffects {
    let sender = tx.sender();
    let gas_owner = tx.gas_owner();

    let signed_tx = if sender != gas_owner {
        let sender_sig = test_cluster
            .wallet
            .config
            .keystore
            .sign_secure(&sender, tx, Intent::sui_transaction())
            .await
            .unwrap();
        let sponsor_sig = test_cluster
            .wallet
            .config
            .keystore
            .sign_secure(&gas_owner, tx, Intent::sui_transaction())
            .await
            .unwrap();
        sui_types::transaction::Transaction::from_data(tx.clone(), vec![sender_sig, sponsor_sig])
    } else {
        test_cluster.sign_transaction(tx).await
    };

    let (effects, _) = test_cluster
        .execute_transaction_return_raw_effects(signed_tx)
        .await
        .unwrap();

    if expect_success {
        assert!(
            effects.status().is_ok(),
            "Expected success: {:?}",
            effects.status()
        );
    } else {
        assert!(
            effects.status().is_err(),
            "Expected failure: {:?}",
            effects.status()
        );
    }

    effects
}

async fn execute_soft_bundle_expecting_one_insufficient_balance(
    test_cluster: &TestCluster,
    tx_1: &TransactionData,
    tx_2: &TransactionData,
) -> u64 {
    let mut effects = test_cluster
        .sign_and_execute_txns_in_soft_bundle(&[tx_1.clone(), tx_2.clone()])
        .await
        .unwrap();

    let digests = [effects[0].0, effects[1].0];
    test_cluster.wait_for_tx_settlement(&digests).await;

    let tx2_effects = effects.pop().unwrap().1;
    let tx1_effects = effects.pop().unwrap().1;

    let tx1_total_gas = calculate_total_gas_cost(tx1_effects.gas_cost_summary());
    let tx2_total_gas = calculate_total_gas_cost(tx2_effects.gas_cost_summary());

    let (succeeded_gas, failed_gas) = if tx1_effects.status().is_ok() {
        assert!(
            tx1_effects.status().is_ok(),
            "One transaction should succeed"
        );
        assert!(!tx2_effects.status().is_ok(), "One transaction should fail");
        (tx1_total_gas, tx2_total_gas)
    } else {
        assert!(
            tx2_effects.status().is_ok(),
            "One transaction should succeed"
        );
        assert!(!tx1_effects.status().is_ok(), "One transaction should fail");
        (tx2_total_gas, tx1_total_gas)
    };

    assert!(
        succeeded_gas > 0,
        "Successful transaction should have gas charged"
    );
    assert_eq!(
        failed_gas, 0,
        "Failed transaction should have zero gas charged"
    );

    let failed_effects = if tx1_effects.status().is_err() {
        &tx1_effects
    } else {
        &tx2_effects
    };
    let status_str = format!("{:?}", failed_effects.status());
    assert!(
        status_str.contains("InsufficientBalanceForWithdraw"),
        "Failed transaction should have InsufficientBalanceForWithdraw error"
    );

    succeeded_gas
}

fn build_tx(
    kind: TransactionKind,
    sender: SuiAddress,
    sponsor: Option<SuiAddress>,
    rgp: u64,
    chain_id: ChainIdentifier,
    nonce: u32,
) -> TransactionData {
    let gas_owner = sponsor.unwrap_or(sender);
    TransactionData::V1(TransactionDataV1 {
        kind,
        sender,
        gas_data: GasData {
            payment: vec![],
            owner: gas_owner,
            price: rgp,
            budget: 10000000,
        },
        expiration: TransactionExpiration::ValidDuring {
            min_epoch: Some(0),
            max_epoch: Some(0),
            min_timestamp_seconds: None,
            max_timestamp_seconds: None,
            chain: chain_id,
            nonce,
        },
    })
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

fn create_delete_kind(
    gas_test_package_id: ObjectID,
    object_to_delete: ObjectRef,
) -> TransactionKind {
    let mut builder = ProgrammableTransactionBuilder::new();
    let object_arg = builder
        .obj(sui_types::transaction::ObjectArg::ImmOrOwnedObject(
            object_to_delete,
        ))
        .unwrap();
    builder.programmable_move_call(
        gas_test_package_id,
        Identifier::new("gas_test").unwrap(),
        Identifier::new("delete_object").unwrap(),
        vec![],
        vec![object_arg],
    );
    TransactionKind::ProgrammableTransaction(builder.finish())
}

fn create_abort_kind(gas_test_package_id: ObjectID, should_abort: bool) -> TransactionKind {
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

fn create_withdraw_balance_kind(withdraw_amount: u64, sender: SuiAddress) -> TransactionKind {
    let mut builder = ProgrammableTransactionBuilder::new();

    let withdraw_arg = FundsWithdrawalArg::balance_from_sender(
        withdraw_amount,
        sui_types::type_input::TypeInput::from(sui_types::gas_coin::GAS::type_tag()),
    );
    let withdraw_arg = builder.funds_withdrawal(withdraw_arg).unwrap();

    let amount_arg = builder.pure(U256::from(withdraw_amount)).unwrap();

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

    TransactionKind::ProgrammableTransaction(builder.finish())
}

fn calculate_total_gas_cost(gas_summary: &GasCostSummary) -> u64 {
    gas_summary.computation_cost + gas_summary.storage_cost + gas_summary.non_refundable_storage_fee
}

#[sim_test]
async fn test_address_balance_gas() {
    let (test_cluster, gas_package_id) = setup_test_cluster().await;
    let initial_balance = 10_000_000;
    let sender = test_cluster
        .get_address_0_with_balance(initial_balance)
        .await;

    let rgp = test_cluster.get_reference_gas_price().await;

    let chain_id = test_cluster.get_chain_identifier();

    let tx = build_tx(
        create_storage_test_transaction_kind(gas_package_id),
        sender,
        None,
        rgp,
        chain_id,
        0,
    );

    let effects = execute_txn_and_check(&test_cluster, &tx, true).await;
    let gas_used = calculate_total_gas_cost(effects.gas_cost_summary());
    assert_eq!(
        test_cluster.get_address_balance(sender),
        initial_balance - gas_used
    );

    test_cluster.trigger_reconfiguration().await;
}

#[sim_test]
async fn test_sponsored_address_balance_storage_rebates() {
    let (test_cluster, gas_test_package_id) = setup_test_cluster().await;

    let initial_balance = 100_000_000;
    let sender = test_cluster
        .get_address_0_with_balance(initial_balance)
        .await;
    let sponsor = test_cluster
        .get_address_1_with_balance(initial_balance)
        .await;

    let chain_id = test_cluster.get_chain_identifier();
    let rgp = test_cluster.wallet.get_reference_gas_price().await.unwrap();

    let create_txn = build_tx(
        create_storage_test_transaction_kind(gas_test_package_id),
        sender,
        Some(sponsor),
        rgp,
        chain_id,
        0,
    );

    let create_effects = execute_txn_and_check(&test_cluster, &create_txn, true).await;

    let sponsor_actual = test_cluster.get_address_balance(sponsor);
    let sender_actual = test_cluster.get_address_balance(sender);

    assert!(
        sponsor_actual < initial_balance,
        "Sponsor balance should have decreased from {}, got: {}",
        initial_balance,
        sponsor_actual
    );
    assert_eq!(
        sender_actual, initial_balance,
        "Sender balance should remain at {}, got: {}",
        initial_balance, sender_actual
    );

    let created = create_effects.created();
    let created_objects: Vec<_> = created.iter().collect();
    assert_eq!(
        created_objects.len(),
        1,
        "Should have created exactly one object"
    );
    let created_obj = created_objects[0].0;
    let delete_txn = build_tx(
        create_delete_kind(gas_test_package_id, created_obj),
        sender,
        Some(sponsor),
        rgp,
        chain_id,
        0,
    );

    let delete_effects = execute_txn_and_check(&test_cluster, &delete_txn, true).await;
    assert!(
        delete_effects.gas_cost_summary().storage_rebate > 0,
        "Should receive storage rebate when deleting object"
    );

    let sponsor_final = test_cluster.get_address_balance(sponsor);
    let sender_final = test_cluster.get_address_balance(sender);

    assert_eq!(
        sender_final, initial_balance,
        "Sender balance should remain unchanged at {}",
        initial_balance
    );
    assert_ne!(
        sponsor_final, initial_balance,
        "Sponsor balance should have changed from {}",
        initial_balance
    );

    test_cluster.trigger_reconfiguration().await;
}

#[sim_test]
async fn test_address_balance_gas_cost_parity() {
    let (test_cluster, gas_test_package_id) = setup_test_cluster().await;
    let initial_balance = 100_000_000;
    let sender = test_cluster
        .get_address_0_with_balance(initial_balance)
        .await;

    let rgp = test_cluster.get_reference_gas_price().await;
    let chain_id = test_cluster.get_chain_identifier();

    let gas_coin = test_cluster.get_one_sender_and_gas().await.1;
    let tx = create_storage_test_transaction_kind(gas_test_package_id);
    let coin_tx = TransactionData::V1(TransactionDataV1 {
        kind: tx,
        sender,
        gas_data: GasData {
            payment: vec![gas_coin],
            owner: sender,
            price: rgp,
            budget: 10000000,
        },
        expiration: TransactionExpiration::None,
    });
    let coin_resp = test_cluster.sign_and_execute_transaction(&coin_tx).await;
    let coin_effects = coin_resp.effects.as_ref().unwrap();

    assert!(
        coin_effects.status().is_ok(),
        "Coin payment storage transaction should succeed, but got error: {:?}",
        coin_effects.status()
    );

    let address_balance_tx = build_tx(
        create_storage_test_transaction_kind(gas_test_package_id),
        sender,
        None,
        rgp,
        chain_id,
        0,
    );
    let address_balance_effects =
        execute_txn_and_check(&test_cluster, &address_balance_tx, true).await;

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
    assert!(
        coin_gas_summary.storage_cost > address_balance_gas_summary.storage_cost,
        "Gas coin storage cost should be higher due to gas coin mutation overhead"
    );
    assert_eq!(
        coin_gas_summary.computation_cost, address_balance_gas_summary.computation_cost,
        "Computation costs should be identical for the same transaction"
    );

    let coin_created_objects: Vec<_> = coin_effects.created().iter().collect();
    let address_balance_created = address_balance_effects.created();
    let address_balance_created_objects: Vec<_> = address_balance_created.iter().collect();

    assert_eq!(
        coin_created_objects.len(),
        1,
        "Should have created exactly one object with coin payment"
    );
    assert_eq!(
        address_balance_created_objects.len(),
        1,
        "Should have created exactly one object with address balance payment"
    );

    let coin_created_obj = coin_created_objects[0].reference.to_object_ref();
    let address_balance_created_obj = address_balance_created_objects[0].0;

    let gas_coin_for_delete = test_cluster.get_one_sender_and_gas().await.1;

    let tx = create_delete_kind(gas_test_package_id, coin_created_obj);
    let coin_delete_tx = TransactionData::V1(TransactionDataV1 {
        kind: tx,
        sender,
        gas_data: GasData {
            payment: vec![gas_coin_for_delete],
            owner: sender,
            price: rgp,
            budget: 10000000,
        },
        expiration: TransactionExpiration::None,
    });
    let coin_delete_resp = test_cluster
        .sign_and_execute_transaction(&coin_delete_tx)
        .await;
    let coin_delete_effects = coin_delete_resp.effects.as_ref().unwrap();

    let address_balance_delete_tx = build_tx(
        create_delete_kind(gas_test_package_id, address_balance_created_obj),
        sender,
        None,
        rgp,
        chain_id,
        0,
    );
    let address_balance_delete_effects =
        execute_txn_and_check(&test_cluster, &address_balance_delete_tx, true).await;

    let coin_delete_gas_summary = coin_delete_effects.gas_cost_summary();
    let address_balance_delete_gas_summary = address_balance_delete_effects.gas_cost_summary();

    assert!(
        coin_delete_effects.status().is_ok(),
        "Coin deletion should succeed"
    );
    assert!(
        address_balance_delete_effects.status().is_ok(),
        "Address balance deletion should succeed"
    );

    assert!(
        coin_delete_gas_summary.storage_rebate > 0,
        "Coin deletion should provide non-zero storage rebate"
    );
    assert!(
        address_balance_delete_gas_summary.storage_rebate > 0,
        "Address balance deletion should provide non-zero storage rebate"
    );

    assert_eq!(
        coin_delete_gas_summary.computation_cost,
        address_balance_delete_gas_summary.computation_cost,
        "Deletion computation costs should be identical for both payment methods"
    );

    test_cluster.trigger_reconfiguration().await;
}

#[sim_test]
async fn test_address_balance_gas_charged_on_move_abort() {
    let (test_cluster, gas_test_package_id) = setup_test_cluster().await;
    let initial_balance = 10_000_000;
    let sender = test_cluster
        .get_address_0_with_balance(initial_balance)
        .await;

    let rgp = test_cluster.get_reference_gas_price().await;
    let chain_id = test_cluster.get_chain_identifier();

    let abort_tx = build_tx(
        create_abort_kind(gas_test_package_id, true),
        sender,
        None,
        rgp,
        chain_id,
        0,
    );

    let effects = execute_txn_and_check(&test_cluster, &abort_tx, false).await;
    let gas_used = calculate_total_gas_cost(effects.gas_cost_summary());
    assert_eq!(
        test_cluster.get_address_balance(sender),
        initial_balance - gas_used
    );

    test_cluster.trigger_reconfiguration().await;
}

#[sim_test]
async fn test_explicit_sponsor_withdrawal_banned() {
    let (test_cluster, _gas_test_package_id) = setup_test_cluster().await;

    let addresses = test_cluster.get_addresses();
    let sender = addresses[0];
    let sponsor = addresses[1];

    let chain_id = test_cluster.get_chain_identifier();
    let rgp = test_cluster.wallet.get_reference_gas_price().await.unwrap();

    let mut builder = ProgrammableTransactionBuilder::new();
    let withdrawal = FundsWithdrawalArg::balance_from_sponsor(
        1000,
        sui_types::type_input::TypeInput::from(GAS::type_tag()),
    );
    builder.funds_withdrawal(withdrawal).unwrap();

    let tx = build_tx(
        TransactionKind::ProgrammableTransaction(builder.finish()),
        sender,
        Some(sponsor),
        rgp,
        chain_id,
        0,
    );

    let signed_tx = test_cluster.sign_transaction(&tx).await;
    let result = test_cluster
        .execute_transaction_return_raw_effects(signed_tx)
        .await;

    let err = result.expect_err("Transaction with explicit sponsor withdrawal should be rejected");
    let err_str = format!("{:?}", err);
    assert!(
        err_str.contains("Explicit sponsor withdrawals are not yet supported"),
        "Error should mention that sponsor withdrawals are not supported, got: {}",
        err_str
    );
}

#[sim_test]
async fn test_sponsor_insufficient_balance_charges_zero_gas() {
    let (test_cluster, gas_test_package_id) = setup_test_cluster().await;

    let sender_initial_balance = 100_000_000;
    let sponsor_initial_balance = 15_000_000;

    let sender = test_cluster
        .get_address_0_with_balance(sender_initial_balance)
        .await;
    let sponsor = test_cluster
        .get_address_1_with_balance(sponsor_initial_balance)
        .await;

    let chain_id = test_cluster.get_chain_identifier();
    let rgp = test_cluster.wallet.get_reference_gas_price().await.unwrap();

    let tx1 = build_tx(
        create_storage_test_transaction_kind(gas_test_package_id),
        sender,
        Some(sponsor),
        rgp,
        chain_id,
        0,
    );
    let tx2 = build_tx(
        create_storage_test_transaction_kind(gas_test_package_id),
        sender,
        Some(sponsor),
        rgp,
        chain_id,
        1,
    );

    let successful_tx_gas =
        execute_soft_bundle_expecting_one_insufficient_balance(&test_cluster, &tx1, &tx2).await;

    let final_sponsor_balance = test_cluster.get_address_balance(sponsor);
    assert_eq!(
        final_sponsor_balance,
        sponsor_initial_balance - successful_tx_gas,
        "Sponsor balance should reflect only the successful transaction"
    );

    let final_sender_balance = test_cluster.get_address_balance(sender);
    assert_eq!(
        final_sender_balance, sender_initial_balance,
        "Sender balance should remain unchanged"
    );

    test_cluster.trigger_reconfiguration().await;
}

#[sim_test]
async fn test_insufficient_balance_charges_zero_gas() {
    let (test_cluster, _gas_test_package_id) = setup_test_cluster().await;

    let initial_balance = 30_000_000u64;
    let withdraw_amount = 15_000_000u64;

    let sender = test_cluster
        .get_address_0_with_balance(initial_balance)
        .await;

    let rgp = test_cluster.get_reference_gas_price().await;
    let chain_id = test_cluster.get_chain_identifier();

    let tx1 = build_tx(
        create_withdraw_balance_kind(withdraw_amount, sender),
        sender,
        None,
        rgp,
        chain_id,
        0,
    );
    let tx2 = build_tx(
        create_withdraw_balance_kind(withdraw_amount, sender),
        sender,
        None,
        rgp,
        chain_id,
        1,
    );

    let successful_tx_gas =
        execute_soft_bundle_expecting_one_insufficient_balance(&test_cluster, &tx1, &tx2).await;

    let final_sender_balance = test_cluster.get_address_balance(sender);
    assert_eq!(
        final_sender_balance,
        initial_balance - withdraw_amount - successful_tx_gas,
        "Final balance should reflect only the successful transaction"
    );

    test_cluster.trigger_reconfiguration().await;
}

#[sim_test]
async fn test_soft_bundle_different_gas_payers() {
    let (test_cluster, gas_test_package_id) = setup_test_cluster().await;

    let sender1_initial_balance = 10_000_000;
    let sender2_initial_balance = 10_000_000;
    let sender1 = test_cluster
        .get_address_0_with_balance(sender1_initial_balance)
        .await;
    let sender2 = test_cluster
        .get_address_1_with_balance(sender2_initial_balance)
        .await;

    let rgp = test_cluster.wallet.get_reference_gas_price().await.unwrap();
    let chain_id = test_cluster.get_chain_identifier();

    let tx1 = build_tx(
        create_storage_test_transaction_kind(gas_test_package_id),
        sender1,
        None,
        rgp,
        chain_id,
        0,
    );

    let tx2 = build_tx(
        create_storage_test_transaction_kind(gas_test_package_id),
        sender2,
        None,
        rgp,
        chain_id,
        1,
    );

    let tx1_digest = tx1.digest();
    let tx2_digest = tx2.digest();

    assert_ne!(
        tx1_digest, tx2_digest,
        "Transaction digests should be different"
    );

    let mut effects = test_cluster
        .sign_and_execute_txns_in_soft_bundle(&[tx1, tx2])
        .await
        .unwrap();

    let tx2_effects = effects.pop().unwrap().1;
    let tx1_effects = effects.pop().unwrap().1;

    assert!(tx1_effects.status().is_ok(), "Transaction 1 should succeed");
    assert!(tx2_effects.status().is_ok(), "Transaction 2 should succeed");

    let gas_summary1 = tx1_effects.gas_cost_summary();
    let gas_used1 = calculate_total_gas_cost(gas_summary1);
    let gas_summary2 = tx2_effects.gas_cost_summary();
    let gas_used2 = calculate_total_gas_cost(gas_summary2);

    let expected_balance1 = sender1_initial_balance - gas_used1;
    let expected_balance2 = sender2_initial_balance - gas_used2;

    test_cluster
        .wait_for_tx_settlement(&[tx1_digest, tx2_digest])
        .await;

    let actual_balance1 = test_cluster.get_address_balance(sender1);
    let actual_balance2 = test_cluster.get_address_balance(sender2);

    assert_eq!(
        actual_balance1, expected_balance1,
        "Sender1 balance should be {} after gas deduction, got {}",
        expected_balance1, actual_balance1
    );
    assert_eq!(
        actual_balance2, expected_balance2,
        "Sender2 balance should be {} after gas deduction, got {}",
        expected_balance2, actual_balance2
    );

    test_cluster.trigger_reconfiguration().await;
}
