// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::time::Duration;

use insta::assert_json_snapshot;
use reqwest::Client;
use serde_json::Value;
use serde_json::json;
use simulacrum::Simulacrum;
use sui_indexer_alt::config::ConcurrentLayer;
use sui_indexer_alt::config::IndexerConfig;
use sui_indexer_alt::config::PipelineLayer;
use sui_indexer_alt::config::PrunerLayer;
use sui_indexer_alt_graphql::config::RpcConfig as GraphQlConfig;
use sui_indexer_alt_graphql::config::WatermarkConfig;

use sui_indexer_alt_e2e_tests::FullCluster;
use sui_indexer_alt_e2e_tests::OffchainClusterConfig;

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

const CHECKPOINT_QUERY: &str = r#"
    query {
        checkpoints(first: 50) {
            nodes {
                sequenceNumber
            }
        }
    }
"#;

/// `Query.transactions`/`Query.events` are served entirely by the ledger gRPC service now (no
/// Postgres fallback), so their available range tracks the "ledger_grpc" watermark rather than any
/// Postgres pipeline: a lower bound of 0 (the ledger service doesn't yet expose its own retention
/// bound) and an upper bound that tracks the current tip, regardless of filters.
#[tokio::test]
async fn test_available_range_for_ledger_grpc_backed_fields() {
    let mut cluster = cluster_with_pipelines(PipelineLayer {
        cp_sequence_numbers: Some(ConcurrentLayer::default()),
        ..Default::default()
    })
    .await;

    for _ in 1..=4 {
        cluster.create_checkpoint().await;
    }

    for (field, filters) in [
        ("transactions", None),
        ("transactions", Some(&["affectedAddress"][..])),
        ("events", None),
        ("events", Some(&["module"][..])),
    ] {
        let response = query_available_range(&cluster, field, filters).await;
        let (first, last) = collect_sequence_numbers(&response);
        assert_eq!(first, 0, "field: {field}, filters: {filters:?}");
        assert_eq!(last, 4, "field: {field}, filters: {filters:?}");
    }
}

/// Test that querying available range for a pipeline that is not enabled returns an error
#[tokio::test]
async fn test_available_range_pipeline_unavailable() {
    let cluster = cluster_with_pipelines(PipelineLayer {
        cp_sequence_numbers: Some(ConcurrentLayer::default()),
        ..Default::default()
    })
    .await;

    let response = query_available_range(&cluster, "objects", None).await;
    assert_json_snapshot!(response["errors"], @r###"
    [
      {
        "message": "consistent queries across objects and balances not available",
        "locations": [
          {
            "line": 4,
            "column": 13
          }
        ],
        "path": [
          "serviceConfig",
          "availableRange"
        ],
        "extensions": {
          "code": "FEATURE_UNAVAILABLE"
        }
      }
    ]"###);
}

#[tokio::test]
async fn test_checkpoint_pagination_pruning() {
    let mut cluster = cluster_with_pipelines(PipelineLayer {
        cp_sequence_numbers: Some(concurrent_pipeline(5)),
        ..Default::default()
    })
    .await;

    let mut cp_sequence_numbers = vec![];

    // Create checkpoints 1 through 9
    for _ in 1..=9 {
        cp_sequence_numbers.push(cluster.create_checkpoint().await.sequence_number);
    }

    // We only retain 5 checkpoints so only checkpoints 5 through 9 should be available after pruning.
    cluster
        .wait_for_pruner("cp_sequence_numbers", 4, Duration::from_secs(10))
        .await
        .unwrap();

    let checkpoints_in_range = execute_graphql_query(&cluster, CHECKPOINT_QUERY, None).await;
    let checkpoints = checkpoints_in_range["data"]["checkpoints"]["nodes"]
        .as_array()
        .unwrap();

    assert_eq!(
        checkpoints[0]["sequenceNumber"].as_u64().unwrap(),
        cp_sequence_numbers[4]
    );
    assert_eq!(
        checkpoints[3]["sequenceNumber"].as_u64().unwrap(),
        cp_sequence_numbers[7]
    );
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
                    watermark_polling_interval: Duration::from_millis(50),
                },
                ..Default::default()
            },
            ..Default::default()
        },
        &prometheus::Registry::new(),
    )
    .await
    .expect("Failed to create cluster")
}

fn collect_sequence_numbers(resp: &Value) -> (u64, u64) {
    let range = &resp["data"]["serviceConfig"]["availableRange"];
    (
        range["first"]["sequenceNumber"].as_u64().unwrap(),
        range["last"]["sequenceNumber"].as_u64().unwrap(),
    )
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

async fn execute_graphql_query(
    cluster: &FullCluster,
    query: &str,
    variables: Option<Value>,
) -> serde_json::Value {
    Client::new()
        .post(cluster.graphql_url().as_str())
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

async fn query_available_range(
    cluster: &FullCluster,
    field: &str,
    filters: Option<&[&str]>,
) -> Value {
    let filters = filters
        .unwrap_or_default()
        .iter()
        .map(|&s| s.to_string())
        .collect::<Vec<_>>();
    execute_graphql_query(
        cluster,
        AVAILABLE_RANGE_QUERY,
        Some(json!({ "type": "Query", "field": field, "filters": filters })),
    )
    .await
}
