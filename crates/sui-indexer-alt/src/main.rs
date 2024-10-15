// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::{Context, Result};
use clap::Parser;
use sui_indexer_alt::{
    args::Args,
    db::Db,
    handlers::{kv_checkpoints::KvCheckpoints, kv_objects::KvObjects, pipeline},
    ingestion::IngestionService,
    metrics::MetricsService,
    task::graceful_shutdown,
};
use tokio_util::sync::CancellationToken;

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();

    // Enable tracing, configured by environment variables.
    let _guard = telemetry_subscribers::TelemetryConfig::new()
        .with_env()
        .init();

    let cancel = CancellationToken::new();

    let db = Db::new(args.db)
        .await
        .context("Failed to connect to database")?;

    let (metrics, metrics_service) =
        MetricsService::new(args.metrics_address, db.clone(), cancel.clone())?;
    let mut ingestion_service =
        IngestionService::new(args.ingestion, metrics.clone(), cancel.clone())?;

    let h_metrics = metrics_service
        .run()
        .await
        .context("Failed to start metrics service")?;

    let (h_cp_handler, h_cp_committer) = pipeline::<KvCheckpoints>(
        db.clone(),
        ingestion_service.subscribe(),
        args.committer.clone(),
        metrics.clone(),
        cancel.clone(),
    );

    let (h_obj_handler, h_obj_committer) = pipeline::<KvObjects>(
        db.clone(),
        ingestion_service.subscribe(),
        args.committer.clone(),
        metrics.clone(),
        cancel.clone(),
    );

    let h_ingestion = ingestion_service
        .run()
        .await
        .context("Failed to start ingestion service")?;

    // Once we receive a Ctrl-C or one of the services panics or is cancelled, notify all services
    // to shutdown, and wait for them to finish.
    graceful_shutdown(
        [
            h_cp_handler,
            h_cp_committer,
            h_obj_handler,
            h_obj_committer,
            h_metrics,
            h_ingestion,
        ],
        cancel,
    )
    .await;

    Ok(())
}
