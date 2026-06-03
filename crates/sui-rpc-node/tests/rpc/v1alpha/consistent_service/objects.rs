// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use sui_indexer_alt_consistent_api::proto::rpc::consistent::v1alpha::ListObjectsByTypeRequest;
use sui_indexer_alt_consistent_api::proto::rpc::consistent::v1alpha::ListOwnedObjectsRequest;
use sui_indexer_alt_consistent_api::proto::rpc::consistent::v1alpha::Owner;
use sui_indexer_alt_consistent_api::proto::rpc::consistent::v1alpha::consistent_service_client::ConsistentServiceClient;
use sui_indexer_alt_consistent_api::proto::rpc::consistent::v1alpha::owner::OwnerKind;
use tonic::transport::Channel;

use crate::cluster::LocalCluster;

const GAS_COIN_TYPE: &str = "0x0000000000000000000000000000000000000000000000000000000000000002::coin::Coin<\
     0x0000000000000000000000000000000000000000000000000000000000000002::sui::SUI>";

async fn client(cluster: &LocalCluster) -> ConsistentServiceClient<Channel> {
    ConsistentServiceClient::connect(cluster.grpc_url().to_string())
        .await
        .unwrap()
}

#[tokio::test]
async fn list_owned_objects_returns_funded_gas_coin() {
    let cluster = LocalCluster::new().await.unwrap();
    let (owner, _kp, gas) = cluster.funded_account(10_000_000_000).await.unwrap();
    cluster.create_checkpoint().await.unwrap();

    let mut svc = client(&cluster).await;

    let response = svc
        .list_owned_objects(ListOwnedObjectsRequest {
            owner: Some(Owner {
                kind: Some(OwnerKind::Address as i32),
                address: Some(owner.to_string()),
            }),
            ..Default::default()
        })
        .await
        .unwrap()
        .into_inner();

    assert_eq!(response.has_previous_page, Some(false));
    assert_eq!(response.has_next_page, Some(false));
    let gas_id = gas.0.to_string();
    assert!(
        response
            .objects
            .iter()
            .any(|o| o.object_id.as_deref() == Some(&*gas_id)),
        "funded gas coin should appear in the owner's listing",
    );
}

#[tokio::test]
async fn list_owned_objects_filters_by_type() {
    let cluster = LocalCluster::new().await.unwrap();
    let (owner, _kp, gas) = cluster.funded_account(10_000_000_000).await.unwrap();
    cluster.create_checkpoint().await.unwrap();
    let mut svc = client(&cluster).await;

    let response = svc
        .list_owned_objects(ListOwnedObjectsRequest {
            owner: Some(Owner {
                kind: Some(OwnerKind::Address as i32),
                address: Some(owner.to_string()),
            }),
            object_type: Some(GAS_COIN_TYPE.to_string()),
            ..Default::default()
        })
        .await
        .unwrap()
        .into_inner();

    let gas_id = gas.0.to_string();
    assert!(
        response
            .objects
            .iter()
            .any(|o| o.object_id.as_deref() == Some(&*gas_id)),
        "type-filtered listing should still return the gas coin",
    );
}

#[tokio::test]
async fn list_owned_objects_missing_owner_is_invalid_argument() {
    let cluster = LocalCluster::new().await.unwrap();
    cluster.create_checkpoint().await.unwrap();
    let mut svc = client(&cluster).await;

    let err = svc
        .list_owned_objects(ListOwnedObjectsRequest::default())
        .await
        .unwrap_err();
    assert_eq!(err.code(), tonic::Code::InvalidArgument);
}

#[tokio::test]
async fn list_objects_by_type_returns_funded_gas_coin() {
    let cluster = LocalCluster::new().await.unwrap();
    let (_owner, _kp, gas) = cluster.funded_account(10_000_000_000).await.unwrap();
    cluster.create_checkpoint().await.unwrap();
    let mut svc = client(&cluster).await;

    let response = svc
        .list_objects_by_type(ListObjectsByTypeRequest {
            object_type: Some(GAS_COIN_TYPE.to_string()),
            ..Default::default()
        })
        .await
        .unwrap()
        .into_inner();

    let gas_id = gas.0.to_string();
    assert!(
        response
            .objects
            .iter()
            .any(|o| o.object_id.as_deref() == Some(&*gas_id)),
        "the gas coin should appear in the type-only listing",
    );
}

#[tokio::test]
async fn list_objects_by_type_missing_type_is_invalid_argument() {
    let cluster = LocalCluster::new().await.unwrap();
    cluster.create_checkpoint().await.unwrap();
    let mut svc = client(&cluster).await;

    let err = svc
        .list_objects_by_type(ListObjectsByTypeRequest::default())
        .await
        .unwrap_err();
    assert_eq!(err.code(), tonic::Code::InvalidArgument);
}
