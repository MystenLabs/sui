// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::transfer_coin;
use sui_macros::sim_test;
use sui_rpc_api::Client;
use sui_sdk_types::Address;
use sui_sdk_types::BalanceChange;
use sui_test_transaction_builder::make_transfer_sui_transaction;
use sui_types::base_types::SuiAddress;
use sui_types::effects::TransactionEffectsAPI;
use sui_types::transaction::TransactionDataAPI;
use test_cluster::TestClusterBuilder;

#[sim_test]
async fn get_object() {
    let test_cluster = TestClusterBuilder::new().build().await;

    let id: Address = "0x5".parse().unwrap();

    let client = Client::new(test_cluster.rpc_url()).unwrap();

    let _object = client.get_object(id.into()).await.unwrap();

    let _object = client
        .get_object_with_version(id.into(), 1.into())
        .await
        .unwrap();
}

#[sim_test]
async fn execute_transaction_transfer() {
    let test_cluster = TestClusterBuilder::new().build().await;

    let client = Client::new(test_cluster.rpc_url()).unwrap();
    let address = SuiAddress::random_for_testing_only();
    let amount = 9;

    let txn =
        make_transfer_sui_transaction(&test_cluster.wallet, Some(address), Some(amount)).await;
    let sender = txn.transaction_data().sender();

    let response = client.execute_transaction(&txn).await.unwrap();

    let gas = response.effects.gas_cost_summary().net_gas_usage();

    let coin_type = sui_types::sui_sdk_types_conversions::type_tag_core_to_sdk(
        sui_types::gas_coin::GAS::type_tag(),
    )
    .unwrap();
    let mut expected = vec![
        BalanceChange {
            address: sender.into(),
            coin_type: coin_type.clone(),
            amount: -(amount as i128 + gas as i128),
        },
        BalanceChange {
            address: address.into(),
            coin_type,
            amount: amount as i128,
        },
    ];
    expected.sort_by_key(|e| e.address);

    let mut actual = response.balance_changes;
    actual.sort_by_key(|e| e.address);

    assert_eq!(actual, expected);
}

#[sim_test]
async fn get_full_checkpoint() {
    let test_cluster = TestClusterBuilder::new().build().await;

    let _transaction_digest = transfer_coin(&test_cluster.wallet).await;

    let client = Client::new(test_cluster.rpc_url()).unwrap();

    let latest = client.get_latest_checkpoint().await.unwrap().into_data();
    let _ = client
        .get_full_checkpoint(latest.sequence_number)
        .await
        .unwrap();
}
