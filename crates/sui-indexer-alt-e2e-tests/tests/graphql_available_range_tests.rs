// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::time::Duration;

use reqwest::Client;
use serde_json::{json, Value};
use simulacrum::Simulacrum;
use sui_indexer_alt::config::{ConcurrentLayer, IndexerConfig, PipelineLayer, PrunerLayer};
use sui_indexer_alt_e2e_tests::{find_address_owned, FullCluster, OffchainClusterConfig};
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

// GraphQL query constants
const RETENTION_TRANSACTIONS_QUERY: &str = r#"
    query {
        serviceConfig {
            retention(type: "Query", field: "transactions", filters: ["affectedAddress"]) {
                first { sequenceNumber }
                last { sequenceNumber }
            }
        }
    }
"#;

const TRANSACTIONS_QUERY: &str = r#"
    query($filter: TransactionFilter!, $first: Int) {
        transactions(filter: $filter, first: $first) {
            nodes {
                digest
            }
        }
    }
"#;

/// Test available range queries with retention configurations
#[tokio::test]
async fn test_available_range_with_pipelines() {
    const RETENTION: u64 = 5;
    let mut cluster = cluster_with_pipelines(PipelineLayer {
        tx_affected_addresses: Some(concurrent_pipeline(RETENTION)),
        tx_digests: Some(concurrent_pipeline(RETENTION)),
        cp_sequence_numbers: Some(concurrent_pipeline(RETENTION)),
        ev_struct_inst: Some(concurrent_pipeline(RETENTION)),
        ..Default::default()
    })
    .await;

    let (a, akp) = get_account_key_pair();
    let (b, _) = get_account_key_pair();

    // Create 10 checkpoints with transactions from `a` and `b`. Each checkpoint contains one
    // transactions from `a` to `b`.
    for _ in 0..10 {
        transfer_dust(&mut cluster, a, &akp, b);
        cluster.create_checkpoint().await;
    }

    cluster
        .wait_for_pruner("tx_digests", RETENTION, Duration::from_secs(10))
        .await
        .unwrap();

    cluster
        .wait_for_pruner("cp_sequence_numbers", RETENTION, Duration::from_secs(10))
        .await
        .unwrap();

    // TODO: (henrychen) Add tests when we use retention configs in our pagination api
    let tx = execute_graphql_query(
        &cluster,
        TRANSACTIONS_QUERY,
        Some(json!({
            "filter": {
                "sentAddress": a.to_string()
            },
            "first": 20
        })),
    )
    .await;

    assert_eq!(
        tx["data"]["transactions"]["nodes"]
            .as_array()
            .unwrap()
            .len(),
        5
    );

    let retention = execute_graphql_query(&cluster, RETENTION_TRANSACTIONS_QUERY, None).await;
    assert_eq!(
        retention["data"]["serviceConfig"]["retention"]["first"]["sequenceNumber"]
            .as_u64()
            .unwrap(),
        6
    );
    assert_eq!(
        retention["data"]["serviceConfig"]["retention"]["last"]["sequenceNumber"]
            .as_u64()
            .unwrap(),
        10
    );

    cluster.stopped().await;
}

/// Set-up a cluster with a custom configuration for pipelines.
async fn cluster_with_pipelines(pipeline: PipelineLayer) -> FullCluster {
    FullCluster::new_with_configs(
        Simulacrum::new(),
        OffchainClusterConfig {
            indexer_config: IndexerConfig {
                pipeline,
                ..IndexerConfig::for_test()
            },
            ..Default::default()
        },
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

/// Helper function to test available range GraphQL queries
async fn execute_graphql_query(
    cluster: &FullCluster,
    query: &str,
    variables: Option<Value>,
) -> serde_json::Value {
    let client = Client::new();
    let url = cluster.graphql_url();

    client
        .post(url.as_str())
        .json(&json!({
            "query": query,
            "variables": variables.unwrap_or_default()
        }))
        .send()
        .await
        .expect("Failed to send request")
        .json()
        .await
        .expect("Failed to parse response")
}
