// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! LRU in-memory implementation of the replay interfaces: `TransactionStore`, `EpochStore`, and
//! `ObjectStore`.
//!
//! Very similar to `InMemoryStore`, but entries are kept in bounded `LruCache`s.
//! This is useful to avoid unbounded memory growth when replaying many transactions.

use crate::{
    replay_interface::{
        EpochData, EpochStore, EpochStoreWriter, ObjectKey, ObjectStore, ObjectStoreWriter,
        StoreSummary, TransactionInfo, TransactionStore, TransactionStoreWriter, VersionQuery,
    },
    Node,
};
use lru::LruCache;
use std::{
    num::NonZeroUsize,
    sync::{
        atomic::{AtomicU64, Ordering},
        RwLock,
    },
};
use sui_types::{
    base_types::ObjectID,
    committee::ProtocolVersion,
    object::Object,
    supported_protocol_versions::{Chain, ProtocolConfig},
};

/// Default cache capacities (can be tuned later or exposed via config)
const DEFAULT_TXN_CAP: usize = 50_000;
const DEFAULT_EPOCH_CAP: usize = 10_000;
const DEFAULT_OBJECT_CAP: usize = 200_000;
const DEFAULT_ROOT_MAP_CAP: usize = 200_000;
const DEFAULT_CHECKPOINT_MAP_CAP: usize = 200_000;

/// LRU-backed store implementing the replay interfaces
pub struct LruMemoryStore {
    node: Node,
    transaction_cache: RwLock<LruCache<String, TransactionInfo>>,
    epoch_data_cache: RwLock<LruCache<u64, EpochData>>,
    /// Objects are cached by (ObjectID, actual_version)
    object_cache: RwLock<LruCache<(ObjectID, u64), Object>>,
    /// Cache mapping (ObjectID, root_version) -> actual_version
    root_version_cache: RwLock<LruCache<(ObjectID, u64), u64>>,
    /// Cache mapping (ObjectID, checkpoint) -> actual_version
    checkpoint_cache: RwLock<LruCache<(ObjectID, u64), u64>>,
    metrics: LruStoreMetrics,
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
        Self {
            node,
            transaction_cache: RwLock::new(LruCache::new(nz(txn_cap))),
            epoch_data_cache: RwLock::new(LruCache::new(nz(epoch_cap))),
            object_cache: RwLock::new(LruCache::new(nz(object_cap))),
            root_version_cache: RwLock::new(LruCache::new(nz(root_map_cap))),
            checkpoint_cache: RwLock::new(LruCache::new(nz(checkpoint_map_cap))),
            metrics: LruStoreMetrics::default(),
        }
    }

    pub fn chain(&self) -> Chain {
        self.node.chain()
    }

    pub fn node(&self) -> &Node {
        &self.node
    }
}

impl TransactionStore for LruMemoryStore {
    fn transaction_data_and_effects(
        &self,
        tx_digest: &str,
    ) -> Result<Option<TransactionInfo>, anyhow::Error> {
        let cached = self
            .transaction_cache
            .write()
            .unwrap()
            .get(tx_digest)
            .cloned();
        if cached.is_some() {
            self.metrics.txn_hit.fetch_add(1, Ordering::Relaxed);
        } else {
            self.metrics.txn_miss.fetch_add(1, Ordering::Relaxed);
        }
        Ok(cached)
    }
}

impl EpochStore for LruMemoryStore {
    fn epoch_info(&self, epoch: u64) -> Result<Option<EpochData>, anyhow::Error> {
        let cached = self.epoch_data_cache.write().unwrap().get(&epoch).cloned();
        if cached.is_some() {
            self.metrics.epoch_hit.fetch_add(1, Ordering::Relaxed);
        } else {
            self.metrics.epoch_miss.fetch_add(1, Ordering::Relaxed);
        }
        Ok(cached)
    }

    fn protocol_config(&self, epoch: u64) -> Result<Option<ProtocolConfig>, anyhow::Error> {
        match self.epoch_info(epoch) {
            Ok(Some(epoch_data)) => {
                self.metrics.proto_hit.fetch_add(1, Ordering::Relaxed);
                Ok(Some(ProtocolConfig::get_for_version(
                    ProtocolVersion::new(epoch_data.protocol_version),
                    self.chain(),
                )))
            }
            Ok(None) => {
                self.metrics.proto_miss.fetch_add(1, Ordering::Relaxed);
                Ok(None)
            }
            Err(e) => {
                self.metrics.proto_error.fetch_add(1, Ordering::Relaxed);
                Err(e)
            }
        }
    }
}

impl ObjectStore for LruMemoryStore {
    fn get_objects(&self, keys: &[ObjectKey]) -> Result<Vec<Option<(Object, u64)>>, anyhow::Error> {
        let mut results = Vec::with_capacity(keys.len());
        for key in keys.iter() {
            let (object_and_version, hit_ctr, miss_ctr) = match &key.version_query {
                VersionQuery::Version(version) => {
                    let res = self
                        .object_cache
                        .write()
                        .unwrap()
                        .get(&(key.object_id, *version))
                        .cloned()
                        .map(|obj| (obj, *version));
                    (
                        res,
                        &self.metrics.obj_version_hit,
                        &self.metrics.obj_version_miss,
                    )
                }
                VersionQuery::RootVersion(max_version) => {
                    let actual_version = self
                        .root_version_cache
                        .write()
                        .unwrap()
                        .get(&(key.object_id, *max_version))
                        .copied();
                    let res = actual_version.and_then(|v| {
                        self.object_cache
                            .write()
                            .unwrap()
                            .get(&(key.object_id, v))
                            .cloned()
                            .map(|obj| (obj, v))
                    });
                    (res, &self.metrics.obj_root_hit, &self.metrics.obj_root_miss)
                }
                VersionQuery::AtCheckpoint(checkpoint) => {
                    let actual_version = self
                        .checkpoint_cache
                        .write()
                        .unwrap()
                        .get(&(key.object_id, *checkpoint))
                        .copied();
                    let res = actual_version.and_then(|v| {
                        self.object_cache
                            .write()
                            .unwrap()
                            .get(&(key.object_id, v))
                            .cloned()
                            .map(|obj| (obj, v))
                    });
                    (
                        res,
                        &self.metrics.obj_checkpoint_hit,
                        &self.metrics.obj_checkpoint_miss,
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
    ) -> Result<(), anyhow::Error> {
        self.transaction_cache
            .write()
            .unwrap()
            .put(tx_digest.to_string(), transaction_info);
        Ok(())
    }
}

impl EpochStoreWriter for LruMemoryStore {
    fn write_epoch_info(&self, epoch: u64, epoch_data: EpochData) -> Result<(), anyhow::Error> {
        self.epoch_data_cache
            .write()
            .unwrap()
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
    ) -> Result<(), anyhow::Error> {
        // Always store the object at its actual version
        self.object_cache
            .write()
            .unwrap()
            .put((key.object_id, actual_version), object);

        // Handle version mappings based on query type
        match &key.version_query {
            VersionQuery::Version(_) => {}
            VersionQuery::RootVersion(max_version) => {
                self.root_version_cache
                    .write()
                    .unwrap()
                    .put((key.object_id, *max_version), actual_version);
            }
            VersionQuery::AtCheckpoint(checkpoint) => {
                self.checkpoint_cache
                    .write()
                    .unwrap()
                    .put((key.object_id, *checkpoint), actual_version);
            }
        }

        Ok(())
    }
}

impl StoreSummary for LruMemoryStore {
    fn summary<W: std::io::Write>(&self, w: &mut W) -> anyhow::Result<()> {
        let m = &self.metrics;
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
