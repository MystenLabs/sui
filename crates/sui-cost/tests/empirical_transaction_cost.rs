// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use insta::assert_json_snapshot;
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, path::PathBuf};
use strum_macros::Display;
use strum_macros::EnumString;
use sui_json_rpc_types::SuiTransactionBlockEffectsAPI;
use sui_swarm_config::genesis_config::{AccountConfig, DEFAULT_GAS_AMOUNT};
use sui_test_transaction_builder::publish_basics_package_and_make_counter;
use sui_test_transaction_builder::TestTransactionBuilder;
use sui_types::base_types::{ObjectRef, SuiAddress};
use sui_types::coin::PAY_JOIN_FUNC_NAME;
use sui_types::coin::PAY_MODULE_NAME;
use sui_types::coin::PAY_SPLIT_VEC_FUNC_NAME;
use sui_types::gas_coin::GAS;
use sui_types::transaction::TransactionData;
use sui_types::SUI_FRAMEWORK_PACKAGE_ID;
use sui_types::{
    gas::GasCostSummary,
    transaction::{CallArg, ObjectArg},
};
use test_cluster::{TestCluster, TestClusterBuilder};

#[derive(
    Debug, Eq, PartialEq, Hash, Serialize, Deserialize, Ord, PartialOrd, Clone, Display, EnumString,
)]
pub enum CommonTransactionCosts {
    Publish,
    MergeCoin,
    SplitCoin(usize),
    TransferWholeCoin,
    TransferWholeSuiCoin,
    TransferPortionSuiCoin,
    SharedCounterCreate,
    SharedCounterAssertValue,
    SharedCounterIncrement,
}

impl CommonTransactionCosts {
    pub fn is_shared_object_tx(&self) -> bool {
        matches!(
            self,
            CommonTransactionCosts::SharedCounterAssertValue
                | CommonTransactionCosts::SharedCounterIncrement
        )
    }
}

const TEST_DATA_DIR: &str = "tests/data/";

// Execute every entry function in Move framework and examples and ensure costs don't change
// To review snapshot changes, and fix snapshot differences,
// 0. Install cargo-insta
// 1. Run `cargo insta test --review` under `./sui-cost`.
// 2. Review, accept or reject changes.

#[tokio::test]
async fn test_good_snapshot() -> Result<(), anyhow::Error> {
    let mut common_costs_actual: BTreeMap<String, GasCostSummary> = BTreeMap::new();

    run_actual_costs().await?.iter().for_each(|(k, actual)| {
        common_costs_actual.insert(k.to_string(), actual.clone());
    });
    assert_json_snapshot!(common_costs_actual);

    Ok(())
}

async fn split_n_tx(
    n: u64,
    coin: ObjectRef,
    gas: ObjectRef,
    gas_price: u64,
    sender: SuiAddress,
) -> TransactionData {
    let split_amounts = vec![10u64; n as usize];
    let type_args = vec![GAS::type_tag()];

    TestTransactionBuilder::new(sender, gas, gas_price)
        .move_call(
            SUI_FRAMEWORK_PACKAGE_ID,
            PAY_MODULE_NAME.as_str(),
            PAY_SPLIT_VEC_FUNC_NAME.as_str(),
            vec![
                CallArg::Object(ObjectArg::ImmOrOwnedObject(coin)),
                CallArg::Pure(bcs::to_bytes(&split_amounts).unwrap()),
            ],
        )
        .with_type_args(type_args)
        .build()
}

async fn create_txes(
    test_cluster: &TestCluster,
) -> BTreeMap<CommonTransactionCosts, TransactionData> {
    // Initial preparations to create a shared counter. This needs to be done first to not interfere
    // with the use of gas objects in the rest of this function.
    let (counter_package, counter) =
        publish_basics_package_and_make_counter(&test_cluster.wallet).await;
    let counter_package_id = counter_package.0;
    let (counter_id, counter_initial_shared_version) = (counter.0, counter.1);

    let mut ret = BTreeMap::new();
    let (sender, mut gas_objects) = test_cluster.wallet.get_one_account().await.unwrap();
    let gas_price = test_cluster.get_reference_gas_price().await;

    //
    // Publish
    //
    let mut package_path = PathBuf::from(TEST_DATA_DIR);
    package_path.push("dummy_modules_publish");
    let publish_tx = TestTransactionBuilder::new(sender, gas_objects.pop().unwrap(), gas_price)
        .publish(package_path)
        .build();
    ret.insert(CommonTransactionCosts::Publish, publish_tx);

    //
    // Transfer Whole Sui Coin and Transfer Portion of Sui Coin
    //
    let whole_sui_coin_tx =
        TestTransactionBuilder::new(sender, gas_objects.pop().unwrap(), gas_price)
            .transfer_sui(None, SuiAddress::default())
            .build();
    let partial_sui_coin_tx =
        TestTransactionBuilder::new(sender, gas_objects.pop().unwrap(), gas_price)
            .transfer_sui(Some(10), SuiAddress::default())
            .build();
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
    let whole_coin_tx = TestTransactionBuilder::new(sender, gas_objects.pop().unwrap(), gas_price)
        .transfer(gas_objects.pop().unwrap(), SuiAddress::default())
        .build();

    ret.insert(CommonTransactionCosts::TransferWholeCoin, whole_coin_tx);

    //
    // Merge Two Coins
    //
    let c1 = gas_objects.pop().unwrap();
    let type_args = vec![GAS::type_tag()];

    let merge_tx = TestTransactionBuilder::new(sender, gas_objects.pop().unwrap(), gas_price)
        .move_call(
            SUI_FRAMEWORK_PACKAGE_ID,
            PAY_MODULE_NAME.as_str(),
            PAY_JOIN_FUNC_NAME.as_str(),
            vec![
                CallArg::Object(ObjectArg::ImmOrOwnedObject(c1)),
                CallArg::Object(ObjectArg::ImmOrOwnedObject(gas_objects.pop().unwrap())),
            ],
        )
        .with_type_args(type_args)
        .build();
    ret.insert(CommonTransactionCosts::MergeCoin, merge_tx);

    //
    // Split A Coin Into N Specific Amounts
    // Note splitting complexity does not depend on the amounts but only on the number of amounts
    //
    for n in 0..4 {
        let gas = gas_objects.pop().unwrap();
        let coin = gas_objects.pop().unwrap();
        let split_tx = split_n_tx(n, gas, coin, gas_price, sender).await.clone();
        ret.insert(CommonTransactionCosts::SplitCoin(n as usize), split_tx);
    }

    //
    // Shared Object Section
    // Using the `counter` example
    //

    let transaction = TestTransactionBuilder::new(sender, gas_objects.pop().unwrap(), gas_price)
        .call_counter_create(counter_package_id)
        .build();

    ret.insert(CommonTransactionCosts::SharedCounterCreate, transaction);

    let transaction = TestTransactionBuilder::new(sender, gas_objects.pop().unwrap(), gas_price)
        .move_call(
            counter_package_id,
            "counter",
            "assert_value",
            vec![
                CallArg::Object(ObjectArg::SharedObject {
                    id: counter_id,
                    initial_shared_version: counter_initial_shared_version,
                    mutable: true,
                }),
                CallArg::Pure(0u64.to_le_bytes().to_vec()),
            ],
        )
        .build();

    ret.insert(
        CommonTransactionCosts::SharedCounterAssertValue,
        transaction,
    );

    // Make a transaction to increment the counter.
    let transaction = TestTransactionBuilder::new(sender, gas_objects.pop().unwrap(), gas_price)
        .call_counter_increment(
            counter_package_id,
            counter_id,
            counter_initial_shared_version,
        )
        .build();

    ret.insert(CommonTransactionCosts::SharedCounterIncrement, transaction);

    ret
}

async fn run_actual_costs(
) -> Result<BTreeMap<CommonTransactionCosts, GasCostSummary>, anyhow::Error> {
    let mut ret = BTreeMap::new();
    let test_cluster = TestClusterBuilder::new()
        .with_accounts(vec![AccountConfig {
            gas_amounts: vec![DEFAULT_GAS_AMOUNT; 30],
            address: None,
        }])
        .build()
        .await;

    let tx_map = create_txes(&test_cluster).await;
    for (tx_type, tx) in tx_map {
        let gas_used = test_cluster
            .sign_and_execute_transaction(&tx)
            .await
            .effects
            .unwrap()
            .gas_cost_summary()
            .clone();

        ret.insert(tx_type, gas_used);
    }
    Ok(ret)
}
