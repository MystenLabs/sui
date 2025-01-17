// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/// This script pulls sampled JSON RPC read requests from Grafana, extracts JSON bodies,
/// and groups them by RPC "method" for later replay and analysis.
use reqwest::header::ACCEPT;
use serde::Deserialize;
use serde_json::Value;
use std::collections::HashMap;
use std::env;
use std::error::Error;
use std::fs::File;
use std::io::{BufWriter, Write};
use std::process;
use tracing::{debug, error, info};

// Loki has a limit of 10000 logs per request.
const MAX_LOGS_PER_REQUEST: u64 = 10000;

/// structs below are to mimic the parsed structure of LokiResponse.
#[derive(Debug, Deserialize)]
struct LokiResponse {
    data: LokiData,
}

#[derive(Debug, Deserialize)]
struct LokiData {
    result: Vec<LokiResult>,
}

#[derive(Debug, Deserialize)]
struct LokiResult {
    values: Vec<(String, String)>,
}

#[derive(Debug, Deserialize)]
struct GrafanaLog {
    message: String,
}

fn extract_body_from_message(message: &str) -> Option<String> {
    if let Some(body_start) = message.find("body=") {
        if let Some(peer_type_start) = message.find(" peer_type=") {
            let raw_body = &message[(body_start + 5)..peer_type_start].trim();
            if raw_body.starts_with('b') {
                let trimmed = raw_body.trim_start_matches('b').trim_matches('"');
                let unescaped = trimmed.replace("\\\"", "\"");
                return Some(unescaped);
            }
        }
    }
    None
}

async fn fetch_logs(
    client: &reqwest::Client,
    url: &str,
    query: &str,
    start: &str,
    end: &str,
    limit: u64,
    offset: Option<u64>,
) -> Result<LokiResponse, Box<dyn Error>> {
    let mut params = vec![
        ("query".to_string(), query.to_string()),
        ("start".to_string(), start.to_string()),
        ("end".to_string(), end.to_string()),
        ("limit".to_string(), limit.to_string()),
    ];
    if let Some(o) = offset {
        params.push(("start_from".to_string(), o.to_string()));
    }

    let resp = client
        .get(url)
        .header(ACCEPT, "application/json")
        .query(&params)
        .send()
        .await?;

    let status = resp.status();
    if !status.is_success() {
        let error_body = resp.text().await?;
        error!("Error response: {}", error_body);
        return Err(format!("Request failed with status: {}", status).into());
    }
    Ok(resp.json().await?)
}

#[tokio::main]
async fn main() {
    let _guard = telemetry_subscribers::TelemetryConfig::new()
        .with_env()
        .init();
    if let Err(e) = run().await {
        error!("Error: {}", e);
        process::exit(1);
    }
}

async fn run() -> Result<(), Box<dyn Error>> {
    let grafana_url = env::var("GRAFANA_LOGS_URL")
        .unwrap_or_else(|_| "https://metrics.sui.io/loki/api/v1/query_range".to_string());
    let net = env::var("NET").unwrap_or_else(|_| "mainnet".to_string());
    let namespace = if net == "testnet" {
        "rpc-testnet".to_string()
    } else if net == "mainnet" {
        "rpc-mainnet".to_string()
    } else {
        "UNKNOWN_NET".to_string()
    };
    let substring = env::var("SUBSTRING").unwrap_or_else(|_| "Sampled read request".to_string());
    let query = format!(
        r#"{{namespace="{}", container="sui-edge-proxy-mysten"}} |= "{}""#,
        namespace, substring
    );
    debug!("Query: {}", query);

    let now = chrono::Utc::now();
    let one_day_ago = now - chrono::Duration::days(1);
    let start = env::var("START").unwrap_or(one_day_ago.format("%Y-%m-%dT%H:%M:%SZ").to_string());
    let end = env::var("END").unwrap_or(now.format("%Y-%m-%dT%H:%M:%SZ").to_string());
    let limit: Option<u64> = env::var("LIMIT").ok().and_then(|l| l.parse().ok());
    let client = reqwest::Client::new();

    let mut all_logs = Vec::new();
    let mut offset = None;
    loop {
        let chunk_limit = match limit {
            Some(l) => {
                let fetched = all_logs.len() as u64;
                if fetched >= l {
                    break;
                }
                std::cmp::min(MAX_LOGS_PER_REQUEST, l - fetched)
            }
            None => MAX_LOGS_PER_REQUEST,
        };

        let response = fetch_logs(
            &client,
            &grafana_url,
            &query,
            &start,
            &end,
            chunk_limit,
            offset,
        )
        .await?;
        let batch: Vec<_> = response
            .data
            .result
            .into_iter()
            .flat_map(|result| {
                result
                    .values
                    .into_iter()
                    .map(|(_, message)| GrafanaLog { message })
            })
            .collect();
        // If we have no logs, break
        if batch.is_empty() {
            break;
        }

        let batch_len = batch.len();
        all_logs.extend(batch);
        offset = Some(offset.unwrap_or(0) + batch_len as u64);
    }

    info!("Found {} logs.", all_logs.len());

    let mut method_map: HashMap<String, Vec<String>> = HashMap::new();
    for log_entry in all_logs {
        if let Some(body_content) = extract_body_from_message(&log_entry.message) {
            if let Ok(parsed) = serde_json::from_str::<Value>(&body_content) {
                let method = parsed
                    .get("method")
                    .and_then(|m| m.as_str())
                    .unwrap_or("unknown_method")
                    .to_string();
                method_map.entry(method).or_default().push(body_content);
            }
        }
    }

    let file = File::create("sampled_read_requests.jsonl")?;
    let mut writer = BufWriter::new(file);

    for (method, bodies) in method_map {
        info!("Writing {} logs for method: {}", bodies.len(), method);
        for body in bodies {
            let line = format!(r#"{{"method":"{}", "body":{}}}"#, method, body);
            writer.write_all(line.as_bytes())?;
            writer.write_all(b"\n")?;
        }
    }

    writer.flush()?;
    info!("Done! Wrote grouped logs to sampled_read_requests.jsonl");
    Ok(())
}
