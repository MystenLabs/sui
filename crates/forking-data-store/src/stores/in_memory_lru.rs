// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{io::Write, num::NonZeroUsize, sync::RwLock};

use anyhow::{Error, Result};
use lru::LruCache;
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

const DEFAULT_EPOCH_CAP: usize = 10_000;
const DEFAULT_CHECKPOINT_CAP: usize = 50_000;

#[derive(Debug)]
struct LruMemoryStoreInner {
    epoch_data_cache: LruCache<u64, EpochData>,
    checkpoint_data_cache: LruCache<CheckpointSequenceNumber, CheckpointData>,
    checkpoint_digest_cache: LruCache<CheckpointDigest, CheckpointSequenceNumber>,
    checkpoint_contents_digest_cache: LruCache<CheckpointContentsDigest, CheckpointSequenceNumber>,
    latest_checkpoint: Option<CheckpointData>,
}

/// Bounded in-memory epoch/checkpoint store.
#[derive(Debug)]
pub struct LruMemoryStore {
    node: Node,
    epoch_cap: usize,
    checkpoint_cap: usize,
    inner: RwLock<LruMemoryStoreInner>,
}

impl LruMemoryStore {
    /// Create a new LRU store with default capacities.
    pub fn new(node: Node) -> Self {
        Self::with_capacities(node, DEFAULT_EPOCH_CAP, DEFAULT_CHECKPOINT_CAP)
    }

    /// Create a new LRU store with explicit capacities.
    pub fn with_capacities(node: Node, epoch_cap: usize, checkpoint_cap: usize) -> Self {
        let cap =
            |value: usize| NonZeroUsize::new(value.max(1)).expect("capacity must be non-zero");
        Self {
            node,
            epoch_cap,
            checkpoint_cap,
            inner: RwLock::new(LruMemoryStoreInner {
                epoch_data_cache: LruCache::new(cap(epoch_cap)),
                checkpoint_data_cache: LruCache::new(cap(checkpoint_cap)),
                checkpoint_digest_cache: LruCache::new(cap(checkpoint_cap)),
                checkpoint_contents_digest_cache: LruCache::new(cap(checkpoint_cap)),
                latest_checkpoint: None,
            }),
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
}

impl EpochStore for LruMemoryStore {
    /// Read epoch metadata from the LRU cache.
    fn epoch_info(&self, epoch: u64) -> Result<Option<EpochData>, Error> {
        Ok(self
            .inner
            .write()
            .expect("LRU store lock poisoned")
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

impl EpochStoreWriter for LruMemoryStore {
    /// Cache epoch metadata, evicting older entries when needed.
    fn write_epoch_info(&self, epoch: u64, epoch_data: EpochData) -> Result<(), Error> {
        self.inner
            .write()
            .expect("LRU store lock poisoned")
            .epoch_data_cache
            .put(epoch, epoch_data);
        Ok(())
    }
}

impl CheckpointStore for LruMemoryStore {
    /// Read a checkpoint payload from the LRU cache.
    fn get_checkpoint_by_sequence_number(
        &self,
        sequence: CheckpointSequenceNumber,
    ) -> Result<Option<CheckpointData>, Error> {
        Ok(self
            .inner
            .write()
            .expect("LRU store lock poisoned")
            .checkpoint_data_cache
            .get(&sequence)
            .cloned())
    }

    /// Return the latest checkpoint tracked separately from the LRU map.
    fn get_latest_checkpoint(&self) -> Result<Option<CheckpointData>, Error> {
        Ok(self
            .inner
            .read()
            .expect("LRU store lock poisoned")
            .latest_checkpoint
            .clone())
    }

    /// Resolve a checkpoint digest through the LRU reverse index.
    fn get_sequence_by_checkpoint_digest(
        &self,
        digest: &CheckpointDigest,
    ) -> Result<Option<CheckpointSequenceNumber>, Error> {
        Ok(self
            .inner
            .write()
            .expect("LRU store lock poisoned")
            .checkpoint_digest_cache
            .get(digest)
            .copied())
    }

    /// Resolve a contents digest through the LRU reverse index.
    fn get_sequence_by_contents_digest(
        &self,
        digest: &CheckpointContentsDigest,
    ) -> Result<Option<CheckpointSequenceNumber>, Error> {
        Ok(self
            .inner
            .write()
            .expect("LRU store lock poisoned")
            .checkpoint_contents_digest_cache
            .get(digest)
            .copied())
    }
}

impl CheckpointStoreWriter for LruMemoryStore {
    /// Cache the checkpoint payload and keep the latest pointer monotonic.
    fn write_checkpoint(&self, checkpoint: &CheckpointData) -> Result<(), Error> {
        let mut inner = self.inner.write().expect("LRU store lock poisoned");
        let sequence = checkpoint.summary.data().sequence_number;
        inner
            .checkpoint_data_cache
            .put(sequence, checkpoint.clone());
        inner
            .checkpoint_digest_cache
            .put(checkpoint.summary.data().digest(), sequence);
        inner
            .checkpoint_contents_digest_cache
            .put(*checkpoint.contents.digest(), sequence);

        let should_update_latest = inner
            .latest_checkpoint
            .as_ref()
            .is_none_or(|latest| latest.summary.data().sequence_number <= sequence);
        if should_update_latest {
            inner.latest_checkpoint = Some(checkpoint.clone());
        }

        Ok(())
    }
}

impl SetupStore for LruMemoryStore {
    /// LRU stores do not persist chain mappings, so setup is a no-op.
    fn setup(&self, _chain_id: Option<String>) -> Result<Option<String>, Error> {
        Ok(None)
    }
}

impl StoreSummary for LruMemoryStore {
    /// Print the configured capacities for debugging and test output.
    fn summary<W: Write>(&self, writer: &mut W) -> Result<()> {
        writeln!(
            writer,
            "LruMemoryStore(node={}, epoch_cap={}, checkpoint_cap={})",
            self.node.network_name(),
            self.epoch_cap,
            self.checkpoint_cap
        )?;
        Ok(())
    }
}
