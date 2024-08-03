// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::env;

use clap::Parser;
use tracing::info;

use sui_indexer::errors::IndexerError;
use sui_indexer::metrics::start_prometheus_server;
use sui_indexer::metrics::IndexerMetrics;
use sui_indexer::restorer::archives::read_next_checkpoint_after_epoch;
use sui_indexer::restorer::formal_snapshot::{
    IndexerFormalSnapshotRestorer, IndexerFormalSnapshotRestorerConfig,
};
use sui_indexer::IndexerConfig;

#[tokio::main]
async fn main() -> Result<(), IndexerError> {
    // NOTE: this is to print out tracing like info, warn & error.
    let _guard = telemetry_subscribers::TelemetryConfig::new()
        .with_env()
        .init();

    let mut indexer_config = IndexerConfig::parse();
    info!("Parsed indexer config: {:#?}", indexer_config);
    // TODO: remove. Temporary safeguard to migrate to `rpc_client_url` usage
    if indexer_config.rpc_client_url.contains("testnet") {
        indexer_config.remote_store_url = Some("https://checkpoints.testnet.sui.io".to_string());
    } else if indexer_config.rpc_client_url.contains("mainnet") {
        indexer_config.remote_store_url = Some("https://checkpoints.mainnet.sui.io".to_string());
    }
    let (_registry_service, registry) = start_prometheus_server(
        // NOTE: this parses the input host addr and port number for socket addr,
        // so unwrap() is safe here.
        format!(
            "{}:{}",
            indexer_config.client_metric_host, indexer_config.client_metric_port
        )
        .parse()
        .unwrap(),
        indexer_config.rpc_client_url.as_str(),
    )?;
    let indexer_metrics = IndexerMetrics::new(&registry);
    mysten_metrics::init_metrics(&registry);

    let start_epoch = env::var("START_EPOCH")
        .expect("START_EPOCH not set")
        .parse::<u64>()
        .ok();
    if let Some(start_epoch) = start_epoch {
        let cred_path = env::var("GOOGLE_APPLICATION_CREDENTIALS_PATH")
            .expect("GOOGLE_APPLICATION_CREDENTIALS_PATH not set");

        let next_checkpoint_after_epoch = read_next_checkpoint_after_epoch(
            cred_path.clone(),
            Some(indexer_config.archive_bucket.clone()),
            start_epoch,
        )
        .await?;
        let base_path_string = env::var("SNAPSHOT_DIR").expect("SNAPSHOT_DIR not set");
        let formal_restorer_config = IndexerFormalSnapshotRestorerConfig {
            cred_path: cred_path.clone(),
            base_path: base_path_string.clone(),
            epoch: start_epoch,
            next_checkpoint_after_epoch,
        };
        let mut formal_restorer = IndexerFormalSnapshotRestorer::new(
            indexer_config.clone(),
            indexer_metrics.clone(),
            formal_restorer_config,
        )
        .await?;
        formal_restorer.restore().await?;
    }

    #[cfg(feature = "postgres-feature")]
    sui_indexer::db::setup_postgres::setup(
        indexer_config.clone(),
        registry.clone(),
        indexer_metrics.clone(),
    )
    .await?;
    #[cfg(feature = "mysql-feature")]
    #[cfg(not(feature = "postgres-feature"))]
    sui_indexer::db::setup_mysql::setup(indexer_config, registry, indexer_metrics).await?;
    Ok(())
}
