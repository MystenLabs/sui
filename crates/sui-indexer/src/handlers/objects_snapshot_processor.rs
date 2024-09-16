// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use async_trait::async_trait;
use futures::StreamExt;
use mysten_metrics::get_metrics;
use mysten_metrics::metered_channel::{Receiver, Sender};
use mysten_metrics::spawn_monitored_task;
use sui_data_ingestion_core::Worker;
use sui_package_resolver::{PackageStoreWithLruCache, Resolver};
use sui_rest_api::CheckpointData;
use sui_types::messages_checkpoint::CheckpointSequenceNumber;
use tokio::sync::watch;
use tokio_util::sync::CancellationToken;
use tracing::info;

use crate::config::SnapshotLagConfig;
use crate::store::package_resolver::{IndexerStorePackageResolver, InterimPackageResolver};
use crate::store::PgIndexerStore;
use crate::types::IndexerResult;
use crate::{metrics::IndexerMetrics, store::IndexerStore};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use super::checkpoint_handler::CheckpointHandler;
use super::tx_processor::IndexingPackageBuffer;
use super::TransactionObjectChangesToCommit;

pub struct ObjectsSnapshotProcessor {
    pub store: PgIndexerStore,
    package_buffer: Arc<Mutex<IndexingPackageBuffer>>,
    package_resolver: Arc<Resolver<PackageStoreWithLruCache<InterimPackageResolver>>>,
    pub indexed_obj_sender: Sender<CheckpointObjectChanges>,
    metrics: IndexerMetrics,
}

pub struct CheckpointObjectChanges {
    pub checkpoint_sequence_number: u64,
    pub object_changes: TransactionObjectChangesToCommit,
}

#[async_trait]
impl Worker for ObjectsSnapshotProcessor {
    async fn process_checkpoint(&self, checkpoint: &CheckpointData) -> anyhow::Result<()> {
        let checkpoint_sequence_number = checkpoint.checkpoint_summary.sequence_number;
        // Index the object changes and send them to the committer.
        let object_changes: TransactionObjectChangesToCommit = CheckpointHandler::index_objects(
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

    fn preprocess_hook(&self, checkpoint: &CheckpointData) -> anyhow::Result<()> {
        let package_objects =
            CheckpointHandler::get_package_objects(std::slice::from_ref(checkpoint));
        self.package_buffer
            .lock()
            .unwrap()
            .insert_packages(package_objects);
        Ok(())
    }
}

// Start both the ingestion pipeline and committer for objects snapshot table.
pub async fn start_objects_snapshot_processor(
    store: PgIndexerStore,
    metrics: IndexerMetrics,
    snapshot_config: SnapshotLagConfig,
    cancel: CancellationToken,
) -> IndexerResult<(ObjectsSnapshotProcessor, u64)> {
    info!("Starting object snapshot processor...");

    let watermark = store
        .get_latest_object_snapshot_checkpoint_sequence_number()
        .await
        .expect("Failed to get latest snapshot checkpoint sequence number from DB")
        .map(|seq| seq + 1)
        .unwrap_or_default();

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
    let worker = ObjectsSnapshotProcessor::new(
        store.clone(),
        indexed_obj_sender,
        commit_receiver,
        metrics.clone(),
    );

    // Now start the task that will commit the indexed object changes to the store.
    spawn_monitored_task!(ObjectsSnapshotProcessor::commit_objects_snapshot(
        store,
        watermark,
        indexed_obj_receiver,
        commit_notifier,
        metrics,
        snapshot_config,
        cancel,
    ));
    Ok((worker, watermark))
}

impl ObjectsSnapshotProcessor {
    pub fn new(
        store: PgIndexerStore,
        indexed_obj_sender: Sender<CheckpointObjectChanges>,
        commit_receiver: watch::Receiver<Option<CheckpointSequenceNumber>>,
        metrics: IndexerMetrics,
    ) -> ObjectsSnapshotProcessor {
        // Start the package buffer used for buffering packages before they are written to the db.
        // We include a commit receiver which will be paged when a checkpoint has been processed and
        // the corresponding package data can be deleted from the buffer.
        let package_buffer = IndexingPackageBuffer::start(commit_receiver);
        let package_db_resolver = IndexerStorePackageResolver::new(store.pool());
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
        store: PgIndexerStore,
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
        // To prevent the processor from committing more data than allowed by the min lag, keep an
        // in-memory buffer of changes that should not be committed to `objects_snapshot` yet.
        let mut unprocessed = HashMap::new();
        let mut batch = vec![];

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

                    // We update the snapshot table when it falls behind the rest of the indexer by
                    // more than the min_lag. When `latest_indexer_cp = start_cp +
                    // config.snapshot_min_lag`, we have not actually indexed `start_cp` yet, hence
                    // why the condition is `>=`.
                    while latest_indexer_cp >= start_cp + config.snapshot_min_lag as u64 {
                        // The maximum checkpoint sequence number that can be committed to the
                        // `objects_snapshot` table.
                        let max_allowed_cp = latest_indexer_cp - config.snapshot_min_lag as u64;

                        if let Some(new_changes) = stream.next().await {
                            for checkpoint in new_changes {
                                unprocessed.insert(checkpoint.checkpoint_sequence_number, checkpoint);
                            }
                        }

                        // Collect the checkpoint object changes to write to `objects_snapshot`,
                        // stopping when there are gaps in the sequence of unprocessed checkpoints.
                        // This is an inclusive range, so if `start_cp` is equal to
                        // `max_allowed_cp`, we'll still index the one checkpoint.
                        for cp in start_cp..=max_allowed_cp {
                            if let Some(checkpoint) = unprocessed.remove(&cp) {
                                batch.push(checkpoint);
                            }
                            else {
                                break;
                            }
                        }

                        if !batch.is_empty() {
                            let first_checkpoint_seq = batch.first().as_ref().unwrap().checkpoint_sequence_number;
                            let last_checkpoint_seq = batch.last().as_ref().unwrap().checkpoint_sequence_number;
                            info!("Objects snapshot processor is updating objects snapshot table from {} to {}", first_checkpoint_seq, last_checkpoint_seq);

                            store.persist_objects_snapshot(batch.drain(..).map(|obj| obj.object_changes).collect())
                                .await
                                .unwrap_or_else(|_| panic!("Failed to backfill objects snapshot from {} to {}", first_checkpoint_seq, last_checkpoint_seq));
                            start_cp = last_checkpoint_seq + 1;

                            // Tells the package buffer that this checkpoint has been processed and
                            // the corresponding package data can be deleted.
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
