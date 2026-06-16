// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Mirrors `sui-e2e-tests/tests/rpc/v2/subscription_service.rs`, but
//! against the standalone rpc-node: the `checkpoint_broadcast`
//! pipeline feeds the subscription service over the checkpoints the
//! node ingests from Simulacrum.

use std::time::Duration;

use sui_rpc::field::FieldMask;
use sui_rpc::field::FieldMaskUtil;
use sui_rpc::proto::sui::rpc::v2::SubscribeCheckpointsRequest;
use sui_rpc::proto::sui::rpc::v2::subscription_service_client::SubscriptionServiceClient;

use crate::cluster::LocalCluster;

/// Subscribing yields the checkpoints the node ingests afterwards, in
/// order, gap-free.
#[tokio::test]
async fn subscribe_checkpoints() {
    let cluster = LocalCluster::new().await.unwrap();

    // Subscribe before creating any checkpoints: a broadcast carries
    // only future checkpoints, which is the intended follow-the-tip
    // behavior.
    let mut client = SubscriptionServiceClient::connect(cluster.grpc_url().to_string())
        .await
        .unwrap();
    let mut request = SubscribeCheckpointsRequest::default();
    request.read_mask = Some(FieldMask::from_str("sequence_number"));
    let mut stream = client
        .subscribe_checkpoints(request)
        .await
        .unwrap()
        .into_inner();

    // Drive a few checkpoints through Simulacrum -> ingestion ->
    // the broadcast pipeline.
    let mut expected = Vec::new();
    for _ in 0..3 {
        let checkpoint = cluster.create_checkpoint().await.unwrap();
        expected.push(checkpoint.sequence_number);
    }

    let mut received = Vec::new();
    while received.len() < expected.len() {
        let item = tokio::time::timeout(Duration::from_secs(10), stream.message())
            .await
            .expect("timed out waiting for a checkpoint on the subscription stream")
            .expect("subscription stream returned an error")
            .expect("subscription stream ended unexpectedly");
        let cursor = item.cursor.unwrap();
        // The response cursor matches the delivered checkpoint's number.
        assert_eq!(cursor, item.checkpoint.unwrap().sequence_number.unwrap());
        received.push(cursor);
    }

    // Delivered in order, gap-free, matching exactly the checkpoints
    // created after subscribing.
    assert_eq!(received, expected);
}
