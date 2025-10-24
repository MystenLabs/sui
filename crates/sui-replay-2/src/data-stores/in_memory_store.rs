// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! In-memory implementation of the replay interfaces: `TransactionStore`, `EpochStore`, and `ObjectStore`.
//! The `InMemoryStore` provides fast in-memory lookups.
//!
//! This store is purely cache-based - it only returns data that has been explicitly stored in memory.
//! For `TransactionStore`, `EpochStore`, and `ObjectStore`, missing data results in `Ok(None)`.
//!
//! # Usage Examples
//!
//! ```ignore
//! use crate::data_stores::InMemoryStore;
//! use crate::Node;
//!
//! // Create an in-memory store
//! let store = InMemoryStore::new(Node::Mainnet);
//!
//! // Attempting to fetch data not in cache will return None
//! let result = store.epoch_info(123); // Returns Ok(None) since cache is empty
//!
//! // Data must be explicitly added to cache before it can be retrieved
//! // (This would typically be done by other parts of the system)
//! ```

use crate::{
    Node,
    replay_interface::{
        EpochData, EpochStore, EpochStoreWriter, ObjectKey, ObjectStore, ObjectStoreWriter,
        SetupStore, StoreSummary, TransactionInfo, TransactionStore, TransactionStoreWriter,
        VersionQuery,
    },
};
use anyhow::{Error, Result};
use std::{
    collections::BTreeMap,
    sync::{
        RwLock,
        atomic::{AtomicU64, Ordering},
    },
};
use sui_types::{
    base_types::ObjectID,
    committee::ProtocolVersion,
    object::Object,
    supported_protocol_versions::{Chain, ProtocolConfig},
};

/// In-memory store implementing the replay interfaces
struct InMemoryStoreInner {
    node: Node,
    transaction_cache: BTreeMap<String, TransactionInfo>,
    epoch_data_cache: BTreeMap<u64, EpochData>,
    object_cache: BTreeMap<ObjectID, BTreeMap<u64, Object>>,
    // The next 2 maps can be organized as flat maps or nested maps
    // and the best solution depends on usage patterns.
    // For now, we use flat maps but we may want to review in the future
    /// Cache mapping (ObjectID, root_version) -> actual_version
    /// Used for VersionQuery::RootVersion lookups
    root_version_cache: BTreeMap<(ObjectID, u64), u64>,
    /// Cache mapping (ObjectID, checkpoint) -> actual_version  
    /// Used for VersionQuery::AtCheckpoint lookups
    checkpoint_cache: BTreeMap<(ObjectID, u64), u64>,
    /// Metrics: hit/miss counters for API calls
    metrics: MemStoreMetrics,
}

// The RwLock is needed for 2 reasons:
// 1. to make tokio happy
// 2. to allow interior mutability for the cache
pub struct InMemoryStore(RwLock<InMemoryStoreInner>);

#[derive(Default)]
struct MemStoreMetrics {
    // transactions
    txn_hit: AtomicU64,
    txn_miss: AtomicU64,
    txn_error: AtomicU64,
    // epochs
    epoch_hit: AtomicU64,
    epoch_miss: AtomicU64,
    epoch_error: AtomicU64,
    // protocol config
    proto_hit: AtomicU64,
    proto_miss: AtomicU64,
    proto_error: AtomicU64,
    // objects by query kind
    obj_version_hit: AtomicU64,
    obj_version_miss: AtomicU64,
    obj_version_error: AtomicU64,
    obj_root_hit: AtomicU64,
    obj_root_miss: AtomicU64,
    obj_root_error: AtomicU64,
    obj_checkpoint_hit: AtomicU64,
    obj_checkpoint_miss: AtomicU64,
    obj_checkpoint_error: AtomicU64,
}

impl InMemoryStore {
    /// Create a new InMemoryStore with the given node
    pub fn new(node: Node) -> Self {
        Self(RwLock::new(InMemoryStoreInner {
            node,
            transaction_cache: BTreeMap::new(),
            epoch_data_cache: BTreeMap::new(),
            object_cache: BTreeMap::new(),
            root_version_cache: BTreeMap::new(),
            checkpoint_cache: BTreeMap::new(),
            metrics: MemStoreMetrics::default(),
        }))
    }

    /// Get the chain for this store
    pub fn chain(&self) -> Chain {
        self.0.read().unwrap().node.chain()
    }

    /// Get the node for this store
    pub fn node(&self) -> Node {
        self.0.read().unwrap().node.clone()
    }

    /// Clear all caches
    pub fn clear_all_caches(&self) {
        let mut inner = self.0.write().unwrap();
        inner.transaction_cache.clear();
        inner.epoch_data_cache.clear();
        inner.object_cache.clear();
        inner.root_version_cache.clear();
        inner.checkpoint_cache.clear();
    }

    /// Get cache statistics
    pub fn cache_stats(&self) -> CacheStats {
        let inner = self.0.read().unwrap();
        let object_cache_size = inner
            .object_cache
            .values()
            .map(|versions| versions.len())
            .sum();

        let root_version_cache_size = inner.root_version_cache.len();

        let checkpoint_cache_size = inner.checkpoint_cache.len();

        CacheStats {
            transaction_cache_size: inner.transaction_cache.len(),
            epoch_data_cache_size: inner.epoch_data_cache.len(),
            object_cache_size,
            root_version_cache_size,
            checkpoint_cache_size,
        }
    }

    /// Add transaction data to the cache
    pub fn add_transaction_data(&self, tx_digest: String, transaction_info: TransactionInfo) {
        self.0
            .write()
            .unwrap()
            .transaction_cache
            .insert(tx_digest, transaction_info);
    }

    /// Add epoch data to the cache
    pub fn add_epoch_data(&self, epoch: u64, epoch_data: EpochData) {
        self.0
            .write()
            .unwrap()
            .epoch_data_cache
            .insert(epoch, epoch_data);
    }

    /// Add object data to the cache
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

/// Statistics about cache usage
#[derive(Debug, Clone)]
pub struct CacheStats {
    pub transaction_cache_size: usize,
    pub epoch_data_cache_size: usize,
    pub object_cache_size: usize,
    pub root_version_cache_size: usize,
    pub checkpoint_cache_size: usize,
}

impl TransactionStore for InMemoryStore {
    fn transaction_data_and_effects(
        &self,
        tx_digest: &str,
    ) -> Result<Option<TransactionInfo>, Error> {
        let inner = self.0.read().unwrap();
        if let Some(cached) = inner.transaction_cache.get(tx_digest) {
            inner.metrics.txn_hit.fetch_add(1, Ordering::Relaxed);
            return Ok(Some(cached.clone()));
        }
        inner.metrics.txn_miss.fetch_add(1, Ordering::Relaxed);
        Ok(None)
    }
}

impl EpochStore for InMemoryStore {
    fn epoch_info(&self, epoch: u64) -> Result<Option<EpochData>, Error> {
        let inner = self.0.read().unwrap();
        if let Some(cached) = inner.epoch_data_cache.get(&epoch) {
            inner.metrics.epoch_hit.fetch_add(1, Ordering::Relaxed);
            return Ok(Some(cached.clone()));
        }
        inner.metrics.epoch_miss.fetch_add(1, Ordering::Relaxed);
        Ok(None)
    }

    fn protocol_config(&self, epoch: u64) -> Result<Option<ProtocolConfig>, Error> {
        match self.epoch_info(epoch) {
            Ok(Some(epoch_data)) => {
                let inner = self.0.read().unwrap();
                inner.metrics.proto_hit.fetch_add(1, Ordering::Relaxed);
                Ok(Some(ProtocolConfig::get_for_version(
                    ProtocolVersion::new(epoch_data.protocol_version),
                    self.chain(),
                )))
            }
            Ok(None) => {
                let inner = self.0.read().unwrap();
                inner.metrics.proto_miss.fetch_add(1, Ordering::Relaxed);
                Ok(None)
            }
            Err(e) => {
                let inner = self.0.read().unwrap();
                inner.metrics.proto_error.fetch_add(1, Ordering::Relaxed);
                Err(e)
            }
        }
    }
}

impl ObjectStore for InMemoryStore {
    fn get_objects(&self, keys: &[ObjectKey]) -> Result<Vec<Option<(Object, u64)>>, Error> {
        let mut results = Vec::with_capacity(keys.len());
        let inner = self.0.read().unwrap();

        // Check caches for each key
        for key in keys.iter() {
            let (object_and_version, hit_ctr, miss_ctr) = match &key.version_query {
                VersionQuery::Version(version) => {
                    let res = if let Some(versions_map) = inner.object_cache.get(&key.object_id) {
                        versions_map
                            .get(version)
                            .cloned()
                            .map(|obj| (obj, *version))
                    } else {
                        None
                    };
                    (
                        res,
                        &inner.metrics.obj_version_hit,
                        &inner.metrics.obj_version_miss,
                    )
                }
                VersionQuery::RootVersion(max_version) => {
                    let actual_version = inner
                        .root_version_cache
                        .get(&(key.object_id, *max_version))
                        .copied();
                    let res = actual_version.and_then(|actual_version| {
                        inner
                            .object_cache
                            .get(&key.object_id)
                            .and_then(|versions_map| versions_map.get(&actual_version))
                            .cloned()
                            .map(|obj| (obj, actual_version))
                    });
                    (
                        res,
                        &inner.metrics.obj_root_hit,
                        &inner.metrics.obj_root_miss,
                    )
                }
                VersionQuery::AtCheckpoint(checkpoint) => {
                    let actual_version = inner
                        .checkpoint_cache
                        .get(&(key.object_id, *checkpoint))
                        .copied();
                    let res = actual_version.and_then(|actual_version| {
                        inner
                            .object_cache
                            .get(&key.object_id)
                            .and_then(|versions_map| versions_map.get(&actual_version))
                            .cloned()
                            .map(|obj| (obj, actual_version))
                    });
                    (
                        res,
                        &inner.metrics.obj_checkpoint_hit,
                        &inner.metrics.obj_checkpoint_miss,
                    )
                }
            };
            if object_and_version.is_some() {
                hit_ctr.fetch_add(1, Ordering::Relaxed);
            } else {
                miss_ctr.fetch_add(1, Ordering::Relaxed);
            }
            results.push(object_and_version);
        }

        Ok(results)
    }
}

impl TransactionStoreWriter for InMemoryStore {
    fn write_transaction(
        &self,
        tx_digest: &str,
        transaction_info: TransactionInfo,
    ) -> Result<(), Error> {
        self.0
            .write()
            .unwrap()
            .transaction_cache
            .insert(tx_digest.to_string(), transaction_info);
        Ok(())
    }
}

impl EpochStoreWriter for InMemoryStore {
    fn write_epoch_info(&self, epoch: u64, epoch_data: EpochData) -> Result<(), Error> {
        self.0
            .write()
            .unwrap()
            .epoch_data_cache
            .insert(epoch, epoch_data);
        Ok(())
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

        // Always store the object at its actual version
        inner
            .object_cache
            .entry(key.object_id)
            .or_default()
            .insert(actual_version, object);

        // Handle version mappings based on query type
        match &key.version_query {
            VersionQuery::Version(_) => {
                // No additional mapping needed for direct version queries
            }
            VersionQuery::RootVersion(max_version) => {
                inner
                    .root_version_cache
                    .insert((key.object_id, *max_version), actual_version);
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

impl SetupStore for InMemoryStore {
    fn setup(&self, _chain_id: Option<String>) -> Result<Option<String>, Error> {
        Ok(None)
    }
}

impl StoreSummary for InMemoryStore {
    fn summary<W: std::io::Write>(&self, w: &mut W) -> Result<()> {
        let stats = self.cache_stats();
        let inner = self.0.read().unwrap();
        let m = &inner.metrics;
        let txn_hit = m.txn_hit.load(Ordering::Relaxed);
        let txn_miss = m.txn_miss.load(Ordering::Relaxed);
        let txn_err = m.txn_error.load(Ordering::Relaxed);
        let epoch_hit = m.epoch_hit.load(Ordering::Relaxed);
        let epoch_miss = m.epoch_miss.load(Ordering::Relaxed);
        let epoch_err = m.epoch_error.load(Ordering::Relaxed);
        let proto_hit = m.proto_hit.load(Ordering::Relaxed);
        let proto_miss = m.proto_miss.load(Ordering::Relaxed);
        let proto_err = m.proto_error.load(Ordering::Relaxed);
        let obj_v_hit = m.obj_version_hit.load(Ordering::Relaxed);
        let obj_v_miss = m.obj_version_miss.load(Ordering::Relaxed);
        let obj_v_err = m.obj_version_error.load(Ordering::Relaxed);
        let obj_r_hit = m.obj_root_hit.load(Ordering::Relaxed);
        let obj_r_miss = m.obj_root_miss.load(Ordering::Relaxed);
        let obj_r_err = m.obj_root_error.load(Ordering::Relaxed);
        let obj_c_hit = m.obj_checkpoint_hit.load(Ordering::Relaxed);
        let obj_c_miss = m.obj_checkpoint_miss.load(Ordering::Relaxed);
        let obj_c_err = m.obj_checkpoint_error.load(Ordering::Relaxed);
        let obj_total_hit = obj_v_hit + obj_r_hit + obj_c_hit;
        let obj_total_miss = obj_v_miss + obj_r_miss + obj_c_miss;
        let obj_total_err = obj_v_err + obj_r_err + obj_c_err;
        let total_hit = txn_hit + epoch_hit + proto_hit + obj_total_hit;
        let total_miss = txn_miss + epoch_miss + proto_miss + obj_total_miss;
        let total_err = txn_err + epoch_err + proto_err + obj_total_err;

        writeln!(w, "InMemoryStore summary")?;
        writeln!(w, "  Node: {:?}", self.node())?;
        writeln!(w, "  Cache sizes:")?;
        writeln!(
            w,
            "    Transactions: {} entries",
            stats.transaction_cache_size
        )?;
        writeln!(w, "    Epochs: {} entries", stats.epoch_data_cache_size)?;
        writeln!(w, "    Objects: {} versions", stats.object_cache_size)?;
        writeln!(w, "    Root map: {} entries", stats.root_version_cache_size)?;
        writeln!(
            w,
            "    Checkpoint map: {} entries",
            stats.checkpoint_cache_size
        )?;
        writeln!(
            w,
            "  Overall:    hit={} miss={} error={}",
            total_hit, total_miss, total_err
        )?;
        writeln!(w, "  Hits/Misses:")?;
        writeln!(
            w,
            "    Transaction: hit={} miss={} error={}",
            txn_hit, txn_miss, txn_err
        )?;
        writeln!(
            w,
            "    Epoch:       hit={} miss={} error={}",
            epoch_hit, epoch_miss, epoch_err
        )?;
        writeln!(
            w,
            "    Protocol:    hit={} miss={} error={}",
            proto_hit, proto_miss, proto_err
        )?;
        writeln!(
            w,
            "    Objects(all): hit={} miss={} error={}",
            obj_total_hit, obj_total_miss, obj_total_err
        )?;
        writeln!(
            w,
            "      Version:     hit={} miss={} error={}",
            obj_v_hit, obj_v_miss, obj_v_err
        )?;
        writeln!(
            w,
            "      RootVersion: hit={} miss={} error={}",
            obj_r_hit, obj_r_miss, obj_r_err
        )?;
        writeln!(
            w,
            "      Checkpoint:  hit={} miss={} error={}",
            obj_c_hit, obj_c_miss, obj_c_err
        )?;
        Ok(())
    }
}
