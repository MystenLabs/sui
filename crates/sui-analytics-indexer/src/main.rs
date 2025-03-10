// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::{anyhow, Result};
use prometheus::Registry;
use std::{collections::HashMap, env, sync::Arc};
use sui_analytics_indexer::{
    analytics_metrics::AnalyticsMetrics, errors::AnalyticsIndexerError, make_analytics_processor,
    AnalyticsIndexerConfig,
};
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
    let config: AnalyticsIndexerConfig = serde_yaml::from_str(&std::fs::read_to_string(&args[1])?)?;
    info!("Parsed config: {:#?}", config);
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

    let mut watermarks = HashMap::new();
    let mut processors = Vec::new();
    let config = Arc::new(config);
    for task_config in config.tasks.clone() {
        let metrics = AnalyticsMetrics::new(&registry);
        let task_name = task_config.task_name.clone();
        let processor = make_analytics_processor(config.clone(), task_config, metrics)
            .await
            .map_err(|e| AnalyticsIndexerError::GenericError(e.to_string()))?;
        let watermark = processor.last_committed_checkpoint().unwrap_or_default() + 1;
        if watermarks.insert(task_name.clone(), watermark).is_some() {
            return Err(anyhow!("Duplicate task_name '{}' found", task_name));
        }
        processors.push(processor);
    }

    let progress_store = ShimIndexerProgressStore::new(watermarks);
    let mut executor = IndexerExecutor::new(
        progress_store,
        config.tasks.len(),
        DataIngestionMetrics::new(&Registry::new()),
    );

    for processor in processors.into_iter() {
        let worker_pool = WorkerPool::new(processor, "workflow".to_string(), 1);
        executor.register(worker_pool).await?;
    }

    let remote_store_url = config.remote_store_url.clone();

    let reader_options = ReaderOptions {
        batch_size: 10,
        ..Default::default()
    };

    let (exit_sender, exit_receiver) = oneshot::channel();
    let executor_progress = executor.run(
        tempfile::tempdir()?.into_path(),
        Some(remote_store_url),
        vec![],
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
