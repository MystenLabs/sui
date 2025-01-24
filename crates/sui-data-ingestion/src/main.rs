// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::Result;
use prometheus::Registry;
use serde::{Deserialize, Serialize};
use std::env;
use std::path::PathBuf;
use std::time::Duration;
use sui_data_ingestion::{
    ArchivalConfig, ArchivalReducer, ArchivalWorker, BlobTaskConfig, BlobWorker,
    DynamoDBProgressStore,
};
use sui_data_ingestion_core::{DataIngestionMetrics, ReaderOptions};
use sui_data_ingestion_core::{IndexerExecutor, WorkerPool};
use sui_kvstore::{BigTableClient, BigTableProgressStore, KvWorker};
use tokio::signal;
use tokio::sync::oneshot;

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "lowercase")]
enum Task {
    Archival(ArchivalConfig),
    Blob(BlobTaskConfig),
    BigTableKV(BigTableTaskConfig),
}

#[derive(Serialize, Deserialize, Clone, Debug)]
struct TaskConfig {
    #[serde(flatten)]
    task: Task,
    name: String,
    concurrency: usize,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "lowercase")]
struct ProgressStoreConfig {
    pub aws_access_key_id: String,
    pub aws_secret_access_key: String,
    pub aws_region: String,
    pub table_name: String,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
struct BigTableTaskConfig {
    instance_id: String,
    timeout_secs: usize,
    credentials: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct IndexerConfig {
    path: PathBuf,
    tasks: Vec<TaskConfig>,
    progress_store: ProgressStoreConfig,
    #[serde(skip_serializing_if = "Option::is_none")]
    remote_store_url: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    remote_store_options: Vec<(String, String)>,
    #[serde(default = "default_remote_read_batch_size")]
    remote_read_batch_size: usize,
    #[serde(default = "default_metrics_host")]
    metrics_host: String,
    #[serde(default = "default_metrics_port")]
    metrics_port: u16,
    #[serde(default)]
    is_backfill: bool,
}

fn default_metrics_host() -> String {
    "127.0.0.1".to_string()
}

fn default_metrics_port() -> u16 {
    8081
}

fn default_remote_read_batch_size() -> usize {
    100
}

fn setup_env(exit_sender: oneshot::Sender<()>) {
    let default_hook = std::panic::take_hook();

    std::panic::set_hook(Box::new(move |panic| {
        default_hook(panic);
        std::process::exit(12);
    }));

    tokio::spawn(async {
        signal::ctrl_c()
            .await
            .expect("Failed to install Ctrl+C handler");
        exit_sender
            .send(())
            .expect("Failed to gracefully process shutdown");
    });
}

#[tokio::main]
async fn main() -> Result<()> {
    let (exit_sender, exit_receiver) = oneshot::channel();
    setup_env(exit_sender);

    let args: Vec<String> = env::args().collect();
    assert_eq!(args.len(), 2, "configuration yaml file is required");
    let config: IndexerConfig = serde_yaml::from_str(&std::fs::read_to_string(&args[1])?)?;

    // setup metrics
    let _guard = telemetry_subscribers::TelemetryConfig::new()
        .with_env()
        .init();
    let registry_service = mysten_metrics::start_prometheus_server(
        format!("{}:{}", config.metrics_host, config.metrics_port).parse()?,
    );
    let registry: Registry = registry_service.default_registry();
    mysten_metrics::init_metrics(&registry);
    let metrics = DataIngestionMetrics::new(&registry);

    let mut bigtable_store = None;
    for task in &config.tasks {
        if let Task::BigTableKV(kv_config) = &task.task {
            std::env::set_var(
                "GOOGLE_APPLICATION_CREDENTIALS",
                kv_config.credentials.clone(),
            );
            let bigtable_client = BigTableClient::new_remote(
                kv_config.instance_id.clone(),
                false,
                Some(Duration::from_secs(kv_config.timeout_secs as u64)),
            )
            .await?;
            bigtable_store = Some(BigTableProgressStore::new(bigtable_client));
        }
    }

    let progress_store = DynamoDBProgressStore::new(
        &config.progress_store.aws_access_key_id,
        &config.progress_store.aws_secret_access_key,
        config.progress_store.aws_region,
        config.progress_store.table_name,
        config.is_backfill,
        bigtable_store,
    )
    .await;
    let mut executor = IndexerExecutor::new(progress_store, config.tasks.len(), metrics);
    for task_config in config.tasks {
        match task_config.task {
            Task::Archival(archival_config) => {
                let reducer = ArchivalReducer::new(archival_config).await?;
                executor
                    .update_watermark(task_config.name.clone(), reducer.get_watermark().await?)
                    .await?;
                let worker_pool = WorkerPool::new_with_reducer(
                    ArchivalWorker,
                    task_config.name,
                    task_config.concurrency,
                    Box::new(reducer),
                );
                executor.register(worker_pool).await?;
            }
            Task::Blob(blob_config) => {
                let worker_pool = WorkerPool::new(
                    BlobWorker::new(blob_config),
                    task_config.name,
                    task_config.concurrency,
                );
                executor.register(worker_pool).await?;
            }
            Task::BigTableKV(kv_config) => {
                let client = BigTableClient::new_remote(
                    kv_config.instance_id,
                    false,
                    Some(Duration::from_secs(kv_config.timeout_secs as u64)),
                )
                .await?;
                let worker_pool = WorkerPool::new(
                    KvWorker { client },
                    task_config.name,
                    task_config.concurrency,
                );
                executor.register(worker_pool).await?;
            }
        };
    }
    let reader_options = ReaderOptions {
        batch_size: config.remote_read_batch_size,
        ..Default::default()
    };
    executor
        .run(
            config.path,
            config.remote_store_url,
            config.remote_store_options,
            reader_options,
            exit_receiver,
        )
        .await?;
    Ok(())
}
