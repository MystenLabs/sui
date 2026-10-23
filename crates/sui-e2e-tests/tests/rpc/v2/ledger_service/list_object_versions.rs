// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use sui_macros::sim_test;
use sui_rpc::proto::sui::rpc::v2::ListObjectVersionsRequest;
use sui_rpc::proto::sui::rpc::v2::ledger_service_client::LedgerServiceClient;
use test_cluster::TestClusterBuilder;

/// Fullnodes prune superseded object versions, so they leave `ListObjectVersions` to archival
/// kv-rpc and answer `Unimplemented`, even for an object they hold.
#[sim_test]
async fn list_object_versions_is_unimplemented() {
    let cluster = TestClusterBuilder::new()
        .with_num_validators(1)
        .build()
        .await;
    let object_id = cluster
        .wallet
        .get_one_gas_object_owned_by_address(cluster.get_address_0())
        .await
        .unwrap()
        .unwrap()
        .0;

    let mut client = LedgerServiceClient::connect(cluster.rpc_url().to_owned())
        .await
        .unwrap();
    let mut request = ListObjectVersionsRequest::default();
    request.object_id = Some(object_id.to_canonical_string(true));
    let status = client.list_object_versions(request).await.unwrap_err();
    assert_eq!(status.code(), tonic::Code::Unimplemented, "{status:?}");
    assert!(status.message().contains("kv-rpc"), "{status:?}");
}
