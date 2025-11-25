// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! LRU in-memory implementation of the data store interfaces: `TransactionStore`, `EpochStore`,
//! and `ObjectStore`.
//!
//! Very similar to `InMemoryStore`, but entries are kept in bounded `LruCache`s.
//! This is useful to avoid unbounded memory growth under heavy load.

use crate::{
    EpochData, EpochStore, EpochStoreWriter, ObjectKey, ObjectStore, ObjectStoreWriter, SetupStore,
    StoreSummary, TransactionInfo, TransactionStore, TransactionStoreWriter, VersionQuery,
    node::Node,
};
use anyhow::{Error, Result};
use lru::LruCache;
use std::{
    num::NonZeroUsize,
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

/// Default cache capacities
const DEFAULT_TXN_CAP: usize = 10_000;
const DEFAULT_EPOCH_CAP: usize = 10_000;
const DEFAULT_OBJECT_CAP: usize = 50_000;
const DEFAULT_ROOT_MAP_CAP: usize = 50_000;
const DEFAULT_CHECKPOINT_MAP_CAP: usize = 50_000;

/// LRU-backed store implementing the data store interfaces.
struct LruMemoryStoreInner {
    node: Node,
    transaction_cache: LruCache<String, TransactionInfo>,
    epoch_data_cache: LruCache<u64, EpochData>,
    /// Objects are cached by (ObjectID, actual_version)
    object_cache: LruCache<(ObjectID, u64), Object>,
    // The next 2 maps can be organized as flat maps or nested maps
    // and the best solution depends on usage patterns.
    // For now, we use flat maps but we may want to review in the future
    /// Cache mapping (ObjectID, root_version) -> actual_version
    root_version_cache: LruCache<(ObjectID, u64), u64>,
    /// Cache mapping (ObjectID, checkpoint) -> actual_version
    checkpoint_cache: LruCache<(ObjectID, u64), u64>,
    metrics: LruStoreMetrics,
}

// The RwLock is needed for 2 reasons:
// 1. to make tokio happy
// 2. to allow interior mutability for the cache
pub struct LruMemoryStore(RwLock<LruMemoryStoreInner>);

#[derive(Default)]
struct LruStoreMetrics {
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

impl LruMemoryStore {
    /// Create a new LRU store with default capacities
    pub fn new(node: Node) -> Self {
        Self::with_capacities(
            node,
            DEFAULT_TXN_CAP,
            DEFAULT_EPOCH_CAP,
            DEFAULT_OBJECT_CAP,
            DEFAULT_ROOT_MAP_CAP,
            DEFAULT_CHECKPOINT_MAP_CAP,
        )
    }

    /// Create a new LRU store with custom capacities
    pub fn with_capacities(
        node: Node,
        txn_cap: usize,
        epoch_cap: usize,
        object_cap: usize,
        root_map_cap: usize,
        checkpoint_map_cap: usize,
    ) -> Self {
        let nz = |n: usize| NonZeroUsize::new(n.max(1)).unwrap();
        Self(RwLock::new(LruMemoryStoreInner {
            node,
            transaction_cache: LruCache::new(nz(txn_cap)),
            epoch_data_cache: LruCache::new(nz(epoch_cap)),
            object_cache: LruCache::new(nz(object_cap)),
            root_version_cache: LruCache::new(nz(root_map_cap)),
            checkpoint_cache: LruCache::new(nz(checkpoint_map_cap)),
            metrics: LruStoreMetrics::default(),
        }))
    }

    pub fn chain(&self) -> Chain {
        self.0.read().unwrap().node.chain()
    }

    pub fn node(&self) -> Node {
        self.0.read().unwrap().node.clone()
    }
}

impl TransactionStore for LruMemoryStore {
    fn transaction_data_and_effects(
        &self,
        tx_digest: &str,
    ) -> Result<Option<TransactionInfo>, Error> {
        let mut inner = self.0.write().unwrap();
        let cached = inner.transaction_cache.get(tx_digest).cloned();
        if cached.is_some() {
            inner.metrics.txn_hit.fetch_add(1, Ordering::Relaxed);
        } else {
            inner.metrics.txn_miss.fetch_add(1, Ordering::Relaxed);
        }
        Ok(cached)
    }
}

impl EpochStore for LruMemoryStore {
    fn epoch_info(&self, epoch: u64) -> Result<Option<EpochData>, Error> {
        let mut inner = self.0.write().unwrap();
        let cached = inner.epoch_data_cache.get(&epoch).cloned();
        if cached.is_some() {
            inner.metrics.epoch_hit.fetch_add(1, Ordering::Relaxed);
        } else {
            inner.metrics.epoch_miss.fetch_add(1, Ordering::Relaxed);
        }
        Ok(cached)
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

impl ObjectStore for LruMemoryStore {
    fn get_objects(&self, keys: &[ObjectKey]) -> Result<Vec<Option<(Object, u64)>>, Error> {
        let mut results = Vec::with_capacity(keys.len());
        let mut inner = self.0.write().unwrap();

        for key in keys.iter() {
            let (object_and_version, hit_ctr, miss_ctr) = match &key.version_query {
                VersionQuery::Version(version) => {
                    let res = inner
                        .object_cache
                        .get(&(key.object_id, *version))
                        .cloned()
                        .map(|obj| (obj, *version));
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
                    let res = actual_version.and_then(|v| {
                        inner
                            .object_cache
                            .get(&(key.object_id, v))
                            .cloned()
                            .map(|obj| (obj, v))
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
                    let res = actual_version.and_then(|v| {
                        inner
                            .object_cache
                            .get(&(key.object_id, v))
                            .cloned()
                            .map(|obj| (obj, v))
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

impl TransactionStoreWriter for LruMemoryStore {
    fn write_transaction(
        &self,
        tx_digest: &str,
        transaction_info: TransactionInfo,
    ) -> Result<(), Error> {
        self.0
            .write()
            .unwrap()
            .transaction_cache
            .put(tx_digest.to_string(), transaction_info);
        Ok(())
    }
}

impl EpochStoreWriter for LruMemoryStore {
    fn write_epoch_info(&self, epoch: u64, epoch_data: EpochData) -> Result<(), Error> {
        self.0
            .write()
            .unwrap()
            .epoch_data_cache
            .put(epoch, epoch_data);
        Ok(())
    }
}

impl ObjectStoreWriter for LruMemoryStore {
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
            .put((key.object_id, actual_version), object);

        // Handle version mappings based on query type
        match &key.version_query {
            VersionQuery::Version(_) => {}
            VersionQuery::RootVersion(max_version) => {
                inner
                    .root_version_cache
                    .put((key.object_id, *max_version), actual_version);
            }
            VersionQuery::AtCheckpoint(checkpoint) => {
                inner
                    .checkpoint_cache
                    .put((key.object_id, *checkpoint), actual_version);
            }
        }

        Ok(())
    }
}

impl SetupStore for LruMemoryStore {
    fn setup(&self, _chain_id: Option<String>) -> Result<Option<String>, Error> {
        Ok(None)
    }
}

impl StoreSummary for LruMemoryStore {
    fn summary<W: std::io::Write>(&self, w: &mut W) -> Result<()> {
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

        writeln!(w, "LruMemoryStore summary")?;
        writeln!(w, "  Node: {:?}", self.node())?;
        writeln!(
            w,
            "  Overall:    hit={} miss={} error={}",
            total_hit, total_miss, total_err
        )?;
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
