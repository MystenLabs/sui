// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::authority::authority_notify_read::EffectsNotifyRead;
use crate::authority::authority_store::SuiLockResult;
use crate::authority::AuthorityStore;
use crate::transaction_outputs::TransactionOutputs;
use async_trait::async_trait;

use either::Either;
use futures::{
    future::{join_all, BoxFuture},
    FutureExt,
};
use mysten_common::sync::notify_read::NotifyRead;
use prometheus::{register_int_gauge_with_registry, IntGauge, Registry};
use std::collections::HashSet;
use std::sync::Arc;
use sui_storage::package_object_cache::PackageObjectCache;
use sui_types::digests::{TransactionDigest, TransactionEffectsDigest, TransactionEventsDigest};
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
use tracing::instrument;
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

    #[instrument(level = "trace", skip_all)]
    fn get_transactions_and_serialized_sizes(
        &self,
        digests: &[TransactionDigest],
    ) -> SuiResult<Vec<Option<(VerifiedTransaction, usize)>>> {
        let txns = self.multi_get_transaction_blocks(digests)?;
        txns.into_iter()
            .map(|txn| {
                txn.map(|txn| {
                    // Note: if the transaction is read from the db, we are wasting some
                    // effort relative to reading the raw bytes from the db instead of
                    // calling serialized_size. However, transactions should usually be
                    // fetched from cache.
                    match txn.serialized_size() {
                        Ok(size) => Ok(((*txn).clone(), size)),
                        Err(e) => Err(e),
                    }
                })
                .transpose()
            })
            .collect::<Result<Vec<_>, _>>()
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
        tx_outputs: TransactionOutputs,
    ) -> BoxFuture<'_, SuiResult>;

    /// Attempt to acquire object locks for all of the owned input locks.
    fn acquire_transaction_locks<'a>(
        &'a self,
        epoch_id: EpochId,
        owned_input_objects: &'a [ObjectRef],
        tx_digest: TransactionDigest,
    ) -> BoxFuture<'a, SuiResult>;
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
    fn write_transaction_outputs<'a>(
        &'a self,
        epoch_id: EpochId,
        tx_outputs: TransactionOutputs,
    ) -> BoxFuture<'a, SuiResult> {
        async move {
            let tx_digest = *tx_outputs.transaction.digest();
            let effects_digest = tx_outputs.effects.digest();
            self.store
                .write_transaction_outputs(epoch_id, tx_outputs)
                .await?;

            self.executed_effects_digests_notify_read
                .notify(&tx_digest, &effects_digest);

            if let Some(metrics) = &self.metrics {
                metrics
                    .pending_notify_read
                    .set(self.executed_effects_digests_notify_read.num_pending() as i64);
            }

            Ok(())
        }
        .boxed()
    }

    fn acquire_transaction_locks<'a>(
        &'a self,
        epoch_id: EpochId,
        owned_input_objects: &'a [ObjectRef],
        tx_digest: TransactionDigest,
    ) -> BoxFuture<'a, SuiResult> {
        self.store
            .acquire_transaction_locks(epoch_id, owned_input_objects, tx_digest)
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

implement_storage_traits!(PassthroughCache);
