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
pub use crate::handlers::ObjectsPipeline;
pub use crate::handlers::PrevEpochUpdate;
pub use crate::handlers::TransactionsPipeline;
pub use crate::handlers::bigtable_max_mutations;

pub const CHECKPOINTS_PIPELINE: &str =
    <BigTableHandler<CheckpointsPipeline> as sui_indexer_alt_framework::pipeline::Processor>::NAME;
pub const CHECKPOINTS_BY_DIGEST_PIPELINE: &str =
    <BigTableHandler<CheckpointsByDigestPipeline> as sui_indexer_alt_framework::pipeline::Processor>::NAME;
pub const TRANSACTIONS_PIPELINE: &str =
    <BigTableHandler<TransactionsPipeline> as sui_indexer_alt_framework::pipeline::Processor>::NAME;
pub const OBJECTS_PIPELINE: &str =
    <BigTableHandler<ObjectsPipeline> as sui_indexer_alt_framework::pipeline::Processor>::NAME;
pub const EPOCH_START_PIPELINE: &str =
    <BigTableHandler<EpochStartPipeline> as sui_indexer_alt_framework::pipeline::Processor>::NAME;
pub const EPOCH_END_PIPELINE: &str =
    <BigTableHandler<EpochEndPipeline> as sui_indexer_alt_framework::pipeline::Processor>::NAME;
pub const EPOCH_LEGACY_PIPELINE: &str =
    <EpochLegacyPipeline as sui_indexer_alt_framework::pipeline::Processor>::NAME;

/// All pipeline names registered by the indexer. Single source of truth used for:
/// - Pipeline registration in `BigTableIndexer::new()`
/// - Per-pipeline watermark queries in `get_watermark()`
/// - Legacy watermark tracker expected count
pub const ALL_PIPELINE_NAMES: [&str; 7] = [
    CHECKPOINTS_PIPELINE,
    CHECKPOINTS_BY_DIGEST_PIPELINE,
    TRANSACTIONS_PIPELINE,
    OBJECTS_PIPELINE,
    EPOCH_START_PIPELINE,
    EPOCH_END_PIPELINE,
    EPOCH_LEGACY_PIPELINE,
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
use sui_indexer_alt_framework::pipeline::CommitterConfig;
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
use sui_types::storage::ObjectKey;
use sui_types::transaction::Transaction;

mod bigtable;
pub mod config;
mod handlers;
pub mod tables;

pub use config::ConcurrentLayer;
pub use config::IndexerConfig;
pub use config::IngestionConfig;
pub use config::PipelineLayer;

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
pub struct CheckpointData {
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

/// Epoch data returned by reader methods.
/// All fields are optional to support partial column queries.
#[derive(Clone, Debug, Default)]
pub struct EpochData {
    pub epoch: Option<u64>,
    pub protocol_version: Option<u64>,
    pub start_timestamp_ms: Option<u64>,
    pub start_checkpoint: Option<u64>,
    pub reference_gas_price: Option<u64>,
    pub system_state: Option<sui_types::sui_system_state::SuiSystemState>,
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

/// Non-legacy pipeline names used for the default `get_watermark` implementation.
const WATERMARK_PIPELINES: [&str; 6] = [
    CHECKPOINTS_PIPELINE,
    CHECKPOINTS_BY_DIGEST_PIPELINE,
    TRANSACTIONS_PIPELINE,
    OBJECTS_PIPELINE,
    EPOCH_START_PIPELINE,
    EPOCH_END_PIPELINE,
];

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
    ) -> Result<Vec<CheckpointData>>;
    async fn get_checkpoint_by_digest(
        &mut self,
        digest: CheckpointDigest,
    ) -> Result<Option<CheckpointData>>;
    /// Return the minimum watermark across the given pipelines, selecting the whole
    /// watermark with the lowest `checkpoint_hi_inclusive`. Returns `None` if any
    /// pipeline is missing a watermark.
    async fn get_watermark_for_pipelines(
        &mut self,
        pipelines: &[&str],
    ) -> Result<Option<Watermark>>;
    /// Return the minimum watermark across all non-legacy pipelines.
    async fn get_watermark(&mut self) -> Result<Option<Watermark>> {
        self.get_watermark_for_pipelines(&WATERMARK_PIPELINES).await
    }
    async fn get_latest_object(&mut self, object_id: &ObjectID) -> Result<Option<Object>>;
    async fn get_epoch(&mut self, epoch_id: EpochId) -> Result<Option<EpochData>>;
    async fn get_latest_epoch(&mut self) -> Result<Option<EpochData>>;
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
        committer: CommitterConfig,
        pipeline: PipelineLayer,
        registry: &Registry,
    ) -> Result<Self> {
        let mut indexer = Indexer::new(
            store,
            indexer_args,
            client_args,
            ingestion_config.into(),
            None,
            registry,
        )
        .await?;

        let base = ConcurrentConfig {
            committer,
            pruner: None,
        };

        indexer
            .concurrent_pipeline(
                BigTableHandler::new(CheckpointsPipeline),
                pipeline.checkpoints.finish(base.clone()),
            )
            .await?;
        indexer
            .concurrent_pipeline(
                BigTableHandler::new(CheckpointsByDigestPipeline),
                pipeline.checkpoints_by_digest.finish(base.clone()),
            )
            .await?;
        indexer
            .concurrent_pipeline(
                BigTableHandler::new(TransactionsPipeline),
                pipeline.transactions.finish(base.clone()),
            )
            .await?;
        indexer
            .concurrent_pipeline(
                BigTableHandler::new(ObjectsPipeline),
                pipeline.objects.finish(base.clone()),
            )
            .await?;
        indexer
            .concurrent_pipeline(
                BigTableHandler::new(EpochStartPipeline),
                pipeline.epoch_start.finish(base.clone()),
            )
            .await?;
        indexer
            .concurrent_pipeline(
                BigTableHandler::new(EpochEndPipeline),
                pipeline.epoch_end.finish(base.clone()),
            )
            .await?;

        if write_legacy_data() {
            indexer
                .concurrent_pipeline(EpochLegacyPipeline, pipeline.epoch_legacy.finish(base))
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
