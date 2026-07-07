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
use sui_rpc_cursor::QueryType;

const TX_QUERY: &str = r#"
query($first: Int, $last: Int, $after: String, $before: String) {
    transactions(first: $first, last: $last, after: $after, before: $before) {
        pageInfo { hasNextPage hasPreviousPage }
        edges { cursor node { effects { checkpoint { sequenceNumber } } } }
    }
}
"#;

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

/// Query the top-level `transactions` connection.
async fn transactions(
    cluster: &FullCluster,
    first: Option<u32>,
    last: Option<u32>,
    after: Option<String>,
    before: Option<String>,
) -> anyhow::Result<graphql::Connection<TxNode>> {
    let data = graphql::query(
        cluster,
        TX_QUERY,
        json!({ "first": first, "last": last, "after": after, "before": before }),
    )
    .await?;

    Ok(serde_json::from_value(data["transactions"].clone())?)
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
async fn test_transactions_query_cursor_pagination() {
    let mut cluster = FullCluster::new().await.unwrap();

    let (a, kp, mut gas) = cluster
        .funded_account(DEFAULT_GAS_BUDGET * 40)
        .expect("Failed to fund account");
    cluster.create_checkpoint().await;

    // Transactions spread across several checkpoints and epochs, so the top-level connection spans
    // checkpoint and epoch boundaries.
    for amount in [10u64, 11] {
        gas = send_sui(&mut cluster, a, &kp, gas, amount).0;
    }
    cluster.create_checkpoint().await;
    cluster.advance_epoch();
    cluster.create_checkpoint().await;
    for amount in [12u64, 13] {
        gas = send_sui(&mut cluster, a, &kp, gas, amount).0;
    }
    cluster.create_checkpoint().await;
    cluster.advance_epoch();
    cluster.create_checkpoint().await;
    for amount in [14u64, 15] {
        gas = send_sui(&mut cluster, a, &kp, gas, amount).0;
    }
    cluster.create_checkpoint().await;

    // Ground truth: the whole transaction list, in order, with each transaction's checkpoint.
    let all = transactions(&cluster, Some(50), None, None, None)
        .await
        .unwrap();
    let n = all.edges.len();
    assert!(n >= 8, "expected enough transactions to paginate, got {n}");

    // Before the 4th transaction, take the first five: only the three before it exist, and there is
    // nothing before the very first transaction.
    let page = transactions(
        &cluster,
        Some(5),
        None,
        None,
        Some(all.edges[3].cursor.clone()),
    )
    .await
    .unwrap();
    assert_eq!(window(&page.edges), window(&all.edges[0..3]));
    assert!(!page.page_info.has_previous_page);
    assert!(page.page_info.has_next_page);

    // After the first, before the 5th, picking two from the front.
    let page = transactions(
        &cluster,
        Some(2),
        None,
        Some(all.edges[0].cursor.clone()),
        Some(all.edges[4].cursor.clone()),
    )
    .await
    .unwrap();
    assert_eq!(window(&page.edges), window(&all.edges[1..3]));
    assert!(page.page_info.has_previous_page);
    assert!(page.page_info.has_next_page);

    // After the first, before the 7th, picking two from the back.
    let page = transactions(
        &cluster,
        None,
        Some(2),
        Some(all.edges[0].cursor.clone()),
        Some(all.edges[6].cursor.clone()),
    )
    .await
    .unwrap();
    assert_eq!(window(&page.edges), window(&all.edges[4..6]));
    assert!(page.page_info.has_previous_page);
    assert!(page.page_info.has_next_page);

    // After the first, picking two from the back: the connection's final two transactions.
    let page = transactions(
        &cluster,
        None,
        Some(2),
        Some(all.edges[0].cursor.clone()),
        None,
    )
    .await
    .unwrap();
    assert_eq!(window(&page.edges), window(&all.edges[n - 2..n]));
    assert!(page.page_info.has_previous_page);
    assert!(!page.page_info.has_next_page);

    let beyond = CursorToken::item(QueryType::Transactions, 0, 100);
    let beyond_cursor = Base64::encode(beyond.encode());
    let page = transactions(&cluster, Some(2), None, Some(beyond_cursor), None)
        .await
        .unwrap();
    assert!(page.edges.is_empty());
    assert!(!page.page_info.has_next_page);

    // Legacy cursor support -- the service accepts the old Base64-encoded JSON transaction
    // sequence number format, but outputs the new format only.
    let after = Base64::encode(serde_json::to_vec(&0u64).unwrap());
    let before = Base64::encode(serde_json::to_vec(&6u64).unwrap());

    let page = transactions(&cluster, None, Some(2), Some(after), Some(before))
        .await
        .unwrap();

    assert_eq!(window(&page.edges), window(&all.edges[4..=5]));
    assert!(page.page_info.has_previous_page);
    assert!(page.page_info.has_next_page);
}
