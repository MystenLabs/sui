// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use fastcrypto::encoding::Base64;
use fastcrypto::encoding::Encoding;
use serde::Deserialize;
use serde_json::json;

use sui_indexer_alt_e2e_tests::FullCluster;
use sui_indexer_alt_e2e_tests::graphql;
use sui_indexer_alt_e2e_tests::transaction::DEFAULT_GAS_BUDGET;
use sui_indexer_alt_e2e_tests::transaction::send_sui;
use sui_rpc_cursor::CursorToken;
use sui_rpc_cursor::Position;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Checkpoint {
    sequence_number: u64,
}

#[derive(Debug, Deserialize)]
struct Effects {
    checkpoint: Option<Checkpoint>,
}

#[derive(Debug, Deserialize)]
struct TxNode {
    effects: Option<Effects>,
}

/// Query an epoch's `transactions` connection. Returns `None` if the epoch does not exist.
async fn epoch_transactions(
    cluster: &FullCluster,
    epoch_id: u64,
    first: Option<u32>,
    last: Option<u32>,
    after: Option<String>,
    before: Option<String>,
) -> anyhow::Result<Option<graphql::Connection<TxNode>>> {
    let query = r#"query($epochId: UInt53, $first: Int, $last: Int, $after: String, $before: String) {
            epoch(epochId: $epochId) {
                transactions(first: $first, last: $last, after: $after, before: $before) {
                    pageInfo { hasNextPage hasPreviousPage }
                    edges { cursor node { effects { checkpoint { sequenceNumber } } } }
                }
            }
        }"#;

    let data = graphql::query(
        cluster,
        query,
        json!({ "epochId": epoch_id, "first": first, "last": last, "after": after, "before": before }),
    )
    .await?;

    let epoch = &data["epoch"];
    if epoch.is_null() {
        return Ok(None);
    }
    Ok(Some(serde_json::from_value(epoch["transactions"].clone())?))
}

/// Each edge's cursor paired with the checkpoint its transaction landed in — enough to identify the
/// transaction exactly while also exercising the `effects.checkpoint` resolution.
fn window(edges: &[graphql::Edge<TxNode>]) -> Vec<(String, Option<u64>)> {
    edges
        .iter()
        .map(|e| {
            let checkpoint = e
                .node
                .effects
                .as_ref()
                .and_then(|f| f.checkpoint.as_ref())
                .map(|c| c.sequence_number);
            (e.cursor.clone(), checkpoint)
        })
        .collect()
}

#[tokio::test]
async fn test_epoch_transactions_cursor_pagination() {
    let mut cluster = FullCluster::new().await.unwrap();

    // Move into epoch 1, the epoch under test.
    cluster.advance_epoch();

    // Fund A, sealing the faucet transaction in its own epoch-1 checkpoint.
    let (a, kp, mut gas) = cluster
        .funded_account(DEFAULT_GAS_BUDGET * 40)
        .expect("Failed to fund account");
    cluster.create_checkpoint().await;

    // Two batches of transactions in separate checkpoints, so epoch 1's transactions span more than
    // one checkpoint.
    for amount in [42u64, 43, 1] {
        gas = send_sui(&mut cluster, a, &kp, gas, amount).0;
    }
    cluster.create_checkpoint().await;
    for amount in [2u64, 3, 4] {
        gas = send_sui(&mut cluster, a, &kp, gas, amount).0;
    }
    cluster.create_checkpoint().await;

    // Close epoch 1 and sync its final checkpoint into the off-chain services.
    cluster.advance_epoch();
    cluster.create_checkpoint().await;

    // Ground truth: every transaction in epoch 1, in order, with its cursor and checkpoint.
    let all = epoch_transactions(&cluster, 1, Some(50), None, None, None)
        .await
        .unwrap()
        .expect("Epoch 1 should exist");
    let n = all.edges.len();
    assert!(
        n >= 7,
        "expected enough epoch-1 transactions to paginate, got {n}",
    );

    // `lo`/`hi` are interior bounds with at least three transactions between them, mirroring the
    // `after: 4, before: 8` shape of the original test.
    let lo = 1;
    let hi = 5;
    let after = all.edges[lo].cursor.clone();
    let before = all.edges[hi].cursor.clone();

    // after `lo`, before `hi`, picking from the front.
    let page = epoch_transactions(
        &cluster,
        1,
        Some(2),
        None,
        Some(after.clone()),
        Some(before.clone()),
    )
    .await
    .unwrap()
    .unwrap();
    assert_eq!(window(&page.edges), window(&all.edges[lo + 1..lo + 3]));
    assert!(page.page_info.has_previous_page);
    assert!(page.page_info.has_next_page);

    // after `lo`, before `hi`, picking from the back.
    let page = epoch_transactions(
        &cluster,
        1,
        None,
        Some(2),
        Some(after.clone()),
        Some(before.clone()),
    )
    .await
    .unwrap()
    .unwrap();
    assert_eq!(window(&page.edges), window(&all.edges[hi - 2..hi]));
    assert!(page.page_info.has_previous_page);
    assert!(page.page_info.has_next_page);

    // after `lo`, picking from the front.
    let page = epoch_transactions(&cluster, 1, Some(2), None, Some(after.clone()), None)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(window(&page.edges), window(&all.edges[lo + 1..lo + 3]));
    assert!(page.page_info.has_previous_page);
    assert!(page.page_info.has_next_page);

    // after `lo`, picking from the back (the epoch's last two transactions).
    let page = epoch_transactions(&cluster, 1, None, Some(2), Some(after.clone()), None)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(window(&page.edges), window(&all.edges[n - 2..n]));
    assert!(page.page_info.has_previous_page);

    // before `hi`, picking from the back.
    let page = epoch_transactions(&cluster, 1, None, Some(2), None, Some(before.clone()))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(window(&page.edges), window(&all.edges[hi - 2..hi]));

    let beyond = CursorToken::item(Position::Transactions {
        checkpoint: 0,
        tx_seq: 100,
    });
    let beyond_cursor = Base64::encode(beyond.encode());
    let page = epoch_transactions(&cluster, 2, None, Some(2), Some(beyond_cursor), None)
        .await
        .unwrap()
        .unwrap();
    assert!(page.edges.is_empty());
    assert!(!page.page_info.has_next_page);
}
