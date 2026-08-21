// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Separate-process subscription load client for `examples/serve_testnet`. Opens N SSE subscriptions,
//! half backfilling from genesis and half live, scrapes the server's `/metrics` out-of-process, and
//! samples isolated server RSS + CPU to a CSV.
//!
//! ```text
//! N=100 SECS=60 NAME=mix_100 OUT=/tmp/subbench_testnet \
//!   cargo run --release --example subscribe_bench -p sui-indexer-alt-graphql
//! ```
//!
//! Subscribers alternate by index: even = backfill from checkpoint 0, odd = live from the tip.
//! `client_delivered` splits into `delivered_backfill` / `delivered_live` so the two phases can be
//! reported separately (the server metric under-counts backfill deliveries).

use std::io::Write;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::sync::atomic::AtomicU64;
use std::sync::atomic::Ordering;
use std::time::Duration;
use std::time::Instant;

const LIVE_QUERY: &str = "subscription { transactions { cursor node { digest } } }";
const BACKFILL_QUERY: &str =
    "subscription { transactions(filter: { afterCheckpoint: 0 }) { cursor node { digest } } }";

/// One SSE subscriber: counts delivered payloads (by `"digest"` occurrences) into `delivered` until
/// `stop`. `query` selects live vs backfill-from-genesis.
async fn subscriber(
    url: String,
    query: &'static str,
    delivered: Arc<AtomicU64>,
    stop: Arc<AtomicBool>,
) {
    let mut resp = match reqwest::Client::new()
        .post(&url)
        .header("Accept", "text/event-stream")
        .header("Content-Type", "application/json")
        .json(&serde_json::json!({ "query": query }))
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

/// RSS (KB) and CPU% of the standalone server process, so both are server-only (not this client's).
/// One `ps` call returns `rss %cpu`; `(0, 0.0)` if the process isn't found.
fn server_rss_cpu() -> (u64, f64) {
    let out = std::process::Command::new("pgrep")
        .args(["-x", "serve_testnet"])
        .output()
        .ok();
    let pid = out
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .and_then(|s| s.lines().next().map(|l| l.trim().to_string()));
    let Some(pid) = pid else { return (0, 0.0) };
    let text = std::process::Command::new("ps")
        .args(["-o", "rss=,%cpu=", "-p", &pid])
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .unwrap_or_default();
    let mut it = text.split_whitespace();
    let rss = it.next().and_then(|v| v.parse().ok()).unwrap_or(0);
    let cpu = it.next().and_then(|v| v.parse().ok()).unwrap_or(0.0);
    (rss, cpu)
}

#[tokio::main(flavor = "multi_thread", worker_threads = 12)]
async fn main() -> anyhow::Result<()> {
    let n: usize = std::env::var("N").ok().and_then(|s| s.parse().ok()).unwrap_or(10);
    let secs: u64 = std::env::var("SECS").ok().and_then(|s| s.parse().ok()).unwrap_or(60);
    // Connections opened per second (0 = all at once). Paced opening avoids a thundering-herd connect
    // burst that saturates the client runtime and starves the sampling loop at high N.
    let ramp: usize = std::env::var("RAMP").ok().and_then(|s| s.parse().ok()).unwrap_or(0);
    // Percent of subscribers that backfill (rest are live). 100 = all backfill, 0 = all live, 50 = interleaved.
    let bf_pct: usize = std::env::var("BACKFILL_PCT").ok().and_then(|s| s.parse().ok()).unwrap_or(50);
    let server = std::env::var("SERVER")
        .unwrap_or_else(|_| "http://127.0.0.1:8123/graphql/subscriptions".into());
    let metrics_url =
        std::env::var("METRICS").unwrap_or_else(|_| "http://127.0.0.1:9184/metrics".into());
    let out_dir = std::env::var("OUT").unwrap_or_else(|_| "/tmp/subbench_testnet".into());
    let name = std::env::var("NAME").unwrap_or_else(|_| format!("n{n}"));
    std::fs::create_dir_all(&out_dir)?;

    let stop = Arc::new(AtomicBool::new(false));
    let delivered_backfill = Arc::new(AtomicU64::new(0));
    let delivered_live = Arc::new(AtomicU64::new(0));
    let mut handles = Vec::with_capacity(n);
    for i in 0..n {
        // Interleave so a paced ramp opens backfill and live subscribers evenly over time.
        let backfill = if bf_pct >= 100 {
            true
        } else if bf_pct == 0 {
            false
        } else {
            i % 2 == 0
        };
        let (query, delivered) = if backfill {
            (BACKFILL_QUERY, delivered_backfill.clone())
        } else {
            (LIVE_QUERY, delivered_live.clone())
        };
        handles.push(tokio::spawn(subscriber(
            server.clone(),
            query,
            delivered,
            stop.clone(),
        )));
        // Pace opening: after each batch of `ramp` spawns, wait a second.
        if ramp > 0 && (i + 1) % ramp == 0 {
            tokio::time::sleep(Duration::from_secs(1)).await;
        }
    }
    tokio::time::sleep(Duration::from_secs(2)).await;

    let client = reqwest::Client::new();
    let path = format!("{out_dir}/{name}.csv");
    let mut f = std::fs::File::create(&path)?;
    writeln!(f, "t_ms,active,opened,delivered,lag_sum,lag_count,term_lagged,client_delivered,delivered_backfill,delivered_live,rss_kb,cpu_pct,processed_cp")?;

    eprintln!(
        "[client] {name}: {n} subscribers ({} backfill / {} live) -> {server} for {secs}s",
        n.div_ceil(2),
        n / 2,
    );
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
        let lag = "graphql_subscription_live_payload_delivery_checkpoint_timestamp_lag";
        let (rss_kb, cpu_pct) = server_rss_cpu();
        let d_backfill = delivered_backfill.load(Ordering::Relaxed);
        let d_live = delivered_live.load(Ordering::Relaxed);
        writeln!(
            f,
            "{},{},{},{},{},{},{},{},{},{},{},{},{}",
            start.elapsed().as_millis(),
            metric_sum(&text, "graphql_subscription_active_subscriptions") as i64,
            metric_sum(&text, "graphql_subscription_opened") as u64,
            metric_sum(&text, "graphql_subscription_payloads_delivered") as u64,
            metric_sum(&text, &format!("{lag}_sum")),
            metric_sum(&text, &format!("{lag}_count")) as u64,
            terminations_lagged(&text) as u64,
            d_backfill + d_live,
            d_backfill,
            d_live,
            rss_kb,
            cpu_pct,
            metric_sum(&text, "graphql_subscription_upstream_processed_checkpoints") as u64,
        )?;
        f.flush().ok();
        tokio::time::sleep(Duration::from_millis(1000)).await;
    }

    stop.store(true, Ordering::Relaxed);
    for h in handles {
        let _ = h.await;
    }
    eprintln!(
        "[client] {name}: done, backfill={} live={} -> {path}",
        delivered_backfill.load(Ordering::Relaxed),
        delivered_live.load(Ordering::Relaxed),
    );
    Ok(())
}
