// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use sui_macros::sim_test;
use sui_rpc_api::client::sdk::Client;
use sui_rpc_api::proto::node::node_client::NodeClient;
use sui_rpc_api::proto::node::{
    GetTransactionOptions, GetTransactionRequest, GetTransactionResponse,
};
use sui_rpc_api::rest::transactions::ListTransactionsCursorParameters;
use test_cluster::TestClusterBuilder;

use crate::{stake_with_validator, transfer_coin};

#[sim_test]
async fn get_transaction() {
    let test_cluster = TestClusterBuilder::new().build().await;

    let transaction_digest = stake_with_validator(&test_cluster).await;

    let client = Client::new(test_cluster.rpc_url()).unwrap();

    let _transaction = client.get_transaction(&transaction_digest).await.unwrap();

    let mut grpc_client = NodeClient::connect(test_cluster.rpc_url().to_owned())
        .await
        .unwrap();

    // Request default fields
    let GetTransactionResponse {
        digest,
        transaction,
        transaction_bcs,
        signatures,
        signatures_bytes,
        effects,
        effects_bcs,
        events,
        events_bcs,
        checkpoint,
        timestamp,
    } = grpc_client
        .get_transaction(GetTransactionRequest::new(transaction_digest))
        .await
        .unwrap()
        .into_inner();

    assert!(digest.is_some());
    assert!(transaction.is_some());
    assert!(transaction_bcs.is_none());
    assert!(signatures.is_some());
    assert!(signatures_bytes.is_none());
    assert!(effects.is_some());
    assert!(effects_bcs.is_none());
    assert!(events.is_some());
    assert!(events_bcs.is_none());
    assert!(checkpoint.is_some());
    assert!(timestamp.is_some());

    // Request no fields
    let GetTransactionResponse {
        digest,
        transaction,
        transaction_bcs,
        signatures,
        signatures_bytes,
        effects,
        effects_bcs,
        events,
        events_bcs,
        checkpoint,
        timestamp,
    } = grpc_client
        .get_transaction(
            GetTransactionRequest::new(transaction_digest)
                .with_options(GetTransactionOptions::none()),
        )
        .await
        .unwrap()
        .into_inner();

    assert!(digest.is_some());
    assert!(transaction.is_none());
    assert!(transaction_bcs.is_none());
    assert!(signatures.is_none());
    assert!(signatures_bytes.is_none());
    assert!(effects.is_none());
    assert!(effects_bcs.is_none());
    assert!(events.is_none());
    assert!(events_bcs.is_none());
    assert!(checkpoint.is_some());
    assert!(timestamp.is_some());

    // Request all fields
    let response = grpc_client
        .get_transaction(
            GetTransactionRequest::new(transaction_digest)
                .with_options(GetTransactionOptions::all()),
        )
        .await
        .unwrap()
        .into_inner();

    let GetTransactionResponse {
        digest,
        transaction,
        transaction_bcs,
        signatures,
        signatures_bytes,
        effects,
        effects_bcs,
        events,
        events_bcs,
        checkpoint,
        timestamp,
    } = &response;

    assert!(digest.is_some());
    assert!(transaction.is_some());
    assert!(transaction_bcs.is_some());
    assert!(signatures.is_some());
    assert!(signatures_bytes.is_some());
    assert!(effects.is_some());
    assert!(effects_bcs.is_some());
    assert!(events.is_some());
    assert!(events_bcs.is_some());
    assert!(checkpoint.is_some());
    assert!(timestamp.is_some());

    // ensure we can convert proto GetTransactionResponse type to rust TransactionResponse
    sui_rpc_api::types::TransactionResponse::try_from(&response).unwrap();
}

#[sim_test]
async fn list_transactions() {
    let test_cluster = TestClusterBuilder::new().build().await;

    let _transaction_digest = transfer_coin(&test_cluster.wallet).await;

    let client = Client::new(test_cluster.rpc_url()).unwrap();

    let transactions = client
        .list_transactions(&ListTransactionsCursorParameters::default())
        .await
        .unwrap()
        .into_inner();

    assert!(!transactions.is_empty());
}
