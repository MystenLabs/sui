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
const AVAILABLE_RANGE_QUERY: &str = r#"
    query($type: String!, $field: String, $filters: [String]) {
        serviceConfig {
            availableRange(type: $type, field: $field, filters: $filters) {
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
    let mut cluster = cluster_with_pipelines(PipelineLayer {
        tx_affected_addresses: Some(concurrent_pipeline(5)),
        tx_digests: Some(concurrent_pipeline(10)),
        cp_sequence_numbers: Some(concurrent_pipeline(5)),
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
        .wait_for_pruner("tx_affected_addresses", 5, Duration::from_secs(10))
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

    // There should only be 5 transactions after pruning as we only retain 5 checkpoints. and each checkpoint contains one transaction.
    assert_eq!(
        tx["data"]["transactions"]["nodes"]
            .as_array()
            .unwrap()
            .len(),
        5
    );

    // Last 5 checkpoints are available as we retain 5 checkpoints for tx_affected_addresses.
    let affected_addresses_retention = execute_graphql_query(
        &cluster,
        AVAILABLE_RANGE_QUERY,
        Some(json!({
            "type": "Query".to_string(),
            "field": "transactions".to_string(),
            "filters": ["affectedAddress".to_string()]
        })),
    )
    .await;

    assert_eq!(
        affected_addresses_retention["data"]["serviceConfig"]["availableRange"]["first"]
            ["sequenceNumber"]
            .as_u64()
            .unwrap(),
        6
    );
    assert_eq!(
        affected_addresses_retention["data"]["serviceConfig"]["availableRange"]["last"]
            ["sequenceNumber"]
            .as_u64()
            .unwrap(),
        10
    );

    for _ in 0..10 {
        transfer_dust(&mut cluster, a, &akp, b);
        cluster.create_checkpoint().await;
    }

    cluster
        .wait_for_pruner("tx_affected_addresses", 10, Duration::from_secs(10))
        .await
        .unwrap();

    cluster
        .wait_for_pruner("tx_digests", 5, Duration::from_secs(10))
        .await
        .unwrap();

    // Last 10 checkpoints are available as we retain 10 checkpoints for tx_digests.
    let transasction_retention = execute_graphql_query(
        &cluster,
        AVAILABLE_RANGE_QUERY,
        Some(json!({
            "type": "Query",
            "field": "transactions",
            "filters": []
        })),
    )
    .await;

    assert_eq!(
        transasction_retention["data"]["serviceConfig"]["availableRange"]["first"]
            ["sequenceNumber"]
            .as_u64()
            .unwrap(),
        11
    );
    assert_eq!(
        transasction_retention["data"]["serviceConfig"]["availableRange"]["last"]["sequenceNumber"]
            .as_u64()
            .unwrap(),
        20
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
