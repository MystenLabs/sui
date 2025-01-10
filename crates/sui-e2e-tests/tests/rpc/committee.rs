// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use sui_macros::sim_test;
use sui_rpc_api::client::sdk::Client;
use sui_rpc_api::proto::node::node_service_client::NodeServiceClient;
use sui_rpc_api::proto::node::GetCommitteeRequest;
use test_cluster::TestClusterBuilder;

#[sim_test]
async fn get_committee() {
    let test_cluster = TestClusterBuilder::new().build().await;

    let client = Client::new(test_cluster.rpc_url()).unwrap();
    let mut grpc_client = NodeServiceClient::connect(test_cluster.rpc_url().to_owned())
        .await
        .unwrap();

    let _committee = client.get_committee(0).await.unwrap();
    let _committee = client.get_current_committee().await.unwrap();

    let latest_committee = grpc_client
        .get_committee(GetCommitteeRequest { epoch: None })
        .await
        .unwrap()
        .into_inner()
        .committee
        .unwrap();

    let epoch_0_committee = grpc_client
        .get_committee(GetCommitteeRequest { epoch: Some(0) })
        .await
        .unwrap()
        .into_inner()
        .committee
        .unwrap();

    assert_eq!(latest_committee, epoch_0_committee);

    // ensure we can convert proto committee type to sdk_types committee
    sui_sdk_types::ValidatorCommittee::try_from(&latest_committee).unwrap();
}
