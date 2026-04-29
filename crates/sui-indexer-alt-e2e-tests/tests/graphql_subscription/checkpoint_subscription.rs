// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! These tests use `#[tokio::test]` rather than the workspace-standard simulator test
//! attribute because the harness needs a real Postgres `TempDb` and a real
//! `TestClusterBuilder` validator, neither of which works inside the simulator's
//! deterministic runtime.

use serde_json::json;
use tokio_stream::StreamExt;

use crate::testing::SubscriptionTestCluster;
use crate::testing::checkpoint_seq;
use crate::testing::checkpoint_tx_digests;
use crate::testing::drain_checkpoints_until_stalled;
use crate::testing::graphql_redactions;
use crate::testing::object_wrapping_harness;
use crate::testing::transfer_coins;
use crate::testing::wait_for_matching_item;

#[tokio::test]
async fn test_subscription_sequential() {
    let cluster = SubscriptionTestCluster::new().await;

    let items: Vec<_> = cluster
        .subscribe("subscription { checkpoints { sequenceNumber } }")
        .await
        .take(3)
        .collect()
        .await;

    let seqs: Vec<u64> = items.iter().map(checkpoint_seq).collect();
    assert!(seqs.windows(2).all(|w| w[1] == w[0] + 1), "{seqs:?}");
}

#[tokio::test]
async fn test_subscription_fields() {
    let cluster = SubscriptionTestCluster::new().await;

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

    graphql_redactions().bind(|| {
        insta::assert_json_snapshot!("subscription_fields", item);
    });
}

#[tokio::test]
async fn test_subscription_transactions() {
    let mut cluster = SubscriptionTestCluster::new().await;
    let sender = cluster.validator.wallet.active_address().unwrap();

    let mut stream = cluster
        .subscribe_with_variables(
            r#"subscription($sender: SuiAddress!) {
                checkpoints {
                    sequenceNumber
                    transactions(filter: { sentAddress: $sender }) {
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
            }"#,
            Some(json!({ "sender": sender.to_string() })),
        )
        .await;
    // Prime the stream so it's actively subscribed before mutations happen.
    let digests = transfer_coins(&mut cluster.validator, &[1000]).await;
    let item = wait_for_matching_item(&mut stream, &digests, checkpoint_tx_digests).await;

    graphql_redactions().bind(|| {
        insta::assert_json_snapshot!("subscription_transactions", item);
    });
}

#[tokio::test]
async fn test_subscription_transactions_pagination_first() {
    let mut cluster = SubscriptionTestCluster::new().await;
    let sender = cluster.validator.wallet.active_address().unwrap();

    let mut stream = cluster
        .subscribe_with_variables(
            r#"subscription($sender: SuiAddress!) {
                checkpoints {
                    sequenceNumber
                    transactions(first: 1, filter: { sentAddress: $sender }) {
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
            }"#,
            Some(json!({ "sender": sender.to_string() })),
        )
        .await;
    let digests = transfer_coins(&mut cluster.validator, &[100, 100]).await;
    let item = wait_for_matching_item(&mut stream, &digests, checkpoint_tx_digests).await;

    graphql_redactions().bind(|| {
        insta::assert_json_snapshot!("subscription_transactions_pagination_first", item);
    });
}

#[tokio::test]
async fn test_subscription_transactions_pagination_last() {
    let mut cluster = SubscriptionTestCluster::new().await;
    let sender = cluster.validator.wallet.active_address().unwrap();

    let mut stream = cluster
        .subscribe_with_variables(
            r#"subscription($sender: SuiAddress!) {
                checkpoints {
                    sequenceNumber
                    transactions(last: 1, filter: { sentAddress: $sender }) {
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
            }"#,
            Some(json!({ "sender": sender.to_string() })),
        )
        .await;
    let digests = transfer_coins(&mut cluster.validator, &[100, 100]).await;
    let item = wait_for_matching_item(&mut stream, &digests, checkpoint_tx_digests).await;

    graphql_redactions().bind(|| {
        insta::assert_json_snapshot!("subscription_transactions_pagination_last", item);
    });
}

// --- Object resolution tests ---

#[tokio::test]
async fn test_subscription_object_create() {
    let mut cluster = SubscriptionTestCluster::new().await;
    let sender = cluster.validator.wallet.active_address().unwrap();
    let package_id = object_wrapping_harness::publish(&mut cluster.validator).await;

    let mut stream = cluster
        .subscribe_with_variables(
            r#"subscription($sender: SuiAddress!) {
                checkpoints {
                    sequenceNumber
                    transactions(filter: { sentAddress: $sender }) {
                        nodes {
                            digest
                            effects {
                                objectChanges {
                                    nodes {
                                        inputState {
                                            address
                                            version
                                            digest
                                            asMoveObject {
                                                contents { type { repr } }
                                            }
                                        }
                                        outputState {
                                            address
                                            version
                                            digest
                                            asMoveObject {
                                                contents { type { repr } }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }"#,
            Some(json!({ "sender": sender.to_string() })),
        )
        .await;

    let (digest, _) =
        object_wrapping_harness::create_item(&mut cluster.validator, package_id, 42).await;
    let item = wait_for_matching_item(&mut stream, &[digest], checkpoint_tx_digests).await;

    graphql_redactions().bind(|| {
        insta::assert_json_snapshot!("subscription_object_create", item);
    });
}

#[tokio::test]
async fn test_subscription_object_lifecycle() {
    let mut cluster = SubscriptionTestCluster::new().await;
    let sender = cluster.validator.wallet.active_address().unwrap();
    let package_id = object_wrapping_harness::publish(&mut cluster.validator).await;

    let mut stream = cluster
        .subscribe_with_variables(
            r#"subscription($sender: SuiAddress!) {
                checkpoints {
                    sequenceNumber
                    transactions(filter: { sentAddress: $sender }) {
                        nodes {
                            digest
                            effects {
                                objectChanges {
                                    nodes {
                                        inputState {
                                            address
                                            version
                                            digest
                                            asMoveObject {
                                                contents { type { repr } }
                                            }
                                        }
                                        outputState {
                                            address
                                            version
                                            digest
                                            asMoveObject {
                                                contents { type { repr } }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }"#,
            Some(json!({ "sender": sender.to_string() })),
        )
        .await;

    let (d1, item) =
        object_wrapping_harness::create_item(&mut cluster.validator, package_id, 42).await;
    let cp1 = wait_for_matching_item(&mut stream, &[d1], checkpoint_tx_digests).await;

    let (d2, item) =
        object_wrapping_harness::update_item(&mut cluster.validator, package_id, item, 100).await;
    let cp2 = wait_for_matching_item(&mut stream, &[d2], checkpoint_tx_digests).await;

    let (d3, wrapper) =
        object_wrapping_harness::wrap_item(&mut cluster.validator, package_id, item).await;
    let cp3 = wait_for_matching_item(&mut stream, &[d3], checkpoint_tx_digests).await;

    let (d4, _) =
        object_wrapping_harness::unwrap_wrapper(&mut cluster.validator, package_id, wrapper).await;
    let cp4 = wait_for_matching_item(&mut stream, &[d4], checkpoint_tx_digests).await;

    graphql_redactions().bind(|| {
        insta::assert_json_snapshot!("subscription_object_lifecycle", [cp1, cp2, cp3, cp4]);
    });
}

/// Tests that `contents.json` resolves for streamed objects using the indexer-backed
/// package store for type layout resolution.
#[tokio::test]
async fn test_subscription_object_json() {
    let mut cluster = SubscriptionTestCluster::new().await;
    let sender = cluster.validator.wallet.active_address().unwrap();
    let package_id = object_wrapping_harness::publish(&mut cluster.validator).await;

    let mut stream = cluster
        .subscribe_with_variables(
            r#"subscription($sender: SuiAddress!) {
                checkpoints {
                    transactions(filter: { sentAddress: $sender }) {
                        nodes {
                            digest
                            effects {
                                objectChanges {
                                    nodes {
                                        inputState {
                                            asMoveObject {
                                                contents {
                                                    type { repr }
                                                    json
                                                }
                                            }
                                        }
                                        outputState {
                                            asMoveObject {
                                                contents {
                                                    type { repr }
                                                    json
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }"#,
            Some(json!({ "sender": sender.to_string() })),
        )
        .await;

    let (d1, item) =
        object_wrapping_harness::create_item(&mut cluster.validator, package_id, 42).await;
    let cp1 = wait_for_matching_item(&mut stream, &[d1], checkpoint_tx_digests).await;

    let (d2, item) =
        object_wrapping_harness::update_item(&mut cluster.validator, package_id, item, 100).await;
    let cp2 = wait_for_matching_item(&mut stream, &[d2], checkpoint_tx_digests).await;

    let (d3, wrapper) =
        object_wrapping_harness::wrap_item(&mut cluster.validator, package_id, item).await;
    let cp3 = wait_for_matching_item(&mut stream, &[d3], checkpoint_tx_digests).await;

    let (d4, _) =
        object_wrapping_harness::unwrap_wrapper(&mut cluster.validator, package_id, wrapper).await;
    let cp4 = wait_for_matching_item(&mut stream, &[d4], checkpoint_tx_digests).await;

    let mut settings = graphql_redactions();
    settings.add_redaction(".**.json.id", "[id]");
    settings.add_redaction(".**.json.item.id", "[id]");
    settings.bind(|| {
        insta::assert_json_snapshot!("subscription_object_json", [cp1, cp2, cp3, cp4]);
    });
}

// --- Gap recovery tests ---

/// Forces a reconnect blackout via the proxy and asserts the subscriber sees a
/// contiguous sequence of checkpoints across it (gap recovery filled the
/// missing range from kv-rpc). `ledger_grpc_url` bypasses the proxy, so
/// recovery reads aren't blocked.
#[tokio::test]
async fn test_subscription_recovers_from_upstream_disconnect() {
    let (cluster, proxy) = SubscriptionTestCluster::new_with_disruption_proxy().await;

    let mut stream = cluster
        .subscribe("subscription { checkpoints { sequenceNumber } }")
        .await;

    // Healthy streaming.
    let first = checkpoint_seq(&stream.next().await.unwrap());
    let second = checkpoint_seq(&stream.next().await.unwrap());
    assert_eq!(second, first + 1);

    // Blackout: graphql can't reconnect.
    proxy.block_connections();
    proxy.disconnect_all();

    // Let the validator advance so a real gap forms, then drain whatever
    // graphql had pushed before the disconnect took effect.
    tokio::time::sleep(std::time::Duration::from_secs(2)).await;
    let last_seen = drain_checkpoints_until_stalled(&mut stream, second).await;
    let validator_tip = cluster.validator_checkpoint_tip();
    assert!(
        validator_tip > last_seen + 1,
        "validator did not advance during blackout: tip={validator_tip}, last_seen={last_seen}",
    );

    // Resume.
    proxy.allow_connections();

    // First item signals reconnect + recovery have started. The validator's
    // tip at this moment bounds the reconnect tip from above, so reading
    // past it crosses from recovery into post-recovery live items.
    let mut received = vec![checkpoint_seq(&stream.next().await.unwrap())];
    let resume_tip = cluster.validator_checkpoint_tip();
    while *received.last().unwrap() < resume_tip + 20 {
        received.push(checkpoint_seq(&stream.next().await.unwrap()));
    }

    assert_eq!(
        received[0],
        last_seen + 1,
        "non-contiguous resume: last_seen={last_seen}, first received={}",
        received[0],
    );
    assert!(
        received.windows(2).all(|w| w[1] == w[0] + 1),
        "post-resume not contiguous: {received:?}",
    );
}
