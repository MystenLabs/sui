// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! These tests use `#[tokio::test]` rather than the workspace-standard simulator test
//! attribute because the harness needs a real Postgres `TempDb` and a real
//! `TestClusterBuilder` validator, neither of which works inside the simulator's
//! deterministic runtime.

use std::collections::BTreeMap;
use std::collections::HashSet;
use std::time::Duration;

use serde_json::Value;
use serde_json::json;
use sui_types::base_types::ObjectID;
use tokio_stream::StreamExt;

use crate::testing::SubscriptionTestCluster;
use crate::testing::emit_event_harness;
use crate::testing::graphql_redactions;
use crate::testing::sort_object_changes;
use crate::testing::wait_for_matching_item;

/// How long to let the validator advance so a resumed subscription's live receiver pins past the
/// pre-subscription events, forcing them through the backfill scan.
const BACKFILL_SETTLE: Duration = Duration::from_secs(5);

fn event_value(item: &Value) -> Option<&str> {
    item["data"]["events"]["node"]["contents"]["json"]["value"].as_str()
}

fn event_bcs(item: &Value) -> Option<&str> {
    item["data"]["events"]["node"]["eventBcs"].as_str()
}

/// The emitted `value` as the single-element list `wait_for_matching_item` expects.
fn event_values(item: &Value) -> Vec<&str> {
    event_value(item).into_iter().collect()
}

/// The `after` cursor string from a delivered event edge.
fn event_cursor(item: &Value) -> String {
    item["data"]["events"]["cursor"]
        .as_str()
        .expect("edge missing cursor")
        .to_string()
}

/// The `{ "filter": <filter> }` variables wrapping an `EventFilter` input for a subscription query.
fn filter_var(filter: Value) -> Option<Value> {
    Some(json!({ "filter": filter }))
}

/// The `$filter` variables for a `type`-filtered event subscription, folding `afterCheckpoint` into
/// the filter when resuming from a checkpoint (backfill).
fn type_filter_var(package_id: &ObjectID, after_checkpoint: Option<u64>) -> Option<Value> {
    let mut filter = json!({ "type": package_id.to_string() });
    if let Some(c) = after_checkpoint {
        filter["afterCheckpoint"] = json!(c);
    }
    filter_var(filter)
}

/// The rich event-subscription query. The filter (and any `afterCheckpoint` resume point) is passed
/// through the `$filter` variable. The node is deliberately broad so the live/backfill parity test
/// catches any field that resolves differently between the two paths.
fn event_query() -> String {
    r#"subscription($filter: EventFilter!) {
        events(filter: $filter) {
            cursor
            node {
                eventBcs
                sequenceNumber
                sender { address }
                contents { type { repr } json }
                transaction {
                    digest
                    sender { address }
                    kind { __typename }
                    gasInput { gasBudget gasPrice }
                }
            }
        }
    }"#
    .to_string()
}

/// Take the next `n` event payloads in arrival order.
async fn take_events(
    stream: &mut (impl tokio_stream::Stream<Item = Value> + Unpin),
    n: usize,
) -> Vec<Value> {
    stream.take(n).collect().await
}

/// Collect the `node` of each expected event (keyed by emitted `value`) from a subscription,
/// returned in `expected` order so the comparison is stable.
async fn collect_event_nodes(
    stream: &mut (impl tokio_stream::Stream<Item = Value> + Unpin),
    expected: &[String],
) -> Vec<Value> {
    let mut by_value: BTreeMap<String, Value> = BTreeMap::new();
    while by_value.len() < expected.len() {
        let item = stream
            .next()
            .await
            .expect("stream ended before all expected events");
        let node = item["data"]["events"]["node"].clone();
        let value = node["contents"]["json"]["value"]
            .as_str()
            .expect("event missing value")
            .to_string();
        if expected.contains(&value) {
            by_value.insert(value, node);
        }
    }
    expected.iter().map(|v| by_value[v].clone()).collect()
}

#[tokio::test]
async fn test_event_subscription() {
    let mut cluster = SubscriptionTestCluster::new().await;
    let package_id = emit_event_harness::publish(&mut cluster.validator).await;

    let mut stream = cluster
        .subscribe_with_variables(
            r#"subscription($filter: EventFilter!) {
                events(filter: $filter) {
                    node {
                        sender { address }
                        transaction { digest }
                        contents {
                            type { repr }
                            json
                        }
                        sequenceNumber
                        timestamp
                    }
                }
            }"#,
            filter_var(json!({ "type": package_id.to_string() })),
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
            r#"subscription($filter: EventFilter!) {
                events(filter: $filter) {
                    node {
                        sender { address }
                        contents { type { repr } }
                    }
                }
            }"#,
            filter_var(json!({ "sender": sender.to_string() })),
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
            r#"subscription($filter: EventFilter!) {
                events(filter: $filter) {
                    node {
                        transaction {
                            digest
                            sender { address }
                            kind { __typename }
                            gasInput { gasBudget gasPrice }
                        }
                        contents { type { repr } }
                    }
                }
            }"#,
            filter_var(json!({ "type": package_id.to_string() })),
        )
        .await;

    let _digest = emit_event_harness::emit(&mut cluster.validator, package_id).await;
    let item = stream.next().await.expect("Stream ended");

    graphql_redactions().bind(|| {
        insta::assert_json_snapshot!("event_subscription_transaction_fields", item);
    });
}

/// Verifies the `IAddressable.asTransactionObject` resolver in streaming mode for the
/// `ObjectChange` variant.
#[tokio::test]
async fn test_event_subscription_as_transaction_object_change() {
    let mut cluster = SubscriptionTestCluster::new().await;
    let package_id = emit_event_harness::publish(&mut cluster.validator).await;

    // Pre-create the object in a separate tx, before subscribing, so the subscription
    // only sees the mutation tx's event.
    let (_, object_ref) =
        emit_event_harness::create_object(&mut cluster.validator, package_id, /* value = */ 7)
            .await;

    let mut stream = cluster
        .subscribe_with_variables(
            r#"subscription($filter: EventFilter!) {
                events(filter: $filter) {
                    node {
                        contents {
                            extract(path: "address_event_id") {
                                asAddress {
                                    asTransactionObject {
                                        __typename
                                        ... on ObjectChange {
                                            inputState {
                                                asMoveObject { contents { json } }
                                            }
                                            outputState {
                                                asMoveObject { contents { json } }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }"#,
            filter_var(json!({ "type": package_id.to_string() })),
        )
        .await;

    let _digest = emit_event_harness::mutate_and_emit(
        &mut cluster.validator,
        package_id,
        object_ref,
        /* new_value = */ 99,
    )
    .await;
    let item = stream.next().await.expect("Stream ended");

    graphql_redactions().bind(|| {
        insta::assert_json_snapshot!("event_subscription_as_transaction_object_change", item);
    });
}

/// Verifies the `IAddressable.asTransactionObject` resolver in streaming mode for the
/// `ConsensusObjectRead` variant. The test tx takes the (read-only) shared clock as a
/// consensus input and emits a `TestAddressEvent` whose payload is the clock's address.
#[tokio::test]
async fn test_event_subscription_as_transaction_object_consensus_read() {
    let mut cluster = SubscriptionTestCluster::new().await;
    let package_id = emit_event_harness::publish(&mut cluster.validator).await;

    let mut stream = cluster
        .subscribe_with_variables(
            r#"subscription($filter: EventFilter!) {
                events(filter: $filter) {
                    node {
                        contents {
                            extract(path: "address_event_id") {
                                asAddress {
                                    asTransactionObject {
                                        __typename
                                        ... on ConsensusObjectRead {
                                            object {
                                                asMoveObject { contents { type { repr } } }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }"#,
            filter_var(json!({ "type": package_id.to_string() })),
        )
        .await;

    let _digest = emit_event_harness::emit_with_clock(&mut cluster.validator, package_id).await;
    let item = stream.next().await.expect("Stream ended");

    graphql_redactions().bind(|| {
        insta::assert_json_snapshot!(
            "event_subscription_as_transaction_object_consensus_read",
            item
        );
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
            r#"subscription($filter: EventFilter!) {
                events(filter: $filter) {
                    node {
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
                }
            }"#,
            filter_var(json!({ "type": package_id.to_string() })),
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
            r#"subscription($filter: EventFilter!) {
                events(filter: $filter) {
                    node {
                        contents { type { repr } }
                    }
                }
            }"#,
            filter_var(json!({ "module": format!("{}::emit_test_event", package_id) })),
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
            r#"subscription($filter: EventFilter!) {
                events(filter: $filter) {
                    node {
                        eventBcs
                        contents { json }
                    }
                }
            }"#,
            filter_var(json!({ "type": package_id.to_string() })),
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

#[tokio::test]
async fn test_event_subscription_ordering() {
    let mut cluster = SubscriptionTestCluster::new().await;
    let package_id = emit_event_harness::publish(&mut cluster.validator).await;

    let mut stream = cluster
        .subscribe_with_variables(&event_query(), type_filter_var(&package_id, None))
        .await;

    for v in [100u64, 200, 300] {
        emit_event_harness::emit_with_value(&mut cluster.validator, package_id, v).await;
    }

    // Events arrive in emit order (parsed `value`), each with distinct `eventBcs`, so neither a
    // reorder nor a duplicate can slip through.
    let events = take_events(&mut stream, 3).await;
    let values: Vec<&str> = events.iter().filter_map(event_value).collect();
    assert_eq!(values, vec!["100", "200", "300"], "events out of order");
    let bcs: HashSet<&str> = events.iter().filter_map(event_bcs).collect();
    assert_eq!(bcs.len(), 3, "eventBcs should be distinct across emits");
}

#[tokio::test]
async fn test_event_subscription_resume_backfill_then_live() {
    let mut cluster = SubscriptionTestCluster::new_with_ledger_history().await;
    let package_id = emit_event_harness::publish(&mut cluster.validator).await;

    // Capture the tip BEFORE the emit so it lands strictly past the resume point.
    let resume_from = cluster.validator_checkpoint_tip();
    let digest =
        emit_event_harness::emit_with_value(&mut cluster.validator, package_id, 1000).await;

    // Make the event backfill-only for the resume below.
    cluster.wait_until_backfillable(resume_from, &digest).await;

    let mut stream = cluster
        .subscribe_with_variables(
            &event_query(),
            type_filter_var(&package_id, Some(resume_from)),
        )
        .await;

    // Phase 1: the pre-subscription event arrives via backfill.
    wait_for_matching_item(&mut stream, &["1000".to_string()], event_values).await;

    // Phase 2: an event emitted after subscribing arrives via the live path.
    emit_event_harness::emit_with_value(&mut cluster.validator, package_id, 2000).await;
    wait_for_matching_item(&mut stream, &["2000".to_string()], event_values).await;
}

#[tokio::test]
async fn test_event_subscription_start_too_far_ahead_errors() {
    let cluster = SubscriptionTestCluster::new_with_ledger_history().await;

    // Well past the default window allowed ahead of the tip (~300 checkpoints, about a minute).
    let far_ahead = cluster.validator_checkpoint_tip() + 1_000;
    let query = format!(
        "subscription {{ events(filter: {{ afterCheckpoint: {far_ahead} }}) {{ node {{ sequenceNumber }} }} }}",
    );
    let mut stream = cluster.subscribe(&query).await;

    let item = stream.next().await.unwrap();
    let message = item["errors"][0]["message"].as_str().unwrap_or_default();
    assert_eq!(
        message,
        "Cannot start a subscription more than 300 checkpoints ahead of the current tip"
    );
}

#[tokio::test]
async fn test_event_subscription_empty_backfill_hands_off_to_live() {
    let mut cluster = SubscriptionTestCluster::new_with_ledger_history().await;
    let package_id = emit_event_harness::publish(&mut cluster.validator).await;

    // Resume from the current tip; no matching events are produced in the backfill range.
    let resume_from = cluster.validator_checkpoint_tip();
    let mut stream = cluster
        .subscribe_with_variables(
            &event_query(),
            type_filter_var(&package_id, Some(resume_from)),
        )
        .await;

    // Let the validator advance through empty checkpoints so the backfill scans a match-less range,
    // pins the handoff via a coverage marker, and transitions to live before any match exists. Kept
    // as a timed wait: there is no pre-subscription match to probe on.
    tokio::time::sleep(BACKFILL_SETTLE).await;

    // The only match is emitted after the handoff, so it can only be delivered by the live path. If
    // pinning required a scanned match, the backfill would never hand off and this would time out.
    emit_event_harness::emit_with_value(&mut cluster.validator, package_id, 1000).await;
    wait_for_matching_item(&mut stream, &["1000".to_string()], event_values).await;
}

#[tokio::test]
async fn test_event_subscription_exactly_once_across_handoff() {
    let mut cluster = SubscriptionTestCluster::new_with_ledger_history().await;
    let package_id = emit_event_harness::publish(&mut cluster.validator).await;

    // Resume so Phase 1 (backfill) runs and hands off to live.
    let resume_from = cluster.validator_checkpoint_tip();
    let mut stream = cluster
        .subscribe_with_variables(
            &event_query(),
            type_filter_var(&package_id, Some(resume_from)),
        )
        .await;

    // Straddle the handoff: the first batch lands while the backfill is scanning, the second after
    // it has pinned and moved to live. Exactly-once must hold across the seam wherever it pins.
    let mut digest = String::new();
    for v in [100u64, 200] {
        digest = emit_event_harness::emit_with_value(&mut cluster.validator, package_id, v).await;
    }
    // Let the first batch reach the broadcast before emitting the second, so they straddle the seam.
    cluster.wait_until_backfillable(resume_from, &digest).await;
    for v in [300u64, 400] {
        emit_event_harness::emit_with_value(&mut cluster.validator, package_id, v).await;
    }

    // Every event arrives exactly once across the seam: a reorder or drop breaks the value order, a
    // duplicate at the seam collapses the `eventBcs` distinct count.
    let events = take_events(&mut stream, 4).await;
    let values: Vec<&str> = events.iter().filter_map(event_value).collect();
    assert_eq!(
        values,
        vec!["100", "200", "300", "400"],
        "events reordered or dropped across handoff",
    );
    let bcs: HashSet<&str> = events.iter().filter_map(event_bcs).collect();
    assert_eq!(
        bcs.len(),
        4,
        "an event was delivered twice across the handoff"
    );
}

#[tokio::test]
async fn test_event_subscription_resume_with_after_cursor() {
    let mut cluster = SubscriptionTestCluster::new_with_ledger_history().await;
    let package_id = emit_event_harness::publish(&mut cluster.validator).await;

    let resume_from = cluster.validator_checkpoint_tip();
    let mut digest = String::new();
    for v in [1000u64, 2000] {
        digest = emit_event_harness::emit_with_value(&mut cluster.validator, package_id, v).await;
    }

    // Make both events backfill-only for the subscriptions below.
    cluster.wait_until_backfillable(resume_from, &digest).await;

    // Subscription 1: backfill from `afterCheckpoint`. Capture the first edge's cursor and value.
    let mut stream = cluster
        .subscribe_with_variables(
            &event_query(),
            type_filter_var(&package_id, Some(resume_from)),
        )
        .await;
    let first = stream.next().await.expect("no backfilled edge");
    let first_edge = &first["data"]["events"];
    let first_value = first_edge["node"]["contents"]["json"]["value"]
        .as_str()
        .expect("edge missing value")
        .to_string();
    let after = first_edge["cursor"]
        .as_str()
        .expect("backfill edge missing cursor")
        .to_string();
    assert!(["1000", "2000"].contains(&first_value.as_str()));
    drop(stream);

    // Subscription 2: resume via `after`. The next matching event must be the other one, proving the
    // event at the cursor was skipped rather than re-delivered.
    let query = r#"subscription($after: String!, $filter: EventFilter!) {
        events(after: $after, filter: $filter) {
            node { contents { json } }
        }
    }"#;
    let mut vars = type_filter_var(&package_id, None).unwrap();
    vars["after"] = json!(after);
    let mut stream = cluster.subscribe_with_variables(query, Some(vars)).await;

    let expected_other = if first_value == "1000" {
        "2000"
    } else {
        "1000"
    };
    let item =
        wait_for_matching_item(&mut stream, &[expected_other.to_string()], event_values).await;
    assert_ne!(
        event_value(&item),
        Some(first_value.as_str()),
        "resume-by-cursor re-delivered the event at the cursor",
    );
}

#[tokio::test]
async fn test_event_subscription_live_backfill_parity() {
    let mut cluster = SubscriptionTestCluster::new_with_ledger_history().await;
    let package_id = emit_event_harness::publish(&mut cluster.validator).await;

    // 1. Start live.
    let mut live = cluster
        .subscribe_with_variables(&event_query(), type_filter_var(&package_id, None))
        .await;

    // 2. Emit a sequence of distinct events.
    let resume_from = cluster.validator_checkpoint_tip();
    let expected: Vec<String> = ["10", "20", "30"].iter().map(|s| s.to_string()).collect();
    let mut digest = String::new();
    for v in [10u64, 20, 30] {
        digest = emit_event_harness::emit_with_value(&mut cluster.validator, package_id, v).await;
    }

    // 3. Collect the live nodes, then drop the live subscription.
    let live_nodes = collect_event_nodes(&mut live, &expected).await;
    drop(live);

    // 4. Resume from before the emits so the same events arrive via backfill.
    cluster.wait_until_backfillable(resume_from, &digest).await;
    let mut backfill = cluster
        .subscribe_with_variables(
            &event_query(),
            type_filter_var(&package_id, Some(resume_from)),
        )
        .await;
    let backfill_nodes = collect_event_nodes(&mut backfill, &expected).await;

    // The two phases resolve the same events identically. Both run with no `checkpoint_viewed_at`, so
    // even checkpoint-anchored fields are null on both and compare equal without normalization.
    assert_eq!(
        live_nodes, backfill_nodes,
        "live and backfill resolved the same events differently",
    );
}

#[tokio::test]
async fn test_event_subscription_invalid_cursor_errors() {
    let mut cluster = SubscriptionTestCluster::new().await;
    let package_id = emit_event_harness::publish(&mut cluster.validator).await;

    let mut stream = cluster
        .subscribe_with_variables(
            r#"subscription($filter: EventFilter!) {
                events(after: "not-a-valid-cursor", filter: $filter) {
                    node { eventBcs }
                }
            }"#,
            type_filter_var(&package_id, None),
        )
        .await;

    let item = stream.next().await.unwrap();
    assert!(
        item.get("errors").is_some(),
        "invalid cursor should surface a GraphQL error, got: {item}",
    );
}

/// `after` (a cursor) and `filter.afterCheckpoint` apply together, and delivery resumes from
/// whichever is later. Both directions are checked: with the cursor at the first event and
/// `afterCheckpoint` past the second, the checkpoint bound wins and the second is skipped; with the
/// cursor at the second and `afterCheckpoint` before the first, the cursor wins and both are
/// skipped. Either way the third event is first.
#[tokio::test]
async fn test_event_subscription_resume_intersects_after_and_checkpoint() {
    let mut cluster = SubscriptionTestCluster::new_with_ledger_history().await;
    let package_id = emit_event_harness::publish(&mut cluster.validator).await;

    let resume_from = cluster.validator_checkpoint_tip();
    emit_event_harness::emit_with_value(&mut cluster.validator, package_id, 100).await;
    let d200 = emit_event_harness::emit_with_value(&mut cluster.validator, package_id, 200).await;
    // Seal the second event's checkpoint before capturing `bound`, so `afterCheckpoint: bound`
    // excludes it.
    cluster.wait_until_backfillable(resume_from, &d200).await;
    let bound = cluster.validator_checkpoint_tip();
    let d300 = emit_event_harness::emit_with_value(&mut cluster.validator, package_id, 300).await;

    // Make all three backfill-only, so the resume bounds apply.
    cluster.wait_until_backfillable(resume_from, &d300).await;

    // Backfill the three events once to mint real cursors at the first two.
    let mut stream = cluster
        .subscribe_with_variables(
            &event_query(),
            type_filter_var(&package_id, Some(resume_from)),
        )
        .await;
    let first = stream.next().await.expect("no first event");
    assert_eq!(event_value(&first), Some("100"));
    let cursor_1 = event_cursor(&first);
    let second = stream.next().await.expect("no second event");
    assert_eq!(event_value(&second), Some("200"));
    let cursor_2 = event_cursor(&second);
    drop(stream);

    // afterCheckpoint wins: `after` at the first event would include the second, but the later bound
    // past it skips it.
    let query = r#"subscription($after: String!, $filter: EventFilter!) {
        events(after: $after, filter: $filter) {
            node { contents { json } }
        }
    }"#;
    let mut vars = type_filter_var(&package_id, Some(bound)).unwrap();
    vars["after"] = json!(cursor_1);
    let mut stream = cluster.subscribe_with_variables(query, Some(vars)).await;
    let delivered = stream.next().await.expect("no delivered edge");
    assert_eq!(
        event_value(&delivered),
        Some("300"),
        "afterCheckpoint should win over the earlier cursor and skip the second event",
    );
    drop(stream);

    // after wins: `afterCheckpoint` before the first event would include both, but the later cursor
    // at the second skips them.
    let query = r#"subscription($after: String!, $filter: EventFilter!) {
        events(after: $after, filter: $filter) {
            node { contents { json } }
        }
    }"#;
    let mut vars = type_filter_var(&package_id, Some(resume_from)).unwrap();
    vars["after"] = json!(cursor_2);
    let mut stream = cluster.subscribe_with_variables(query, Some(vars)).await;
    let delivered = stream.next().await.expect("no delivered edge");
    assert_eq!(
        event_value(&delivered),
        Some("300"),
        "the later cursor should win over afterCheckpoint and skip the first two events",
    );
}

/// A checkpoint predicate inside the filter is rejected: a subscription streams forward, so
/// filter-level checkpoint bounds have no meaning.
#[tokio::test]
async fn test_event_subscription_checkpoint_filter_errors() {
    let mut cluster = SubscriptionTestCluster::new().await;
    let package_id = emit_event_harness::publish(&mut cluster.validator).await;

    let mut stream = cluster
        .subscribe_with_variables(
            r#"subscription($filter: EventFilter!) {
                events(filter: $filter) {
                    node { eventBcs }
                }
            }"#,
            filter_var(json!({ "type": package_id.to_string(), "atCheckpoint": 1 })),
        )
        .await;

    let item = stream.next().await.unwrap();
    assert!(
        item.get("errors").is_some(),
        "a checkpoint filter should be rejected, got: {item}",
    );
}

/// An event subscription query selecting each object change's own Move type, its
/// `previousTransaction`, and, through the output object's `previousTransaction`, the Move contents of
/// that transaction's own input objects. Those input objects belong to earlier checkpoints than the
/// emitting transaction, so resolving their contents on the live path, reached through a
/// `previousTransaction` hop, exercises the streamed object store. The object's own type also names
/// each change, giving the snapshot a stable order. Live by default; resumes (backfill) when
/// `after_checkpoint` is set.
fn prev_tx_event_query(filter: &str, after_checkpoint: Option<u64>) -> String {
    let resume = after_checkpoint
        .map(|c| format!("afterCheckpoint: {c},"))
        .unwrap_or_default();
    format!(
        r#"subscription($pkg: SuiAddress!) {{
            events(filter: {{ {resume} {filter} }}) {{
                node {{
                    transaction {{
                        digest
                        effects {{
                            objectChanges {{
                                nodes {{
                                    inputState {{
                                        asMoveObject {{ contents {{ type {{ repr }} }} }}
                                        previousTransaction {{ digest sender {{ address }} kind {{ __typename }} }}
                                    }}
                                    outputState {{
                                        asMoveObject {{ contents {{ type {{ repr }} }} }}
                                        previousTransaction {{
                                            digest sender {{ address }} kind {{ __typename }}
                                            effects {{ objectChanges(first: 10) {{ nodes {{ inputState {{ asMoveObject {{ contents {{ type {{ repr }} }} }} }} }} }} }}
                                        }}
                                    }}
                                }}
                            }}
                        }}
                    }}
                }}
            }}
        }}"#
    )
}

/// The next delivered event node whose emitting transaction has `tx_digest`.
async fn next_event_for_tx(
    stream: &mut (impl tokio_stream::Stream<Item = Value> + Unpin),
    tx_digest: &str,
) -> Value {
    loop {
        let item = stream
            .next()
            .await
            .expect("stream ended before the event arrived");
        let node = item["data"]["events"]["node"].clone();
        if node["transaction"]["digest"].as_str() == Some(tx_digest) {
            return node;
        }
    }
}

/// `previousTransaction` reached from a live *event* subscription (event -> transaction -> object)
/// resolves its full contents, and identically to the backfill path, as do the Move contents of
/// objects reached through a `previousTransaction` hop.
///
/// The emitting transaction mutates an object created by an earlier transaction, so the object's
/// `inputState.previousTransaction` is a different transaction than the one being delivered. On the
/// live path the scope is anchored to the emitting transaction, so that lookup misses the anchor and
/// is served by the in-memory streamed transaction store while the checkpoint runs ahead of the KV
/// backend; the objects reached through `previousTransaction` belong to earlier checkpoints and have
/// their contents served by the streamed object store. The backfill path resolves the same data from
/// the index.
#[tokio::test]
async fn test_event_subscription_previous_transaction_parity() {
    let mut cluster = SubscriptionTestCluster::new_with_ledger_history().await;
    let package_id = emit_event_harness::publish(&mut cluster.validator).await;
    let pkg_var = || Some(json!({ "pkg": package_id.to_string() }));

    // 1. Start live.
    let mut live = cluster
        .subscribe_with_variables(&prev_tx_event_query("type: $pkg", None), pkg_var())
        .await;

    // 2. Create an object (no event), then mutate it and emit an event, so the emitting transaction's
    //    object change has an input object created by the earlier transaction.
    let resume_from = cluster.validator_checkpoint_tip();
    let (_create, object) =
        emit_event_harness::create_object(&mut cluster.validator, package_id, 42).await;
    let emit =
        emit_event_harness::mutate_and_emit(&mut cluster.validator, package_id, object, 100).await;

    // 3. Collect the live event node for the emitting transaction, then drop the live subscription.
    let mut live_node = next_event_for_tx(&mut live, &emit).await;
    drop(live);

    // 4. Resume from before the lifecycle so the same event arrives via backfill.
    tokio::time::sleep(BACKFILL_SETTLE).await;
    let mut backfill = cluster
        .subscribe_with_variables(
            &prev_tx_event_query("type: $pkg", Some(resume_from)),
            pkg_var(),
        )
        .await;
    let mut backfill_node = next_event_for_tx(&mut backfill, &emit).await;

    // 5. Sort object changes (including those nested under `previousTransaction`) by type so the
    //    snapshot is stable, then check live and backfill resolve identically.
    sort_object_changes(&mut live_node);
    sort_object_changes(&mut backfill_node);
    assert_eq!(
        live_node, backfill_node,
        "live and backfill resolved event.transaction previousTransaction differently",
    );

    // 6. Pin the resolved shape, including the object contents reached through previousTransaction.
    //    A regression that failed to resolve would surface here as null contents.
    graphql_redactions().bind(|| {
        insta::assert_json_snapshot!("event_subscription_previous_transaction", live_node);
    });
}
