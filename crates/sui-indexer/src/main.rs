// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use clap::Parser;
use tracing::{info, warn};

use sui_indexer::errors::IndexerError;
use sui_indexer::metrics::start_prometheus_server;
use sui_indexer::IndexerConfig;

#[tokio::main]
async fn main() -> Result<(), IndexerError> {
    // NOTE: this is to print out tracing like info, warn & error.
    let _guard = telemetry_subscribers::TelemetryConfig::new()
        .with_env()
        .init();
    warn!("WARNING: Sui indexer is still experimental and we expect occasional breaking changes that require backfills.");

    let mut indexer_config = IndexerConfig::parse();
    // TODO: remove. Temporary safeguard to migrate to `rpc_client_url` usage
    if indexer_config.rpc_client_url.contains("testnet") {
        indexer_config.remote_store_url = Some("https://checkpoints.testnet.sui.io".to_string());
    } else if indexer_config.rpc_client_url.contains("mainnet") {
        indexer_config.remote_store_url = Some("https://checkpoints.mainnet.sui.io".to_string());
    }
    info!("Parsed indexer config: {:#?}", indexer_config);
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
    #[cfg(feature = "postgres-feature")]
    sui_indexer::db::setup_postgres::setup(indexer_config.clone(), registry.clone()).await?;

    #[cfg(feature = "mysql-feature")]
    #[cfg(not(feature = "postgres-feature"))]
    sui_indexer::db::setup_mysql::setup(indexer_config, registry).await?;
    Ok(())
}
