// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use async_trait::async_trait;
use diesel::r2d2::R2D2Connection;
use futures::StreamExt;
use mysten_metrics::get_metrics;
use mysten_metrics::metered_channel::{Receiver, Sender};
use mysten_metrics::spawn_monitored_task;
use prometheus::Registry;
use sui_data_ingestion_core::{
    DataIngestionMetrics, IndexerExecutor, ReaderOptions, ShimProgressStore, Worker, WorkerPool,
};
use sui_package_resolver::{PackageStoreWithLruCache, Resolver};
use sui_rest_api::CheckpointData;
use sui_types::messages_checkpoint::CheckpointSequenceNumber;
use tokio::sync::{oneshot, watch};
use tokio_util::sync::CancellationToken;
use tracing::info;

use crate::store::package_resolver::{IndexerStorePackageResolver, InterimPackageResolver};
use crate::types::IndexerResult;
use crate::IndexerConfig;
use crate::{metrics::IndexerMetrics, store::IndexerStore};
use std::sync::{Arc, Mutex};

use super::checkpoint_handler::CheckpointHandler;
use super::tx_processor::IndexingPackageBuffer;
use super::TransactionObjectChangesToCommit;

const OBJECTS_SNAPSHOT_MAX_CHECKPOINT_LAG: usize = 900;
const OBJECTS_SNAPSHOT_MIN_CHECKPOINT_LAG: usize = 300;

pub struct ObjectsSnapshotProcessor<S, T: R2D2Connection + 'static> {
    pub store: S,
    package_buffer: Arc<Mutex<IndexingPackageBuffer>>,
    package_resolver: Arc<Resolver<PackageStoreWithLruCache<InterimPackageResolver<T>>>>,
    pub indexed_obj_sender: Sender<CheckpointObjectChanges>,
    metrics: IndexerMetrics,
}

pub struct CheckpointObjectChanges {
    pub checkpoint_sequence_number: u64,
    pub object_changes: TransactionObjectChangesToCommit,
}

#[derive(Clone)]
pub struct SnapshotLagConfig {
    pub snapshot_min_lag: usize,
    pub snapshot_max_lag: usize,
    pub sleep_duration: u64,
}

impl SnapshotLagConfig {
    pub fn new(
        min_lag: Option<usize>,
        max_lag: Option<usize>,
        sleep_duration: Option<u64>,
    ) -> Self {
        let default_config = Self::default();
        Self {
            snapshot_min_lag: min_lag.unwrap_or(default_config.snapshot_min_lag),
            snapshot_max_lag: max_lag.unwrap_or(default_config.snapshot_max_lag),
            sleep_duration: sleep_duration.unwrap_or(default_config.sleep_duration),
        }
    }
}

impl Default for SnapshotLagConfig {
    fn default() -> Self {
        let snapshot_min_lag = std::env::var("OBJECTS_SNAPSHOT_MIN_CHECKPOINT_LAG")
            .ok()
            .and_then(|s| s.parse::<usize>().ok())
            .unwrap_or(OBJECTS_SNAPSHOT_MIN_CHECKPOINT_LAG);

        let snapshot_max_lag = std::env::var("OBJECTS_SNAPSHOT_MAX_CHECKPOINT_LAG")
            .ok()
            .and_then(|s| s.parse::<usize>().ok())
            .unwrap_or(OBJECTS_SNAPSHOT_MAX_CHECKPOINT_LAG);

        SnapshotLagConfig {
            snapshot_min_lag,
            snapshot_max_lag,
            sleep_duration: 5,
        }
    }
}

#[async_trait]
impl<S, T> Worker for ObjectsSnapshotProcessor<S, T>
where
    S: IndexerStore + Clone + Sync + Send + 'static,
    T: R2D2Connection + 'static,
{
    async fn process_checkpoint(&self, checkpoint: CheckpointData) -> anyhow::Result<()> {
        let checkpoint_sequence_number = checkpoint.checkpoint_summary.sequence_number;
        // Index the object changes and send them to the committer.
        let object_changes: TransactionObjectChangesToCommit =
            CheckpointHandler::<S, T>::index_objects(
                checkpoint,
                &self.metrics,
                self.package_resolver.clone(),
            )
            .await?;
        self.indexed_obj_sender
            .send(CheckpointObjectChanges {
                checkpoint_sequence_number,
                object_changes,
            })
            .await?;
        Ok(())
    }

    fn preprocess_hook(&self, checkpoint: CheckpointData) -> anyhow::Result<()> {
        let package_objects = CheckpointHandler::<S, T>::get_package_objects(&[checkpoint]);
        self.package_buffer
            .lock()
            .unwrap()
            .insert_packages(package_objects);
        Ok(())
    }
}

// Start both the ingestion pipeline and committer for objects snapshot table.
pub async fn start_objects_snapshot_processor<S, T>(
    store: S,
    metrics: IndexerMetrics,
    config: IndexerConfig,
    snapshot_config: SnapshotLagConfig,
    reader_options: ReaderOptions,
    cancel: CancellationToken,
) -> IndexerResult<()>
where
    S: IndexerStore + Clone + Sync + Send + 'static,
    T: R2D2Connection + 'static,
{
    info!("Starting object snapshot processor...");

    let watermark = store
        .get_latest_object_snapshot_checkpoint_sequence_number()
        .await
        .expect("Failed to get latest snapshot checkpoint sequence number from DB")
        .map(|seq| seq + 1)
        .unwrap_or_default();
    let download_queue_size = std::env::var("DOWNLOAD_QUEUE_SIZE")
        .unwrap_or_else(|_| crate::indexer::DOWNLOAD_QUEUE_SIZE.to_string())
        .parse::<usize>()
        .expect("Invalid DOWNLOAD_QUEUE_SIZE");

    // Set up exit sender and receiver. Sender signals the ingestion executor to exit when
    // the cancel token is triggered.
    let (exit_sender, exit_receiver) = oneshot::channel();
    let cancel_clone = cancel.clone();
    spawn_monitored_task!(async move {
        cancel_clone.cancelled().await;
        let _ = exit_sender.send(());
    });

    // Commit notifier to to signal the package buffer that a checkpoint has been committed.
    let (commit_notifier, commit_receiver) = watch::channel(None);

    let global_metrics = get_metrics().unwrap();
    // Channel for actually communicating indexed object changes between the ingestion pipeline and the committer.
    let (indexed_obj_sender, indexed_obj_receiver) = mysten_metrics::metered_channel::channel(
        // TODO: placeholder for now
        600,
        &global_metrics
            .channel_inflight
            .with_label_values(&["obj_indexing_for_snapshot"]),
    );

    // Start an ingestion pipeline with the objects snapshot processor as a worker.
    let worker = ObjectsSnapshotProcessor::<S, T>::new(
        store.clone(),
        indexed_obj_sender,
        commit_receiver,
        metrics.clone(),
    );
    let mut executor = IndexerExecutor::new(
        ShimProgressStore(watermark),
        1,
        DataIngestionMetrics::new(&Registry::new()),
    );
    let worker_pool = WorkerPool::new(
        worker,
        "objects_snapshot_worker".to_string(),
        download_queue_size,
    );

    executor.register(worker_pool).await?;

    spawn_monitored_task!(async move {
        info!("Starting data ingestion executor for processing objects snapshot...");
        let _ = executor
            .run(
                config
                    .data_ingestion_path
                    .clone()
                    .unwrap_or(tempfile::tempdir().unwrap().into_path()),
                config.remote_store_url.clone(),
                vec![],
                reader_options,
                exit_receiver,
            )
            .await;
    });

    // Now start the task that will commit the indexed object changes to the store.
    spawn_monitored_task!(ObjectsSnapshotProcessor::<S, T>::commit_objects_snapshot(
        store,
        watermark,
        indexed_obj_receiver,
        commit_notifier,
        metrics,
        snapshot_config,
        cancel,
    ));
    Ok(())
}

impl<S, T> ObjectsSnapshotProcessor<S, T>
where
    S: IndexerStore + Clone + Sync + Send + 'static,
    T: R2D2Connection + 'static,
{
    pub fn new(
        store: S,
        indexed_obj_sender: Sender<CheckpointObjectChanges>,
        commit_receiver: watch::Receiver<Option<CheckpointSequenceNumber>>,
        metrics: IndexerMetrics,
    ) -> ObjectsSnapshotProcessor<S, T> {
        // Start the package buffer used for buffering packages before they are written to the db.
        // We include a commit receiver which will be paged when a checkpoint has been processed and
        // the corresponding package data can be deleted from the buffer.
        let package_buffer = IndexingPackageBuffer::start(commit_receiver);
        let pg_blocking_cp = CheckpointHandler::pg_blocking_cp(store.clone()).unwrap();
        let package_db_resolver = IndexerStorePackageResolver::new(pg_blocking_cp);
        let in_mem_package_resolver = InterimPackageResolver::new(
            package_db_resolver,
            package_buffer.clone(),
            metrics.clone(),
        );
        let cached_package_resolver = PackageStoreWithLruCache::new(in_mem_package_resolver);
        let package_resolver = Arc::new(Resolver::new(cached_package_resolver));

        Self {
            store,
            indexed_obj_sender,
            package_resolver,
            package_buffer,
            metrics,
        }
    }

    // Receives object changes from the ingestion pipeline and commits them to the store,
    // keeping the appropriate amount of checkpoint lag behind the rest of the indexer.
    pub async fn commit_objects_snapshot(
        store: S,
        watermark: CheckpointSequenceNumber,
        indexed_obj_receiver: Receiver<CheckpointObjectChanges>,
        commit_notifier: watch::Sender<Option<CheckpointSequenceNumber>>,
        metrics: IndexerMetrics,
        config: SnapshotLagConfig,
        cancel: CancellationToken,
    ) -> IndexerResult<()> {
        let batch_size = 100;
        let mut stream = mysten_metrics::metered_channel::ReceiverStream::new(indexed_obj_receiver)
            .ready_chunks(batch_size);

        let mut start_cp = watermark;

        info!("Starting objects snapshot committer...");
        loop {
            tokio::select! {
                _ = cancel.cancelled() => {
                    info!("Shutdown signal received, terminating object snapshot processor");
                    return Ok(());
                }
                _ = tokio::time::sleep(std::time::Duration::from_secs(config.sleep_duration)) => {
                    let latest_indexer_cp = store
                        .get_latest_checkpoint_sequence_number()
                        .await?
                        .unwrap_or_default();

                    // We update the snapshot table when it falls behind the rest of the indexer by more than the min_lag.
                    while latest_indexer_cp >= start_cp + config.snapshot_min_lag as u64 {
                        // Stream the next object changes to be committed to the store.
                        if let Some(object_changes_batch) = stream.next().await {
                            let first_checkpoint_seq = object_changes_batch.first().as_ref().unwrap().checkpoint_sequence_number;
                            let last_checkpoint_seq = object_changes_batch.last().as_ref().unwrap().checkpoint_sequence_number;
                            info!("Objects snapshot processor is updating objects snapshot table from {} to {}", first_checkpoint_seq, last_checkpoint_seq);

                            let changes_to_commit = object_changes_batch.into_iter().map(|obj| obj.object_changes).collect();
                            store.backfill_objects_snapshot(changes_to_commit)
                                .await
                                .unwrap_or_else(|_| panic!("Failed to backfill objects snapshot from {} to {}", first_checkpoint_seq, last_checkpoint_seq));
                            start_cp = last_checkpoint_seq + 1;

                            // Tells the package buffer that this checkpoint has been processed and the corresponding package data can be deleted.
                            commit_notifier.send(Some(last_checkpoint_seq)).expect("Commit watcher should not be closed");
                            metrics
                                .latest_object_snapshot_sequence_number
                                .set(last_checkpoint_seq as i64);
                        }
                    }
                }
            }
        }
    }
}
