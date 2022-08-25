// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// This file covers running actual transactions and reporting the costs end to end

use std::{collections::BTreeMap, path::PathBuf};

// Execute every entry function in Move framework and examples and ensure costs don't change
use move_package::BuildConfig;
use serde::Serialize;
use sui::client_commands::{SuiClientCommandResult, SuiClientCommands};
use sui_json_rpc_types::SuiGasCostSummary;
use sui_types::{
    gas::GasCostSummary,
    messages::{CallArg, ExecutionStatus, ObjectArg},
};
use test_utils::{
    authority::{spawn_test_authorities, test_authority_configs},
    messages::move_transaction,
    network::setup_network_and_wallet,
    objects::test_gas_objects,
    transaction::{
        publish_counter_package, submit_shared_object_transaction, submit_single_owner_transaction,
    },
};

const TEST_DATA_DIR: &str = "tests/data/";

#[derive(Debug, Eq, PartialEq, Hash, Serialize, Ord, PartialOrd, Clone)]
pub enum CommonTransactionCosts {
    Publish,
    MergeCoin,
    SplitCoin(usize),
    SplitCoinEqual(usize),
    TransferWholeCoin,
    TransferPortionCoin,
    TransferWholeSuiCoin,
    TransferPortionSuiCoin,
    SharedCounterCreate,
    SharedCounterAssertValue,
    SharedCounterIncrement,
}

pub async fn run_common_tx_costs(
) -> Result<BTreeMap<CommonTransactionCosts, SuiGasCostSummary>, anyhow::Error> {
    let mut m = run_common_single_writer_tx_costs().await?;
    m.extend(run_counter_costs().await);
    Ok(m)
}

async fn run_common_single_writer_tx_costs(
) -> Result<BTreeMap<CommonTransactionCosts, SuiGasCostSummary>, anyhow::Error> {
    let mut ret = BTreeMap::new();

    async fn split_n(n: u64, equal_parts: bool) -> Result<SuiGasCostSummary, anyhow::Error> {
        let (_network, mut context, address) = setup_network_and_wallet().await?;

        let object_refs = context
            .gateway
            .read_api()
            .get_objects_owned_by_address(address)
            .await?;

        // Check log output contains all object ids.
        let gas = object_refs.first().unwrap().object_id;
        let coin = object_refs.get(1).unwrap().object_id;

        let amt_vec = if equal_parts {
            None
        } else {
            Some(vec![1000; n as usize])
        };
        let count = if equal_parts { n } else { 0 };

        // Test with gas specified
        let resp = SuiClientCommands::SplitCoin {
            gas: Some(gas),
            gas_budget: 10000,
            coin_id: coin,
            amounts: amt_vec,
            count,
        }
        .execute(&mut context)
        .await?;

        if let SuiClientCommandResult::SplitCoin(response) = resp {
            Ok(response.effects.gas_used)
        } else {
            unreachable!("Invalid response");
        }
    }

    for i in 0..4 {
        let gas_used = split_n(i, false).await?;
        ret.insert(CommonTransactionCosts::SplitCoin(i as usize), gas_used);
    }

    for i in 1..4 {
        let gas_used = split_n(i, true).await?;
        ret.insert(CommonTransactionCosts::SplitCoinEqual(i as usize), gas_used);
    }

    let (_network, mut context, address) = setup_network_and_wallet().await?;

    let object_refs = context
        .gateway
        .read_api()
        .get_objects_owned_by_address(address)
        .await?;

    // Pairwise Merge Coin
    let gas = object_refs.first().unwrap().object_id;
    let primary_coin = object_refs.get(1).unwrap().object_id;
    let coin_to_merge = object_refs.get(2).unwrap().object_id;

    // Test with gas specified
    let resp = SuiClientCommands::MergeCoin {
        primary_coin,
        coin_to_merge,
        gas: Some(gas),
        gas_budget: 10000,
    }
    .execute(&mut context)
    .await?;

    let gas_used = if let SuiClientCommandResult::MergeCoin(r) = resp {
        r.effects.gas_used
    } else {
        panic!("Command failed")
    };
    ret.insert(CommonTransactionCosts::MergeCoin, gas_used);

    // Publish Package
    // Provide path to well formed package sources
    let mut package_path = PathBuf::from(TEST_DATA_DIR);
    package_path.push("dummy_modules_publish");
    let build_config = BuildConfig::default();
    let resp = SuiClientCommands::Publish {
        package_path,
        build_config,
        gas: Some(gas),
        gas_budget: 10000,
    }
    .execute(&mut context)
    .await?;

    let gas_used = if let SuiClientCommandResult::Publish(response) = resp {
        response.effects.gas_used
    } else {
        unreachable!("Invalid response");
    };
    ret.insert(CommonTransactionCosts::Publish, gas_used);

    SuiClientCommands::SyncClientState {
        address: Some(address),
    }
    .execute(&mut context)
    .await?;

    let (_network, mut context, address) = setup_network_and_wallet().await?;

    let object_refs = context
        .gateway
        .read_api()
        .get_objects_owned_by_address(address)
        .await?;

    // Transfer a the whole object
    let obj_id = object_refs.get(1).unwrap().object_id;
    let recipient = context.keystore.addresses().get(1).cloned().unwrap();

    let resp = SuiClientCommands::Transfer {
        gas: None,
        to: recipient,
        object_id: obj_id,
        gas_budget: 10000,
    }
    .execute(&mut context)
    .await?;

    let gas_used = if let SuiClientCommandResult::Transfer(_, _, r) = resp {
        r.gas_used
    } else {
        panic!("Command failed")
    };
    ret.insert(CommonTransactionCosts::TransferWholeCoin, gas_used);

    // Transfer a portion of the coin
    let gas = object_refs.get(2).unwrap().object_id;
    let obj_id = object_refs.get(3).unwrap().object_id;
    let recipient = context.keystore.addresses().get(1).cloned().unwrap();

    let resp = SuiClientCommands::Transfer {
        gas: Some(gas),
        to: recipient,
        object_id: obj_id,
        gas_budget: 10000,
    }
    .execute(&mut context)
    .await?;

    let gas_used = if let SuiClientCommandResult::Transfer(_, _, r) = resp {
        r.gas_used
    } else {
        panic!("Command failed")
    };
    ret.insert(CommonTransactionCosts::TransferPortionCoin, gas_used);

    // Transfer the whole Sui object

    let obj_id = object_refs.get(4).unwrap().object_id;

    let resp = SuiClientCommands::TransferSui {
        amount: None,
        to: recipient,
        sui_coin_object_id: obj_id,
        gas_budget: 10000,
    }
    .execute(&mut context)
    .await?;

    let gas_used = if let SuiClientCommandResult::TransferSui(_, r) = resp {
        r.gas_used
    } else {
        panic!("Command failed")
    };
    ret.insert(CommonTransactionCosts::TransferWholeSuiCoin, gas_used);

    // Transfer a portion of Sui
    let (_network, mut context, address) = setup_network_and_wallet().await?;

    let object_refs = context
        .gateway
        .read_api()
        .get_objects_owned_by_address(address)
        .await?;
    let obj_id = object_refs.get(1).unwrap().object_id;

    let resp = SuiClientCommands::TransferSui {
        amount: Some(100),
        to: recipient,
        sui_coin_object_id: obj_id,
        gas_budget: 10000,
    }
    .execute(&mut context)
    .await?;

    let gas_used = if let SuiClientCommandResult::TransferSui(_, r) = resp {
        r.gas_used
    } else {
        panic!("Command failed")
    };
    ret.insert(CommonTransactionCosts::TransferPortionSuiCoin, gas_used);

    Ok(ret)
}

async fn run_counter_costs() -> BTreeMap<CommonTransactionCosts, SuiGasCostSummary> {
    run_counter_costs_impl()
        .await
        .into_iter()
        .map(|(k, v)| (k, SuiGasCostSummary::from(v)))
        .collect()
}

async fn run_counter_costs_impl() -> BTreeMap<CommonTransactionCosts, GasCostSummary> {
    let mut ret = BTreeMap::new();

    let mut gas_objects = test_gas_objects();

    // Get the authority configs and spawn them. Note that it is important to not drop
    // the handles (or the authorities will stop).
    let configs = test_authority_configs();
    let _handles = spawn_test_authorities(gas_objects.clone(), &configs).await;
    // Publish the move package to all authorities and get the new package ref.
    tokio::task::yield_now().await;
    tokio::time::sleep(std::time::Duration::from_secs(3)).await;
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
