// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use insta::assert_json_snapshot;
use std::{collections::BTreeMap, path::PathBuf};
use sui_config::ValidatorInfo;
use sui_cost::estimator::estimate_transaction_computation_cost;
use sui_cost::estimator::{
    estimate_computational_costs_for_transaction, read_estimate_file, CommonTransactionCosts,
};
use sui_types::base_types::SuiAddress;
use sui_types::coin::COIN_JOIN_FUNC_NAME;
use sui_types::coin::COIN_MODULE_NAME;
use sui_types::coin::COIN_SPLIT_VEC_FUNC_NAME;
use sui_types::messages::Transaction;
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

#[tokio::test]
async fn test_transaction_estimator() -> Result<(), anyhow::Error> {
    let mut gas_objects = test_gas_objects();
    let (sender, keypair) = test_account_keys().pop().unwrap();
    // Get the authority configs and spawn them. Note that it is important to not drop
    // the handles (or the authorities will stop).
    let configs = test_authority_configs();
    let handles = spawn_test_authorities(gas_objects.clone(), &configs).await;
    // Publish the move package to all authorities and get the new package ref.
    tokio::task::yield_now().await;
    tokio::time::sleep(std::time::Duration::from_secs(3)).await;

    let mut txes = vec![];

    for n in 0..4 {
        let gas = gas_objects.pop().unwrap();
        let coin = gas_objects.pop().unwrap();
        let tx = split_n_tx(n, &gas, &coin, configs.validator_set())
            .await
            .clone();
        txes.push(tx);
    }

    for tx in txes {
        let state = handles[0].state();
        let v = estimate_transaction_computation_cost(
            tx.signed_data.data,
            state.clone(),
            1,
            1,
            100,
            100,
            0,
        )
        .await?;

        println!("{:?}", v);
    }

    Ok(())
}

// Helper function to split
async fn split_n_tx(
    n: u64,
    coin: &Object,
    gas: &Object,
    validator_info: &[ValidatorInfo],
) -> Transaction {
    let split_amounts = vec![10u64; n as usize];
    let type_args = vec![coin.get_move_template_type().unwrap()];

    move_transaction_with_type_tags(
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
    )
}
