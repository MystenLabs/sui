// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::handlers::tx_processor::{InMemObjectCache, InMemPackageCache};
use crate::models_v2::transactions::StoredTransaction;
use async_trait::async_trait;
use move_binary_format::CompiledModule;
use move_bytecode_utils::module_cache::GetModule;
use move_core_types::language_storage::ModuleId;
use std::sync::{Arc, Mutex};
use sui_json_rpc_types::{
    Checkpoint as RpcCheckpoint, CheckpointId, EpochInfo, EventFilter, EventPage, SuiEvent,
    SuiTransactionBlockResponse, SuiTransactionBlockResponseOptions,
};
use sui_types::base_types::{EpochId, ObjectID, ObjectRef, SequenceNumber};
use sui_types::digests::CheckpointDigest;
use sui_types::event::EventID;
use sui_types::messages_checkpoint::CheckpointSequenceNumber;
use sui_types::object::{Object, ObjectRead};
use tap::TapFallible;

use crate::errors::IndexerError;
use crate::metrics::IndexerMetrics;

use crate::types_v2::{
    IndexedCheckpoint, IndexedEndOfEpochInfo, IndexedEpochInfo, IndexedEvent, IndexedObject,
    IndexedPackage, IndexedTransaction, IndexerResult, TxIndex,
};

#[async_trait]
pub trait IndexerStoreV2 {
    type ModuleCache: GetModule<Item = Arc<CompiledModule>, Error = anyhow::Error>
        + Send
        + Sync
        + 'static;

    async fn get_latest_tx_checkpoint_sequence_number(&self) -> Result<Option<u64>, IndexerError>;
    async fn get_checkpoint(&self, id: CheckpointId) -> Result<RpcCheckpoint, IndexerError>;
    async fn get_checkpoints(
        &self,
        cursor: Option<CheckpointId>,
        limit: usize,
    ) -> Result<Vec<RpcCheckpoint>, IndexerError>;

    async fn get_checkpoint_sequence_number(
        &self,
        digest: CheckpointDigest,
    ) -> Result<CheckpointSequenceNumber, IndexerError>;

    async fn get_event(&self, id: EventID) -> Result<SuiEvent, IndexerError>;
    async fn get_events(
        &self,
        query: EventFilter,
        cursor: Option<EventID>,
        limit: Option<usize>,
        descending_order: bool,
    ) -> Result<EventPage, IndexerError>;

    async fn get_object_read(
        &self,
        object_id: ObjectID,
        version: Option<SequenceNumber>,
    ) -> Result<ObjectRead, IndexerError>;

    async fn get_object(
        &self,
        object_id: ObjectID,
        version: Option<SequenceNumber>,
    ) -> Result<Option<Object>, IndexerError>;

    async fn get_total_transaction_number_from_checkpoints(&self) -> Result<i64, IndexerError>;

    // TODO: combine all get_transaction* methods
    async fn get_transaction_by_digest(
        &self,
        tx_digest: &str,
        options: Option<&SuiTransactionBlockResponseOptions>,
    ) -> Result<SuiTransactionBlockResponse, IndexerError>;

    async fn multi_get_transactions_by_digests(
        &self,
        tx_digests: &[String],
    ) -> Result<Vec<SuiTransactionBlockResponse>, IndexerError>;

    async fn persist_objects_and_checkpoints(
        &self,
        object_changes: Vec<TransactionObjectChangesV2>,
        checkpoints: Vec<IndexedCheckpoint>,
        metrics: IndexerMetrics,
    ) -> Result<(), IndexerError>;

    async fn persist_transactions(
        &self,
        transactions: Vec<IndexedTransaction>,
        metrics: IndexerMetrics,
    ) -> Result<(), IndexerError>;

    async fn persist_tx_indices(
        &self,
        indices: Vec<TxIndex>,
        metrics: IndexerMetrics,
    ) -> Result<(), IndexerError>;

    async fn persist_events(
        &self,
        events: Vec<IndexedEvent>,
        metrics: IndexerMetrics,
    ) -> Result<(), IndexerError>;

    async fn persist_packages(
        &self,
        packages: Vec<IndexedPackage>,
        metrics: IndexerMetrics,
    ) -> Result<(), IndexerError>;

    async fn persist_epoch(
        &self,
        data: TemporaryEpochStoreV2,
        metrics: IndexerMetrics,
    ) -> Result<(), IndexerError>;

    async fn get_checkpoint_ending_tx_sequence_number(
        &self,
        seq_num: CheckpointSequenceNumber,
    ) -> Result<Option<u64>, IndexerError>;

    async fn get_network_total_transactions_previous_epoch(
        &self,
        epoch: u64,
    ) -> Result<u64, IndexerError>;

    async fn get_epochs(
        &self,
        cursor: Option<EpochId>,
        limit: usize,
        descending_order: Option<bool>,
    ) -> Result<Vec<EpochInfo>, IndexerError>;

    async fn get_current_epoch(&self) -> Result<EpochInfo, IndexerError>;

    fn module_cache(&self) -> Arc<Self::ModuleCache>;

    fn indexer_metrics(&self) -> &IndexerMetrics;

    fn compose_sui_transaction_block_response(
        &self,
        tx: StoredTransaction,
        options: Option<&SuiTransactionBlockResponseOptions>,
    ) -> IndexerResult<SuiTransactionBlockResponse>;
}

#[derive(Debug)]
pub struct TemporaryCheckpointStoreV2 {
    pub checkpoint: IndexedCheckpoint,
    pub transactions: Vec<IndexedTransaction>,
    pub events: Vec<IndexedEvent>,
    pub tx_indices: Vec<TxIndex>,
    pub object_changes: TransactionObjectChangesV2,
    pub packages: Vec<IndexedPackage>,
    pub epoch: Option<TemporaryEpochStoreV2>,
}

#[derive(Debug)]
pub struct TransactionObjectChangesV2 {
    pub changed_objects: Vec<IndexedObject>,
    pub deleted_objects: Vec<ObjectRef>,
}

#[derive(Debug)]
pub struct TemporaryEpochStoreV2 {
    pub last_epoch: Option<IndexedEndOfEpochInfo>,
    pub new_epoch: IndexedEpochInfo,
}

pub struct InterimModuleResolver<GM>
where
    GM: GetModule<Item = Arc<CompiledModule>, Error = anyhow::Error>,
{
    backup: GM,
    package_cache: Arc<Mutex<InMemPackageCache>>,
    metrics: IndexerMetrics,
}

impl<GM> InterimModuleResolver<GM>
where
    GM: GetModule<Item = Arc<CompiledModule>, Error = anyhow::Error>,
{
    pub fn new(
        backup: GM,
        package_cache: Arc<Mutex<InMemPackageCache>>,
        new_packages: &[IndexedPackage],
        checkpoint_seq: CheckpointSequenceNumber,
        metrics: IndexerMetrics,
    ) -> Self {
        package_cache
            .lock()
            .unwrap()
            .insert_packages(new_packages, checkpoint_seq);
        Self {
            backup,
            package_cache,
            metrics,
        }
    }
}

impl<GM> GetModule for InterimModuleResolver<GM>
where
    GM: GetModule<Item = Arc<CompiledModule>, Error = anyhow::Error>,
{
    type Error = IndexerError;
    type Item = Arc<CompiledModule>;

    fn get_module_by_id(&self, id: &ModuleId) -> Result<Option<Arc<CompiledModule>>, Self::Error> {
        if let Some(m) = self.package_cache.lock().unwrap().get_module_by_id(id) {
            self.metrics.indexing_module_resolver_in_mem_hit.inc();
            Ok(Some(m))
        } else {
            self.backup
                .get_module_by_id(id)
                .map_err(|e| IndexerError::ModuleResolutionError(e.to_string()))
        }
    }
}
