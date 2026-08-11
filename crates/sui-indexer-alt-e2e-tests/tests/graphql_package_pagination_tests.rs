// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! End-to-end tests for `Query.packages` served from ledger history — the gRPC path, active when
//! the kv-rpc List APIs are enabled. Publishes real packages across checkpoints, then paginates
//! forwards and backwards, resuming from the cursors the connection mints.

use std::path::PathBuf;
use std::time::Duration;

use serde::Deserialize;
use serde_json::json;
use simulacrum::Simulacrum;
use sui_indexer_alt_e2e_tests::FullCluster;
use sui_indexer_alt_e2e_tests::OffchainClusterConfig;
use sui_indexer_alt_e2e_tests::graphql;
use sui_indexer_alt_e2e_tests::transaction::DEFAULT_GAS_BUDGET;
use sui_kv_rpc::KvRpcConfig;
use sui_test_transaction_builder::TestTransactionBuilder;
use sui_types::base_types::ObjectID;
use sui_types::base_types::ObjectRef;
use sui_types::base_types::SuiAddress;
use sui_types::crypto::AccountKeyPair;
use sui_types::effects::TransactionEffectsAPI;
use sui_types::object::Owner;
use sui_types::transaction::Transaction;

const PACKAGES_QUERY: &str = r#"
query($first: Int, $last: Int, $after: String, $before: String) {
    packages(
        first: $first,
        last: $last,
        after: $after,
        before: $before,
        filter: { afterCheckpoint: 0 },
    ) {
        pageInfo { hasNextPage hasPreviousPage startCursor endCursor }
        edges { cursor node { address version } }
    }
}
"#;

#[derive(Debug, Deserialize)]
struct PkgNode {
    address: String,
    version: u64,
}

/// A cluster whose kv-rpc server serves the List APIs, which routes `Query.packages` through the
/// ledger-history scan instead of the `kv_packages` table.
async fn list_api_cluster() -> FullCluster {
    FullCluster::new_with_configs(
        Simulacrum::new(),
        OffchainClusterConfig {
            kv_rpc_config: KvRpcConfig {
                enable_list_apis: Some(true),
                ..Default::default()
            },
            ..Default::default()
        },
        &prometheus::Registry::new(),
    )
    .await
    .expect("Failed to create cluster")
}

/// Query the top-level `packages` connection, filtered to post-genesis checkpoints so the
/// framework packages published at genesis stay out of the expected windows.
async fn packages(
    cluster: &FullCluster,
    first: Option<u32>,
    after: Option<String>,
    last: Option<u32>,
    before: Option<String>,
) -> anyhow::Result<graphql::Connection<PkgNode>> {
    let data = graphql::query(
        &cluster.graphql_url(),
        PACKAGES_QUERY,
        json!({ "first": first, "last": last, "after": after, "before": before }),
    )
    .await?;

    Ok(serde_json::from_value(data["packages"].clone())?)
}

/// Query `packages(first: ...)` until the pipelines catch up and it returns `expected` edges.
async fn packages_eventually(
    cluster: &FullCluster,
    first: u32,
    expected: usize,
) -> graphql::Connection<PkgNode> {
    tokio::time::timeout(Duration::from_secs(30), async {
        loop {
            if let Ok(conn) = packages(cluster, Some(first), None, None, None).await
                && conn.edges.len() == expected
            {
                return conn;
            }
            tokio::time::sleep(Duration::from_millis(200)).await;
        }
    })
    .await
    .expect("timed out waiting for packages to be served")
}

/// Publish the fixture package and return `(package_id, updated_gas_ref)`.
async fn publish(
    cluster: &mut FullCluster,
    sender: SuiAddress,
    kp: &AccountKeyPair,
    gas: ObjectRef,
) -> (ObjectID, ObjectRef) {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("packages/event/emit_test_event");
    let (fx, _) = cluster
        .execute_transaction(Transaction::from_data_and_signer(
            TestTransactionBuilder::new(sender, gas, cluster.reference_gas_price())
                .with_gas_budget(DEFAULT_GAS_BUDGET)
                .publish(path)
                .build(),
            vec![kp],
        ))
        .expect("publish failed");

    let pkg_id = fx
        .created()
        .into_iter()
        .find_map(|((id, v, _), owner)| {
            (v.value() == 1 && matches!(owner, Owner::Immutable)).then_some(id)
        })
        .expect("package id");

    let new_gas = fx
        .mutated()
        .into_iter()
        .find(|((id, _, _), _)| *id == gas.0)
        .map(|((id, version, digest), _)| (id, version, digest))
        .expect("gas mutated");

    (pkg_id, new_gas)
}

/// The canonical addresses of a page's edges.
fn addresses(conn: &graphql::Connection<PkgNode>) -> Vec<String> {
    conn.edges.iter().map(|e| e.node.address.clone()).collect()
}

fn canonical(id: ObjectID) -> String {
    id.to_canonical_string(/* with_prefix */ true)
}

/// Publish three packages — one in its own checkpoint, two sharing the next — and return the
/// cluster plus package IDs in publish (transaction) order.
async fn cluster_with_three_packages() -> (FullCluster, [ObjectID; 3]) {
    let mut cluster = list_api_cluster().await;

    let (sender, kp, gas) = cluster
        .funded_account(DEFAULT_GAS_BUDGET * 10)
        .expect("Failed to fund account");
    cluster.create_checkpoint().await;

    let (pkg_a, gas) = publish(&mut cluster, sender, &kp, gas).await;
    cluster.create_checkpoint().await;

    let (pkg_b, gas) = publish(&mut cluster, sender, &kp, gas).await;
    let (pkg_c, _) = publish(&mut cluster, sender, &kp, gas).await;
    cluster.create_checkpoint().await;

    (cluster, [pkg_a, pkg_b, pkg_c])
}

#[tokio::test]
async fn forward_pagination_resumes_from_end_cursor() {
    let (cluster, [a, b, c]) = cluster_with_three_packages().await;

    // Full drain establishes the pipelines are caught up and fixes the expected order:
    // transaction order across checkpoints.
    let all = packages_eventually(&cluster, 10, 3).await;
    assert_eq!(addresses(&all), [canonical(a), canonical(b), canonical(c)]);
    assert!(!all.page_info.has_next_page);
    assert!(!all.page_info.has_previous_page);
    assert_eq!(all.edges.iter().map(|e| e.node.version).max(), Some(1));

    let page = packages(&cluster, Some(2), None, None, None)
        .await
        .expect("first page");
    assert_eq!(addresses(&page), [canonical(a), canonical(b)]);
    assert!(page.page_info.has_next_page);
    assert!(!page.page_info.has_previous_page);

    let rest = packages(
        &cluster,
        Some(2),
        page.page_info.end_cursor.clone(),
        None,
        None,
    )
    .await
    .expect("second page");
    assert_eq!(addresses(&rest), [canonical(c)]);
    assert!(rest.page_info.has_previous_page);
    assert!(!rest.page_info.has_next_page);

    // Resuming from the drained connection's end cursor serves nothing further.
    let empty = packages(
        &cluster,
        Some(2),
        all.page_info.end_cursor.clone(),
        None,
        None,
    )
    .await
    .expect("page after the end");
    assert!(empty.edges.is_empty());
    assert!(!empty.page_info.has_next_page);
}

#[tokio::test]
async fn forward_pagination_by_single_steps() {
    let (cluster, [a, b, c]) = cluster_with_three_packages().await;
    packages_eventually(&cluster, 10, 3).await;

    // Walk the connection one package at a time, resuming from each page's end cursor —
    // exercising resumption both at checkpoint boundaries and between two publishes that share
    // a checkpoint (b → c).
    let mut cursor = None;
    let mut walked = vec![];
    loop {
        let page = packages(&cluster, Some(1), cursor.clone(), None, None)
            .await
            .expect("page");
        let Some(edge) = page.edges.first() else {
            break;
        };
        walked.push(edge.node.address.clone());
        cursor = page.page_info.end_cursor.clone();
        if !page.page_info.has_next_page {
            break;
        }
    }

    assert_eq!(walked, [canonical(a), canonical(b), canonical(c)]);
}

#[tokio::test]
async fn backward_pagination_resumes_from_start_cursor() {
    let (cluster, [a, b, c]) = cluster_with_three_packages().await;
    packages_eventually(&cluster, 10, 3).await;

    // A backward page returns the tail, in ascending order.
    let page = packages(&cluster, None, None, Some(2), None)
        .await
        .expect("last page");
    assert_eq!(addresses(&page), [canonical(b), canonical(c)]);
    assert!(page.page_info.has_previous_page);
    assert!(!page.page_info.has_next_page);

    let rest = packages(
        &cluster,
        None,
        None,
        Some(2),
        page.page_info.start_cursor.clone(),
    )
    .await
    .expect("previous page");
    assert_eq!(addresses(&rest), [canonical(a)]);
    assert!(!rest.page_info.has_previous_page);
    assert!(rest.page_info.has_next_page);
}

/// An edge's own cursor is usable in both directions: `after` it resumes with its successors,
/// `before` it with its predecessors.
#[tokio::test]
async fn edge_cursors_are_bidirectional() {
    let (cluster, [a, _b, c]) = cluster_with_three_packages().await;
    let all = packages_eventually(&cluster, 10, 3).await;
    let middle = all.edges[1].cursor.clone();

    let after = packages(&cluster, Some(10), Some(middle.clone()), None, None)
        .await
        .expect("page after middle edge");
    assert_eq!(addresses(&after), [canonical(c)]);

    let before = packages(&cluster, None, None, Some(10), Some(middle))
        .await
        .expect("page before middle edge");
    assert_eq!(addresses(&before), [canonical(a)]);
}
