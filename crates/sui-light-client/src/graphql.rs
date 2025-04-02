// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use reqwest;
use serde_json::{json, Value};

use crate::config::Config;

pub async fn query_last_checkpoint_of_epoch(config: &Config, epoch_id: u64) -> anyhow::Result<u64> {
    // GraphQL query to get the last checkpoint of an epoch
    let query = json!({
        "query": "query ($epochID: Int) { epoch(id: $epochID) { checkpoints(last: 1) { nodes { sequenceNumber } } } }",
        "variables": { "epochID": epoch_id }
    });

    // Submit the query by POSTing to the GraphQL endpoint
    let client = reqwest::Client::new();
    let resp = client
        .post(config.graphql_url.as_ref().cloned().unwrap())
        .header("Content-Type", "application/json")
        .body(query.to_string())
        .send()
        .await
        .expect("Cannot connect to graphql")
        .text()
        .await
        .expect("Cannot parse response");

    // Parse the JSON response to get the last checkpoint of the epoch
    let v: Value = serde_json::from_str(resp.as_str()).expect("Incorrect JSON response");
    let checkpoint_number = v["data"]["epoch"]["checkpoints"]["nodes"][0]["sequenceNumber"]
        .as_u64()
        .unwrap();

    Ok(checkpoint_number)
}
