// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use sui_macros::sim_test;
use sui_rpc_api::proto::node::v2::node_service_client::NodeServiceClient;
use sui_rpc_api::proto::node::v2::GetNodeInfoRequest;
use sui_rpc_api::proto::node::v2::GetNodeInfoResponse;
use test_cluster::TestClusterBuilder;

#[sim_test]
async fn get_node_info() {
    let test_cluster = TestClusterBuilder::new().build().await;

    let mut grpc_client = NodeServiceClient::connect(test_cluster.rpc_url().to_owned())
        .await
        .unwrap();

    let GetNodeInfoResponse {
        chain_id,
        chain,
        epoch,
        checkpoint_height,
        timestamp,
        lowest_available_checkpoint,
        lowest_available_checkpoint_objects,
        software_version,
    } = grpc_client
        .get_node_info(GetNodeInfoRequest {})
        .await
        .unwrap()
        .into_inner();

    assert!(chain_id.is_some());
    assert!(chain.is_some());
    assert!(epoch.is_some());
    assert!(checkpoint_height.is_some());
    assert!(timestamp.is_some());
    assert!(lowest_available_checkpoint.is_some());
    assert!(lowest_available_checkpoint_objects.is_some());
    assert!(software_version.is_some());
}
