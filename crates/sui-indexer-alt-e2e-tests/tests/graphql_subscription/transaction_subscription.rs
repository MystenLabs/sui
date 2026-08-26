// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! These tests use `#[tokio::test]` rather than the workspace-standard simulator test
//! attribute because the harness needs a real Postgres `TempDb` and a real
//! `TestClusterBuilder` validator, neither of which works inside the simulator's
//! deterministic runtime.

use std::collections::BTreeSet;
use std::time::Duration;

use async_graphql::connection::CursorType;
use serde_json::Value;
use serde_json::json;
use sui_indexer_alt_graphql::CTransaction;
use sui_rpc_cursor::CursorToken;
use sui_rpc_cursor::Position;
use sui_types::base_types::SuiAddress;
use test_cluster::TestCluster;
use tokio_stream::StreamExt;

use super::testing::SubscriptionTestCluster;
use super::testing::graphql_redactions;
use super::testing::object_wrapping_harness::create_item;
use super::testing::object_wrapping_harness::publish;
use super::testing::object_wrapping_harness::unwrap_wrapper;
use super::testing::object_wrapping_harness::update_item;
use super::testing::object_wrapping_harness::wrap_item;
use super::testing::sort_object_changes;
use super::testing::transaction_digest;
use super::testing::transfer_coins;
use super::testing::wait_for_matching_item;

/// How long backfill tests let the validator advance after executing their pre-subscription
/// transactions, so a later subscription's live receiver pins past them and they can only be
/// delivered through the backfill scan (never the live path).
const BACKFILL_SETTLE: Duration = Duration::from_secs(5);

/// Decode a transaction edge's `cursor` field into its `(checkpoint, tx_sequence)` position.
fn decode_tx_cursor(edge: &serde_json::Value) -> (u64, u64) {
    let cursor = edge["cursor"]
        .as_str()
        .expect("transaction edge missing cursor");
    let ct = CTransaction::decode_cursor(cursor).expect("cursor is not a valid CTransaction");
    match CursorToken::from(&*ct).position {
        Position::Transactions { checkpoint, tx_seq } => (checkpoint, tx_seq),
        position => panic!("expected a transactions cursor, got {position:?}"),
    }
}

/// The `after` cursor string from a delivered transaction edge.
fn cursor_of(item: &Value) -> String {
    item["data"]["transactions"]["cursor"]
        .as_str()
        .expect("edge missing cursor")
        .to_string()
}

/// The `{ "sender": ... }` variables shared by the filtered subscription queries.
fn sender_var(sender: SuiAddress) -> Option<Value> {
    Some(json!({ "sender": sender.to_string() }))
}

/// The rich transaction-subscription query under a given `filter` predicate: live by default, or
/// resuming from a checkpoint (backfill) when `after_checkpoint` is set.
fn tx_query(filter: &str, after_checkpoint: Option<u64>) -> String {
    let resume = after_checkpoint
        .map(|c| format!("afterCheckpoint: {c},"))
        .unwrap_or_default();
    format!(
        r#"subscription($sender: SuiAddress!) {{
            transactions(filter: {{ {resume} {filter} }}) {{
                node {{
                    digest
                    kind {{
                        __typename
                        ... on ProgrammableTransaction {{
                            commands {{ nodes {{ __typename }} }}
                            inputs {{ nodes {{ __typename }} }}
                        }}
                    }}
                    sender {{ address }}
                    signatures {{ signatureBytes }}
                    gasInput {{ gasBudget gasPrice gasSponsor {{ address }} }}
                    effects {{
                        status
                        lamportVersion
                        epoch {{ epochId }}
                        checkpoint {{ sequenceNumber }}
                        gasEffects {{
                            gasObject {{ address }}
                            gasSummary {{
                                computationCost
                                storageCost
                                storageRebate
                                nonRefundableStorageFee
                            }}
                        }}
                        objectChanges {{
                            nodes {{
                                outputState {{
                                    address
                                    asMoveObject {{ contents {{ type {{ repr }} json }} }}
                                }}
                            }}
                        }}
                        events {{ nodes {{ __typename }} }}
                        dependencies {{ nodes {{ digest }} }}
                    }}
                }}
            }}
        }}"#
    )
}

/// Collect the `node` of each expected tx from a subscription, returned in `expected` order (skips
/// any non-expected txs) so the comparison is stable.
async fn collect_nodes(
    stream: &mut (impl tokio_stream::Stream<Item = Value> + Unpin),
    expected: &[String],
) -> Vec<Value> {
    let mut by_digest: std::collections::BTreeMap<String, Value> =
        std::collections::BTreeMap::new();
    while by_digest.len() < expected.len() {
        let item = stream
            .next()
            .await
            .expect("stream ended before all expected txs");
        let node = item["data"]["transactions"]["node"].clone();
        let digest = node["digest"]
            .as_str()
            .expect("edge missing digest")
            .to_string();
        if expected.contains(&digest) {
            by_digest.insert(digest, node);
        }
    }
    expected.iter().map(|d| by_digest[d].clone()).collect()
}

/// Execute `n` matching SUI transfers in soft bundles of four (one bundle per checkpoint) and return
/// their digests.
async fn execute_n_transfers(validator: &mut TestCluster, n: usize) -> BTreeSet<String> {
    let mut digests = BTreeSet::new();
    while digests.len() < n {
        digests.extend(transfer_coins(validator, &[100, 200, 300, 400]).await);
    }
    digests
}

/// Take the next `n` payloads and return their transaction digests in arrival order.
async fn take_digests(
    stream: &mut (impl tokio_stream::Stream<Item = Value> + Unpin),
    n: usize,
) -> Vec<String> {
    stream
        .take(n)
        .map(|item| {
            item["data"]["transactions"]["node"]["digest"]
                .as_str()
                .unwrap()
                .to_string()
        })
        .collect()
        .await
}

/// Consume `stream` until every digest in `expected` has arrived, asserting matches arrive in
/// strictly increasing cursor order. That single invariant rules out both re-ordering and
/// re-delivery: a repeat or an out-of-order edge carries a cursor that is not greater than the
/// previous one. Non-expected transactions from the same sender (e.g. wallet gas management) are
/// skipped. Returns the matched `node` objects in arrival order, so callers can assert their
/// content.
async fn collect_matches(
    stream: &mut (impl tokio_stream::Stream<Item = Value> + Unpin),
    expected: &BTreeSet<String>,
) -> Vec<Value> {
    let mut seen = BTreeSet::new();
    let mut nodes = Vec::new();
    let mut prev: Option<(u64, u64)> = None;
    while !expected.is_subset(&seen) {
        let item = stream.next().await.unwrap();
        let edge = &item["data"]["transactions"];
        let cursor = decode_tx_cursor(edge);
        assert!(
            prev.is_none_or(|p| cursor > p),
            "matches not delivered in strictly increasing cursor order: {prev:?} then {cursor:?}",
        );
        prev = Some(cursor);
        let digest = edge["node"]["digest"].as_str().unwrap().to_string();
        if expected.contains(&digest) {
            seen.insert(digest);
            nodes.push(edge["node"].clone());
        }
    }
    nodes
}

#[tokio::test]
async fn test_transaction_subscription() {
    let mut cluster = SubscriptionTestCluster::new_with_ledger_history().await;
    let sender = cluster.validator.wallet.active_address().unwrap();

    let mut stream = cluster
        .subscribe_with_variables(
            r#"subscription($sender: SuiAddress!) {
                transactions(filter: { sentAddress: $sender }) {
                    node {
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
    let mut cluster = SubscriptionTestCluster::new_with_ledger_history().await;
    let sender = cluster.validator.wallet.active_address().unwrap();
    let package_id = publish(&mut cluster.validator).await;

    let mut stream = cluster
        .subscribe_with_variables(
            r#"subscription($sender: SuiAddress!) {
                transactions(filter: { sentAddress: $sender }) {
                    node {
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
            }"#,
            Some(json!({ "sender": sender.to_string() })),
        )
        .await;

    let (digest, _) = create_item(&mut cluster.validator, package_id, 42).await;
    let item = wait_for_matching_item(&mut stream, &[digest], transaction_digest).await;

    graphql_redactions().bind(|| {
        insta::assert_json_snapshot!("transaction_subscription_object_changes", item);
    });
}

/// Snapshots a broad field set to surface any field that fails to resolve in streaming mode.
#[tokio::test]
async fn test_transaction_subscription_field_coverage() {
    let mut cluster = SubscriptionTestCluster::new_with_ledger_history().await;
    let sender = cluster.validator.wallet.active_address().unwrap();
    let package_id = publish(&mut cluster.validator).await;

    let mut stream = cluster
        .subscribe_with_variables(
            r#"subscription($sender: SuiAddress!) {
                transactions(filter: { sentAddress: $sender }) {
                    node {
                        digest
                        kind {
                            __typename
                            ... on ProgrammableTransaction {
                                commands { nodes { __typename } }
                                inputs { nodes { __typename } }
                            }
                        }
                        sender { address }
                        signatures { signatureBytes }
                        gasInput { gasBudget gasPrice gasSponsor { address } }
                        effects {
                            status
                            lamportVersion
                            epoch { epochId }
                            checkpoint { sequenceNumber }
                            gasEffects {
                                gasObject { address }
                                gasSummary {
                                    computationCost
                                    storageCost
                                    storageRebate
                                    nonRefundableStorageFee
                                }
                            }
                            objectChanges {
                                nodes {
                                    outputState {
                                        address
                                        asMoveObject { contents { type { repr } json } }
                                    }
                                }
                            }
                            events { nodes { __typename } }
                            dependencies { nodes { digest } }
                        }
                    }
                }
            }"#,
            Some(json!({ "sender": sender.to_string() })),
        )
        .await;

    let (digest, _) = create_item(&mut cluster.validator, package_id, 42).await;
    let item = wait_for_matching_item(&mut stream, &[digest], transaction_digest).await;

    let mut settings = graphql_redactions();
    settings.add_redaction(".**.signatureBytes", "[signature]");
    settings.add_redaction(".**.lamportVersion", "[lamportVersion]");
    settings.add_redaction(".**.gasSummary", "[gasSummary]");
    settings.add_redaction(".**.json", "[json]");
    settings.bind(|| {
        insta::assert_json_snapshot!("transaction_subscription_field_coverage", item);
    });
}

#[tokio::test]
async fn test_transaction_subscription_ordering() {
    let mut cluster = SubscriptionTestCluster::new_with_ledger_history().await;
    let sender = cluster.validator.wallet.active_address().unwrap();

    let mut stream = cluster
        .subscribe_with_variables(
            r#"subscription($sender: SuiAddress!) {
                transactions(filter: { sentAddress: $sender }) {
                    cursor
                    node { digest }
                }
            }"#,
            Some(json!({ "sender": sender.to_string() })),
        )
        .await;

    let expected: BTreeSet<String> = transfer_coins(&mut cluster.validator, &[100, 200, 300])
        .await
        .into_iter()
        .collect();

    // Each edge carries a strictly larger cursor than the last, and every match arrives.
    collect_matches(&mut stream, &expected).await;
}

#[tokio::test]
async fn test_transaction_subscription_resume_backfill_then_live() {
    let mut cluster = SubscriptionTestCluster::new_with_ledger_history().await;
    let sender = cluster.validator.wallet.active_address().unwrap();

    // Capture the tip BEFORE the tx so it lands strictly past the resume point.
    let resume_from = cluster.validator_checkpoint_tip();
    let backfilled = transfer_coins(&mut cluster.validator, &[1000]).await;

    // Advance the validator so a fresh subscription's live receiver pins past the tx: it can only
    // be delivered through the backfill scan.
    tokio::time::sleep(BACKFILL_SETTLE).await;

    let query = format!(
        r#"subscription($sender: SuiAddress!) {{
            transactions(filter: {{ afterCheckpoint: {resume_from}, sentAddress: $sender }}) {{
                node {{ digest }}
            }}
        }}"#,
    );
    let mut stream = cluster
        .subscribe_with_variables(&query, Some(json!({ "sender": sender.to_string() })))
        .await;

    // Phase 1: the pre-subscription tx arrives via backfill.
    wait_for_matching_item(&mut stream, &backfilled, transaction_digest).await;

    // Phase 2: a tx executed after subscribing arrives via the live path.
    let live = transfer_coins(&mut cluster.validator, &[2000]).await;
    wait_for_matching_item(&mut stream, &live, transaction_digest).await;
}

#[tokio::test]
async fn test_transaction_subscription_empty_backfill_hands_off_to_live() {
    let mut cluster = SubscriptionTestCluster::new_with_ledger_history().await;
    let sender = cluster.validator.wallet.active_address().unwrap();

    // Resume from the current tip; no matching txs are produced in the backfill range.
    let resume_from = cluster.validator_checkpoint_tip();
    let query = format!(
        r#"subscription($sender: SuiAddress!) {{
            transactions(filter: {{ afterCheckpoint: {resume_from}, sentAddress: $sender }}) {{
                node {{ digest }}
            }}
        }}"#,
    );
    let mut stream = cluster
        .subscribe_with_variables(&query, sender_var(sender))
        .await;

    // Let the validator advance through empty checkpoints so the backfill scans a match-less range,
    // pins the handoff via a coverage marker, and transitions to live before any match exists.
    tokio::time::sleep(BACKFILL_SETTLE).await;

    // The only match is sent after the handoff, so it can only be delivered by the live path. If
    // pinning required a scanned match, the backfill would never hand off and this would time out.
    let live = transfer_coins(&mut cluster.validator, &[1000]).await;
    wait_for_matching_item(&mut stream, &live, transaction_digest).await;
}

#[tokio::test]
async fn test_transaction_subscription_exactly_once_across_handoff() {
    let mut cluster = SubscriptionTestCluster::new_with_ledger_history().await;
    let sender = cluster.validator.wallet.active_address().unwrap();

    // Resume so Phase 1 (backfill) runs and hands off to live.
    let resume_from = cluster.validator_checkpoint_tip();
    let query = format!(
        r#"subscription($sender: SuiAddress!) {{
            transactions(filter: {{ afterCheckpoint: {resume_from}, sentAddress: $sender }}) {{
                cursor
                node {{ digest }}
            }}
        }}"#,
    );
    let mut stream = cluster
        .subscribe_with_variables(&query, sender_var(sender))
        .await;

    // Straddle the handoff: the first bundle lands while the backfill is scanning, the second after
    // it has pinned and moved to live. Exactly-once must hold across the seam wherever it pins.
    let mut expected: BTreeSet<String> = BTreeSet::new();
    expected.extend(transfer_coins(&mut cluster.validator, &[100, 200]).await);
    tokio::time::sleep(Duration::from_secs(3)).await;
    expected.extend(transfer_coins(&mut cluster.validator, &[300, 400]).await);

    // Every match arrives exactly once across the seam: a gap stalls the subset (timeout), a
    // duplicate at the seam breaks the strictly increasing cursor order.
    collect_matches(&mut stream, &expected).await;
}

#[tokio::test]
async fn test_transaction_subscription_resume_with_after_cursor() {
    let mut cluster = SubscriptionTestCluster::new_with_ledger_history().await;
    let sender = cluster.validator.wallet.active_address().unwrap();

    let resume_from = cluster.validator_checkpoint_tip();
    // One soft bundle yields two transactions with consecutive sequence numbers, both past the
    // resume point.
    let expected: std::collections::BTreeSet<String> =
        transfer_coins(&mut cluster.validator, &[1000, 2000])
            .await
            .into_iter()
            .collect();

    // Both txs must be delivered by the backfill scan, not the live path.
    tokio::time::sleep(BACKFILL_SETTLE).await;

    // Subscription 1: backfill from `afterCheckpoint`. The first edge yielded has the lowest
    // sequence number; capture its cursor and digest.
    let query = format!(
        r#"subscription($sender: SuiAddress!) {{
            transactions(filter: {{ afterCheckpoint: {resume_from}, sentAddress: $sender }}) {{
                cursor
                node {{ digest }}
            }}
        }}"#,
    );
    let mut stream = cluster
        .subscribe_with_variables(&query, Some(json!({ "sender": sender.to_string() })))
        .await;
    let first_item = stream.next().await.expect("no backfilled edge");
    let first_edge = &first_item["data"]["transactions"];
    let first_digest = first_edge["node"]["digest"]
        .as_str()
        .expect("edge missing digest")
        .to_string();
    let after = first_edge["cursor"]
        .as_str()
        .expect("backfill edge missing cursor")
        .to_string();
    assert!(expected.contains(&first_digest));
    drop(stream);

    // Subscription 2: resume via `after`. The next matching tx must be the other one, proving the
    // tx at the cursor was skipped rather than re-delivered.
    let query = format!(
        r#"subscription($sender: SuiAddress!) {{
            transactions(after: "{after}", filter: {{ sentAddress: $sender }}) {{
                node {{ digest }}
            }}
        }}"#,
    );
    let mut stream = cluster
        .subscribe_with_variables(&query, Some(json!({ "sender": sender.to_string() })))
        .await;

    let remaining: Vec<String> = expected.iter().cloned().collect();
    let item = wait_for_matching_item(&mut stream, &remaining, transaction_digest).await;
    let got = transaction_digest(&item);
    assert!(
        !got.contains(&first_digest.as_str()),
        "resume-by-cursor re-delivered the tx at the cursor",
    );
}

#[tokio::test]
async fn test_transaction_subscription_live_backfill_parity() {
    let mut cluster = SubscriptionTestCluster::new_with_ledger_history().await;
    let sender = cluster.validator.wallet.active_address().unwrap();
    let package_id = publish(&mut cluster.validator).await;

    // 1. Start live.
    let mut live = cluster
        .subscribe_with_variables(&tx_query("sentAddress: $sender", None), sender_var(sender))
        .await;

    // 2. Execute a lifecycle of varied object-change shapes (created, mutated, wrapped, deleted).
    let resume_from = cluster.validator_checkpoint_tip();
    let (d1, item) = create_item(&mut cluster.validator, package_id, 42).await;
    let (d2, item) = update_item(&mut cluster.validator, package_id, item, 100).await;
    let (d3, wrapper) = wrap_item(&mut cluster.validator, package_id, item).await;
    let (d4, _) = unwrap_wrapper(&mut cluster.validator, package_id, wrapper).await;
    let expected = vec![d1, d2, d3, d4];

    // 3. Collect the live nodes, then drop the live subscription.
    let live_nodes = collect_nodes(&mut live, &expected).await;
    drop(live);

    // 4. Resume from before the lifecycle so the same txs arrive via backfill.
    tokio::time::sleep(BACKFILL_SETTLE).await;
    let mut backfill = cluster
        .subscribe_with_variables(
            &tx_query("sentAddress: $sender", Some(resume_from)),
            sender_var(sender),
        )
        .await;
    let backfill_nodes = collect_nodes(&mut backfill, &expected).await;

    // 5. The two phases resolve the same transactions identically. Both run with no
    // `checkpoint_viewed_at` (backfill matches live), so even checkpoint-anchored fields like
    // `effects.checkpoint` are null on both and compare equal without any normalization.
    assert_eq!(
        live_nodes, backfill_nodes,
        "live and backfill resolved the same transactions differently",
    );
}

#[tokio::test]
async fn test_transaction_subscription_recovers_from_upstream_disconnect() {
    let (mut cluster, proxy) =
        SubscriptionTestCluster::new_with_disruption_proxy_and_ledger_history().await;
    let sender = cluster.validator.wallet.active_address().unwrap();

    let mut stream = cluster
        .subscribe_with_variables(
            r#"subscription($sender: SuiAddress!) {
                transactions(filter: { sentAddress: $sender }) { cursor node { digest } }
            }"#,
            sender_var(sender),
        )
        .await;

    // Healthy: one live transaction streams through.
    let healthy = transfer_coins(&mut cluster.validator, &[1000]).await;
    wait_for_matching_item(&mut stream, &healthy, transaction_digest).await;

    // Blackout: drop the upstream connection so the streaming server goes silent.
    proxy.block_connections();
    proxy.disconnect_all();
    tokio::time::sleep(Duration::from_secs(2)).await;
    let silence = tokio::time::timeout(Duration::from_secs(1), stream.next()).await;
    assert!(
        silence.is_err(),
        "stream yielded during blackout: {silence:?}"
    );

    // Execute matches during the blackout, one per checkpoint (a sleep between) so their delivery
    // order is deterministic and can be asserted directly.
    let mut expected: Vec<String> = Vec::new();
    for amount in [100u64, 200, 300] {
        expected.extend(transfer_coins(&mut cluster.validator, &[amount]).await);
        tokio::time::sleep(Duration::from_secs(1)).await;
    }

    // Resume: recovery delivers exactly the missed matches, in order.
    proxy.allow_connections();
    let received = take_digests(&mut stream, expected.len()).await;
    assert_eq!(
        received, expected,
        "recovery did not deliver the missed transactions in order"
    );
}

#[tokio::test]
async fn test_transaction_subscription_backfill_spans_scan_pages() {
    // The one knob sets both the resolution window and the scan page; this test cares only that the
    // scan page is 2, so the eight matches below cannot fit in a single page and the scan must chain.
    let mut cluster = SubscriptionTestCluster::new_with_ledger_history_and_concurrency(2).await;
    let sender = cluster.validator.wallet.active_address().unwrap();

    let resume_from = cluster.validator_checkpoint_tip();
    let expected = execute_n_transfers(&mut cluster.validator, 8).await;
    // Advance so the matches are delivered by the backfill scan, not live.
    tokio::time::sleep(BACKFILL_SETTLE).await;

    let query = format!(
        r#"subscription($sender: SuiAddress!) {{
            transactions(filter: {{ afterCheckpoint: {resume_from}, sentAddress: $sender }}) {{
                cursor
                node {{ digest }}
            }}
        }}"#,
    );
    let mut stream = cluster
        .subscribe_with_variables(&query, sender_var(sender))
        .await;

    // All eight arrive exactly once, in cursor order across the page seams.
    collect_matches(&mut stream, &expected).await;
}

#[tokio::test]
async fn test_transaction_subscription_concurrency_coalesces_content_reads() {
    // Deliver the same backfill at a given resolution window; return its content `BatchGetTransactions`
    // round trips and how many matches were delivered. Serial (window 1) is the no-coalescing baseline.
    async fn content_reads_with_concurrency(concurrency: usize) -> (u64, u64) {
        let mut cluster =
            SubscriptionTestCluster::new_with_ledger_history_and_concurrency(concurrency).await;
        let sender = cluster.validator.wallet.active_address().unwrap();
        let resume_from = cluster.validator_checkpoint_tip();
        let expected = execute_n_transfers(&mut cluster.validator, 40).await;
        tokio::time::sleep(BACKFILL_SETTLE).await;

        // `effectsJson` renders effects through the ledger service per payload, a
        // `BatchGetTransactions` read the scan does not prefetch, so each resolution hits the
        // `KvLoader` that coalesces across a window.
        let query = format!(
            r#"subscription($sender: SuiAddress!) {{
                transactions(filter: {{ afterCheckpoint: {resume_from}, sentAddress: $sender }}) {{
                    cursor
                    node {{ digest effects {{ effectsJson }} }}
                }}
            }}"#,
        );
        let before = cluster.ledger_grpc_call_count("BatchGetTransactions");
        let mut stream = cluster
            .subscribe_with_variables(&query, sender_var(sender))
            .await;
        collect_matches(&mut stream, &expected).await;
        let reads = cluster.ledger_grpc_call_count("BatchGetTransactions") - before;
        (reads, expected.len() as u64)
    }

    let (serial, total) = content_reads_with_concurrency(1).await;
    // 100 is the production default window, well above the batch, so every payload resolves in one
    // pass and its reads coalesce.
    let (concurrent, _) = content_reads_with_concurrency(10).await;

    // Serial (window 1) does one read per delivered payload: the no-coalescing baseline.
    assert!(
        serial >= total,
        "serial baseline should be one read per transaction: {serial} reads for {total} txs"
    );
    // Concurrency coalesces reads, so it falls below that baseline. Only the direction is asserted;
    // the exact count is timing-dependent and has no deterministic formula here.
    assert!(
        concurrent < serial,
        "concurrency did not coalesce content reads: concurrent={concurrent} serial={serial}"
    );
}

#[tokio::test]
async fn test_transaction_subscription_invalid_cursor_errors() {
    let mut cluster = SubscriptionTestCluster::new_with_ledger_history().await;
    let sender = cluster.validator.wallet.active_address().unwrap();

    let mut stream = cluster
        .subscribe_with_variables(
            r#"subscription($sender: SuiAddress!) {
                transactions(after: "not-a-valid-cursor", filter: { sentAddress: $sender }) {
                    node { digest }
                }
            }"#,
            sender_var(sender),
        )
        .await;

    let item = stream.next().await.unwrap();
    assert!(
        item.get("errors").is_some(),
        "invalid cursor should surface a GraphQL error, got: {item}",
    );
}

/// `after` (a cursor) and `filter.afterCheckpoint` apply together, and delivery resumes from
/// whichever is later. Both directions are checked: with the cursor at `t1` and `afterCheckpoint`
/// past `t2`, the checkpoint bound wins and `t2` is skipped; with the cursor at `t2` and
/// `afterCheckpoint` before `t1`, the cursor wins and both are skipped. Either way `t3` is first.
#[tokio::test]
async fn test_transaction_subscription_resume_intersects_after_and_checkpoint() {
    let mut cluster = SubscriptionTestCluster::new_with_ledger_history().await;
    let sender = cluster.validator.wallet.active_address().unwrap();

    let resume_from = cluster.validator_checkpoint_tip();
    let t1 = transfer_coins(&mut cluster.validator, &[100]).await;
    let t2 = transfer_coins(&mut cluster.validator, &[200]).await;
    // Seal `t2`'s checkpoint before capturing `bound`, so `afterCheckpoint: bound` excludes t2.
    tokio::time::sleep(Duration::from_secs(1)).await;
    let bound = cluster.validator_checkpoint_tip();
    let t3 = transfer_coins(&mut cluster.validator, &[300]).await;

    // Advance so all three are delivered by the backfill scan, where the resume bounds apply.
    tokio::time::sleep(BACKFILL_SETTLE).await;

    // Backfill the three matches once to mint real cursors at t1 and t2.
    let query = format!(
        r#"subscription($sender: SuiAddress!) {{
            transactions(filter: {{ afterCheckpoint: {resume_from}, sentAddress: $sender }}) {{
                cursor
                node {{ digest }}
            }}
        }}"#,
    );
    let mut stream = cluster
        .subscribe_with_variables(&query, sender_var(sender))
        .await;
    let first = stream.next().await.expect("no first match");
    assert_eq!(transaction_digest(&first), vec![t1[0].as_str()]);
    let cursor_t1 = cursor_of(&first);
    let second = stream.next().await.expect("no second match");
    assert_eq!(transaction_digest(&second), vec![t2[0].as_str()]);
    let cursor_t2 = cursor_of(&second);
    drop(stream);

    // afterCheckpoint wins: `after` at t1 would include t2, but the later bound past t2 skips it.
    let query = format!(
        r#"subscription($sender: SuiAddress!) {{
            transactions(after: "{cursor_t1}", filter: {{ afterCheckpoint: {bound}, sentAddress: $sender }}) {{
                node {{ digest }}
            }}
        }}"#,
    );
    let mut stream = cluster
        .subscribe_with_variables(&query, sender_var(sender))
        .await;
    let delivered = stream.next().await.expect("no delivered edge");
    assert_eq!(
        transaction_digest(&delivered),
        vec![t3[0].as_str()],
        "afterCheckpoint should win over the earlier cursor and skip t2",
    );
    drop(stream);

    // after wins: `afterCheckpoint` before t1 would include t1 and t2, but the later cursor at t2
    // skips both.
    let query = format!(
        r#"subscription($sender: SuiAddress!) {{
            transactions(after: "{cursor_t2}", filter: {{ afterCheckpoint: {resume_from}, sentAddress: $sender }}) {{
                node {{ digest }}
            }}
        }}"#,
    );
    let mut stream = cluster
        .subscribe_with_variables(&query, sender_var(sender))
        .await;
    let delivered = stream.next().await.expect("no delivered edge");
    assert_eq!(
        transaction_digest(&delivered),
        vec![t3[0].as_str()],
        "the later cursor should win over afterCheckpoint and skip t1 and t2",
    );
}

/// A checkpoint predicate inside the filter is rejected: a subscription streams forward, so
/// filter-level checkpoint bounds have no meaning.
#[tokio::test]
async fn test_transaction_subscription_checkpoint_filter_errors() {
    let mut cluster = SubscriptionTestCluster::new_with_ledger_history().await;
    let sender = cluster.validator.wallet.active_address().unwrap();

    let mut stream = cluster
        .subscribe_with_variables(
            r#"subscription($sender: SuiAddress!) {
                transactions(filter: { sentAddress: $sender, atCheckpoint: 1 }) {
                    node { digest }
                }
            }"#,
            sender_var(sender),
        )
        .await;

    let item = stream.next().await.unwrap();
    assert!(
        item.get("errors").is_some(),
        "a checkpoint filter should be rejected, got: {item}",
    );
}

/// A start far beyond the tip is rejected rather than parked: nothing exists to backfill ahead of
/// the tip, so waiting for it could hold a connection open indefinitely.
#[tokio::test]
async fn test_transaction_subscription_start_too_far_ahead_errors() {
    let mut cluster = SubscriptionTestCluster::new_with_ledger_history().await;
    let sender = cluster.validator.wallet.active_address().unwrap();

    // Well past the default window allowed ahead of the tip (~300 checkpoints, about a minute).
    let far_ahead = cluster.validator_checkpoint_tip() + 1_000;
    let mut stream = cluster
        .subscribe_with_variables(
            &tx_query("sentAddress: $sender", Some(far_ahead)),
            sender_var(sender),
        )
        .await;

    let item = stream.next().await.unwrap();
    let message = item["errors"][0]["message"].as_str().unwrap_or_default();
    assert_eq!(
        message,
        "Cannot start a subscription more than 300 checkpoints ahead of the current tip"
    );
}

/// Backfill/live parity for the `affectedAddress` predicate: the same transactions must resolve
/// identically whether matched by the gRPC filter (backfill) or in memory (live). Guards against the
/// two filter translations diverging, which would gap or duplicate at the seam.
#[tokio::test]
async fn test_transaction_subscription_affected_address_parity() {
    let mut cluster = SubscriptionTestCluster::new_with_ledger_history().await;
    let sender = cluster.validator.wallet.active_address().unwrap();
    let filter = "affectedAddress: $sender";

    let mut live = cluster
        .subscribe_with_variables(&tx_query(filter, None), sender_var(sender))
        .await;

    let resume_from = cluster.validator_checkpoint_tip();
    let expected = transfer_coins(&mut cluster.validator, &[100, 200, 300]).await;

    let live_nodes = collect_nodes(&mut live, &expected).await;
    drop(live);

    tokio::time::sleep(BACKFILL_SETTLE).await;
    let mut backfill = cluster
        .subscribe_with_variables(&tx_query(filter, Some(resume_from)), sender_var(sender))
        .await;
    let backfill_nodes = collect_nodes(&mut backfill, &expected).await;

    assert_eq!(
        live_nodes, backfill_nodes,
        "affectedAddress: live and backfill resolved the same transactions differently",
    );
}

/// A transaction subscription query selecting each object change's own Move type, its
/// `previousTransaction`, and, through the output object's `previousTransaction`, the Move contents of
/// that transaction's own input objects. Those input objects belong to earlier checkpoints than the
/// delivered transaction, so resolving their contents on the live path, reached through a
/// `previousTransaction` hop, exercises the streamed object store. The object's own type also names
/// each change, giving the snapshot a stable order. Live by default; resumes (backfill) when
/// `after_checkpoint` is set.
fn prev_tx_query(filter: &str, after_checkpoint: Option<u64>) -> String {
    let resume = after_checkpoint
        .map(|c| format!("afterCheckpoint: {c},"))
        .unwrap_or_default();
    format!(
        r#"subscription($sender: SuiAddress!) {{
            transactions(filter: {{ {resume} {filter} }}) {{
                node {{
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
        }}"#
    )
}

/// The digests of the resolved `previousTransaction`s on the output object of each change in `node`.
/// "Resolved" means the digest plus its `sender` and `kind` contents are present.
fn output_previous_transactions(node: &Value) -> Vec<&str> {
    previous_transactions(node, "outputState")
}

/// The digests of the resolved `previousTransaction`s on the input object of each change in `node`.
fn input_previous_transactions(node: &Value) -> Vec<&str> {
    previous_transactions(node, "inputState")
}

fn previous_transactions<'a>(node: &'a Value, side: &str) -> Vec<&'a str> {
    node["effects"]["objectChanges"]["nodes"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|change| {
            let pt = &change[side]["previousTransaction"];
            let resolved =
                pt["sender"]["address"].is_string() && pt["kind"]["__typename"].is_string();
            resolved.then(|| pt["digest"].as_str()).flatten()
        })
        .collect()
}

/// `previousTransaction` (full contents, not just the digest) resolves from a live subscription
/// identically to the backfill path, across every object-change shape, as do the Move contents of
/// objects reached through a `previousTransaction` hop. On the live path the cross-transaction
/// lookups are served by the in-memory streamed transaction store, and the objects reached through
/// `previousTransaction`, which belong to earlier checkpoints, have their contents served by the
/// streamed object store.
#[tokio::test]
async fn test_transaction_subscription_previous_transaction_parity() {
    let mut cluster = SubscriptionTestCluster::new_with_ledger_history().await;
    let sender = cluster.validator.wallet.active_address().unwrap();
    let package_id = publish(&mut cluster.validator).await;

    // 1. Start live.
    let mut live = cluster
        .subscribe_with_variables(
            &prev_tx_query("sentAddress: $sender", None),
            sender_var(sender),
        )
        .await;

    // 2. Walk one object through every change shape (created, mutated, wrapped, unwrapped, deleted),
    //    each in its own checkpoint.
    let resume_from = cluster.validator_checkpoint_tip();
    let (d1, item) = create_item(&mut cluster.validator, package_id, 42).await;
    let (d2, item) = update_item(&mut cluster.validator, package_id, item, 100).await;
    let (d3, wrapper) = wrap_item(&mut cluster.validator, package_id, item).await;
    let (d4, _) = unwrap_wrapper(&mut cluster.validator, package_id, wrapper).await;
    let digests = vec![d1, d2, d3, d4];

    // 3. Collect the live nodes, then drop the subscription.
    let mut live_nodes = collect_nodes(&mut live, &digests).await;
    drop(live);

    // 4. Resume before the lifecycle so the same txs arrive via backfill.
    tokio::time::sleep(BACKFILL_SETTLE).await;
    let mut backfill = cluster
        .subscribe_with_variables(
            &prev_tx_query("sentAddress: $sender", Some(resume_from)),
            sender_var(sender),
        )
        .await;
    let mut backfill_nodes = collect_nodes(&mut backfill, &digests).await;

    // 5. Sort object changes (including those nested under `previousTransaction`) by type so the
    //    snapshot is stable, then check live and backfill resolve identically.
    live_nodes.iter_mut().for_each(sort_object_changes);
    backfill_nodes.iter_mut().for_each(sort_object_changes);
    assert_eq!(
        live_nodes, backfill_nodes,
        "live and backfill resolved previousTransaction differently",
    );

    // 6. Pin the resolved shape of every object change. Digests are redacted, so this checks shape;
    //    identity is asserted below.
    graphql_redactions().bind(|| {
        insta::assert_json_snapshot!("transaction_subscription_previous_transaction", live_nodes);
    });

    // 7. Same transaction: the created object's previousTransaction is the create itself (digests[0]).
    assert!(
        output_previous_transactions(&live_nodes[0]).contains(&digests[0].as_str()),
        "created object's previousTransaction was not the create transaction: {:#}",
        live_nodes[0],
    );

    // 8. Earlier checkpoint: the mutation's input previousTransaction is the create (served live by
    //    the streamed store, since the anchor is the mutation).
    assert!(
        input_previous_transactions(&live_nodes[1]).contains(&digests[0].as_str()),
        "mutated object's input previousTransaction was not the earlier create transaction: {:#}",
        live_nodes[1],
    );
}
