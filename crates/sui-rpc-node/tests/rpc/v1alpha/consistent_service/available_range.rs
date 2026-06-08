// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use sui_indexer_alt_consistent_api::proto::rpc::consistent::v1alpha::AvailableRangeRequest;
use sui_indexer_alt_consistent_api::proto::rpc::consistent::v1alpha::CHECKPOINT_HEIGHT_METADATA;
use sui_indexer_alt_consistent_api::proto::rpc::consistent::v1alpha::LOWEST_AVAILABLE_CHECKPOINT_METADATA;
use sui_indexer_alt_consistent_api::proto::rpc::consistent::v1alpha::consistent_service_client::ConsistentServiceClient;
use tonic::metadata::AsciiMetadataValue;
use tonic::metadata::MetadataMap;
use tonic::transport::Channel;

use crate::cluster::LocalCluster;

/// Read the consistent-store's range headers off response or
/// status metadata.
fn checkpoint_bounds(metadata: &MetadataMap) -> (u64, u64) {
    let min = metadata
        .get(LOWEST_AVAILABLE_CHECKPOINT_METADATA)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.parse::<u64>().ok())
        .expect("lowest-available-checkpoint header should be present");
    let max = metadata
        .get(CHECKPOINT_HEIGHT_METADATA)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.parse::<u64>().ok())
        .expect("checkpoint-height header should be present");
    (min, max)
}

#[tokio::test]
async fn available_range_reports_current_snapshot_range() {
    let cluster = LocalCluster::new().await.unwrap();
    cluster.create_checkpoint().await.unwrap();
    cluster.create_checkpoint().await.unwrap();
    cluster.create_checkpoint().await.unwrap();

    let mut client: ConsistentServiceClient<Channel> =
        ConsistentServiceClient::connect(cluster.grpc_url().to_string())
            .await
            .unwrap();

    let response = client
        .available_range(AvailableRangeRequest::default())
        .await
        .unwrap();

    let metadata = response.metadata().clone();
    let body = response.into_inner();

    assert!(body.min_checkpoint.is_some());
    let max = body.max_checkpoint.expect("max_checkpoint should be set");
    assert!(
        max >= 3,
        "after 3 user checkpoints the snapshot window should include at least checkpoint 3 (got {max})",
    );
    assert!(body.max_epoch.is_some());
    assert!(body.total_transactions.is_some());
    assert!(body.max_timestamp_ms.is_some());
    assert_eq!(body.stride, Some(1), "for_test uses stride=1");

    // Response headers should stamp the same range.
    assert!(
        metadata.contains_key(CHECKPOINT_HEIGHT_METADATA),
        "response should carry the high-water checkpoint header",
    );
    assert!(
        metadata.contains_key(LOWEST_AVAILABLE_CHECKPOINT_METADATA),
        "response should carry the low-water checkpoint header",
    );
}

/// Ports `test_checkpoint_bounds_metadata_on_non_out_of_range_status`:
/// a malformed `x-sui-checkpoint-height` header surfaces as
/// `InvalidArgument`, and the bounds headers are still stamped on
/// the error metadata so clients can rebound on the next request.
#[tokio::test]
async fn bounds_headers_present_on_invalid_argument() {
    let cluster = LocalCluster::new().await.unwrap();
    cluster.create_checkpoint().await.unwrap();
    cluster.create_checkpoint().await.unwrap();

    let mut client: ConsistentServiceClient<Channel> =
        ConsistentServiceClient::connect(cluster.grpc_url().to_string())
            .await
            .unwrap();

    let mut request = tonic::Request::new(AvailableRangeRequest::default());
    request.metadata_mut().insert(
        CHECKPOINT_HEIGHT_METADATA,
        AsciiMetadataValue::from_static("not-a-number"),
    );

    let status = client.available_range(request).await.unwrap_err();
    assert_eq!(status.code(), tonic::Code::InvalidArgument);

    let (min, max) = checkpoint_bounds(status.metadata());
    assert_eq!(min, 0, "minimum should be the first snapshot");
    assert!(
        max >= 2,
        "after 2 user checkpoints + genesis the high-water mark should reach at least 2 (got {max})",
    );
}

/// Ports `test_checkpoint_bounds_metadata_on_out_of_range_status`:
/// a valid-but-out-of-range checkpoint returns `OutOfRange` *and*
/// the bounds headers, so a paginating client can recalibrate
/// without a separate `available_range` round-trip.
#[tokio::test]
async fn bounds_headers_present_on_out_of_range() {
    let cluster = LocalCluster::new().await.unwrap();
    cluster.create_checkpoint().await.unwrap();

    let mut client: ConsistentServiceClient<Channel> =
        ConsistentServiceClient::connect(cluster.grpc_url().to_string())
            .await
            .unwrap();

    let mut request = tonic::Request::new(AvailableRangeRequest::default());
    request.metadata_mut().insert(
        CHECKPOINT_HEIGHT_METADATA,
        AsciiMetadataValue::from_static("1000"),
    );

    let status = client.available_range(request).await.unwrap_err();
    assert_eq!(status.code(), tonic::Code::OutOfRange);

    let (min, max) = checkpoint_bounds(status.metadata());
    assert_eq!(min, 0);
    assert!(
        max < 1000,
        "the rejecting bounds should be below the requested checkpoint (got max={max})",
    );
}
