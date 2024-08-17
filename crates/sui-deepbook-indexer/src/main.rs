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
use sui_deepbook_indexer::metrics::DeepBookIndexerMetrics;
use sui_deepbook_indexer::postgres_manager::get_connection_pool;
use sui_deepbook_indexer::sui_deepbook_indexer::{PgDeepbookPersistent, SuiDeepBookDataMapper};
use sui_indexer_builder::indexer_builder::IndexerBuilder;
use sui_indexer_builder::sui_datasource::SuiCheckpointDatasource;
use sui_types::base_types::ObjectID;
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
    let indexer_meterics = DeepBookIndexerMetrics::new(&registry);
    let ingestion_metrics = DataIngestionMetrics::new(&registry);

    let db_url = config.db_url.clone();
    let datastore = PgDeepbookPersistent::new(get_connection_pool(db_url.clone()));

    let sui_checkpoint_datasource = SuiCheckpointDatasource::new(
        config.remote_store_url,
        config.concurrency as usize,
        config.checkpoints_path.clone().into(),
        ingestion_metrics.clone(),
    );
    let indexer = IndexerBuilder::new(
        "SuiDeepBookIndexer",
        sui_checkpoint_datasource,
        SuiDeepBookDataMapper {
            metrics: indexer_meterics.clone(),
            package_id: ObjectID::from_hex_literal(&config.deepbook_package_id.clone())
                .unwrap_or_else(|err| panic!("Failed to parse deepbook package ID: {}", err)),
        },
    )
    .build(
        config
            .resume_from_checkpoint
            .unwrap_or(config.deepbook_genesis_checkpoint),
        config.deepbook_genesis_checkpoint,
        datastore,
    );
    indexer.start().await?;

    Ok(())
}
