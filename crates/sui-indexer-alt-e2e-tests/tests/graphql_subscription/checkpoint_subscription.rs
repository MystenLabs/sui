// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use sui_macros::sim_test;
use test_cluster::TestClusterBuilder;
use tokio_stream::StreamExt;

use crate::testing::SubscriptionTestCluster;
use crate::testing::checkpoint_tx_digests;
use crate::testing::transfer_coins;
use crate::testing::wait_for_matching_item;

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

#[sim_test]
async fn test_subscription_transactions() {
    let mut validator_cluster = TestClusterBuilder::new()
        .with_num_validators(1)
        .build()
        .await;
    let cluster = SubscriptionTestCluster::new(&validator_cluster).await;
    let sender = validator_cluster.wallet.active_address().unwrap();

    let query = r#"subscription {
        checkpoints {
            sequenceNumber
            transactions(filter: { sentAddress: "SENDER" }) {
                nodes {
                    digest
                    sender { address }
                    gasInput { gasBudget }
                    effects {
                        status
                        balanceChanges {
                            nodes {
                                amount
                                coinType { repr }
                                owner { address }
                            }
                        }
                    }
                }
            }
        }
    }"#
    .replace("SENDER", &sender.to_string());
    let mut stream = cluster.subscribe(&query).await;
    let digests = transfer_coins(&mut validator_cluster, &[1000]).await;
    let item = wait_for_matching_item(&mut stream, &digests, checkpoint_tx_digests).await;

    insta::assert_json_snapshot!("subscription_transactions", item, {
        ".data.checkpoints.sequenceNumber" => "[seq]",
        ".**.digest" => "[digest]",
        ".**.address" => "[address]",
    });
}

#[sim_test]
async fn test_subscription_transactions_pagination_first() {
    let mut validator_cluster = TestClusterBuilder::new()
        .with_num_validators(1)
        .build()
        .await;
    let cluster = SubscriptionTestCluster::new(&validator_cluster).await;
    let sender = validator_cluster.wallet.active_address().unwrap();

    let query = r#"subscription {
        checkpoints {
            sequenceNumber
            transactions(first: 1, filter: { sentAddress: "SENDER" }) {
                nodes {
                    digest
                    effects {
                        status
                        balanceChanges {
                            nodes {
                                amount
                                coinType { repr }
                            }
                        }
                    }
                }
                edges { cursor }
                pageInfo { hasNextPage hasPreviousPage }
            }
        }
    }"#
    .replace("SENDER", &sender.to_string());
    let mut stream = cluster.subscribe(&query).await;
    // Under sim_test, soft-bundled transactions deterministically land in the
    // same checkpoint, ordered by digest.
    let digests = transfer_coins(&mut validator_cluster, &[100, 100]).await;
    let item = wait_for_matching_item(&mut stream, &digests, checkpoint_tx_digests).await;

    insta::assert_json_snapshot!("subscription_transactions_pagination_first", item, {
        ".data.checkpoints.sequenceNumber" => "[seq]",
        ".**.digest" => "[digest]",
        ".**.cursor" => "[cursor]",
    });
}

#[sim_test]
async fn test_subscription_transactions_pagination_last() {
    let mut validator_cluster = TestClusterBuilder::new()
        .with_num_validators(1)
        .build()
        .await;
    let cluster = SubscriptionTestCluster::new(&validator_cluster).await;
    let sender = validator_cluster.wallet.active_address().unwrap();

    let query = r#"subscription {
        checkpoints {
            sequenceNumber
            transactions(last: 1, filter: { sentAddress: "SENDER" }) {
                nodes {
                    digest
                    effects {
                        status
                        balanceChanges {
                            nodes {
                                amount
                                coinType { repr }
                            }
                        }
                    }
                }
                edges { cursor }
                pageInfo { hasNextPage hasPreviousPage }
            }
        }
    }"#
    .replace("SENDER", &sender.to_string());
    let mut stream = cluster.subscribe(&query).await;
    let digests = transfer_coins(&mut validator_cluster, &[100, 100]).await;
    let item = wait_for_matching_item(&mut stream, &digests, checkpoint_tx_digests).await;

    insta::assert_json_snapshot!("subscription_transactions_pagination_last", item, {
        ".data.checkpoints.sequenceNumber" => "[seq]",
        ".**.digest" => "[digest]",
        ".**.cursor" => "[cursor]",
    });
}
