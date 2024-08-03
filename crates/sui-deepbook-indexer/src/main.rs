// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::Result;
use clap::*;
use mysten_metrics::start_prometheus_server;
use std::env;
use std::path::PathBuf;
use sui_config::Config;
use sui_data_ingestion_core::DataIngestionMetrics;
use sui_deepbook_indexer::config::IndexerConfig;
use sui_deepbook_indexer::deepbook::metrics::DeepbookIndexerMetrics;
use sui_deepbook_indexer::postgres_manager::get_connection_pool;
use sui_deepbook_indexer::sui_checkpoint_syncer::SuiCheckpointSyncer;

use tracing::info;

#[derive(Parser, Clone, Debug)]
struct Args {
    /// Path to a yaml config
    #[clap(long, short)]
    config_path: Option<PathBuf>,
}

#[tokio::main]
async fn main() -> Result<()> {
    let _guard = telemetry_subscribers::TelemetryConfig::new()
        .with_env()
        .init();

    let args = Args::parse();

    // load config
    let config_path = if let Some(path) = args.config_path {
        path
    } else {
        env::current_dir()
            .expect("Couldn't get current directory")
            .join("config.yaml")
    };
    let config = IndexerConfig::load(&config_path)?;

    // Init metrics server
    let registry_service = start_prometheus_server(
        format!("{}:{}", config.metric_url, config.metric_port,)
            .parse()
            .unwrap_or_else(|err| panic!("Failed to parse metric address: {}", err)),
    );
    let registry = registry_service.default_registry();

    mysten_metrics::init_metrics(&registry);

    info!(
        "Metrics server started at {}::{}",
        config.metric_url, config.metric_port
    );
    let indexer_meterics = DeepbookIndexerMetrics::new(&registry);
    let ingestion_metrics = DataIngestionMetrics::new(&registry);
    let db_url = config.db_url.clone();

    let pg_pool = get_connection_pool(db_url.clone());
    SuiCheckpointSyncer::new(
        pg_pool,
        config.start_checkpoint.unwrap_or(0),
        config.end_checkpoint.unwrap_or(u64::MAX),
    )
    .start(&config, indexer_meterics, ingestion_metrics)
    .await?;

    Ok(())
}
