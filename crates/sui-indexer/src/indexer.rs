// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::HashMap;
use std::env;

use anyhow::Result;
use diesel::r2d2::R2D2Connection;
use prometheus::Registry;
use tokio::sync::oneshot;
use tokio_util::sync::CancellationToken;
use tracing::info;

use async_trait::async_trait;
use mysten_metrics::spawn_monitored_task;
use sui_data_ingestion_core::{
    DataIngestionMetrics, IndexerExecutor, ProgressStore, ReaderOptions, WorkerPool,
};
use sui_types::messages_checkpoint::CheckpointSequenceNumber;

use crate::build_json_rpc_server;
use crate::errors::IndexerError;
use crate::handlers::checkpoint_handler::new_handlers;
use crate::handlers::objects_snapshot_processor::{
    start_objects_snapshot_processor, SnapshotLagConfig,
};
use crate::handlers::pruner::Pruner;
use crate::indexer_reader::IndexerReader;
use crate::metrics::IndexerMetrics;
use crate::store::IndexerStore;
use crate::IndexerConfig;

pub(crate) const DOWNLOAD_QUEUE_SIZE: usize = 200;
const INGESTION_READER_TIMEOUT_SECS: u64 = 20;
// Limit indexing parallelism on big checkpoints to avoid OOM,
// by limiting the total size of batch checkpoints to ~20MB.
// On testnet, most checkpoints are < 200KB, some can go up to 50MB.
const CHECKPOINT_PROCESSING_BATCH_DATA_LIMIT: usize = 20000000;

pub struct Indexer;

impl Indexer {
    pub async fn start_writer<
        S: IndexerStore + Sync + Send + Clone + 'static,
        T: R2D2Connection + 'static,
    >(
        config: &IndexerConfig,
        store: S,
        metrics: IndexerMetrics,
    ) -> Result<(), IndexerError> {
        let snapshot_config = SnapshotLagConfig::default();
        Indexer::start_writer_with_config::<S, T>(
            config,
            store,
            metrics,
            snapshot_config,
            CancellationToken::new(),
        )
        .await
    }

    pub async fn start_writer_with_config<
        S: IndexerStore + Sync + Send + Clone + 'static,
        T: R2D2Connection + 'static,
    >(
        config: &IndexerConfig,
        store: S,
        metrics: IndexerMetrics,
        snapshot_config: SnapshotLagConfig,
        cancel: CancellationToken,
    ) -> Result<(), IndexerError> {
        info!(
            "Sui Indexer Writer (version {:?}) started...",
            env!("CARGO_PKG_VERSION")
        );

        let primary_watermark = store
            .get_latest_checkpoint_sequence_number()
            .await
            .expect("Failed to get latest tx checkpoint sequence number from DB")
            .map(|seq| seq + 1)
            .unwrap_or_default();
        let download_queue_size = env::var("DOWNLOAD_QUEUE_SIZE")
            .unwrap_or_else(|_| DOWNLOAD_QUEUE_SIZE.to_string())
            .parse::<usize>()
            .expect("Invalid DOWNLOAD_QUEUE_SIZE");
        let ingestion_reader_timeout_secs = env::var("INGESTION_READER_TIMEOUT_SECS")
            .unwrap_or_else(|_| INGESTION_READER_TIMEOUT_SECS.to_string())
            .parse::<u64>()
            .expect("Invalid INGESTION_READER_TIMEOUT_SECS");
        let data_limit = std::env::var("CHECKPOINT_PROCESSING_BATCH_DATA_LIMIT")
            .unwrap_or(CHECKPOINT_PROCESSING_BATCH_DATA_LIMIT.to_string())
            .parse::<usize>()
            .unwrap();
        let extra_reader_options = ReaderOptions {
            batch_size: download_queue_size,
            timeout_secs: ingestion_reader_timeout_secs,
            data_limit,
            ..Default::default()
        };

        // Start objects snapshot processor, which is a separate pipeline with its ingestion pipeline.
        let (object_snapshot_worker, object_snapshot_watermark) =
            start_objects_snapshot_processor::<S, T>(
                store.clone(),
                metrics.clone(),
                snapshot_config,
                cancel.clone(),
            )
            .await?;

        let epochs_to_keep = std::env::var("EPOCHS_TO_KEEP")
            .map(|s| s.parse::<u64>().ok())
            .unwrap_or_else(|_e| None);
        if let Some(epochs_to_keep) = epochs_to_keep {
            info!(
                "Starting indexer pruner with epochs to keep: {}",
                epochs_to_keep
            );
            let pruner = Pruner::new(store.clone(), epochs_to_keep, metrics.clone());
            spawn_monitored_task!(pruner.start(CancellationToken::new()));
        }

        let cancel_clone = cancel.clone();
        let (exit_sender, exit_receiver) = oneshot::channel();
        // Spawn a task that links the cancellation token to the exit sender
        spawn_monitored_task!(async move {
            cancel_clone.cancelled().await;
            let _ = exit_sender.send(());
        });

        let mut executor = IndexerExecutor::new(
            ShimIndexerProgressStore::new(vec![
                ("primary".to_string(), primary_watermark),
                ("object_snapshot".to_string(), object_snapshot_watermark),
            ]),
            1,
            DataIngestionMetrics::new(&Registry::new()),
        );
        let worker =
            new_handlers::<S, T>(store, metrics, primary_watermark, cancel.clone()).await?;
        let worker_pool = WorkerPool::new(worker, "primary".to_string(), download_queue_size);

        executor.register(worker_pool).await?;

        let worker_pool = WorkerPool::new(
            object_snapshot_worker,
            "object_snapshot".to_string(),
            download_queue_size,
        );
        executor.register(worker_pool).await?;
        info!("Starting data ingestion executor...");
        executor
            .run(
                config
                    .data_ingestion_path
                    .clone()
                    .unwrap_or(tempfile::tempdir().unwrap().into_path()),
                config.remote_store_url.clone(),
                vec![],
                extra_reader_options,
                exit_receiver,
            )
            .await?;
        Ok(())
    }

    pub async fn start_reader<T: R2D2Connection + 'static>(
        config: &IndexerConfig,
        registry: &Registry,
        db_url: String,
    ) -> Result<(), IndexerError> {
        info!(
            "Sui Indexer Reader (version {:?}) started...",
            env!("CARGO_PKG_VERSION")
        );
        let indexer_reader = IndexerReader::<T>::new(db_url)?;
        let handle = build_json_rpc_server(registry, indexer_reader, config, None)
            .await
            .expect("Json rpc server should not run into errors upon start.");
        tokio::spawn(async move { handle.stopped().await })
            .await
            .expect("Rpc server task failed");

        Ok(())
    }
}

struct ShimIndexerProgressStore {
    watermarks: HashMap<String, CheckpointSequenceNumber>,
}

impl ShimIndexerProgressStore {
    fn new(watermarks: Vec<(String, CheckpointSequenceNumber)>) -> Self {
        Self {
            watermarks: watermarks.into_iter().collect(),
        }
    }
}

#[async_trait]
impl ProgressStore for ShimIndexerProgressStore {
    async fn load(&mut self, task_name: String) -> Result<CheckpointSequenceNumber> {
        Ok(*self.watermarks.get(&task_name).expect("missing watermark"))
    }

    async fn save(&mut self, _: String, _: CheckpointSequenceNumber) -> Result<()> {
        Ok(())
    }
}
