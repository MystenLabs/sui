// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::env;

use anyhow::Result;
use prometheus::Registry;
use tokio::sync::oneshot;
use tracing::info;

use mysten_metrics::spawn_monitored_task;
use sui_data_ingestion_core::{
    DataIngestionMetrics, IndexerExecutor, ReaderOptions, ShimProgressStore, WorkerPool,
};

use crate::build_json_rpc_server;
use crate::errors::IndexerError;
use crate::handlers::checkpoint_handler::new_handlers;
use crate::handlers::objects_snapshot_processor::{ObjectsSnapshotProcessor, SnapshotLagConfig};
use crate::indexer_reader::IndexerReader;
use crate::metrics::IndexerMetrics;
use crate::store::IndexerStore;
use crate::IndexerConfig;

const DOWNLOAD_QUEUE_SIZE: usize = 200;

pub struct Indexer;

impl Indexer {
    pub async fn start_writer<S: IndexerStore + Sync + Send + Clone + 'static>(
        config: &IndexerConfig,
        store: S,
        metrics: IndexerMetrics,
    ) -> Result<(), IndexerError> {
        let snapshot_config = SnapshotLagConfig::default();
        Indexer::start_writer_with_config(config, store, metrics, snapshot_config).await
    }

    pub async fn start_writer_with_config<S: IndexerStore + Sync + Send + Clone + 'static>(
        config: &IndexerConfig,
        store: S,
        metrics: IndexerMetrics,
        snapshot_config: SnapshotLagConfig,
    ) -> Result<(), IndexerError> {
        info!(
            "Sui Indexer Writer (version {:?}) started...",
            env!("CARGO_PKG_VERSION")
        );

        let watermark = store
            .get_latest_tx_checkpoint_sequence_number()
            .await
            .expect("Failed to get latest tx checkpoint sequence number from DB")
            .map(|seq| seq + 1)
            .unwrap_or_default();
        let download_queue_size = env::var("DOWNLOAD_QUEUE_SIZE")
            .unwrap_or_else(|_| DOWNLOAD_QUEUE_SIZE.to_string())
            .parse::<usize>()
            .expect("Invalid DOWNLOAD_QUEUE_SIZE");

        let objects_snapshot_processor = ObjectsSnapshotProcessor::new_with_config(
            store.clone(),
            metrics.clone(),
            snapshot_config,
        );
        spawn_monitored_task!(objects_snapshot_processor.start());

        #[allow(unused_variables)]
        let (exit_sender, exit_receiver) = oneshot::channel();
        let mut executor = IndexerExecutor::new(
            ShimProgressStore(watermark),
            1,
            DataIngestionMetrics::new(&Registry::new()),
        );
        let worker = new_handlers(store, metrics, watermark).await?;
        let worker_pool = WorkerPool::new(worker, "workflow".to_string(), download_queue_size);
        let extra_reader_options = ReaderOptions {
            batch_size: download_queue_size,
            ..Default::default()
        };
        executor.register(worker_pool).await?;
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

    pub async fn start_reader(
        config: &IndexerConfig,
        registry: &Registry,
        db_url: String,
    ) -> Result<(), IndexerError> {
        info!(
            "Sui Indexer Reader (version {:?}) started...",
            env!("CARGO_PKG_VERSION")
        );
        let indexer_reader = IndexerReader::new(db_url)?;
        let handle = build_json_rpc_server(registry, indexer_reader, config, None)
            .await
            .expect("Json rpc server should not run into errors upon start.");
        tokio::spawn(async move { handle.stopped().await })
            .await
            .expect("Rpc server task failed");

        Ok(())
    }
}
