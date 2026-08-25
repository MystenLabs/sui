// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Local subscription benchmark (`#[ignore]`). Stands up the real streaming GraphQL server, drives N
//! concurrent SSE subscribers plus continuous transaction traffic, and samples the subscription
//! metrics registry + process RSS once a second to a per-scenario CSV under `$BENCH_OUT`.
//!
//! Run one scenario set with e.g.:
//! ```text
//! BENCH_OUT=/tmp/subbench BENCH_SECS=30 BENCH_SCENARIOS=verify \
//!   cargo nextest run -p sui-indexer-alt-e2e-tests --features staging \
//!   --test graphql_subscription bench_subscription_scaling --run-ignored all --no-capture
//! ```

use std::io::Write;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::sync::atomic::AtomicU64;
use std::sync::atomic::Ordering;
use std::time::Duration;
use std::time::Instant;

use bytes::BytesMut;
use futures::StreamExt;
use serde_json::json;

use super::testing::SubscriptionTestCluster;
use super::testing::transfer_coins;

const LIVE_QUERY: &str = "subscription { transactions { cursor node { digest } } }";

/// One metric snapshot pulled from the GraphQL registry.
#[derive(Default)]
struct Sample {
    active: i64,
    opened: u64,
    delivered: u64,
    lag_sum: f64,
    lag_count: u64,
    term_lagged: u64,
    term_client_closed: u64,
    term_error: u64,
    upstream_processed: u64,
}

fn extract(mfs: &[prometheus::proto::MetricFamily]) -> Sample {
    let mut s = Sample::default();
    for mf in mfs {
        match mf.name() {
            "graphql_subscription_active_subscriptions" => {
                for m in mf.get_metric() {
                    s.active += m.get_gauge().value() as i64;
                }
            }
            "graphql_subscription_opened" => {
                for m in mf.get_metric() {
                    s.opened += m.get_counter().value() as u64;
                }
            }
            "graphql_subscription_payloads_delivered" => {
                for m in mf.get_metric() {
                    s.delivered += m.get_counter().value() as u64;
                }
            }
            "graphql_subscription_payload_delivery_checkpoint_timestamp_lag" => {
                for m in mf.get_metric() {
                    let h = m.get_histogram();
                    s.lag_sum += h.get_sample_sum();
                    s.lag_count += h.get_sample_count();
                }
            }
            "graphql_subscription_terminations" => {
                for m in mf.get_metric() {
                    let reason = m
                        .get_label()
                        .iter()
                        .find(|l| l.name() == "reason")
                        .map(|l| l.value())
                        .unwrap_or("");
                    let v = m.get_counter().value() as u64;
                    match reason {
                        "lagged" => s.term_lagged += v,
                        "client_closed" => s.term_client_closed += v,
                        "error" => s.term_error += v,
                        _ => {}
                    }
                }
            }
            "graphql_subscription_upstream_processed_checkpoints" => {
                for m in mf.get_metric() {
                    s.upstream_processed += m.get_counter().value() as u64;
                }
            }
            _ => {}
        }
    }
    s
}

/// Resident set size of this process in KB (whole process: server + in-process clients).
fn rss_kb() -> u64 {
    let pid = std::process::id().to_string();
    std::process::Command::new("ps")
        .args(["-o", "rss=", "-p", &pid])
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .and_then(|s| s.trim().parse().ok())
        .unwrap_or(0)
}

/// One SSE subscriber: streams the subscription, counting delivered payloads (`event: next` frames)
/// into `delivered` until `stop` is set. Holds only the URL, so it never borrows the cluster.
async fn run_subscriber(
    url: String,
    query: String,
    delivered: Arc<AtomicU64>,
    stop: Arc<AtomicBool>,
    resume_after: Option<u64>,
) {
    let body = if let Some(cp) = resume_after {
        let q = format!(
            "subscription {{ transactions(filter: {{ afterCheckpoint: {cp} }}) {{ cursor node {{ digest }} }} }}"
        );
        json!({ "query": q })
    } else {
        json!({ "query": query })
    };

    let resp = match reqwest::Client::new()
        .post(&url)
        .header("Accept", "text/event-stream")
        .header("Content-Type", "application/json")
        .json(&body)
        .send()
        .await
    {
        Ok(r) if r.status().is_success() => r,
        _ => return,
    };

    let mut stream = resp.bytes_stream();
    let mut buf = BytesMut::new();
    let mut event: Option<String> = None;
    let mut data = String::new();

    while !stop.load(Ordering::Relaxed) {
        let chunk = match tokio::time::timeout(Duration::from_millis(500), stream.next()).await {
            Ok(Some(Ok(c))) => c,
            Ok(Some(Err(_))) | Ok(None) => break,
            Err(_) => continue,
        };
        buf.extend_from_slice(&chunk);
        while let Some(pos) = buf.iter().position(|&b| b == b'\n') {
            let line_bytes = buf.split_to(pos + 1);
            let line = std::str::from_utf8(&line_bytes[..line_bytes.len() - 1])
                .unwrap_or("")
                .trim_end_matches('\r');
            if line.is_empty() {
                if event.as_deref() == Some("next") && !data.is_empty() {
                    delivered.fetch_add(1, Ordering::Relaxed);
                }
                event = None;
                data.clear();
            } else if let Some(rest) = line.strip_prefix("event:") {
                event = Some(rest.trim().to_string());
            } else if let Some(rest) = line.strip_prefix("data:") {
                data.push_str(rest.trim_start());
            }
        }
    }
}

/// Run one scenario: spawn `n` subscribers, then for `dur` drive tx traffic and sample metrics every
/// second into `<out_dir>/<name>.csv`. `resume_after` makes subscribers backfill from a checkpoint.
async fn run_scenario(
    cluster: &mut SubscriptionTestCluster,
    name: &str,
    specs: Vec<Option<u64>>,
    dur: Duration,
    out_dir: &str,
) {
    let n = specs.len();
    let stop = Arc::new(AtomicBool::new(false));
    let delivered = Arc::new(AtomicU64::new(0));
    let mut handles = Vec::with_capacity(n);
    for resume_after in specs {
        handles.push(tokio::spawn(run_subscriber(
            cluster.subscription_url.clone(),
            LIVE_QUERY.to_string(),
            delivered.clone(),
            stop.clone(),
            resume_after,
        )));
    }
    tokio::time::sleep(Duration::from_secs(2)).await;

    let path = format!("{out_dir}/{name}.csv");
    let mut f = std::fs::File::create(&path).expect("create csv");
    writeln!(
        f,
        "t_ms,active,opened,delivered,lag_sum,lag_count,term_lagged,term_client_closed,term_error,upstream_processed,client_delivered,rss_kb"
    )
    .unwrap();

    let start = Instant::now();
    while start.elapsed() < dur {
        let _ = transfer_coins(&mut cluster.validator, &[1_000_000; 4]).await;
        let s = extract(&cluster.gather_metrics());
        let client_delivered = delivered.load(Ordering::Relaxed);
        writeln!(
            f,
            "{},{},{},{},{},{},{},{},{},{},{},{}",
            start.elapsed().as_millis(),
            s.active,
            s.opened,
            s.delivered,
            s.lag_sum,
            s.lag_count,
            s.term_lagged,
            s.term_client_closed,
            s.term_error,
            s.upstream_processed,
            client_delivered,
            rss_kb(),
        )
        .unwrap();
        f.flush().ok();
        tokio::time::sleep(Duration::from_millis(1000)).await;
    }

    stop.store(true, Ordering::Relaxed);
    for h in handles {
        let _ = h.await;
    }
    eprintln!(
        "[bench] {name}: N={n} delivered(client)={} over {}s",
        delivered.load(Ordering::Relaxed),
        dur.as_secs()
    );
}

/// Ramp subscribers up to `target` at ~`per_tick` per sample tick (a paced connect, not a burst),
/// then keep sampling for the rest of `dur`. Shows whether N connect and stay reliably served.
async fn run_ramp(
    cluster: &mut SubscriptionTestCluster,
    name: &str,
    target: usize,
    per_tick: usize,
    dur: Duration,
    out_dir: &str,
) {
    let stop = Arc::new(AtomicBool::new(false));
    let delivered = Arc::new(AtomicU64::new(0));
    let mut handles = Vec::with_capacity(target);

    let path = format!("{out_dir}/{name}.csv");
    let mut f = std::fs::File::create(&path).expect("create csv");
    writeln!(
        f,
        "t_ms,active,opened,delivered,lag_sum,lag_count,term_lagged,term_client_closed,term_error,upstream_processed,client_delivered,rss_kb"
    )
    .unwrap();

    let start = Instant::now();
    while start.elapsed() < dur {
        for _ in 0..per_tick {
            if handles.len() >= target {
                break;
            }
            handles.push(tokio::spawn(run_subscriber(
                cluster.subscription_url.clone(),
                LIVE_QUERY.to_string(),
                delivered.clone(),
                stop.clone(),
                None,
            )));
        }
        let _ = transfer_coins(&mut cluster.validator, &[1_000_000; 4]).await;
        let s = extract(&cluster.gather_metrics());
        writeln!(
            f,
            "{},{},{},{},{},{},{},{},{},{},{},{}",
            start.elapsed().as_millis(),
            s.active,
            s.opened,
            s.delivered,
            s.lag_sum,
            s.lag_count,
            s.term_lagged,
            s.term_client_closed,
            s.term_error,
            s.upstream_processed,
            delivered.load(Ordering::Relaxed),
            rss_kb(),
        )
        .unwrap();
        f.flush().ok();
        tokio::time::sleep(Duration::from_millis(1000)).await;
    }
    stop.store(true, Ordering::Relaxed);
    for h in handles {
        let _ = h.await;
    }
    eprintln!(
        "[bench] {name}: spawned={} client_delivered={}",
        target,
        delivered.load(Ordering::Relaxed)
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 12)]
#[ignore = "benchmark; run explicitly with --run-ignored"]
async fn bench_subscription_scaling() {
    let out_dir = std::env::var("BENCH_OUT").unwrap_or_else(|_| "/tmp/subbench".to_string());
    std::fs::create_dir_all(&out_dir).unwrap();
    let secs = std::env::var("BENCH_SECS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(120u64);
    let dur = Duration::from_secs(secs);

    let mut cluster = SubscriptionTestCluster::new_with_ledger_history().await;

    let set = std::env::var("BENCH_SCENARIOS").unwrap_or_else(|_| "all".to_string());
    match set.as_str() {
        "verify" => {
            run_scenario(&mut cluster, "baseline_1", vec![None; 1], dur, &out_dir).await;
        }
        "backfill" => {
            // Build history first, then subscribers resume from checkpoint 0.
            for _ in 0..50 {
                let _ = transfer_coins(&mut cluster.validator, &[1_000_000; 4]).await;
            }
            run_scenario(
                &mut cluster,
                "backfill_50",
                vec![Some(0); 50],
                dur,
                &out_dir,
            )
            .await;
        }
        "stress" => {
            run_scenario(&mut cluster, "stress_500", vec![None; 500], dur, &out_dir).await;
        }
        "ramp" => {
            // Paced connect: ~30 new subscribers per second up to 1000, then hold.
            run_ramp(&mut cluster, "ramp_1000", 1000, 30, dur, &out_dir).await;
        }
        "mixed" => {
            for _ in 0..40 {
                let _ = transfer_coins(&mut cluster.validator, &[1_000_000; 4]).await;
            }
            let mut specs = vec![None; 50];
            specs.extend(vec![Some(0); 50]);
            run_scenario(
                &mut cluster,
                "mixed_50live_50backfill",
                specs,
                dur,
                &out_dir,
            )
            .await;
        }
        _ => {
            run_scenario(&mut cluster, "baseline_1", vec![None; 1], dur, &out_dir).await;
            run_scenario(&mut cluster, "live_10", vec![None; 10], dur, &out_dir).await;
            run_scenario(&mut cluster, "live_100", vec![None; 100], dur, &out_dir).await;
        }
    }
}
