// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use prost::Message;
use sui_macros::sim_test;
use sui_rest_api::client::sdk::Client;
use sui_rest_api::client::Client as CoreClient;
use sui_rest_api::{CheckpointResponse, ListCheckpointsQueryParameters};
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
        .list_checkpoints(&ListCheckpointsQueryParameters::default())
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
        .header(reqwest::header::ACCEPT, sui_rest_api::APPLICATION_JSON)
        .send()
        .await
        .unwrap()
        .json::<Vec<CheckpointResponse>>()
        .await
        .unwrap();

    // Make sure list works with protobuf
    let bytes = client
        .get(&url)
        .header(reqwest::header::ACCEPT, sui_rest_api::APPLICATION_PROTOBUF)
        .send()
        .await
        .unwrap()
        .bytes()
        .await
        .unwrap();
    let _checkpoints = sui_rest_api::proto::ListCheckpointResponse::decode(bytes).unwrap();

    // TODO remove this once the BCS format is no longer accepted and clients have migrated to the
    // protobuf version
    // Make sure list works with BCS and the old format of only a SignedCheckpoint with no contents
    let bytes = client
        .get(&url)
        .header(reqwest::header::ACCEPT, sui_rest_api::APPLICATION_BCS)
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

    let client = reqwest::Client::new();
    let url = format!(
        "{}/v2/checkpoints/{}",
        test_cluster.rpc_url(),
        latest.checkpoint.sequence_number
    );
    // Make sure list works with json
    let _checkpoints = client
        .get(&url)
        .header(reqwest::header::ACCEPT, sui_rest_api::APPLICATION_JSON)
        .send()
        .await
        .unwrap()
        .json::<CheckpointResponse>()
        .await
        .unwrap();

    // Make sure it works with protobuf
    let bytes = client
        .get(&url)
        .header(reqwest::header::ACCEPT, sui_rest_api::APPLICATION_PROTOBUF)
        .send()
        .await
        .unwrap()
        .bytes()
        .await
        .unwrap();
    let _checkpoints = sui_rest_api::proto::GetCheckpointResponse::decode(bytes).unwrap();

    // TODO remove this once the BCS format is no longer accepted and clients have migrated to the
    // protobuf version
    let bytes = client
        .get(&url)
        .header(reqwest::header::ACCEPT, sui_rest_api::APPLICATION_BCS)
        .send()
        .await
        .unwrap()
        .bytes()
        .await
        .unwrap();
    let _checkpoints = bcs::from_bytes::<CheckpointResponse>(&bytes).unwrap();
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
    // Make sure it works with protobuf
    let bytes = client
        .get(&url)
        .header(reqwest::header::ACCEPT, sui_rest_api::APPLICATION_PROTOBUF)
        .send()
        .await
        .unwrap()
        .bytes()
        .await
        .unwrap();
    let _checkpoints = sui_rest_api::proto::FullCheckpoint::decode(bytes).unwrap();

    // TODO remove this once the BCS format is no longer accepted and clients have migrated to the
    // protobuf version
    let bytes = client
        .get(&url)
        .header(reqwest::header::ACCEPT, sui_rest_api::APPLICATION_BCS)
        .send()
        .await
        .unwrap()
        .bytes()
        .await
        .unwrap();
    let _checkpoints =
        bcs::from_bytes::<sui_types::full_checkpoint_content::CheckpointData>(&bytes).unwrap();
}
