// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

pub mod construct;
pub mod proof;

#[doc(inline)]
pub use proof::*;

#[doc(inline)]
pub use construct::*;
use serde_json::{json, Value};

pub async fn query_last_checkpoint_of_epoch(graphql_url: &str, epoch_id: u64) -> anyhow::Result<u64> {
    // GraphQL query to get the last checkpoint of an epoch
    let query = json!({
        "query": "query ($epochID: Int) { epoch(id: $epochID) { checkpoints(last: 1) { nodes { sequenceNumber } } } }",
        "variables": { "epochID": epoch_id }
    });

    // Submit the query by POSTing to the GraphQL endpoint
    let client = reqwest::Client::new();
    let resp = client
        .post(graphql_url)
        .header("Content-Type", "application/json")
        .body(query.to_string())
        .send()
        .await
        .expect("Cannot connect to graphql")
        .text()
        // .json::<HashMap<String, String>>()
        .await
        .expect("Cannot parse response");
    println!("Response: {}", resp);
    // Parse the JSON response to get the last checkpoint of the epoch
    let v: Value = serde_json::from_str(resp.as_str()).expect("Incorrect JSON response");
    let checkpoint_number = v["data"]["epoch"]["checkpoints"]["nodes"][0]["sequenceNumber"]
        .as_u64()
        .unwrap();

    Ok(checkpoint_number)
}

// The list of checkpoints at the end of each epoch
#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
pub struct CheckpointsList {
    // List of end of epoch checkpoints
    pub checkpoints: Vec<u64>,
}
