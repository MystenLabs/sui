// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::Result;
use clap::*;
use mysten_metrics::start_prometheus_server;
use prometheus::Registry;
use std::env;
use std::path::PathBuf;
use sui_bridge_indexer::eth_worker::EthBridgeWorker;
use sui_bridge_indexer::postgres_manager::{get_connection_pool, PgProgressStore};
use sui_bridge_indexer::sui_worker::SuiBridgeWorker;
use sui_bridge_indexer::{config::load_config, metrics::BridgeIndexerMetrics};
use sui_data_ingestion_core::{DataIngestionMetrics, IndexerExecutor, ReaderOptions, WorkerPool};
use tokio::sync::oneshot;
use tracing::info;
// use sui_bridge::retry_with_max_elapsed_time;
// use tokio::time::Duration;

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
        Some(path)
    } else {
        Some(
            env::current_dir()
                .expect("Current directory is invalid.")
                .join("config.yaml"),
        )
    };
    let config_path = config_path.unwrap();
    let config = load_config(&config_path).unwrap();
    let config_clone = config.clone();

    // Init metrics server
    let registry_service = start_prometheus_server(
        format!("{}:{}", config.metric_url, config.metric_port,)
            .parse()
            .unwrap_or_else(|err| panic!("Failed to parse metric address: {}", err)),
    );
    let registry: Registry = registry_service.default_registry();

    mysten_metrics::init_metrics(&registry);

    info!(
        "Metrics server started at {}::{}",
        config.metric_url, config.metric_port
    );
    let indexer_meterics = BridgeIndexerMetrics::new(&registry);

    let eth_worker = EthBridgeWorker::new(
        get_connection_pool(config.db_url.clone()),
        indexer_meterics.clone(),
        config,
    );

    // TODO: retry_with_max_elapsed_time

    let unfinalized_handle = eth_worker.start_indexing_unfinalized_events();
    let finalized_handle = eth_worker.start_indexing_finalized_events();

    let _ = tokio::try_join!(finalized_handle, unfinalized_handle);

    // TODO: add retry_with_max_elapsed_time
    let _ = start_processing_sui_checkpoints(&config_clone, indexer_meterics.clone()).await;

    Ok(())
}

async fn start_processing_sui_checkpoints(
    config: &sui_bridge_indexer::config::Config,
    indexer_meterics: BridgeIndexerMetrics,
) -> Result<()> {
    // metrics init
    let (_exit_sender, exit_receiver) = oneshot::channel();
    let ingestion_metrics = DataIngestionMetrics::new(&Registry::new());

    let pg_pool = get_connection_pool(config.db_url.clone());
    let progress_store = PgProgressStore::new(pg_pool, config.bridge_genesis_checkpoint);
    let mut executor = IndexerExecutor::new(
        progress_store,
        1, /* workflow types */
        ingestion_metrics,
    );

    let indexer_metrics_cloned = indexer_meterics.clone();

    let worker_pool = WorkerPool::new(
        SuiBridgeWorker::new(vec![], config.db_url.clone(), indexer_metrics_cloned),
        "bridge worker".into(),
        config.concurrency as usize,
    );
    executor.register(worker_pool).await?;
    executor
        .run(
            config.checkpoints_path.clone().into(),
            Some(config.remote_store_url.clone()),
            vec![], // optional remote store access options
            ReaderOptions::default(),
            exit_receiver,
        )
        .await?;

    Ok(())
}
