// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use sui_macros::sim_test;
use sui_rpc_api::client::sdk::Client;
use sui_rpc_api::client::Client as CoreClient;
use sui_rpc_api::CheckpointResponse;
use sui_sdk_types::types::SignedCheckpointSummary;
use test_cluster::TestClusterBuilder;

use crate::transfer_coin;

#[sim_test]
async fn list_checkpoint() {
    let test_cluster = TestClusterBuilder::new().build().await;

    let _transaction_digest = transfer_coin(&test_cluster.wallet).await;

    let client = Client::new(test_cluster.rpc_url()).unwrap();
    let core_client = CoreClient::new(test_cluster.rpc_url());

    let checkpoints = client
        .list_checkpoints(&Default::default())
        .await
        .unwrap()
        .into_inner();

    assert!(!checkpoints.is_empty());

    let _latest = client.get_latest_checkpoint().await.unwrap().into_inner();

    let _latest = core_client.get_latest_checkpoint().await.unwrap();

    let client = reqwest::Client::new();
    let url = format!("{}/v2/checkpoints", test_cluster.rpc_url());
    // Make sure list works with json
    let _checkpoints = client
        .get(&url)
        .header(reqwest::header::ACCEPT, sui_rpc_api::rest::APPLICATION_JSON)
        .send()
        .await
        .unwrap()
        .json::<Vec<CheckpointResponse>>()
        .await
        .unwrap();

    // TODO remove this once the BCS format is no longer supported by the rest endpoint and clients
    // wanting binary have migrated to grpc
    //
    // Make sure list works with BCS and the old format of only a SignedCheckpoint with no contents
    let bytes = client
        .get(&url)
        .header(reqwest::header::ACCEPT, sui_rpc_api::rest::APPLICATION_BCS)
        .send()
        .await
        .unwrap()
        .bytes()
        .await
        .unwrap();
    let _checkpoints = bcs::from_bytes::<Vec<SignedCheckpointSummary>>(&bytes).unwrap();
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
    let core_client = CoreClient::new(test_cluster.rpc_url());

    let latest = client.get_latest_checkpoint().await.unwrap().into_inner();
    let _ = client
        .get_full_checkpoint(latest.checkpoint.sequence_number)
        .await
        .unwrap();
    let _ = core_client
        .get_full_checkpoint(latest.checkpoint.sequence_number)
        .await
        .unwrap();

    let client = reqwest::Client::new();
    let url = format!(
        "{}/v2/checkpoints/{}/full",
        test_cluster.rpc_url(),
        latest.checkpoint.sequence_number
    );

    // TODO remove this once the BCS format is no longer supported by the rest endpoint and clients
    // wanting binary have migrated to grpc
    let bytes = client
        .get(&url)
        .header(reqwest::header::ACCEPT, sui_rpc_api::rest::APPLICATION_BCS)
        .send()
        .await
        .unwrap()
        .bytes()
        .await
        .unwrap();
    let _checkpoints =
        bcs::from_bytes::<sui_types::full_checkpoint_content::CheckpointData>(&bytes).unwrap();
}
