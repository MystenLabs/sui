// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use sui_indexer_alt_consistent_api::proto::rpc::consistent::v1alpha::AvailableRangeRequest;
use sui_indexer_alt_consistent_api::proto::rpc::consistent::v1alpha::CHECKPOINT_HEIGHT_METADATA;
use sui_indexer_alt_consistent_api::proto::rpc::consistent::v1alpha::LOWEST_AVAILABLE_CHECKPOINT_METADATA;
use sui_indexer_alt_consistent_api::proto::rpc::consistent::v1alpha::consistent_service_client::ConsistentServiceClient;
use sui_indexer_alt_e2e_tests::FullCluster;
use tonic::metadata::AsciiMetadataValue;

#[tokio::test]
async fn test_checkpoint_bounds_metadata_on_success_response() {
    let mut cluster = FullCluster::new().await.unwrap();
    cluster.create_checkpoint().await;
    cluster.create_checkpoint().await;
    cluster.create_checkpoint().await;

    let response = ConsistentServiceClient::connect(cluster.consistent_store_url().to_string())
        .await
        .unwrap()
        .available_range(tonic::Request::new(AvailableRangeRequest {}))
        .await
        .unwrap();

    assert_eq!(checkpoint_bounds(response.metadata()), (0, 3));
}

#[tokio::test]
async fn test_checkpoint_bounds_metadata_on_non_out_of_range_status() {
    let mut cluster = FullCluster::new().await.unwrap();
    cluster.create_checkpoint().await;
    cluster.create_checkpoint().await;

    let mut request = tonic::Request::new(AvailableRangeRequest {});
    request.metadata_mut().insert(
        CHECKPOINT_HEIGHT_METADATA,
        AsciiMetadataValue::from_static("not-a-number"),
    );

    let status = ConsistentServiceClient::connect(cluster.consistent_store_url().to_string())
        .await
        .unwrap()
        .available_range(request)
        .await
        .unwrap_err();

    assert_eq!(status.code(), tonic::Code::InvalidArgument);
    assert_eq!(checkpoint_bounds(status.metadata()), (0, 2));
}

#[tokio::test]
async fn test_checkpoint_bounds_metadata_on_out_of_range_status() {
    let mut cluster = FullCluster::new().await.unwrap();
    cluster.create_checkpoint().await;

    let mut request = tonic::Request::new(AvailableRangeRequest {});
    request.metadata_mut().insert(
        CHECKPOINT_HEIGHT_METADATA,
        AsciiMetadataValue::from_static("1000"),
    );

    let status = ConsistentServiceClient::connect(cluster.consistent_store_url().to_string())
        .await
        .unwrap()
        .available_range(request)
        .await
        .unwrap_err();

    assert_eq!(status.code(), tonic::Code::OutOfRange);
    assert_eq!(checkpoint_bounds(status.metadata()), (0, 1));
}

fn checkpoint_bounds(metadata: &tonic::metadata::MetadataMap) -> (u64, u64) {
    let min = metadata
        .get(LOWEST_AVAILABLE_CHECKPOINT_METADATA)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap();

    let max = metadata
        .get(CHECKPOINT_HEIGHT_METADATA)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap();

    (min, max)
}
