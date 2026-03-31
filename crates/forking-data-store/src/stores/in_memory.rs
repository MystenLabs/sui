// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{
    collections::{BTreeMap, HashMap},
    io::Write,
    sync::RwLock,
};

use anyhow::{Error, Result};
use sui_types::{
    committee::ProtocolVersion,
    digests::{CheckpointContentsDigest, CheckpointDigest},
    message_envelope::Message as _,
    messages_checkpoint::CheckpointSequenceNumber,
    supported_protocol_versions::{Chain, ProtocolConfig},
};

use crate::{
    CheckpointData, CheckpointStore, CheckpointStoreWriter, EpochData, EpochStore,
    EpochStoreWriter, SetupStore, StoreSummary, node::Node,
};

/// Cheap summary of the in-memory epoch/checkpoint caches.
#[derive(Clone, Debug, Default)]
pub struct CacheStats {
    pub epoch_data_cache_size: usize,
    pub checkpoint_data_cache_size: usize,
    pub checkpoint_digest_cache_size: usize,
    pub checkpoint_contents_digest_cache_size: usize,
}

#[derive(Debug, Default)]
struct InMemoryStoreInner {
    epoch_data_cache: BTreeMap<u64, EpochData>,
    checkpoint_data_cache: BTreeMap<CheckpointSequenceNumber, CheckpointData>,
    checkpoint_digest_cache: HashMap<CheckpointDigest, CheckpointSequenceNumber>,
    checkpoint_contents_digest_cache: HashMap<CheckpointContentsDigest, CheckpointSequenceNumber>,
}

/// Unbounded in-memory epoch/checkpoint store.
#[derive(Debug)]
pub struct InMemoryStore {
    node: Node,
    inner: RwLock<InMemoryStoreInner>,
}

impl InMemoryStore {
    /// Create a new in-memory store.
    pub fn new(node: Node) -> Self {
        Self {
            node,
            inner: RwLock::new(InMemoryStoreInner::default()),
        }
    }

    /// Return the chain associated with the configured node.
    pub fn chain(&self) -> Chain {
        self.node.chain()
    }

    /// Return the configured node.
    pub fn node(&self) -> &Node {
        &self.node
    }

    /// Clear all caches maintained by this store.
    pub fn clear_all_caches(&self) {
        let mut inner = self.inner.write().expect("in-memory store lock poisoned");
        inner.epoch_data_cache.clear();
        inner.checkpoint_data_cache.clear();
        inner.checkpoint_digest_cache.clear();
        inner.checkpoint_contents_digest_cache.clear();
    }

    /// Return current cache sizes.
    pub fn cache_stats(&self) -> CacheStats {
        let inner = self.inner.read().expect("in-memory store lock poisoned");
        CacheStats {
            epoch_data_cache_size: inner.epoch_data_cache.len(),
            checkpoint_data_cache_size: inner.checkpoint_data_cache.len(),
            checkpoint_digest_cache_size: inner.checkpoint_digest_cache.len(),
            checkpoint_contents_digest_cache_size: inner.checkpoint_contents_digest_cache.len(),
        }
    }

    /// Add epoch data to the cache.
    pub fn add_epoch_data(&self, epoch: u64, epoch_data: EpochData) {
        self.inner
            .write()
            .expect("in-memory store lock poisoned")
            .epoch_data_cache
            .insert(epoch, epoch_data);
    }

    /// Add checkpoint data and its reverse indexes to the cache.
    pub fn add_checkpoint_data(&self, checkpoint: CheckpointData) {
        let _ = self.write_checkpoint(&checkpoint);
    }
}

impl EpochStore for InMemoryStore {
    /// Read cached epoch metadata by epoch number.
    fn epoch_info(&self, epoch: u64) -> Result<Option<EpochData>, Error> {
        Ok(self
            .inner
            .read()
            .expect("in-memory store lock poisoned")
            .epoch_data_cache
            .get(&epoch)
            .cloned())
    }

    /// Derive the protocol config from cached epoch metadata.
    fn protocol_config(&self, epoch: u64) -> Result<Option<ProtocolConfig>, Error> {
        Ok(self.epoch_info(epoch)?.map(|epoch_data| {
            ProtocolConfig::get_for_version(
                ProtocolVersion::new(epoch_data.protocol_version),
                self.chain(),
            )
        }))
    }
}

impl EpochStoreWriter for InMemoryStore {
    /// Cache epoch metadata by epoch number.
    fn write_epoch_info(&self, epoch: u64, epoch_data: EpochData) -> Result<(), Error> {
        self.inner
            .write()
            .expect("in-memory store lock poisoned")
            .epoch_data_cache
            .insert(epoch, epoch_data);
        Ok(())
    }
}

impl CheckpointStore for InMemoryStore {
    /// Read a checkpoint payload from the sequence-number cache.
    fn get_checkpoint_by_sequence_number(
        &self,
        sequence: CheckpointSequenceNumber,
    ) -> Result<Option<CheckpointData>, Error> {
        Ok(self
            .inner
            .read()
            .expect("in-memory store lock poisoned")
            .checkpoint_data_cache
            .get(&sequence)
            .cloned())
    }

    /// Return the highest-sequence checkpoint cached so far.
    fn get_latest_checkpoint(&self) -> Result<Option<CheckpointData>, Error> {
        Ok(self
            .inner
            .read()
            .expect("in-memory store lock poisoned")
            .checkpoint_data_cache
            .last_key_value()
            .map(|(_, checkpoint)| checkpoint.clone()))
    }

    /// Resolve a checkpoint digest through the cached reverse index.
    fn get_sequence_by_checkpoint_digest(
        &self,
        digest: &CheckpointDigest,
    ) -> Result<Option<CheckpointSequenceNumber>, Error> {
        Ok(self
            .inner
            .read()
            .expect("in-memory store lock poisoned")
            .checkpoint_digest_cache
            .get(digest)
            .copied())
    }

    /// Resolve a contents digest through the cached reverse index.
    fn get_sequence_by_contents_digest(
        &self,
        digest: &CheckpointContentsDigest,
    ) -> Result<Option<CheckpointSequenceNumber>, Error> {
        Ok(self
            .inner
            .read()
            .expect("in-memory store lock poisoned")
            .checkpoint_contents_digest_cache
            .get(digest)
            .copied())
    }
}

impl CheckpointStoreWriter for InMemoryStore {
    /// Cache the checkpoint payload and its reverse indexes.
    fn write_checkpoint(&self, checkpoint: &CheckpointData) -> Result<(), Error> {
        let mut inner = self.inner.write().expect("in-memory store lock poisoned");
        let sequence = checkpoint.summary.data().sequence_number;
        inner
            .checkpoint_digest_cache
            .insert(checkpoint.summary.data().digest(), sequence);
        inner
            .checkpoint_contents_digest_cache
            .insert(*checkpoint.contents.digest(), sequence);
        inner
            .checkpoint_data_cache
            .insert(sequence, checkpoint.clone());
        Ok(())
    }
}

impl SetupStore for InMemoryStore {
    /// In-memory stores do not persist chain mappings, so setup is a no-op.
    fn setup(&self, _chain_id: Option<String>) -> Result<Option<String>, Error> {
        Ok(None)
    }
}

impl StoreSummary for InMemoryStore {
    /// Print current cache sizes for debugging and test output.
    fn summary<W: Write>(&self, writer: &mut W) -> Result<()> {
        let stats = self.cache_stats();
        writeln!(
            writer,
            "InMemoryStore(node={}, epoch_cache={}, checkpoint_cache={}, checkpoint_digest_cache={}, checkpoint_contents_cache={})",
            self.node.network_name(),
            stats.epoch_data_cache_size,
            stats.checkpoint_data_cache_size,
            stats.checkpoint_digest_cache_size,
            stats.checkpoint_contents_digest_cache_size
        )?;
        Ok(())
    }
}
