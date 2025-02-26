// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use sui_macros::sim_test;
use sui_rpc_api::client::Client as CoreClient;
use sui_rpc_api::field_mask::FieldMask;
use sui_rpc_api::field_mask::FieldMaskUtil;
use sui_rpc_api::proto::node::v2::node_service_client::NodeServiceClient;
use sui_rpc_api::proto::node::v2::GetObjectRequest;
use sui_rpc_api::proto::node::v2::GetObjectResponse;
use sui_sdk_types::ObjectId;
use test_cluster::TestClusterBuilder;

#[sim_test]
async fn get_object() {
    let test_cluster = TestClusterBuilder::new().build().await;

    let id: ObjectId = "0x5".parse().unwrap();

    let core_client = CoreClient::new(test_cluster.rpc_url()).unwrap();
    let mut grpc_client = NodeServiceClient::connect(test_cluster.rpc_url().to_owned())
        .await
        .unwrap();

    let _object = core_client.get_object(id.into()).await.unwrap();

    let _object = core_client
        .get_object_with_version(id.into(), 1.into())
        .await
        .unwrap();

    // Request with no provided read_mask
    let GetObjectResponse {
        object_id,
        version,
        digest,
        object,
        object_bcs,
    } = grpc_client
        .get_object(GetObjectRequest::new(id))
        .await
        .unwrap()
        .into_inner();

    // These fields default to being read
    assert_eq!(object_id, Some(id.into()));
    assert!(version.is_some());
    assert!(digest.is_some());

    // while these fields default to not being read
    assert!(object.is_none());
    assert!(object_bcs.is_none());

    // Request with provided read_mask
    let GetObjectResponse {
        object_id,
        version,
        digest,
        object,
        object_bcs,
    } = grpc_client
        .get_object(
            GetObjectRequest::new(id)
                .with_version(1)
                .with_read_mask(FieldMask::from_paths(["object_id", "version"])),
        )
        .await
        .unwrap()
        .into_inner();

    assert_eq!(object_id, Some(id.into()));
    assert_eq!(version, Some(1));

    // These fields were not requested
    assert!(digest.is_none());
    assert!(object.is_none());
    assert!(object_bcs.is_none());

    let response = grpc_client
        .get_object(
            GetObjectRequest::new(id).with_read_mask(FieldMask::from_paths([
                "object_id",
                "version",
                "digest",
                "object",
                "object_bcs",
            ])),
        )
        .await
        .unwrap()
        .into_inner();

    let GetObjectResponse {
        object_id,
        version,
        digest,
        object,
        object_bcs,
    } = &response;

    assert!(object_id.is_some());
    assert!(version.is_some());
    assert!(digest.is_some());
    assert!(object.is_some());
    assert!(object_bcs.is_some());
}
