// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use sui_macros::sim_test;
use sui_rpc_api::proto::rpc::v2beta::ledger_service_client::LedgerServiceClient;
use sui_rpc_api::proto::rpc::v2beta::GetServiceInfoRequest;
use sui_rpc_api::proto::rpc::v2beta::GetServiceInfoResponse;
use test_cluster::TestClusterBuilder;

#[sim_test]
async fn get_service_info() {
    let test_cluster = TestClusterBuilder::new().build().await;

    let mut grpc_client = LedgerServiceClient::connect(test_cluster.rpc_url().to_owned())
        .await
        .unwrap();

    let GetServiceInfoResponse {
        chain_id,
        chain,
        epoch,
        checkpoint_height,
        timestamp,
        lowest_available_checkpoint,
        lowest_available_checkpoint_objects,
        server_version,
    } = grpc_client
        .get_service_info(GetServiceInfoRequest {})
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
    assert!(server_version.is_some());
}
