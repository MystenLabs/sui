// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeMap;

use async_trait::async_trait;
use futures::{FutureExt, StreamExt};

use sui_rest_api::CheckpointData;
use tokio_util::sync::CancellationToken;

use crate::{
    errors::IndexerError,
    models::{display::StoredDisplay, obj_indices::StoredObjectVersion},
    types::{
        EventIndex, IndexedCheckpoint, IndexedDeletedObject, IndexedEpochInfo, IndexedEvent,
        IndexedObject, IndexedPackage, IndexedTransaction, IndexerResult, TxIndex,
    },
};

pub mod checkpoint_handler;
pub mod committer;
pub mod objects_snapshot_processor;
pub mod pruner;
pub mod tx_processor;

pub(crate) const CHECKPOINT_COMMIT_BATCH_SIZE: usize = 100;

#[derive(Debug)]
pub struct CheckpointDataToCommit {
    pub checkpoint: IndexedCheckpoint,
    pub transactions: Vec<IndexedTransaction>,
    pub events: Vec<IndexedEvent>,
    pub event_indices: Vec<EventIndex>,
    pub tx_indices: Vec<TxIndex>,
    pub display_updates: BTreeMap<String, StoredDisplay>,
    pub object_changes: TransactionObjectChangesToCommit,
    pub object_history_changes: TransactionObjectChangesToCommit,
    pub object_versions: Vec<StoredObjectVersion>,
    pub packages: Vec<IndexedPackage>,
    pub epoch: Option<EpochToCommit>,
}

#[derive(Clone, Debug)]
pub struct TransactionObjectChangesToCommit {
    pub changed_objects: Vec<IndexedObject>,
    pub deleted_objects: Vec<IndexedDeletedObject>,
}

#[derive(Clone, Debug)]
pub struct EpochToCommit {
    pub last_epoch: Option<IndexedEpochInfo>,
    pub new_epoch: IndexedEpochInfo,
    pub network_total_transactions: u64,
}

pub struct CommonHandler<T> {
    handler: Box<dyn Handler<T>>,
}

impl<T> CommonHandler<T> {
    pub fn new(handler: Box<dyn Handler<T>>) -> Self {
        Self { handler }
    }

    async fn start_transform_and_load(
        &self,
        cp_receiver: mysten_metrics::metered_channel::Receiver<CheckpointData>,
        cancel: CancellationToken,
    ) -> IndexerResult<()> {
        let checkpoint_commit_batch_size = std::env::var("CHECKPOINT_COMMIT_BATCH_SIZE")
            .unwrap_or(CHECKPOINT_COMMIT_BATCH_SIZE.to_string())
            .parse::<usize>()
            .unwrap();
        let mut stream = mysten_metrics::metered_channel::ReceiverStream::new(cp_receiver)
            .ready_chunks(checkpoint_commit_batch_size);

        let mut unprocessed = BTreeMap::new();
        let mut batch: Vec<CheckpointData> = vec![];
        let mut next_cp_to_process = self
            .handler
            .get_watermark_hi()
            .await?
            .map(|n| n.saturating_add(1))
            .unwrap_or_default();

        loop {
            if cancel.is_cancelled() {
                return Ok(());
            }

            // Try to fetch new checkpoints from the stream
            match stream.next().now_or_never() {
                Some(Some(indexed_checkpoint_batch)) => {
                    if cancel.is_cancelled() {
                        return Ok(());
                    }
                    for checkpoint in indexed_checkpoint_batch {
                        unprocessed
                            .insert(checkpoint.checkpoint_summary.sequence_number, checkpoint);
                    }
                }
                Some(None) => break, // Stream has ended
                None => {}           // No new checkpoints available right now
            }

            // Process unprocessed checkpoints, even no new checkpoints from stream
            let checkpoint_lag_limiter = self.handler.get_checkpoint_lag_limiter().await?;
            while next_cp_to_process <= checkpoint_lag_limiter {
                if let Some(checkpoint) = unprocessed.remove(&next_cp_to_process) {
                    batch.push(checkpoint);
                    next_cp_to_process += 1;
                } else {
                    break;
                }
            }

            if !batch.is_empty() {
                let last_checkpoint_seq = batch.last().unwrap().checkpoint_summary.sequence_number;
                let transformed_data = self.handler.transform(batch).await.map_err(|e| {
                    IndexerError::DataTransformationError(format!(
                        "Failed to transform checkpoint batch: {}. Handler: {}",
                        e,
                        self.handler.name()
                    ))
                })?;
                self.handler.load(transformed_data).await.map_err(|e| {
                    IndexerError::PostgresWriteError(format!(
                        "Failed to load transformed data into DB for handler {}: {}",
                        self.handler.name(),
                        e
                    ))
                })?;
                self.handler.set_watermark_hi(last_checkpoint_seq).await?;
                batch = vec![];
            }
        }
        Err(IndexerError::ChannelClosed(format!(
            "Checkpoint channel is closed unexpectedly for handler {}",
            self.handler.name()
        )))
    }
}

#[async_trait]
pub trait Handler<T>: Send + Sync {
    /// return handler name
    fn name(&self) -> String;

    /// transform data from `CheckpointData` to .*ToCommit
    async fn transform(&self, cp_batch: Vec<CheckpointData>) -> IndexerResult<Vec<T>>;

    /// commit .*ToCommit to DB
    async fn load(&self, tranformed_data: Vec<T>) -> IndexerResult<()>;

    /// read high watermark of the table DB
    async fn get_watermark_hi(&self) -> IndexerResult<Option<u64>>;

    /// set high watermark of the table DB, also update metrics.
    async fn set_watermark_hi(&self, watermark_hi: u64) -> IndexerResult<()>;

    /// get checkpoint lag limiter, for handlers without lag limiter, return u64::MAX
    async fn get_checkpoint_lag_limiter(&self) -> IndexerResult<u64> {
        Ok(u64::MAX)
    }
}
