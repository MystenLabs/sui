// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::Result;
use bytes::Bytes;
use object_store::path::Path;
use object_store::{Error, ObjectStore};
use prometheus::Registry;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use sui_data_ingestion::{BlobTaskConfig, BlobWorker};
use sui_data_ingestion_core::{
    create_remote_store_client, end_of_epoch_data, DataIngestionMetrics, FileProgressStore,
    IndexerExecutor, ProgressStore, ReaderOptions, WorkerPool,
};
use tokio::sync::oneshot;

static TASK_NAME: String = String::new();

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
struct Config {
    source_url: String,
    watermark_file_path: PathBuf,
    target_url: String,
    #[serde(default)]
    target_remote_store_options: Vec<(String, String)>,
    #[serde(default = "default_concurrency")]
    concurrency: usize,
    #[serde(default = "default_timeout_secs")]
    timeout_secs: u64,
}

fn default_timeout_secs() -> u64 {
    10
}

fn default_concurrency() -> usize {
    10
}

async fn init_watermark(progress_store: &mut dyn ProgressStore, config: &Config) -> Result<()> {
    match progress_store.load(TASK_NAME.clone()).await {
        Ok(_) => Ok(()),
        Err(_) => {
            let epochs = end_of_epoch_data(
                config.target_url.clone(),
                config.target_remote_store_options.clone(),
                config.timeout_secs,
            )
            .await?;
            let watermark = *epochs.last().unwrap_or(&0);
            println!("missing local watermark. Starting with {}", watermark);
            progress_store.save(TASK_NAME.clone(), watermark).await?;
            Ok(())
        }
    }
}

async fn ensure_epochs_file_exist(config: &Config) -> Result<()> {
    let client = create_remote_store_client(
        config.target_url.clone(),
        config.target_remote_store_options.clone(),
        config.timeout_secs,
    )?;
    let path = Path::from("epochs.json");
    match client.head(&path).await {
        Ok(_) => Ok(()),
        Err(Error::NotFound { .. }) => {
            println!("epochs.json not found in the bucket: creating an empty file");
            client.put(&path, Bytes::from_static(b"[]").into()).await?;
            Ok(())
        }
        Err(err) => Err(err.into()),
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    let _guard = telemetry_subscribers::TelemetryConfig::new()
        .with_env()
        .init();
    let args: Vec<String> = std::env::args().collect();
    assert_eq!(args.len(), 2, "configuration yaml file is required");
    let config: Config = serde_yaml::from_str(&std::fs::read_to_string(&args[1])?)?;

    let (_exit_sender, exit_receiver) = oneshot::channel();
    ensure_epochs_file_exist(&config).await?;
    let mut progress_store = FileProgressStore::new(config.watermark_file_path.clone());
    init_watermark(&mut progress_store, &config).await?;

    let mut executor = IndexerExecutor::new(
        progress_store,
        1,
        DataIngestionMetrics::new(&Registry::new()),
    );
    let worker = BlobWorker::new(BlobTaskConfig {
        url: config.target_url,
        remote_store_options: config.target_remote_store_options,
    });
    let worker_pool = WorkerPool::new(worker, TASK_NAME.clone(), config.concurrency);
    executor.register(worker_pool).await?;
    executor
        .run(
            tempfile::tempdir()?.keep(),
            Some(config.source_url),
            vec![],
            ReaderOptions::default(),
            exit_receiver,
        )
        .await?;
    Ok(())
}
