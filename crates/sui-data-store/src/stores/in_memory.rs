// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! In-memory store with live object caching only.

use std::{
    collections::BTreeMap,
    io::Write,
    sync::{
        RwLock,
        atomic::{AtomicU64, Ordering},
    },
};

use anyhow::{Error, Result};
use sui_types::{
    base_types::ObjectID,
    digests::{CheckpointContentsDigest, CheckpointDigest},
    messages_checkpoint::CheckpointSequenceNumber,
    object::Object,
    supported_protocol_versions::{Chain, ProtocolConfig},
};

use crate::{
    CheckpointStore, CheckpointStoreWriter, EpochData, EpochStore, EpochStoreWriter,
    FullCheckpointData, ObjectKey, ObjectStore, ObjectStoreWriter, SetupStore, StoreSummary,
    TransactionInfo, TransactionStore, TransactionStoreWriter, VersionQuery, node::Node,
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

#[derive(Default)]
struct ObjectMetrics {
    version_hit: AtomicU64,
    version_miss: AtomicU64,
    root_hit: AtomicU64,
    root_miss: AtomicU64,
    checkpoint_hit: AtomicU64,
    checkpoint_miss: AtomicU64,
}

struct InMemoryStoreInner {
    node: Node,
    object_cache: BTreeMap<ObjectID, BTreeMap<u64, Object>>,
    root_version_cache: BTreeMap<(ObjectID, u64), u64>,
    checkpoint_cache: BTreeMap<(ObjectID, u64), u64>,
    metrics: ObjectMetrics,
}

/// Unbounded in-memory store.
pub struct InMemoryStore(RwLock<InMemoryStoreInner>);

impl InMemoryStore {
    /// Create a new in-memory store.
    pub fn new(node: Node) -> Self {
        Self(RwLock::new(InMemoryStoreInner {
            node,
            object_cache: BTreeMap::new(),
            root_version_cache: BTreeMap::new(),
            checkpoint_cache: BTreeMap::new(),
            metrics: ObjectMetrics::default(),
        }))
    }

    /// Return the chain associated with the configured node.
    pub fn chain(&self) -> Chain {
        self.0.read().unwrap().node.chain()
    }

    /// Return the configured node.
    pub fn node(&self) -> Node {
        self.0.read().unwrap().node.clone()
    }

    /// Clear all caches.
    pub fn clear_all_caches(&self) {
        let mut inner = self.0.write().unwrap();
        inner.object_cache.clear();
        inner.root_version_cache.clear();
        inner.checkpoint_cache.clear();
    }

    /// Return current cache sizes.
    pub fn cache_stats(&self) -> CacheStats {
        let inner = self.0.read().unwrap();
        let object_cache_size = inner
            .object_cache
            .values()
            .map(std::collections::BTreeMap::len)
            .sum();

        CacheStats {
            transaction_cache_size: 0,
            epoch_data_cache_size: 0,
            checkpoint_data_cache_size: 0,
            checkpoint_digest_cache_size: 0,
            checkpoint_contents_digest_cache_size: 0,
            object_cache_size,
            root_version_cache_size: inner.root_version_cache.len(),
            object_checkpoint_map_cache_size: inner.checkpoint_cache.len(),
        }
    }

    /// Add transaction data to the cache.
    pub fn add_transaction_data(&self, _tx_digest: String, _transaction_info: TransactionInfo) {
        todo!("in-memory transaction insertion is not implemented in the PR2 slice")
    }

    /// Add epoch data to the cache.
    pub fn add_epoch_data(&self, _epoch: u64, _epoch_data: EpochData) {
        todo!("in-memory epoch insertion is not implemented in the PR2 slice")
    }

    /// Add checkpoint data to the cache.
    pub fn add_checkpoint_data(&self, _checkpoint: FullCheckpointData) {
        todo!("in-memory checkpoint insertion is not implemented in the PR2 slice")
    }

    /// Add object data to the cache.
    pub fn add_object_data(&self, object_id: ObjectID, version: u64, object: Object) {
        self.0
            .write()
            .unwrap()
            .object_cache
            .entry(object_id)
            .or_default()
            .insert(version, object);
    }
}

impl TransactionStore for InMemoryStore {
    fn transaction_data_and_effects(
        &self,
        _tx_digest: &str,
    ) -> Result<Option<TransactionInfo>, Error> {
        todo!("in-memory transaction reads are not implemented in the PR2 slice")
    }
}

impl TransactionStoreWriter for InMemoryStore {
    fn write_transaction(
        &self,
        _tx_digest: &str,
        _transaction_info: TransactionInfo,
    ) -> Result<(), Error> {
        todo!("in-memory transaction writes are not implemented in the PR2 slice")
    }
}

impl EpochStore for InMemoryStore {
    fn epoch_info(&self, _epoch: u64) -> Result<Option<EpochData>, Error> {
        todo!("in-memory epoch reads are not implemented in the PR2 slice")
    }

    fn protocol_config(&self, _epoch: u64) -> Result<Option<ProtocolConfig>, Error> {
        todo!("in-memory protocol-config reads are not implemented in the PR2 slice")
    }
}

impl EpochStoreWriter for InMemoryStore {
    fn write_epoch_info(&self, _epoch: u64, _epoch_data: EpochData) -> Result<(), Error> {
        todo!("in-memory epoch writes are not implemented in the PR2 slice")
    }
}

impl ObjectStore for InMemoryStore {
    fn get_objects(&self, keys: &[ObjectKey]) -> Result<Vec<Option<(Object, u64)>>, Error> {
        let inner = self.0.read().unwrap();
        let mut results = Vec::with_capacity(keys.len());

        for key in keys {
            let (object_and_version, hit_counter, miss_counter) = match &key.version_query {
                VersionQuery::Version(version) => {
                    let object = inner
                        .object_cache
                        .get(&key.object_id)
                        .and_then(|versions| versions.get(version))
                        .cloned()
                        .map(|object| (object, *version));
                    (
                        object,
                        &inner.metrics.version_hit,
                        &inner.metrics.version_miss,
                    )
                }
                VersionQuery::RootVersion(root_version) => {
                    let actual_version = inner
                        .root_version_cache
                        .get(&(key.object_id, *root_version))
                        .copied();
                    let object = actual_version.and_then(|actual_version| {
                        inner
                            .object_cache
                            .get(&key.object_id)
                            .and_then(|versions| versions.get(&actual_version))
                            .cloned()
                            .map(|object| (object, actual_version))
                    });
                    (object, &inner.metrics.root_hit, &inner.metrics.root_miss)
                }
                VersionQuery::AtCheckpoint(checkpoint) => {
                    let actual_version = inner
                        .checkpoint_cache
                        .get(&(key.object_id, *checkpoint))
                        .copied();
                    let object = actual_version.and_then(|actual_version| {
                        inner
                            .object_cache
                            .get(&key.object_id)
                            .and_then(|versions| versions.get(&actual_version))
                            .cloned()
                            .map(|object| (object, actual_version))
                    });
                    (
                        object,
                        &inner.metrics.checkpoint_hit,
                        &inner.metrics.checkpoint_miss,
                    )
                }
            };

            if object_and_version.is_some() {
                hit_counter.fetch_add(1, Ordering::Relaxed);
            } else {
                miss_counter.fetch_add(1, Ordering::Relaxed);
            }
            results.push(object_and_version);
        }

        Ok(results)
    }
}

impl ObjectStoreWriter for InMemoryStore {
    fn write_object(
        &self,
        key: &ObjectKey,
        object: Object,
        actual_version: u64,
    ) -> Result<(), Error> {
        let mut inner = self.0.write().unwrap();
        inner
            .object_cache
            .entry(key.object_id)
            .or_default()
            .insert(actual_version, object);

        match &key.version_query {
            VersionQuery::Version(_) => {}
            VersionQuery::RootVersion(root_version) => {
                inner
                    .root_version_cache
                    .insert((key.object_id, *root_version), actual_version);
            }
            VersionQuery::AtCheckpoint(checkpoint) => {
                inner
                    .checkpoint_cache
                    .insert((key.object_id, *checkpoint), actual_version);
            }
        }

        Ok(())
    }
}

impl CheckpointStore for InMemoryStore {
    fn get_checkpoint_by_sequence_number(
        &self,
        _sequence: CheckpointSequenceNumber,
    ) -> Result<Option<FullCheckpointData>, Error> {
        todo!("in-memory checkpoint reads are not implemented in the PR2 slice")
    }

    fn get_latest_checkpoint(&self) -> Result<Option<FullCheckpointData>, Error> {
        todo!("in-memory latest-checkpoint lookups are not implemented in the PR2 slice")
    }

    fn get_sequence_by_checkpoint_digest(
        &self,
        _digest: &CheckpointDigest,
    ) -> Result<Option<CheckpointSequenceNumber>, Error> {
        todo!("in-memory checkpoint-digest lookups are not implemented in the PR2 slice")
    }

    fn get_sequence_by_contents_digest(
        &self,
        _digest: &CheckpointContentsDigest,
    ) -> Result<Option<CheckpointSequenceNumber>, Error> {
        todo!("in-memory contents-digest lookups are not implemented in the PR2 slice")
    }
}

impl CheckpointStoreWriter for InMemoryStore {
    fn write_checkpoint(&self, _checkpoint: &FullCheckpointData) -> Result<(), Error> {
        todo!("in-memory checkpoint writes are not implemented in the PR2 slice")
    }
}

impl SetupStore for InMemoryStore {
    fn setup(&self, _chain_id: Option<String>) -> Result<Option<String>, Error> {
        Ok(None)
    }
}

impl StoreSummary for InMemoryStore {
    fn summary<W: Write>(&self, writer: &mut W) -> Result<()> {
        let stats = self.cache_stats();
        let inner = self.0.read().unwrap();

        writeln!(writer, "InMemoryStore(node={})", inner.node.network_name())?;
        writeln!(writer, "  object_versions={}", stats.object_cache_size)?;
        writeln!(
            writer,
            "  root_version_entries={}",
            stats.root_version_cache_size
        )?;
        writeln!(
            writer,
            "  checkpoint_version_entries={}",
            stats.object_checkpoint_map_cache_size
        )?;
        writeln!(
            writer,
            "  hits(version/root/checkpoint)=({}/{}/{})",
            inner.metrics.version_hit.load(Ordering::Relaxed),
            inner.metrics.root_hit.load(Ordering::Relaxed),
            inner.metrics.checkpoint_hit.load(Ordering::Relaxed)
        )?;
        writeln!(
            writer,
            "  misses(version/root/checkpoint)=({}/{}/{})",
            inner.metrics.version_miss.load(Ordering::Relaxed),
            inner.metrics.root_miss.load(Ordering::Relaxed),
            inner.metrics.checkpoint_miss.load(Ordering::Relaxed)
        )?;
        Ok(())
    }
}
