// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeMap;

use async_trait::async_trait;
use futures::{FutureExt, StreamExt};

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
pub mod objects_snapshot_handler;
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
        cp_receiver: mysten_metrics::metered_channel::Receiver<(u64, T)>,
        cancel: CancellationToken,
    ) -> IndexerResult<()> {
        let checkpoint_commit_batch_size = std::env::var("CHECKPOINT_COMMIT_BATCH_SIZE")
            .unwrap_or(CHECKPOINT_COMMIT_BATCH_SIZE.to_string())
            .parse::<usize>()
            .unwrap();
        let mut stream = mysten_metrics::metered_channel::ReceiverStream::new(cp_receiver)
            .ready_chunks(checkpoint_commit_batch_size);

        let mut unprocessed = BTreeMap::new();
        let mut tuple_batch = vec![];
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

            // Try to fetch new data tuple from the stream
            match stream.next().now_or_never() {
                Some(Some(tuple_chunk)) => {
                    if cancel.is_cancelled() {
                        return Ok(());
                    }
                    for tuple in tuple_chunk {
                        unprocessed.insert(tuple.0, tuple);
                    }
                }
                Some(None) => break, // Stream has ended
                None => {}           // No new data tuple available right now
            }

            // Process unprocessed checkpoints, even no new checkpoints from stream
            let checkpoint_lag_limiter = self.handler.get_max_committable_checkpoint().await?;
            while next_cp_to_process <= checkpoint_lag_limiter {
                if let Some(data_tuple) = unprocessed.remove(&next_cp_to_process) {
                    tuple_batch.push(data_tuple);
                    next_cp_to_process += 1;
                } else {
                    break;
                }
            }

            if !tuple_batch.is_empty() {
                let last_checkpoint_seq = tuple_batch.last().unwrap().0;
                let batch = tuple_batch.into_iter().map(|t| t.1).collect();
                self.handler.load(batch).await.map_err(|e| {
                    IndexerError::PostgresWriteError(format!(
                        "Failed to load transformed data into DB for handler {}: {}",
                        self.handler.name(),
                        e
                    ))
                })?;
                self.handler.set_watermark_hi(last_checkpoint_seq).await?;
                tuple_batch = vec![];
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

    /// commit batch of transformed data to DB
    async fn load(&self, batch: Vec<T>) -> IndexerResult<()>;

    /// read high watermark of the table DB
    async fn get_watermark_hi(&self) -> IndexerResult<Option<u64>>;

    /// set high watermark of the table DB, also update metrics.
    async fn set_watermark_hi(&self, watermark_hi: u64) -> IndexerResult<()>;

    /// By default, return u64::MAX, which means no extra waiting is needed before commiting;
    /// get max committable checkpoint, for handlers that want to wait for some condition before commiting,
    /// one use-case is the objects snapshot handler,
    /// which waits for the lag between snapshot and latest checkpoint to reach a certain threshold.
    async fn get_max_committable_checkpoint(&self) -> IndexerResult<u64> {
        Ok(u64::MAX)
    }
}
