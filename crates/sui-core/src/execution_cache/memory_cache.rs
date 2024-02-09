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
//! 
//! The cache is divided into two parts: dirty and cached. The dirty part contains data that has been

use crate::authority::authority_per_epoch_store::AuthorityPerEpochStore;
use crate::authority::authority_store::{
    ExecutionLockReadGuard, ExecutionLockWriteGuard, SuiLockResult,
};
use crate::authority::authority_store::{LockDetails, LockDetailsWrapper};
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
use dashmap::mapref::entry::OccupiedEntry as DashMapOccupiedEntry;
use dashmap::mapref::one::Ref as DashMapRef;
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
use std::sync::Arc;
use sui_config::node::AuthorityStorePruningConfig;
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
use typed_store::Map;

use super::{
    implement_passthrough_traits, utils::CachedVersionMap, CheckpointCache, ExecutionCacheCommit,
    ExecutionCacheMetrics, ExecutionCacheRead, ExecutionCacheReconfigAPI, ExecutionCacheWrite,
    NotifyReadWrapper, StateSyncAPI,
};

#[derive(Clone, PartialEq, Eq)]
enum ObjectEntry {
    Object(Object),
    Deleted,
    Wrapped,
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

#[derive(Clone, Debug, PartialEq, Eq)]
enum Lock {
    Lock(Option<LockDetails>),
    Deleted,
}

impl Lock {
    fn is_deleted(&self) -> bool {
        matches!(self, Lock::Deleted)
    }
}

type LockMap = DashMap<ObjectID, (ObjectRef, Lock)>;
type LockMapRef<'a> = DashMapRef<'a, ObjectID, (ObjectRef, Lock)>;
type LockMapEntry<'a> = DashMapEntry<'a, ObjectID, (ObjectRef, Lock)>;
type LockMapOccupiedEntry<'a> = DashMapOccupiedEntry<'a, ObjectID, (ObjectRef, Lock)>;

trait LocksByObjectRef {
    fn entry_by_objref(&self, obj_ref: &ObjectRef) -> LockMapEntry<'_>;
    fn get_by_objref(&self, obj_ref: &ObjectRef) -> Option<LockMapRef<'_>>;
    fn insert_by_objref(&self, obj_ref: ObjectRef, lock: Lock);
}

impl LocksByObjectRef for LockMap {
    fn entry_by_objref(&self, obj_ref: &ObjectRef) -> LockMapEntry<'_> {
        self.entry(obj_ref.0)
    }

    fn get_by_objref(&self, obj_ref: &ObjectRef) -> Option<LockMapRef<'_>> {
        if let Some(r) = self.get(&obj_ref.0) {
            assert_eq!(r.value().0, *obj_ref);
            Some(r)
        } else {
            None
        }
    }

    fn insert_by_objref(&self, obj_ref: ObjectRef, lock: Lock) {
        assert!(
            self.insert(obj_ref.0, (obj_ref, lock)).is_none(),
            "lock already existed for {:?}",
            obj_ref
        );
    }
}

/// UncommitedData stores execution outputs that are not yet written to the db. Entries in this
/// struct can only be purged after they are committed.
struct UncommittedData {
    /// The object dirty set. All writes go into this table first. After we flush the data to the
    /// db, the data is removed from this table and inserted into the object_cache.
    ///
    /// This table may contain both live and dead objects, since we flush both live and dead
    /// objects to the db in order to support past object queries on fullnodes.
    /// When we move data into the object_cache we only retain the live objects.
    ///
    /// Further, we only remove objects in FIFO order, which ensures that the the cached
    /// sequence of objects has no gaps. In other words, if we have versions 4, 8, 13 of
    /// an object, we can deduce that version 9 does not exist. This also makes child object
    /// reads efficient. `object_cache` cannot contain a more recent version of an object than
    /// `objects`, and neither can have any gaps. Therefore if there is any object <= the version
    /// bound for a child read in objects, it is the correct object to return.
    objects: DashMap<ObjectID, CachedVersionMap<ObjectEntry>>,

    // Mirrors the owned_object_transaction_locks table in the db, but with the following
    // difference: Since there cannot be locks for multiple object versions in existence
    // at the same time, this map is keyed by ObjectID. We store the full ObjectRef in the
    // value. (This is done so that we can find the lock by ObjectID).
    //
    // This map is mutated in the following situations:
    // 1. When we load a lock, if it is not in this map, we attempt to load it from the db.
    //    If it is found, it is inserted into this map.
    // 2. When we lock an object for a transaction, we change the entry in this map from
    //    None to Some(LockDetails(..)). In this situation we also write the lock to the db.
    // 3. In write_transaction_outputs:
    //    - If a lock is deleted, and a lock for the next version of the same object is created,
    //      we update the entry in this map to reflect the new objref and set the lock to None
    //    - If a lock is deleted (and no new lock is created), we set the entry to Lock::Deleted.
    //      This is necessary so that readers cannot observe the deleted lock in undeleted state
    //      via a cache miss.
    //    - If a lock is created (and no prior version existed), we insert the lock into this map.
    //      In this situation, there will be no record of the lock in the db until commit_transaction_outputs
    //      is called.
    //
    // NB: Suppose a lock is created, inserted into the cache, and then locked to a new transaction.
    // The previous version of the lock may still exist on disk if commit_transaction_outputs has not
    // run. To lock the transaction we immediately write the new LockDetails to the db. Now the db has
    // locks for multiple versions of the object, a situation that is normally impossible!
    // To deal with this, we should actually use a log to persist locked transactions rather than eagerly
    // writing to the db. This will ensure that while the db can only be behind the cache, it cannot
    // contain illegal states.
    owned_object_transaction_locks: LockMap,

    // Markers for received objects and deleted shared objects. This contains all of the dirty
    // marker state, which is committed to the db at the same time as other transaction data.
    // After markers are committed to the db we remove them from this table and insert them into
    // marker_cache.
    markers: DashMap<MarkerKey, CachedVersionMap<MarkerValue>>,

    transaction_effects: DashMap<TransactionEffectsDigest, TransactionEffects>,

    transaction_events: DashMap<TransactionEventsDigest, TransactionEvents>,

    executed_effects_digests: DashMap<TransactionDigest, TransactionEffectsDigest>,

    // Transaction outputs that have not yet been written to the DB. Items are removed from this
    // table as they are flushed to the db.
    pending_transaction_writes: DashMap<TransactionDigest, Arc<TransactionOutputs>>,
}

impl UncommittedData {
    fn new() -> Self {
        Self {
            objects: DashMap::new(),
            owned_object_transaction_locks: DashMap::new(),
            markers: DashMap::new(),
            transaction_effects: DashMap::new(),
            executed_effects_digests: DashMap::new(),
            pending_transaction_writes: DashMap::new(),
            transaction_events: DashMap::new(),
        }
    }
}

/// CachedData stores data that has been committed to the db, but is likely to be read soon.
struct CachedData {
    /// Contains live, non-package objects that have been committed to the db.
    /// As with `objects`, we remove objects from this table in FIFO order (or we allow the cache
    /// to evict all versions of the object at once), which ensures that the the cached sequence
    /// of objects has no gaps. See the comment above for more details.
    // TODO(cache): this is not populated yet, we will populate it when we implement flushing.
    object_cache: MokaCache<ObjectID, Arc<Mutex<CachedVersionMap<ObjectEntry>>>>,

    // Packages are cached separately from objects because they are immutable and can be used by any
    // number of transactions. Additionally, many operations require loading large numbers of packages
    // (due to dependencies), so we want to try to keep all packages in memory.
    // Note that, like any other dirty object, all packages are also stored in `objects` until they are
    // flushed to disk.
    packages: MokaCache<ObjectID, PackageObject>,

    // Because markers (e.g. received markers) can be read by many transactions, we also cache
    // them. Markers are added to this cache in two ways:
    // 1. When they are committed to the db and removed from the `markers` table.
    // 2. After a cache miss in which we retrieve the marker from the db.

    // Note that MokaCache can only return items by value, so we store the map as an Arc<Mutex>.
    // (There should be no contention on the inner mutex, it is used only for interior mutability.)
    marker_cache: MokaCache<MarkerKey, Arc<Mutex<CachedVersionMap<MarkerValue>>>>,

    // Objects that were read at transaction signing time - allows us to access them again at
    // execution time with a single lock / hash lookup
    _transaction_objects: MokaCache<TransactionDigest, Vec<Object>>,
}

impl CachedData {
    fn new() -> Self {
        let object_cache = MokaCache::builder()
            .max_capacity(10000)
            .initial_capacity(10000)
            .build();
        let packages = MokaCache::builder()
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
            packages,
            marker_cache,
            _transaction_objects: transaction_objects,
        }
    }
}
pub struct MemoryCache {
    dirty: UncommittedData,
    cached: CachedData,

    executed_effects_digests_notify_read: NotifyRead<TransactionDigest, TransactionEffectsDigest>,
    store: Arc<AuthorityStore>,
    metrics: Option<ExecutionCacheMetrics>,
}

impl MemoryCache {
    pub fn new(store: Arc<AuthorityStore>, registry: &Registry) -> Self {
        Self {
            dirty: UncommittedData::new(),
            cached: CachedData::new(),
            executed_effects_digests_notify_read: NotifyRead::new(),
            store,
            metrics: Some(ExecutionCacheMetrics::new(registry)),
        }
    }

    pub fn new_with_no_metrics(store: Arc<AuthorityStore>) -> Self {
        Self {
            dirty: UncommittedData::new(),
            cached: CachedData::new(),
            executed_effects_digests_notify_read: NotifyRead::new(),
            store,
            metrics: None,
        }
    }

    // Insert a new object in the dirty state. The object will not be persisted to disk.
    fn write_object(&self, object_id: &ObjectID, object: &Object) {
        let version = object.version();
        tracing::debug!("inserting object {:?}: {:?}", object_id, version);
        self.dirty
            .objects
            .entry(*object_id)
            .or_default()
            .insert(object.version(), object.clone().into());
    }

    // Insert a deleted tombstone in the dirty state. The tombstone will not be persisted to disk.
    fn write_deleted_tombstone(&self, object_id: &ObjectID, version: SequenceNumber) {
        tracing::debug!("inserting deleted tombstone {:?}: {:?}", object_id, version);
        self.dirty
            .objects
            .entry(*object_id)
            .or_default()
            .insert(version, ObjectEntry::Deleted);
    }

    // Insert a wrapped tombstone in the dirty state. The tombstone will not be persisted to disk.
    fn write_wrapped_tombstone(&self, object_id: &ObjectID, version: SequenceNumber) {
        tracing::debug!("inserting wrapped tombstone {:?}: {:?}", object_id, version);
        self.dirty
            .objects
            .entry(*object_id)
            .or_default()
            .insert(version, ObjectEntry::Wrapped);
    }

    // Attempt to get an object from the cache. The DB is not consulted.
    // Can return Hit, Miss, or NegativeHit (if the object is known to not exist).
    fn get_object_by_key_cache_only(
        &self,
        object_id: &ObjectID,
        version: SequenceNumber,
    ) -> CacheResult<Object> {
        macro_rules! check_cache_entry {
            ($objects: expr) => {
                if let Some(object) = $objects.get(&version) {
                    if let ObjectEntry::Object(object) = object {
                        return CacheResult::Hit(object.clone());
                    } else {
                        // object exists but is a tombstone
                        return CacheResult::NegativeHit;
                    }
                }

                if $objects.get_last().0 < version {
                    // If the version is greater than the last version in the cache, then we know
                    // that the object does not exist anywhere
                    return CacheResult::NegativeHit;
                }
            };
        }

        if let Some(objects) = self.dirty.objects.get(object_id) {
            check_cache_entry!(objects);
        }

        if let Some(objects) = self.cached.object_cache.get(object_id) {
            let objects = objects.lock();
            check_cache_entry!(objects);
        }

        CacheResult::Miss
    }

    // Load a lock entry from the cache, populating it from the db if the cache
    // does not have the entry. The cache may be missing entries, but it can
    // never be incoherent (hold an entry that doesn't exist in the db, or that
    // differs from the db) because all db writes for locks go through the cache.
    //
    // Furthermore, if the cache holds a lock, it must be the most recent lock
    // (because all writes go through the cache). Since the cache only holds one
    // lock, we check that the objref matches. If it does not, we return an error.
    fn get_owned_object_lock_entry_or_error(
        &self,
        obj_ref: &ObjectRef,
    ) -> SuiResult<LockMapOccupiedEntry<'_>> {
        let entry = self
            .dirty
            .owned_object_transaction_locks
            .entry_by_objref(obj_ref);
        let occupied = match entry {
            DashMapEntry::Occupied(occupied) => {
                if cfg!(debug_assertions) {
                    if let (Ok(db_lock), (_, Lock::Lock(cached_lock))) =
                        (self.store.get_lock_entry(*obj_ref), occupied.get())
                    {
                        assert_eq!(
                            *cached_lock, db_lock,
                            "cache is incoherent for object ref {:?}",
                            obj_ref
                        );
                    }
                }

                // If the lock is deleted, or is for a different objref, we return an error
                let value = occupied.get();
                if value.1.is_deleted() || value.0 != *obj_ref {
                    return Err(SuiError::UserInputError {
                        error: UserInputError::ObjectNotFound {
                            object_id: obj_ref.0,
                            version: Some(obj_ref.1),
                        },
                    });
                }

                occupied
            }
            DashMapEntry::Vacant(entry) => {
                let lock = self.store.get_lock_entry(*obj_ref)?;
                entry.insert_entry((*obj_ref, Lock::Lock(lock)))
            }
        };

        assert_eq!(occupied.get().0, *obj_ref);
        Ok(occupied)
    }

    async fn acquire_transaction_locks(
        &self,
        execution_lock: &ExecutionLockReadGuard<'_>,
        owned_input_objects: &[ObjectRef],
        tx_digest: TransactionDigest,
    ) -> SuiResult {
        let epoch = **execution_lock;
        let mut ret = Ok(());

        let mut locks_to_write: Vec<(_, Option<LockDetailsWrapper>)> =
            Vec::with_capacity(owned_input_objects.len());
        let mut previous_values = Vec::with_capacity(owned_input_objects.len());

        // Note that this function does not have to operate atomically. If there are two racing threads,
        // then they are either trying to lock the same transaction (in which case both will succeed),
        // or they are trying to lock the same object in two different transactions, in which case
        // the sender has equivocated, and we are under no obligation to help them form a cert.
        for obj_ref in owned_input_objects.iter() {
            // entry holds a lock in the dashmap shard which allows us to safely test and set here.
            let mut entry = match self.get_owned_object_lock_entry_or_error(obj_ref) {
                Ok(entry) => entry,
                Err(e) => {
                    ret = Err(e);
                    break;
                }
            };

            let previous_value = entry.get().clone();

            match previous_value.1 {
                Lock::Deleted => {
                    unreachable!("get_owned_object_lock_entry_or_error checks for deleted")
                }

                // Lock exists, but is not set, so we can overwrite it
                Lock::Lock(None) => (),

                // Lock is set. Check for equivocation and expiry due to the lock
                // being set in a previous epoch.
                Lock::Lock(Some(LockDetails {
                    epoch: previous_epoch,
                    tx_digest: previous_tx_digest,
                })) => {
                    // this should not be possible because we hold the execution lock
                    assert!(
                        epoch >= previous_epoch,
                        "epoch changed while acquiring locks"
                    );

                    let same_epoch = epoch == previous_epoch;
                    let same_tx = tx_digest == previous_tx_digest;

                    // If the lock is set in a previous epoch, it's ok to override it.
                    if same_epoch && same_tx {
                        continue;
                    } else if same_epoch && !same_tx {
                        // Error: lock already set to different transaction from the same epoch.
                        // TODO: add metrics here
                        info!(prev_tx_digest = ?previous_tx_digest,
                            cur_tx_digest = ?tx_digest,
                            "Cannot acquire lock: conflicting transaction!");
                        ret = Err(SuiError::ObjectLockConflict {
                            obj_ref: *obj_ref,
                            pending_transaction: previous_tx_digest,
                        });
                        break;
                    } else {
                        info!(prev_epoch =? previous_epoch, cur_epoch =? epoch, "Overriding an old lock from previous epoch");
                        // Fall through and override the old lock.
                    }
                }
            }

            let lock_details = LockDetails { epoch, tx_digest };
            previous_values.push(previous_value.clone());
            locks_to_write.push((*obj_ref, Some(lock_details.clone().into())));
            entry.get_mut().1 = Lock::Lock(Some(lock_details));
        }

        if ret.is_ok() {
            // commit all writes to DB
            self.store.write_locks(&locks_to_write)?;
        } else {
            // revert all writes and return error
            // Note that reverting is not required for liveness, since a well formed and un-equivocating
            // txn cannot fail to acquire locks.
            // However, a user may inadvertently sign a txn that tries to use an old object. If they do this,
            // they will not be able to obtain a lock, but we'd like to unlock the other objects in the
            // transaction so they can correct the error.
            assert_eq!(locks_to_write.len(), previous_values.len());
            for ((obj_ref, new_value), previous_value) in
                locks_to_write.into_iter().zip(previous_values.into_iter())
            {
                let DashMapEntry::Occupied(mut entry) = self
                    .dirty
                    .owned_object_transaction_locks
                    .entry_by_objref(&obj_ref)
                else {
                    panic!("entry was just populated, cannot be vacant");
                };

                let value = entry.get();

                // it is impossible for any other thread to modify the lock value after we have
                // written it. This is because the only case in which we overwrite a lock is when
                // the epoch has changed, but because we are holding ExecutionLockReadGuard, the
                // epoch cannot change within this function.
                assert_eq!(
                    value.0, obj_ref,
                    "entry was just populated, cannot be for different obj ref"
                );
                assert_eq!(
                    value.1,
                    Lock::Lock(new_value.map(|l| l.clone().migrate().into_inner())),
                    "lock for {:?} was modified by another thread (should be impossible)",
                    obj_ref
                );
                *entry.get_mut() = previous_value;
            }
        }

        ret
    }

    // Commits dirty data for the given TransactionDigest to the db.
    async fn commit_transaction_outputs(
        &self,
        epoch: EpochId,
        digest: TransactionDigest,
    ) -> SuiResult {
        let Some((_, outputs)) = self.dirty.pending_transaction_writes.remove(&digest) else {
            return Err(SuiError::TransactionNotFound { digest });
        };

        // Flush writes to disk
        self.store
            .write_transaction_outputs(epoch, outputs.clone())
            .await?;

        static MAX_VERSIONS: usize = 3;

        // Now, remove each piece of committed data from the dirty state and insert it into the cache.
        // TODO: outputs should have a strong count of 1 so we should be able to move out of it
        let TransactionOutputs {
            transaction,
            effects,
            markers,
            written,
            deleted,
            wrapped,
            locks_to_delete,
            new_locks_to_init,
            events,
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

        // process deleted and new locks
        let tx_digest = *transaction.digest();
        let effects_digest = effects.digest();

        self.dirty
            .transaction_effects
            .insert(effects_digest, effects.clone());

        self.dirty
            .transaction_events
            .insert(events.digest(), events.clone());

        self.dirty
            .executed_effects_digests
            .insert(tx_digest, effects_digest);

        self.executed_effects_digests_notify_read
            .notify(&tx_digest, &effects_digest);

        Ok(())
    }

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
        //let key = (epoch, object_key.0);
        //let version = object_key.1;

        // IMPORTANT: lock both the dirty set entry and the cache entry before modifying either
        // this ensures that readers cannot see a value temporarily disappear.
        let cache_entry = cache.entry(key).or_default();
        let mut cache_map = cache_entry.value().lock();
        let dirty_entry = dirty.entry(key);

        // insert into cache and drop old versions.
        cache_map.insert(version, value.clone());
        // TODO: make this automatic by giving CachedVersionMap an optional max capacity
        cache_map.truncate(MAX_VERSIONS);

        let DashMapEntry::Occupied(mut occupied_dirty_entry) = dirty_entry else {
            panic!("dirty map must exist");
        };

        let removed = occupied_dirty_entry.get_mut().remove(&version);

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
}

impl ExecutionCacheCommit for MemoryCache {
    fn commit_transaction_outputs(
        &self,
        epoch: EpochId,
        digest: &TransactionDigest,
    ) -> BoxFuture<'_, SuiResult> {
        MemoryCache::commit_transaction_outputs(self, epoch, *digest).boxed()
    }
}

impl ExecutionCacheRead for MemoryCache {
    fn get_package_object(&self, package_id: &ObjectID) -> SuiResult<Option<PackageObject>> {
        if let Some(p) = self.cached.packages.get(package_id) {
            #[cfg(debug_assertions)]
            {
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
                self.cached.packages.insert(*package_id, p.clone());
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

    fn force_reload_system_packages(&self, system_package_ids: &[ObjectID]) {
        for package_id in system_package_ids {
            if let Some(p) = self
                .store
                .get_object(package_id)
                .expect("Failed to update system packages")
            {
                assert!(p.is_package());
                self.cached
                    .packages
                    .insert(*package_id, PackageObject::new(p));
            }
            // It's possible that a package is not found if it's newly added system package ID
            // that hasn't got created yet. This should be very very rare though.
        }
    }

    fn get_object(&self, id: &ObjectID) -> SuiResult<Option<Object>> {
        if let Some(objects) = self.dirty.objects.get(id) {
            // If any version of the object is in the cache, it must be the most recent version.
            return match &objects.get_last().1 {
                ObjectEntry::Object(object) => Ok(Some(object.clone())),
                _ => Ok(None), // tombstone
            };
        }

        if let Some(objects) = self.cached.object_cache.get(id) {
            let objects = objects.lock();
            // If any version of the object is in the cache, it must be the most recent version.
            return match &objects.get_last().1 {
                ObjectEntry::Object(object) => Ok(Some(object.clone())),
                _ => Ok(None), // tombstone
            };
        }

        // We don't insert objects into the cache because they are usually only
        // read once.
        // TODO: we might want to cache immutable reads (RO shared objects and immutable objects)
        self.store.get_object(id).map_err(Into::into)
    }

    fn get_object_by_key(
        &self,
        object_id: &ObjectID,
        version: SequenceNumber,
    ) -> SuiResult<Option<Object>> {
        match self.get_object_by_key_cache_only(object_id, version) {
            CacheResult::Hit(object) => Ok(Some(object)),
            CacheResult::NegativeHit => Ok(None),
            // We don't insert objects into the cache after a miss because they are usually only
            // read once.
            CacheResult::Miss => Ok(self.store.get_object_by_key(object_id, version)?),
        }
    }

    fn multi_get_objects_by_key(
        &self,
        object_keys: &[ObjectKey],
    ) -> Result<Vec<Option<Object>>, SuiError> {
        let mut results = vec![None; object_keys.len()];
        let mut fallback_keys = Vec::with_capacity(object_keys.len());
        let mut fetch_indices = Vec::with_capacity(object_keys.len());

        for (i, key) in object_keys.iter().enumerate() {
            match self.get_object_by_key_cache_only(&key.0, key.1) {
                CacheResult::Hit(object) => results[i] = Some(object),
                CacheResult::NegativeHit => (),
                CacheResult::Miss => {
                    fallback_keys.push(*key);
                    fetch_indices.push(i);
                }
            }
        }

        let store_results = self.store.multi_get_objects_by_key(&fallback_keys)?;
        assert_eq!(store_results.len(), fetch_indices.len());
        assert_eq!(store_results.len(), fallback_keys.len());

        for (i, result) in fetch_indices.into_iter().zip(store_results.into_iter()) {
            results[i] = result;
        }

        Ok(results)
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
        let mut results = vec![false; object_keys.len()];
        let mut fallback_keys = Vec::with_capacity(object_keys.len());
        let mut fetch_indices = Vec::with_capacity(object_keys.len());

        for (i, key) in object_keys.iter().enumerate() {
            match self.get_object_by_key_cache_only(&key.0, key.1) {
                CacheResult::Hit(_) => results[i] = true,
                CacheResult::NegativeHit => (),
                CacheResult::Miss => {
                    fallback_keys.push(*key);
                    fetch_indices.push(i);
                }
            }
        }

        let store_results = self.store.multi_object_exists_by_key(&fallback_keys)?;
        assert_eq!(store_results.len(), fetch_indices.len());
        assert_eq!(store_results.len(), fallback_keys.len());

        for (i, result) in fetch_indices.into_iter().zip(store_results.into_iter()) {
            results[i] = result;
        }

        Ok(results)
    }

    fn get_latest_object_ref_or_tombstone(
        &self,
        object_id: ObjectID,
    ) -> SuiResult<Option<ObjectRef>> {
        if let Some(objects) = self.dirty.objects.get(&object_id) {
            let (version, object) = objects.get_last();
            let objref = match object {
                ObjectEntry::Object(object) => object.compute_object_reference(),
                ObjectEntry::Deleted => (object_id, *version, ObjectDigest::OBJECT_DIGEST_DELETED),
                ObjectEntry::Wrapped => (object_id, *version, ObjectDigest::OBJECT_DIGEST_WRAPPED),
            };
            return Ok(Some(objref));
        }

        self.store.get_latest_object_ref_or_tombstone(object_id)
    }

    fn get_latest_object_or_tombstone(
        &self,
        object_id: ObjectID,
    ) -> Result<Option<(ObjectKey, ObjectOrTombstone)>, SuiError> {
        if let Some(objref) = self.get_latest_object_ref_or_tombstone(object_id)? {
            if !objref.2.is_alive() {
                return Ok(Some((objref.into(), ObjectOrTombstone::Tombstone(objref))));
            } else {
                let key: ObjectKey = objref.into();
                let object = ExecutionCacheRead::get_object_by_key(self, &objref.0, objref.1)?;
                return Ok(object.map(|o| (key, o.into())));
            }
        }

        self.store.get_latest_object_or_tombstone(object_id)
    }

    fn find_object_lt_or_eq_version(
        &self,
        object_id: ObjectID,
        version: SequenceNumber,
    ) -> SuiResult<Option<Object>> {
        // Both self.dirty.objects and self.cached.object_cache have no gaps,
        // and self.object_cache cannot have a more recent version than self.objects.
        // note that while binary searching would be more efficient for random keys, child
        // reads will be disproportionately more likely o be for a very recent version.
        if let Some(objects) = self.dirty.objects.get(&object_id) {
            if let Some((_, object)) = objects.all_lt_or_eq_rev(&version).next() {
                if let ObjectEntry::Object(object) = object {
                    return Ok(Some(object.clone()));
                } else {
                    // if we find a tombstone, the object does not exist
                    return Ok(None);
                }
            }
        }

        if let Some(objects) = self.cached.object_cache.get(&object_id) {
            let objects = objects.lock();
            if let Some((_, object)) = objects.all_lt_or_eq_rev(&version).next() {
                if let ObjectEntry::Object(object) = object {
                    return Ok(Some(object.clone()));
                } else {
                    // if we find a tombstone, the object does not exist
                    return Ok(None);
                }
            };
        }

        self.store.find_object_lt_or_eq_version(object_id, version)
    }

    fn get_lock(&self, obj_ref: ObjectRef, epoch_id: EpochId) -> SuiLockResult {
        self.store.get_lock(obj_ref, epoch_id)
    }

    fn get_latest_lock_for_object_id(&self, object_id: ObjectID) -> SuiResult<ObjectRef> {
        // TODO(cache) - read lock from cache
        let lock = self
            .store
            .get_latest_lock_for_object_id(object_id)
            .expect("read cannot fail");
        Ok(lock)
    }

    fn check_owned_object_locks_exist(&self, objects: &[ObjectRef]) -> SuiResult {
        let mut fallback_objects = Vec::with_capacity(objects.len());

        for obj_ref in objects {
            match self
                .dirty
                .owned_object_transaction_locks
                .get_by_objref(obj_ref)
                .as_ref()
                .map(|r| &r.value().1)
            {
                Some(Lock::Deleted) => {
                    let lock = self.get_latest_lock_for_object_id(obj_ref.0)?;
                    return Err(SuiError::UserInputError {
                        error: UserInputError::ObjectVersionUnavailableForConsumption {
                            provided_obj_ref: *obj_ref,
                            current_version: lock.1,
                        },
                    });
                }
                Some(Lock::Lock(_)) => (),
                None => fallback_objects.push(*obj_ref),
            }
        }

        self.store.check_owned_object_locks_exist(&fallback_objects)
    }

    fn multi_get_transaction_blocks(
        &self,
        digests: &[TransactionDigest],
    ) -> SuiResult<Vec<Option<Arc<VerifiedTransaction>>>> {
        let mut results = vec![None; digests.len()];
        let mut fetch_indices = Vec::with_capacity(digests.len());
        let mut fetch_digests = Vec::with_capacity(digests.len());

        for (i, digest) in digests.iter().enumerate() {
            if let Some(tx) = self.dirty.pending_transaction_writes.get(digest) {
                results[i] = Some(tx.transaction.clone());
            } else {
                fetch_indices.push(i);
                fetch_digests.push(*digest);
            }
        }

        let multiget_results = self.store.multi_get_transaction_blocks(&fetch_digests)?;
        assert_eq!(multiget_results.len(), fetch_indices.len());
        assert_eq!(multiget_results.len(), fetch_digests.len());

        for (i, result) in fetch_indices.into_iter().zip(multiget_results.into_iter()) {
            results[i] = result.map(Arc::new);
        }

        Ok(results)
    }

    fn multi_get_executed_effects_digests(
        &self,
        digests: &[TransactionDigest],
    ) -> SuiResult<Vec<Option<TransactionEffectsDigest>>> {
        let mut results = vec![None; digests.len()];
        let mut fetch_indices = Vec::with_capacity(digests.len());
        let mut fetch_digests = Vec::with_capacity(digests.len());

        for (i, digest) in digests.iter().enumerate() {
            if let Some(digest) = self.dirty.executed_effects_digests.get(digest) {
                results[i] = Some(*digest);
            } else {
                fetch_indices.push(i);
                fetch_digests.push(*digest);
            }
        }

        let multiget_results = self
            .store
            .multi_get_executed_effects_digests(&fetch_digests)?;
        assert_eq!(multiget_results.len(), fetch_indices.len());
        assert_eq!(multiget_results.len(), fetch_digests.len());

        for (i, result) in fetch_indices.into_iter().zip(multiget_results.into_iter()) {
            results[i] = result;
        }

        Ok(results)
    }

    fn multi_get_effects(
        &self,
        digests: &[TransactionEffectsDigest],
    ) -> SuiResult<Vec<Option<TransactionEffects>>> {
        let mut results = vec![None; digests.len()];
        let mut fetch_indices = Vec::with_capacity(digests.len());
        let mut fetch_digests = Vec::with_capacity(digests.len());

        for (i, digest) in digests.iter().enumerate() {
            if let Some(effects) = self.dirty.transaction_effects.get(digest) {
                results[i] = Some(effects.clone());
            } else {
                fetch_indices.push(i);
                fetch_digests.push(*digest);
            }
        }

        let fetch_results = self.store.perpetual_tables.effects.multi_get(digests)?;
        for (i, result) in fetch_indices.into_iter().zip(fetch_results.into_iter()) {
            results[i] = result;
        }

        Ok(results)
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
                    CacheResult::Hit(events.clone())
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
        version: &SequenceNumber,
        epoch_id: EpochId,
    ) -> SuiResult<Option<MarkerValue>> {
        // first check the dirty markers
        if let Some(markers) = self.dirty.markers.get(&(epoch_id, *object_id)) {
            if let Some(marker) = markers.get(version) {
                return Ok(Some(*marker));
            }
        }

        // now check the cache
        if let Some(markers) = self.cached.marker_cache.get(&(epoch_id, *object_id)) {
            if let Some(marker) = markers.lock().get(version) {
                return Ok(Some(*marker));
            }
        }

        // fall back to the db
        // NOTE: we cannot insert this marker into the cache, because the cache
        // must always contain the latest marker version if it contains any marker
        // for an object.
        self.store.get_marker_value(object_id, version, epoch_id)
    }

    fn get_latest_marker(
        &self,
        object_id: &ObjectID,
        epoch_id: EpochId,
    ) -> SuiResult<Option<(SequenceNumber, MarkerValue)>> {
        // Note: the reads from the dirty set and the cache are both safe, because both
        // of these structures are guaranteed to contain the latest marker if they have
        // an entry at all for a given object.

        if let Some(markers) = self.dirty.markers.get(&(epoch_id, *object_id)) {
            let (k, v) = markers.get_last();
            return Ok(Some((*k, *v)));
        }

        if let Some(markers) = self.cached.marker_cache.get(&(epoch_id, *object_id)) {
            let markers = markers.lock();
            let (k, v) = markers.get_last();
            return Ok(Some((*k, *v)));
        }

        // TODO: we could insert this marker into the cache since it is the latest
        self.store.get_latest_marker(object_id, epoch_id)
    }
}

impl ExecutionCacheWrite for MemoryCache {
    #[instrument(level = "trace", skip_all)]
    fn acquire_transaction_locks<'a>(
        &'a self,
        execution_lock: &'a ExecutionLockReadGuard<'a>,
        owned_input_objects: &'a [ObjectRef],
        tx_digest: TransactionDigest,
    ) -> BoxFuture<'a, SuiResult> {
        MemoryCache::acquire_transaction_locks(self, execution_lock, owned_input_objects, tx_digest)
            .boxed()
    }

    #[instrument(level = "debug", skip_all)]
    fn write_transaction_outputs(
        &self,
        epoch_id: EpochId,
        tx_outputs: Arc<TransactionOutputs>,
    ) -> BoxFuture<'_, SuiResult> {
        let TransactionOutputs {
            transaction,
            effects,
            markers,
            written,
            deleted,
            wrapped,
            locks_to_delete,
            new_locks_to_init,
            events,
        } = &*tx_outputs;

        // Update all markers
        for (object_key, marker_value) in markers.iter() {
            self.dirty
                .markers
                .entry((epoch_id, object_key.0))
                .or_default()
                .value_mut()
                .insert(object_key.1, *marker_value);
        }

        // Write children before parents to ensure that readers do not observe a parent object
        // before its most recent children are visible.
        for (object_id, object) in written.iter() {
            if object.is_child_object() {
                self.write_object(object_id, object);
            }
        }
        for (object_id, object) in written.iter() {
            if !object.is_child_object() {
                self.write_object(object_id, object);
                if object.is_package() {
                    self.cached
                        .packages
                        .insert(*object_id, PackageObject::new(object.clone()));
                }
            }
        }

        for ObjectKey(id, version) in deleted.iter() {
            self.write_deleted_tombstone(id, *version);
        }
        for ObjectKey(id, version) in wrapped.iter() {
            self.write_wrapped_tombstone(id, *version);
        }

        // Create / update / delete locks
        //
        // Note that deleted locks must be marked with Lock::Deleted instead of being removed
        // from the map, because we have not yet persisted any writes to the db. The
        // Lock::Deleted entry is necessary to prevent a subsequent query from finding the
        // not-yet-deleted db entry.
        let deleted_locks_iter: AssertOrdered<_> = locks_to_delete.iter().into();
        let new_locks_iter: AssertOrdered<_> = new_locks_to_init.iter().into();
        let mut deleted_locks_iter = deleted_locks_iter.peekable();
        let mut new_locks_iter = new_locks_iter.peekable();

        loop {
            match (deleted_locks_iter.peek(), new_locks_iter.peek()) {
                (None, None) => break,

                (Some(to_delete), Some(to_create)) => {
                    if to_delete.0 == to_create.0 {
                        // object was mutated, update the lock
                        *self
                            .get_owned_object_lock_entry_or_error(to_delete)
                            .expect("lock must exist")
                            .get_mut() = (**to_create, Lock::Lock(None));

                        deleted_locks_iter.next().unwrap();
                        new_locks_iter.next().unwrap();
                    } else if to_delete.0 < to_create.0 {
                        self.get_owned_object_lock_entry_or_error(to_delete)
                            .expect("lock must exist")
                            .get_mut()
                            .1 = Lock::Deleted;
                        deleted_locks_iter.next().unwrap();
                    } else if to_delete.0 > to_create.0 {
                        self.dirty
                            .owned_object_transaction_locks
                            .insert_by_objref(**to_create, Lock::Lock(None));
                        new_locks_iter.next().unwrap();
                    }
                }

                (Some(to_delete), None) => {
                    self.get_owned_object_lock_entry_or_error(to_delete)
                        .expect("lock must exist")
                        .get_mut()
                        .1 = Lock::Deleted;
                    deleted_locks_iter.next().unwrap();
                }

                (None, Some(to_create)) => {
                    self.dirty
                        .owned_object_transaction_locks
                        .insert_by_objref(**to_create, Lock::Lock(None));
                    new_locks_iter.next().unwrap();
                }
            }
        }

        let tx_digest = *transaction.digest();
        let effects_digest = effects.digest();

        self.dirty
            .transaction_effects
            .insert(effects_digest, effects.clone());

        self.dirty
            .transaction_events
            .insert(events.digest(), events.clone());

        self.dirty
            .executed_effects_digests
            .insert(tx_digest, effects_digest);

        self.dirty
            .pending_transaction_writes
            .insert(tx_digest, tx_outputs);

        self.executed_effects_digests_notify_read
            .notify(&tx_digest, &effects_digest);

        if let Some(metrics) = &self.metrics {
            metrics
                .pending_notify_read
                .set(self.executed_effects_digests_notify_read.num_pending() as i64);
        }

        std::future::ready(Ok(())).boxed()
    }
}

fn do_fallback_lookup<K: Copy, V>(
    keys: &[K],
    get_cached_key: impl Fn(&K) -> CacheResult<V>,
    multiget_fallback: impl Fn(&[K]) -> SuiResult<Vec<Option<V>>>,
) -> SuiResult<Vec<Option<V>>> {
    //let mut results = vec![None; keys.len()];
    let mut results = Vec::with_capacity(keys.len());
    for elt in results.iter_mut() {
        *elt = None;
    }
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
                results[i] = Some(value);
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

implement_passthrough_traits!(MemoryCache);

impl AccumulatorStore for MemoryCache {
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
        self.store.iter_live_object_set(include_wrapped_tombstone)
    }
}

// an iterator adapter that asserts that the wrapped iterator yields elements in order
struct AssertOrdered<I: Iterator> {
    iter: I,
    last: Option<I::Item>,
}

impl<I: Iterator> AssertOrdered<I> {
    fn new(iter: I) -> Self {
        Self { iter, last: None }
    }
}

impl<I: Iterator> From<I> for AssertOrdered<I> {
    fn from(iter: I) -> Self {
        Self::new(iter)
    }
}

impl<I: Iterator> Iterator for AssertOrdered<I>
where
    I::Item: Ord + Copy,
{
    type Item = I::Item;

    fn next(&mut self) -> Option<Self::Item> {
        let next = self.iter.next();
        if let Some(next) = next {
            if let Some(last) = &self.last {
                assert!(*last < next, "iterator must yield elements in order");
            }
            self.last = Some(next);
        }
        next
    }
}
