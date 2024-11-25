// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use prost::Message;
use sui_macros::sim_test;
use sui_rest_api::client::sdk::Client;
use sui_rest_api::rest::transactions::{ListTransactionsQueryParameters, TransactionResponse};
use test_cluster::TestClusterBuilder;

use crate::transfer_coin;

#[sim_test]
async fn get_transaction() {
    let test_cluster = TestClusterBuilder::new().build().await;

    let transaction_digest = transfer_coin(&test_cluster.wallet).await;

    let client = Client::new(test_cluster.rpc_url()).unwrap();

    let _transaction = client.get_transaction(&transaction_digest).await.unwrap();

    let client = reqwest::Client::new();
    let url = format!(
        "{}/v2/transactions/{}",
        test_cluster.rpc_url(),
        transaction_digest,
    );
    // Make sure it works with json
    let _transaction = client
        .get(&url)
        .header(
            reqwest::header::ACCEPT,
            sui_rest_api::rest::APPLICATION_JSON,
        )
        .send()
        .await
        .unwrap()
        .json::<TransactionResponse>()
        .await
        .unwrap();

    // Make sure it works with protobuf
    let bytes = client
        .get(&url)
        .header(
            reqwest::header::ACCEPT,
            sui_rest_api::rest::APPLICATION_PROTOBUF,
        )
        .send()
        .await
        .unwrap()
        .bytes()
        .await
        .unwrap();
    let _transaction = sui_rest_api::proto::GetTransactionResponse::decode(bytes).unwrap();

    // TODO remove this once the BCS format is no longer accepted and clients have migrated to the
    // protobuf version
    let bytes = client
        .get(&url)
        .header(reqwest::header::ACCEPT, sui_rest_api::rest::APPLICATION_BCS)
        .send()
        .await
        .unwrap()
        .bytes()
        .await
        .unwrap();
    let _transaction = bcs::from_bytes::<TransactionResponse>(&bytes).unwrap();
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

    let client = reqwest::Client::new();
    let url = format!("{}/v2/transactions", test_cluster.rpc_url());
    // Make sure it works with json
    let _transactions = client
        .get(&url)
        .header(
            reqwest::header::ACCEPT,
            sui_rest_api::rest::APPLICATION_JSON,
        )
        .send()
        .await
        .unwrap()
        .json::<Vec<TransactionResponse>>()
        .await
        .unwrap();

    // Make sure it works with protobuf
    let bytes = client
        .get(&url)
        .header(
            reqwest::header::ACCEPT,
            sui_rest_api::rest::APPLICATION_PROTOBUF,
        )
        .send()
        .await
        .unwrap()
        .bytes()
        .await
        .unwrap();
    let _transactions = sui_rest_api::proto::ListTransactionsResponse::decode(bytes).unwrap();

    // TODO remove this once the BCS format is no longer accepted and clients have migrated to the
    // protobuf version
    let bytes = client
        .get(&url)
        .header(reqwest::header::ACCEPT, sui_rest_api::rest::APPLICATION_BCS)
        .send()
        .await
        .unwrap()
        .bytes()
        .await
        .unwrap();
    let _transactions = bcs::from_bytes::<Vec<TransactionResponse>>(&bytes).unwrap();
}
