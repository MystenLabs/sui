// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::num::NonZeroUsize;
use std::env;
use std::path::{Path, PathBuf};
use std::{fs, io};


use clap::Parser;
use indicatif::{MultiProgress, ProgressBar, ProgressStyle};
use tracing::info;

use sui_config::object_storage_config::{ObjectStoreConfig, ObjectStoreType};
use sui_snapshot::reader::StateSnapshotReaderV1;

use sui_indexer::errors::IndexerError;
use sui_indexer::metrics::start_prometheus_server;
use sui_indexer::IndexerConfig;

#[tokio::main]
async fn main() -> Result<(), IndexerError> {
    // NOTE: this is to print out tracing like info, warn & error.
    let _guard = telemetry_subscribers::TelemetryConfig::new()
        .with_env()
        .init();
    let m = MultiProgress::new();
    let cred_path = env::var("GCS_SNAPSHOT_SERVICE_ACCOUNT_FILE_PATH").unwrap_or(
        "/Users/gegao/Desktop/ge-sa.json".to_string()
    );
    let remote_store_config = ObjectStoreConfig {
        object_store: Some(ObjectStoreType::GCS),
        bucket: Some("mysten-mainnet-formal".to_string()),
        google_service_account: Some(cred_path),
        object_store_connection_limit: 200,
        no_sign_request: false,
        ..Default::default()
    };

    let base_path_string = env::var("SNAPSHOT_DIR")
        .unwrap_or_else(|_| "/Users/gegao/Desktop/gcs_snapshot".to_string());
    let base_path = PathBuf::from(base_path_string);
    tracing::info!("Snapshot directory: {:?}", base_path);
    let snapshot_dir = base_path.join("snapshot");
    if snapshot_dir.exists() {
        fs::remove_dir_all(snapshot_dir.clone()).unwrap();
        info!("Deleted all files from snapshot directory: {:?}", snapshot_dir);
    } else {
        fs::create_dir(snapshot_dir.clone()).unwrap();
        info!("Created snapshot directory: {:?}", snapshot_dir);
    }

    let local_store_config = ObjectStoreConfig {
        object_store: Some(ObjectStoreType::File),
        directory: Some(snapshot_dir.clone().to_path_buf()),
        ..Default::default()
    };

    StateSnapshotReaderV1::new(
        400,
        &remote_store_config,
        &local_store_config,
        usize::MAX,
        NonZeroUsize::new(100 as usize).unwrap(),
        m.clone(),
    ).await.unwrap_or_else(|err| panic!("Failed to create reader: {}", err));
    info!("Finished reading snapshot");

    // let mut indexer_config = IndexerConfig::parse();
    // // TODO: remove. Temporary safeguard to migrate to `rpc_client_url` usage
    // if indexer_config.rpc_client_url.contains("testnet") {
    //     indexer_config.remote_store_url = Some("https://checkpoints.testnet.sui.io".to_string());
    // } else if indexer_config.rpc_client_url.contains("mainnet") {
    //     indexer_config.remote_store_url = Some("https://checkpoints.mainnet.sui.io".to_string());
    // }
    // info!("Parsed indexer config: {:#?}", indexer_config);
    // let (_registry_service, registry) = start_prometheus_server(
    //     // NOTE: this parses the input host addr and port number for socket addr,
    //     // so unwrap() is safe here.
    //     format!(
    //         "{}:{}",
    //         indexer_config.client_metric_host, indexer_config.client_metric_port
    //     )
    //     .parse()
    //     .unwrap(),
    //     indexer_config.rpc_client_url.as_str(),
    // )?;
    // #[cfg(feature = "postgres-feature")]
    // sui_indexer::db::setup_postgres::setup(indexer_config.clone(), registry.clone()).await?;

    // #[cfg(feature = "mysql-feature")]
    // #[cfg(not(feature = "postgres-feature"))]
    // sui_indexer::db::setup_mysql::setup(indexer_config, registry).await?;
    Ok(())
}
