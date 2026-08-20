// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Separate-process subscription load client for `examples/serve_testnet`. Opens N SSE subscriptions,
//! scrapes the server's `/metrics` out-of-process, and samples isolated server RSS to a CSV.
//!
//! ```text
//! N=100 SECS=60 NAME=live_100 OUT=/tmp/subbench_testnet \
//!   cargo run --release --example subscribe_bench -p sui-indexer-alt-graphql
//! ```

use std::io::Write;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::sync::atomic::AtomicU64;
use std::sync::atomic::Ordering;
use std::time::Duration;
use std::time::Instant;

const QUERY: &str = "subscription { transactions { cursor node { digest } } }";

/// One SSE subscriber: counts delivered payloads (by `"digest"` occurrences) until `stop`.
async fn subscriber(url: String, delivered: Arc<AtomicU64>, stop: Arc<AtomicBool>) {
    let mut resp = match reqwest::Client::new()
        .post(&url)
        .header("Accept", "text/event-stream")
        .header("Content-Type", "application/json")
        .json(&serde_json::json!({ "query": QUERY }))
        .send()
        .await
    {
        Ok(r) if r.status().is_success() => r,
        _ => return,
    };
    while !stop.load(Ordering::Relaxed) {
        match tokio::time::timeout(Duration::from_millis(500), resp.chunk()).await {
            Ok(Ok(Some(chunk))) => {
                let n = std::str::from_utf8(&chunk)
                    .map(|s| s.matches("\"digest\"").count())
                    .unwrap_or(0);
                delivered.fetch_add(n as u64, Ordering::Relaxed);
            }
            Ok(Ok(None)) | Ok(Err(_)) => break,
            Err(_) => continue,
        }
    }
}

/// Sum a Prometheus counter/gauge across all label sets. For histograms pass `name_sum`/`name_count`.
fn metric_sum(text: &str, name: &str) -> f64 {
    let mut total = 0.0;
    for line in text.lines() {
        if line.starts_with('#') {
            continue;
        }
        let matches = line.starts_with(&format!("{name} ")) || line.starts_with(&format!("{name}{{"));
        if matches {
            if let Some(v) = line.rsplit(' ').next().and_then(|v| v.parse::<f64>().ok()) {
                total += v;
            }
        }
    }
    total
}

fn terminations_lagged(text: &str) -> f64 {
    text.lines()
        .filter(|l| l.starts_with("graphql_subscription_terminations{") && l.contains("reason=\"lagged\""))
        .filter_map(|l| l.rsplit(' ').next().and_then(|v| v.parse::<f64>().ok()))
        .sum()
}

/// RSS (KB) of the standalone server process, so memory is server-only (not this client's).
fn server_rss_kb() -> u64 {
    let out = std::process::Command::new("pgrep")
        .args(["-x", "serve_testnet"])
        .output()
        .ok();
    let pid = out
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .and_then(|s| s.lines().next().map(|l| l.trim().to_string()));
    let Some(pid) = pid else { return 0 };
    std::process::Command::new("ps")
        .args(["-o", "rss=", "-p", &pid])
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .and_then(|s| s.trim().parse().ok())
        .unwrap_or(0)
}

#[tokio::main(flavor = "multi_thread", worker_threads = 8)]
async fn main() -> anyhow::Result<()> {
    let n: usize = std::env::var("N").ok().and_then(|s| s.parse().ok()).unwrap_or(10);
    let secs: u64 = std::env::var("SECS").ok().and_then(|s| s.parse().ok()).unwrap_or(60);
    let server = std::env::var("SERVER")
        .unwrap_or_else(|_| "http://127.0.0.1:8123/graphql/subscriptions".into());
    let metrics_url =
        std::env::var("METRICS").unwrap_or_else(|_| "http://127.0.0.1:9184/metrics".into());
    let out_dir = std::env::var("OUT").unwrap_or_else(|_| "/tmp/subbench_testnet".into());
    let name = std::env::var("NAME").unwrap_or_else(|_| format!("n{n}"));
    std::fs::create_dir_all(&out_dir)?;

    let stop = Arc::new(AtomicBool::new(false));
    let delivered = Arc::new(AtomicU64::new(0));
    let mut handles = Vec::with_capacity(n);
    for _ in 0..n {
        handles.push(tokio::spawn(subscriber(
            server.clone(),
            delivered.clone(),
            stop.clone(),
        )));
    }
    tokio::time::sleep(Duration::from_secs(2)).await;

    let client = reqwest::Client::new();
    let path = format!("{out_dir}/{name}.csv");
    let mut f = std::fs::File::create(&path)?;
    writeln!(f, "t_ms,active,opened,delivered,lag_sum,lag_count,term_lagged,client_delivered,rss_kb")?;

    eprintln!("[client] {name}: {n} subscribers -> {server} for {secs}s");
    let start = Instant::now();
    while start.elapsed() < Duration::from_secs(secs) {
        let text = client
            .get(&metrics_url)
            .send()
            .await
            .and_then(|r| r.error_for_status())
            .ok();
        let text = match text {
            Some(r) => r.text().await.unwrap_or_default(),
            None => String::new(),
        };
        let lag = "graphql_subscription_payload_delivery_checkpoint_timestamp_lag";
        writeln!(
            f,
            "{},{},{},{},{},{},{},{},{}",
            start.elapsed().as_millis(),
            metric_sum(&text, "graphql_subscription_active_subscriptions") as i64,
            metric_sum(&text, "graphql_subscription_opened") as u64,
            metric_sum(&text, "graphql_subscription_payloads_delivered") as u64,
            metric_sum(&text, &format!("{lag}_sum")),
            metric_sum(&text, &format!("{lag}_count")) as u64,
            terminations_lagged(&text) as u64,
            delivered.load(Ordering::Relaxed),
            server_rss_kb(),
        )?;
        f.flush().ok();
        tokio::time::sleep(Duration::from_millis(1000)).await;
    }

    stop.store(true, Ordering::Relaxed);
    for h in handles {
        let _ = h.await;
    }
    eprintln!(
        "[client] {name}: done, client_delivered={} -> {path}",
        delivered.load(Ordering::Relaxed)
    );
    Ok(())
}
