// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! These tests use `#[tokio::test]` rather than the workspace-standard simulator test
//! attribute because the harness needs a real Postgres `TempDb` and a real
//! `TestClusterBuilder` validator, neither of which works inside the simulator's
//! deterministic runtime.

use serde_json::json;

use super::testing::SubscriptionTestCluster;
use super::testing::graphql_redactions;
use super::testing::object_wrapping_harness;
use super::testing::transaction_digest;
use super::testing::transfer_coins;
use super::testing::wait_for_matching_item;

#[tokio::test]
async fn test_transaction_subscription() {
    let mut cluster = SubscriptionTestCluster::new().await;
    let sender = cluster.validator.wallet.active_address().unwrap();

    let mut stream = cluster
        .subscribe_with_variables(
            r#"subscription($sender: SuiAddress!) {
                transactions(filter: { sentAddress: $sender }) {
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
            }"#,
            Some(json!({ "sender": sender.to_string() })),
        )
        .await;

    let digests = transfer_coins(&mut cluster.validator, &[1000]).await;
    let item = wait_for_matching_item(&mut stream, &digests, transaction_digest).await;

    graphql_redactions().bind(|| {
        insta::assert_json_snapshot!("transaction_subscription", item);
    });
}

#[tokio::test]
async fn test_transaction_subscription_object_changes() {
    let mut cluster = SubscriptionTestCluster::new().await;
    let sender = cluster.validator.wallet.active_address().unwrap();
    let package_id = object_wrapping_harness::publish(&mut cluster.validator).await;

    let mut stream = cluster
        .subscribe_with_variables(
            r#"subscription($sender: SuiAddress!) {
                transactions(filter: { sentAddress: $sender }) {
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
            }"#,
            Some(json!({ "sender": sender.to_string() })),
        )
        .await;

    let (digest, _) =
        object_wrapping_harness::create_item(&mut cluster.validator, package_id, 42).await;
    let item = wait_for_matching_item(&mut stream, &[digest], transaction_digest).await;

    graphql_redactions().bind(|| {
        insta::assert_json_snapshot!("transaction_subscription_object_changes", item);
    });
}
