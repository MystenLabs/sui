// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::{Context, Result};
use clap::Parser;
use mysten_metrics::spawn_monitored_task;
use sui_indexer_alt::{args::Args, ingestion::IngestionService, metrics::MetricsService};
use tokio::{signal, task::JoinHandle};
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
    let mut ingestion_service =
        IngestionService::new(args.ingestion, metrics.clone(), cancel.clone())?;

    let metrics_handle = metrics_service
        .run()
        .await
        .context("Failed to start metrics service")?;

    let ingester_handle = digest_ingester(&mut ingestion_service, cancel.clone());

    let ingestion_handle = ingestion_service
        .run()
        .await
        .context("Failed to start ingestion service")?;

    // Once we receive a Ctrl-C, notify all services to shutdown, and wait for them to finish.
    signal::ctrl_c().await.unwrap();
    cancel.cancel();
    ingester_handle.await.unwrap();
    metrics_handle.await.unwrap();
    ingestion_handle.await.unwrap();

    Ok(())
}

/// Test ingester which logs the digests of checkpoints it receives.
fn digest_ingester(ingestion: &mut IngestionService, cancel: CancellationToken) -> JoinHandle<()> {
    let mut rx = ingestion.subscribe();
    spawn_monitored_task!(async move {
        info!("Starting checkpoint digest ingester");
        loop {
            tokio::select! {
                _ = cancel.cancelled() => break,
                Some(checkpoint) = rx.recv() => {
                    let cp = checkpoint.checkpoint_summary.sequence_number;
                    let digest = checkpoint.checkpoint_summary.content_digest;
                    info!("{cp}: {digest}");
                }
            }
        }
        info!("Shutdown received, stopping digest ingester");
    })
}
