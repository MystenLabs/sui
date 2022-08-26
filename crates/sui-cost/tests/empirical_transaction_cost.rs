// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use insta::assert_yaml_snapshot;
use std::fs;
use std::{collections::BTreeMap, path::PathBuf};
use sui_config::ValidatorInfo;
use sui_cost::estimator::ESTIMATE_FILE;
use sui_cost::estimator::{
    estimate_computational_costs_for_transaction, read_estimate_file, CommonTransactionCosts,
};
use sui_types::base_types::SuiAddress;
use sui_types::coin::COIN_JOIN_FUNC_NAME;
use sui_types::coin::COIN_MODULE_NAME;
use sui_types::coin::COIN_SPLIT_VEC_FUNC_NAME;
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
use test_utils::{
    authority::{spawn_test_authorities, test_authority_configs},
    messages::move_transaction,
    objects::test_gas_objects,
    transaction::{
        publish_counter_package, publish_package_for_effects, submit_shared_object_transaction,
        submit_single_owner_transaction,
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
    let common_costs = run_common_tx_costs().await?;
    assert_yaml_snapshot!(common_costs);
    Ok(())
}

#[tokio::test]
async fn check_estimates() {
    // Generate the estimates to file
    generate_estimates().await.unwrap();

    // Read the estimates
    let cost_map = read_estimate_file().unwrap();

    // Check that Sui Transfer estimate
    let mut gas_objects = test_gas_objects();
    let (sender, keypair) = test_account_keys().pop().unwrap();
    let whole_sui_coin_tx = make_transfer_sui_transaction(
        gas_objects.pop().unwrap().compute_object_reference(),
        SuiAddress::default(),
        None,
        sender,
        &keypair,
    );
    let partial_sui_coin_tx = make_transfer_sui_transaction(
        gas_objects.pop().unwrap().compute_object_reference(),
        SuiAddress::default(),
        Some(100),
        sender,
        &keypair,
    );

    let e1 = estimate_computational_costs_for_transaction(whole_sui_coin_tx.signed_data.data.kind)
        .unwrap();
    let e2 =
        estimate_computational_costs_for_transaction(partial_sui_coin_tx.signed_data.data.kind)
            .unwrap();

    assert_eq!(
        e1,
        cost_map
            .get(&CommonTransactionCosts::TransferWholeSuiCoin)
            .unwrap()
            .clone()
    );
    assert_eq!(
        e2,
        cost_map
            .get(&CommonTransactionCosts::TransferPortionSuiCoin)
            .unwrap()
            .clone()
    );
}

async fn generate_estimates() -> Result<(), anyhow::Error> {
    let common_costs: BTreeMap<_, _> = run_common_tx_costs()
        .await?
        .iter()
        .map(|(k, v)| (format!("{k}"), v.clone()))
        .collect();

    let out_string = toml::to_string(&common_costs).unwrap();

    fs::write(ESTIMATE_FILE, out_string).expect("Could not write estimator to file!");
    Ok(())
}

pub async fn run_common_tx_costs(
) -> Result<BTreeMap<CommonTransactionCosts, GasCostSummary>, anyhow::Error> {
    Ok(run_counter_costs().await)
}

pub async fn run_counter_costs() -> BTreeMap<CommonTransactionCosts, GasCostSummary> {
    run_cost_test().await.into_iter().collect()
}

async fn run_cost_test() -> BTreeMap<CommonTransactionCosts, GasCostSummary> {
    let mut ret = BTreeMap::new();
    let mut gas_objects = test_gas_objects();
    let (sender, keypair) = test_account_keys().pop().unwrap();

    // Get the authority configs and spawn them. Note that it is important to not drop
    // the handles (or the authorities will stop).
    let configs = test_authority_configs();
    let _handles = spawn_test_authorities(gas_objects.clone(), &configs).await;
    // Publish the move package to all authorities and get the new package ref.
    tokio::task::yield_now().await;
    tokio::time::sleep(std::time::Duration::from_secs(3)).await;

    //
    // Publish
    //
    let mut package_path = PathBuf::from(TEST_DATA_DIR);
    package_path.push("dummy_modules_publish");
    let gas_used = publish_package_for_effects(
        gas_objects.pop().unwrap(),
        package_path,
        configs.validator_set(),
    )
    .await
    .gas_cost_summary()
    .clone();
    ret.insert(CommonTransactionCosts::Publish, gas_used);

    //
    // Transfer Whole Sui Coin and Transfer Portion of Sui Coin
    //
    let whole_sui_coin_tx = make_transfer_sui_transaction(
        gas_objects.pop().unwrap().compute_object_reference(),
        SuiAddress::default(),
        None,
        sender,
        &keypair,
    );
    let partial_sui_coin_tx = make_transfer_sui_transaction(
        gas_objects.pop().unwrap().compute_object_reference(),
        SuiAddress::default(),
        Some(100),
        sender,
        &keypair,
    );

    let whole_sui_coin_tx_gas_used =
        submit_single_owner_transaction(whole_sui_coin_tx, configs.validator_set())
            .await
            .gas_cost_summary()
            .clone();
    let partial_sui_coin_tx_gas_used =
        submit_single_owner_transaction(partial_sui_coin_tx, configs.validator_set())
            .await
            .gas_cost_summary()
            .clone();

    ret.insert(
        CommonTransactionCosts::TransferWholeSuiCoin,
        whole_sui_coin_tx_gas_used,
    );
    ret.insert(
        CommonTransactionCosts::TransferPortionSuiCoin,
        partial_sui_coin_tx_gas_used,
    );

    //
    // Transfer Whole Coin Object
    //
    let whole_coin_tx = make_transfer_object_transaction(
        gas_objects.pop().unwrap().compute_object_reference(),
        gas_objects.pop().unwrap().compute_object_reference(),
        sender,
        &keypair,
        SuiAddress::default(),
    );

    let whole_coin_tx_gas_used =
        submit_single_owner_transaction(whole_coin_tx, configs.validator_set())
            .await
            .gas_cost_summary()
            .clone();

    ret.insert(
        CommonTransactionCosts::TransferWholeCoin,
        whole_coin_tx_gas_used,
    );

    //
    // Merge Two Coins
    //
    let c1 = gas_objects.pop().unwrap();
    let type_args = vec![c1.get_move_template_type().unwrap()];

    let merge_tx = move_transaction_with_type_tags(
        gas_objects.pop().unwrap(),
        COIN_MODULE_NAME.as_str(),
        COIN_JOIN_FUNC_NAME.as_str(),
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

    let merge_tx_gas_used = submit_single_owner_transaction(merge_tx, configs.validator_set())
        .await
        .gas_cost_summary()
        .clone();

    ret.insert(CommonTransactionCosts::MergeCoin, merge_tx_gas_used);

    //
    // Splt A Coin Into N Specific Amounts
    // Note spltting complexity does not depend on the amounts but only on the number of amounts
    //

    for n in 0..4 {
        let gas = gas_objects.pop().unwrap();
        let coin = gas_objects.pop().unwrap();
        let split_gas_used = split_n(n, &gas, &coin, configs.validator_set())
            .await
            .clone();
        ret.insert(
            CommonTransactionCosts::SplitCoin(n as usize),
            split_gas_used,
        );
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
    let effects = submit_single_owner_transaction(transaction, configs.validator_set()).await;
    assert!(matches!(effects.status, ExecutionStatus::Success { .. }));
    let ((counter_id, _, _), _) = effects.created[0];

    let gas_used = effects.gas_used;
    ret.insert(CommonTransactionCosts::SharedCounterCreate, gas_used);

    // Ensure the value of the counter is `0`.
    tokio::task::yield_now().await;
    let transaction = move_transaction(
        gas_objects.pop().unwrap(),
        "counter",
        "assert_value",
        package_ref,
        vec![
            CallArg::Object(ObjectArg::SharedObject(counter_id)),
            CallArg::Pure(0u64.to_le_bytes().to_vec()),
        ],
    );
    let effects = submit_shared_object_transaction(transaction, &configs.validator_set()[0..1])
        .await
        .unwrap();
    assert!(matches!(effects.status, ExecutionStatus::Success { .. }));

    let gas_used = effects.gas_used;
    ret.insert(CommonTransactionCosts::SharedCounterAssertValue, gas_used);

    // Make a transaction to increment the counter.
    tokio::task::yield_now().await;
    let transaction = move_transaction(
        gas_objects.pop().unwrap(),
        "counter",
        "increment",
        package_ref,
        vec![CallArg::Object(ObjectArg::SharedObject(counter_id))],
    );
    let effects = submit_shared_object_transaction(transaction, &configs.validator_set()[0..1])
        .await
        .unwrap();
    assert!(matches!(effects.status, ExecutionStatus::Success { .. }));

    let gas_used = effects.gas_used;
    ret.insert(CommonTransactionCosts::SharedCounterIncrement, gas_used);

    ret
}

// Helper function to split
async fn split_n(
    n: u64,
    coin: &Object,
    gas: &Object,
    validator_info: &[ValidatorInfo],
) -> GasCostSummary {
    let split_amounts = vec![10u64; n as usize];
    let type_args = vec![coin.get_move_template_type().unwrap()];

    let split_tx = move_transaction_with_type_tags(
        gas.clone(),
        COIN_MODULE_NAME.as_str(),
        COIN_SPLIT_VEC_FUNC_NAME.as_str(),
        get_framework_object(validator_info)
            .await
            .compute_object_reference(),
        &type_args,
        vec![
            CallArg::Object(ObjectArg::ImmOrOwnedObject(coin.compute_object_reference())),
            CallArg::Pure(bcs::to_bytes(&split_amounts).unwrap()),
        ],
    );

    submit_single_owner_transaction(split_tx, validator_info)
        .await
        .gas_cost_summary()
        .clone()
}
