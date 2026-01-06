// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use reqwest::Client;
use serde_json::{Value, json};
use sui_indexer_alt_e2e_tests::FullCluster;
use sui_indexer_alt_graphql::extensions::logging::REQUEST_ID_HEADER;

struct TestResult {
    request_id: String,
    json: Value,
}

#[tokio::test]
async fn request_id_success() -> Result<(), anyhow::Error> {
    let TestResult { request_id, json } = request_id("query { epoch(id: 0) { id } }").await?;
    assert!(json.get("data").is_some());
    assert_eq!(request_id.len(), 36);
    Ok(())
}

#[tokio::test]
async fn request_id_error() -> Result<(), anyhow::Error> {
    let TestResult { request_id, json } = request_id("invalid").await?;
    assert!(json.get("errors").is_some());
    assert_eq!(request_id.len(), 36);
    Ok(())
}

async fn request_id(query: &str) -> Result<TestResult, anyhow::Error> {
    telemetry_subscribers::init_for_testing();
    let cluster = FullCluster::new().await.unwrap();

    let client = Client::new();

    let query = json!({"query": query});
    let request = client.post(cluster.graphql_url()).json(&query);
    let response = request.send().await?;

    let request_id = response
        .headers()
        .get(REQUEST_ID_HEADER)
        .unwrap()
        .to_str()?
        .to_string();
    let json: Value = response.json().await?;

    Ok(TestResult { request_id, json })
}
