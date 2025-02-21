// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use sui_macros::sim_test;
use sui_rpc_api::client::Client as CoreClient;
use sui_rpc_api::proto::node::v2::node_service_client::NodeServiceClient;
use sui_rpc_api::proto::node::v2::GetObjectOptions;
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

    assert_eq!(object_id, Some(id.into()));
    assert!(version.is_some());
    assert!(digest.is_some());
    assert!(object.is_none());
    assert!(object_bcs.is_none()); // By default object_bcs isn't returned

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
                .with_options(GetObjectOptions::none()),
        )
        .await
        .unwrap()
        .into_inner();

    assert_eq!(object_id, Some(id.into()));
    assert_eq!(version, Some(1));
    assert!(digest.is_some());

    // These fields were not requested
    assert!(object.is_none());
    assert!(object_bcs.is_none());

    let response = grpc_client
        .get_object(GetObjectRequest::new(id).with_options(GetObjectOptions::all()))
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

    // ensure we can convert proto ObjectResponse type to rust ObjectResponse
    sui_rpc_api::types::ObjectResponse::try_from(&response).unwrap();
}
