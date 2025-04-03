// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::transfer_coin;
use sui_macros::sim_test;
use sui_rpc_api::field_mask::FieldMask;
use sui_rpc_api::field_mask::FieldMaskUtil;
use sui_rpc_api::proto::rpc::v2alpha::subscription_service_client::SubscriptionServiceClient;
use sui_rpc_api::proto::rpc::v2alpha::SubscribeCheckpointsRequest;
use test_cluster::TestClusterBuilder;
use tokio_stream::StreamExt;

#[sim_test]
async fn subscribe_checkpoint() {
    let test_cluster = TestClusterBuilder::new().build().await;

    let _transaction_digest = transfer_coin(&test_cluster.wallet).await;

    let mut client = SubscriptionServiceClient::connect(test_cluster.rpc_url().to_owned())
        .await
        .unwrap();

    let request = SubscribeCheckpointsRequest {
        read_mask: Some(FieldMask::from_str("sequence_number")),
    };

    let mut stream = client
        .subscribe_checkpoints(request)
        .await
        .unwrap()
        .into_inner();

    let mut count = 0;
    let mut last = None;
    while let Some(item) = stream.next().await {
        let checkpoint = item.unwrap();
        let cursor = checkpoint.cursor.unwrap();
        assert_eq!(
            cursor,
            checkpoint.checkpoint.unwrap().sequence_number.unwrap()
        );
        println!("checkpoint: {cursor}");

        if let Some(last) = last {
            assert_eq!(last, cursor - 1);
        }
        last = Some(cursor);

        // Subscribe for 50 checkponts to ensure the subscription system works
        count += 1;
        if count > 50 {
            break;
        }
    }

    assert!(count >= 50);
}
