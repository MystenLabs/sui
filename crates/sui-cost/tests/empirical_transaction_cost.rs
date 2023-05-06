// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use insta::assert_json_snapshot;
use std::{collections::BTreeMap, path::PathBuf};
use sui_config::NetworkConfig;
use sui_core::test_utils::make_transfer_object_transaction;
use sui_core::test_utils::make_transfer_sui_transaction;
use sui_types::base_types::SuiAddress;
use sui_types::coin::PAY_JOIN_FUNC_NAME;
use sui_types::coin::PAY_MODULE_NAME;
use sui_types::coin::PAY_SPLIT_VEC_FUNC_NAME;
use sui_types::crypto::{deterministic_random_account_key, AccountKeyPair};
use sui_types::object::{generate_test_gas_objects, Object};
use sui_types::transaction::VerifiedTransaction;
use sui_types::SUI_FRAMEWORK_OBJECT_ID;
use sui_types::{
    gas::GasCostSummary,
    transaction::{CallArg, ObjectArg},
};
use test_utils::authority::spawn_test_authorities;
use test_utils::transaction::make_publish_package;
use test_utils::{
    authority::test_authority_configs_with_objects,
    transaction::{
        publish_counter_package, submit_shared_object_transaction, submit_single_owner_transaction,
    },
};
// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use serde::{Deserialize, Serialize};
use strum_macros::Display;
use strum_macros::EnumString;
use sui_test_transaction_builder::TestTransactionBuilder;
use sui_types::effects::TransactionEffectsAPI;
use sui_types::execution_status::ExecutionStatus;

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

async fn split_n_tx(n: u64, coin: &Object, gas: &Object, gas_price: u64) -> VerifiedTransaction {
    let split_amounts = vec![10u64; n as usize];
    let type_args = vec![coin.get_move_template_type().unwrap()];

    let (sender, keypair) = deterministic_random_account_key();
    TestTransactionBuilder::new(sender, gas.compute_object_reference(), gas_price)
        .move_call(
            SUI_FRAMEWORK_OBJECT_ID,
            PAY_MODULE_NAME.as_str(),
            PAY_SPLIT_VEC_FUNC_NAME.as_str(),
            vec![
                CallArg::Object(ObjectArg::ImmOrOwnedObject(coin.compute_object_reference())),
                CallArg::Pure(bcs::to_bytes(&split_amounts).unwrap()),
            ],
        )
        .with_type_args(type_args)
        .build_and_sign(&keypair)
}

async fn create_txes(
    sender: SuiAddress,
    keypair: &AccountKeyPair,
    gas_objects: &[Object],
    configs: &NetworkConfig,
    gas_price: u64,
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
    let publish_tx = make_publish_package(
        gas_objects.pop().unwrap().compute_object_reference(),
        package_path,
        gas_price,
    );
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
        gas_price,
    );
    let partial_sui_coin_tx = make_transfer_sui_transaction(
        gas_objects.pop().unwrap().compute_object_reference(),
        SuiAddress::default(),
        Some(100),
        sender,
        keypair,
        gas_price,
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
        gas_price,
    );

    ret.insert(CommonTransactionCosts::TransferWholeCoin, whole_coin_tx);

    //
    // Merge Two Coins
    //
    let c1 = gas_objects.pop().unwrap();
    let type_args = vec![c1.get_move_template_type().unwrap()];

    let merge_tx = TestTransactionBuilder::new(
        sender,
        gas_objects.pop().unwrap().compute_object_reference(),
        gas_price,
    )
    .move_call(
        SUI_FRAMEWORK_OBJECT_ID,
        PAY_MODULE_NAME.as_str(),
        PAY_JOIN_FUNC_NAME.as_str(),
        vec![
            CallArg::Object(ObjectArg::ImmOrOwnedObject(c1.compute_object_reference())),
            CallArg::Object(ObjectArg::ImmOrOwnedObject(
                gas_objects.pop().unwrap().compute_object_reference(),
            )),
        ],
    )
    .with_type_args(type_args)
    .build_and_sign(keypair);
    ret.insert(CommonTransactionCosts::MergeCoin, merge_tx);

    //
    // Splt A Coin Into N Specific Amounts
    // Note spltting complexity does not depend on the amounts but only on the number of amounts
    //
    for n in 0..4 {
        let gas = gas_objects.pop().unwrap();
        let coin = gas_objects.pop().unwrap();
        let split_tx = split_n_tx(n, &gas, &coin, gas_price).await.clone();
        ret.insert(CommonTransactionCosts::SplitCoin(n as usize), split_tx);
    }

    //
    // Shared Object Section
    // Using the `counter` example
    //

    let package_id = publish_counter_package(
        gas_objects.pop().unwrap(),
        &configs.net_addresses(),
        gas_price,
    )
    .await
    .0;

    // Make a transaction to create a counter.
    tokio::task::yield_now().await;
    let transaction = TestTransactionBuilder::new(
        sender,
        gas_objects.pop().unwrap().compute_object_reference(),
        gas_price,
    )
    .call_counter_create(package_id)
    .build_and_sign(keypair);
    let (effects, _, _) =
        submit_single_owner_transaction(transaction.clone(), &configs.net_addresses()).await;
    assert!(matches!(effects.status(), ExecutionStatus::Success { .. }));
    let ((counter_id, counter_initial_shared_version, _), _) = effects.created()[0];

    ret.insert(CommonTransactionCosts::SharedCounterCreate, transaction);

    // Ensure the value of the counter is `0`.
    tokio::task::yield_now().await;
    let transaction = TestTransactionBuilder::new(
        sender,
        gas_objects.pop().unwrap().compute_object_reference(),
        gas_price,
    )
    .move_call(
        package_id,
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
    .build_and_sign(keypair);

    ret.insert(
        CommonTransactionCosts::SharedCounterAssertValue,
        transaction,
    );

    // Make a transaction to increment the counter.
    tokio::task::yield_now().await;
    let transaction = TestTransactionBuilder::new(
        sender,
        gas_objects.pop().unwrap().compute_object_reference(),
        gas_price,
    )
    .call_counter_increment(package_id, counter_id, counter_initial_shared_version)
    .build_and_sign(keypair);

    ret.insert(CommonTransactionCosts::SharedCounterIncrement, transaction);

    ret
}

async fn run_actual_costs(
) -> Result<BTreeMap<CommonTransactionCosts, GasCostSummary>, anyhow::Error> {
    let mut ret = BTreeMap::new();
    let gas_objects = generate_test_gas_objects();
    let (sender, keypair) = deterministic_random_account_key();

    // Get the authority configs and spawn them. Note that it is important to not drop
    // the handles (or the authorities will stop).
    let (configs, gas_objects) = test_authority_configs_with_objects(gas_objects);
    let authorities = spawn_test_authorities(&configs).await;
    let rgp = authorities
        .get(0)
        .unwrap()
        .with(|sui_node| sui_node.state().reference_gas_price_for_testing())
        .unwrap();

    // Publish the move package to all authorities and get the new package ref.
    tokio::task::yield_now().await;
    tokio::time::sleep(std::time::Duration::from_secs(3)).await;

    let tx_map = create_txes(sender, &keypair, &gas_objects, &configs, rgp).await;
    for (tx_type, tx) in tx_map {
        let gas_used = if tx_type.is_shared_object_tx() {
            submit_shared_object_transaction(tx, &configs.net_addresses())
                .await
                .unwrap()
                .0
                .gas_cost_summary()
                .clone()
        } else {
            submit_single_owner_transaction(tx, &configs.net_addresses())
                .await
                .0
                .gas_cost_summary()
                .clone()
        };

        ret.insert(tx_type, gas_used);
    }
    Ok(ret)
}
