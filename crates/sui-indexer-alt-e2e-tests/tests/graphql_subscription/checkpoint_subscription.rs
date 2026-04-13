// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use sui_macros::sim_test;
use test_cluster::TestClusterBuilder;
use tokio_stream::StreamExt;

use crate::testing::SubscriptionTestCluster;

#[sim_test]
async fn test_subscription_sequential() {
    let validator_cluster = TestClusterBuilder::new()
        .with_num_validators(1)
        .build()
        .await;
    let cluster = SubscriptionTestCluster::new(&validator_cluster).await;

    let items: Vec<_> = cluster
        .subscribe("subscription { checkpoints { sequenceNumber } }")
        .await
        .take(3)
        .collect()
        .await;

    insta::assert_json_snapshot!("subscription_sequential", items);
}

#[sim_test]
async fn test_subscription_fields() {
    let validator_cluster = TestClusterBuilder::new()
        .with_num_validators(1)
        .build()
        .await;
    let cluster = SubscriptionTestCluster::new(&validator_cluster).await;

    let item = cluster
        .subscribe(
            r#"subscription {
                checkpoints {
                    sequenceNumber
                    digest
                    contentDigest
                    timestamp
                    networkTotalTransactions
                    rollingGasSummary {
                        computationCost
                        storageCost
                        storageRebate
                        nonRefundableStorageFee
                    }
                    epoch {
                        epochId
                    }
                    validatorSignatures {
                        signature
                        signersMap
                    }
                }
            }"#,
        )
        .await
        .next()
        .await
        .unwrap();

    insta::assert_json_snapshot!("subscription_fields", item, {
        ".data.checkpoints.digest" => "[digest]",
        ".data.checkpoints.contentDigest" => "[contentDigest]",
        ".data.checkpoints.timestamp" => "[timestamp]",
        ".data.checkpoints.networkTotalTransactions" => "[networkTotalTransactions]",
        ".data.checkpoints.validatorSignatures.signature" => "[signature]",
    });
}
