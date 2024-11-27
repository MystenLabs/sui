// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use sui_macros::sim_test;
use sui_rpc_api::client::sdk::Client;
use sui_rpc_api::rest::transactions::ListTransactionsQueryParameters;
use test_cluster::TestClusterBuilder;

use crate::transfer_coin;

#[sim_test]
async fn get_transaction() {
    let test_cluster = TestClusterBuilder::new().build().await;

    let transaction_digest = transfer_coin(&test_cluster.wallet).await;

    let client = Client::new(test_cluster.rpc_url()).unwrap();

    let _transaction = client.get_transaction(&transaction_digest).await.unwrap();
}

#[sim_test]
async fn list_checkpoint() {
    let test_cluster = TestClusterBuilder::new().build().await;

    let _transaction_digest = transfer_coin(&test_cluster.wallet).await;

    let client = Client::new(test_cluster.rpc_url()).unwrap();

    let transactions = client
        .list_transactions(&ListTransactionsQueryParameters::default())
        .await
        .unwrap()
        .into_inner();

    assert!(!transactions.is_empty());
}
