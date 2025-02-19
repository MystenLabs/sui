// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! These tests check that transaction queries respond correctly to pruning, especially given that
//! the implementation applies a bound based on the reader low watermark that needs to consider the
//! progress of the pruner across multiple tables.

use std::{str::FromStr, time::Duration};

use reqwest::Client;
use serde_json::{json, Value};
use simulacrum::Simulacrum;
use sui_indexer_alt::config::{ConcurrentLayer, IndexerConfig, PipelineLayer, PrunerLayer};
use sui_indexer_alt_e2e_tests::{find_address_owned, FullCluster};
use sui_indexer_alt_framework::IndexerArgs;
use sui_indexer_alt_jsonrpc::{
    config::RpcConfig, data::system_package_task::SystemPackageTaskArgs,
};
use sui_types::{
    base_types::SuiAddress,
    crypto::{get_account_key_pair, Signature, Signer},
    digests::TransactionDigest,
    effects::TransactionEffectsAPI,
    programmable_transaction_builder::ProgrammableTransactionBuilder,
    transaction::{Transaction, TransactionData},
};
use tokio_util::sync::CancellationToken;

/// 5 SUI gas budget
const DEFAULT_GAS_BUDGET: u64 = 5_000_000_000;

// Check that querying transactions by sender works when fetchings transactions all in one go, and
// paginated, in ascending and descending order, for both `a` and `b`.
macro_rules! check_tx_digests {
    ($cluster:expr, $sender:expr, $desc:expr, $expect:expr) => {{
        let cluster = $cluster;
        let desc = $desc;
        let sender = $sender;

        let (all_txs, _) = query_transactions(cluster, sender, None, 100, desc).await;

        let mut next = None;
        let mut paginated = vec![];
        loop {
            let (page, cursor) = query_transactions(cluster, sender, next, 4, desc).await;
            paginated.extend(page);

            next = cursor;
            if next.is_none() {
                break;
            }
        }

        let expect: Vec<_> = $expect.copied().collect();

        assert_eq!(all_txs, expect, "Mismatch fetching all transactions");
        assert_eq!(paginated, expect, "Mismatch fetching paged transactions");
    }};
}

/// Set-up a cluster where the filter (`tx_affected_addresses`) table is pruned more than the
/// digests table, and RPC calls querying transactions respect both pruning configurations.
#[tokio::test]
async fn test_filter_pruned() {
    let mut cluster = cluster_with_pipelines(PipelineLayer {
        tx_affected_addresses: Some(concurrent_pipeline(5)),
        tx_digests: Some(concurrent_pipeline(10)),
        kv_transactions: Some(ConcurrentLayer::default()),
        cp_sequence_numbers: Some(ConcurrentLayer::default()),
        ..Default::default()
    })
    .await;

    let (a, akp) = get_account_key_pair();
    let (b, bkp) = get_account_key_pair();

    let mut a_txs = vec![];
    let mut b_txs = vec![];

    // (1) Create 5 checkpoints with transactions from `a` and `b`. Each checkpoint contains two
    // transactions form `a` and one from `b`. At this point nothing should be pruned on either
    // side.
    for _ in 0..5 {
        a_txs.push(transfer_dust(&mut cluster, a, &akp, b));
        a_txs.push(transfer_dust(&mut cluster, a, &akp, b));
        b_txs.push(transfer_dust(&mut cluster, b, &bkp, a));
        cluster.create_checkpoint().await;
    }

    check_tx_digests!(&cluster, a, false, a_txs.iter());
    check_tx_digests!(&cluster, b, false, b_txs.iter());
    check_tx_digests!(&cluster, a, true, a_txs.iter().rev());
    check_tx_digests!(&cluster, b, true, b_txs.iter().rev());

    // (2) Add 5 more checkpoints, now the filter table is pruned, but the digests are not.
    for _ in 0..5 {
        a_txs.push(transfer_dust(&mut cluster, a, &akp, b));
        a_txs.push(transfer_dust(&mut cluster, a, &akp, b));
        b_txs.push(transfer_dust(&mut cluster, b, &bkp, a));
        cluster.create_checkpoint().await;
    }

    cluster
        .wait_for_pruner("tx_affected_addresses", 5, Duration::from_secs(10))
        .await
        .unwrap();

    check_tx_digests!(&cluster, a, false, a_txs[10..].iter());
    check_tx_digests!(&cluster, b, false, b_txs[5..].iter());
    check_tx_digests!(&cluster, a, true, a_txs[10..].iter().rev());
    check_tx_digests!(&cluster, b, true, b_txs[5..].iter().rev());

    // (3) Last 5 checkpoints, now both tables have been pruned.
    for _ in 0..5 {
        a_txs.push(transfer_dust(&mut cluster, a, &akp, b));
        a_txs.push(transfer_dust(&mut cluster, a, &akp, b));
        b_txs.push(transfer_dust(&mut cluster, b, &bkp, a));
        cluster.create_checkpoint().await;
    }

    cluster
        .wait_for_pruner("tx_digests", 5, Duration::from_secs(10))
        .await
        .unwrap();

    cluster
        .wait_for_pruner("tx_affected_addresses", 10, Duration::from_secs(10))
        .await
        .unwrap();

    check_tx_digests!(&cluster, a, false, a_txs[20..].iter());
    check_tx_digests!(&cluster, b, false, b_txs[10..].iter());
    check_tx_digests!(&cluster, a, true, a_txs[20..].iter().rev());
    check_tx_digests!(&cluster, b, true, b_txs[10..].iter().rev());
}

/// The same as the test above, but this time the digests are pruned more than the filter.
#[tokio::test]
async fn test_digests_pruned() {
    let mut cluster = cluster_with_pipelines(PipelineLayer {
        tx_affected_addresses: Some(concurrent_pipeline(10)),
        tx_digests: Some(concurrent_pipeline(5)),
        kv_transactions: Some(ConcurrentLayer::default()),
        cp_sequence_numbers: Some(ConcurrentLayer::default()),
        ..Default::default()
    })
    .await;

    let (a, akp) = get_account_key_pair();
    let (b, bkp) = get_account_key_pair();

    let mut a_txs = vec![];
    let mut b_txs = vec![];

    // (1) Create 5 checkpoints with transactions from `a` and `b`. Each checkpoint contains two
    // transactions form `a` and one from `b`. At this point nothing should be pruned on either
    // side.
    for _ in 0..5 {
        a_txs.push(transfer_dust(&mut cluster, a, &akp, b));
        a_txs.push(transfer_dust(&mut cluster, a, &akp, b));
        b_txs.push(transfer_dust(&mut cluster, b, &bkp, a));
        cluster.create_checkpoint().await;
    }

    check_tx_digests!(&cluster, a, false, a_txs.iter());
    check_tx_digests!(&cluster, b, false, b_txs.iter());
    check_tx_digests!(&cluster, a, true, a_txs.iter().rev());
    check_tx_digests!(&cluster, b, true, b_txs.iter().rev());

    // (2) Add 5 more checkpoints, now the digests table is pruned, but the filters are not.
    for _ in 0..5 {
        a_txs.push(transfer_dust(&mut cluster, a, &akp, b));
        a_txs.push(transfer_dust(&mut cluster, a, &akp, b));
        b_txs.push(transfer_dust(&mut cluster, b, &bkp, a));
        cluster.create_checkpoint().await;
    }

    cluster
        .wait_for_pruner("tx_digests", 5, Duration::from_secs(10))
        .await
        .unwrap();

    check_tx_digests!(&cluster, a, false, a_txs[10..].iter());
    check_tx_digests!(&cluster, b, false, b_txs[5..].iter());
    check_tx_digests!(&cluster, a, true, a_txs[10..].iter().rev());
    check_tx_digests!(&cluster, b, true, b_txs[5..].iter().rev());

    // (3) Last 5 checkpoints, now both tables have been pruned.
    for _ in 0..5 {
        a_txs.push(transfer_dust(&mut cluster, a, &akp, b));
        a_txs.push(transfer_dust(&mut cluster, a, &akp, b));
        b_txs.push(transfer_dust(&mut cluster, b, &bkp, a));
        cluster.create_checkpoint().await;
    }

    cluster
        .wait_for_pruner("tx_digests", 10, Duration::from_secs(10))
        .await
        .unwrap();

    cluster
        .wait_for_pruner("tx_affected_addresses", 5, Duration::from_secs(10))
        .await
        .unwrap();

    check_tx_digests!(&cluster, a, false, a_txs[20..].iter());
    check_tx_digests!(&cluster, a, true, a_txs[20..].iter().rev());
    check_tx_digests!(&cluster, b, false, b_txs[10..].iter());
    check_tx_digests!(&cluster, b, true, b_txs[10..].iter().rev());
}

/// Set-up a cluster with a custom configuration for pipelines.
async fn cluster_with_pipelines(pipeline: PipelineLayer) -> FullCluster {
    FullCluster::new_with_configs(
        Simulacrum::new(),
        IndexerArgs::default(),
        SystemPackageTaskArgs::default(),
        IndexerConfig {
            pipeline,
            ..IndexerConfig::for_test()
        },
        RpcConfig::example(),
        &prometheus::Registry::new(),
        CancellationToken::new(),
    )
    .await
    .expect("Failed to create cluster")
}

/// Create a configuration for a concurrent pipeline with pruning configured to retain `retention`
/// checkpoints.
fn concurrent_pipeline(retention: u64) -> ConcurrentLayer {
    ConcurrentLayer {
        pruner: Some(PrunerLayer {
            retention: Some(retention),
            ..Default::default()
        }),
        ..Default::default()
    }
}

/// Request gas from the "faucet" in `cluster`, and craft a transaction transferring 1 MIST from
/// `sender` (signed for with `signer`) to `recipient`, and returns the digest of the transaction as
/// long as it succeeded.
fn transfer_dust(
    cluster: &mut FullCluster,
    sender: SuiAddress,
    signer: &dyn Signer<Signature>,
    recipient: SuiAddress,
) -> TransactionDigest {
    let fx = cluster
        .request_gas(sender, DEFAULT_GAS_BUDGET + 1)
        .expect("Failed to request gas");

    let gas = find_address_owned(&fx).expect("Failed to find gas object");

    let mut builder = ProgrammableTransactionBuilder::new();
    builder.transfer_sui(recipient, Some(1));

    let data = TransactionData::new_programmable(
        sender,
        vec![gas],
        builder.finish(),
        DEFAULT_GAS_BUDGET,
        cluster.reference_gas_price(),
    );

    let digest = data.digest();
    let (fx, _) = cluster
        .execute_transaction(Transaction::from_data_and_signer(data, vec![signer]))
        .expect("Failed to execute transaction");

    assert!(fx.status().is_ok());
    digest
}

/// Query a page of transactions sent by `sender` from the RPC on `cluster`. `cursor`, `limit`, and
/// `descending` control the pagination of the request. Returns a list of digests, and a cursor if
/// a next page exists and there is a cursor.
async fn query_transactions(
    cluster: &FullCluster,
    sender: SuiAddress,
    cursor: Option<String>,
    limit: usize,
    descending: bool,
) -> (Vec<TransactionDigest>, Option<String>) {
    let query = json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "suix_queryTransactionBlocks",
        "params": [
            {
                "filter": {
                    "FromAddress": sender,
                },
            },
            cursor,
            limit,
            descending
        ]
    });

    let client = Client::new();
    let response = client
        .post(cluster.rpc_url())
        .json(&query)
        .send()
        .await
        .expect("Failed to send request");

    let body: Value = response
        .json()
        .await
        .expect("Failed to parse JSON-RPC response");

    let mut digests = vec![];
    assert!(body["error"].is_null(), "RPC error: {}", body["error"]);
    for result in body["result"]["data"].as_array().unwrap() {
        let digest = result["digest"].as_str().unwrap();
        let digest = TransactionDigest::from_str(digest).unwrap();
        digests.push(digest);
    }

    let has_next_page = body["result"]["hasNextPage"].as_bool().unwrap();
    let cursor = has_next_page.then(|| body["result"]["nextCursor"].as_str().unwrap().to_owned());

    (digests, cursor)
}
