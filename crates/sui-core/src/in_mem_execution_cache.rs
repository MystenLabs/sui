// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::authority::authority_notify_read::EffectsNotifyRead;
use crate::authority::AuthorityStore;
use crate::transaction_output_writer::TransactionOutputs;
use async_trait::async_trait;

use dashmap::DashMap;
use either::Either;
use futures::{
    future::{join_all, BoxFuture},
    FutureExt,
};
use moka::sync::Cache as MokaCache;
use mysten_common::sync::notify_read::NotifyRead;
use parking_lot::Mutex;
use std::collections::BTreeMap;
use std::sync::Arc;
use sui_types::digests::{
    ObjectDigest, TransactionDigest, TransactionEffectsDigest, TransactionEventsDigest,
};
use sui_types::effects::{TransactionEffects, TransactionEvents};
use sui_types::error::{SuiError, SuiResult, UserInputError};
use sui_types::message_envelope::Message;
use sui_types::object::Object;
use sui_types::storage::{MarkerValue, ObjectKey, ObjectStore, PackageObject};
use sui_types::transaction::VerifiedTransaction;
use sui_types::{
    base_types::{EpochId, ObjectID, ObjectRef, SequenceNumber},
    effects::TransactionEffectsAPI,
};
use typed_store::Map;

pub trait ExecutionCacheRead: Send + Sync {
    fn get_package_object(&self, id: &ObjectID) -> SuiResult<Option<PackageObject>>;
    fn force_reload_system_packages(&self, system_package_ids: &[ObjectID]);

    fn get_object(&self, id: &ObjectID) -> SuiResult<Option<Object>>;

    fn get_latest_object_ref_or_tombstone(
        &self,
        object_id: ObjectID,
    ) -> SuiResult<Option<ObjectRef>>;

    fn get_object_by_key(
        &self,
        object_id: &ObjectID,
        version: SequenceNumber,
    ) -> SuiResult<Option<Object>>;

    fn multi_get_object_by_key(&self, object_keys: &[ObjectKey]) -> SuiResult<Vec<Option<Object>>>;

    /// Variant of multi_get_object_by_key used by transaction signing that returns better error messages
    /// when objects are not found. Returns an error if any object is not found.
    fn multi_get_object_by_objref(&self, objrefs: &[ObjectRef]) -> SuiResult<Vec<Object>>;

    /// If the shared object was deleted, return deletion info for the current live version
    fn get_last_shared_object_deletion_info(
        &self,
        object_id: &ObjectID,
        epoch_id: EpochId,
    ) -> SuiResult<Option<(SequenceNumber, TransactionDigest)>>;

    /// If the shared object was deleted, return deletion info for the specified version.
    fn get_deleted_shared_object_previous_tx_digest(
        &self,
        object_id: &ObjectID,
        version: &SequenceNumber,
        epoch_id: EpochId,
    ) -> SuiResult<Option<TransactionDigest>>;

    fn have_received_object_at_version(
        &self,
        object_id: &ObjectID,
        version: SequenceNumber,
        epoch_id: EpochId,
    ) -> SuiResult<bool>;

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
}

pub trait ExecutionCacheWrite: Send + Sync {
    fn update_state(&self, epoch_id: EpochId, tx_outputs: TransactionOutputs) -> SuiResult;
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

pub struct InMemoryCache {
    // Objects are not cached using an LRU because we manage cache evictions manually due to sui
    // semantics.
    objects: DashMap<ObjectID, BTreeMap<SequenceNumber, ObjectEntry>>,

    // packages are cache separately from objects because they are immutable and can be used by any
    // number of transactions
    packages: MokaCache<ObjectID, PackageObject>,

    // Markers for received objects and deleted shared objects. This cache can be invalidated at
    // any time, but if there is an entry, it must contain the most recent marker for the object.
    // Note that MokaCache can only return items by value, so we store the map as an Arc<Mutex>.
    // (There should be no contention on the inner mutex, it is used only for interior mutability.)
    markers: MokaCache<ObjectID, Arc<Mutex<BTreeMap<SequenceNumber, MarkerValue>>>>,

    // Objects that were read at transaction signing time - allows us to access them again at
    // execution time with a single lock / hash lookup
    _transaction_objects: MokaCache<TransactionDigest, Vec<Object>>,

    transaction_effects: DashMap<TransactionEffectsDigest, TransactionEffects>,

    executed_effects_digests: DashMap<TransactionDigest, TransactionEffectsDigest>,

    // Transaction outputs that have not yet been written to the DB. Items are removed from this
    // table as they are flushed to the db.
    pending_transaction_writes: DashMap<TransactionDigest, TransactionOutputs>,

    executed_effects_digests_notify_read: NotifyRead<TransactionDigest, TransactionEffectsDigest>,

    store: Arc<AuthorityStore>,
}

impl InMemoryCache {
    pub fn new(store: Arc<AuthorityStore>) -> Self {
        let packages = MokaCache::builder()
            .max_capacity(10000)
            .initial_capacity(10000)
            .build();
        let markers = MokaCache::builder()
            .max_capacity(10000)
            .initial_capacity(10000)
            .build();
        let transaction_objects = MokaCache::builder()
            .max_capacity(10000)
            .initial_capacity(10000)
            .build();

        Self {
            objects: DashMap::new(),
            packages,
            markers,
            _transaction_objects: transaction_objects,
            transaction_effects: DashMap::new(),
            executed_effects_digests: DashMap::new(),
            pending_transaction_writes: DashMap::new(),
            executed_effects_digests_notify_read: NotifyRead::new(),
            store,
        }
    }

    fn insert_object(&self, object_id: &ObjectID, object: &Object) {
        let version = object.version();
        tracing::debug!("inserting object {:?}: {:?}", object_id, version);
        self.objects
            .entry(*object_id)
            .or_default()
            .insert(object.version(), object.clone().into());
    }

    fn insert_deleted_tombstone(&self, object_id: &ObjectID, version: SequenceNumber) {
        tracing::debug!("inserting deleted tombstone {:?}: {:?}", object_id, version);
        self.objects
            .entry(*object_id)
            .or_default()
            .insert(version, ObjectEntry::Deleted);
    }

    fn insert_wrapped_tombstone(&self, object_id: &ObjectID, version: SequenceNumber) {
        tracing::debug!("inserting wrapped tombstone {:?}: {:?}", object_id, version);
        self.objects
            .entry(*object_id)
            .or_default()
            .insert(version, ObjectEntry::Wrapped);
    }

    pub fn as_notify_read_wrapper(self: Arc<Self>) -> NotifyReadWrapper {
        NotifyReadWrapper(self)
    }
}

fn get_last<K, V>(map: &BTreeMap<K, V>) -> (&K, &V) {
    map.iter().next_back().expect("map cannot be empty")
}

impl ExecutionCacheRead for InMemoryCache {
    fn get_package_object(&self, package_id: &ObjectID) -> SuiResult<Option<PackageObject>> {
        if let Some(p) = self.packages.get(package_id) {
            #[cfg(debug_assertions)]
            {
                assert_eq!(
                    self.store.get_object(package_id).unwrap().unwrap().digest(),
                    p.object().digest(),
                    "Package object cache is inconsistent for package {:?}",
                    package_id
                )
            }
            return Ok(Some(p));
        }

        if let Some(p) = self.store.get_object(package_id)? {
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

    fn get_object(&self, id: &ObjectID) -> SuiResult<Option<Object>> {
        if let Some(objects) = self.objects.get(id) {
            // If any version of the object is in the cache, it must be the most recent version.
            return match get_last(&*objects).1 {
                ObjectEntry::Object(object) => Ok(Some(object.clone())),
                _ => Ok(None),
            };
        }

        // We don't insert objects into the cache because they are usually only
        // read once.
        // TODO: we might want to cache immutable reads (RO shared objects and immutable objects)
        self.store.get_object(id)
    }

    fn get_object_by_key(
        &self,
        object_id: &ObjectID,
        version: SequenceNumber,
    ) -> SuiResult<Option<Object>> {
        if let Some(objects) = self.objects.get(object_id) {
            if let Some(object) = objects.get(&version) {
                if let ObjectEntry::Object(object) = object {
                    return Ok(Some(object.clone()));
                } else {
                    return Ok(None);
                }
            }
        }

        // We don't insert objects into the cache because they are usually only
        // read once.
        self.store.get_object_by_key(object_id, version)
    }

    fn multi_get_object_by_key(
        &self,
        object_keys: &[ObjectKey],
    ) -> Result<Vec<Option<Object>>, SuiError> {
        let mut results = vec![None; object_keys.len()];
        let mut fallback_keys = Vec::with_capacity(object_keys.len());
        let mut fetch_indices = Vec::with_capacity(object_keys.len());

        for (i, key) in object_keys.iter().enumerate() {
            if let Some(object) = self.get_object_by_key(&key.0, key.1)? {
                results[i] = Some(object);
            } else {
                fallback_keys.push(*key);
                fetch_indices.push(i);
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

    fn multi_get_object_by_objref(&self, objrefs: &[ObjectRef]) -> SuiResult<Vec<Object>> {
        let mut results = vec![None; objrefs.len()];
        let mut fallback_keys = Vec::with_capacity(objrefs.len());
        let mut fetch_indices = Vec::with_capacity(objrefs.len());

        for (i, objref) in objrefs.iter().enumerate() {
            if let Some(object) = self.get_object_by_key(&objref.0, objref.1)? {
                results[i] = Some(object);
            } else {
                fallback_keys.push(*objref);
                fetch_indices.push(i);
            }
        }

        let store_results = self
            .store
            .multi_get_object_with_more_accurate_error_return(&fallback_keys)?;
        assert_eq!(store_results.len(), fetch_indices.len());
        assert_eq!(store_results.len(), fallback_keys.len());

        for (i, result) in fetch_indices.into_iter().zip(store_results.into_iter()) {
            results[i] = Some(result);
        }

        Ok(results.into_iter().map(|r| r.unwrap()).collect())
    }

    fn get_latest_object_ref_or_tombstone(
        &self,
        object_id: ObjectID,
    ) -> SuiResult<Option<ObjectRef>> {
        if let Some(objects) = self.objects.get(&object_id) {
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

    /// If the shared object was deleted, return deletion info for the current live version
    fn get_last_shared_object_deletion_info(
        &self,
        object_id: &ObjectID,
        epoch_id: EpochId,
    ) -> SuiResult<Option<(SequenceNumber, TransactionDigest)>> {
        if let Some(markers) = self.markers.get(object_id) {
            if let (version, MarkerValue::SharedDeleted(digest)) = get_last(&*markers.lock()) {
                return Ok(Some((*version, *digest)));
            }
        }

        // TODO: should we update the cache?
        self.store
            .get_last_shared_object_deletion_info(object_id, epoch_id)
    }

    /// If the shared object was deleted, return deletion info for the specified version.
    fn get_deleted_shared_object_previous_tx_digest(
        &self,
        object_id: &ObjectID,
        version: &SequenceNumber,
        epoch_id: EpochId,
    ) -> SuiResult<Option<TransactionDigest>> {
        if let Some(markers) = self.markers.get(object_id) {
            if let Some(MarkerValue::SharedDeleted(digest)) = markers.lock().get(version) {
                return Ok(Some(*digest));
            }
        }

        self.store
            .get_deleted_shared_object_previous_tx_digest(object_id, version, epoch_id)
    }

    fn have_received_object_at_version(
        &self,
        object_id: &ObjectID,
        version: SequenceNumber,
        epoch_id: EpochId,
    ) -> SuiResult<bool> {
        if let Some(markers) = self.markers.get(object_id) {
            if let Some(MarkerValue::Received) = markers.lock().get(&version) {
                return Ok(true);
            }
        }

        self.store
            .have_received_object_at_version(object_id, version, epoch_id)
    }

    fn multi_get_transaction_blocks(
        &self,
        digests: &[TransactionDigest],
    ) -> SuiResult<Vec<Option<Arc<VerifiedTransaction>>>> {
        let mut results = vec![None; digests.len()];
        let mut fetch_indices = Vec::with_capacity(digests.len());
        let mut fetch_digests = Vec::with_capacity(digests.len());

        for (i, digest) in digests.iter().enumerate() {
            if let Some(tx) = self.pending_transaction_writes.get(digest) {
                results[i] = Some(tx.transaction.clone());
            } else {
                fetch_indices.push(i);
                fetch_digests.push(*digest);
            }
        }

        let multiget_results = self.store.multi_get_transaction_blocks(&fetch_digests)?;
        assert_eq!(results.len(), fetch_indices.len());
        assert_eq!(results.len(), fetch_digests.len());

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
            if let Some(digest) = self.executed_effects_digests.get(digest) {
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
            if let Some(effects) = self.transaction_effects.get(digest) {
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
        // TODO: use cache?
        self.store.multi_get_events(event_digests)
    }
}

impl ExecutionCacheWrite for InMemoryCache {
    fn update_state(&self, _epoch_id: EpochId, tx_outputs: TransactionOutputs) -> SuiResult {
        let TransactionOutputs {
            transaction,
            effects,
            markers,
            written,
            deleted,
            wrapped,
            ..
        } = &tx_outputs;

        // Update all marekrs
        for (object_key, marker_value) in markers.iter() {
            self.markers
                .entry_by_ref(&object_key.0)
                .or_insert_with(|| Arc::new(Mutex::new(BTreeMap::new())))
                .value()
                .lock()
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
                    self.packages
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

        // remove dead objects from cache
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

        let tx_digest = *transaction.digest();
        let effects_digest = effects.digest();

        self.transaction_effects
            .insert(effects_digest, effects.clone());

        self.transaction_effects
            .insert(effects_digest, effects.clone());

        self.executed_effects_digests
            .insert(tx_digest, effects_digest);

        self.pending_transaction_writes
            .insert(tx_digest, tx_outputs);

        self.executed_effects_digests_notify_read
            .notify(&tx_digest, &effects_digest);

        Ok(())
    }
}

// TODO: Remove EffectsNotifyRead trait and just use ExecutionCacheRead directly everywhere.
/// This wrapper is used so that we don't have to disambiguate traits at every callsite.
pub struct NotifyReadWrapper(Arc<InMemoryCache>);

#[async_trait]
impl EffectsNotifyRead for NotifyReadWrapper {
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
