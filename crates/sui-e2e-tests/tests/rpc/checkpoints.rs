// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use sui_macros::sim_test;
use sui_rpc_api::client::sdk::Client;
use sui_rpc_api::client::Client as CoreClient;
use test_cluster::TestClusterBuilder;

use crate::transfer_coin;

#[sim_test]
async fn list_checkpoint() {
    let test_cluster = TestClusterBuilder::new().build().await;

    let _transaction_digest = transfer_coin(&test_cluster.wallet).await;

    let client = Client::new(test_cluster.rpc_url()).unwrap();
    let core_client = CoreClient::new(test_cluster.rpc_url()).unwrap();

    let checkpoints = client
        .list_checkpoints(&Default::default())
        .await
        .unwrap()
        .into_inner();

    assert!(!checkpoints.is_empty());

    let _latest = client.get_latest_checkpoint().await.unwrap().into_inner();

    let _latest = core_client.get_latest_checkpoint().await.unwrap();
}

#[sim_test]
async fn get_checkpoint() {
    let test_cluster = TestClusterBuilder::new().build().await;

    let _transaction_digest = transfer_coin(&test_cluster.wallet).await;

    let client = Client::new(test_cluster.rpc_url()).unwrap();

    let latest = client.get_latest_checkpoint().await.unwrap().into_inner();
    let _ = client
        .get_checkpoint(latest.checkpoint.sequence_number)
        .await
        .unwrap();
}

#[sim_test]
async fn get_full_checkpoint() {
    let test_cluster = TestClusterBuilder::new().build().await;

    let _transaction_digest = transfer_coin(&test_cluster.wallet).await;

    let client = Client::new(test_cluster.rpc_url()).unwrap();
    let core_client = CoreClient::new(test_cluster.rpc_url()).unwrap();

    let latest = client.get_latest_checkpoint().await.unwrap().into_inner();
    let _ = core_client
        .get_full_checkpoint(latest.checkpoint.sequence_number)
        .await
        .unwrap();
}
