// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! New kvstore binary using sui-indexer-alt-framework.
//!
//! This binary can run alongside the legacy kvstore binary during migration.
//! Both write to the same BigTable tables and share the same watermark.

use std::time::Duration;

use anyhow::Result;
use clap::Parser;
use sui_indexer_alt_framework::ingestion::{ClientArgs, IngestionConfig};
use sui_indexer_alt_framework::pipeline::CommitterConfig;
use sui_indexer_alt_framework::pipeline::concurrent::ConcurrentConfig;
use sui_indexer_alt_framework::{Indexer, IndexerArgs};
use sui_indexer_alt_metrics::MetricsArgs;
use sui_kvstore::{BigTableClient, BigTableStore, KvStorePipeline};
use telemetry_subscribers::TelemetryConfig;
use tracing::info;

#[derive(Parser)]
#[command(name = "sui-kvstore-alt")]
#[command(about = "KVStore indexer using sui-indexer-alt-framework")]
struct Args {
    /// BigTable instance ID (e.g., "projects/myproject/instances/myinstance")
    instance_id: String,

    /// Number of concurrent checkpoint writes
    #[arg(long, default_value = "10")]
    write_concurrency: usize,

    /// Interval between watermark updates
    #[arg(long, default_value = "1m", value_parser = humantime::parse_duration)]
    watermark_interval: Duration,

    #[command(flatten)]
    metrics_args: MetricsArgs,

    #[command(flatten)]
    client_args: ClientArgs,

    #[command(flatten)]
    indexer_args: IndexerArgs,
}

#[tokio::main]
async fn main() -> Result<()> {
    let _guard = TelemetryConfig::new().with_env().init();

    let args = Args::parse();

    info!("Starting sui-kvstore-alt indexer");
    info!(instance_id = %args.instance_id);

    // Create BigTable client
    let client = BigTableClient::new_remote(
        args.instance_id,
        false, // write mode
        None,
        "sui-kvstore-alt".to_string(),
        None,
        None,
    )
    .await?;

    // Create store
    let store = BigTableStore::new(client);

    // Set up metrics
    let registry = prometheus::Registry::new_custom(Some("kvstore_alt".into()), None)?;
    let metrics_service =
        sui_indexer_alt_metrics::MetricsService::new(args.metrics_args, registry.clone());

    // Create indexer
    let mut indexer = Indexer::new(
        store,
        args.indexer_args,
        args.client_args,
        IngestionConfig::default(),
        None,
        &registry,
    )
    .await?;

    // Register the kvstore pipeline
    let config = ConcurrentConfig {
        committer: CommitterConfig {
            write_concurrency: args.write_concurrency,
            watermark_interval_ms: args.watermark_interval.as_millis() as u64,
            ..Default::default()
        },
        ..Default::default()
    };
    indexer.concurrent_pipeline(KvStorePipeline, config).await?;

    info!("Indexer created");

    // Run the indexer
    let metrics_handle = metrics_service.run().await?;
    let service = indexer.run().await?;
    service.attach(metrics_handle).main().await?;

    Ok(())
}
