// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! These tests use `#[tokio::test]` rather than the workspace-standard simulator test
//! attribute because the harness needs a real Postgres `TempDb` and a real
//! `TestClusterBuilder` validator, neither of which works inside the simulator's
//! deterministic runtime.

use std::collections::HashSet;
use std::time::Duration;

use serde_json::Value;
use serde_json::json;
use tokio_stream::StreamExt;

use crate::testing::SubscriptionTestCluster;
use crate::testing::emit_event_harness;
use crate::testing::graphql_redactions;

fn event_value(item: &Value) -> Option<&str> {
    item["data"]["events"]["contents"]["json"]["value"].as_str()
}

fn event_bcs(item: &Value) -> Option<&str> {
    item["data"]["events"]["eventBcs"].as_str()
}

#[tokio::test]
async fn test_event_subscription() {
    let mut cluster = SubscriptionTestCluster::new().await;
    let package_id = emit_event_harness::publish(&mut cluster.validator).await;

    let mut stream = cluster
        .subscribe_with_variables(
            r#"subscription($pkg: SuiAddress!) {
                events(filter: { type: $pkg }) {
                    sender { address }
                    transaction { digest }
                    contents {
                        type { repr }
                        json
                    }
                    sequenceNumber
                    timestamp
                }
            }"#,
            Some(json!({ "pkg": package_id.to_string() })),
        )
        .await;

    let _digest = emit_event_harness::emit(&mut cluster.validator, package_id).await;
    let item = stream.next().await.expect("Stream ended");

    graphql_redactions().bind(|| {
        insta::assert_json_snapshot!("event_subscription", item);
    });
}

#[tokio::test]
async fn test_event_subscription_sender_filter() {
    let mut cluster = SubscriptionTestCluster::new().await;
    let sender = cluster.validator.wallet.active_address().unwrap();
    let package_id = emit_event_harness::publish(&mut cluster.validator).await;

    let mut stream = cluster
        .subscribe_with_variables(
            r#"subscription($sender: SuiAddress!) {
                events(filter: { sender: $sender }) {
                    sender { address }
                    contents { type { repr } }
                }
            }"#,
            Some(json!({ "sender": sender.to_string() })),
        )
        .await;

    let _digest = emit_event_harness::emit(&mut cluster.validator, package_id).await;
    let item = stream.next().await.expect("Stream ended");

    graphql_redactions().bind(|| {
        insta::assert_json_snapshot!("event_subscription_sender_filter", item);
    });
}

/// Verifies that `event.transaction.<field>` resolves fully in streaming mode rather
/// than returning a digest-only stub. Exercises `TransactionContents::fetch`'s streaming
/// fast path.
#[tokio::test]
async fn test_event_subscription_transaction_fields() {
    let mut cluster = SubscriptionTestCluster::new().await;
    let package_id = emit_event_harness::publish(&mut cluster.validator).await;

    let mut stream = cluster
        .subscribe_with_variables(
            r#"subscription($pkg: SuiAddress!) {
                events(filter: { type: $pkg }) {
                    transaction {
                        digest
                        sender { address }
                        kind { __typename }
                        gasInput { gasBudget gasPrice }
                    }
                    contents { type { repr } }
                }
            }"#,
            Some(json!({ "pkg": package_id.to_string() })),
        )
        .await;

    let _digest = emit_event_harness::emit(&mut cluster.validator, package_id).await;
    let item = stream.next().await.expect("Stream ended");

    graphql_redactions().bind(|| {
        insta::assert_json_snapshot!("event_subscription_transaction_fields", item);
    });
}

/// Verifies that `event.transaction.effects.objectChanges` resolves with non-empty
/// object data in streaming mode. Exercises `EffectsContents::fetch`'s streaming fast
/// path plus the per-tx execution-objects anchor that `Scope::with_tx_sequence_number_viewed_at`
/// sets up.
#[tokio::test]
async fn test_event_subscription_object_changes() {
    let mut cluster = SubscriptionTestCluster::new().await;
    let package_id = emit_event_harness::publish(&mut cluster.validator).await;

    let mut stream = cluster
        .subscribe_with_variables(
            r#"subscription($pkg: SuiAddress!) {
                events(filter: { type: $pkg }) {
                    transaction {
                        effects {
                            status
                            objectChanges {
                                nodes {
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
            }"#,
            Some(json!({ "pkg": package_id.to_string() })),
        )
        .await;

    let _digest = emit_event_harness::emit_and_create(&mut cluster.validator, package_id, 7).await;
    let item = stream.next().await.expect("Stream ended");

    graphql_redactions().bind(|| {
        insta::assert_json_snapshot!("event_subscription_object_changes", item);
    });
}

#[tokio::test]
async fn test_event_subscription_module_filter() {
    let mut cluster = SubscriptionTestCluster::new().await;
    let package_id = emit_event_harness::publish(&mut cluster.validator).await;

    let mut stream = cluster
        .subscribe_with_variables(
            r#"subscription($mod: String!) {
                events(filter: { module: $mod }) {
                    contents { type { repr } }
                }
            }"#,
            Some(json!({ "mod": format!("{}::emit_test_event", package_id) })),
        )
        .await;

    let _digest = emit_event_harness::emit(&mut cluster.validator, package_id).await;
    let item = stream.next().await.expect("Stream ended");

    graphql_redactions().bind(|| {
        insta::assert_json_snapshot!("event_subscription_module_filter", item);
    });
}

/// Forces a reconnect blackout via the proxy and asserts the subscriber receives every
/// event emitted during the gap once the connection is restored. Mirrors
/// `test_subscription_recovers_from_upstream_disconnect` but for the events subscription.
///
/// Each emit uses a distinct `value` so the resulting events have unique `eventBcs`
/// bytes, letting us verify both ordering (via the parsed `contents.json.value`) and
/// individual identity (via `eventBcs` distinctness).
#[tokio::test]
async fn test_event_subscription_recovers_from_upstream_disconnect() {
    let (mut cluster, proxy) = SubscriptionTestCluster::new_with_disruption_proxy().await;
    let package_id = emit_event_harness::publish(&mut cluster.validator).await;

    let mut stream = cluster
        .subscribe_with_variables(
            r#"subscription($pkg: SuiAddress!) {
                events(filter: { type: $pkg }) {
                    eventBcs
                    contents { json }
                }
            }"#,
            Some(json!({ "pkg": package_id.to_string() })),
        )
        .await;

    // Healthy: emit value=1, verify it streams live.
    emit_event_harness::emit_with_value(&mut cluster.validator, package_id, 1).await;
    let live = stream.next().await.expect("Stream ended");
    assert_eq!(event_value(&live), Some("1"));

    // Blackout: drop the upstream gRPC connection.
    proxy.block_connections();
    proxy.disconnect_all();

    // Let the disconnect take effect on the streaming server before asserting silence.
    tokio::time::sleep(Duration::from_secs(2)).await;

    // No events should arrive while the upstream is blacked out.
    let silence = tokio::time::timeout(Duration::from_secs(1), stream.next()).await;
    assert!(
        silence.is_err(),
        "stream yielded an event during blackout: {silence:?}",
    );

    // Emit eight events during the blackout (values 2..10). The validator advances and
    // produces checkpoints that the streaming server can't see live.
    for v in 2..10u64 {
        emit_event_harness::emit_with_value(&mut cluster.validator, package_id, v).await;
    }

    // Resume: gap recovery via kv-rpc fills in the missing checkpoints in order.
    proxy.allow_connections();

    let received: Vec<Value> = (&mut stream).take(8).collect().await;
    let values: Vec<&str> = received.iter().filter_map(event_value).collect();
    let bcs_set: HashSet<&str> = received.iter().filter_map(event_bcs).collect();

    assert_eq!(
        values,
        vec!["2", "3", "4", "5", "6", "7", "8", "9"],
        "events out of order",
    );
    assert_eq!(bcs_set.len(), 8, "eventBcs should be distinct across emits");
}
