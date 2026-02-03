// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

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

/// All pipeline names registered by the indexer. Single source of truth used for:
/// - Pipeline registration in `BigTableIndexer::new()`
/// - Per-pipeline watermark queries in `get_latest_checkpoint()`
/// - Legacy watermark tracker expected count
pub const ALL_PIPELINE_NAMES: [&str; 8] = [
    <BigTableHandler<CheckpointsPipeline> as sui_indexer_alt_framework::pipeline::Processor>::NAME,
    <BigTableHandler<CheckpointsByDigestPipeline> as sui_indexer_alt_framework::pipeline::Processor>::NAME,
    <BigTableHandler<TransactionsPipeline> as sui_indexer_alt_framework::pipeline::Processor>::NAME,
    <BigTableHandler<ObjectsPipeline> as sui_indexer_alt_framework::pipeline::Processor>::NAME,
    <BigTableHandler<ObjectTypesPipeline> as sui_indexer_alt_framework::pipeline::Processor>::NAME,
    <BigTableHandler<EpochStartPipeline> as sui_indexer_alt_framework::pipeline::Processor>::NAME,
    <BigTableHandler<EpochEndPipeline> as sui_indexer_alt_framework::pipeline::Processor>::NAME,
    <EpochLegacyPipeline as sui_indexer_alt_framework::pipeline::Processor>::NAME,
];

use std::sync::OnceLock;

use anyhow::Result;
use async_trait::async_trait;
use prometheus::Registry;
use serde::Deserialize;
use serde::Serialize;
use sui_indexer_alt_framework::Indexer;
use sui_indexer_alt_framework::IndexerArgs;
use sui_indexer_alt_framework::ingestion::ClientArgs;
use sui_indexer_alt_framework::ingestion::IngestionConfig;
use sui_indexer_alt_framework::pipeline::concurrent::ConcurrentConfig;
use sui_types::balance_change::BalanceChange;
use sui_types::base_types::ObjectID;
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

mod bigtable;
mod handlers;
pub mod tables;

static WRITE_LEGACY_DATA: OnceLock<bool> = OnceLock::new();

/// Set whether to write legacy data (legacy watermark row, epoch DEFAULT_COLUMN, tx column).
/// Must be called before creating any pipelines. Panics if called more than once.
pub fn set_write_legacy_data(value: bool) {
    WRITE_LEGACY_DATA
        .set(value)
        .expect("write_legacy_data already set");
}

pub fn write_legacy_data() -> bool {
    *WRITE_LEGACY_DATA.get_or_init(|| false)
}

pub struct BigTableIndexer {
    pub indexer: Indexer<BigTableStore>,
}

#[derive(Clone, Debug)]
pub struct Checkpoint {
    pub summary: CheckpointSummary,
    pub contents: CheckpointContents,
    pub signatures: AuthorityStrongQuorumSignInfo,
}

#[derive(Clone, Debug)]
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
#[derive(Clone, Debug)]
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
pub struct Watermark {
    pub epoch_hi_inclusive: u64,
    pub checkpoint_hi_inclusive: u64,
    pub tx_hi: u64,
    pub timestamp_ms_hi_inclusive: u64,
}

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
}

impl BigTableIndexer {
    pub async fn new(
        store: BigTableStore,
        indexer_args: IndexerArgs,
        client_args: ClientArgs,
        ingestion_config: IngestionConfig,
        config: ConcurrentConfig,
        registry: &Registry,
    ) -> Result<Self> {
        let mut indexer = Indexer::new(
            store,
            indexer_args,
            client_args,
            ingestion_config,
            None,
            registry,
        )
        .await?;

        indexer
            .concurrent_pipeline(BigTableHandler::new(CheckpointsPipeline), config.clone())
            .await?;
        indexer
            .concurrent_pipeline(
                BigTableHandler::new(CheckpointsByDigestPipeline),
                config.clone(),
            )
            .await?;
        indexer
            .concurrent_pipeline(BigTableHandler::new(TransactionsPipeline), config.clone())
            .await?;
        indexer
            .concurrent_pipeline(BigTableHandler::new(ObjectsPipeline), config.clone())
            .await?;
        indexer
            .concurrent_pipeline(BigTableHandler::new(ObjectTypesPipeline), config.clone())
            .await?;
        indexer
            .concurrent_pipeline(BigTableHandler::new(EpochStartPipeline), config.clone())
            .await?;
        indexer
            .concurrent_pipeline(BigTableHandler::new(EpochEndPipeline), config.clone())
            .await?;

        if write_legacy_data() {
            indexer
                .concurrent_pipeline(EpochLegacyPipeline, config)
                .await?;
        }

        Ok(Self { indexer })
    }

    pub fn pipeline_names(&self) -> Vec<&'static str> {
        self.indexer.pipelines().collect()
    }
}

impl From<sui_indexer_alt_framework_store_traits::CommitterWatermark> for Watermark {
    fn from(w: sui_indexer_alt_framework_store_traits::CommitterWatermark) -> Self {
        Self {
            epoch_hi_inclusive: w.epoch_hi_inclusive,
            checkpoint_hi_inclusive: w.checkpoint_hi_inclusive,
            tx_hi: w.tx_hi,
            timestamp_ms_hi_inclusive: w.timestamp_ms_hi_inclusive,
        }
    }
}

impl From<Watermark> for sui_indexer_alt_framework_store_traits::CommitterWatermark {
    fn from(w: Watermark) -> Self {
        Self {
            epoch_hi_inclusive: w.epoch_hi_inclusive,
            checkpoint_hi_inclusive: w.checkpoint_hi_inclusive,
            tx_hi: w.tx_hi,
            timestamp_ms_hi_inclusive: w.timestamp_ms_hi_inclusive,
        }
    }
}
