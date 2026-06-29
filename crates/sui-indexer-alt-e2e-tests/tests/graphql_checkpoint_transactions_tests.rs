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

#[derive(Debug, Deserialize)]
struct Sender {
    address: Option<String>,
}

#[derive(Debug, Deserialize)]
struct TxNode {
    digest: String,
    sender: Option<Sender>,
}

/// Query a checkpoint's `transactions` connection. Returns `None` if the checkpoint does not exist.
async fn transactions(
    cluster: &FullCluster,
    seq: u64,
    first: Option<u32>,
    last: Option<u32>,
    after: Option<String>,
    before: Option<String>,
) -> anyhow::Result<Option<graphql::Connection<TxNode>>> {
    let query = format!(
        r#"query($first: Int, $last: Int, $after: String, $before: String) {{
            checkpoint(sequenceNumber: {seq}) {{
                transactions(first: $first, last: $last, after: $after, before: $before) {{
                    pageInfo {{ hasNextPage hasPreviousPage }}
                    edges {{ cursor node {{ digest sender {{ address }} }} }}
                }}
            }}
        }}"#
    );

    let data = graphql::query(
        cluster,
        &query,
        json!({ "first": first, "last": last, "after": after, "before": before }),
    )
    .await?;

    let checkpoint = &data["checkpoint"];
    if checkpoint.is_null() {
        return Ok(None);
    }
    Ok(Some(serde_json::from_value(
        checkpoint["transactions"].clone(),
    )?))
}

fn sender_addr(edge: &graphql::Edge<TxNode>) -> Option<&str> {
    edge.node.sender.as_ref()?.address.as_deref()
}

fn digests(conn: &graphql::Connection<TxNode>) -> Vec<String> {
    conn.edges.iter().map(|e| e.node.digest.clone()).collect()
}

/// Test cursor pagination using cursors from each Transaction edge.
#[tokio::test]
async fn test_checkpoint_transactions_cursor_pagination() {
    let mut cluster = FullCluster::new().await.unwrap();

    // Fund A and seal the funding in its own checkpoint, so the checkpoint under test holds only
    // A's transactions (plus the checkpoint's system transaction).
    let (a, kp, mut gas) = cluster
        .funded_account(DEFAULT_GAS_BUDGET * 20)
        .expect("Failed to fund account");
    cluster.create_checkpoint().await;

    // Five transactions, all sent by A, in the checkpoint under test.
    let mut a_digests = Vec::new();
    for amount in [42u64, 43, 1, 2, 3] {
        let (next_gas, digest) = send_sui(&mut cluster, a, &kp, gas, amount);
        gas = next_gas;
        a_digests.push(digest.to_string());
    }
    let target = cluster.create_checkpoint().await.sequence_number;

    let a_addr = a.to_string();

    // Capture A's transaction cursors, in order, from the checkpoint's connection. These drive the
    // offset cases below, in place of the synthetic `bcs(...)` cursors of the transactional test.
    let all = transactions(&cluster, target, Some(10), None, None, None)
        .await
        .unwrap()
        .expect("Checkpoint under test should exist");
    let a_edges: Vec<&graphql::Edge<TxNode>> = all
        .edges
        .iter()
        .filter(|e| sender_addr(e) == Some(a_addr.as_str()))
        .collect();
    assert_eq!(
        a_edges
            .iter()
            .map(|e| e.node.digest.clone())
            .collect::<Vec<_>>(),
        a_digests,
    );
    let a_cursors: Vec<String> = a_edges.iter().map(|e| e.cursor.clone()).collect();

    // Offset at the front: after A's first transaction, take the next three.
    let page = transactions(
        &cluster,
        target,
        Some(3),
        None,
        Some(a_cursors[0].clone()),
        None,
    )
    .await
    .unwrap()
    .unwrap();
    assert_eq!(digests(&page), a_digests[1..4].to_vec());
    assert!(page.page_info.has_previous_page);
    assert!(page.page_info.has_next_page);

    // Offset from both ends, picking from the front.
    let page = transactions(
        &cluster,
        target,
        Some(2),
        None,
        Some(a_cursors[0].clone()),
        Some(a_cursors[4].clone()),
    )
    .await
    .unwrap()
    .unwrap();
    assert_eq!(digests(&page), a_digests[1..3].to_vec());
    assert!(page.page_info.has_previous_page);
    assert!(page.page_info.has_next_page);

    // Offset from both ends, picking from the back.
    let page = transactions(
        &cluster,
        target,
        None,
        Some(2),
        Some(a_cursors[0].clone()),
        Some(a_cursors[4].clone()),
    )
    .await
    .unwrap()
    .unwrap();
    assert_eq!(digests(&page), a_digests[2..4].to_vec());
    assert!(page.page_info.has_previous_page);
    assert!(page.page_info.has_next_page);

    // Offset from the end, picking from the back.
    let page = transactions(
        &cluster,
        target,
        None,
        Some(2),
        None,
        Some(a_cursors[2].clone()),
    )
    .await
    .unwrap()
    .unwrap();
    assert_eq!(digests(&page), a_digests[0..2].to_vec());
    assert!(page.page_info.has_next_page);

    let beyond = CursorToken::item(QueryType::Transactions, 0, 100);
    let beyond_cursor = Base64::encode(beyond.encode());
    let page = transactions(&cluster, target, None, None, Some(beyond_cursor), None)
        .await
        .unwrap()
        .unwrap();
    assert!(page.edges.is_empty());
    assert!(!page.page_info.has_next_page);
}
