// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::io::Write;

use anyhow::{Error, Result};

use sui_types::{
    base_types::ObjectID,
    digests::{CheckpointContentsDigest, CheckpointDigest},
    messages_checkpoint::CheckpointSequenceNumber,
    object::Object,
    supported_protocol_versions::{Chain, ProtocolConfig},
};

use crate::{
    CheckpointData, CheckpointStore, CheckpointStoreWriter, EpochData, EpochStore,
    EpochStoreWriter, ObjectKey, ObjectStore, ObjectStoreWriter, SetupStore, StoreSummary,
    TransactionInfo, TransactionStore, TransactionStoreWriter, node::Node,
};

/// Cheap summary of the in-memory caches.
#[derive(Clone, Debug, Default)]
pub struct CacheStats {
    pub transaction_cache_size: usize,
    pub epoch_data_cache_size: usize,
    pub checkpoint_data_cache_size: usize,
    pub checkpoint_digest_cache_size: usize,
    pub checkpoint_contents_digest_cache_size: usize,
    pub object_cache_size: usize,
    pub root_version_cache_size: usize,
    pub object_checkpoint_map_cache_size: usize,
}

/// Unbounded in-memory store.
#[derive(Debug)]
pub struct InMemoryStore {
    node: Node,
}

impl InMemoryStore {
    /// Create a new in-memory store.
    pub fn new(node: Node) -> Self {
        Self { node }
    }

    /// Return the chain associated with the configured node.
    pub fn chain(&self) -> Chain {
        self.node.chain()
    }

    /// Return the configured node.
    pub fn node(&self) -> &Node {
        &self.node
    }

    /// Clear all caches.
    pub fn clear_all_caches(&self) {
        todo!("in-memory cache clearing is not implemented in the skeleton")
    }

    /// Return current cache sizes.
    pub fn cache_stats(&self) -> CacheStats {
        CacheStats::default()
    }

    /// Add transaction data to the cache.
    pub fn add_transaction_data(&self, _tx_digest: String, _transaction_info: TransactionInfo) {
        todo!("in-memory transaction insertion is not implemented in the skeleton")
    }

    /// Add epoch data to the cache.
    pub fn add_epoch_data(&self, _epoch: u64, _epoch_data: EpochData) {
        todo!("in-memory epoch insertion is not implemented in the skeleton")
    }

    /// Add checkpoint data to the cache.
    pub fn add_checkpoint_data(&self, _checkpoint: CheckpointData) {
        todo!("in-memory checkpoint insertion is not implemented in the skeleton")
    }

    /// Add object data to the cache.
    pub fn add_object_data(&self, _object_id: ObjectID, _version: u64, _object: Object) {
        todo!("in-memory object insertion is not implemented in the skeleton")
    }
}

impl TransactionStore for InMemoryStore {
    fn transaction_data_and_effects(
        &self,
        _tx_digest: &str,
    ) -> Result<Option<TransactionInfo>, Error> {
        todo!("in-memory transaction reads are not implemented in the skeleton")
    }
}

impl TransactionStoreWriter for InMemoryStore {
    fn write_transaction(
        &self,
        _tx_digest: &str,
        _transaction_info: TransactionInfo,
    ) -> Result<(), Error> {
        todo!("in-memory transaction writes are not implemented in the skeleton")
    }
}

impl EpochStore for InMemoryStore {
    fn epoch_info(&self, _epoch: u64) -> Result<Option<EpochData>, Error> {
        todo!("in-memory epoch reads are not implemented in the skeleton")
    }

    fn protocol_config(&self, _epoch: u64) -> Result<Option<ProtocolConfig>, Error> {
        todo!("in-memory protocol-config reads are not implemented in the skeleton")
    }
}

impl EpochStoreWriter for InMemoryStore {
    fn write_epoch_info(&self, _epoch: u64, _epoch_data: EpochData) -> Result<(), Error> {
        todo!("in-memory epoch writes are not implemented in the skeleton")
    }
}

impl ObjectStore for InMemoryStore {
    fn get_objects(&self, _keys: &[ObjectKey]) -> Result<Vec<Option<(Object, u64)>>, Error> {
        todo!("in-memory object reads are not implemented in the skeleton")
    }
}

impl ObjectStoreWriter for InMemoryStore {
    fn write_object(
        &self,
        _key: &ObjectKey,
        _object: Object,
        _actual_version: u64,
    ) -> Result<(), Error> {
        todo!("in-memory object writes are not implemented in the skeleton")
    }
}

impl CheckpointStore for InMemoryStore {
    fn get_checkpoint_by_sequence_number(
        &self,
        _sequence: CheckpointSequenceNumber,
    ) -> Result<Option<CheckpointData>, Error> {
        todo!("in-memory checkpoint reads are not implemented in the skeleton")
    }

    fn get_latest_checkpoint(&self) -> Result<Option<CheckpointData>, Error> {
        todo!("in-memory latest-checkpoint lookup is not implemented in the skeleton")
    }

    fn get_sequence_by_checkpoint_digest(
        &self,
        _digest: &CheckpointDigest,
    ) -> Result<Option<CheckpointSequenceNumber>, Error> {
        todo!("in-memory checkpoint-digest lookups are not implemented in the skeleton")
    }

    fn get_sequence_by_contents_digest(
        &self,
        _digest: &CheckpointContentsDigest,
    ) -> Result<Option<CheckpointSequenceNumber>, Error> {
        todo!("in-memory contents-digest lookups are not implemented in the skeleton")
    }
}

impl CheckpointStoreWriter for InMemoryStore {
    fn write_checkpoint(&self, _checkpoint: &CheckpointData) -> Result<(), Error> {
        todo!("in-memory checkpoint writes are not implemented in the skeleton")
    }
}

impl SetupStore for InMemoryStore {
    fn setup(&self, _chain_id: Option<String>) -> Result<Option<String>, Error> {
        todo!("in-memory setup is not implemented in the skeleton")
    }
}

impl StoreSummary for InMemoryStore {
    fn summary<W: Write>(&self, writer: &mut W) -> Result<()> {
        writeln!(writer, "InMemoryStore(node={})", self.node.network_name())?;
        Ok(())
    }
}
