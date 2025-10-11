// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::time::Duration;

use reqwest::Client;
use serde_json::{json, Value};
use simulacrum::Simulacrum;
use sui_indexer_alt::config::{ConcurrentLayer, IndexerConfig, PipelineLayer, PrunerLayer};
use sui_indexer_alt_e2e_tests::{find::address_owned, FullCluster, OffchainClusterConfig};

use sui_indexer_alt_graphql::config::{RpcConfig as GraphQlConfig, WatermarkConfig};
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

macro_rules! assert_sequence_numbers_eq {
    ($first:expr, $last:expr, $resp:expr) => {
        let resp = $resp;
        let first = resp["data"]["serviceConfig"]["availableRange"]["first"]["sequenceNumber"]
            .as_u64()
            .unwrap();
        let last = resp["data"]["serviceConfig"]["availableRange"]["last"]["sequenceNumber"]
            .as_u64()
            .unwrap();

        assert_eq!(
            $first, first,
            "Expected first sequence number {}, got {resp:#?}",
            $first,
        );
        assert_eq!(
            $last, last,
            "Expected last sequence number {}, got {resp:#?}",
            $last,
        );
    };
}

/// Test available range queries with retention configurations
#[tokio::test]
async fn test_available_range_with_pipelines() {
    let mut cluster = cluster_with_pipelines(PipelineLayer {
        tx_affected_addresses: Some(concurrent_pipeline(5)),
        tx_digests: Some(concurrent_pipeline(10)),
        cp_sequence_numbers: Some(ConcurrentLayer::default()),
        ..Default::default()
    })
    .await;

    let (a, akp) = get_account_key_pair();
    let (b, _) = get_account_key_pair();

    // (1) Create 4 checkpoints with transactions from `a` and `b`. Each checkpoint contains one
    // transactions from `a` to `b`.  At this point we have 5 total checkpoints (Cp 0 is created by the cluster),
    // nothing should be pruned on either side.
    for _ in 1..=4 {
        transfer_dust(&mut cluster, a, &akp, b);
        cluster.create_checkpoint().await;
    }

    // Last 5 checkpoints are available as we have not pruned yet.
    let affected_addresses_available_range = execute_graphql_query(
        &cluster,
        AVAILABLE_RANGE_QUERY,
        Some(json!({
            "type": "Query".to_string(),
            "field": "transactions".to_string(),
            "filters": ["affectedAddress".to_string()]
        })),
    )
    .await;

    assert_sequence_numbers_eq!(0, 4, affected_addresses_available_range);

    // (2) Add 1 more checkpoint, now the affected_addresses table is pruned, but the digests are not.
    transfer_dust(&mut cluster, a, &akp, b);
    cluster.create_checkpoint().await;

    cluster
        .wait_for_pruner("tx_affected_addresses", 1, Duration::from_secs(10))
        .await
        .unwrap();

    let affected_addresses_available_range = execute_graphql_query(
        &cluster,
        AVAILABLE_RANGE_QUERY,
        Some(json!({
            "type": "Query".to_string(),
            "field": "transactions".to_string(),
            "filters": ["affectedAddress".to_string()]
        })),
    )
    .await;

    assert_sequence_numbers_eq!(1, 5, affected_addresses_available_range);

    // (3) Add 5 more checkpoints, now both tables have been pruned.
    for _ in 6..=10 {
        transfer_dust(&mut cluster, a, &akp, b);
        cluster.create_checkpoint().await;
    }

    cluster
        .wait_for_pruner("tx_digests", 1, Duration::from_secs(10))
        .await
        .unwrap();

    cluster
        .wait_for_pruner("tx_affected_addresses", 6, Duration::from_secs(10))
        .await
        .unwrap();

    // Last 10 checkpoints of 11 checkpoints are available as we retain 10 checkpoints for tx_digests.
    let transasction_available_range = execute_graphql_query(
        &cluster,
        AVAILABLE_RANGE_QUERY,
        Some(json!({
            "type": "Query",
            "field": "transactions",
            "filters": []
        })),
    )
    .await;

    assert_sequence_numbers_eq!(1, 10, transasction_available_range);

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
            graphql_config: GraphQlConfig {
                watermark: WatermarkConfig {
                    watermark_polling_interval: Duration::from_millis(100),
                },
                ..Default::default()
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

    let gas = address_owned(&fx).expect("Failed to find gas object");

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
