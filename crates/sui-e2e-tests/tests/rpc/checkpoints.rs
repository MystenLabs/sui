// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use sui_macros::sim_test;
use sui_rpc_api::client::sdk::Client;
use sui_rpc_api::client::Client as CoreClient;
use sui_rpc_api::proto::node::node_client::NodeClient;
use sui_rpc_api::proto::node::{GetCheckpointOptions, GetCheckpointRequest, GetCheckpointResponse};
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

    let mut grpc_client = NodeClient::connect(test_cluster.rpc_url().to_owned())
        .await
        .unwrap();

    // Request default fields
    let GetCheckpointResponse {
        sequence_number,
        digest,
        summary,
        summary_bcs,
        signature,
        contents,
        contents_bcs,
    } = grpc_client
        .get_checkpoint(GetCheckpointRequest::latest())
        .await
        .unwrap()
        .into_inner();

    assert!(sequence_number.is_some());
    assert!(digest.is_some());
    assert!(summary.is_some());
    assert!(summary_bcs.is_none());
    assert!(signature.is_some());
    assert!(contents.is_none());
    assert!(contents_bcs.is_none());

    // Request no fields
    let GetCheckpointResponse {
        sequence_number,
        digest,
        summary,
        summary_bcs,
        signature,
        contents,
        contents_bcs,
    } = grpc_client
        .get_checkpoint(GetCheckpointRequest::latest().with_options(GetCheckpointOptions::none()))
        .await
        .unwrap()
        .into_inner();

    assert!(sequence_number.is_some());
    assert!(digest.is_some());
    assert!(summary.is_none());
    assert!(summary_bcs.is_none());
    assert!(signature.is_none());
    assert!(contents.is_none());
    assert!(contents_bcs.is_none());

    // Request all fields
    let response = grpc_client
        .get_checkpoint(GetCheckpointRequest::latest().with_options(GetCheckpointOptions::all()))
        .await
        .unwrap()
        .into_inner();

    let GetCheckpointResponse {
        sequence_number,
        digest,
        summary,
        summary_bcs,
        signature,
        contents,
        contents_bcs,
    } = &response;

    assert!(sequence_number.is_some());
    assert!(digest.is_some());
    assert!(summary.is_some());
    assert!(summary_bcs.is_some());
    assert!(signature.is_some());
    assert!(contents.is_some());
    assert!(contents_bcs.is_some());

    // ensure we can convert proto GetCheckpointResponse type to rust CheckpointResponse
    sui_rpc_api::types::CheckpointResponse::try_from(&response).unwrap();

    // Request by digest
    let response = grpc_client
        .get_checkpoint(
            GetCheckpointRequest::by_digest(digest.clone().unwrap())
                .with_options(GetCheckpointOptions::none()),
        )
        .await
        .unwrap()
        .into_inner();
    assert_eq!(response.digest, digest.to_owned());

    // Request by sequence_number
    let response = grpc_client
        .get_checkpoint(
            GetCheckpointRequest::by_sequence_number(sequence_number.unwrap())
                .with_options(GetCheckpointOptions::none()),
        )
        .await
        .unwrap()
        .into_inner();
    assert_eq!(response.sequence_number, sequence_number.to_owned());
    assert_eq!(response.digest, digest.to_owned());

    // Request by digest and sequence_number results in an error
    grpc_client
        .get_checkpoint(GetCheckpointRequest {
            sequence_number: Some(sequence_number.unwrap()),
            digest: Some(digest.clone().unwrap()),
            options: None,
        })
        .await
        .unwrap_err();
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
