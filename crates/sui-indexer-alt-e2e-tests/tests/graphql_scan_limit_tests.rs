// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::time::Duration;

use insta::assert_json_snapshot;
use prometheus::Registry;
use reqwest::Client;
use serde_json::json;
use simulacrum::Simulacrum;
use sui_indexer_alt_e2e_tests::FullCluster;
use sui_indexer_alt_e2e_tests::OffchainClusterConfig;
use sui_indexer_alt_graphql::config::Limits;
use sui_indexer_alt_graphql::config::RpcConfig;

#[tokio::test]
async fn test_scan_limit_exceeded() {
    let registry = Registry::new();

    let graphql_config = RpcConfig {
        limits: Limits {
            max_scan_limit: 10,
            ..Default::default()
        },
        ..Default::default()
    };

    let mut cluster = FullCluster::new_with_configs(
        Simulacrum::new(),
        OffchainClusterConfig {
            graphql_config,
            ..Default::default()
        },
        &registry,
    )
    .await
    .expect("Failed to create cluster");

    // Create 20 checkpoints to exceed the limit of 10
    for _ in 0..20 {
        cluster.create_checkpoint().await;
    }

    cluster
        .wait_for_graphql(20, Duration::from_secs(30))
        .await
        .expect("Timed out waiting for checkpoint");

    // Query that would scan all 20 checkpoints (exceeds limit of 10)
    let query = r#"
        query($addr: SuiAddress!) {
            scanTransactions(filter: { affectedAddress: $addr, afterCheckpoint: 0, beforeCheckpoint: 21 }) {
                edges { node { digest } }
            }
        }
    "#;

    let variables = json!({
        "addr": "0x0000000000000000000000000000000000000000000000000000000000000000"
    });

    let client = Client::new();
    let response = client
        .post(cluster.graphql_url())
        .json(&json!({
            "query": query,
            "variables": variables
        }))
        .send()
        .await
        .expect("Failed to send request");

    let result: serde_json::Value = response.json().await.expect("Failed to parse response");
    assert_json_snapshot!(result["errors"]);
}
