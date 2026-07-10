// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use reqwest::Client;
use serde_json::Value;
use serde_json::json;
use sui_indexer_alt_e2e_tests::FullCluster;
use sui_protocol_config::Chain;
use sui_protocol_config::ProtocolConfig;
use sui_protocol_config::ProtocolVersion;

async fn get_protocol_config(cluster: &FullCluster, params: Value) -> Value {
    let query = json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "sui_getProtocolConfig",
        "params": params,
    });

    let response = Client::new()
        .post(cluster.jsonrpc_url())
        .json(&query)
        .send()
        .await
        .expect("Request to JSON-RPC server failed");

    response
        .json()
        .await
        .expect("Failed to parse JSON-RPC response")
}

#[tokio::test]
async fn test_latest_version() {
    telemetry_subscribers::init_for_testing();
    let mut cluster = FullCluster::new().await.unwrap();

    // Make sure the genesis epoch start has been indexed.
    cluster.create_checkpoint().await;

    let response = get_protocol_config(&cluster, json!([])).await;
    assert!(
        response["error"].is_null(),
        "RPC error: {}",
        response["error"]
    );

    let result = &response["result"];
    let min = ProtocolVersion::MIN.as_u64().to_string();
    let max = ProtocolVersion::MAX.as_u64().to_string();

    // The simulator runs at the maximum supported protocol version.
    assert_eq!(result["protocolVersion"].as_str().unwrap(), max);
    assert_eq!(result["minSupportedProtocolVersion"].as_str().unwrap(), min);
    assert_eq!(result["maxSupportedProtocolVersion"].as_str().unwrap(), max);

    // Spot check the contents of each table against the protocol config the response was
    // generated from.
    let config = ProtocolConfig::get_for_version(ProtocolVersion::MAX, Chain::Unknown);

    assert_eq!(
        result["attributes"]["max_tx_gas"],
        json!({ "u64": config.max_tx_gas().to_string() }),
    );

    let feature_flags = result["featureFlags"].as_object().unwrap();
    assert_eq!(feature_flags.len(), config.feature_map().len());

    // `configs` is a complete view: it includes both attributes and feature flags.
    let configs = result["configs"].as_object().unwrap();
    assert!(configs.len() >= feature_flags.len());
    assert!(!configs["max_tx_gas"].is_null());
}

#[tokio::test]
async fn test_specific_version() {
    telemetry_subscribers::init_for_testing();
    let mut cluster = FullCluster::new().await.unwrap();
    cluster.create_checkpoint().await;

    // The simulator's chain runs at the max supported version, so that is the only version the
    // indexer has records for.
    let version = ProtocolVersion::MAX.as_u64();
    let response = get_protocol_config(&cluster, json!([version.to_string()])).await;
    assert!(
        response["error"].is_null(),
        "RPC error: {}",
        response["error"]
    );

    assert_eq!(
        response["result"]["protocolVersion"].as_str().unwrap(),
        version.to_string(),
    );
}

#[tokio::test]
async fn test_unknown_version() {
    telemetry_subscribers::init_for_testing();
    let mut cluster = FullCluster::new().await.unwrap();
    cluster.create_checkpoint().await;

    // Configs are served from the database, so any version the indexer has not seen -- here, a
    // version the chain has not reached -- is not found.
    let unknown = ProtocolVersion::MAX.as_u64() + 1;
    let response = get_protocol_config(&cluster, json!([unknown.to_string()])).await;

    assert_eq!(response["error"]["code"], -32602);
    assert!(
        response["error"]["message"]
            .as_str()
            .unwrap()
            .contains("not found"),
        "Unexpected error message: {}",
        response["error"]["message"],
    );
}
