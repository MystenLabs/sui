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
use crate::authority::authority_store::{ExecutionLockWriteGuard, SuiLockResult};
use crate::authority::authority_store_pruner::{
    AuthorityStorePruner, AuthorityStorePruningMetrics,
};
use crate::authority::authority_store_tables::LiveObject;
use crate::authority::epoch_start_configuration::{EpochFlag, EpochStartConfiguration};
use crate::authority::AuthorityStore;
use crate::checkpoints::CheckpointStore;
use crate::state_accumulator::AccumulatorStore;
use crate::transaction_outputs::TransactionOutputs;

use dashmap::mapref::entry::Entry as DashMapEntry;
use dashmap::DashMap;
use either::Either;
use futures::{
    future::{join_all, BoxFuture},
    FutureExt,
};
use moka::sync::Cache as MokaCache;
use mysten_common::sync::notify_read::NotifyRead;
use parking_lot::Mutex;
use prometheus::Registry;
use std::collections::BTreeSet;
use std::hash::Hash;
use std::sync::Arc;
use sui_config::node::AuthorityStorePruningConfig;
use sui_macros::fail_point_async;
use sui_protocol_config::ProtocolVersion;
use sui_types::accumulator::Accumulator;
use sui_types::base_types::{EpochId, ObjectID, ObjectRef, SequenceNumber, VerifiedExecutionData};
use sui_types::digests::{
    ObjectDigest, TransactionDigest, TransactionEffectsDigest, TransactionEventsDigest,
};
use sui_types::effects::{TransactionEffects, TransactionEvents};
use sui_types::error::{SuiError, SuiResult, UserInputError};
use sui_types::message_envelope::Message;
use sui_types::messages_checkpoint::CheckpointSequenceNumber;
use sui_types::object::Object;
use sui_types::storage::{MarkerValue, ObjectKey, ObjectOrTombstone, ObjectStore, PackageObject};
use sui_types::sui_system_state::{get_sui_system_state, SuiSystemState};
use sui_types::transaction::VerifiedTransaction;
use tracing::{info, instrument};

use super::ExecutionCacheAPI;
use super::{
    cached_version_map::CachedVersionMap, implement_passthrough_traits, CheckpointCache,
    ExecutionCacheCommit, ExecutionCacheMetrics, ExecutionCacheRead, ExecutionCacheReconfigAPI,
    ExecutionCacheWrite, NotifyReadWrapper, StateSyncAPI,
};

#[cfg(test)]
#[path = "unit_tests/writeback_cache_tests.rs"]
mod writeback_cache_tests;

#[derive(Clone, PartialEq, Eq)]
enum ObjectEntry {
    Object(Object),
    Deleted,
    Wrapped,
}

#[cfg(test)]
impl ObjectEntry {
    fn unwrap_object(&self) -> &Object {
        match self {
            ObjectEntry::Object(o) => o,
            _ => panic!("unwrap_object called on non-Object"),
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

type MarkerKey = (EpochId, ObjectID);

enum CacheResult<T> {
    /// Entry is in the cache
    Hit(T),
    /// Entry is not in the cache and is known to not exist
    NegativeHit,
    /// Entry is not in the cache and may or may not exist in the store
    Miss,
}

/// UncommitedData stores execution outputs that are not yet written to the db. Entries in this
/// struct can only be purged after they are committed.
struct UncommittedData {
    /// The object dirty set. All writes go into this table first. After we flush the data to the
    /// db, the data is removed from this table and inserted into the object_cache.
    ///
    /// This table may contain both live and dead objects, since we flush both live and dead
    /// objects to the db in order to support past object queries on fullnodes.
    ///
    /// Further, we only remove objects in FIFO order, which ensures that the the cached
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
        }
    }

    fn clear(&self) {
        self.objects.clear();
        self.markers.clear();
        self.transaction_effects.clear();
        self.executed_effects_digests.clear();
        self.pending_transaction_writes.clear();
        self.transaction_events.clear();
    }
}

/// CachedData stores data that has been committed to the db, but is likely to be read soon.
struct CachedCommittedData {
    // See module level comment for an explanation of caching strategy.
    object_cache: MokaCache<ObjectID, Arc<Mutex<CachedVersionMap<ObjectEntry>>>>,

    // See module level comment for an explanation of caching strategy.
    marker_cache: MokaCache<MarkerKey, Arc<Mutex<CachedVersionMap<MarkerValue>>>>,

    // Objects that were read at transaction signing time - allows us to access them again at
    // execution time with a single lock / hash lookup
    _transaction_objects: MokaCache<TransactionDigest, Vec<Object>>,
}

impl CachedCommittedData {
    fn new() -> Self {
        let object_cache = MokaCache::builder()
            .max_capacity(10000)
            .initial_capacity(10000)
            .build();
        let marker_cache = MokaCache::builder()
            .max_capacity(10000)
            .initial_capacity(10000)
            .build();
        let transaction_objects = MokaCache::builder()
            .max_capacity(10000)
            .initial_capacity(10000)
            .build();

        Self {
            object_cache,
            marker_cache,
            _transaction_objects: transaction_objects,
        }
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

    executed_effects_digests_notify_read: NotifyRead<TransactionDigest, TransactionEffectsDigest>,
    store: Arc<AuthorityStore>,
    metrics: Arc<ExecutionCacheMetrics>,
}

macro_rules! check_cache_entry_by_version {
    ($cache: expr, $version: expr) => {
        if let Some(cache) = $cache {
            if let Some(entry) = cache.get(&$version) {
                return CacheResult::Hit(entry.clone());
            }

            if let Some(least_version) = cache.get_least() {
                if least_version.0 < $version {
                    // If the version is greater than the least version in the cache, then we know
                    // that the object does not exist anywhere
                    return CacheResult::NegativeHit;
                }
            }
        }
    };
}

macro_rules! check_cache_entry_by_latest {
    ($cache: expr) => {
        if let Some(cache) = $cache {
            if let Some((version, entry)) = cache.get_highest() {
                return CacheResult::Hit((*version, entry.clone()));
            } else {
                panic!("empty CachedVersionMap should have been removed");
            }
        }
    };
}

impl WritebackCache {
    fn new(store: Arc<AuthorityStore>, metrics: Arc<ExecutionCacheMetrics>) -> Self {
        let packages = MokaCache::builder()
            .max_capacity(10000)
            .initial_capacity(10000)
            .build();
        Self {
            dirty: UncommittedData::new(),
            cached: CachedCommittedData::new(),
            packages,
            executed_effects_digests_notify_read: NotifyRead::new(),
            store,
            metrics,
        }
    }

    pub fn new_for_tests(store: Arc<AuthorityStore>, registry: &Registry) -> Self {
        Self::new(store, ExecutionCacheMetrics::new(registry).into())
    }

    #[cfg(test)]
    pub fn reset_for_test(&mut self) {
        let mut new = Self::new(self.store.clone(), self.metrics.clone());
        std::mem::swap(self, &mut new);
    }

    async fn write_object_entry(
        &self,
        object_id: &ObjectID,
        version: SequenceNumber,
        object: ObjectEntry,
    ) {
        tracing::trace!("inserting object entry {:?}: {:?}", object_id, version);
        fail_point_async!("write_object_entry");
        self.dirty
            .objects
            .entry(*object_id)
            .or_default()
            .insert(version, object);
    }

    async fn write_marker_value(
        &self,
        epoch_id: EpochId,
        object_key: &ObjectKey,
        marker_value: MarkerValue,
    ) {
        tracing::trace!(
            "inserting marker value {:?}: {:?}",
            object_key,
            marker_value
        );
        fail_point_async!("write_marker_entry");
        self.dirty
            .markers
            .entry((epoch_id, object_key.0))
            .or_default()
            .value_mut()
            .insert(object_key.1, marker_value);
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
                check_cache_entry_by_version!(dirty_entry, version);
                check_cache_entry_by_version!(cached_entry, version);
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
        object_id: &ObjectID,
    ) -> CacheResult<(SequenceNumber, ObjectEntry)> {
        Self::with_locked_cache_entries(
            &self.dirty.objects,
            &self.cached.object_cache,
            object_id,
            |dirty_entry, cached_entry| {
                check_cache_entry_by_latest!(dirty_entry);
                check_cache_entry_by_latest!(cached_entry);

                CacheResult::Miss
            },
        )
    }

    fn get_object_by_id_cache_only(
        &self,
        object_id: &ObjectID,
    ) -> CacheResult<(SequenceNumber, Object)> {
        match self.get_object_entry_by_id_cache_only(object_id) {
            CacheResult::Hit((version, entry)) => match entry {
                ObjectEntry::Object(object) => CacheResult::Hit((version, object)),
                ObjectEntry::Deleted | ObjectEntry::Wrapped => CacheResult::NegativeHit,
            },
            CacheResult::NegativeHit => {
                panic!("cannot have negative hit when getting latest object")
            }
            CacheResult::Miss => CacheResult::Miss,
        }
    }

    fn get_marker_value_cache_only(
        &self,
        object_id: &ObjectID,
        version: SequenceNumber,
        epoch_id: EpochId,
    ) -> CacheResult<MarkerValue> {
        Self::with_locked_cache_entries(
            &self.dirty.markers,
            &self.cached.marker_cache,
            &(epoch_id, *object_id),
            |dirty_entry, cached_entry| {
                check_cache_entry_by_version!(dirty_entry, version);
                check_cache_entry_by_version!(cached_entry, version);
                CacheResult::Miss
            },
        )
    }

    fn get_latest_marker_value_cache_only(
        &self,
        object_id: &ObjectID,
        epoch_id: EpochId,
    ) -> CacheResult<(SequenceNumber, MarkerValue)> {
        Self::with_locked_cache_entries(
            &self.dirty.markers,
            &self.cached.marker_cache,
            &(epoch_id, *object_id),
            |dirty_entry, cached_entry| {
                check_cache_entry_by_latest!(dirty_entry);
                check_cache_entry_by_latest!(cached_entry);
                CacheResult::Miss
            },
        )
    }

    // Commits dirty data for the given TransactionDigest to the db.
    async fn commit_transaction_outputs(
        &self,
        epoch: EpochId,
        digest: TransactionDigest,
    ) -> SuiResult {
        let Some((_, outputs)) = self.dirty.pending_transaction_writes.remove(&digest) else {
            panic!("Attempt to commit unknown transaction {:?}", digest);
        };

        // Flush writes to disk
        self.store
            .write_transaction_outputs(epoch, outputs.clone())
            .await?;

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
        } = &*outputs;

        // Move dirty markers to cache
        for (object_key, marker_value) in markers.iter() {
            Self::move_version_from_dirty_to_cache(
                &self.dirty.markers,
                &self.cached.marker_cache,
                (epoch, object_key.0),
                object_key.1,
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

        let tx_digest = *transaction.digest();
        let effects_digest = effects.digest();

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

        Ok(())
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

    pub async fn prune_objects_and_compact_for_testing(
        &self,
        checkpoint_store: &Arc<CheckpointStore>,
    ) {
        let pruning_config = AuthorityStorePruningConfig {
            num_epochs_to_retain: 0,
            ..Default::default()
        };
        let _ = AuthorityStorePruner::prune_objects_for_eligible_epochs(
            &self.store.perpetual_tables,
            checkpoint_store,
            &self.store.objects_lock_table,
            pruning_config,
            AuthorityStorePruningMetrics::new_for_test(),
            usize::MAX,
        )
        .await;
        let _ = AuthorityStorePruner::compact(&self.store.perpetual_tables);
    }

    pub fn store_for_testing(&self) -> &Arc<AuthorityStore> {
        &self.store
    }

    pub fn as_notify_read_wrapper(self: Arc<Self>) -> NotifyReadWrapper<Self> {
        NotifyReadWrapper(self)
    }

    fn clear_state_end_of_epoch_impl(&self, _execution_guard: &ExecutionLockWriteGuard<'_>) {
        info!("clearing state at end of epoch");
        assert!(
            self.dirty.pending_transaction_writes.is_empty(),
            "should be empty due to revert_state_update"
        );
        self.dirty.clear();
    }

    fn revert_state_update_impl(&self, tx: &TransactionDigest) -> SuiResult {
        // TODO: remove revert_state_update_impl entirely, and simply drop all dirty
        // state when clear_state_end_of_epoch_impl is called.
        let (_, outputs) = self
            .dirty
            .pending_transaction_writes
            .remove(tx)
            .expect("transaction must exist");

        for (object_id, object) in outputs.written.iter() {
            if object.is_package() {
                info!("removing non-finalized package from cache: {:?}", object_id);
                self.packages.invalidate(object_id);
            }
        }

        Ok(())
    }

    #[cfg(test)]
    pub fn clear_caches(&self) {
        self.cached.object_cache.invalidate_all();
        self.packages.invalidate_all();
        self.cached.marker_cache.invalidate_all();
        self.cached._transaction_objects.invalidate_all();
    }
}

impl ExecutionCacheAPI for WritebackCache {}

impl ExecutionCacheCommit for WritebackCache {
    fn commit_transaction_outputs(
        &self,
        epoch: EpochId,
        digest: &TransactionDigest,
    ) -> BoxFuture<'_, SuiResult> {
        WritebackCache::commit_transaction_outputs(self, epoch, *digest).boxed()
    }
}

impl ExecutionCacheRead for WritebackCache {
    fn get_package_object(&self, package_id: &ObjectID) -> SuiResult<Option<PackageObject>> {
        if let Some(p) = self.packages.get(package_id) {
            if cfg!(debug_assertions) {
                if let Some(store_package) = self.store.get_object(package_id).unwrap() {
                    assert_eq!(
                        store_package.digest(),
                        p.object().digest(),
                        "Package object cache is inconsistent for package {:?}",
                        package_id
                    );
                }
            }
            return Ok(Some(p));
        }

        // We try the dirty objects cache as well before going to the database. This is necessary
        // because the package could be evicted from the package cache before it is committed
        // to the database.
        if let Some(p) = ExecutionCacheRead::get_object(self, package_id)? {
            if p.is_package() {
                let p = PackageObject::new(p);
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

    // TOOO: we may not need this function now that all writes go through the cache
    fn force_reload_system_packages(&self, system_package_ids: &[ObjectID]) {
        for package_id in system_package_ids {
            if let Some(p) = self
                .store
                .get_object(package_id)
                .expect("Failed to update system packages")
            {
                assert!(p.is_package());
                self.packages.insert(*package_id, PackageObject::new(p));
            }
            // It's possible that a package is not found if it's newly added system package ID
            // that hasn't got created yet. This should be very very rare though.
        }
    }

    // get_object and variants.
    //
    // TODO: We don't insert objects into the cache after misses because they are usually only
    // read once. We might want to cache immutable reads (RO shared objects and immutable objects)
    // If we do this, we must be VERY CAREFUL not to break the contiguous version property
    // of the cache.

    fn get_object(&self, id: &ObjectID) -> SuiResult<Option<Object>> {
        match self.get_object_by_id_cache_only(id) {
            CacheResult::Hit((_, object)) => Ok(Some(object)),
            CacheResult::NegativeHit => Ok(None),
            CacheResult::Miss => Ok(self.store.get_object(id)?),
        }
    }

    fn get_object_by_key(
        &self,
        object_id: &ObjectID,
        version: SequenceNumber,
    ) -> SuiResult<Option<Object>> {
        match self.get_object_by_key_cache_only(object_id, version) {
            CacheResult::Hit(object) => Ok(Some(object)),
            CacheResult::NegativeHit => Ok(None),
            CacheResult::Miss => Ok(self.store.get_object_by_key(object_id, version)?),
        }
    }

    fn multi_get_objects_by_key(
        &self,
        object_keys: &[ObjectKey],
    ) -> Result<Vec<Option<Object>>, SuiError> {
        do_fallback_lookup(
            object_keys,
            |key| match self.get_object_by_key_cache_only(&key.0, key.1) {
                CacheResult::Hit(maybe_object) => CacheResult::Hit(Some(maybe_object)),
                CacheResult::NegativeHit => CacheResult::NegativeHit,
                CacheResult::Miss => CacheResult::Miss,
            },
            |remaining| {
                self.store
                    .multi_get_objects_by_key(remaining)
                    .map_err(Into::into)
            },
        )
    }

    fn object_exists_by_key(
        &self,
        object_id: &ObjectID,
        version: SequenceNumber,
    ) -> SuiResult<bool> {
        match self.get_object_by_key_cache_only(object_id, version) {
            CacheResult::Hit(_) => Ok(true),
            CacheResult::NegativeHit => Ok(false),
            CacheResult::Miss => self.store.object_exists_by_key(object_id, version),
        }
    }

    fn multi_object_exists_by_key(&self, object_keys: &[ObjectKey]) -> SuiResult<Vec<bool>> {
        do_fallback_lookup(
            object_keys,
            |key| match self.get_object_by_key_cache_only(&key.0, key.1) {
                CacheResult::Hit(_) => CacheResult::Hit(true),
                CacheResult::NegativeHit => CacheResult::Hit(false),
                CacheResult::Miss => CacheResult::Miss,
            },
            |remaining| self.store.multi_object_exists_by_key(remaining),
        )
    }

    fn get_latest_object_ref_or_tombstone(
        &self,
        object_id: ObjectID,
    ) -> SuiResult<Option<ObjectRef>> {
        match self.get_object_entry_by_id_cache_only(&object_id) {
            CacheResult::Hit((version, entry)) => Ok(Some(match entry {
                ObjectEntry::Object(object) => object.compute_object_reference(),
                ObjectEntry::Deleted => (object_id, version, ObjectDigest::OBJECT_DIGEST_DELETED),
                ObjectEntry::Wrapped => (object_id, version, ObjectDigest::OBJECT_DIGEST_WRAPPED),
            })),
            CacheResult::NegativeHit => {
                panic!("cannot have negative hit when getting latest object or tombstone")
            }
            CacheResult::Miss => self.store.get_latest_object_ref_or_tombstone(object_id),
        }
    }

    fn get_latest_object_or_tombstone(
        &self,
        object_id: ObjectID,
    ) -> Result<Option<(ObjectKey, ObjectOrTombstone)>, SuiError> {
        match self.get_object_entry_by_id_cache_only(&object_id) {
            CacheResult::Hit((version, entry)) => {
                let key = ObjectKey(object_id, version);
                Ok(Some(match entry {
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
                }))
            }
            CacheResult::NegativeHit => {
                panic!("cannot have negative hit when getting latest object or tombstone")
            }
            CacheResult::Miss => self.store.get_latest_object_or_tombstone(object_id),
        }
    }

    fn find_object_lt_or_eq_version(
        &self,
        object_id: ObjectID,
        version: SequenceNumber,
    ) -> SuiResult<Option<Object>> {
        macro_rules! check_cache_entry {
            ($objects: expr) => {
                if let Some(objects) = $objects {
                    if let Some((_, object)) =
                        objects.all_versions_lt_or_eq_descending(&version).next()
                    {
                        if let ObjectEntry::Object(object) = object {
                            return Ok(Some(object.clone()));
                        } else {
                            // if we find a tombstone, the object does not exist
                            return Ok(None);
                        }
                    }
                }
            };
        }

        Self::with_locked_cache_entries(
            &self.dirty.objects,
            &self.cached.object_cache,
            &object_id,
            |dirty_entry, cached_entry| {
                check_cache_entry!(dirty_entry);
                check_cache_entry!(cached_entry);
                self.store.find_object_lt_or_eq_version(object_id, version)
            },
        )
    }

    fn multi_get_transaction_blocks(
        &self,
        digests: &[TransactionDigest],
    ) -> SuiResult<Vec<Option<Arc<VerifiedTransaction>>>> {
        do_fallback_lookup(
            digests,
            |digest| {
                if let Some(tx) = self.dirty.pending_transaction_writes.get(digest) {
                    CacheResult::Hit(Some(tx.transaction.clone()))
                } else {
                    CacheResult::Miss
                }
            },
            |remaining| {
                self.store
                    .multi_get_transaction_blocks(remaining)
                    .map(|v| v.into_iter().map(|o| o.map(Arc::new)).collect())
            },
        )
    }

    fn multi_get_executed_effects_digests(
        &self,
        digests: &[TransactionDigest],
    ) -> SuiResult<Vec<Option<TransactionEffectsDigest>>> {
        do_fallback_lookup(
            digests,
            |digest| {
                if let Some(digest) = self.dirty.executed_effects_digests.get(digest) {
                    CacheResult::Hit(Some(*digest))
                } else {
                    CacheResult::Miss
                }
            },
            |remaining| self.store.multi_get_executed_effects_digests(remaining),
        )
    }

    fn multi_get_effects(
        &self,
        digests: &[TransactionEffectsDigest],
    ) -> SuiResult<Vec<Option<TransactionEffects>>> {
        do_fallback_lookup(
            digests,
            |digest| {
                if let Some(effects) = self.dirty.transaction_effects.get(digest) {
                    CacheResult::Hit(Some(effects.clone()))
                } else {
                    CacheResult::Miss
                }
            },
            |remaining| self.store.multi_get_effects(remaining.iter()),
        )
    }

    fn notify_read_executed_effects_digests<'a>(
        &'a self,
        digests: &'a [TransactionDigest],
    ) -> BoxFuture<'a, SuiResult<Vec<TransactionEffectsDigest>>> {
        async move {
            let registrations = self
                .executed_effects_digests_notify_read
                .register_all(digests);

            let executed_effects_digests = self.multi_get_executed_effects_digests(digests)?;

            let results = executed_effects_digests
                .into_iter()
                .zip(registrations)
                .map(|(a, r)| match a {
                    // Note that Some() clause also drops registration that is already fulfilled
                    Some(ready) => Either::Left(futures::future::ready(ready)),
                    None => Either::Right(r),
                });

            Ok(join_all(results).await)
        }
        .boxed()
    }

    fn multi_get_events(
        &self,
        event_digests: &[TransactionEventsDigest],
    ) -> SuiResult<Vec<Option<TransactionEvents>>> {
        do_fallback_lookup(
            event_digests,
            |digest| {
                if let Some(events) = self.dirty.transaction_events.get(digest) {
                    CacheResult::Hit(Some(events.1.clone()))
                } else {
                    CacheResult::Miss
                }
            },
            |digests| self.store.multi_get_events(digests),
        )
    }

    fn get_sui_system_state_object_unsafe(&self) -> SuiResult<SuiSystemState> {
        get_sui_system_state(self)
    }

    fn get_marker_value(
        &self,
        object_id: &ObjectID,
        version: SequenceNumber,
        epoch_id: EpochId,
    ) -> SuiResult<Option<MarkerValue>> {
        match self.get_marker_value_cache_only(object_id, version, epoch_id) {
            CacheResult::Hit(marker) => Ok(Some(marker)),
            CacheResult::NegativeHit => Ok(None),
            CacheResult::Miss => self.store.get_marker_value(object_id, &version, epoch_id),
        }
    }

    fn get_latest_marker(
        &self,
        object_id: &ObjectID,
        epoch_id: EpochId,
    ) -> SuiResult<Option<(SequenceNumber, MarkerValue)>> {
        match self.get_latest_marker_value_cache_only(object_id, epoch_id) {
            CacheResult::Hit((v, marker)) => Ok(Some((v, marker))),
            CacheResult::NegativeHit => {
                panic!("cannot have negative hit when getting latest marker")
            }
            CacheResult::Miss => self.store.get_latest_marker(object_id, epoch_id),
        }
    }

    fn get_lock(&self, _obj_ref: ObjectRef, _epoch_id: EpochId) -> SuiLockResult {
        todo!()
    }

    fn _get_latest_lock_for_object_id(&self, _object_id: ObjectID) -> SuiResult<ObjectRef> {
        todo!()
    }

    fn check_owned_object_locks_exist(&self, _owned_object_refs: &[ObjectRef]) -> SuiResult {
        todo!()
    }
}

impl ExecutionCacheWrite for WritebackCache {
    #[instrument(level = "trace", skip_all)]
    fn acquire_transaction_locks<'a>(
        &'a self,
        _epoch_id: EpochId,
        _owned_input_objects: &'a [ObjectRef],
        _tx_digest: TransactionDigest,
    ) -> BoxFuture<'a, SuiResult> {
        todo!()
    }

    #[instrument(level = "debug", skip_all)]
    fn write_transaction_outputs(
        &self,
        epoch_id: EpochId,
        tx_outputs: Arc<TransactionOutputs>,
    ) -> BoxFuture<'_, SuiResult> {
        async move {
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
                self.write_object_entry(id, *version, ObjectEntry::Deleted)
                    .await;
            }

            for ObjectKey(id, version) in wrapped.iter() {
                self.write_object_entry(id, *version, ObjectEntry::Wrapped)
                    .await;
            }

            // Update all markers
            for (object_key, marker_value) in markers.iter() {
                self.write_marker_value(epoch_id, object_key, *marker_value)
                    .await;
            }

            // Write children before parents to ensure that readers do not observe a parent object
            // before its most recent children are visible.
            for (object_id, object) in written.iter() {
                if object.is_child_object() {
                    self.write_object_entry(object_id, object.version(), object.clone().into())
                        .await;
                }
            }
            for (object_id, object) in written.iter() {
                if !object.is_child_object() {
                    self.write_object_entry(object_id, object.version(), object.clone().into())
                        .await;
                    if object.is_package() {
                        self.packages
                            .insert(*object_id, PackageObject::new(object.clone()));
                    }
                }
            }

            let tx_digest = *transaction.digest();
            let effects_digest = effects.digest();

            self.dirty
                .transaction_effects
                .insert(effects_digest, effects.clone());

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

            self.dirty
                .executed_effects_digests
                .insert(tx_digest, effects_digest);

            self.dirty
                .pending_transaction_writes
                .insert(tx_digest, tx_outputs);

            self.executed_effects_digests_notify_read
                .notify(&tx_digest, &effects_digest);

            self.metrics
                .pending_notify_read
                .set(self.executed_effects_digests_notify_read.num_pending() as i64);

            Ok(())
        }
        .boxed()
    }
}

/// do_fallback_lookup is a helper function for multi-get operations.
/// It takes a list of keys and first attempts to look up each key in the cache.
/// The cache can return a hit, a miss, or a negative hit (if the object is known to not exist).
/// Any keys that result in a miss are then looked up in the store.
///
/// The "get from cache" and "get from store" behavior are implemented by the caller and provided
/// via the get_cached_key and multiget_fallback functions.
fn do_fallback_lookup<K: Copy, V: Default + Clone>(
    keys: &[K],
    get_cached_key: impl Fn(&K) -> CacheResult<V>,
    multiget_fallback: impl Fn(&[K]) -> SuiResult<Vec<V>>,
) -> SuiResult<Vec<V>> {
    let mut results = vec![V::default(); keys.len()];
    let mut fallback_keys = Vec::with_capacity(keys.len());
    let mut fallback_indices = Vec::with_capacity(keys.len());

    for (i, key) in keys.iter().enumerate() {
        match get_cached_key(key) {
            CacheResult::Miss => {
                fallback_keys.push(*key);
                fallback_indices.push(i);
            }
            CacheResult::NegativeHit => (),
            CacheResult::Hit(value) => {
                results[i] = value;
            }
        }
    }

    let fallback_results = multiget_fallback(&fallback_keys)?;
    assert_eq!(fallback_results.len(), fallback_indices.len());
    assert_eq!(fallback_results.len(), fallback_keys.len());

    for (i, result) in fallback_indices
        .into_iter()
        .zip(fallback_results.into_iter())
    {
        results[i] = result;
    }
    Ok(results)
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
        self.store.iter_live_object_set(include_wrapped_tombstone)
    }
}
