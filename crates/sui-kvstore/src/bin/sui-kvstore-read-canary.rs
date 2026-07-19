// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Throwaway read-latency canary: issues steady-state random point reads
//! against the kvstore `transactions` table so `kv_get_latency_ms` reflects
//! read latency under live write load. Not part of any deployment workflow.

use std::path::PathBuf;
use std::str::FromStr;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use anyhow::{Context, Result};
use clap::Parser;
use rand::Rng;
use sui_indexer_alt_metrics::{MetricsArgs, MetricsService};
use sui_kvstore::{BigTableClient, PoolConfig};
use sui_types::digests::TransactionDigest;
use telemetry_subscribers::TelemetryConfig;
use tracing::{info, warn};

#[derive(Parser)]
#[command(name = "sui-kvstore-read-canary")]
#[command(about = "Steady-state Bigtable point-read canary for kvstore")]
struct Args {
    /// BigTable instance ID
    instance_id: String,

    /// GCP project ID for the BigTable instance
    #[arg(long)]
    bigtable_project: Option<String>,

    /// File with one base58 transaction digest per line
    #[arg(long)]
    corpus: PathBuf,

    /// Point reads issued per second
    #[arg(long, default_value_t = 20.0)]
    rps: f64,

    /// Max concurrent in-flight reads; ticks beyond this are skipped and counted
    #[arg(long, default_value_t = 16)]
    max_inflight: usize,

    #[command(flatten)]
    metrics_args: MetricsArgs,
}

#[tokio::main]
async fn main() -> Result<()> {
    rustls::crypto::ring::default_provider()
        .install_default()
        .expect("Failed to install rustls crypto provider");

    let _guard = TelemetryConfig::new().with_env().init();
    let args = Args::parse();

    let corpus: Vec<TransactionDigest> = std::fs::read_to_string(&args.corpus)
        .with_context(|| format!("reading corpus {}", args.corpus.display()))?
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(|line| {
            TransactionDigest::from_str(line)
                .with_context(|| format!("invalid digest line {line:?}"))
        })
        .collect::<Result<_>>()?;
    anyhow::ensure!(!corpus.is_empty(), "corpus is empty");
    info!(digests = corpus.len(), rps = args.rps, "loaded corpus");

    let registry = prometheus::Registry::new();
    let skipped = prometheus::register_int_counter_with_registry!(
        "read_canary_skipped_total",
        "Read ticks skipped because max-inflight was reached",
        &registry,
    )
    .unwrap();
    let metrics_service = MetricsService::new(args.metrics_args, registry.clone());

    let client = BigTableClient::new_remote(
        args.instance_id,
        args.bigtable_project,
        true, // read-only OAuth scope
        None,
        None,
        "sui-kvstore-read-canary".to_string(),
        Some(&registry),
        None, // default app profile (single-cluster routing)
        PoolConfig::singleton(),
        false, // no batch-write flow control: this client never writes
    )
    .await?;

    let _metrics_handle = metrics_service.run().await?;

    let inflight = Arc::new(AtomicUsize::new(0));
    let mut interval = tokio::time::interval(Duration::from_secs_f64(1.0 / args.rps));
    interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    loop {
        interval.tick().await;
        if inflight.load(Ordering::Relaxed) >= args.max_inflight {
            skipped.inc();
            continue;
        }
        let digest = corpus[rand::thread_rng().gen_range(0..corpus.len())];
        let mut client = client.clone();
        let inflight = Arc::clone(&inflight);
        inflight.fetch_add(1, Ordering::Relaxed);
        tokio::spawn(async move {
            // Errors and not-founds are already counted by kv_get_errors /
            // kv_get_not_found inside the client; log for kubectl visibility.
            if let Err(error) = client.get_transactions_filtered(&[digest], None).await {
                warn!(%digest, "canary read failed: {error:#}");
            }
            inflight.fetch_sub(1, Ordering::Relaxed);
        });
    }
}
