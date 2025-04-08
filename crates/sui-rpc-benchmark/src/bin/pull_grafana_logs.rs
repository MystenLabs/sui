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

#[derive(Debug)]
struct LogEntry {
    timestamp: String,
    host: String,
    method: String,
    body: String,
}

/// One example message is:
/// 2025-02-11T23:15:17.944697206Z stderr F 2025-02-11T23:15:17.944501Z  INFO sui_edge_proxy::handlers: Sampled read request headers={"host": "wallet-rpc.mainnet.sui.io", "client-sdk-type": "typescript", "client-sdk-version": "1.17.0", "client-target-api-version": "1.40.0", "client-request-method": "suix_getBalance", "content-type": "application/json", "content-length": "152", "accept-encoding": "gzip", "user-agent": "okhttp/4.9.2", "x-cloud-trace-context": "31caa7db658044d850a002ccf4ff15b1/8018737809747708392", "cookie": "_cfuvid=h0GD1bYot45Ln6kVCdL4qsFCCyw3h2cLw3caDNmhWNw-1739262948231-0.0.1.1-604800000", "via": "1.1 google", "x-forwarded-for": "171.236.184.3, 34.8.28.138", "x-forwarded-proto": "https", "connection": "Keep-Alive"} body=b"{\"jsonrpc\":\"2.0\",\"id\":189393,\"method\":\"suix_getBalance\",\"params\":[\"0x23cad599a375b9c2cedd62fa20112526c90a71764230425cb7f557c0c0b3b150\",\"0x2::sui::SUI\"]}" peer_type=Read
fn extract_from_message(message: &str) -> Option<LogEntry> {
    let timestamp = message.split_whitespace().next()?.to_string();

    let headers_start = message.find("headers=")?;
    let headers_str = &message[headers_start..];
    let headers_json_str = headers_str
        .trim_start_matches("headers=")
        .split_once(" body=")?
        .0;
    let headers: Value = serde_json::from_str(headers_json_str).ok()?;
    let host = headers
        .get("host")
        .and_then(|h| h.as_str())
        .unwrap_or("unknown_host")
        .to_string();

    if let Some(body_start) = message.find("body=") {
        if let Some(peer_type_start) = message.find(" peer_type=") {
            let raw_body = &message[(body_start + 5)..peer_type_start].trim();
            if raw_body.starts_with('b') {
                let trimmed = raw_body.trim_start_matches('b').trim_matches('"');
                let unescaped = trimmed.replace("\\\"", "\"");

                if let Ok(parsed) = serde_json::from_str::<Value>(&unescaped) {
                    let method = parsed
                        .get("method")
                        .and_then(|m| m.as_str())
                        .unwrap_or("unknown_method")
                        .to_string();
                    return Some(LogEntry {
                        timestamp,
                        host,
                        method,
                        body: unescaped,
                    });
                }
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

    // Gather method statistics
    let mut method_map: HashMap<String, usize> = HashMap::new();
    let mut asc_log_entries = Vec::new();
    for log_entry in all_logs.into_iter().rev() {
        if let Some(entry) = extract_from_message(&log_entry.message) {
            *method_map.entry(entry.method.clone()).or_default() += 1;
            asc_log_entries.push(entry);
        }
    }
    for (method, count) in &method_map {
        info!("Found {} logs for method: {}", count, method);
    }

    let output_dir = env::var("OUTPUT_DIR").unwrap_or_else(|_| ".".to_string());
    let output_file = format!("{}/sampled_read_requests.jsonl", output_dir);
    if let Some(parent) = std::path::Path::new(&output_file).parent() {
        if !parent.exists() {
            std::fs::create_dir_all(parent)?;
        }
    }

    let file = File::create(&output_file)?;
    let mut writer = BufWriter::new(file);
    for entry in asc_log_entries {
        let line = format!(
            r#"{{"timestamp":"{}", "host":"{}", "method":"{}", "body":{}}}"#,
            entry.timestamp, entry.host, entry.method, entry.body
        );
        writer.write_all(line.as_bytes())?;
        writer.write_all(b"\n")?;
    }
    writer.flush()?;
    info!("Done! Wrote grouped logs to {}", output_file);
    Ok(())
}
