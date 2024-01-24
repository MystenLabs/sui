// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::authority::authority_store::ExecutionLockReadGuard;
use crate::authority::authority_store::SuiLockResult;
use crate::authority::AuthorityStore;
use crate::authority::{
    authority_notify_read::EffectsNotifyRead,
    authority_store::{LockDetails, LockDetailsWrapper},
};
use crate::transaction_outputs::TransactionOutputs;
use async_trait::async_trait;

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
use prometheus::{register_int_gauge_with_registry, IntGauge, Registry};
use std::collections::{BTreeMap, HashSet};
use std::sync::Arc;
use sui_storage::package_object_cache::PackageObjectCache;
use sui_types::digests::{
    ObjectDigest, TransactionDigest, TransactionEffectsDigest, TransactionEventsDigest,
};
use sui_types::effects::{TransactionEffects, TransactionEvents};
use sui_types::error::{SuiError, SuiResult, UserInputError};
use sui_types::message_envelope::Message;
use sui_types::object::Object;
use sui_types::storage::{
    error::{Error as StorageError, Result as StorageResult},
    BackingPackageStore, ChildObjectResolver, MarkerValue, ObjectKey, ObjectOrTombstone,
    ObjectStore, PackageObject, ParentSync,
};
use sui_types::sui_system_state::{get_sui_system_state, SuiSystemState};
use sui_types::transaction::VerifiedTransaction;
use sui_types::{
    base_types::{EpochId, ObjectID, ObjectRef, SequenceNumber},
    object::Owner,
    storage::InputKey,
};
use tracing::{info, instrument};
use typed_store::Map;

struct ExecutionCacheMetrics {
    pending_notify_read: IntGauge,
}

impl ExecutionCacheMetrics {
    pub fn new(registry: &Registry) -> Self {
        Self {
            pending_notify_read: register_int_gauge_with_registry!(
                "pending_notify_read",
                "Pending notify read requests",
                registry,
            )
            .unwrap(),
        }
    }
}

pub type ExecutionCache = PassthroughCache;

pub trait ExecutionCacheRead: Send + Sync {
    fn get_package_object(&self, id: &ObjectID) -> SuiResult<Option<PackageObject>>;
    fn force_reload_system_packages(&self, system_package_ids: &[ObjectID]);

    fn get_object(&self, id: &ObjectID) -> SuiResult<Option<Object>>;

    fn get_objects(&self, objects: &[ObjectID]) -> SuiResult<Vec<Option<Object>>> {
        let mut ret = Vec::with_capacity(objects.len());
        for object_id in objects {
            ret.push(self.get_object(object_id)?);
        }
        Ok(ret)
    }

    fn get_latest_object_ref_or_tombstone(
        &self,
        object_id: ObjectID,
    ) -> SuiResult<Option<ObjectRef>>;

    fn get_latest_object_or_tombstone(
        &self,
        object_id: ObjectID,
    ) -> SuiResult<Option<(ObjectKey, ObjectOrTombstone)>>;

    fn get_object_by_key(
        &self,
        object_id: &ObjectID,
        version: SequenceNumber,
    ) -> SuiResult<Option<Object>>;

    fn multi_get_object_by_key(&self, object_keys: &[ObjectKey]) -> SuiResult<Vec<Option<Object>>>;

    fn object_exists_by_key(
        &self,
        object_id: &ObjectID,
        version: SequenceNumber,
    ) -> SuiResult<bool>;

    fn multi_object_exists_by_key(&self, object_keys: &[ObjectKey]) -> SuiResult<Vec<bool>>;

    /// Load a list of objects from the store by object reference.
    /// If they exist in the store, they are returned directly.
    /// If any object missing, we try to figure out the best error to return.
    /// If the object we are asking is currently locked at a future version, we know this
    /// transaction is out-of-date and we return a ObjectVersionUnavailableForConsumption,
    /// which indicates this is not retriable.
    /// Otherwise, we return a ObjectNotFound error, which indicates this is retriable.
    fn multi_get_object_with_more_accurate_error_return(
        &self,
        object_refs: &[ObjectRef],
    ) -> Result<Vec<Object>, SuiError> {
        let objects = self.multi_get_object_by_key(
            &object_refs.iter().map(ObjectKey::from).collect::<Vec<_>>(),
        )?;
        let mut result = Vec::new();
        for (object_opt, object_ref) in objects.into_iter().zip(object_refs) {
            match object_opt {
                None => {
                    let lock = self.get_latest_lock_for_object_id(object_ref.0)?;
                    let error = if lock.1 >= object_ref.1 {
                        UserInputError::ObjectVersionUnavailableForConsumption {
                            provided_obj_ref: *object_ref,
                            current_version: lock.1,
                        }
                    } else {
                        UserInputError::ObjectNotFound {
                            object_id: object_ref.0,
                            version: Some(object_ref.1),
                        }
                    };
                    return Err(SuiError::UserInputError { error });
                }
                Some(object) => {
                    result.push(object);
                }
            }
        }
        assert_eq!(result.len(), object_refs.len());
        Ok(result)
    }

    /// Used by transaction manager to determine if input objects are ready. Distinct from multi_get_object_by_key
    /// because it also consults markers to handle the case where an object will never become available (e.g.
    /// because it has been received by some other transaction already).
    fn multi_input_objects_available(
        &self,
        keys: &[InputKey],
        receiving_objects: HashSet<InputKey>,
        epoch: EpochId,
    ) -> Result<Vec<bool>, SuiError> {
        let (keys_with_version, keys_without_version): (Vec<_>, Vec<_>) = keys
            .iter()
            .enumerate()
            .partition(|(_, key)| key.version().is_some());

        let mut versioned_results = vec![];
        for ((idx, input_key), has_key) in keys_with_version.iter().zip(
            self.multi_object_exists_by_key(
                &keys_with_version
                    .iter()
                    .map(|(_, k)| ObjectKey(k.id(), k.version().unwrap()))
                    .collect::<Vec<_>>(),
            )?
            .into_iter(),
        ) {
            // If the key exists at the specified version, then the object is available.
            if has_key {
                versioned_results.push((*idx, true))
            } else if receiving_objects.contains(input_key) {
                // There could be a more recent version of this object, and the object at the
                // specified version could have already been pruned. In such a case `has_key` will
                // be false, but since this is a receiving object we should mark it as available if
                // we can determine that an object with a version greater than or equal to the
                // specified version exists or was deleted. We will then let mark it as available
                // to let the the transaction through so it can fail at execution.
                let is_available = self
                    .get_object(&input_key.id())?
                    .map(|obj| obj.version() >= input_key.version().unwrap())
                    .unwrap_or(false)
                    || self.have_deleted_owned_object_at_version_or_after(
                        &input_key.id(),
                        input_key.version().unwrap(),
                        epoch,
                    )?;
                versioned_results.push((*idx, is_available));
            } else if self
                .get_deleted_shared_object_previous_tx_digest(
                    &input_key.id(),
                    &input_key.version().unwrap(),
                    epoch,
                )?
                .is_some()
            {
                // If the object is an already deleted shared object, mark it as available if the
                // version for that object is in the shared deleted marker table.
                versioned_results.push((*idx, true));
            } else {
                versioned_results.push((*idx, false));
            }
        }

        let unversioned_results = keys_without_version.into_iter().map(|(idx, key)| {
            (
                idx,
                match self
                    .get_latest_object_ref_or_tombstone(key.id())
                    .expect("read cannot fail")
                {
                    None => false,
                    Some(entry) => entry.2.is_alive(),
                },
            )
        });

        let mut results = versioned_results
            .into_iter()
            .chain(unversioned_results)
            .collect::<Vec<_>>();
        results.sort_by_key(|(idx, _)| *idx);
        Ok(results.into_iter().map(|(_, result)| result).collect())
    }

    /// Return the object with version less then or eq to the provided seq number.
    /// This is used by indexer to find the correct version of dynamic field child object.
    /// We do not store the version of the child object, but because of lamport timestamp,
    /// we know the child must have version number less then or eq to the parent.
    fn find_object_lt_or_eq_version(
        &self,
        object_id: ObjectID,
        version: SequenceNumber,
    ) -> Option<Object>;

    fn get_lock(&self, obj_ref: ObjectRef, epoch_id: EpochId) -> SuiLockResult;

    fn get_latest_lock_for_object_id(&self, object_id: ObjectID) -> SuiResult<ObjectRef>;

    fn check_owned_object_locks_exist(&self, objects: &[ObjectRef]) -> SuiResult;

    fn multi_get_transaction_blocks(
        &self,
        digests: &[TransactionDigest],
    ) -> SuiResult<Vec<Option<Arc<VerifiedTransaction>>>>;

    fn get_transaction_block(
        &self,
        digest: &TransactionDigest,
    ) -> SuiResult<Option<Arc<VerifiedTransaction>>> {
        self.multi_get_transaction_blocks(&[*digest])
            .map(|mut blocks| {
                blocks
                    .pop()
                    .expect("multi-get must return correct number of items")
            })
    }

    fn multi_get_executed_effects_digests(
        &self,
        digests: &[TransactionDigest],
    ) -> SuiResult<Vec<Option<TransactionEffectsDigest>>>;

    fn is_tx_already_executed(&self, digest: &TransactionDigest) -> SuiResult<bool> {
        self.multi_get_executed_effects_digests(&[*digest])
            .map(|mut digests| {
                digests
                    .pop()
                    .expect("multi-get must return correct number of items")
                    .is_some()
            })
    }

    fn multi_get_executed_effects(
        &self,
        digests: &[TransactionDigest],
    ) -> SuiResult<Vec<Option<TransactionEffects>>> {
        let effects_digests = self.multi_get_executed_effects_digests(digests)?;
        assert_eq!(effects_digests.len(), digests.len());

        let mut results = vec![None; digests.len()];
        let mut fetch_digests = Vec::with_capacity(digests.len());
        let mut fetch_indices = Vec::with_capacity(digests.len());

        for (i, digest) in effects_digests.into_iter().enumerate() {
            if let Some(digest) = digest {
                fetch_digests.push(digest);
                fetch_indices.push(i);
            }
        }

        let effects = self.multi_get_effects(&fetch_digests)?;
        for (i, effects) in fetch_indices.into_iter().zip(effects.into_iter()) {
            results[i] = effects;
        }

        Ok(results)
    }

    fn get_executed_effects(
        &self,
        digest: &TransactionDigest,
    ) -> SuiResult<Option<TransactionEffects>> {
        self.multi_get_executed_effects(&[*digest])
            .map(|mut effects| {
                effects
                    .pop()
                    .expect("multi-get must return correct number of items")
            })
    }

    fn multi_get_effects(
        &self,
        digests: &[TransactionEffectsDigest],
    ) -> SuiResult<Vec<Option<TransactionEffects>>>;

    fn get_effects(
        &self,
        digest: &TransactionEffectsDigest,
    ) -> SuiResult<Option<TransactionEffects>> {
        self.multi_get_effects(&[*digest]).map(|mut effects| {
            effects
                .pop()
                .expect("multi-get must return correct number of items")
        })
    }

    fn multi_get_events(
        &self,
        event_digests: &[TransactionEventsDigest],
    ) -> SuiResult<Vec<Option<TransactionEvents>>>;

    fn get_events(&self, digest: &TransactionEventsDigest) -> SuiResult<Option<TransactionEvents>> {
        self.multi_get_events(&[*digest]).map(|mut events| {
            events
                .pop()
                .expect("multi-get must return correct number of items")
        })
    }

    fn notify_read_executed_effects_digests<'a>(
        &'a self,
        digests: &'a [TransactionDigest],
    ) -> BoxFuture<'a, SuiResult<Vec<TransactionEffectsDigest>>>;

    fn notify_read_executed_effects<'a>(
        &'a self,
        digests: &'a [TransactionDigest],
    ) -> BoxFuture<'a, SuiResult<Vec<TransactionEffects>>> {
        async move {
            let digests = self.notify_read_executed_effects_digests(digests).await?;
            // once digests are available, effects must be present as well
            self.multi_get_effects(&digests).map(|effects| {
                effects
                    .into_iter()
                    .map(|e| e.expect("digests must exist"))
                    .collect()
            })
        }
        .boxed()
    }

    fn get_sui_system_state_object_unsafe(&self) -> SuiResult<SuiSystemState>;

    // Marker methods

    /// Get the marker at a specific version
    fn get_marker_value(
        &self,
        object_id: &ObjectID,
        version: &SequenceNumber,
        epoch_id: EpochId,
    ) -> SuiResult<Option<MarkerValue>>;

    /// Get the latest marker for a given object.
    fn get_latest_marker(
        &self,
        object_id: &ObjectID,
        epoch_id: EpochId,
    ) -> SuiResult<Option<(SequenceNumber, MarkerValue)>>;

    /// If the shared object was deleted, return deletion info for the current live version
    fn get_last_shared_object_deletion_info(
        &self,
        object_id: &ObjectID,
        epoch_id: EpochId,
    ) -> SuiResult<Option<(SequenceNumber, TransactionDigest)>> {
        match self.get_latest_marker(object_id, epoch_id)? {
            Some((version, MarkerValue::SharedDeleted(digest))) => Ok(Some((version, digest))),
            _ => Ok(None),
        }
    }

    /// If the shared object was deleted, return deletion info for the specified version.
    fn get_deleted_shared_object_previous_tx_digest(
        &self,
        object_id: &ObjectID,
        version: &SequenceNumber,
        epoch_id: EpochId,
    ) -> SuiResult<Option<TransactionDigest>> {
        match self.get_marker_value(object_id, version, epoch_id)? {
            Some(MarkerValue::SharedDeleted(digest)) => Ok(Some(digest)),
            _ => Ok(None),
        }
    }

    fn have_received_object_at_version(
        &self,
        object_id: &ObjectID,
        version: SequenceNumber,
        epoch_id: EpochId,
    ) -> SuiResult<bool> {
        match self.get_marker_value(object_id, &version, epoch_id)? {
            Some(MarkerValue::Received) => Ok(true),
            _ => Ok(false),
        }
    }

    fn have_deleted_owned_object_at_version_or_after(
        &self,
        object_id: &ObjectID,
        version: SequenceNumber,
        epoch_id: EpochId,
    ) -> SuiResult<bool> {
        match self.get_latest_marker(object_id, epoch_id)? {
            Some((marker_version, MarkerValue::OwnedDeleted)) if marker_version >= version => {
                Ok(true)
            }
            _ => Ok(false),
        }
    }
}

pub trait ExecutionCacheWrite: Send + Sync {
    /// Write the output of a transaction.
    ///
    /// Because of the child object consistency rule (readers that observe parents must observe all
    /// children of that parent, up to the parent's version bound), implementations of this method
    /// must not write any top-level (address-owned or shared) objects before they have written all
    /// of the object-owned objects (i.e. child objects) in the `objects` list.
    ///
    /// In the future, we may modify this method to expose finer-grained information about
    /// parent/child relationships. (This may be especially necessary for distributed object
    /// storage, but is unlikely to be an issue before we tackle that problem).
    ///
    /// This function may evict the mutable input objects (and successfully received objects) of
    /// transaction from the cache, since they cannot be read by any other transaction.
    ///
    /// Any write performed by this method immediately notifies any waiter that has previously
    /// called notify_read_objects_for_execution or notify_read_objects_for_signing for the object
    /// in question.
    fn write_transaction_outputs(
        &self,
        epoch_id: EpochId,
        tx_outputs: Arc<TransactionOutputs>,
    ) -> BoxFuture<'_, SuiResult>;

    /// Attempt to acquire object locks for all of the owned input locks.
    fn acquire_transaction_locks<'a>(
        &'a self,
        execution_lock: &'a ExecutionLockReadGuard<'_>,
        owned_input_objects: &'a [ObjectRef],
        tx_digest: TransactionDigest,
    ) -> BoxFuture<'a, SuiResult>;
}

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
    objects: DashMap<ObjectID, BTreeMap<SequenceNumber, ObjectEntry>>,

    // Mirrors the owned_object_transaction_locks table in the db.
    owned_object_transaction_locks: DashMap<ObjectRef, LockEntry>,

    // Markers for received objects and deleted shared objects. This contains all of the dirty
    // marker state, which is committed to the db at the same time as other transaction data.
    // After markers are committed to the db we remove them from this table and insert them into
    // marker_cache.
    markers: DashMap<MarkerKey, BTreeMap<SequenceNumber, MarkerValue>>,

    transaction_effects: DashMap<TransactionEffectsDigest, TransactionEffects>,

    transaction_events: DashMap<TransactionEventsDigest, TransactionEvents>,

    executed_effects_digests: DashMap<TransactionDigest, TransactionEffectsDigest>,

    // Transaction outputs that have not yet been written to the DB. Items are removed from this
    // table as they are flushed to the db.
    pending_transaction_writes: DashMap<TransactionDigest, TransactionOutputs>,
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
    object_cache: MokaCache<ObjectID, Arc<Mutex<BTreeMap<SequenceNumber, ObjectEntry>>>>,

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
    marker_cache: MokaCache<MarkerKey, Arc<Mutex<BTreeMap<SequenceNumber, MarkerValue>>>>,

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
pub struct MemoryExecutionCache {
    dirty: UncommittedData,
    cached: CachedData,

    executed_effects_digests_notify_read: NotifyRead<TransactionDigest, TransactionEffectsDigest>,
    store: Arc<AuthorityStore>,
}

impl MemoryExecutionCache {
    pub fn new(store: Arc<AuthorityStore>) -> Self {
        Self {
            dirty: UncommittedData::new(),
            cached: CachedData::new(),
            executed_effects_digests_notify_read: NotifyRead::new(),
            store,
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

                if get_last(&*$objects).0 < &version {
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
    ) -> SuiResult<DashMapOccupiedEntry<'_, ObjectRef, LockEntry>> {
        let entry = self.dirty.owned_object_transaction_locks.entry(*obj_ref);
        let occupied = match entry {
            DashMapEntry::Occupied(occupied) => {
                if cfg!(debug_assertions) {
                    if let (Ok(db_lock), LockEntry::Lock(cached_lock)) =
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
                entry.insert_entry(LockEntry::Lock(lock))
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
}

fn get_last<K, V>(map: &BTreeMap<K, V>) -> (&K, &V) {
    map.iter().next_back().expect("map cannot be empty")
}

impl ExecutionCacheRead for MemoryExecutionCache {
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
            return match get_last(&*objects).1 {
                ObjectEntry::Object(object) => Ok(Some(object.clone())),
                _ => Ok(None),
            };
        }

        if let Some(objects) = self.cached.object_cache.get(id) {
            let objects = objects.lock();
            // If any version of the object is in the cache, it must be the most recent version.
            return match get_last(&*objects).1 {
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

    fn multi_get_object_by_key(
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

        let store_results = self.store.multi_get_object_by_key(&fallback_keys)?;
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
            let (version, object) = get_last(&*objects);
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
    ) -> Option<Object> {
        // Both self.objects and self.object_cache have no gaps, and self.object_cache
        // cannot have a more recent version than self.objects.
        // note that while binary searching would be more efficient for random keys, child
        // reads will be disproportionately more likely to be for a very recent version.
        if let Some(objects) = self.dirty.objects.get(&object_id) {
            for (_, object) in objects.range(..=version).rev() {
                if let ObjectEntry::Object(object) = object {
                    return Some(object.clone());
                }
            }
        }

        if let Some(objects) = self.cached.object_cache.get(&object_id) {
            let objects = objects.lock();
            for (_, object) in objects.range(..=version).rev() {
                if let ObjectEntry::Object(object) = object {
                    return Some(object.clone());
                }
            }
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
        let mut results = vec![None; event_digests.len()];
        let mut fallback_digests = Vec::with_capacity(event_digests.len());
        let mut fallback_indices = Vec::with_capacity(event_digests.len());

        for (i, digest) in event_digests.iter().enumerate() {
            if let Some(events) = self.dirty.transaction_events.get(digest) {
                results[i] = Some(events.clone());
            } else {
                fallback_digests.push(*digest);
                fallback_indices.push(i);
            }
        }

        let fallback_results = self.store.multi_get_events(&fallback_digests)?;
        assert_eq!(fallback_results.len(), fallback_indices.len());
        assert_eq!(fallback_results.len(), fallback_digests.len());
        for (i, result) in fallback_indices
            .into_iter()
            .zip(fallback_results.into_iter())
        {
            results[i] = result;
        }
        Ok(results)
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
            let (k, v) = get_last(&*markers);
            return Ok(Some((*k, *v)));
        }

        if let Some(markers) = self.cached.marker_cache.get(&(epoch_id, *object_id)) {
            let markers = markers.lock();
            let (k, v) = get_last(&*markers);
            return Ok(Some((*k, *v)));
        }

        // TODO: we could insert this marker into the cache since it is the latest
        self.store.get_latest_marker(object_id, epoch_id)
    }
}

impl ExecutionCacheWrite for MemoryExecutionCache {
    #[instrument(level = "trace", skip_all)]
    fn acquire_transaction_locks<'a>(
        &'a self,
        execution_lock: &'a ExecutionLockReadGuard<'_>,
        owned_input_objects: &'a [ObjectRef],
        tx_digest: TransactionDigest,
    ) -> BoxFuture<'a, SuiResult> {
        MemoryExecutionCache::acquire_transaction_locks(
            self,
            execution_lock,
            owned_input_objects,
            tx_digest,
        )
        .boxed()
    }

    #[instrument(level = "trace", skip_all)]
    fn write_transaction_outputs(
        &self,
        epoch_id: EpochId,
        tx_outputs: TransactionOutputs,
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
        } = &tx_outputs;

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

pub struct PassthroughCache {
    store: Arc<AuthorityStore>,
    metrics: Option<ExecutionCacheMetrics>,
    package_cache: Arc<PackageObjectCache>,
    executed_effects_digests_notify_read: NotifyRead<TransactionDigest, TransactionEffectsDigest>,
}

impl PassthroughCache {
    pub fn new(store: Arc<AuthorityStore>, registry: &Registry) -> Self {
        Self {
            store,
            metrics: Some(ExecutionCacheMetrics::new(registry)),
            package_cache: PackageObjectCache::new(),
            executed_effects_digests_notify_read: NotifyRead::new(),
        }
    }

    pub fn new_with_no_metrics(store: Arc<AuthorityStore>) -> Self {
        Self {
            store,
            metrics: None,
            package_cache: PackageObjectCache::new(),
            executed_effects_digests_notify_read: NotifyRead::new(),
        }
    }

    pub fn as_notify_read_wrapper(self: Arc<Self>) -> NotifyReadWrapper<Self> {
        NotifyReadWrapper(self)
    }
}

impl ExecutionCacheRead for PassthroughCache {
    fn get_package_object(&self, package_id: &ObjectID) -> SuiResult<Option<PackageObject>> {
        self.package_cache
            .get_package_object(package_id, &*self.store)
    }

    fn force_reload_system_packages(&self, system_package_ids: &[ObjectID]) {
        self.package_cache
            .force_reload_system_packages(system_package_ids.iter().cloned(), self);
    }

    fn get_object(&self, id: &ObjectID) -> SuiResult<Option<Object>> {
        self.store.get_object(id).map_err(Into::into)
    }

    fn get_object_by_key(
        &self,
        object_id: &ObjectID,
        version: SequenceNumber,
    ) -> SuiResult<Option<Object>> {
        Ok(self.store.get_object_by_key(object_id, version)?)
    }

    fn multi_get_object_by_key(
        &self,
        object_keys: &[ObjectKey],
    ) -> Result<Vec<Option<Object>>, SuiError> {
        self.store.multi_get_object_by_key(object_keys)
    }

    fn object_exists_by_key(
        &self,
        object_id: &ObjectID,
        version: SequenceNumber,
    ) -> SuiResult<bool> {
        self.store.object_exists_by_key(object_id, version)
    }

    fn multi_object_exists_by_key(&self, object_keys: &[ObjectKey]) -> SuiResult<Vec<bool>> {
        self.store.multi_object_exists_by_key(object_keys)
    }

    fn get_latest_object_ref_or_tombstone(
        &self,
        object_id: ObjectID,
    ) -> SuiResult<Option<ObjectRef>> {
        self.store.get_latest_object_ref_or_tombstone(object_id)
    }

    fn get_latest_object_or_tombstone(
        &self,
        object_id: ObjectID,
    ) -> Result<Option<(ObjectKey, ObjectOrTombstone)>, SuiError> {
        self.store.get_latest_object_or_tombstone(object_id)
    }

    fn find_object_lt_or_eq_version(
        &self,
        object_id: ObjectID,
        version: SequenceNumber,
    ) -> Option<Object> {
        self.store.find_object_lt_or_eq_version(object_id, version)
    }

    fn get_lock(&self, obj_ref: ObjectRef, epoch_id: EpochId) -> SuiLockResult {
        self.store.get_lock(obj_ref, epoch_id)
    }

    fn get_latest_lock_for_object_id(&self, object_id: ObjectID) -> SuiResult<ObjectRef> {
        self.store.get_latest_lock_for_object_id(object_id)
    }

    fn check_owned_object_locks_exist(&self, objects: &[ObjectRef]) -> SuiResult {
        self.store.check_owned_object_locks_exist(objects)
    }

    fn multi_get_transaction_blocks(
        &self,
        digests: &[TransactionDigest],
    ) -> SuiResult<Vec<Option<Arc<VerifiedTransaction>>>> {
        Ok(self
            .store
            .multi_get_transaction_blocks(digests)?
            .into_iter()
            .map(|o| o.map(Arc::new))
            .collect())
    }

    fn multi_get_executed_effects_digests(
        &self,
        digests: &[TransactionDigest],
    ) -> SuiResult<Vec<Option<TransactionEffectsDigest>>> {
        self.store.multi_get_executed_effects_digests(digests)
    }

    fn multi_get_effects(
        &self,
        digests: &[TransactionEffectsDigest],
    ) -> SuiResult<Vec<Option<TransactionEffects>>> {
        Ok(self.store.perpetual_tables.effects.multi_get(digests)?)
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
        self.store.multi_get_events(event_digests)
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
        self.store.get_marker_value(object_id, version, epoch_id)
    }

    fn get_latest_marker(
        &self,
        object_id: &ObjectID,
        epoch_id: EpochId,
    ) -> SuiResult<Option<(SequenceNumber, MarkerValue)>> {
        self.store.get_latest_marker(object_id, epoch_id)
    }
}

impl ExecutionCacheWrite for PassthroughCache {
    #[instrument(level = "debug", skip_all)]
    fn write_transaction_outputs(
        &self,
        epoch_id: EpochId,
        tx_outputs: Arc<TransactionOutputs>,
    ) -> BoxFuture<'_, SuiResult> {
        async move {
            let tx_digest = *tx_outputs.transaction.digest();
            let effects_digest = tx_outputs.effects.digest();
            self.store
                .write_transaction_outputs(epoch_id, tx_outputs)
                .await?;

            self.executed_effects_digests_notify_read
                .notify(&tx_digest, &effects_digest);

            if let Some(m) = self.metrics.as_ref() {
                m.pending_notify_read
                    .set(self.executed_effects_digests_notify_read.num_pending() as i64)
            }

            Ok(())
        }
        .boxed()
    }

    fn acquire_transaction_locks<'a>(
        &'a self,
        execution_lock: &ExecutionLockReadGuard<'_>,
        owned_input_objects: &'a [ObjectRef],
        tx_digest: TransactionDigest,
    ) -> BoxFuture<'a, SuiResult> {
        let epoch_id = **execution_lock;
        self.store
            .deprecated_acquire_transaction_locks(epoch_id, owned_input_objects, tx_digest)
            .boxed()
    }
}

// TODO: Remove EffectsNotifyRead trait and just use ExecutionCacheRead directly everywhere.
/// This wrapper is used so that we don't have to disambiguate traits at every callsite.
pub struct NotifyReadWrapper<T>(Arc<T>);

#[async_trait]
impl<T: ExecutionCacheRead + 'static> EffectsNotifyRead for NotifyReadWrapper<T> {
    async fn notify_read_executed_effects(
        &self,
        digests: Vec<TransactionDigest>,
    ) -> SuiResult<Vec<TransactionEffects>> {
        self.0.notify_read_executed_effects(&digests).await
    }

    async fn notify_read_executed_effects_digests(
        &self,
        digests: Vec<TransactionDigest>,
    ) -> SuiResult<Vec<TransactionEffectsDigest>> {
        self.0.notify_read_executed_effects_digests(&digests).await
    }

    fn multi_get_executed_effects(
        &self,
        digests: &[TransactionDigest],
    ) -> SuiResult<Vec<Option<TransactionEffects>>> {
        self.0.multi_get_executed_effects(digests)
    }
}

macro_rules! implement_storage_traits {
    ($implementor: ident) => {
        impl ObjectStore for $implementor {
            fn get_object(&self, object_id: &ObjectID) -> StorageResult<Option<Object>> {
                ExecutionCacheRead::get_object(self, object_id).map_err(StorageError::custom)
            }

            fn get_object_by_key(
                &self,
                object_id: &ObjectID,
                version: sui_types::base_types::VersionNumber,
            ) -> StorageResult<Option<Object>> {
                ExecutionCacheRead::get_object_by_key(self, object_id, version)
                    .map_err(StorageError::custom)
            }
        }

        impl ChildObjectResolver for $implementor {
            fn read_child_object(
                &self,
                parent: &ObjectID,
                child: &ObjectID,
                child_version_upper_bound: SequenceNumber,
            ) -> SuiResult<Option<Object>> {
                let Some(child_object) =
                    self.find_object_lt_or_eq_version(*child, child_version_upper_bound)
                else {
                    return Ok(None);
                };

                let parent = *parent;
                if child_object.owner != Owner::ObjectOwner(parent.into()) {
                    return Err(SuiError::InvalidChildObjectAccess {
                        object: *child,
                        given_parent: parent,
                        actual_owner: child_object.owner,
                    });
                }
                Ok(Some(child_object))
            }

            fn get_object_received_at_version(
                &self,
                owner: &ObjectID,
                receiving_object_id: &ObjectID,
                receive_object_at_version: SequenceNumber,
                epoch_id: EpochId,
            ) -> SuiResult<Option<Object>> {
                let Some(recv_object) = ExecutionCacheRead::get_object_by_key(
                    self,
                    receiving_object_id,
                    receive_object_at_version,
                )?
                else {
                    return Ok(None);
                };

                // Check for:
                // * Invalid access -- treat as the object does not exist. Or;
                // * If we've already received the object at the version -- then treat it as though it doesn't exist.
                // These two cases must remain indisguishable to the caller otherwise we risk forks in
                // transaction replay due to possible reordering of transactions during replay.
                if recv_object.owner != Owner::AddressOwner((*owner).into())
                    || self.have_received_object_at_version(
                        receiving_object_id,
                        receive_object_at_version,
                        epoch_id,
                    )?
                {
                    return Ok(None);
                }

                Ok(Some(recv_object))
            }
        }

        impl BackingPackageStore for $implementor {
            fn get_package_object(
                &self,
                package_id: &ObjectID,
            ) -> SuiResult<Option<PackageObject>> {
                ExecutionCacheRead::get_package_object(self, package_id)
            }
        }

        impl ParentSync for $implementor {
            fn get_latest_parent_entry_ref_deprecated(
                &self,
                object_id: ObjectID,
            ) -> SuiResult<Option<ObjectRef>> {
                ExecutionCacheRead::get_latest_object_ref_or_tombstone(self, object_id)
            }
        }
    };
}

implement_storage_traits!(MemoryExecutionCache);
implement_storage_traits!(PassthroughCache);
