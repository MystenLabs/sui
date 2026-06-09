// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Backtest the execution layer against historical mainnet data: re-execute past transactions under
//! the current execution rules and report where the recomputed result diverges from what was
//! recorded on chain. Useful for measuring the behavioral impact of an execution/protocol change
//! before it ships.
//!
//! For each epoch it resolves the checkpoint range + protocol version from a fullnode, then streams
//! the checkpoints through a two-stage pipeline (see [`pipeline`]): a prefetch stage fetches +
//! indexes checkpoints with `--concurrency` in flight, feeding a bounded buffer that an execute
//! stage drains, re-executing every programmable transaction matching `--status` (success / failed
//! / all) against reconstructed state via the `sui-execution` Executor on blocking workers
//! (`--execute-concurrency` at a time). Decoupling fetch from execute keeps the cores fed
//! regardless of fetch latency — fetching is the usual bottleneck, so prefer a remote object store
//! (`--remote-store-url https://checkpoints.<network>.sui.io`) over a fullnode `--rpc-api-url` for
//! the checkpoint source. Any transaction whose recomputed success/failure status disagrees with
//! its on-chain status is a *divergence*: it is written to an NDJSON file tagged with its on-chain
//! status and the recomputed error (if any). `--status success` is the strict baseline (a tx that
//! succeeded on chain now erroring); `all`/`failed` also replay failures, with each record carrying
//! the on-chain status so the differential can be applied downstream.

mod execute;
mod grpc;
mod pipeline;
mod store;

use std::io::Write as _;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;

use anyhow::{Context as _, Result, bail};
use clap::{Parser, ValueEnum};
use futures::StreamExt as _;
use prometheus::Registry;
use sui_indexer_alt_framework::ingestion::ingestion_client::{
    IngestionClient, IngestionClientArgs,
};
use sui_indexer_alt_framework::metrics::IngestionMetrics;
use sui_protocol_config::Chain;
use sui_types::metrics::ExecutionMetrics;
use tracing::info;
use url::Url;

use crate::execute::CheckpointStats;
use crate::grpc::RpcClient;
use crate::pipeline::{
    pipeline_channel, resolve_epoch_work, spawn_producer, stream_to_execution_results,
};
use crate::store::PackageCache;

/// Which on-chain transaction statuses to re-execute.
#[derive(Clone, Copy, Debug, PartialEq, Eq, ValueEnum)]
pub(crate) enum StatusFilter {
    /// Only transactions that succeeded on-chain (strict differential baseline).
    Success,
    /// Only transactions that failed on-chain.
    Failed,
    /// Both; divergence records carry their on-chain status/failure kind for downstream filtering.
    All,
}

#[derive(Parser)]
#[clap(
    name = "sui-execution-backtest",
    about = "Re-execute historical transactions and report divergences from their on-chain effects."
)]
struct Args {
    #[clap(flatten)]
    ingestion: IngestionClientArgs,

    /// Sui fullnode gRPC url used to resolve epochs and fetch packages. Defaults to `--rpc-api-url`
    /// when that is provided as the checkpoint source.
    #[clap(long)]
    fullnode_url: Option<Url>,

    /// First epoch to scan (inclusive).
    #[clap(long)]
    start_epoch: u64,

    /// Last epoch to scan (inclusive).
    #[clap(long)]
    end_epoch: u64,

    /// Optional cap on the number of checkpoints to process per epoch (for bounded samples).
    #[clap(long)]
    max_checkpoints_per_epoch: Option<u64>,

    /// Which on-chain transaction statuses to re-execute. `success` keeps the strict baseline (only
    /// txns that succeeded on-chain, so any divergence is a clear regression); `failed` runs only
    /// failed txns; `all` runs both and tags each divergence record with its `original_status`/
    /// failure kind so the differential can be applied in the analysis layer.
    #[clap(long, value_enum, default_value_t = StatusFilter::All)]
    status: StatusFilter,

    /// I/O width: checkpoints fetched + indexed concurrently by the prefetch stage. This is now
    /// purely the fetch pipeline width (execution width is `--execute-concurrency`). Fetching is the
    /// usual bottleneck, so prefer a remote object store —
    /// `--remote-store-url https://checkpoints.<network>.sui.io` — over a fullnode `--rpc-api-url`
    /// (measured ~+40% throughput), and raise this if the prefetch buffer keeps running dry.
    #[clap(long, default_value_t = 32)]
    concurrency: usize,

    /// CPU width: transactions executed concurrently on blocking workers. Defaults to ~2x the
    /// machine's parallelism (each unit is one transaction, and per-transaction prep leaves some
    /// slack, so mild oversubscription keeps the cores busy; measured best around 2-2.5x cores).
    /// Decoupled from `--concurrency` so fetch latency can't starve the cores.
    #[clap(long)]
    execute_concurrency: Option<usize>,

    /// Depth of the prefetched-checkpoint buffer between the fetch and execute stages. A larger
    /// buffer absorbs fetch-latency bursts at the cost of memory (each buffered checkpoint holds its
    /// object set). Defaults to `--concurrency`.
    #[clap(long)]
    prefetch_depth: Option<usize>,

    /// Optional directory for an on-disk package cache (speeds up re-scans).
    #[clap(long)]
    cache: Option<PathBuf>,

    /// Path to write divergent transactions to, as newline-delimited JSON.
    #[clap(long)]
    output: PathBuf,
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt::init();

    let args = Args::parse();

    if args.start_epoch > args.end_epoch {
        bail!(
            "start_epoch ({}) must be <= end_epoch ({})",
            args.start_epoch,
            args.end_epoch
        );
    }
    if args.concurrency == 0 {
        bail!("--concurrency must be >= 1");
    }

    let fullnode_url: Url = args
        .fullnode_url
        .clone()
        .or_else(|| args.ingestion.rpc_api_url.clone())
        .context("provide --fullnode-url (or --rpc-api-url) for epoch and package resolution")?;
    let rpc = RpcClient::new(fullnode_url)?;

    let registry = Registry::new();
    let metrics = IngestionMetrics::new(None, &registry);
    let ingestion = Arc::new(IngestionClient::new(args.ingestion, metrics)?);
    // Execution metrics, shared across all epochs.
    let execution_metrics = Arc::new(ExecutionMetrics::new(&registry));

    let packages = Arc::new(PackageCache::new(
        rpc.clone(),
        tokio::runtime::Handle::current(),
        args.cache.clone(),
    )?);

    // Divergences are rare, so buffer the output and flush on each progress tick / at the end rather
    // than syscalling per record.
    let mut output = std::io::BufWriter::new(
        std::fs::File::create(&args.output)
            .with_context(|| format!("creating output file {}", args.output.display()))?,
    );

    // Learn the chain identifier (needed to build the right ProtocolConfig) from the first
    // checkpoint of the first epoch.
    let first_bounds = rpc
        .epoch_bounds(args.start_epoch)
        .await
        .with_context(|| format!("resolving epoch {}", args.start_epoch))?;
    let chain: Chain = ingestion
        .checkpoint(first_bounds.first_checkpoint)
        .await
        .with_context(|| format!("fetching checkpoint {}", first_bounds.first_checkpoint))?
        .chain_id
        .chain();

    let work = resolve_epoch_work(
        &rpc,
        &ingestion,
        chain,
        args.start_epoch..=args.end_epoch,
        args.max_checkpoints_per_epoch,
        first_bounds,
        &execution_metrics,
    )
    .await?;

    let total_work = work.len() as u64;
    let fetch_concurrency = args.concurrency;
    let execute_concurrency = args.execute_concurrency.unwrap_or_else(|| {
        // Each unit is one transaction running on a blocking worker; per-transaction prep leaves a
        // little slack, so target ~2x the cores to keep them busy without thrashing.
        std::thread::available_parallelism()
            .map(|n| n.get())
            .unwrap_or(8)
            .saturating_mul(2)
    });
    let prefetch_depth = args.prefetch_depth.unwrap_or(fetch_concurrency).max(1);
    let status = args.status;
    info!(
        total_checkpoints = total_work,
        fetch_concurrency, execute_concurrency, prefetch_depth, "starting scan"
    );

    let (tx, rx) = pipeline_channel(prefetch_depth);
    let producer = spawn_producer(
        work,
        ingestion.clone(),
        packages.clone(),
        fetch_concurrency,
        tx,
    );

    let mut totals = CheckpointStats::default();
    let mut processed: u64 = 0;
    let checkpoints_done = Arc::new(AtomicU64::new(0));
    let scan_start = Instant::now();
    let mut results =
        stream_to_execution_results(rx, status, execute_concurrency, checkpoints_done.clone());

    while let Some(stats) = results.next().await {
        totals.merge(stats);
        for record in totals.records.drain(..) {
            writeln!(output, "{record}").context("writing output record")?;
        }

        // `processed` counts pipeline units (transactions); checkpoints are tallied separately.
        processed += 1;
        if processed.is_multiple_of(5000) {
            output.flush().ok();
            let elapsed = scan_start.elapsed().as_secs_f64().max(1e-9);
            log_progress(
                checkpoints_done.load(Ordering::Relaxed),
                total_work,
                &totals,
                elapsed,
            );
        }
    }
    output.flush().context("flushing output")?;
    producer.await.ok();

    let elapsed = scan_start.elapsed().as_secs_f64().max(1e-9);
    log_summary(&totals, checkpoints_done.load(Ordering::Relaxed), elapsed);
    Ok(())
}

/// Periodic progress line during the scan.
fn log_progress(
    checkpoints_done: u64,
    total_checkpoints: u64,
    totals: &CheckpointStats,
    elapsed: f64,
) {
    info!(
        checkpoints_done,
        total_checkpoints,
        total_checked = totals.checked,
        total_divergences = totals.divergences,
        total_reconstruction_errors = totals.reconstruction_errors,
        total_fetch_errors = totals.fetch_errors,
        tx_per_s = format!("{:.0}", totals.checked as f64 / elapsed),
        cp_per_s = format!("{:.1}", checkpoints_done as f64 / elapsed),
        "progress"
    );
}

/// Final tally line at the end of the run.
fn log_summary(totals: &CheckpointStats, checkpoints_done: u64, elapsed: f64) {
    info!(
        total_checked = totals.checked,
        total_divergences = totals.divergences,
        total_reconstruction_errors = totals.reconstruction_errors,
        total_coin_reservation_skipped = totals.coin_reservation_skipped,
        total_fetch_errors = totals.fetch_errors,
        total_execute_skipped = totals.execute_skipped,
        total_gas_from_balance = totals.gas_from_balance,
        total_executed = totals.executed,
        total_cancellation_excluded = totals.cancellation_excluded,
        checkpoints_done,
        elapsed_s = format!("{:.0}", elapsed),
        tx_per_s = format!("{:.0}", totals.checked as f64 / elapsed),
        cp_per_s = format!("{:.1}", checkpoints_done as f64 / elapsed),
        "backtest complete"
    );
}
