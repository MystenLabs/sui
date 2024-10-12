// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::{Context, Result};
use clap::Parser;
use sui_indexer_alt::{args::Args, ingestion::IngestionClient, metrics::MetricsService};
use tokio::signal;
use tokio_util::sync::CancellationToken;
use tracing::info;

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();

    // Enable tracing, configured by environment variables.
    let _guard = telemetry_subscribers::TelemetryConfig::new()
        .with_env()
        .init();

    let cancel = CancellationToken::new();

    let (metrics, metrics_service) = MetricsService::new(args.metrics_address, cancel.clone())?;

    let metrics_handle = metrics_service
        .run()
        .await
        .context("Failed to start metrics service")?;

    info!("Fetching {}", args.remote_store_url);

    let client = IngestionClient::new(args.remote_store_url, metrics.clone())?;
    let checkpoint = client.fetch(args.start).await?;

    info!(
        txs = checkpoint.transactions.len(),
        evs = checkpoint
            .transactions
            .iter()
            .map(|tx| tx.events.as_ref().map_or(0, |evs| evs.data.len()))
            .sum::<usize>(),
        ins = checkpoint
            .transactions
            .iter()
            .map(|tx| tx.input_objects.len())
            .sum::<usize>(),
        outs = checkpoint
            .transactions
            .iter()
            .map(|tx| tx.output_objects.len())
            .sum::<usize>(),
        "Fetch checkpoint {}",
        args.start,
    );

    // Once we receive a Ctrl-C, notify all services to shutdown, and wait for them to finish.
    signal::ctrl_c().await.unwrap();
    cancel.cancel();
    metrics_handle.await.unwrap();
    Ok(())
}
