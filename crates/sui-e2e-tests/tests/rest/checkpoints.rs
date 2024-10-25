// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use prost::Message;
use sui_macros::sim_test;
use sui_rest_api::client::sdk::Client;
use sui_rest_api::client::Client as CoreClient;
use sui_rest_api::{CheckpointResponse, ListCheckpointsQueryParameters};
use sui_sdk_types::types::SignedCheckpointSummary;
use test_cluster::TestClusterBuilder;

#[sim_test]
async fn list_checkpoint() {
    let test_cluster = TestClusterBuilder::new().build().await;

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
    let _checkpoints = sui_rest_api::proto::CheckpointPage::decode(bytes).unwrap();

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
