// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Convert transaction trace logs to Chrome Trace Viewer format.
//!
//! This tool reads transaction trace logs, fetches full transaction data from
//! the Sui GraphQL endpoint, and generates a Chrome Trace Viewer JSON file
//! that visualizes object utilization by transactions.

use anyhow::{Context, Result};
use clap::Parser;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use sui_transaction_trace::{LogReader, TimestampedEvent, TxEventType};

#[derive(Parser)]
#[command(name = "trace-to-chrome")]
#[command(about = "Convert transaction trace logs to Chrome Trace Viewer format")]
struct Args {
    /// Input trace log file (or directory with multiple files)
    #[arg(short, long)]
    input: PathBuf,

    /// Output Chrome Trace JSON file
    #[arg(short, long)]
    output: PathBuf,

    /// GraphQL endpoint URL
    #[arg(long, default_value = "https://graphql.mainnet.sui.io/graphql")]
    graphql_url: String,

    /// Use fake timing data (for testing)
    #[arg(long)]
    fake_data: bool,
}

/// Chrome Trace Event format
#[derive(Debug, Serialize)]
struct ChromeTraceEvent {
    name: String,
    cat: String,
    ph: String, // Phase: "B" (begin), "E" (end), or "X" (complete)
    ts: i64,    // Timestamp in microseconds
    #[serde(skip_serializing_if = "Option::is_none")]
    dur: Option<i64>, // Duration in microseconds (for "X" events)
    pid: u32,   // Process ID
    tid: String, // Thread ID (object ID)
    #[serde(skip_serializing_if = "Option::is_none")]
    args: Option<serde_json::Value>,
}

/// Transaction data from GraphQL
#[derive(Debug, Deserialize)]
struct TransactionData {
    input_objects: Vec<String>,
}

/// Fake transaction data for testing
fn get_fake_transaction_data(digest: &str) -> TransactionData {
    // Map digest to objects
    // Digest all-1s (0x0101...) uses objects A and B
    // Digest all-2s (0x0202...) uses objects B and C (conflicts on B)
    let input_objects = if digest.starts_with("01010101") {
        vec![
            "0x000000000000000000000000000000000000000000000000000000000000000a".to_string(),
            "0x000000000000000000000000000000000000000000000000000000000000000b".to_string(),
        ]
    } else if digest.starts_with("02020202") {
        vec![
            "0x000000000000000000000000000000000000000000000000000000000000000b".to_string(),
            "0x000000000000000000000000000000000000000000000000000000000000000c".to_string(),
        ]
    } else {
        // Default: use a generated object ID based on digest
        vec![format!("0x{:064x}", digest.len())]
    };

    TransactionData { input_objects }
}

/// Fetch transaction data from GraphQL endpoint
async fn fetch_transaction_data(
    client: &reqwest::Client,
    graphql_url: &str,
    digest: &str,
) -> Result<TransactionData> {
    let query = json!({
        "query": r#"
            query ($digest: String!) {
                transaction(digest: $digest) {
                    digest
                    effects {
                        objectChanges {
                            nodes {
                                inputState {
                                    address
                                }
                            }
                        }
                    }
                }
            }
        "#,
        "variables": {
            "digest": digest
        }
    });

    let response = client
        .post(graphql_url)
        .json(&query)
        .send()
        .await
        .context("Failed to send GraphQL request")?;

    let result: serde_json::Value = response
        .json()
        .await
        .context("Failed to parse GraphQL response")?;

    // Parse the response and extract input objects
    let tx_block = result
        .get("data")
        .and_then(|d| d.get("transaction"))
        .context("No transaction in response")?;

    let mut input_objects = Vec::new();
    if let Some(effects) = tx_block.get("effects") {
        if let Some(object_changes) = effects.get("objectChanges") {
            if let Some(nodes) = object_changes.get("nodes") {
                if let Some(nodes_array) = nodes.as_array() {
                    for node in nodes_array {
                        if let Some(input_state) = node.get("inputState") {
                            if let Some(address) = input_state.get("address") {
                                if let Some(addr_str) = address.as_str() {
                                    input_objects.push(addr_str.to_string());
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    Ok(TransactionData { input_objects })
}

/// Convert trace events to Chrome Trace format
fn convert_to_chrome_trace(
    events: Vec<TimestampedEvent>,
    tx_data_map: HashMap<String, TransactionData>,
) -> Vec<ChromeTraceEvent> {
    let mut chrome_events = Vec::new();
    let mut tx_begin_times: HashMap<String, i64> = HashMap::new();

    for event in events {
        let digest_str = hex::encode(event.digest);

        match event.event_type {
            TxEventType::ExecutionBegin => {
                // Convert SystemTime to microseconds since epoch
                let ts = event
                    .timestamp
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_micros() as i64;

                tx_begin_times.insert(digest_str.clone(), ts);

                // Get input objects for this transaction
                if let Some(tx_data) = tx_data_map.get(&digest_str) {
                    for object_id in &tx_data.input_objects {
                        // Create a Begin event for each object
                        chrome_events.push(ChromeTraceEvent {
                            name: digest_str[..16].to_string(), // Shortened digest
                            cat: "transaction".to_string(),
                            ph: "B".to_string(),
                            ts,
                            dur: None,
                            pid: 1,
                            tid: object_id.clone(),
                            args: Some(json!({
                                "digest": &digest_str,
                                "object": object_id,
                            })),
                        });
                    }
                }
            }
            TxEventType::ExecutionComplete => {
                let ts = event
                    .timestamp
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_micros() as i64;

                // Get input objects for this transaction
                if let Some(tx_data) = tx_data_map.get(&digest_str) {
                    for object_id in &tx_data.input_objects {
                        // Create an End event for each object
                        chrome_events.push(ChromeTraceEvent {
                            name: digest_str[..16].to_string(),
                            cat: "transaction".to_string(),
                            ph: "E".to_string(),
                            ts,
                            dur: None,
                            pid: 1,
                            tid: object_id.clone(),
                            args: Some(json!({
                                "digest": &digest_str,
                                "object": object_id,
                                "duration_us": ts - tx_begin_times.get(&digest_str).unwrap_or(&ts),
                            })),
                        });
                    }
                }
            }
        }
    }

    chrome_events
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();

    println!("Reading trace logs from {:?}...", args.input);

    // Read trace logs
    let mut all_events = Vec::new();
    if args.input.is_dir() {
        // Read all trace files in directory
        for entry in std::fs::read_dir(&args.input)? {
            let entry = entry?;
            let path = entry.path();
            if path
                .file_name()
                .and_then(|n| n.to_str())
                .map(|s| s.starts_with("tx-trace-") && s.ends_with(".bin"))
                .unwrap_or(false)
            {
                let mut reader = LogReader::new(&path)?;
                for event in reader.iter() {
                    all_events.push(event?);
                }
            }
        }
    } else {
        // Read single file
        let mut reader = LogReader::new(&args.input)?;
        for event in reader.iter() {
            all_events.push(event?);
        }
    }

    println!("Found {} events", all_events.len());

    // Extract unique transaction digests (keep raw bytes)
    let mut tx_digests = HashSet::new();
    for event in &all_events {
        tx_digests.insert(event.digest);
    }

    println!("Found {} unique transactions", tx_digests.len());

    // Fetch transaction data
    let client = reqwest::Client::new();
    let mut tx_data_map = HashMap::new();

    for digest_bytes in tx_digests {
        let digest_hex = hex::encode(digest_bytes);
        let digest_base58 = bs58::encode(digest_bytes).into_string();

        let tx_data = if args.fake_data {
            get_fake_transaction_data(&digest_hex)
        } else {
            println!(
                "Fetching data for transaction {} (base58: {})",
                &digest_hex[..16],
                digest_base58
            );
            match fetch_transaction_data(&client, &args.graphql_url, &digest_base58).await {
                Ok(data) => data,
                Err(e) => {
                    eprintln!(
                        "Warning: Failed to fetch transaction {}: {}",
                        digest_base58, e
                    );
                    continue;
                }
            }
        };

        println!(
            "  Transaction {} uses {} objects",
            &digest_hex[..16],
            tx_data.input_objects.len()
        );
        tx_data_map.insert(digest_hex, tx_data);
    }

    // Convert to Chrome Trace format
    println!("Converting to Chrome Trace format...");
    let chrome_events = convert_to_chrome_trace(all_events, tx_data_map);

    // Write output
    let output = json!({
        "traceEvents": chrome_events,
        "displayTimeUnit": "ms",
    });

    std::fs::write(&args.output, serde_json::to_string_pretty(&output)?)?;
    println!("Wrote Chrome Trace to {:?}", args.output);
    println!("Open chrome://tracing and load the file to visualize object utilization");

    Ok(())
}
