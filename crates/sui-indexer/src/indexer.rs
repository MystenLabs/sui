// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::HashMap;
use std::env;

use anyhow::Result;
use prometheus::Registry;
use tokio::sync::oneshot;
use tokio_util::sync::CancellationToken;
use tracing::info;

use async_trait::async_trait;
use futures::future::try_join_all;
use mysten_metrics::spawn_monitored_task;
use sui_data_ingestion_core::{
    DataIngestionMetrics, IndexerExecutor, ProgressStore, ReaderOptions, WorkerPool,
};
use sui_types::messages_checkpoint::CheckpointSequenceNumber;

use crate::build_json_rpc_server;
use crate::config::{IngestionConfig, JsonRpcConfig, PruningOptions, SnapshotLagConfig};
use crate::database::ConnectionPool;
use crate::errors::IndexerError;
use crate::handlers::checkpoint_handler::new_handlers;
use crate::handlers::objects_snapshot_processor::start_objects_snapshot_processor;
use crate::handlers::pruner::Pruner;
use crate::indexer_reader::IndexerReader;
use crate::metrics::IndexerMetrics;
use crate::store::{IndexerStore, PgIndexerStore};

pub struct Indexer;

impl Indexer {
    pub async fn start_writer(
        config: &IngestionConfig,
        store: PgIndexerStore,
        metrics: IndexerMetrics,
    ) -> Result<(), IndexerError> {
        let snapshot_config = SnapshotLagConfig::default();
        Indexer::start_writer_with_config(
            config,
            store,
            metrics,
            snapshot_config,
            PruningOptions::default(),
            CancellationToken::new(),
        )
        .await
    }

    pub async fn start_writer_with_config(
        config: &IngestionConfig,
        store: PgIndexerStore,
        metrics: IndexerMetrics,
        snapshot_config: SnapshotLagConfig,
        pruning_options: PruningOptions,
        cancel: CancellationToken,
    ) -> Result<(), IndexerError> {
        info!(
            "Sui Indexer Writer (version {:?}) started...",
            env!("CARGO_PKG_VERSION")
        );

        info!("Sui Indexer Writer config: {config:?}",);

        let primary_watermark = store
            .get_latest_checkpoint_sequence_number()
            .await
            .expect("Failed to get latest tx checkpoint sequence number from DB")
            .map(|seq| seq + 1)
            .unwrap_or_default();

        let extra_reader_options = ReaderOptions {
            batch_size: config.checkpoint_download_queue_size,
            timeout_secs: config.checkpoint_download_timeout,
            data_limit: config.checkpoint_download_queue_size_bytes,
            ..Default::default()
        };

        // Start objects snapshot processor, which is a separate pipeline with its ingestion pipeline.
        let (object_snapshot_worker, object_snapshot_watermark) = start_objects_snapshot_processor(
            store.clone(),
            metrics.clone(),
            snapshot_config,
            cancel.clone(),
        )
        .await?;

        if let Some(epochs_to_keep) = pruning_options.epochs_to_keep {
            info!(
                "Starting indexer pruner with epochs to keep: {}",
                epochs_to_keep
            );
            assert!(epochs_to_keep > 0, "Epochs to keep must be positive");
            let pruner = Pruner::new(store.clone(), epochs_to_keep, metrics.clone())?;
            spawn_monitored_task!(pruner.start(CancellationToken::new()));
        }

        // If we already have chain identifier indexed (i.e. the first checkpoint has been indexed),
        // then we persist protocol configs for protocol versions not yet in the db.
        // Otherwise, we would do the persisting in `commit_checkpoint` while the first cp is
        // being indexed.
        if let Some(chain_id) = IndexerStore::get_chain_identifier(&store).await? {
            store
                .persist_protocol_configs_and_feature_flags(chain_id)
                .await?;
        }

        let mut exit_senders = vec![];
        let mut executors = vec![];
        let progress_store = ShimIndexerProgressStore::new(vec![
            ("primary".to_string(), primary_watermark),
            ("object_snapshot".to_string(), object_snapshot_watermark),
        ]);
        let mut executor = IndexerExecutor::new(
            progress_store.clone(),
            2,
            DataIngestionMetrics::new(&Registry::new()),
        );
        let worker = new_handlers(store, metrics, primary_watermark, cancel.clone()).await?;
        let worker_pool = WorkerPool::new(
            worker,
            "primary".to_string(),
            config.checkpoint_download_queue_size,
        );
        executor.register(worker_pool).await?;
        let (exit_sender, exit_receiver) = oneshot::channel();
        executors.push((executor, exit_receiver));
        exit_senders.push(exit_sender);

        // in a non-colocated setup, start a separate indexer for processing object snapshots
        if config.sources.data_ingestion_path.is_none() {
            let executor = IndexerExecutor::new(
                progress_store,
                1,
                DataIngestionMetrics::new(&Registry::new()),
            );
            let (exit_sender, exit_receiver) = oneshot::channel();
            exit_senders.push(exit_sender);
            executors.push((executor, exit_receiver));
        }

        let worker_pool = WorkerPool::new(
            object_snapshot_worker,
            "object_snapshot".to_string(),
            config.checkpoint_download_queue_size,
        );
        let executor = executors.last_mut().expect("executors is not empty");
        executor.0.register(worker_pool).await?;

        // Spawn a task that links the cancellation token to the exit sender
        spawn_monitored_task!(async move {
            cancel.cancelled().await;
            for exit_sender in exit_senders {
                let _ = exit_sender.send(());
            }
        });

        info!("Starting data ingestion executor...");
        let futures = executors.into_iter().map(|(executor, exit_receiver)| {
            executor.run(
                config
                    .sources
                    .data_ingestion_path
                    .clone()
                    .unwrap_or(tempfile::tempdir().unwrap().into_path()),
                config
                    .sources
                    .remote_store_url
                    .as_ref()
                    .map(|url| url.as_str().to_owned()),
                vec![],
                extra_reader_options.clone(),
                exit_receiver,
            )
        });
        try_join_all(futures).await?;
        Ok(())
    }

    pub async fn start_reader(
        config: &JsonRpcConfig,
        registry: &Registry,
        pool: ConnectionPool,
    ) -> Result<(), IndexerError> {
        info!(
            "Sui Indexer Reader (version {:?}) started...",
            env!("CARGO_PKG_VERSION")
        );
        let indexer_reader = IndexerReader::new(pool);
        let handle = build_json_rpc_server(registry, indexer_reader, config)
            .await
            .expect("Json rpc server should not run into errors upon start.");
        tokio::spawn(async move { handle.stopped().await })
            .await
            .expect("Rpc server task failed");

        Ok(())
    }
}

#[derive(Clone)]
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
