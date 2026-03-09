// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::path::PathBuf;

use anyhow::Result;
use clap::Parser;
use prometheus::Registry;
use sui_futures::service::Error;
use sui_indexer_alt_framework::IndexerArgs;
use sui_indexer_alt_framework::ingestion::ClientArgs;
use sui_indexer_alt_metrics::MetricsArgs;
use tracing::info;

use sui_analytics_indexer::IndexerConfig;
use sui_analytics_indexer::build_analytics_indexer;
use sui_analytics_indexer::metrics::Metrics;

#[derive(Parser)]
#[command(name = "sui-analytics-indexer")]
struct Args {
    /// Path to YAML config file
    #[arg(long)]
    config: PathBuf,

    #[command(flatten)]
    client_args: ClientArgs,

    #[command(flatten)]
    indexer_args: IndexerArgs,

    #[command(flatten)]
    metrics_args: MetricsArgs,
}

#[tokio::main]
async fn main() -> Result<()> {
    let _guard = telemetry_subscribers::TelemetryConfig::new()
        .with_env()
        .init();

    let args = Args::parse();

    let config: IndexerConfig = serde_yaml::from_str(&std::fs::read_to_string(&args.config)?)?;
    info!("Parsed config: {:#?}", config);

    let is_bounded_job = args.indexer_args.last_checkpoint.is_some();

    let registry = Registry::new();
    let metrics_service =
        sui_indexer_alt_metrics::MetricsService::new(args.metrics_args, registry.clone());

    let metrics = Metrics::new(&registry);

    let service = build_analytics_indexer(
        config,
        args.indexer_args,
        args.client_args,
        metrics,
        registry,
    )
    .await?;

    let s_metrics = metrics_service.run().await?;

    match service.attach(s_metrics).main().await {
        Ok(()) => {
            info!("Indexer completed successfully");
        }
        Err(Error::Terminated) => {
            info!("Received termination signal");
            if is_bounded_job {
                std::process::exit(1);
            }
        }
        Err(Error::Aborted) => {
            std::process::exit(1);
        }
        Err(Error::Task(_)) => {
            std::process::exit(2);
        }
    }

    Ok(())
}
