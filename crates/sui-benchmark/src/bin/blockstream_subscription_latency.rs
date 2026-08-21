// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Measures the checkpoint-subscription delivery-latency difference between two full nodes.
//!
//! A single client subscribes to `SubscribeCheckpoints` on both a "primary" and a "baseline" full
//! node and records which node delivers each checkpoint (by sequence number) first. Because one
//! process times both streams, the per-checkpoint delta has no cross-node clock skew. This is the
//! *stream-consumer's* view of any head start — index-gated, unlike the raw execution latency.

use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Instant;

use anyhow::Context;
use anyhow::Result;
use clap::Parser;
use comfy_table::Table;
use prometheus::HistogramVec;
use prometheus::IntCounter;
use prometheus::IntCounterVec;
use prometheus::Registry;
use prometheus::register_histogram_vec_with_registry;
use prometheus::register_int_counter_vec_with_registry;
use prometheus::register_int_counter_with_registry;
use sui_rpc::field::FieldMask;
use sui_rpc::field::FieldMaskUtil;
use sui_rpc::proto::sui::rpc::v2::SubscribeCheckpointsRequest;
use sui_rpc::proto::sui::rpc::v2::subscription_service_client::SubscriptionServiceClient;
use tokio::sync::mpsc;
use tracing::info;

#[derive(Parser, Debug)]
#[command(
    name = "blockstream-subscription-latency",
    about = "Compare checkpoint-subscription delivery latency between two full nodes"
)]
struct Opts {
    /// gRPC URL of the primary full node (e.g. the consensus-observer node).
    #[arg(long)]
    primary_node_url: String,
    /// gRPC URL of the baseline full node (e.g. the state-sync node).
    #[arg(long)]
    baseline_node_url: String,
    /// Number of checkpoints delivered by both nodes to measure before stopping.
    #[arg(long, default_value_t = 200)]
    num_checkpoints: usize,
    /// Host for the Prometheus metrics scrape endpoint.
    #[arg(long, default_value = "127.0.0.1")]
    client_metric_host: String,
    /// Port for the Prometheus metrics scrape endpoint.
    #[arg(long, default_value_t = 8081)]
    client_metric_port: u16,
}

/// A checkpoint delivered by one node's stream, timestamped on arrival at this client.
struct Arrival {
    node: Node,
    sequence_number: u64,
    at: Instant,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Node {
    Primary,
    Baseline,
}

impl Node {
    fn as_str(self) -> &'static str {
        match self {
            Node::Primary => "primary",
            Node::Baseline => "baseline",
        }
    }
}

#[derive(Default)]
struct Pending {
    primary: Option<Instant>,
    baseline: Option<Instant>,
}

/// Prometheus metrics for live Grafana dashboards.
struct Metrics {
    /// Per-node delivery lag relative to whichever node delivered the checkpoint first (seconds).
    /// The winner records ~0; the loser records the delta, so the loser's quantiles are the head
    /// start distribution.
    lag_seconds: HistogramVec,
    /// Count of checkpoints for which each node delivered first (label `node`).
    first: IntCounterVec,
    matched: IntCounter,
}

impl Metrics {
    fn new(registry: &Registry) -> Self {
        Self {
            lag_seconds: register_histogram_vec_with_registry!(
                "blockstream_subscription_lag_seconds",
                "Per-node checkpoint delivery lag relative to the first node to deliver it",
                &["node"],
                mysten_metrics::LATENCY_SEC_BUCKETS.to_vec(),
                registry,
            )
            .unwrap(),
            first: register_int_counter_vec_with_registry!(
                "blockstream_subscription_first_total",
                "Checkpoints for which this node delivered first",
                &["node"],
                registry,
            )
            .unwrap(),
            matched: register_int_counter_with_registry!(
                "blockstream_subscription_matched_total",
                "Checkpoints delivered by both nodes and compared",
                registry,
            )
            .unwrap(),
        }
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    let opts = Opts::parse();

    let mut telemetry = telemetry_subscribers::TelemetryConfig::new();
    telemetry.log_string = Some("info".to_string());
    let _guard = telemetry.with_env().init();

    let registry_service = mysten_metrics::start_prometheus_server(
        format!("{}:{}", opts.client_metric_host, opts.client_metric_port)
            .parse::<SocketAddr>()
            .context("parsing metrics address")?,
    );
    let registry = registry_service.default_registry();
    mysten_metrics::init_metrics(&registry);
    registry
        .register(mysten_metrics::uptime_metric(
            "blockstream-subscription-latency",
            env!("CARGO_PKG_VERSION"),
            "unknown",
        ))
        .unwrap();
    let metric = Arc::new(Metrics::new(&registry));

    let (tx, mut rx) = mpsc::channel::<Arrival>(1024);
    let primary = tokio::spawn(subscribe(opts.primary_node_url, Node::Primary, tx.clone()));
    let baseline = tokio::spawn(subscribe(
        opts.baseline_node_url,
        Node::Baseline,
        tx.clone(),
    ));
    drop(tx);

    let mut pending: HashMap<u64, Pending> = HashMap::new();
    let mut deltas: Vec<f64> = Vec::with_capacity(opts.num_checkpoints);
    let mut primary_first = 0usize;

    while let Some(arrival) = rx.recv().await {
        let entry = pending.entry(arrival.sequence_number).or_default();
        match arrival.node {
            Node::Primary => entry.primary = Some(arrival.at),
            Node::Baseline => entry.baseline = Some(arrival.at),
        }
        let (Some(primary_at), Some(baseline_at)) = (entry.primary, entry.baseline) else {
            continue;
        };
        pending.remove(&arrival.sequence_number);

        let earliest = primary_at.min(baseline_at);
        let lag_primary = (primary_at - earliest).as_secs_f64();
        let lag_baseline = (baseline_at - earliest).as_secs_f64();
        // Signed delta in ms: positive means the primary node delivered sooner.
        let delta_ms = if baseline_at >= primary_at {
            (baseline_at - primary_at).as_secs_f64() * 1000.0
        } else {
            -((primary_at - baseline_at).as_secs_f64() * 1000.0)
        };
        let leader = if primary_at <= baseline_at {
            primary_first += 1;
            Node::Primary
        } else {
            Node::Baseline
        };

        metric
            .lag_seconds
            .with_label_values(&["primary"])
            .observe(lag_primary);
        metric
            .lag_seconds
            .with_label_values(&["baseline"])
            .observe(lag_baseline);
        metric.first.with_label_values(&[leader.as_str()]).inc();
        metric.matched.inc();
        info!(
            sequence_number = arrival.sequence_number,
            delta_ms,
            leader = leader.as_str(),
            "checkpoint delivered by both nodes"
        );

        deltas.push(delta_ms);
        if deltas.len() >= opts.num_checkpoints {
            break;
        }
    }

    primary.abort();
    baseline.abort();
    report(&deltas, primary_first);
    Ok(())
}

async fn subscribe(url: String, node: Node, tx: mpsc::Sender<Arrival>) -> Result<()> {
    let mut client = SubscriptionServiceClient::connect(url)
        .await
        .with_context(|| format!("connecting to {} node", node.as_str()))?;
    let mut request = SubscribeCheckpointsRequest::default();
    request.read_mask = Some(FieldMask::from_paths(["sequence_number"]));
    let mut stream = client.subscribe_checkpoints(request).await?.into_inner();
    while let Some(response) = stream.message().await? {
        if let Some(sequence_number) = response.cursor {
            let arrival = Arrival {
                node,
                sequence_number,
                at: Instant::now(),
            };
            if tx.send(arrival).await.is_err() {
                break;
            }
        }
    }
    Ok(())
}

fn report(deltas: &[f64], primary_first: usize) {
    if deltas.is_empty() {
        println!("no checkpoints were delivered by both nodes");
        return;
    }
    let mut sorted = deltas.to_vec();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let mean = deltas.iter().sum::<f64>() / deltas.len() as f64;

    let mut table = Table::new();
    table.set_header(vec!["metric", "value"]);
    table.add_row(vec![
        "checkpoints compared".to_string(),
        deltas.len().to_string(),
    ]);
    table.add_row(vec![
        "primary delivered first".to_string(),
        format!(
            "{primary_first} ({:.1}%)",
            100.0 * primary_first as f64 / deltas.len() as f64
        ),
    ]);
    for (label, value) in [
        ("delta mean (ms)", mean),
        ("delta p50 (ms)", percentile(&sorted, 0.50)),
        ("delta p90 (ms)", percentile(&sorted, 0.90)),
        ("delta p99 (ms)", percentile(&sorted, 0.99)),
        ("delta min (ms)", sorted[0]),
        ("delta max (ms)", sorted[sorted.len() - 1]),
    ] {
        table.add_row(vec![label.to_string(), format!("{value:.2}")]);
    }
    println!("{table}");
}

fn percentile(sorted: &[f64], p: f64) -> f64 {
    let rank = (p * (sorted.len() as f64 - 1.0)).round() as usize;
    sorted[rank.min(sorted.len() - 1)]
}
