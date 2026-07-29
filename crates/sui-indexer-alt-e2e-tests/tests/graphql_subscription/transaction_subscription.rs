// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! These tests use `#[tokio::test]` rather than the workspace-standard simulator test
//! attribute because the harness needs a real Postgres `TempDb` and a real
//! `TestClusterBuilder` validator, neither of which works inside the simulator's
//! deterministic runtime.

use std::time::Duration;

use async_graphql::connection::CursorType;
use serde_json::Value;
use serde_json::json;
use sui_indexer_alt_graphql::CTransaction;
use sui_rpc_cursor::CursorToken;
use sui_rpc_cursor::Position;
use sui_types::base_types::SuiAddress;
use tokio_stream::StreamExt;

use super::testing::SubscriptionTestCluster;
use super::testing::graphql_redactions;
use super::testing::object_wrapping_harness::create_item;
use super::testing::object_wrapping_harness::publish;
use super::testing::object_wrapping_harness::unwrap_wrapper;
use super::testing::object_wrapping_harness::update_item;
use super::testing::object_wrapping_harness::wrap_item;
use super::testing::transaction_digest;
use super::testing::transfer_coins;
use super::testing::wait_for_matching_item;

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

/// The `{ "sender": ... }` variables shared by the filtered subscription queries.
fn sender_var(sender: SuiAddress) -> Option<Value> {
    Some(json!({ "sender": sender.to_string() }))
}

/// The rich transaction-subscription query: live by default, or resuming from a checkpoint
/// (backfill) when `after_checkpoint` is set.
fn tx_query(after_checkpoint: Option<u64>) -> String {
    let resume = after_checkpoint
        .map(|c| format!("afterCheckpoint: {c},"))
        .unwrap_or_default();
    format!(
        r#"subscription($sender: SuiAddress!) {{
            transactions({resume} filter: {{ sentAddress: $sender }}) {{
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

/// Field coverage: publishes a Move package, calls into it, and probes a broad set of transaction,
/// effects, and Move-object fields at once. The snapshot documents which fields resolve in streaming
/// mode (and surfaces any that error).
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

/// Live path: transactions stream in tx_sequence order, each carrying a strictly increasing
/// `CTransaction` (tx_sequence_number) cursor. A single soft bundle yields several transactions
/// with consecutive sequence numbers.
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

    let expected: std::collections::BTreeSet<String> =
        transfer_coins(&mut cluster.validator, &[100, 200, 300])
            .await
            .into_iter()
            .collect();

    // Consume edges in arrival order (one per payload); each must carry a strictly larger cursor
    // than the last.
    let mut seen = std::collections::BTreeSet::new();
    let mut prev_cursor: Option<(u64, u64)> = None;
    while seen.len() < expected.len() {
        let item = stream.next().await.expect("stream ended before all txs");
        let edge = &item["data"]["transactions"];
        let digest = edge["node"]["digest"]
            .as_str()
            .expect("edge missing digest")
            .to_string();
        if !expected.contains(&digest) {
            continue;
        }
        let cursor = decode_tx_cursor(edge);
        if let Some(prev) = prev_cursor {
            assert!(
                cursor > prev,
                "cursors not strictly increasing: {prev:?} then {cursor:?}",
            );
        }
        prev_cursor = Some(cursor);
        seen.insert(digest);
    }

    assert_eq!(seen, expected, "did not observe exactly the executed txs");
}

/// Resume path: a transaction executed before the subscription starts is delivered through the
/// backfill scan (`afterCheckpoint`), then the stream transitions to live delivery of a
/// transaction executed after subscribing.
#[tokio::test]
async fn test_transaction_subscription_resume_backfill_then_live() {
    let mut cluster = SubscriptionTestCluster::new_with_ledger_history().await;
    let sender = cluster.validator.wallet.active_address().unwrap();

    // Capture the tip BEFORE the tx so it lands strictly past the resume point.
    let resume_from = cluster.validator_checkpoint_tip();
    let backfilled = transfer_coins(&mut cluster.validator, &[1000]).await;

    // Advance the validator so a fresh subscription's live receiver pins past the tx: it can only
    // be delivered through the backfill scan.
    tokio::time::sleep(Duration::from_secs(5)).await;

    let query = format!(
        r#"subscription($sender: SuiAddress!) {{
            transactions(afterCheckpoint: {resume_from}, filter: {{ sentAddress: $sender }}) {{
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

/// Sparse backfill: when the resumed range contains no matches, Phase 1 has only coverage markers to
/// advance on, so the handoff must be pinned by a coverage marker rather than a match. A tx sent only
/// after the handoff must then arrive via the live path, which proves pinning doesn't depend on the
/// scan producing a match (otherwise a sparse subscription would never hand off).
#[tokio::test]
async fn test_transaction_subscription_empty_backfill_hands_off_to_live() {
    let mut cluster = SubscriptionTestCluster::new_with_ledger_history().await;
    let sender = cluster.validator.wallet.active_address().unwrap();

    // Resume from the current tip; no matching txs are produced in the backfill range.
    let resume_from = cluster.validator_checkpoint_tip();
    let query = format!(
        r#"subscription($sender: SuiAddress!) {{
            transactions(afterCheckpoint: {resume_from}, filter: {{ sentAddress: $sender }}) {{
                node {{ digest }}
            }}
        }}"#,
    );
    let mut stream = cluster
        .subscribe_with_variables(&query, sender_var(sender))
        .await;

    // Let the validator advance through empty checkpoints so the backfill scans a match-less range,
    // pins the handoff via a coverage marker, and transitions to live before any match exists.
    tokio::time::sleep(Duration::from_secs(5)).await;

    // The only match is sent after the handoff, so it can only be delivered by the live path. If
    // pinning required a scanned match, the backfill would never hand off and this would time out.
    let live = transfer_coins(&mut cluster.validator, &[1000]).await;
    wait_for_matching_item(&mut stream, &live, transaction_digest).await;
}

/// No gap and no duplicate across the backfill->live seam: matches produced both before and after the
/// handoff must each be delivered exactly once, whatever checkpoint the handoff happens to pin at.
/// This is the observable invariant the handoff's boundary rules exist to protect.
#[tokio::test]
async fn test_transaction_subscription_exactly_once_across_handoff() {
    let mut cluster = SubscriptionTestCluster::new_with_ledger_history().await;
    let sender = cluster.validator.wallet.active_address().unwrap();

    // Resume so Phase 1 (backfill) runs and hands off to live.
    let resume_from = cluster.validator_checkpoint_tip();
    let query = format!(
        r#"subscription($sender: SuiAddress!) {{
            transactions(afterCheckpoint: {resume_from}, filter: {{ sentAddress: $sender }}) {{
                node {{ digest }}
            }}
        }}"#,
    );
    let mut stream = cluster
        .subscribe_with_variables(&query, sender_var(sender))
        .await;

    // Straddle the handoff: the first bundle lands while the backfill is scanning, the second after
    // it has pinned and moved to live. Exactly-once must hold across the seam wherever it pins.
    let mut expected: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
    expected.extend(transfer_coins(&mut cluster.validator, &[100, 200]).await);
    tokio::time::sleep(Duration::from_secs(3)).await;
    expected.extend(transfer_coins(&mut cluster.validator, &[300, 400]).await);

    // Every match must arrive (no gap), and none twice (no duplicate). A gap hangs to timeout; a
    // duplicate trips the insert assertion.
    let mut seen: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
    while seen.len() < expected.len() {
        let item = stream
            .next()
            .await
            .expect("stream ended before all matches");
        for digest in transaction_digest(&item) {
            if expected.contains(digest) {
                assert!(
                    seen.insert(digest.to_string()),
                    "transaction {digest} delivered more than once across the handoff",
                );
            }
        }
    }
    assert_eq!(
        seen, expected,
        "did not observe exactly the produced matches"
    );
}

/// Resume-by-cursor: the opaque cursor a backfilled edge carries can seed a new subscription via
/// `after`, which resumes strictly past that transaction (no re-delivery of the already-seen tx).
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
    tokio::time::sleep(Duration::from_secs(5)).await;

    // Subscription 1: backfill from `afterCheckpoint`. The first edge yielded has the lowest
    // sequence number; capture its cursor and digest.
    let query = format!(
        r#"subscription($sender: SuiAddress!) {{
            transactions(afterCheckpoint: {resume_from}, filter: {{ sentAddress: $sender }}) {{
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

/// Live/backfill parity: the same transactions must resolve identically whether delivered live
/// (`matching_edges`) or through the backfill scan (`build_scanned_edge`), across a variety of
/// object-change shapes (created, mutated, wrapped, deleted).
#[tokio::test]
async fn test_transaction_subscription_live_backfill_parity() {
    let mut cluster = SubscriptionTestCluster::new_with_ledger_history().await;
    let sender = cluster.validator.wallet.active_address().unwrap();
    let package_id = publish(&mut cluster.validator).await;

    // 1. Start live.
    let mut live = cluster
        .subscribe_with_variables(&tx_query(None), sender_var(sender))
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
    tokio::time::sleep(Duration::from_secs(5)).await;
    let mut backfill = cluster
        .subscribe_with_variables(&tx_query(Some(resume_from)), sender_var(sender))
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

/// Live gap recovery: force an upstream blackout mid-stream, execute several matching transactions
/// during the gap, then restore the connection and assert every one is delivered exactly once, in
/// strictly increasing cursor order. Mirrors the checkpoint/event recovery tests for transactions.
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

    // Execute matches across several checkpoints the server can't see live.
    let mut expected: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
    for _ in 0..3 {
        expected.extend(transfer_coins(&mut cluster.validator, &[100, 200]).await);
        tokio::time::sleep(Duration::from_secs(1)).await;
    }

    // Resume: recovery delivers every missed match, exactly once, in cursor order.
    proxy.allow_connections();

    let mut seen: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
    let mut prev: Option<(u64, u64)> = None;
    while !expected.is_subset(&seen) {
        let item = stream
            .next()
            .await
            .expect("stream ended before recovery completed");
        let edge = &item["data"]["transactions"];
        let Some(digest) = edge["node"]["digest"].as_str().map(str::to_string) else {
            continue;
        };
        let cursor = decode_tx_cursor(edge);
        if let Some(p) = prev {
            assert!(
                cursor > p,
                "cursors not strictly increasing across recovery"
            );
        }
        prev = Some(cursor);
        if expected.contains(&digest) {
            assert!(
                seen.insert(digest.clone()),
                "duplicate delivery of {digest}"
            );
        }
    }
    assert!(
        expected.is_subset(&seen),
        "did not recover exactly the missed transactions",
    );
}

/// Concurrency stress: backfill a batch of matches larger than one scan page / resolution window and
/// assert every one is delivered exactly once, in strictly increasing cursor order, with its content
/// resolved correctly. Exercises the ordered `buffered` resolution and `KvLoader` coalescing under
/// real volume (concurrent per-payload content reads must not cross-contaminate or drop matches).
#[tokio::test]
async fn test_transaction_subscription_high_volume_concurrent_backfill() {
    let mut cluster = SubscriptionTestCluster::new_with_ledger_history().await;
    let sender = cluster.validator.wallet.active_address().unwrap();

    let resume_from = cluster.validator_checkpoint_tip();
    // 26 * 4 = 104 matches: exceeds the default resolve concurrency (100) and one scan page, so the
    // backfill spans multiple scans and resolution windows.
    let mut expected: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
    for _ in 0..26 {
        expected.extend(transfer_coins(&mut cluster.validator, &[100, 200, 300, 400]).await);
    }
    // Advance so they are delivered by the backfill scan, not live.
    tokio::time::sleep(Duration::from_secs(5)).await;

    let query = format!(
        r#"subscription($sender: SuiAddress!) {{
            transactions(afterCheckpoint: {resume_from}, filter: {{ sentAddress: $sender }}) {{
                cursor
                node {{ digest sender {{ address }} effects {{ status }} }}
            }}
        }}"#,
    );
    let mut stream = cluster
        .subscribe_with_variables(&query, sender_var(sender))
        .await;

    let sender_addr = sender.to_string();
    let mut seen: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
    let mut prev: Option<(u64, u64)> = None;
    while !expected.is_subset(&seen) {
        let item = stream
            .next()
            .await
            .expect("stream ended before all matches delivered");
        let edge = &item["data"]["transactions"];
        let digest = edge["node"]["digest"]
            .as_str()
            .expect("missing digest")
            .to_string();
        // Each concurrently-resolved payload must carry its own correct content.
        assert_eq!(
            edge["node"]["effects"]["status"].as_str(),
            Some("SUCCESS"),
            "content (effects) not resolved for {digest}",
        );
        assert_eq!(
            edge["node"]["sender"]["address"].as_str(),
            Some(sender_addr.as_str()),
            "wrong sender resolved for {digest} (concurrent cross-contamination?)",
        );
        let cursor = decode_tx_cursor(edge);
        if let Some(p) = prev {
            assert!(
                cursor > p,
                "cursors not strictly increasing: {p:?} then {cursor:?}",
            );
        }
        prev = Some(cursor);
        if expected.contains(&digest) {
            assert!(
                seen.insert(digest.clone()),
                "duplicate delivery of {digest}"
            );
        }
    }
    assert!(
        expected.is_subset(&seen),
        "did not deliver exactly the executed matches",
    );
}

/// KvLoader coalescing: a backfill of many matches resolved concurrently issues far fewer ledger
/// `BatchGetTransactions` round trips than there are transactions, because the DataLoader batches
/// the concurrent window's content reads. Serial resolution would issue one read per transaction.
#[tokio::test]
async fn test_transaction_subscription_kvloader_coalesces_reads() {
    let mut cluster = SubscriptionTestCluster::new_with_ledger_history().await;
    let sender = cluster.validator.wallet.active_address().unwrap();

    let resume_from = cluster.validator_checkpoint_tip();
    let mut expected: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
    for _ in 0..12 {
        expected.extend(transfer_coins(&mut cluster.validator, &[100, 200, 300, 400]).await);
    }
    let total = expected.len();
    // Advance so the matches are delivered by the backfill scan, not live.
    tokio::time::sleep(Duration::from_secs(5)).await;

    let before = cluster.ledger_grpc_call_count("BatchGetTransactions");

    // Querying a content field (effects) makes each payload trigger a KvLoader content read.
    let query = format!(
        r#"subscription($sender: SuiAddress!) {{
            transactions(afterCheckpoint: {resume_from}, filter: {{ sentAddress: $sender }}) {{
                node {{ digest effects {{ status }} }}
            }}
        }}"#,
    );
    let mut stream = cluster
        .subscribe_with_variables(&query, sender_var(sender))
        .await;

    let mut seen: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
    while !expected.is_subset(&seen) {
        let item = stream
            .next()
            .await
            .expect("stream ended before all matches delivered");
        if let Some(d) = item["data"]["transactions"]["node"]["digest"].as_str()
            && expected.contains(d)
        {
            seen.insert(d.to_string());
        }
    }

    let reads = cluster.ledger_grpc_call_count("BatchGetTransactions") - before;
    assert!(
        reads > 0,
        "expected some BatchGetTransactions content reads; the metric may not be wired",
    );
    assert!(
        reads < total as u64,
        "expected coalesced content reads (< {total}) but got {reads} for {total} transactions; \
         the DataLoader is not batching under concurrent resolution",
    );
}

/// A malformed resume cursor is surfaced as a GraphQL error to the client, not a hang or a silent
/// empty stream. Exercises the resolver's input-validation failure path over SSE.
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

    let item = stream
        .next()
        .await
        .expect("expected an error payload, got end of stream");
    assert!(
        item.get("errors").is_some(),
        "invalid cursor should surface a GraphQL error, got: {item}",
    );
}

/// Backfill round-trip / throughput benchmark. Seeds a large matching history, then for serial
/// (concurrency 1) vs concurrent resolution measures wall-clock delivery time and counts backend
/// round trips: scan `ListTransactions` and content `BatchGetTransactions`. The content reads should
/// collapse dramatically under concurrency (DataLoader coalescing). Ignored by default; run with:
///
/// ```text
/// cargo nextest run -p sui-indexer-alt-e2e-tests --features staging \
///     bench_transaction_subscription_backfill --run-ignored all --no-capture
/// ```
#[tokio::test]
#[ignore = "benchmark; run explicitly with --run-ignored"]
async fn bench_transaction_subscription_backfill() {
    for concurrency in [1usize, 100] {
        let mut cluster =
            SubscriptionTestCluster::new_with_ledger_history_and_concurrency(concurrency).await;
        let sender = cluster.validator.wallet.active_address().unwrap();
        let resume_from = cluster.validator_checkpoint_tip();

        // ~200 matches across ~50 checkpoints, so the backfill pages several times.
        let mut expected: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
        for _ in 0..50 {
            expected.extend(transfer_coins(&mut cluster.validator, &[100, 200, 300, 400]).await);
        }
        let total = expected.len();
        tokio::time::sleep(Duration::from_secs(5)).await;

        let scan_before = cluster.ledger_grpc_call_count("ListTransactions");
        let content_before = cluster.ledger_grpc_call_count("BatchGetTransactions");

        let query = format!(
            r#"subscription($sender: SuiAddress!) {{
                transactions(afterCheckpoint: {resume_from}, filter: {{ sentAddress: $sender }}) {{
                    node {{ digest effects {{ status }} }}
                }}
            }}"#,
        );
        let mut stream = cluster
            .subscribe_with_variables(&query, sender_var(sender))
            .await;

        let start = std::time::Instant::now();
        let mut seen: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
        while !expected.is_subset(&seen) {
            let item = stream
                .next()
                .await
                .expect("stream ended before all matches delivered");
            if let Some(d) = item["data"]["transactions"]["node"]["digest"].as_str()
                && expected.contains(d)
            {
                seen.insert(d.to_string());
            }
        }
        let elapsed = start.elapsed();

        let scan = cluster.ledger_grpc_call_count("ListTransactions") - scan_before;
        let content = cluster.ledger_grpc_call_count("BatchGetTransactions") - content_before;
        eprintln!(
            "[bench] concurrency={concurrency:>3} txs={total} elapsed={elapsed:?} \
             scan_rts={scan} content_rts={content} total_rts={}",
            scan + content,
        );
    }
}

/// Stand up the real streaming GraphQL server and keep it alive for manual queries. Seeds matching
/// history, prints the subscription endpoint plus a ready-to-run curl, then sleeps. Ignored by
/// default; run with `SUB_SERVER_SECS=900 ... serve_transaction_subscription --run-ignored all --no-capture`.
#[tokio::test]
#[ignore = "manual server; run explicitly with --run-ignored"]
async fn serve_transaction_subscription() {
    let mut cluster = SubscriptionTestCluster::new_with_ledger_history().await;
    let sender = cluster.validator.wallet.active_address().unwrap();
    let resume_from = cluster.validator_checkpoint_tip();

    for _ in 0..50 {
        transfer_coins(&mut cluster.validator, &[1_000_000; 4]).await;
    }
    tokio::time::sleep(Duration::from_secs(5)).await;
    let tip = cluster.validator_checkpoint_tip();

    let example = format!(
        "subscription {{\n  transactions(afterCheckpoint: {resume_from}, filter: {{ sentAddress: \"{sender}\" }}) {{\n    cursor\n    node {{ digest effects {{ status }} }}\n  }}\n}}"
    );
    let body = serde_json::json!({ "query": example }).to_string();
    eprintln!("\n===================== streaming graphql server =====================");
    eprintln!("subscription endpoint : {}", cluster.subscription_url);
    eprintln!("sender                : {sender}");
    eprintln!("resume checkpoint     : {resume_from}   (tip now ~{tip})");
    eprintln!(
        "\ncurl (SSE):\n  curl -N -X POST {} -H 'Accept: text/event-stream' -H 'Content-Type: application/json' -d '{}'",
        cluster.subscription_url, body,
    );
    eprintln!("====================================================================\n");

    let secs = std::env::var("SUB_SERVER_SECS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(600);
    eprintln!("server alive for {secs}s (set SUB_SERVER_SECS to change)...");
    tokio::time::sleep(Duration::from_secs(secs)).await;
}

/// Object-read coalescing: backfilling many matches while querying `objectChanges` (which hydrate
/// output objects) issues far fewer ledger `BatchGetObjects` round trips than there are matches,
/// because concurrent resolution coalesces the object reads the same way it coalesces content reads.
#[tokio::test]
async fn test_transaction_subscription_object_reads_coalesce() {
    let mut cluster = SubscriptionTestCluster::new_with_ledger_history().await;
    let sender = cluster.validator.wallet.active_address().unwrap();
    let resume_from = cluster.validator_checkpoint_tip();

    let mut expected: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
    for _ in 0..12 {
        expected.extend(transfer_coins(&mut cluster.validator, &[100, 200, 300, 400]).await);
    }
    let total = expected.len();
    tokio::time::sleep(Duration::from_secs(5)).await;

    let before = cluster.ledger_grpc_call_count("BatchGetObjects");

    let query = format!(
        r#"subscription($sender: SuiAddress!) {{
            transactions(afterCheckpoint: {resume_from}, filter: {{ sentAddress: $sender }}) {{
                node {{
                    digest
                    effects {{
                        objectChanges {{
                            nodes {{ outputState {{ asMoveObject {{ contents {{ type {{ repr }} }} }} }} }}
                        }}
                    }}
                }}
            }}
        }}"#,
    );
    let mut stream = cluster
        .subscribe_with_variables(&query, sender_var(sender))
        .await;

    let mut seen: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
    while !expected.is_subset(&seen) {
        let item = stream
            .next()
            .await
            .expect("stream ended before all matches delivered");
        let node = &item["data"]["transactions"]["node"];
        if let Some(d) = node["digest"].as_str()
            && expected.contains(d)
        {
            // Object changes hydrated (a transfer has at least the transferred + gas coin).
            let resolved = node["effects"]["objectChanges"]["nodes"]
                .as_array()
                .is_some_and(|n| !n.is_empty());
            assert!(resolved, "objectChanges not resolved for {d}");
            seen.insert(d.to_string());
        }
    }

    let reads = cluster.ledger_grpc_call_count("BatchGetObjects") - before;
    assert!(
        reads > 0,
        "expected some BatchGetObjects reads; the metric may not be wired",
    );
    assert!(
        reads < total as u64,
        "expected coalesced object reads (< {total}) but got {reads} for {total} transactions",
    );
}
