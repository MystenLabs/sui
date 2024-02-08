// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

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
use std::cmp::Ordering;
use std::collections::VecDeque;
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
    implement_passthrough_traits, CheckpointCache, ExecutionCacheCommit, ExecutionCacheMetrics,
    ExecutionCacheRead, ExecutionCacheReconfigAPI, ExecutionCacheWrite, NotifyReadWrapper,
    StateSyncAPI,
};

#[derive(Clone)]
enum ObjectEntry {
    Object(Object),
    Deleted,
    Wrapped,
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
enum LockEntry {
    Lock(Option<LockDetails>),
    Deleted,
}

type LockMap = DashMap<ObjectID, (ObjectRef, LockEntry)>;
type LockMapEntry<'a> = DashMapEntry<'a, ObjectID, (ObjectRef, LockEntry)>;
type LockMapOccupiedEntry<'a> = DashMapOccupiedEntry<'a, ObjectID, (ObjectRef, LockEntry)>;

trait LocksByObjectRef {
    fn entry_by_objref(&self, obj_ref: &ObjectRef) -> LockMapEntry<'_>;
    fn get_by_objref(&self, obj_ref: &ObjectRef) -> Option<&LockEntry>;
    fn insert_by_objref(&self, obj_ref: ObjectRef, lock: LockEntry);
}

impl LocksByObjectRef for LockMap {
    fn entry_by_objref(&self, obj_ref: &ObjectRef) -> LockMapEntry<'_> {
        let entry = self.entry(obj_ref.0);
        if let DashMapEntry::Occupied(e) = entry {
            assert_eq!(e.get().0, *obj_ref);
        }
        entry
    }

    fn get_by_objref(&self, obj_ref: &ObjectRef) -> Option<&LockEntry> {
        if let Some((cur_objref, lock)) = self.get(&obj_ref.0).as_ref().map(|r| &**r) {
            assert_eq!(*cur_objref, *obj_ref);
            Some(lock)
        } else {
            None
        }
    }

    fn insert_by_objref(&self, obj_ref: ObjectRef, lock: LockEntry) {
        if let Some(old_value) = self.insert(obj_ref.0, (obj_ref, lock)) {
            assert_eq!(
                old_value.0, obj_ref,
                "lock already existed for {:?}",
                old_value
            );
        }
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
    // difference: Since only one version of an object can ever be locked at a time, this
    // map is keyed by ObjectID. We store the full ObjectRef in the value.
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

    fn insert_object(&self, object_id: &ObjectID, object: &Object) {
        let version = object.version();
        tracing::debug!("inserting object {:?}: {:?}", object_id, version);
        self.dirty
            .objects
            .entry(*object_id)
            .or_default()
            .insert(object.version(), object.clone().into());
    }

    fn insert_deleted_tombstone(&self, object_id: &ObjectID, version: SequenceNumber) {
        tracing::debug!("inserting deleted tombstone {:?}: {:?}", object_id, version);
        self.dirty
            .objects
            .entry(*object_id)
            .or_default()
            .insert(version, ObjectEntry::Deleted);
    }

    fn insert_wrapped_tombstone(&self, object_id: &ObjectID, version: SequenceNumber) {
        tracing::debug!("inserting wrapped tombstone {:?}: {:?}", object_id, version);
        self.dirty
            .objects
            .entry(*object_id)
            .or_default()
            .insert(version, ObjectEntry::Wrapped);
    }

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

    pub fn store_for_testing(&self) -> &Arc<AuthorityStore> {
        &self.store
    }

    pub fn as_notify_read_wrapper(self: Arc<Self>) -> NotifyReadWrapper<Self> {
        NotifyReadWrapper(self)
    }

    // Load a lock entry from the cache, populating it from the db if the cache
    // does not have the entry. The cache may be missing entries, but it can
    // never be incoherent (hold an entry that doesn't exist in the db, or that
    // differs from the db) because all db writes for locks go through the cache.
    fn get_owned_object_lock_entry(
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
                    if let (Ok(db_lock), (_, LockEntry::Lock(cached_lock))) =
                        (self.store.get_lock_entry(*obj_ref), occupied.get())
                    {
                        assert_eq!(
                            *cached_lock, db_lock,
                            "cache is incoherent for object ref {:?}",
                            obj_ref
                        );
                    }
                }

                occupied
            }
            DashMapEntry::Vacant(entry) => {
                let lock = self.store.get_lock_entry(*obj_ref)?;

                entry.insert_entry((*obj_ref, LockEntry::Lock(lock)))
            }
        };

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
            let mut entry = match self.get_owned_object_lock_entry(obj_ref) {
                Ok(entry) => entry,
                Err(e) => {
                    ret = Err(e);
                    break;
                }
            };

            let previous_value = entry.get().clone();

            match previous_value {
                LockEntry::Deleted => {
                    ret = Err(SuiError::UserInputError {
                        error: UserInputError::ObjectNotFound {
                            object_id: obj_ref.0,
                            version: Some(obj_ref.1),
                        },
                    });
                    break;
                }

                // Lock exists, but is not set, so we can overwrite it
                LockEntry::Lock(None) => (),

                // Lock is set. Check for equivocation and expiry due to the lock
                // being set in a previous epoch.
                LockEntry::Lock(Some(LockDetails {
                    epoch: previous_epoch,
                    tx_digest: previous_tx_digest,
                })) => {
                    // this should not be possible because we hold the execution lock
                    debug_assert!(
                        epoch >= previous_epoch,
                        "epoch changed while acquiring locks"
                    );
                    if epoch < previous_epoch {
                        ret = Err(SuiError::ObjectLockedAtFutureEpoch {
                            obj_refs: owned_input_objects.to_vec(),
                            locked_epoch: previous_epoch,
                            new_epoch: epoch,
                            locked_by_tx: previous_tx_digest,
                        });
                        break;
                    }

                    // Lock already set to different transaction from the same epoch.
                    // If the lock is set in a previous epoch, it's ok to override it.
                    if previous_epoch == epoch && previous_tx_digest != tx_digest {
                        // TODO: add metrics here
                        info!(prev_tx_digest = ?previous_tx_digest,
                            cur_tx_digest = ?tx_digest,
                            "Cannot acquire lock: conflicting transaction!");
                        ret = Err(SuiError::ObjectLockConflict {
                            obj_ref: *obj_ref,
                            pending_transaction: previous_tx_digest,
                        });
                        break;
                    }
                    if epoch == previous_epoch {
                        // Exactly the same epoch and same transaction, nothing to lock here.
                        continue;
                    } else {
                        info!(prev_epoch =? previous_epoch, cur_epoch =? epoch, "Overriding an old lock from previous epoch");
                        // Fall through and override the old lock.
                    }
                }
            }
            let lock_details = LockDetails { epoch, tx_digest };
            previous_values.push(previous_value.clone());
            locks_to_write.push((*obj_ref, Some(lock_details.clone().into())));
            *entry.get_mut() = LockEntry::Lock(Some(lock_details));
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
                let mut entry = self.get_owned_object_lock_entry(&obj_ref)?;

                // it is impossible for any other thread to modify the lock value after we have
                // written it. This is because the only case in which we overwrite a lock is when
                // the epoch has changed, but because we are holding ExecutionLockReadGuard, the
                // epoch cannot change within this function.
                assert_eq!(
                    *entry.get(),
                    LockEntry::Lock(new_value.map(|l| l.clone().migrate().into_inner())),
                    "lock for {:?} was modified by another thread (should be impossible)",
                    obj_ref
                );
                *entry.get_mut() = previous_value;
            }
        }

        ret
    }

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
            ..
        } = &*outputs;

        // Move dirty markers to cache
        for (object_key, marker_value) in markers.iter() {
            let key = (epoch, object_key.0);

            // first insert into cache
            let marker_cache_entry = self.cached.marker_cache.entry(key).or_default();
            let marker_map = &mut *marker_cache_entry.value().lock();
            marker_map.insert(object_key.1, *marker_value);
            marker_map.truncate(MAX_VERSIONS);

            // remove from dirty collection
            let DashMapEntry::Occupied(mut marker_entry) = self.dirty.markers.entry(key) else {
                panic!("marker map must exist");
            };

            let removed = marker_entry
                .get_mut()
                .remove(&object_key.1)
                .expect("marker version must exist");

            debug_assert_eq!(removed, *marker_value);
        }

        // Move dirty objects to cache
        for (object_id, object) in written.iter() {
            if object.is_child_object() {
                self.insert_object(object_id, object);
            }
        }
        for (object_id, object) in written.iter() {
            if !object.is_child_object() {
                self.insert_object(object_id, object);
                if object.is_package() {
                    self.cached
                        .packages
                        .insert(*object_id, PackageObject::new(object.clone()));
                }
            }
        }

        for ObjectKey(id, version) in deleted.iter() {
            self.insert_deleted_tombstone(id, *version);
        }
        for ObjectKey(id, version) in wrapped.iter() {
            self.insert_wrapped_tombstone(id, *version);
        }

        // TODO(cache): remove dead objects from cache - this cannot actually be done
        // until until objects are committed to the db, because
        /*
        for (id, version) in effects.modified_at_versions().iter() {
            // delete the given id, version from self.objects. if no versions remain, remove the
            // entry from self.objects
            match self.objects.entry(*id) {
                dashmap::mapref::entry::Entry::Occupied(mut entry) => {
                    entry.get_mut().remove(version);
                    if entry.get().is_empty() {
                        entry.remove();
                    }
                }
                dashmap::mapref::entry::Entry::Vacant(_) => panic!("object not found"),
            }
        }
        */

        // delete old locks
        for obj_ref in locks_to_delete.iter() {
            let mut entry = self
                .get_owned_object_lock_entry(obj_ref)
                .expect("lock must exist");
            // NOTE: We just check here that locks exist, not that they are locked to a specific TX. Why?
            // 1. Lock existence prevents re-execution of old certs when objects have been upgraded
            // 2. Not all validators lock, just 2f+1, so transaction should proceed regardless
            //    (But the lock should exist which means previous transactions finished)
            // 3. Equivocation possible (different TX) but as long as 2f+1 approves current TX its
            //    fine
            assert!(
                matches!(entry.get().1, LockEntry::Lock(_)),
                "lock must exist for {:?}",
                obj_ref
            );
            *entry.get_mut() = LockEntry::Deleted;
        }

        // create new locks
        for obj_ref in new_locks_to_init.iter() {
            #[cfg(debug_assertions)]
            {
                assert!(
                    // genesis objects are inserted *prior* to executing the genesis transaction
                    // so we need a loophole for anything that might be a genesis object.
                    self.store.get_lock_entry(*obj_ref).is_err() || obj_ref.1.value() == 1,
                    "lock must not exist in store {:?}",
                    obj_ref
                );
                assert!(
                    self.dirty
                        .owned_object_transaction_locks
                        .get_by_objref(obj_ref)
                        .is_none(),
                    "lock must not exist in cache {:?}",
                    obj_ref
                );
            }

            self.dirty
                .owned_object_transaction_locks
                .insert_by_objref(*obj_ref, LockEntry::Lock(None));
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

        self.executed_effects_digests_notify_read
            .notify(&tx_digest, &effects_digest);

        Ok(())
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
                _ => Ok(None),
            };
        }

        if let Some(objects) = self.cached.object_cache.get(id) {
            let objects = objects.lock();
            // If any version of the object is in the cache, it must be the most recent version.
            return match &objects.get_last().1 {
                ObjectEntry::Object(object) => Ok(Some(object.clone())),
                _ => Ok(None),
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
                .get(obj_ref)
                .as_ref()
                .map(|e| e.value())
            {
                Some(LockEntry::Deleted) => {
                    let lock = self.get_latest_lock_for_object_id(obj_ref.0)?;
                    return Err(SuiError::UserInputError {
                        error: UserInputError::ObjectVersionUnavailableForConsumption {
                            provided_obj_ref: *obj_ref,
                            current_version: lock.1,
                        },
                    });
                }
                Some(LockEntry::Lock(_)) => (),
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
            ..
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
                self.insert_object(object_id, object);
            }
        }
        for (object_id, object) in written.iter() {
            if !object.is_child_object() {
                self.insert_object(object_id, object);
                if object.is_package() {
                    self.cached
                        .packages
                        .insert(*object_id, PackageObject::new(object.clone()));
                }
            }
        }

        for ObjectKey(id, version) in deleted.iter() {
            self.insert_deleted_tombstone(id, *version);
        }
        for ObjectKey(id, version) in wrapped.iter() {
            self.insert_wrapped_tombstone(id, *version);
        }

        // TODO(cache): remove dead objects from cache - this cannot actually be done
        // until until objects are committed to the db, because
        /*
        for (id, version) in effects.modified_at_versions().iter() {
            // delete the given id, version from self.objects. if no versions remain, remove the
            // entry from self.objects
            match self.objects.entry(*id) {
                dashmap::mapref::entry::Entry::Occupied(mut entry) => {
                    entry.get_mut().remove(version);
                    if entry.get().is_empty() {
                        entry.remove();
                    }
                }
                dashmap::mapref::entry::Entry::Vacant(_) => panic!("object not found"),
            }
        }
        */

        // delete old locks
        for obj_ref in locks_to_delete.iter() {
            let mut entry = self
                .get_owned_object_lock_entry(obj_ref)
                .expect("lock must exist");
            // NOTE: We just check here that locks exist, not that they are locked to a specific TX. Why?
            // 1. Lock existence prevents re-execution of old certs when objects have been upgraded
            // 2. Not all validators lock, just 2f+1, so transaction should proceed regardless
            //    (But the lock should exist which means previous transactions finished)
            // 3. Equivocation possible (different TX) but as long as 2f+1 approves current TX its
            //    fine
            assert!(
                matches!(entry.get(), LockEntry::Lock(_)),
                "lock must exist for {:?}",
                obj_ref
            );
            *entry.get_mut() = LockEntry::Deleted;
        }

        // create new locks
        for obj_ref in new_locks_to_init.iter() {
            #[cfg(debug_assertions)]
            {
                assert!(
                    // genesis objects are inserted *prior* to executing the genesis transaction
                    // so we need a loophole for anything that might be a genesis object.
                    self.store.get_lock_entry(*obj_ref).is_err() || obj_ref.1.value() == 1,
                    "lock must not exist in store {:?}",
                    obj_ref
                );
                assert!(
                    self.dirty
                        .owned_object_transaction_locks
                        .get(obj_ref)
                        .is_none(),
                    "lock must not exist in cache {:?}",
                    obj_ref
                );
            }

            self.dirty
                .owned_object_transaction_locks
                .insert(*obj_ref, LockEntry::Lock(None));
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

// TODO(cache): this is not safe, because now we are committing persistent state to the db, while the rest
// of the transaction outputs could simply be lost if the validator restarts. To fix this we must
// cache locks (which we want to do anyway) but that's a big enough change for it to be worth
// saving for a separate PR.
#[instrument(level = "trace", skip_all)]
fn write_locks(
    store: &AuthorityStore,
    locks_to_delete: &[ObjectRef],
    new_locks_to_init: &[ObjectRef],
) {
    store
        .check_owned_object_locks_exist(locks_to_delete)
        .expect("locks must exist for certificate to be executed");
    let lock_table = &store.perpetual_tables.owned_object_transaction_locks;
    let mut batch = lock_table.batch();
    AuthorityStore::initialize_locks(lock_table, &mut batch, new_locks_to_init, false)
        .expect("Failed to initialize locks");
    store
        .delete_locks(&mut batch, locks_to_delete)
        .expect("Failed to delete locks");
    batch.write().expect("Failed to write locks");
}

/// CachedVersionMap is a map from version to value, with the additional contraints:
/// - The key (SequenceNumber) must be monotonically increasing for each insert. If
///   a key is inserted that is less than the previous key, it results in an assertion
///   failure.
/// - Similarly, only the item with the least key can be removed. If an item is removed
///   from the middle of the map, it is marked for removal by setting its corresponding
///   `should_remove` flag to true. If the item with the least key is removed, it is removed
///   immediately, and any consecutive entries that are marked in `should_remove` are also
///   removed.
/// - The intent of these constraints is to ensure that there are never gaps in the collection,
///   so that membership in the map can be tested by comparing to both the highest and lowest
///   (first and last) entries.
#[derive(Debug)]
struct CachedVersionMap<V> {
    values: VecDeque<(SequenceNumber, V)>,
    should_remove: VecDeque<bool>,
}

impl<V> Default for CachedVersionMap<V> {
    fn default() -> Self {
        Self {
            values: VecDeque::new(),
            should_remove: VecDeque::new(),
        }
    }
}

impl<V> CachedVersionMap<V>
where
    V: Clone,
{
    fn insert(&mut self, version: SequenceNumber, value: V) {
        assert!(
            self.values.is_empty() || self.values.back().unwrap().0 < version,
            "version must be monotonically increasing"
        );
        self.values.push_back((version, value));
        self.should_remove.push_back(false);
    }

    // remove the value if it is the first element in values. otherwise mark it
    // for removal.
    fn remove(&mut self, version: &SequenceNumber) -> Option<V> {
        if self.values.is_empty() {
            return None;
        }

        if self.values.front().unwrap().0 == *version {
            self.should_remove.pop_front();
            let ret = self.values.pop_front().unwrap().1;

            // process any deferred removals
            while *self.should_remove.front().unwrap_or(&false) {
                self.should_remove.pop_front();
                self.values.pop_front();
            }

            Some(ret)
        } else {
            // Removals from the interior are deferred.
            // Removals will generally be from the front, and the collection will usually
            // be short, so linear search is preferred.
            if let Some(index) = self.values.iter().position(|(v, _)| v == version) {
                self.should_remove[index] = true;
                Some(self.values[index].1.clone())
            } else {
                None
            }
        }
    }

    fn all_lt_or_eq_rev<'a>(
        &'a self,
        version: &'a SequenceNumber,
    ) -> impl Iterator<Item = &'a (SequenceNumber, V)> {
        self.values
            .iter()
            .rev()
            .take_while(move |(v, _)| v <= version)
    }

    fn get(&self, version: &SequenceNumber) -> Option<&V> {
        if self.values.is_empty() {
            return None;
        }

        for (v, value) in self.values.iter().rev() {
            match v.cmp(version) {
                Ordering::Less => return None,
                Ordering::Equal => return Some(value),
                Ordering::Greater => (),
            }
        }

        None
    }

    fn get_prior_to(&self, version: &SequenceNumber) -> Option<(SequenceNumber, &V)> {
        for (v, value) in self.values.iter().rev() {
            if v < version {
                return Some((*v, value));
            }
        }

        None
    }

    fn get_last(&self) -> &(SequenceNumber, V) {
        self.values.back().expect("CachedVersionMap is empty")
    }

    // pop items from the front of the map until the first item is >= version
    fn truncate(&mut self, limit: usize) {
        while self.values.len() > limit {
            self.should_remove.pop_front();
            self.values.pop_front();
        }
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
