// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use sui_indexer_alt_consistent_api::proto::rpc::consistent::v1alpha::AvailableRangeRequest;
use sui_indexer_alt_consistent_api::proto::rpc::consistent::v1alpha::CHECKPOINT_HEIGHT_METADATA;
use sui_indexer_alt_consistent_api::proto::rpc::consistent::v1alpha::LOWEST_AVAILABLE_CHECKPOINT_METADATA;
use sui_indexer_alt_consistent_api::proto::rpc::consistent::v1alpha::consistent_service_client::ConsistentServiceClient;
use tonic::transport::Channel;

use crate::cluster::LocalCluster;

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
