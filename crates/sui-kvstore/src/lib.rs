// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::Result;
use async_trait::async_trait;
use serde::Deserialize;
use serde::Serialize;
use sui_types::balance_change::BalanceChange;
use sui_types::base_types::ObjectID;
use sui_types::base_types::ObjectType;
use sui_types::committee::EpochId;
use sui_types::crypto::AuthorityStrongQuorumSignInfo;
use sui_types::digests::CheckpointDigest;
use sui_types::digests::TransactionDigest;
use sui_types::effects::TransactionEffects;
use sui_types::effects::TransactionEvents;
use sui_types::event::Event;
use sui_types::messages_checkpoint::CheckpointContents;
use sui_types::messages_checkpoint::CheckpointSequenceNumber;
use sui_types::messages_checkpoint::CheckpointSummary;
use sui_types::object::Object;
use sui_types::storage::EpochInfo;
use sui_types::storage::ObjectKey;
use sui_types::transaction::Transaction;

pub use crate::bigtable::client::BigTableClient;
pub use crate::bigtable::store::BigTableConnection;
pub use crate::bigtable::store::BigTableStore;
pub use crate::handlers::BIGTABLE_MAX_MUTATIONS;
pub use crate::handlers::BigTableHandler;
pub use crate::handlers::CheckpointsByDigestPipeline;
pub use crate::handlers::CheckpointsPipeline;
pub use crate::handlers::EpochEndPipeline;
pub use crate::handlers::EpochLegacyBatch;
pub use crate::handlers::EpochLegacyPipeline;
pub use crate::handlers::EpochStartPipeline;
pub use crate::handlers::ObjectTypesPipeline;
pub use crate::handlers::ObjectsPipeline;
pub use crate::handlers::PrevEpochUpdate;
pub use crate::handlers::TransactionsPipeline;
pub use crate::handlers::set_max_mutations;

mod bigtable;
mod handlers;
pub mod tables;

#[async_trait]
pub trait KeyValueStoreReader {
    async fn get_objects(&mut self, objects: &[ObjectKey]) -> Result<Vec<Object>>;
    async fn get_transactions(
        &mut self,
        transactions: &[TransactionDigest],
    ) -> Result<Vec<TransactionData>>;
    async fn get_checkpoints(
        &mut self,
        sequence_numbers: &[CheckpointSequenceNumber],
    ) -> Result<Vec<Checkpoint>>;
    async fn get_checkpoint_by_digest(
        &mut self,
        digest: CheckpointDigest,
    ) -> Result<Option<Checkpoint>>;
    async fn get_latest_checkpoint(&mut self) -> Result<CheckpointSequenceNumber>;
    async fn get_latest_checkpoint_summary(&mut self) -> Result<Option<CheckpointSummary>>;
    async fn get_latest_object(&mut self, object_id: &ObjectID) -> Result<Option<Object>>;
    async fn get_epoch(&mut self, epoch_id: EpochId) -> Result<Option<EpochInfo>>;
    async fn get_latest_epoch(&mut self) -> Result<Option<EpochInfo>>;
    async fn get_events_for_transactions(
        &mut self,
        keys: &[TransactionDigest],
    ) -> Result<Vec<(TransactionDigest, TransactionEventsData)>>;
    async fn get_object_types(&mut self, object_ids: &[ObjectID]) -> Result<Vec<ObjectType>>;
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Checkpoint {
    pub summary: CheckpointSummary,
    pub contents: CheckpointContents,
    pub signatures: AuthorityStrongQuorumSignInfo,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TransactionData {
    pub transaction: Transaction,
    pub effects: TransactionEffects,
    pub events: Option<TransactionEvents>,
    pub checkpoint_number: CheckpointSequenceNumber,
    pub timestamp: u64,
    pub balance_changes: Vec<BalanceChange>,
    pub unchanged_loaded_runtime_objects: Vec<ObjectKey>,
}

/// Partial transaction and events for when we only need transaction content for events
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TransactionEventsData {
    pub events: Vec<Event>,
    pub timestamp_ms: u64,
}

/// Data written at epoch start (before transactions are processed)
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct EpochStartData {
    pub epoch: u64,
    pub protocol_version: Option<u64>,
    pub start_timestamp_ms: Option<u64>,
    pub start_checkpoint: Option<u64>,
    pub reference_gas_price: Option<u64>,
    pub system_state: Option<sui_types::sui_system_state::SuiSystemState>,
}

impl From<&EpochInfo> for EpochStartData {
    fn from(info: &EpochInfo) -> Self {
        Self {
            epoch: info.epoch,
            protocol_version: info.protocol_version,
            start_timestamp_ms: info.start_timestamp_ms,
            start_checkpoint: info.start_checkpoint,
            reference_gas_price: info.reference_gas_price,
            system_state: info.system_state.clone(),
        }
    }
}

/// Data written at epoch end (after the last transaction of the epoch)
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct EpochEndData {
    pub end_timestamp_ms: Option<u64>,
    pub end_checkpoint: Option<u64>,
}

/// Serializable watermark for per-pipeline tracking in BigTable.
/// Mirrors the framework's CommitterWatermark type.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PipelineWatermark {
    pub epoch_hi_inclusive: u64,
    pub checkpoint_hi_inclusive: u64,
    pub tx_hi: u64,
    pub timestamp_ms_hi_inclusive: u64,
}

impl From<sui_indexer_alt_framework_store_traits::CommitterWatermark> for PipelineWatermark {
    fn from(w: sui_indexer_alt_framework_store_traits::CommitterWatermark) -> Self {
        Self {
            epoch_hi_inclusive: w.epoch_hi_inclusive,
            checkpoint_hi_inclusive: w.checkpoint_hi_inclusive,
            tx_hi: w.tx_hi,
            timestamp_ms_hi_inclusive: w.timestamp_ms_hi_inclusive,
        }
    }
}

impl From<PipelineWatermark> for sui_indexer_alt_framework_store_traits::CommitterWatermark {
    fn from(w: PipelineWatermark) -> Self {
        Self {
            epoch_hi_inclusive: w.epoch_hi_inclusive,
            checkpoint_hi_inclusive: w.checkpoint_hi_inclusive,
            tx_hi: w.tx_hi,
            timestamp_ms_hi_inclusive: w.timestamp_ms_hi_inclusive,
        }
    }
}
