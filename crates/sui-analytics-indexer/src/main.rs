// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::Result;
use prometheus::Registry;
use std::{collections::HashMap, env};
use sui_analytics_indexer::{analytics_metrics::AnalyticsMetrics, JobConfig};
use sui_data_ingestion_core::{
    DataIngestionMetrics, IndexerExecutor, ReaderOptions, ShimIndexerProgressStore, WorkerPool,
};
use tokio::{signal, sync::oneshot};
use tracing::info;

#[tokio::main]
async fn main() -> Result<()> {
    let _guard = telemetry_subscribers::TelemetryConfig::new()
        .with_env()
        .init();

    let args: Vec<String> = env::args().collect();
    assert_eq!(args.len(), 2, "configuration yaml file is required");

    // Parse the config
    let config: JobConfig = serde_yaml::from_str(&std::fs::read_to_string(&args[1])?)?;
    info!("Parsed config: {:#?}", config);

    // Setup metrics
    let registry_service = mysten_metrics::start_prometheus_server(
        format!(
            "{}:{}",
            config.client_metric_host, config.client_metric_port
        )
        .parse()
        .unwrap(),
    );
    let registry: Registry = registry_service.default_registry();
    mysten_metrics::init_metrics(&registry);
    let metrics = AnalyticsMetrics::new(&registry);

    let remote_store_url = config.remote_store_url.clone();
    let remote_store_options = config.remote_store_options.clone();
    let batch_size = config.batch_size;
    let data_limit = config.data_limit;

    let processors = config.create_checkpoint_processors(metrics).await?;

    let mut watermarks = HashMap::new();
    for processor in processors.iter() {
        let watermark = processor
            .last_committed_checkpoint()
            .map(|seq_num| seq_num + 1)
            .unwrap_or(0);
        watermarks.insert(processor.task_name.clone(), watermark);
    }

    let progress_store = ShimIndexerProgressStore::new(watermarks);
    let mut executor = IndexerExecutor::new(
        progress_store,
        processors.len(),
        DataIngestionMetrics::new(&registry),
    );

    for processor in processors {
        let task_name = processor.task_name.clone();
        let worker_pool = WorkerPool::new(processor, task_name, 1);
        executor.register(worker_pool).await?;
    }

    let reader_options = ReaderOptions {
        batch_size,
        data_limit,
        ..Default::default()
    };

    let (exit_sender, exit_receiver) = oneshot::channel();
    let executor_progress = executor.run(
        tempfile::tempdir()?.into_path(),
        Some(remote_store_url),
        remote_store_options,
        reader_options,
        exit_receiver,
    );

    tokio::spawn(async {
        signal::ctrl_c()
            .await
            .expect("Failed to install Ctrl+C handler");
        exit_sender
            .send(())
            .expect("Failed to gracefully process shutdown");
    });
    executor_progress.await?;
    Ok(())
}
