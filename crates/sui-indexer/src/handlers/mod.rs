// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeMap;

use async_trait::async_trait;
use futures::{FutureExt, StreamExt};

use serde::{Deserialize, Serialize};
use sui_rpc_api::CheckpointData;
use tokio_util::sync::CancellationToken;

use crate::{
    errors::IndexerError,
    models::{
        display::StoredDisplay,
        epoch::{EndOfEpochUpdate, StartOfEpochUpdate},
        obj_indices::StoredObjectVersion,
    },
    types::{
        EventIndex, IndexedCheckpoint, IndexedDeletedObject, IndexedEvent, IndexedObject,
        IndexedPackage, IndexedTransaction, IndexerResult, TxIndex,
    },
};

pub mod checkpoint_handler;
pub mod committer;
pub mod objects_snapshot_handler;
pub mod pruner;
pub mod tx_processor;

pub(crate) const CHECKPOINT_COMMIT_BATCH_SIZE: usize = 100;
pub(crate) const UNPROCESSED_CHECKPOINT_SIZE_LIMIT: usize = 1000;

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
    pub last_epoch: Option<EndOfEpochUpdate>,
    pub new_epoch: StartOfEpochUpdate,
}

impl EpochToCommit {
    pub fn new_epoch_id(&self) -> u64 {
        self.new_epoch.epoch as u64
    }

    pub fn new_epoch_first_checkpoint_id(&self) -> u64 {
        self.new_epoch.first_checkpoint_id as u64
    }

    pub fn last_epoch_total_transactions(&self) -> Option<u64> {
        self.last_epoch
            .as_ref()
            .map(|e| e.epoch_total_transactions as u64)
    }

    pub fn new_epoch_first_tx_sequence_number(&self) -> u64 {
        self.new_epoch.first_tx_sequence_number as u64
    }
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
        cp_receiver: mysten_metrics::metered_channel::Receiver<(CommitterWatermark, T)>,
        cancel: CancellationToken,
        start_checkpoint: u64,
        end_checkpoint_opt: Option<u64>,
    ) -> IndexerResult<()> {
        let checkpoint_commit_batch_size = std::env::var("CHECKPOINT_COMMIT_BATCH_SIZE")
            .unwrap_or(CHECKPOINT_COMMIT_BATCH_SIZE.to_string())
            .parse::<usize>()
            .unwrap();
        let mut stream = mysten_metrics::metered_channel::ReceiverStream::new(cp_receiver)
            .ready_chunks(checkpoint_commit_batch_size);

        // Mapping of ordered checkpoint data to ensure that we process them in order. The key is
        // just the checkpoint sequence number, and the tuple is (CommitterWatermark, T).
        let mut unprocessed: BTreeMap<u64, (CommitterWatermark, _)> = BTreeMap::new();
        let mut tuple_batch = vec![];
        let mut next_cp_to_process = start_checkpoint;

        loop {
            if cancel.is_cancelled() {
                return Ok(());
            }

            // Try to fetch new data tuple from the stream
            if unprocessed.len() >= UNPROCESSED_CHECKPOINT_SIZE_LIMIT {
                tracing::info!(
                    "Unprocessed checkpoint size reached limit {}, skip reading from stream...",
                    UNPROCESSED_CHECKPOINT_SIZE_LIMIT
                );
            } else {
                // Try to fetch new data tuple from the stream
                match stream.next().now_or_never() {
                    Some(Some(tuple_chunk)) => {
                        if cancel.is_cancelled() {
                            return Ok(());
                        }
                        for tuple in tuple_chunk {
                            unprocessed.insert(tuple.0.checkpoint_hi_inclusive, tuple);
                        }
                    }
                    Some(None) => break, // Stream has ended
                    None => {}           // No new data tuple available right now
                }
            }

            // Process unprocessed checkpoints, even no new checkpoints from stream
            let checkpoint_lag_limiter = self.handler.get_max_committable_checkpoint().await?;
            let max_commitable_cp = std::cmp::min(
                checkpoint_lag_limiter,
                end_checkpoint_opt.unwrap_or(u64::MAX),
            );
            // Stop pushing to tuple_batch if we've reached the end checkpoint.
            while next_cp_to_process <= max_commitable_cp {
                if let Some(data_tuple) = unprocessed.remove(&next_cp_to_process) {
                    tuple_batch.push(data_tuple);
                    next_cp_to_process += 1;
                } else {
                    break;
                }
            }

            if !tuple_batch.is_empty() {
                let committer_watermark = tuple_batch.last().unwrap().0;
                let batch = tuple_batch.into_iter().map(|t| t.1).collect();
                self.handler.load(batch).await.map_err(|e| {
                    IndexerError::PostgresWriteError(format!(
                        "Failed to load transformed data into DB for handler {}: {}",
                        self.handler.name(),
                        e
                    ))
                })?;
                self.handler.set_watermark_hi(committer_watermark).await?;
                tuple_batch = vec![];
            }

            if let Some(end_checkpoint) = end_checkpoint_opt {
                if next_cp_to_process > end_checkpoint {
                    tracing::info!(
                        "Reached end checkpoint, stopping handler {}...",
                        self.handler.name()
                    );
                    return Ok(());
                }
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

    /// Updates the relevant entries on the `watermarks` table with the full `CommitterWatermark`,
    /// which tracks the latest epoch, cp, and tx sequence number of the committed batch.
    async fn set_watermark_hi(&self, watermark: CommitterWatermark) -> IndexerResult<()>;

    /// By default, return u64::MAX, which means no extra waiting is needed before commiting;
    /// get max committable checkpoint, for handlers that want to wait for some condition before commiting,
    /// one use-case is the objects snapshot handler,
    /// which waits for the lag between snapshot and latest checkpoint to reach a certain threshold.
    async fn get_max_committable_checkpoint(&self) -> IndexerResult<u64> {
        Ok(u64::MAX)
    }
}

/// The indexer writer operates on checkpoint data, which contains information on the current epoch,
/// checkpoint, and transaction. These three numbers form the watermark upper bound for each
/// committed table. The reader and pruner are responsible for determining which of the three units
/// will be used for a particular table.
#[derive(Clone, Copy, Ord, PartialOrd, Eq, PartialEq)]
pub struct CommitterWatermark {
    pub epoch_hi_inclusive: u64,
    pub checkpoint_hi_inclusive: u64,
    pub tx_hi: u64,
}

impl From<&IndexedCheckpoint> for CommitterWatermark {
    fn from(checkpoint: &IndexedCheckpoint) -> Self {
        Self {
            epoch_hi_inclusive: checkpoint.epoch,
            checkpoint_hi_inclusive: checkpoint.sequence_number,
            tx_hi: checkpoint.network_total_transactions,
        }
    }
}

impl From<&CheckpointData> for CommitterWatermark {
    fn from(checkpoint: &CheckpointData) -> Self {
        Self {
            epoch_hi_inclusive: checkpoint.checkpoint_summary.epoch,
            checkpoint_hi_inclusive: checkpoint.checkpoint_summary.sequence_number,
            tx_hi: checkpoint.checkpoint_summary.network_total_transactions,
        }
    }
}

/// Enum representing tables that the committer handler writes to.
#[derive(
    Debug,
    Eq,
    PartialEq,
    strum_macros::Display,
    strum_macros::EnumString,
    strum_macros::EnumIter,
    strum_macros::AsRefStr,
    Hash,
    Serialize,
    Deserialize,
    Clone,
)]
#[strum(serialize_all = "snake_case")]
#[serde(rename_all = "snake_case")]
pub enum CommitterTables {
    // Unpruned tables
    ChainIdentifier,
    Display,
    Epochs,
    FeatureFlags,
    FullObjectsHistory,
    Objects,
    ObjectsVersion,
    Packages,
    ProtocolConfigs,
    RawCheckpoints,

    // Prunable tables
    ObjectsHistory,
    Transactions,
    Events,

    EventEmitPackage,
    EventEmitModule,
    EventSenders,
    EventStructInstantiation,
    EventStructModule,
    EventStructName,
    EventStructPackage,

    TxAffectedAddresses,
    TxAffectedObjects,
    TxCallsPkg,
    TxCallsMod,
    TxCallsFun,
    TxChangedObjects,
    TxDigests,
    TxInputObjects,
    TxKinds,

    Checkpoints,
    PrunerCpWatermark,
}

/// Enum representing tables that the objects snapshot handler writes to.
#[derive(
    Debug,
    Eq,
    PartialEq,
    strum_macros::Display,
    strum_macros::EnumString,
    strum_macros::EnumIter,
    strum_macros::AsRefStr,
    Hash,
    Serialize,
    Deserialize,
    Clone,
)]
#[strum(serialize_all = "snake_case")]
#[serde(rename_all = "snake_case")]
pub enum ObjectsSnapshotHandlerTables {
    ObjectsSnapshot,
}
