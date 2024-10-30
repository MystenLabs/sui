// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeMap;

use async_trait::async_trait;
use strum::IntoEnumIterator;

use crate::errors::IndexerError;
use crate::handlers::pruner::PrunableTable;
use crate::handlers::{CommitterWatermark, EpochToCommit, TransactionObjectChangesToCommit};
use crate::models::display::StoredDisplay;
use crate::models::obj_indices::StoredObjectVersion;
use crate::models::objects::{StoredDeletedObject, StoredObject};
use crate::models::raw_checkpoints::StoredRawCheckpoint;
use crate::models::watermarks::StoredWatermark;
use crate::types::{
    EventIndex, IndexedCheckpoint, IndexedEvent, IndexedPackage, IndexedTransaction, TxIndex,
};

#[allow(clippy::large_enum_variant)]
pub enum ObjectsToCommit {
    MutatedObject(StoredObject),
    DeletedObject(StoredDeletedObject),
}

#[async_trait]
pub trait IndexerStore: Clone + Sync + Send + 'static {
    async fn get_latest_checkpoint_sequence_number(&self) -> Result<Option<u64>, IndexerError>;

    async fn get_available_epoch_range(&self) -> Result<(u64, u64), IndexerError>;

    async fn get_available_checkpoint_range(&self) -> Result<(u64, u64), IndexerError>;

    async fn get_latest_object_snapshot_checkpoint_sequence_number(
        &self,
    ) -> Result<Option<u64>, IndexerError>;

    async fn get_chain_identifier(&self) -> Result<Option<Vec<u8>>, IndexerError>;

    async fn persist_protocol_configs_and_feature_flags(
        &self,
        chain_id: Vec<u8>,
    ) -> Result<(), IndexerError>;

    async fn persist_objects(
        &self,
        object_changes: Vec<TransactionObjectChangesToCommit>,
    ) -> Result<(), IndexerError>;

    async fn persist_object_history(
        &self,
        object_changes: Vec<TransactionObjectChangesToCommit>,
    ) -> Result<(), IndexerError>;

    async fn persist_full_objects_history(
        &self,
        object_changes: Vec<TransactionObjectChangesToCommit>,
    ) -> Result<(), IndexerError>;

    async fn persist_objects_version(
        &self,
        object_versions: Vec<StoredObjectVersion>,
    ) -> Result<(), IndexerError>;

    async fn persist_objects_snapshot(
        &self,
        object_changes: Vec<TransactionObjectChangesToCommit>,
    ) -> Result<(), IndexerError>;

    async fn persist_checkpoints(
        &self,
        checkpoints: Vec<IndexedCheckpoint>,
    ) -> Result<(), IndexerError>;

    async fn persist_chain_identifier(
        &self,
        checkpoint_digest: Vec<u8>,
    ) -> Result<(), IndexerError>;

    async fn persist_transactions(
        &self,
        transactions: Vec<IndexedTransaction>,
    ) -> Result<(), IndexerError>;

    async fn persist_tx_indices(&self, indices: Vec<TxIndex>) -> Result<(), IndexerError>;

    async fn persist_events(&self, events: Vec<IndexedEvent>) -> Result<(), IndexerError>;
    async fn persist_event_indices(
        &self,
        event_indices: Vec<EventIndex>,
    ) -> Result<(), IndexerError>;

    async fn persist_displays(
        &self,
        display_updates: BTreeMap<String, StoredDisplay>,
    ) -> Result<(), IndexerError>;

    async fn persist_packages(&self, packages: Vec<IndexedPackage>) -> Result<(), IndexerError>;

    /// Updates the current epoch with end-of-epoch data, and writes a new epoch to the database.
    async fn persist_epoch(&self, epoch: EpochToCommit) -> Result<(), IndexerError>;

    /// Updates epoch-partitioned tables to accept data from the new epoch.
    async fn advance_epoch(&self, epoch: EpochToCommit) -> Result<(), IndexerError>;

    async fn prune_epoch(&self, epoch: u64) -> Result<(), IndexerError>;

    async fn get_network_total_transactions_by_end_of_epoch(
        &self,
        epoch: u64,
    ) -> Result<Option<u64>, IndexerError>;

    async fn upload_display(&self, epoch: u64) -> Result<(), IndexerError>;

    async fn restore_display(&self, bytes: bytes::Bytes) -> Result<(), IndexerError>;

    async fn persist_raw_checkpoints(
        &self,
        checkpoints: Vec<StoredRawCheckpoint>,
    ) -> Result<(), IndexerError>;

    /// Update the upper bound of the watermarks for the given tables.
    async fn update_watermarks_upper_bound<E: IntoEnumIterator>(
        &self,
        watermark: CommitterWatermark,
    ) -> Result<(), IndexerError>
    where
        E::Iterator: Iterator<Item: AsRef<str>>;

    /// Updates each watermark entry's lower bounds per the list of tables and their new epoch lower
    /// bounds.
    async fn update_watermarks_lower_bound(
        &self,
        watermarks: Vec<(PrunableTable, u64)>,
    ) -> Result<(), IndexerError>;

    /// Load all watermark entries from the store, and the latest timestamp from the db.
    async fn get_watermarks(&self) -> Result<(Vec<StoredWatermark>, i64), IndexerError>;
}
