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

/// Decode a transaction edge's `cursor` field into its `(checkpoint, tx_sequence)` position.
fn decode_tx_cursor(item: &serde_json::Value) -> (u64, u64) {
    let cursor = item["data"]["transactions"]["cursor"]
        .as_str()
        .expect("transaction edge missing cursor");
    let bytes = CTransaction::decode_cursor(cursor).expect("cursor is not a valid CTransaction");
    let token = CursorToken::decode(&bytes).expect("cursor is not a valid CursorToken");
    (token.checkpoint, token.position)
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

    // Consume edges in arrival order; each must carry a strictly larger cursor than the last.
    let mut seen = std::collections::BTreeSet::new();
    let mut prev_cursor: Option<(u64, u64)> = None;
    while seen.len() < expected.len() {
        let item = stream.next().await.expect("stream ended before all txs");
        let digest = item["data"]["transactions"]["node"]["digest"]
            .as_str()
            .expect("edge missing digest")
            .to_string();
        if !expected.contains(&digest) {
            continue;
        }
        let cursor = decode_tx_cursor(&item);
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
    let first_digest = first_item["data"]["transactions"]["node"]["digest"]
        .as_str()
        .expect("edge missing digest")
        .to_string();
    let after = first_item["data"]["transactions"]["cursor"]
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
    let got = item["data"]["transactions"]["node"]["digest"]
        .as_str()
        .unwrap();
    assert_ne!(
        got, first_digest,
        "resume-by-cursor re-delivered the tx at the cursor",
    );
}

/// Live/backfill parity: the same transactions must resolve identically whether delivered live
/// (`matching_edges`) or through the backfill scan (`build_scanned_edge`). The snapshot doubles as
/// the field-correctness check across a variety of object-change shapes.
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

    // 5. Identical across phases (parity), and matches the recorded shape (correctness).
    assert_eq!(
        live_nodes, backfill_nodes,
        "live and backfill resolved the same transactions differently",
    );
    rich_tx_redactions().bind(|| {
        insta::assert_json_snapshot!("transaction_subscription_parity", live_nodes);
    });
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
/// any non-expected txs) so the comparison and snapshot are stable.
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

/// `graphql_redactions` plus the volatile leaves of the rich transaction node, so a full-node
/// snapshot is stable across runs.
fn rich_tx_redactions() -> insta::Settings {
    let mut settings = graphql_redactions();
    settings.add_redaction(".**.signatureBytes", "[signature]");
    settings.add_redaction(".**.lamportVersion", "[lamportVersion]");
    settings.add_redaction(".**.gasSummary", "[gasSummary]");
    settings.add_redaction(".**.json", "[json]");
    settings
}
