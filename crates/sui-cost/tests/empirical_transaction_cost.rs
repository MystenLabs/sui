// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use insta::assert_json_snapshot;
use std::{collections::BTreeMap, path::PathBuf};
use sui_config::NetworkConfig;
use sui_config::ValidatorInfo;
use sui_cost::estimator::estimate_transaction_computation_cost;
use sui_cost::estimator::CommonTransactionCosts;
use sui_types::base_types::SuiAddress;
use sui_types::coin::PAY_JOIN_FUNC_NAME;
use sui_types::coin::PAY_MODULE_NAME;
use sui_types::coin::PAY_SPLIT_VEC_FUNC_NAME;
use sui_types::crypto::AccountKeyPair;
use sui_types::messages::VerifiedTransaction;
use sui_types::object::Object;
use sui_types::{
    gas::GasCostSummary,
    messages::{CallArg, ExecutionStatus, ObjectArg},
};
use test_utils::messages::make_transfer_object_transaction;
use test_utils::messages::make_transfer_sui_transaction;
use test_utils::messages::move_transaction_with_type_tags;
use test_utils::test_account_keys;
use test_utils::transaction::get_framework_object;
use test_utils::transaction::make_publish_package;
use test_utils::{
    authority::{spawn_test_authorities, test_authority_configs},
    messages::move_transaction,
    objects::test_gas_objects,
    transaction::{
        publish_counter_package, submit_shared_object_transaction, submit_single_owner_transaction,
    },
};

const TEST_DATA_DIR: &str = "tests/data/";

// Execute every entry function in Move framework and examples and ensure costs don't change
// To review snapshot changes, and fix snapshot differences,
// 0. Install cargo-insta
// 1. Run `cargo insta test --review` under `./sui-cost`.
// 2. Review, accept or reject changes.

#[tokio::test]
async fn test_good_snapshot() -> Result<(), anyhow::Error> {
    let mut common_costs_actual: BTreeMap<String, GasCostSummary> = BTreeMap::new();
    let mut common_costs_estimate: BTreeMap<String, GasCostSummary> = BTreeMap::new();

    run_actual_and_estimate_costs()
        .await?
        .iter()
        .for_each(|(k, (actual, estimate))| {
            common_costs_actual.insert(k.clone().to_string(), actual.clone());
            common_costs_estimate.insert(k.clone().to_string(), estimate.clone());
        });
    assert_json_snapshot!(common_costs_actual);
    assert_json_snapshot!(common_costs_estimate);

    Ok(())
}

async fn split_n_tx(
    n: u64,
    coin: &Object,
    gas: &Object,
    validator_info: &[ValidatorInfo],
) -> VerifiedTransaction {
    let split_amounts = vec![10u64; n as usize];
    let type_args = vec![coin.get_move_template_type().unwrap()];

    move_transaction_with_type_tags(
        gas.clone(),
        PAY_MODULE_NAME.as_str(),
        PAY_SPLIT_VEC_FUNC_NAME.as_str(),
        get_framework_object(validator_info)
            .await
            .compute_object_reference(),
        &type_args,
        vec![
            CallArg::Object(ObjectArg::ImmOrOwnedObject(coin.compute_object_reference())),
            CallArg::Pure(bcs::to_bytes(&split_amounts).unwrap()),
        ],
    )
}

async fn create_txes(
    sender: SuiAddress,
    keypair: &AccountKeyPair,
    gas_objects: &[Object],
    configs: &NetworkConfig,
) -> BTreeMap<CommonTransactionCosts, VerifiedTransaction> {
    let mut ret = BTreeMap::new();
    let mut gas_objects = gas_objects.to_vec().clone();
    // let _handles = spawn_test_authorities(gas_objects.clone(), configs).await;
    // Publish the move package to all authorities and get the new package ref.
    tokio::task::yield_now().await;
    tokio::time::sleep(std::time::Duration::from_secs(3)).await;

    //
    // Publish
    //
    let mut package_path = PathBuf::from(TEST_DATA_DIR);
    package_path.push("dummy_modules_publish");
    let publish_tx = make_publish_package(gas_objects.pop().unwrap(), package_path);
    ret.insert(CommonTransactionCosts::Publish, publish_tx);

    //
    // Transfer Whole Sui Coin and Transfer Portion of Sui Coin
    //
    let whole_sui_coin_tx = make_transfer_sui_transaction(
        gas_objects.pop().unwrap().compute_object_reference(),
        SuiAddress::default(),
        None,
        sender,
        keypair,
    );
    let partial_sui_coin_tx = make_transfer_sui_transaction(
        gas_objects.pop().unwrap().compute_object_reference(),
        SuiAddress::default(),
        Some(100),
        sender,
        keypair,
    );
    ret.insert(
        CommonTransactionCosts::TransferWholeSuiCoin,
        whole_sui_coin_tx,
    );
    ret.insert(
        CommonTransactionCosts::TransferPortionSuiCoin,
        partial_sui_coin_tx,
    );

    //
    // Transfer Whole Coin Object
    //
    let whole_coin_tx = make_transfer_object_transaction(
        gas_objects.pop().unwrap().compute_object_reference(),
        gas_objects.pop().unwrap().compute_object_reference(),
        sender,
        keypair,
        SuiAddress::default(),
    );

    ret.insert(CommonTransactionCosts::TransferWholeCoin, whole_coin_tx);

    //
    // Merge Two Coins
    //
    let c1 = gas_objects.pop().unwrap();
    let type_args = vec![c1.get_move_template_type().unwrap()];

    let merge_tx = move_transaction_with_type_tags(
        gas_objects.pop().unwrap(),
        PAY_MODULE_NAME.as_str(),
        PAY_JOIN_FUNC_NAME.as_str(),
        get_framework_object(configs.validator_set())
            .await
            .compute_object_reference(),
        &type_args,
        vec![
            CallArg::Object(ObjectArg::ImmOrOwnedObject(c1.compute_object_reference())),
            CallArg::Object(ObjectArg::ImmOrOwnedObject(
                gas_objects.pop().unwrap().compute_object_reference(),
            )),
        ],
    );
    ret.insert(CommonTransactionCosts::MergeCoin, merge_tx);

    //
    // Splt A Coin Into N Specific Amounts
    // Note spltting complexity does not depend on the amounts but only on the number of amounts
    //
    for n in 0..4 {
        let gas = gas_objects.pop().unwrap();
        let coin = gas_objects.pop().unwrap();
        let split_tx = split_n_tx(n, &gas, &coin, configs.validator_set())
            .await
            .clone();
        ret.insert(CommonTransactionCosts::SplitCoin(n as usize), split_tx);
    }

    //
    // Shared Object Section
    // Using the `counter` example
    //

    let package_ref =
        publish_counter_package(gas_objects.pop().unwrap(), configs.validator_set()).await;

    // Make a transaction to create a counter.
    tokio::task::yield_now().await;
    let transaction = move_transaction(
        gas_objects.pop().unwrap(),
        "counter",
        "create",
        package_ref,
        /* arguments */ Vec::default(),
    );
    let effects =
        submit_single_owner_transaction(transaction.clone(), configs.validator_set()).await;
    assert!(matches!(effects.status, ExecutionStatus::Success { .. }));
    let ((counter_id, counter_initial_shared_version, _), _) = effects.created[0];
    let counter_object_arg = ObjectArg::SharedObject {
        id: counter_id,
        initial_shared_version: counter_initial_shared_version,
    };

    ret.insert(CommonTransactionCosts::SharedCounterCreate, transaction);

    // Ensure the value of the counter is `0`.
    tokio::task::yield_now().await;
    let transaction = move_transaction(
        gas_objects.pop().unwrap(),
        "counter",
        "assert_value",
        package_ref,
        vec![
            CallArg::Object(counter_object_arg),
            CallArg::Pure(0u64.to_le_bytes().to_vec()),
        ],
    );

    ret.insert(
        CommonTransactionCosts::SharedCounterAssertValue,
        transaction,
    );

    // Make a transaction to increment the counter.
    tokio::task::yield_now().await;
    let transaction = move_transaction(
        gas_objects.pop().unwrap(),
        "counter",
        "increment",
        package_ref,
        vec![CallArg::Object(counter_object_arg)],
    );

    ret.insert(CommonTransactionCosts::SharedCounterIncrement, transaction);

    ret
}

async fn run_actual_and_estimate_costs(
) -> Result<BTreeMap<CommonTransactionCosts, (GasCostSummary, GasCostSummary)>, anyhow::Error> {
    let mut ret = BTreeMap::new();
    let gas_objects = test_gas_objects();
    let (sender, keypair) = test_account_keys().pop().unwrap();

    // Get the authority configs and spawn them. Note that it is important to not drop
    // the handles (or the authorities will stop).
    let configs = test_authority_configs();
    let handles = spawn_test_authorities(gas_objects.clone(), &configs).await;
    // Publish the move package to all authorities and get the new package ref.
    tokio::task::yield_now().await;
    tokio::time::sleep(std::time::Duration::from_secs(3)).await;

    let tx_map = create_txes(sender, &keypair, &gas_objects, &configs).await;

    for (tx_type, tx) in tx_map {
        let gas_used = if tx_type.is_shared_object_tx() {
            submit_shared_object_transaction(tx.clone(), configs.validator_set())
                .await
                .unwrap()
                .gas_cost_summary()
                .clone()
        } else {
            submit_single_owner_transaction(tx.clone(), configs.validator_set())
                .await
                .gas_cost_summary()
                .clone()
        };

        let gas_estimate = handles[0]
            .with_async(|node| async move {
                let state = node.state();
                estimate_transaction_computation_cost(
                    tx.into_inner().into_data().data,
                    state.clone(),
                    None,
                    None,
                    None,
                    None,
                )
                .await
                .unwrap()
            })
            .await;
        ret.insert(tx_type.clone(), (gas_used.clone(), gas_estimate.clone()));

        // Check that the estimates are not less than actual
        assert!(gas_used.computation_cost <= gas_estimate.computation_cost);
        assert!(gas_used.storage_cost <= gas_estimate.storage_cost);
        // Rebate should be less since this is returned
        assert!(gas_used.storage_rebate >= gas_estimate.storage_rebate);
    }
    Ok(ret)
}
