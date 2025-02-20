// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! MemoryCache is a cache for the transaction execution which delays writes to the database until
//! transaction results are certified (i.e. they appear in a certified checkpoint, or an effects cert
//! is observed by a fullnode). The cache also stores committed data in memory in order to serve
//! future reads without hitting the database.
//!
//! For storing uncommitted transaction outputs, we cannot evict the data at all until it is written
//! to disk. Committed data not only can be evicted, but it is also unbounded (imagine a stream of
//! transactions that keep splitting a coin into smaller coins).
//!
//! We also want to be able to support negative cache hits (i.e. the case where we can determine an
//! object does not exist without hitting the database).
//!
//! To achieve both of these goals, we split the cache data into two pieces, a dirty set and a cached
//! set. The dirty set has no automatic evictions, data is only removed after being committed. The
//! cached set is in a bounded-sized cache with automatic evictions. In order to support negative
//! cache hits, we treat the two halves of the cache as FIFO queue. Newly written (dirty) versions are
//! inserted to one end of the dirty queue. As versions are committed to disk, they are
//! removed from the other end of the dirty queue and inserted into the cache queue. The cache queue
//! is truncated if it exceeds its maximum size, by removing all but the N newest versions.
//!
//! This gives us the property that the sequence of versions in the dirty and cached queues are the
//! most recent versions of the object, i.e. there can be no "gaps". This allows for the following:
//!
//!   - Negative cache hits: If the queried version is not in memory, but is higher than the smallest
//!     version in the cached queue, it does not exist in the db either.
//!   - Bounded reads: When reading the most recent version that is <= some version bound, we can
//!     correctly satisfy this query from the cache, or determine that we must go to the db.
//!
//! Note that at any time, either or both the dirty or the cached queue may be non-existent. There may be no
//! dirty versions of the objects, in which case there will be no dirty queue. And, the cached queue
//! may be evicted from the cache, in which case there will be no cached queue. Because only the cached
//! queue can be evicted (the dirty queue can only become empty by moving versions from it to the cached
//! queue), the "highest versions" property still holds in all cases.
//!
//! The above design is used for both objects and markers.

use crate::authority::authority_per_epoch_store::AuthorityPerEpochStore;
use crate::authority::authority_store::{
    ExecutionLockWriteGuard, LockDetailsDeprecated, ObjectLockStatus, SuiLockResult,
};
use crate::authority::authority_store_tables::LiveObject;
use crate::authority::backpressure::BackpressureManager;
use crate::authority::epoch_start_configuration::{EpochFlag, EpochStartConfiguration};
use crate::authority::AuthorityStore;
use crate::fallback_fetch::{do_fallback_lookup, do_fallback_lookup_fallible};
use crate::state_accumulator::AccumulatorStore;
use crate::transaction_outputs::TransactionOutputs;

use dashmap::mapref::entry::Entry as DashMapEntry;
use dashmap::DashMap;
use futures::{future::BoxFuture, FutureExt};
use moka::sync::Cache as MokaCache;
use mysten_common::sync::notify_read::NotifyRead;
use parking_lot::Mutex;
use prometheus::Registry;
use std::collections::{BTreeMap, BTreeSet};
use std::hash::Hash;
use std::sync::atomic::AtomicU64;
use std::sync::Arc;
use sui_config::ExecutionCacheConfig;
use sui_macros::fail_point;
use sui_protocol_config::ProtocolVersion;
use sui_types::accumulator::Accumulator;
use sui_types::base_types::{
    EpochId, FullObjectID, ObjectID, ObjectRef, SequenceNumber, VerifiedExecutionData,
};
use sui_types::bridge::{get_bridge, Bridge};
use sui_types::digests::{
    ObjectDigest, TransactionDigest, TransactionEffectsDigest, TransactionEventsDigest,
};
use sui_types::effects::{TransactionEffects, TransactionEvents};
use sui_types::error::{SuiError, SuiResult, UserInputError};
use sui_types::executable_transaction::VerifiedExecutableTransaction;
use sui_types::message_envelope::Message;
use sui_types::messages_checkpoint::CheckpointSequenceNumber;
use sui_types::object::Object;
use sui_types::storage::{
    FullObjectKey, MarkerValue, ObjectKey, ObjectOrTombstone, ObjectStore, PackageObject,
};
use sui_types::sui_system_state::{get_sui_system_state, SuiSystemState};
use sui_types::transaction::{VerifiedSignedTransaction, VerifiedTransaction};
use tap::TapOptional;
use tracing::{debug, info, instrument, trace, warn};

use super::cache_types::Ticket;
use super::ExecutionCacheAPI;
use super::{
    cache_types::{CacheResult, CachedVersionMap, IsNewer, MonotonicCache},
    implement_passthrough_traits,
    object_locks::ObjectLocks,
    CheckpointCache, ExecutionCacheCommit, ExecutionCacheMetrics, ExecutionCacheReconfigAPI,
    ExecutionCacheWrite, ObjectCacheRead, StateSyncAPI, TestingAPI, TransactionCacheRead,
};

#[cfg(test)]
#[path = "unit_tests/writeback_cache_tests.rs"]
pub mod writeback_cache_tests;

#[derive(Clone, PartialEq, Eq)]
enum ObjectEntry {
    Object(Object),
    Deleted,
    Wrapped,
}

impl ObjectEntry {
    #[cfg(test)]
    fn unwrap_object(&self) -> &Object {
        match self {
            ObjectEntry::Object(o) => o,
            _ => panic!("unwrap_object called on non-Object"),
        }
    }

    fn is_tombstone(&self) -> bool {
        match self {
            ObjectEntry::Deleted | ObjectEntry::Wrapped => true,
            ObjectEntry::Object(_) => false,
        }
    }
}

impl std::fmt::Debug for ObjectEntry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ObjectEntry::Object(o) => {
                write!(f, "ObjectEntry::Object({:?})", o.compute_object_reference())
            }
            ObjectEntry::Deleted => write!(f, "ObjectEntry::Deleted"),
            ObjectEntry::Wrapped => write!(f, "ObjectEntry::Wrapped"),
        }
    }
}

impl From<Object> for ObjectEntry {
    fn from(object: Object) -> Self {
        ObjectEntry::Object(object)
    }
}

impl From<ObjectOrTombstone> for ObjectEntry {
    fn from(object: ObjectOrTombstone) -> Self {
        match object {
            ObjectOrTombstone::Object(o) => o.into(),
            ObjectOrTombstone::Tombstone(obj_ref) => {
                if obj_ref.2.is_deleted() {
                    ObjectEntry::Deleted
                } else if obj_ref.2.is_wrapped() {
                    ObjectEntry::Wrapped
                } else {
                    panic!("tombstone digest must either be deleted or wrapped");
                }
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum LatestObjectCacheEntry {
    Object(SequenceNumber, ObjectEntry),
    NonExistent,
}

impl LatestObjectCacheEntry {
    #[cfg(test)]
    fn version(&self) -> Option<SequenceNumber> {
        match self {
            LatestObjectCacheEntry::Object(version, _) => Some(*version),
            LatestObjectCacheEntry::NonExistent => None,
        }
    }
}

impl IsNewer for LatestObjectCacheEntry {
    fn is_newer_than(&self, other: &LatestObjectCacheEntry) -> bool {
        match (self, other) {
            (LatestObjectCacheEntry::Object(v1, _), LatestObjectCacheEntry::Object(v2, _)) => {
                v1 > v2
            }
            (LatestObjectCacheEntry::Object(_, _), LatestObjectCacheEntry::NonExistent) => true,
            _ => false,
        }
    }
}

type MarkerKey = (EpochId, FullObjectID);

/// UncommitedData stores execution outputs that are not yet written to the db. Entries in this
/// struct can only be purged after they are committed.
struct UncommittedData {
    /// The object dirty set. All writes go into this table first. After we flush the data to the
    /// db, the data is removed from this table and inserted into the object_cache.
    ///
    /// This table may contain both live and dead objects, since we flush both live and dead
    /// objects to the db in order to support past object queries on fullnodes.
    ///
    /// Further, we only remove objects in FIFO order, which ensures that the cached
    /// sequence of objects has no gaps. In other words, if we have versions 4, 8, 13 of
    /// an object, we can deduce that version 9 does not exist. This also makes child object
    /// reads efficient. `object_cache` cannot contain a more recent version of an object than
    /// `objects`, and neither can have any gaps. Therefore if there is any object <= the version
    /// bound for a child read in objects, it is the correct object to return.
    objects: DashMap<ObjectID, CachedVersionMap<ObjectEntry>>,

    // Markers for received objects and deleted shared objects. This contains all of the dirty
    // marker state, which is committed to the db at the same time as other transaction data.
    // After markers are committed to the db we remove them from this table and insert them into
    // marker_cache.
    markers: DashMap<MarkerKey, CachedVersionMap<MarkerValue>>,

    transaction_effects: DashMap<TransactionEffectsDigest, TransactionEffects>,

    // Because TransactionEvents are not unique to the transaction that created them, we must
    // reference count them in order to know when we can remove them from the cache. For now
    // we track all referers explicitly, but we can use a ref count when we are confident in
    // the correctness of the code.
    transaction_events:
        DashMap<TransactionEventsDigest, (BTreeSet<TransactionDigest>, TransactionEvents)>,

    executed_effects_digests: DashMap<TransactionDigest, TransactionEffectsDigest>,

    // Transaction outputs that have not yet been written to the DB. Items are removed from this
    // table as they are flushed to the db.
    pending_transaction_writes: DashMap<TransactionDigest, Arc<TransactionOutputs>>,

    total_transaction_inserts: AtomicU64,
    total_transaction_commits: AtomicU64,
}

impl UncommittedData {
    fn new() -> Self {
        Self {
            objects: DashMap::new(),
            markers: DashMap::new(),
            transaction_effects: DashMap::new(),
            executed_effects_digests: DashMap::new(),
            pending_transaction_writes: DashMap::new(),
            transaction_events: DashMap::new(),
            total_transaction_inserts: AtomicU64::new(0),
            total_transaction_commits: AtomicU64::new(0),
        }
    }

    fn clear(&self) {
        self.objects.clear();
        self.markers.clear();
        self.transaction_effects.clear();
        self.executed_effects_digests.clear();
        self.pending_transaction_writes.clear();
        self.transaction_events.clear();
        self.total_transaction_inserts
            .store(0, std::sync::atomic::Ordering::Relaxed);
        self.total_transaction_commits
            .store(0, std::sync::atomic::Ordering::Relaxed);
    }

    fn is_empty(&self) -> bool {
        let empty = self.pending_transaction_writes.is_empty();
        if empty && cfg!(debug_assertions) {
            assert!(
                self.objects.is_empty()
                    && self.markers.is_empty()
                    && self.transaction_effects.is_empty()
                    && self.executed_effects_digests.is_empty()
                    && self.transaction_events.is_empty()
                    && self
                        .total_transaction_inserts
                        .load(std::sync::atomic::Ordering::Relaxed)
                        == self
                            .total_transaction_commits
                            .load(std::sync::atomic::Ordering::Relaxed),
            );
        }
        empty
    }
}

// Point items (anything without a version number) can be negatively cached as None
type PointCacheItem<T> = Option<T>;

// PointCacheItem can only be used for insert-only collections, so a Some entry
// is always newer than a None entry.
impl<T: Eq + std::fmt::Debug> IsNewer for PointCacheItem<T> {
    fn is_newer_than(&self, other: &PointCacheItem<T>) -> bool {
        match (self, other) {
            (Some(_), None) => true,

            (Some(a), Some(b)) => {
                // conflicting inserts should never happen
                debug_assert_eq!(a, b);
                false
            }

            _ => false,
        }
    }
}

/// CachedData stores data that has been committed to the db, but is likely to be read soon.
struct CachedCommittedData {
    // See module level comment for an explanation of caching strategy.
    object_cache: MokaCache<ObjectID, Arc<Mutex<CachedVersionMap<ObjectEntry>>>>,

    // We separately cache the latest version of each object. Although this seems
    // redundant, it is the only way to support populating the cache after a read.
    // We cannot simply insert objects that we read off the disk into `object_cache`,
    // since that may violate the no-missing-versions property.
    // `object_by_id_cache` is also written to on writes so that it is always coherent.
    object_by_id_cache: MonotonicCache<ObjectID, LatestObjectCacheEntry>,

    // See module level comment for an explanation of caching strategy.
    marker_cache: MokaCache<MarkerKey, Arc<Mutex<CachedVersionMap<MarkerValue>>>>,

    transactions: MonotonicCache<TransactionDigest, PointCacheItem<Arc<VerifiedTransaction>>>,

    transaction_effects:
        MonotonicCache<TransactionEffectsDigest, PointCacheItem<Arc<TransactionEffects>>>,

    transaction_events:
        MonotonicCache<TransactionEventsDigest, PointCacheItem<Arc<TransactionEvents>>>,

    executed_effects_digests:
        MonotonicCache<TransactionDigest, PointCacheItem<TransactionEffectsDigest>>,

    // Objects that were read at transaction signing time - allows us to access them again at
    // execution time with a single lock / hash lookup
    _transaction_objects: MokaCache<TransactionDigest, Vec<Object>>,
}

impl CachedCommittedData {
    fn new(config: &ExecutionCacheConfig) -> Self {
        let object_cache = MokaCache::builder()
            .max_capacity(config.object_cache_size())
            .build();
        let marker_cache = MokaCache::builder()
            .max_capacity(config.marker_cache_size())
            .build();

        let transactions = MonotonicCache::new(config.transaction_cache_size());
        let transaction_effects = MonotonicCache::new(config.effect_cache_size());
        let transaction_events = MonotonicCache::new(config.events_cache_size());
        let executed_effects_digests = MonotonicCache::new(config.executed_effect_cache_size());

        let transaction_objects = MokaCache::builder()
            .max_capacity(config.transaction_objects_cache_size())
            .build();

        Self {
            object_cache,
            object_by_id_cache: MonotonicCache::new(config.object_by_id_cache_size()),
            marker_cache,
            transactions,
            transaction_effects,
            transaction_events,
            executed_effects_digests,
            _transaction_objects: transaction_objects,
        }
    }

    fn clear_and_assert_empty(&self) {
        self.object_cache.invalidate_all();
        self.object_by_id_cache.invalidate_all();
        self.marker_cache.invalidate_all();
        self.transactions.invalidate_all();
        self.transaction_effects.invalidate_all();
        self.transaction_events.invalidate_all();
        self.executed_effects_digests.invalidate_all();
        self._transaction_objects.invalidate_all();

        assert_empty(&self.object_cache);
        assert!(&self.object_by_id_cache.is_empty());
        assert_empty(&self.marker_cache);
        assert!(self.transactions.is_empty());
        assert!(self.transaction_effects.is_empty());
        assert!(self.transaction_events.is_empty());
        assert!(self.executed_effects_digests.is_empty());
        assert_empty(&self._transaction_objects);
    }
}

fn assert_empty<K, V>(cache: &MokaCache<K, V>)
where
    K: std::hash::Hash + std::cmp::Eq + std::cmp::PartialEq + Send + Sync + 'static,
    V: std::clone::Clone + std::marker::Send + std::marker::Sync + 'static,
{
    if cache.iter().next().is_some() {
        panic!("cache should be empty");
    }
}

pub struct WritebackCache {
    dirty: UncommittedData,
    cached: CachedCommittedData,

    // The packages cache is treated separately from objects, because they are immutable and can be
    // used by any number of transactions. Additionally, many operations require loading large
    // numbers of packages (due to dependencies), so we want to try to keep all packages in memory.
    //
    // Also, this cache can contain packages that are dirty or committed, so it does not live in
    // UncachedData or CachedCommittedData. The cache is populated in two ways:
    // - when packages are written (in which case they will also be present in the dirty set)
    // - after a cache miss. Because package IDs are unique (only one version exists for each ID)
    //   we do not need to worry about the contiguous version property.
    // - note that we removed any unfinalized packages from the cache during revert_state_update().
    packages: MokaCache<ObjectID, PackageObject>,

    object_locks: ObjectLocks,

    executed_effects_digests_notify_read: NotifyRead<TransactionDigest, TransactionEffectsDigest>,
    store: Arc<AuthorityStore>,
    backpressure_threshold: u64,
    backpressure_manager: Arc<BackpressureManager>,
    metrics: Arc<ExecutionCacheMetrics>,
}

macro_rules! check_cache_entry_by_version {
    ($self: ident, $table: expr, $level: expr, $cache: expr, $version: expr) => {
        $self.metrics.record_cache_request($table, $level);
        if let Some(cache) = $cache {
            if let Some(entry) = cache.get(&$version) {
                $self.metrics.record_cache_hit($table, $level);
                return CacheResult::Hit(entry.clone());
            }

            if let Some(least_version) = cache.get_least() {
                if least_version.0 < $version {
                    // If the version is greater than the least version in the cache, then we know
                    // that the object does not exist anywhere
                    $self.metrics.record_cache_negative_hit($table, $level);
                    return CacheResult::NegativeHit;
                }
            }
        }
        $self.metrics.record_cache_miss($table, $level);
    };
}

macro_rules! check_cache_entry_by_latest {
    ($self: ident, $table: expr, $level: expr, $cache: expr) => {
        $self.metrics.record_cache_request($table, $level);
        if let Some(cache) = $cache {
            if let Some((version, entry)) = cache.get_highest() {
                $self.metrics.record_cache_hit($table, $level);
                return CacheResult::Hit((*version, entry.clone()));
            } else {
                panic!("empty CachedVersionMap should have been removed");
            }
        }
        $self.metrics.record_cache_miss($table, $level);
    };
}

impl WritebackCache {
    pub fn new(
        config: &ExecutionCacheConfig,
        store: Arc<AuthorityStore>,
        metrics: Arc<ExecutionCacheMetrics>,
        backpressure_manager: Arc<BackpressureManager>,
    ) -> Self {
        let packages = MokaCache::builder()
            .max_capacity(config.package_cache_size())
            .build();
        Self {
            dirty: UncommittedData::new(),
            cached: CachedCommittedData::new(config),
            packages,
            object_locks: ObjectLocks::new(),
            executed_effects_digests_notify_read: NotifyRead::new(),
            store,
            backpressure_manager,
            backpressure_threshold: config.backpressure_threshold(),
            metrics,
        }
    }

    pub fn new_for_tests(store: Arc<AuthorityStore>, registry: &Registry) -> Self {
        Self::new(
            &Default::default(),
            store,
            ExecutionCacheMetrics::new(registry).into(),
            BackpressureManager::new_for_tests(),
        )
    }

    #[cfg(test)]
    pub fn reset_for_test(&mut self) {
        let mut new = Self::new(
            &Default::default(),
            self.store.clone(),
            self.metrics.clone(),
            self.backpressure_manager.clone(),
        );
        std::mem::swap(self, &mut new);
    }

    fn write_object_entry(
        &self,
        object_id: &ObjectID,
        version: SequenceNumber,
        object: ObjectEntry,
    ) {
        trace!(?object_id, ?version, ?object, "inserting object entry");
        self.metrics.record_cache_write("object");

        // We must hold the lock for the object entry while inserting to the
        // object_by_id_cache. Otherwise, a surprising bug can occur:
        //
        // 1. A thread executing TX1 can write object (O,1) to the dirty set and then pause.
        // 2. TX2, which reads (O,1) can begin executing, because TransactionManager immediately
        //    schedules transactions if their inputs are available. It does not matter that TX1
        //    hasn't finished executing yet.
        // 3. TX2 can write (O,2) to both the dirty set and the object_by_id_cache.
        // 4. The thread executing TX1 can resume and write (O,1) to the object_by_id_cache.
        //
        // Now, any subsequent attempt to get the latest version of O will return (O,1) instead of
        // (O,2).
        //
        // This seems very unlikely, but it may be possible under the following circumstances:
        // - While a thread is unlikely to pause for so long, moka cache uses optimistic
        //   lock-free algorithms that have retry loops. Possibly, under high contention, this
        //   code might spin for a surprisingly long time.
        // - Additionally, many concurrent re-executions of the same tx could happen due to
        //   the tx finalizer, plus checkpoint executor, consensus, and RPCs from fullnodes.
        let mut entry = self.dirty.objects.entry(*object_id).or_default();

        self.cached
            .object_by_id_cache
            .insert(
                object_id,
                LatestObjectCacheEntry::Object(version, object.clone()),
                Ticket::Write,
            )
            // While Ticket::Write cannot expire, this insert may still fail.
            // See the comment in `MonotonicCache::insert`.
            .ok();

        entry.insert(version, object);
    }

    fn write_marker_value(
        &self,
        epoch_id: EpochId,
        object_key: FullObjectKey,
        marker_value: MarkerValue,
    ) {
        tracing::trace!("inserting marker value {object_key:?}: {marker_value:?}",);
        self.metrics.record_cache_write("marker");
        self.dirty
            .markers
            .entry((epoch_id, object_key.id()))
            .or_default()
            .value_mut()
            .insert(object_key.version(), marker_value);
    }

    // lock both the dirty and committed sides of the cache, and then pass the entries to
    // the callback. Written with the `with` pattern because any other way of doing this
    // creates lifetime hell.
    fn with_locked_cache_entries<K, V, R>(
        dirty_map: &DashMap<K, CachedVersionMap<V>>,
        cached_map: &MokaCache<K, Arc<Mutex<CachedVersionMap<V>>>>,
        key: &K,
        cb: impl FnOnce(Option<&CachedVersionMap<V>>, Option<&CachedVersionMap<V>>) -> R,
    ) -> R
    where
        K: Copy + Eq + Hash + Send + Sync + 'static,
        V: Send + Sync + 'static,
    {
        let dirty_entry = dirty_map.entry(*key);
        let dirty_entry = match &dirty_entry {
            DashMapEntry::Occupied(occupied) => Some(occupied.get()),
            DashMapEntry::Vacant(_) => None,
        };

        let cached_entry = cached_map.get(key);
        let cached_lock = cached_entry.as_ref().map(|entry| entry.lock());
        let cached_entry = cached_lock.as_deref();

        cb(dirty_entry, cached_entry)
    }

    // Attempt to get an object from the cache. The DB is not consulted.
    // Can return Hit, Miss, or NegativeHit (if the object is known to not exist).
    fn get_object_entry_by_key_cache_only(
        &self,
        object_id: &ObjectID,
        version: SequenceNumber,
    ) -> CacheResult<ObjectEntry> {
        Self::with_locked_cache_entries(
            &self.dirty.objects,
            &self.cached.object_cache,
            object_id,
            |dirty_entry, cached_entry| {
                check_cache_entry_by_version!(
                    self,
                    "object_by_version",
                    "uncommitted",
                    dirty_entry,
                    version
                );
                check_cache_entry_by_version!(
                    self,
                    "object_by_version",
                    "committed",
                    cached_entry,
                    version
                );
                CacheResult::Miss
            },
        )
    }

    fn get_object_by_key_cache_only(
        &self,
        object_id: &ObjectID,
        version: SequenceNumber,
    ) -> CacheResult<Object> {
        match self.get_object_entry_by_key_cache_only(object_id, version) {
            CacheResult::Hit(entry) => match entry {
                ObjectEntry::Object(object) => CacheResult::Hit(object),
                ObjectEntry::Deleted | ObjectEntry::Wrapped => CacheResult::NegativeHit,
            },
            CacheResult::Miss => CacheResult::Miss,
            CacheResult::NegativeHit => CacheResult::NegativeHit,
        }
    }

    fn get_object_entry_by_id_cache_only(
        &self,
        request_type: &'static str,
        object_id: &ObjectID,
    ) -> CacheResult<(SequenceNumber, ObjectEntry)> {
        self.metrics
            .record_cache_request(request_type, "object_by_id");
        let entry = self.cached.object_by_id_cache.get(object_id);

        if cfg!(debug_assertions) {
            if let Some(entry) = &entry {
                // check that cache is coherent
                let highest: Option<ObjectEntry> = self
                    .dirty
                    .objects
                    .get(object_id)
                    .and_then(|entry| entry.get_highest().map(|(_, o)| o.clone()))
                    .or_else(|| {
                        let obj: Option<ObjectEntry> = self
                            .store
                            .get_latest_object_or_tombstone(*object_id)
                            .unwrap()
                            .map(|(_, o)| o.into());
                        obj
                    });

                let cache_entry = match &*entry.lock() {
                    LatestObjectCacheEntry::Object(_, entry) => Some(entry.clone()),
                    LatestObjectCacheEntry::NonExistent => None,
                };

                // If the cache entry is a tombstone, the db entry may be missing if it was pruned.
                let tombstone_possibly_pruned = highest.is_none()
                    && cache_entry
                        .as_ref()
                        .map(|e| e.is_tombstone())
                        .unwrap_or(false);

                if highest != cache_entry && !tombstone_possibly_pruned {
                    tracing::error!(
                        ?highest,
                        ?cache_entry,
                        ?tombstone_possibly_pruned,
                        "object_by_id cache is incoherent for {:?}",
                        object_id
                    );
                    panic!("object_by_id cache is incoherent for {:?}", object_id);
                }
            }
        }

        if let Some(entry) = entry {
            let entry = entry.lock();
            match &*entry {
                LatestObjectCacheEntry::Object(latest_version, latest_object) => {
                    self.metrics.record_cache_hit(request_type, "object_by_id");
                    return CacheResult::Hit((*latest_version, latest_object.clone()));
                }
                LatestObjectCacheEntry::NonExistent => {
                    self.metrics
                        .record_cache_negative_hit(request_type, "object_by_id");
                    return CacheResult::NegativeHit;
                }
            }
        } else {
            self.metrics.record_cache_miss(request_type, "object_by_id");
        }

        Self::with_locked_cache_entries(
            &self.dirty.objects,
            &self.cached.object_cache,
            object_id,
            |dirty_entry, cached_entry| {
                check_cache_entry_by_latest!(self, request_type, "uncommitted", dirty_entry);
                check_cache_entry_by_latest!(self, request_type, "committed", cached_entry);
                CacheResult::Miss
            },
        )
    }

    fn get_object_by_id_cache_only(
        &self,
        request_type: &'static str,
        object_id: &ObjectID,
    ) -> CacheResult<(SequenceNumber, Object)> {
        match self.get_object_entry_by_id_cache_only(request_type, object_id) {
            CacheResult::Hit((version, entry)) => match entry {
                ObjectEntry::Object(object) => CacheResult::Hit((version, object)),
                ObjectEntry::Deleted | ObjectEntry::Wrapped => CacheResult::NegativeHit,
            },
            CacheResult::NegativeHit => CacheResult::NegativeHit,
            CacheResult::Miss => CacheResult::Miss,
        }
    }

    fn get_marker_value_cache_only(
        &self,
        object_key: FullObjectKey,
        epoch_id: EpochId,
    ) -> CacheResult<MarkerValue> {
        Self::with_locked_cache_entries(
            &self.dirty.markers,
            &self.cached.marker_cache,
            &(epoch_id, object_key.id()),
            |dirty_entry, cached_entry| {
                check_cache_entry_by_version!(
                    self,
                    "marker_by_version",
                    "uncommitted",
                    dirty_entry,
                    object_key.version()
                );
                check_cache_entry_by_version!(
                    self,
                    "marker_by_version",
                    "committed",
                    cached_entry,
                    object_key.version()
                );
                CacheResult::Miss
            },
        )
    }

    fn get_latest_marker_value_cache_only(
        &self,
        object_id: FullObjectID,
        epoch_id: EpochId,
    ) -> CacheResult<(SequenceNumber, MarkerValue)> {
        Self::with_locked_cache_entries(
            &self.dirty.markers,
            &self.cached.marker_cache,
            &(epoch_id, object_id),
            |dirty_entry, cached_entry| {
                check_cache_entry_by_latest!(self, "marker_latest", "uncommitted", dirty_entry);
                check_cache_entry_by_latest!(self, "marker_latest", "committed", cached_entry);
                CacheResult::Miss
            },
        )
    }

    fn get_object_impl(&self, request_type: &'static str, id: &ObjectID) -> Option<Object> {
        let ticket = self.cached.object_by_id_cache.get_ticket_for_read(id);
        match self.get_object_by_id_cache_only(request_type, id) {
            CacheResult::Hit((_, object)) => Some(object),
            CacheResult::NegativeHit => None,
            CacheResult::Miss => {
                let obj = self.store.get_object(id);
                if let Some(obj) = &obj {
                    self.cache_latest_object_by_id(
                        id,
                        LatestObjectCacheEntry::Object(obj.version(), obj.clone().into()),
                        ticket,
                    );
                } else {
                    self.cache_object_not_found(id, ticket);
                }
                obj
            }
        }
    }

    fn record_db_get(&self, request_type: &'static str) -> &AuthorityStore {
        self.metrics.record_cache_request(request_type, "db");
        &self.store
    }

    fn record_db_multi_get(&self, request_type: &'static str, count: usize) -> &AuthorityStore {
        self.metrics
            .record_cache_multi_request(request_type, "db", count);
        &self.store
    }

    #[instrument(level = "debug", skip_all)]
    fn write_transaction_outputs(&self, epoch_id: EpochId, tx_outputs: Arc<TransactionOutputs>) {
        trace!(digest = ?tx_outputs.transaction.digest(), "writing transaction outputs to cache");

        let TransactionOutputs {
            transaction,
            effects,
            markers,
            written,
            deleted,
            wrapped,
            events,
            ..
        } = &*tx_outputs;

        // Deletions and wraps must be written first. The reason is that one of the deletes
        // may be a child object, and if we write the parent object first, a reader may or may
        // not see the previous version of the child object, instead of the deleted/wrapped
        // tombstone, which would cause an execution fork
        for ObjectKey(id, version) in deleted.iter() {
            self.write_object_entry(id, *version, ObjectEntry::Deleted);
        }

        for ObjectKey(id, version) in wrapped.iter() {
            self.write_object_entry(id, *version, ObjectEntry::Wrapped);
        }

        // Update all markers
        for (object_key, marker_value) in markers.iter() {
            self.write_marker_value(epoch_id, *object_key, *marker_value);
        }

        // Write children before parents to ensure that readers do not observe a parent object
        // before its most recent children are visible.
        for (object_id, object) in written.iter() {
            if object.is_child_object() {
                self.write_object_entry(object_id, object.version(), object.clone().into());
            }
        }
        for (object_id, object) in written.iter() {
            if !object.is_child_object() {
                self.write_object_entry(object_id, object.version(), object.clone().into());
                if object.is_package() {
                    debug!("caching package: {:?}", object.compute_object_reference());
                    self.packages
                        .insert(*object_id, PackageObject::new(object.clone()));
                }
            }
        }

        let tx_digest = *transaction.digest();
        let effects_digest = effects.digest();

        self.metrics.record_cache_write("transaction_block");
        self.dirty
            .pending_transaction_writes
            .insert(tx_digest, tx_outputs.clone());

        // insert transaction effects before executed_effects_digests so that there
        // are never dangling entries in executed_effects_digests
        self.metrics.record_cache_write("transaction_effects");
        self.dirty
            .transaction_effects
            .insert(effects_digest, effects.clone());

        // note: if events.data.is_empty(), then there are no events for this transaction. We
        // store it anyway to avoid special cases in commint_transaction_outputs, and translate
        // an empty events structure to None when reading.
        self.metrics.record_cache_write("transaction_events");
        match self.dirty.transaction_events.entry(events.digest()) {
            DashMapEntry::Occupied(mut occupied) => {
                occupied.get_mut().0.insert(tx_digest);
            }
            DashMapEntry::Vacant(entry) => {
                let mut txns = BTreeSet::new();
                txns.insert(tx_digest);
                entry.insert((txns, events.clone()));
            }
        }

        self.metrics.record_cache_write("executed_effects_digests");
        self.dirty
            .executed_effects_digests
            .insert(tx_digest, effects_digest);

        self.executed_effects_digests_notify_read
            .notify(&tx_digest, &effects_digest);

        self.metrics
            .pending_notify_read
            .set(self.executed_effects_digests_notify_read.num_pending() as i64);

        let prev = self
            .dirty
            .total_transaction_inserts
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);

        let pending_count = (prev + 1).saturating_sub(
            self.dirty
                .total_transaction_commits
                .load(std::sync::atomic::Ordering::Relaxed),
        );

        self.set_backpressure(pending_count);
    }

    // Commits dirty data for the given TransactionDigest to the db.
    #[instrument(level = "debug", skip_all)]
    fn commit_transaction_outputs(
        &self,
        epoch: EpochId,
        digests: &[TransactionDigest],
        // TODO: Delete this parameter once table migration is complete.
        use_object_per_epoch_marker_table_v2: bool,
    ) {
        fail_point!("writeback-cache-commit");
        trace!(?digests);

        let mut all_outputs = Vec::with_capacity(digests.len());
        for tx in digests {
            let Some(outputs) = self
                .dirty
                .pending_transaction_writes
                .get(tx)
                .map(|o| o.clone())
            else {
                // This can happen in the following rare case:
                // All transactions in the checkpoint are committed to the db (by commit_transaction_outputs,
                // called in CheckpointExecutor::process_executed_transactions), but the process crashes before
                // the checkpoint water mark is bumped. We will then re-commit thhe checkpoint at startup,
                // despite that all transactions are already executed.
                warn!("Attempt to commit unknown transaction {:?}", tx);
                continue;
            };
            all_outputs.push(outputs);
        }

        // Flush writes to disk before removing anything from dirty set. otherwise,
        // a cache eviction could cause a value to disappear briefly, even if we insert to the
        // cache before removing from the dirty set.
        self.store
            .write_transaction_outputs(epoch, &all_outputs, use_object_per_epoch_marker_table_v2)
            .expect("db error");

        for outputs in all_outputs.iter() {
            let tx_digest = outputs.transaction.digest();
            assert!(self
                .dirty
                .pending_transaction_writes
                .remove(tx_digest)
                .is_some());
            self.flush_transactions_from_dirty_to_cached(epoch, *tx_digest, outputs);
        }

        let num_outputs = all_outputs.len() as u64;
        let num_commits = self
            .dirty
            .total_transaction_commits
            .fetch_add(num_outputs, std::sync::atomic::Ordering::Relaxed)
            + num_outputs;

        let pending_count = self
            .dirty
            .total_transaction_inserts
            .load(std::sync::atomic::Ordering::Relaxed)
            .saturating_sub(num_commits);

        self.set_backpressure(pending_count);
    }

    fn approximate_pending_transaction_count(&self) -> u64 {
        let num_commits = self
            .dirty
            .total_transaction_commits
            .load(std::sync::atomic::Ordering::Relaxed);

        self.dirty
            .total_transaction_inserts
            .load(std::sync::atomic::Ordering::Relaxed)
            .saturating_sub(num_commits)
    }

    fn set_backpressure(&self, pending_count: u64) {
        let backpressure = pending_count > self.backpressure_threshold;
        let backpressure_changed = self.backpressure_manager.set_backpressure(backpressure);
        if backpressure_changed {
            self.metrics.backpressure_toggles.inc();
        }
        self.metrics
            .backpressure_status
            .set(if backpressure { 1 } else { 0 });
    }

    fn flush_transactions_from_dirty_to_cached(
        &self,
        epoch: EpochId,
        tx_digest: TransactionDigest,
        outputs: &TransactionOutputs,
    ) {
        // Now, remove each piece of committed data from the dirty state and insert it into the cache.
        // TODO: outputs should have a strong count of 1 so we should be able to move out of it
        let TransactionOutputs {
            transaction,
            effects,
            markers,
            written,
            deleted,
            wrapped,
            events,
            ..
        } = outputs;

        let effects_digest = effects.digest();
        let events_digest = events.digest();

        // Update cache before removing from self.dirty to avoid
        // unnecessary cache misses
        self.cached
            .transactions
            .insert(
                &tx_digest,
                PointCacheItem::Some(transaction.clone()),
                Ticket::Write,
            )
            .ok();
        self.cached
            .transaction_effects
            .insert(
                &effects_digest,
                PointCacheItem::Some(effects.clone().into()),
                Ticket::Write,
            )
            .ok();
        self.cached
            .executed_effects_digests
            .insert(
                &tx_digest,
                PointCacheItem::Some(effects_digest),
                Ticket::Write,
            )
            .ok();
        self.cached
            .transaction_events
            .insert(
                &events_digest,
                PointCacheItem::Some(events.clone().into()),
                Ticket::Write,
            )
            .ok();

        self.dirty
            .transaction_effects
            .remove(&effects_digest)
            .expect("effects must exist");

        match self.dirty.transaction_events.entry(events.digest()) {
            DashMapEntry::Occupied(mut occupied) => {
                let txns = &mut occupied.get_mut().0;
                assert!(txns.remove(&tx_digest), "transaction must exist");
                if txns.is_empty() {
                    occupied.remove();
                }
            }
            DashMapEntry::Vacant(_) => {
                panic!("events must exist");
            }
        }

        self.dirty
            .executed_effects_digests
            .remove(&tx_digest)
            .expect("executed effects must exist");

        // Move dirty markers to cache
        for (object_key, marker_value) in markers.iter() {
            Self::move_version_from_dirty_to_cache(
                &self.dirty.markers,
                &self.cached.marker_cache,
                (epoch, object_key.id()),
                object_key.version(),
                marker_value,
            );
        }

        for (object_id, object) in written.iter() {
            Self::move_version_from_dirty_to_cache(
                &self.dirty.objects,
                &self.cached.object_cache,
                *object_id,
                object.version(),
                &ObjectEntry::Object(object.clone()),
            );
        }

        for ObjectKey(object_id, version) in deleted.iter() {
            Self::move_version_from_dirty_to_cache(
                &self.dirty.objects,
                &self.cached.object_cache,
                *object_id,
                *version,
                &ObjectEntry::Deleted,
            );
        }

        for ObjectKey(object_id, version) in wrapped.iter() {
            Self::move_version_from_dirty_to_cache(
                &self.dirty.objects,
                &self.cached.object_cache,
                *object_id,
                *version,
                &ObjectEntry::Wrapped,
            );
        }
    }

    // Move the oldest/least entry from the dirty queue to the cache queue.
    // This is called after the entry is committed to the db.
    fn move_version_from_dirty_to_cache<K, V>(
        dirty: &DashMap<K, CachedVersionMap<V>>,
        cache: &MokaCache<K, Arc<Mutex<CachedVersionMap<V>>>>,
        key: K,
        version: SequenceNumber,
        value: &V,
    ) where
        K: Eq + std::hash::Hash + Clone + Send + Sync + Copy + 'static,
        V: Send + Sync + Clone + Eq + std::fmt::Debug + 'static,
    {
        static MAX_VERSIONS: usize = 3;

        // IMPORTANT: lock both the dirty set entry and the cache entry before modifying either.
        // this ensures that readers cannot see a value temporarily disappear.
        let dirty_entry = dirty.entry(key);
        let cache_entry = cache.entry(key).or_default();
        let mut cache_map = cache_entry.value().lock();

        // insert into cache and drop old versions.
        cache_map.insert(version, value.clone());
        // TODO: make this automatic by giving CachedVersionMap an optional max capacity
        cache_map.truncate_to(MAX_VERSIONS);

        let DashMapEntry::Occupied(mut occupied_dirty_entry) = dirty_entry else {
            panic!("dirty map must exist");
        };

        let removed = occupied_dirty_entry.get_mut().pop_oldest(&version);

        assert_eq!(removed.as_ref(), Some(value), "dirty version must exist");

        // if there are no versions remaining, remove the map entry
        if occupied_dirty_entry.get().is_empty() {
            occupied_dirty_entry.remove();
        }
    }

    // Updates the latest object id cache with an entry that was read from the db.
    fn cache_latest_object_by_id(
        &self,
        object_id: &ObjectID,
        object: LatestObjectCacheEntry,
        ticket: Ticket,
    ) {
        trace!("caching object by id: {:?} {:?}", object_id, object);
        if self
            .cached
            .object_by_id_cache
            .insert(object_id, object, ticket)
            .is_ok()
        {
            self.metrics.record_cache_write("object_by_id");
        } else {
            trace!("discarded cache write due to expired ticket");
            self.metrics.record_ticket_expiry();
        }
    }

    fn cache_object_not_found(&self, object_id: &ObjectID, ticket: Ticket) {
        self.cache_latest_object_by_id(object_id, LatestObjectCacheEntry::NonExistent, ticket);
    }

    fn clear_state_end_of_epoch_impl(&self, _execution_guard: &ExecutionLockWriteGuard<'_>) {
        info!("clearing state at end of epoch");
        assert!(
            self.dirty.pending_transaction_writes.is_empty(),
            "should be empty due to revert_state_update"
        );
        self.dirty.clear();
        info!("clearing old transaction locks");
        self.object_locks.clear();
    }

    fn revert_state_update_impl(&self, tx: &TransactionDigest) {
        // TODO: remove revert_state_update_impl entirely, and simply drop all dirty
        // state when clear_state_end_of_epoch_impl is called.
        // Futher, once we do this, we can delay the insertion of the transaction into
        // pending_consensus_transactions until after the transaction has executed.
        let Some((_, outputs)) = self.dirty.pending_transaction_writes.remove(tx) else {
            assert!(
                !self.is_tx_already_executed(tx),
                "attempt to revert committed transaction"
            );

            // A transaction can be inserted into pending_consensus_transactions, but then reconfiguration
            // can happen before the transaction executes.
            info!("Not reverting {:?} as it was not executed", tx);
            return;
        };

        for (object_id, object) in outputs.written.iter() {
            if object.is_package() {
                info!("removing non-finalized package from cache: {:?}", object_id);
                self.packages.invalidate(object_id);
            }
            self.cached.object_by_id_cache.invalidate(object_id);
            self.cached.object_cache.invalidate(object_id);
        }

        for ObjectKey(object_id, _) in outputs.deleted.iter().chain(outputs.wrapped.iter()) {
            self.cached.object_by_id_cache.invalidate(object_id);
            self.cached.object_cache.invalidate(object_id);
        }

        // Note: individual object entries are removed when clear_state_end_of_epoch_impl is called
    }

    fn bulk_insert_genesis_objects_impl(&self, objects: &[Object]) {
        self.store
            .bulk_insert_genesis_objects(objects)
            .expect("db error");
        for obj in objects {
            self.cached.object_cache.invalidate(&obj.id());
            self.cached.object_by_id_cache.invalidate(&obj.id());
        }
    }

    fn insert_genesis_object_impl(&self, object: Object) {
        self.cached.object_by_id_cache.invalidate(&object.id());
        self.cached.object_cache.invalidate(&object.id());
        self.store.insert_genesis_object(object).expect("db error");
    }

    pub fn clear_caches_and_assert_empty(&self) {
        info!("clearing caches");
        self.cached.clear_and_assert_empty();
        self.packages.invalidate_all();
        assert_empty(&self.packages);
    }
}

impl ExecutionCacheAPI for WritebackCache {}

impl ExecutionCacheCommit for WritebackCache {
    fn commit_transaction_outputs(
        &self,
        epoch: EpochId,
        digests: &[TransactionDigest],
        // TODO: Delete this parameter once table migration is complete.
        use_object_per_epoch_marker_table_v2: bool,
    ) {
        WritebackCache::commit_transaction_outputs(
            self,
            epoch,
            digests,
            use_object_per_epoch_marker_table_v2,
        )
    }

    fn persist_transaction(&self, tx: &VerifiedExecutableTransaction) {
        self.store.persist_transaction(tx).expect("db error");
    }

    fn approximate_pending_transaction_count(&self) -> u64 {
        WritebackCache::approximate_pending_transaction_count(self)
    }
}

impl ObjectCacheRead for WritebackCache {
    fn get_package_object(&self, package_id: &ObjectID) -> SuiResult<Option<PackageObject>> {
        self.metrics
            .record_cache_request("package", "package_cache");
        if let Some(p) = self.packages.get(package_id) {
            if cfg!(debug_assertions) {
                let canonical_package = self
                    .dirty
                    .objects
                    .get(package_id)
                    .and_then(|v| match v.get_highest().map(|v| v.1.clone()) {
                        Some(ObjectEntry::Object(object)) => Some(object),
                        _ => None,
                    })
                    .or_else(|| self.store.get_object(package_id));

                if let Some(canonical_package) = canonical_package {
                    assert_eq!(
                        canonical_package.digest(),
                        p.object().digest(),
                        "Package object cache is inconsistent for package {:?}",
                        package_id
                    );
                }
            }
            self.metrics.record_cache_hit("package", "package_cache");
            return Ok(Some(p));
        } else {
            self.metrics.record_cache_miss("package", "package_cache");
        }

        // We try the dirty objects cache as well before going to the database. This is necessary
        // because the package could be evicted from the package cache before it is committed
        // to the database.
        if let Some(p) = self.get_object_impl("package", package_id) {
            if p.is_package() {
                let p = PackageObject::new(p);
                tracing::trace!(
                    "caching package: {:?}",
                    p.object().compute_object_reference()
                );
                self.metrics.record_cache_write("package");
                self.packages.insert(*package_id, p.clone());
                Ok(Some(p))
            } else {
                Err(SuiError::UserInputError {
                    error: UserInputError::MoveObjectAsPackage {
                        object_id: *package_id,
                    },
                })
            }
        } else {
            Ok(None)
        }
    }

    fn force_reload_system_packages(&self, _system_package_ids: &[ObjectID]) {
        // This is a no-op because all writes go through the cache, therefore it can never
        // be incoherent
    }

    // get_object and variants.

    fn get_object(&self, id: &ObjectID) -> Option<Object> {
        self.get_object_impl("object_latest", id)
    }

    fn get_object_by_key(&self, object_id: &ObjectID, version: SequenceNumber) -> Option<Object> {
        match self.get_object_by_key_cache_only(object_id, version) {
            CacheResult::Hit(object) => Some(object),
            CacheResult::NegativeHit => None,
            CacheResult::Miss => self
                .record_db_get("object_by_version")
                .get_object_by_key(object_id, version),
        }
    }

    fn multi_get_objects_by_key(&self, object_keys: &[ObjectKey]) -> Vec<Option<Object>> {
        do_fallback_lookup(
            object_keys,
            |key| match self.get_object_by_key_cache_only(&key.0, key.1) {
                CacheResult::Hit(maybe_object) => CacheResult::Hit(Some(maybe_object)),
                CacheResult::NegativeHit => CacheResult::NegativeHit,
                CacheResult::Miss => CacheResult::Miss,
            },
            |remaining| {
                self.record_db_multi_get("object_by_version", remaining.len())
                    .multi_get_objects_by_key(remaining)
                    .expect("db error")
            },
        )
    }

    fn object_exists_by_key(&self, object_id: &ObjectID, version: SequenceNumber) -> bool {
        match self.get_object_by_key_cache_only(object_id, version) {
            CacheResult::Hit(_) => true,
            CacheResult::NegativeHit => false,
            CacheResult::Miss => self
                .record_db_get("object_by_version")
                .object_exists_by_key(object_id, version)
                .expect("db error"),
        }
    }

    fn multi_object_exists_by_key(&self, object_keys: &[ObjectKey]) -> Vec<bool> {
        do_fallback_lookup(
            object_keys,
            |key| match self.get_object_by_key_cache_only(&key.0, key.1) {
                CacheResult::Hit(_) => CacheResult::Hit(true),
                CacheResult::NegativeHit => CacheResult::Hit(false),
                CacheResult::Miss => CacheResult::Miss,
            },
            |remaining| {
                self.record_db_multi_get("object_by_version", remaining.len())
                    .multi_object_exists_by_key(remaining)
                    .expect("db error")
            },
        )
    }

    fn get_latest_object_ref_or_tombstone(&self, object_id: ObjectID) -> Option<ObjectRef> {
        match self.get_object_entry_by_id_cache_only("latest_objref_or_tombstone", &object_id) {
            CacheResult::Hit((version, entry)) => Some(match entry {
                ObjectEntry::Object(object) => object.compute_object_reference(),
                ObjectEntry::Deleted => (object_id, version, ObjectDigest::OBJECT_DIGEST_DELETED),
                ObjectEntry::Wrapped => (object_id, version, ObjectDigest::OBJECT_DIGEST_WRAPPED),
            }),
            CacheResult::NegativeHit => None,
            CacheResult::Miss => self
                .record_db_get("latest_objref_or_tombstone")
                .get_latest_object_ref_or_tombstone(object_id)
                .expect("db error"),
        }
    }

    fn get_latest_object_or_tombstone(
        &self,
        object_id: ObjectID,
    ) -> Option<(ObjectKey, ObjectOrTombstone)> {
        match self.get_object_entry_by_id_cache_only("latest_object_or_tombstone", &object_id) {
            CacheResult::Hit((version, entry)) => {
                let key = ObjectKey(object_id, version);
                Some(match entry {
                    ObjectEntry::Object(object) => (key, object.into()),
                    ObjectEntry::Deleted => (
                        key,
                        ObjectOrTombstone::Tombstone((
                            object_id,
                            version,
                            ObjectDigest::OBJECT_DIGEST_DELETED,
                        )),
                    ),
                    ObjectEntry::Wrapped => (
                        key,
                        ObjectOrTombstone::Tombstone((
                            object_id,
                            version,
                            ObjectDigest::OBJECT_DIGEST_WRAPPED,
                        )),
                    ),
                })
            }
            CacheResult::NegativeHit => None,
            CacheResult::Miss => self
                .record_db_get("latest_object_or_tombstone")
                .get_latest_object_or_tombstone(object_id)
                .expect("db error"),
        }
    }

    #[instrument(level = "trace", skip_all, fields(object_id, version_bound))]
    fn find_object_lt_or_eq_version(
        &self,
        object_id: ObjectID,
        version_bound: SequenceNumber,
    ) -> Option<Object> {
        macro_rules! check_cache_entry {
            ($level: expr, $objects: expr) => {
                self.metrics
                    .record_cache_request("object_lt_or_eq_version", $level);
                if let Some(objects) = $objects {
                    if let Some((_, object)) = objects
                        .all_versions_lt_or_eq_descending(&version_bound)
                        .next()
                    {
                        if let ObjectEntry::Object(object) = object {
                            self.metrics
                                .record_cache_hit("object_lt_or_eq_version", $level);
                            return Some(object.clone());
                        } else {
                            // if we find a tombstone, the object does not exist
                            self.metrics
                                .record_cache_negative_hit("object_lt_or_eq_version", $level);
                            return None;
                        }
                    } else {
                        self.metrics
                            .record_cache_miss("object_lt_or_eq_version", $level);
                    }
                }
            };
        }

        // if we have the latest version cached, and it is within the bound, we are done
        self.metrics
            .record_cache_request("object_lt_or_eq_version", "object_by_id");
        if let Some(latest) = self.cached.object_by_id_cache.get(&object_id) {
            let latest = latest.lock();
            match &*latest {
                LatestObjectCacheEntry::Object(latest_version, object) => {
                    if *latest_version <= version_bound {
                        if let ObjectEntry::Object(object) = object {
                            self.metrics
                                .record_cache_hit("object_lt_or_eq_version", "object_by_id");
                            return Some(object.clone());
                        } else {
                            // object is a tombstone, but is still within the version bound
                            self.metrics.record_cache_negative_hit(
                                "object_lt_or_eq_version",
                                "object_by_id",
                            );
                            return None;
                        }
                    }
                    // latest object is not within the version bound. fall through.
                }
                // No object by this ID exists at all
                LatestObjectCacheEntry::NonExistent => {
                    self.metrics
                        .record_cache_negative_hit("object_lt_or_eq_version", "object_by_id");
                    return None;
                }
            }
        }
        self.metrics
            .record_cache_miss("object_lt_or_eq_version", "object_by_id");

        Self::with_locked_cache_entries(
            &self.dirty.objects,
            &self.cached.object_cache,
            &object_id,
            |dirty_entry, cached_entry| {
                check_cache_entry!("committed", dirty_entry);
                check_cache_entry!("uncommitted", cached_entry);

                // Much of the time, the query will be for the very latest object version, so
                // try that first. But we have to be careful:
                // 1. We must load the tombstone if it is present, because its version may exceed
                //    the version_bound, in which case we must do a scan.
                // 2. You might think we could just call `self.store.get_latest_object_or_tombstone` here.
                //    But we cannot, because there may be a more recent version in the dirty set, which
                //    we skipped over in check_cache_entry! because of the version bound. However, if we
                //    skipped it above, we will skip it here as well, again due to the version bound.
                // 3. Despite that, we really want to warm the cache here. Why? Because if the object is
                //    cold (not being written to), then we will very soon be able to start serving reads
                //    of it from the object_by_id cache, IF we can warm the cache. If we don't warm the
                //    the cache here, and no writes to the object occur, then we will always have to go
                //    to the db for the object.
                //
                // Lastly, it is important to understand the rationale for all this: If the object is
                // write-hot, we will serve almost all reads to it from the dirty set (or possibly the
                // cached set if it is only written to once every few checkpoints). If the object is
                // write-cold (or non-existent) and read-hot, then we will serve almost all reads to it
                // from the object_by_id cache check above.  Most of the apparently wasteful code here
                // exists only to ensure correctness in all the edge cases.
                let latest: Option<(SequenceNumber, ObjectEntry)> =
                    if let Some(dirty_set) = dirty_entry {
                        dirty_set
                            .get_highest()
                            .cloned()
                            .tap_none(|| panic!("dirty set cannot be empty"))
                    } else {
                        // TODO: we should try not to read from the db while holding the locks.
                        self.record_db_get("object_lt_or_eq_version_latest")
                            .get_latest_object_or_tombstone(object_id)
                            .expect("db error")
                            .map(|(ObjectKey(_, version), obj_or_tombstone)| {
                                (version, ObjectEntry::from(obj_or_tombstone))
                            })
                    };

                if let Some((obj_version, obj_entry)) = latest {
                    // we can always cache the latest object (or tombstone), even if it is not within the
                    // version_bound. This is done in order to warm the cache in the case where a sequence
                    // of transactions all read the same child object without writing to it.

                    // Note: no need to call with_object_by_id_cache_update here, because we are holding
                    // the lock on the dirty cache entry, and `latest` cannot become out-of-date
                    // while we hold that lock.
                    self.cache_latest_object_by_id(
                        &object_id,
                        LatestObjectCacheEntry::Object(obj_version, obj_entry.clone()),
                        // We can get a ticket at the last second, because we are holding the lock
                        // on dirty, so there cannot be any concurrent writes.
                        self.cached
                            .object_by_id_cache
                            .get_ticket_for_read(&object_id),
                    );

                    if obj_version <= version_bound {
                        match obj_entry {
                            ObjectEntry::Object(object) => Some(object),
                            ObjectEntry::Deleted | ObjectEntry::Wrapped => None,
                        }
                    } else {
                        // The latest object exceeded the bound, so now we have to do a scan
                        // But we already know there is no dirty entry within the bound,
                        // so we go to the db.
                        self.record_db_get("object_lt_or_eq_version_scan")
                            .find_object_lt_or_eq_version(object_id, version_bound)
                            .expect("db error")
                    }
                } else {
                    // no object found in dirty set or db, object does not exist
                    // When this is called from a read api (i.e. not the execution path) it is
                    // possible that the object has been deleted and pruned. In this case,
                    // there would be no entry at all on disk, but we may have a tombstone in the
                    // cache
                    let highest = cached_entry.and_then(|c| c.get_highest());
                    assert!(highest.is_none() || highest.unwrap().1.is_tombstone());
                    self.cache_object_not_found(
                        &object_id,
                        // okay to get ticket at last second - see above
                        self.cached
                            .object_by_id_cache
                            .get_ticket_for_read(&object_id),
                    );
                    None
                }
            },
        )
    }

    fn get_sui_system_state_object_unsafe(&self) -> SuiResult<SuiSystemState> {
        get_sui_system_state(self)
    }

    fn get_bridge_object_unsafe(&self) -> SuiResult<Bridge> {
        get_bridge(self)
    }

    fn get_marker_value(
        &self,
        object_key: FullObjectKey,
        epoch_id: EpochId,
        // TODO: Delete this parameter once table migration is complete.
        use_object_per_epoch_marker_table_v2: bool,
    ) -> Option<MarkerValue> {
        match self.get_marker_value_cache_only(object_key, epoch_id) {
            CacheResult::Hit(marker) => Some(marker),
            CacheResult::NegativeHit => None,
            CacheResult::Miss => self
                .record_db_get("marker_by_version")
                .get_marker_value(object_key, epoch_id, use_object_per_epoch_marker_table_v2)
                .expect("db error"),
        }
    }

    fn get_latest_marker(
        &self,
        object_id: FullObjectID,
        epoch_id: EpochId,
        // TODO: Delete this parameter once table migration is complete.
        use_object_per_epoch_marker_table_v2: bool,
    ) -> Option<(SequenceNumber, MarkerValue)> {
        match self.get_latest_marker_value_cache_only(object_id, epoch_id) {
            CacheResult::Hit((v, marker)) => Some((v, marker)),
            CacheResult::NegativeHit => {
                panic!("cannot have negative hit when getting latest marker")
            }
            CacheResult::Miss => self
                .record_db_get("marker_latest")
                .get_latest_marker(object_id, epoch_id, use_object_per_epoch_marker_table_v2)
                .expect("db error"),
        }
    }

    fn get_lock(&self, obj_ref: ObjectRef, epoch_store: &AuthorityPerEpochStore) -> SuiLockResult {
        let cur_epoch = epoch_store.epoch();
        match self.get_object_by_id_cache_only("lock", &obj_ref.0) {
            CacheResult::Hit((_, obj)) => {
                let actual_objref = obj.compute_object_reference();
                if obj_ref != actual_objref {
                    Ok(ObjectLockStatus::LockedAtDifferentVersion {
                        locked_ref: actual_objref,
                    })
                } else {
                    // requested object ref is live, check if there is a lock
                    Ok(
                        match self
                            .object_locks
                            .get_transaction_lock(&obj_ref, epoch_store)?
                        {
                            Some(tx_digest) => ObjectLockStatus::LockedToTx {
                                locked_by_tx: LockDetailsDeprecated {
                                    epoch: cur_epoch,
                                    tx_digest,
                                },
                            },
                            None => ObjectLockStatus::Initialized,
                        },
                    )
                }
            }
            CacheResult::NegativeHit => {
                Err(SuiError::from(UserInputError::ObjectNotFound {
                    object_id: obj_ref.0,
                    // even though we know the requested version, we leave it as None to indicate
                    // that the object does not exist at any version
                    version: None,
                }))
            }
            CacheResult::Miss => self.record_db_get("lock").get_lock(obj_ref, epoch_store),
        }
    }

    fn _get_live_objref(&self, object_id: ObjectID) -> SuiResult<ObjectRef> {
        let obj = self.get_object_impl("live_objref", &object_id).ok_or(
            UserInputError::ObjectNotFound {
                object_id,
                version: None,
            },
        )?;
        Ok(obj.compute_object_reference())
    }

    fn check_owned_objects_are_live(&self, owned_object_refs: &[ObjectRef]) -> SuiResult {
        do_fallback_lookup_fallible(
            owned_object_refs,
            |obj_ref| match self.get_object_by_id_cache_only("object_is_live", &obj_ref.0) {
                CacheResult::Hit((version, obj)) => {
                    if obj.compute_object_reference() != *obj_ref {
                        Err(UserInputError::ObjectVersionUnavailableForConsumption {
                            provided_obj_ref: *obj_ref,
                            current_version: version,
                        }
                        .into())
                    } else {
                        Ok(CacheResult::Hit(()))
                    }
                }
                CacheResult::NegativeHit => Err(UserInputError::ObjectNotFound {
                    object_id: obj_ref.0,
                    version: None,
                }
                .into()),
                CacheResult::Miss => Ok(CacheResult::Miss),
            },
            |remaining| {
                self.record_db_multi_get("object_is_live", remaining.len())
                    .check_owned_objects_are_live(remaining)?;
                Ok(vec![(); remaining.len()])
            },
        )?;
        Ok(())
    }

    fn get_highest_pruned_checkpoint(&self) -> CheckpointSequenceNumber {
        self.store
            .perpetual_tables
            .get_highest_pruned_checkpoint()
            .expect("db error")
    }
}

impl TransactionCacheRead for WritebackCache {
    fn multi_get_transaction_blocks(
        &self,
        digests: &[TransactionDigest],
    ) -> Vec<Option<Arc<VerifiedTransaction>>> {
        let digests_and_tickets: Vec<_> = digests
            .iter()
            .map(|d| (*d, self.cached.transactions.get_ticket_for_read(d)))
            .collect();
        do_fallback_lookup(
            &digests_and_tickets,
            |(digest, _)| {
                self.metrics
                    .record_cache_request("transaction_block", "uncommitted");
                if let Some(tx) = self.dirty.pending_transaction_writes.get(digest) {
                    self.metrics
                        .record_cache_hit("transaction_block", "uncommitted");
                    return CacheResult::Hit(Some(tx.transaction.clone()));
                }
                self.metrics
                    .record_cache_miss("transaction_block", "uncommitted");

                self.metrics
                    .record_cache_request("transaction_block", "committed");

                match self
                    .cached
                    .transactions
                    .get(digest)
                    .map(|l| l.lock().clone())
                {
                    Some(PointCacheItem::Some(tx)) => {
                        self.metrics
                            .record_cache_hit("transaction_block", "committed");
                        CacheResult::Hit(Some(tx))
                    }
                    Some(PointCacheItem::None) => CacheResult::NegativeHit,
                    None => {
                        self.metrics
                            .record_cache_miss("transaction_block", "committed");

                        CacheResult::Miss
                    }
                }
            },
            |remaining| {
                let remaining_digests: Vec<_> = remaining.iter().map(|(d, _)| *d).collect();
                let results: Vec<_> = self
                    .record_db_multi_get("transaction_block", remaining.len())
                    .multi_get_transaction_blocks(&remaining_digests)
                    .expect("db error")
                    .into_iter()
                    .map(|o| o.map(Arc::new))
                    .collect();
                for ((digest, ticket), result) in remaining.iter().zip(results.iter()) {
                    if result.is_none() {
                        self.cached.transactions.insert(digest, None, *ticket).ok();
                    }
                }
                results
            },
        )
    }

    fn multi_get_executed_effects_digests(
        &self,
        digests: &[TransactionDigest],
    ) -> Vec<Option<TransactionEffectsDigest>> {
        let digests_and_tickets: Vec<_> = digests
            .iter()
            .map(|d| {
                (
                    *d,
                    self.cached.executed_effects_digests.get_ticket_for_read(d),
                )
            })
            .collect();
        do_fallback_lookup(
            &digests_and_tickets,
            |(digest, _)| {
                self.metrics
                    .record_cache_request("executed_effects_digests", "uncommitted");
                if let Some(digest) = self.dirty.executed_effects_digests.get(digest) {
                    self.metrics
                        .record_cache_hit("executed_effects_digests", "uncommitted");
                    return CacheResult::Hit(Some(*digest));
                }
                self.metrics
                    .record_cache_miss("executed_effects_digests", "uncommitted");

                self.metrics
                    .record_cache_request("executed_effects_digests", "committed");
                match self
                    .cached
                    .executed_effects_digests
                    .get(digest)
                    .map(|l| *l.lock())
                {
                    Some(PointCacheItem::Some(digest)) => {
                        self.metrics
                            .record_cache_hit("executed_effects_digests", "committed");
                        CacheResult::Hit(Some(digest))
                    }
                    Some(PointCacheItem::None) => CacheResult::NegativeHit,
                    None => {
                        self.metrics
                            .record_cache_miss("executed_effects_digests", "committed");
                        CacheResult::Miss
                    }
                }
            },
            |remaining| {
                let remaining_digests: Vec<_> = remaining.iter().map(|(d, _)| *d).collect();
                let results = self
                    .record_db_multi_get("executed_effects_digests", remaining.len())
                    .multi_get_executed_effects_digests(&remaining_digests)
                    .expect("db error");
                for ((digest, ticket), result) in remaining.iter().zip(results.iter()) {
                    if result.is_none() {
                        self.cached
                            .executed_effects_digests
                            .insert(digest, None, *ticket)
                            .ok();
                    }
                }
                results
            },
        )
    }

    fn multi_get_effects(
        &self,
        digests: &[TransactionEffectsDigest],
    ) -> Vec<Option<TransactionEffects>> {
        let digests_and_tickets: Vec<_> = digests
            .iter()
            .map(|d| (*d, self.cached.transaction_effects.get_ticket_for_read(d)))
            .collect();
        do_fallback_lookup(
            &digests_and_tickets,
            |(digest, _)| {
                self.metrics
                    .record_cache_request("transaction_effects", "uncommitted");
                if let Some(effects) = self.dirty.transaction_effects.get(digest) {
                    self.metrics
                        .record_cache_hit("transaction_effects", "uncommitted");
                    return CacheResult::Hit(Some(effects.clone()));
                }
                self.metrics
                    .record_cache_miss("transaction_effects", "uncommitted");

                self.metrics
                    .record_cache_request("transaction_effects", "committed");
                match self
                    .cached
                    .transaction_effects
                    .get(digest)
                    .map(|l| l.lock().clone())
                {
                    Some(PointCacheItem::Some(effects)) => {
                        self.metrics
                            .record_cache_hit("transaction_effects", "committed");
                        CacheResult::Hit(Some((*effects).clone()))
                    }
                    Some(PointCacheItem::None) => CacheResult::NegativeHit,
                    None => {
                        self.metrics
                            .record_cache_miss("transaction_effects", "committed");
                        CacheResult::Miss
                    }
                }
            },
            |remaining| {
                let remaining_digests: Vec<_> = remaining.iter().map(|(d, _)| *d).collect();
                let results = self
                    .record_db_multi_get("transaction_effects", remaining.len())
                    .multi_get_effects(remaining_digests.iter())
                    .expect("db error");
                for ((digest, ticket), result) in remaining.iter().zip(results.iter()) {
                    if result.is_none() {
                        self.cached
                            .transaction_effects
                            .insert(digest, None, *ticket)
                            .ok();
                    }
                }
                results
            },
        )
    }

    fn notify_read_executed_effects_digests<'a>(
        &'a self,
        digests: &'a [TransactionDigest],
    ) -> BoxFuture<'a, Vec<TransactionEffectsDigest>> {
        self.executed_effects_digests_notify_read
            .read(digests, |digests| {
                self.multi_get_executed_effects_digests(digests)
            })
            .boxed()
    }

    fn multi_get_events(
        &self,
        event_digests: &[TransactionEventsDigest],
    ) -> Vec<Option<TransactionEvents>> {
        fn map_events(events: TransactionEvents) -> Option<TransactionEvents> {
            if events.data.is_empty() {
                None
            } else {
                Some(events)
            }
        }

        let digests_and_tickets: Vec<_> = event_digests
            .iter()
            .map(|d| (*d, self.cached.transaction_events.get_ticket_for_read(d)))
            .collect();
        do_fallback_lookup(
            &digests_and_tickets,
            |(digest, _)| {
                self.metrics
                    .record_cache_request("transaction_events", "uncommitted");
                if let Some(events) = self
                    .dirty
                    .transaction_events
                    .get(digest)
                    .map(|e| e.1.clone())
                {
                    self.metrics
                        .record_cache_hit("transaction_events", "uncommitted");

                    return CacheResult::Hit(map_events(events));
                }
                self.metrics
                    .record_cache_miss("transaction_events", "uncommitted");

                self.metrics
                    .record_cache_request("transaction_events", "committed");
                match self
                    .cached
                    .transaction_events
                    .get(digest)
                    .map(|l| l.lock().clone())
                {
                    Some(PointCacheItem::Some(events)) => {
                        self.metrics
                            .record_cache_hit("transaction_events", "committed");
                        CacheResult::Hit(map_events((*events).clone()))
                    }
                    Some(PointCacheItem::None) => CacheResult::NegativeHit,
                    None => {
                        self.metrics
                            .record_cache_miss("transaction_events", "committed");

                        CacheResult::Miss
                    }
                }
            },
            |remaining| {
                let remaining_digests: Vec<_> = remaining.iter().map(|(d, _)| *d).collect();
                let results = self
                    .store
                    .multi_get_events(&remaining_digests)
                    .expect("db error");
                for ((digest, ticket), result) in remaining.iter().zip(results.iter()) {
                    if result.is_none() {
                        self.cached
                            .transaction_events
                            .insert(digest, None, *ticket)
                            .ok();
                    }
                }
                results
            },
        )
    }
}

impl ExecutionCacheWrite for WritebackCache {
    fn acquire_transaction_locks(
        &self,
        epoch_store: &AuthorityPerEpochStore,
        owned_input_objects: &[ObjectRef],
        tx_digest: TransactionDigest,
        signed_transaction: Option<VerifiedSignedTransaction>,
    ) -> SuiResult {
        self.object_locks.acquire_transaction_locks(
            self,
            epoch_store,
            owned_input_objects,
            tx_digest,
            signed_transaction,
        )
    }

    fn write_transaction_outputs(
        &self,
        epoch_id: EpochId,
        tx_outputs: Arc<TransactionOutputs>,
        // TODO: Delete this parameter once table migration is complete.
        _use_object_per_epoch_marker_table_v2: bool,
    ) {
        WritebackCache::write_transaction_outputs(self, epoch_id, tx_outputs);
    }
}

implement_passthrough_traits!(WritebackCache);

impl AccumulatorStore for WritebackCache {
    fn get_object_ref_prior_to_key_deprecated(
        &self,
        object_id: &ObjectID,
        version: SequenceNumber,
    ) -> SuiResult<Option<ObjectRef>> {
        // There is probably a more efficient way to implement this, but since this is only used by
        // old protocol versions, it is better to do the simple thing that is obviously correct.
        // In this case we previous version from all sources and choose the highest
        let mut candidates = Vec::new();

        let check_versions =
            |versions: &CachedVersionMap<ObjectEntry>| match versions.get_prior_to(&version) {
                Some((version, object_entry)) => match object_entry {
                    ObjectEntry::Object(object) => {
                        assert_eq!(object.version(), version);
                        Some(object.compute_object_reference())
                    }
                    ObjectEntry::Deleted => {
                        Some((*object_id, version, ObjectDigest::OBJECT_DIGEST_DELETED))
                    }
                    ObjectEntry::Wrapped => {
                        Some((*object_id, version, ObjectDigest::OBJECT_DIGEST_WRAPPED))
                    }
                },
                None => None,
            };

        // first check dirty data
        if let Some(objects) = self.dirty.objects.get(object_id) {
            if let Some(prior) = check_versions(&objects) {
                candidates.push(prior);
            }
        }

        if let Some(objects) = self.cached.object_cache.get(object_id) {
            if let Some(prior) = check_versions(&objects.lock()) {
                candidates.push(prior);
            }
        }

        if let Some(prior) = self
            .store
            .get_object_ref_prior_to_key_deprecated(object_id, version)?
        {
            candidates.push(prior);
        }

        // sort candidates by version, and return the highest
        candidates.sort_by_key(|(_, version, _)| *version);
        Ok(candidates.pop())
    }

    fn get_root_state_accumulator_for_epoch(
        &self,
        epoch: EpochId,
    ) -> SuiResult<Option<(CheckpointSequenceNumber, Accumulator)>> {
        self.store.get_root_state_accumulator_for_epoch(epoch)
    }

    fn get_root_state_accumulator_for_highest_epoch(
        &self,
    ) -> SuiResult<Option<(EpochId, (CheckpointSequenceNumber, Accumulator))>> {
        self.store.get_root_state_accumulator_for_highest_epoch()
    }

    fn insert_state_accumulator_for_epoch(
        &self,
        epoch: EpochId,
        checkpoint_seq_num: &CheckpointSequenceNumber,
        acc: &Accumulator,
    ) -> SuiResult {
        self.store
            .insert_state_accumulator_for_epoch(epoch, checkpoint_seq_num, acc)
    }

    fn iter_live_object_set(
        &self,
        include_wrapped_tombstone: bool,
    ) -> Box<dyn Iterator<Item = LiveObject> + '_> {
        // The only time it is safe to iterate the live object set is at an epoch boundary,
        // at which point the db is consistent and the dirty cache is empty. So this does
        // read the cache
        assert!(
            self.dirty.is_empty(),
            "cannot iterate live object set with dirty data"
        );
        self.store.iter_live_object_set(include_wrapped_tombstone)
    }

    // A version of iter_live_object_set that reads the cache. Only use for testing. If used
    // on a live validator, can cause the server to block for as long as it takes to iterate
    // the entire live object set.
    fn iter_cached_live_object_set_for_testing(
        &self,
        include_wrapped_tombstone: bool,
    ) -> Box<dyn Iterator<Item = LiveObject> + '_> {
        // hold iter until we are finished to prevent any concurrent inserts/deletes
        let iter = self.dirty.objects.iter();
        let mut dirty_objects = BTreeMap::new();

        // add everything from the store
        for obj in self.store.iter_live_object_set(include_wrapped_tombstone) {
            dirty_objects.insert(obj.object_id(), obj);
        }

        // add everything from the cache, but also remove deletions
        for entry in iter {
            let id = *entry.key();
            let value = entry.value();
            match value.get_highest().unwrap() {
                (_, ObjectEntry::Object(object)) => {
                    dirty_objects.insert(id, LiveObject::Normal(object.clone()));
                }
                (version, ObjectEntry::Wrapped) => {
                    if include_wrapped_tombstone {
                        dirty_objects.insert(id, LiveObject::Wrapped(ObjectKey(id, *version)));
                    } else {
                        dirty_objects.remove(&id);
                    }
                }
                (_, ObjectEntry::Deleted) => {
                    dirty_objects.remove(&id);
                }
            }
        }

        Box::new(dirty_objects.into_values())
    }
}

// TODO: For correctness, we must at least invalidate the cache when items are written through this
// trait (since they could be negatively cached as absent). But it may or may not be optimal to
// actually insert them into the cache. For instance if state sync is running ahead of execution,
// they might evict other items that are about to be read. This could be an area for tuning in the
// future.
impl StateSyncAPI for WritebackCache {
    fn insert_transaction_and_effects(
        &self,
        transaction: &VerifiedTransaction,
        transaction_effects: &TransactionEffects,
    ) {
        self.store
            .insert_transaction_and_effects(transaction, transaction_effects)
            .expect("db error");
        self.cached
            .transactions
            .insert(
                transaction.digest(),
                PointCacheItem::Some(Arc::new(transaction.clone())),
                Ticket::Write,
            )
            .ok();
        self.cached
            .transaction_effects
            .insert(
                &transaction_effects.digest(),
                PointCacheItem::Some(Arc::new(transaction_effects.clone())),
                Ticket::Write,
            )
            .ok();
    }

    fn multi_insert_transaction_and_effects(
        &self,
        transactions_and_effects: &[VerifiedExecutionData],
    ) {
        self.store
            .multi_insert_transaction_and_effects(transactions_and_effects.iter())
            .expect("db error");
        for VerifiedExecutionData {
            transaction,
            effects,
        } in transactions_and_effects
        {
            self.cached
                .transactions
                .insert(
                    transaction.digest(),
                    PointCacheItem::Some(Arc::new(transaction.clone())),
                    Ticket::Write,
                )
                .ok();
            self.cached
                .transaction_effects
                .insert(
                    &effects.digest(),
                    PointCacheItem::Some(Arc::new(effects.clone())),
                    Ticket::Write,
                )
                .ok();
        }
    }
}
