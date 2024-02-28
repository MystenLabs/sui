// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::authority::authority_per_epoch_store::AuthorityPerEpochStore;
use crate::authority::authority_store::{ExecutionLockWriteGuard, SuiLockResult};
use crate::authority::epoch_start_configuration::EpochFlag;
use crate::authority::{
    authority_notify_read::EffectsNotifyRead, epoch_start_configuration::EpochStartConfiguration,
};
use crate::transaction_outputs::TransactionOutputs;
use async_trait::async_trait;

use futures::{future::BoxFuture, FutureExt};
use prometheus::{register_int_gauge_with_registry, IntGauge, Registry};
use std::collections::HashSet;
use std::path::Path;
use std::sync::Arc;
use sui_protocol_config::ProtocolVersion;
use sui_types::base_types::VerifiedExecutionData;
use sui_types::digests::{TransactionDigest, TransactionEffectsDigest, TransactionEventsDigest};
use sui_types::effects::{TransactionEffects, TransactionEvents};
use sui_types::error::{SuiError, SuiResult, UserInputError};
use sui_types::messages_checkpoint::CheckpointSequenceNumber;
use sui_types::object::Object;
use sui_types::storage::{
    error::{Error as StorageError, Result as StorageResult},
    BackingPackageStore, ChildObjectResolver, MarkerValue, ObjectKey, ObjectOrTombstone,
    ObjectStore, PackageObject, ParentSync,
};
use sui_types::sui_system_state::SuiSystemState;
use sui_types::transaction::VerifiedTransaction;
use sui_types::{
    base_types::{EpochId, ObjectID, ObjectRef, SequenceNumber},
    object::Owner,
    storage::InputKey,
};
use tracing::instrument;

pub(crate) mod cached_version_map;
pub mod passthrough_cache;
pub mod writeback_cache;

use passthrough_cache::PassthroughCache;
use writeback_cache::WritebackCache;

pub struct ExecutionCacheMetrics {
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

pub trait ExecutionCacheCommit: Send + Sync {
    /// Durably commit the transaction outputs of the given transaction to the database.
    /// Will be called by CheckpointExecutor to ensure that transaction outputs are
    /// written durably before marking a checkpoint as finalized.
    fn commit_transaction_outputs(
        &self,
        epoch: EpochId,
        digest: &TransactionDigest,
    ) -> BoxFuture<'_, SuiResult>;
}

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

    fn multi_get_objects_by_key(&self, object_keys: &[ObjectKey])
        -> SuiResult<Vec<Option<Object>>>;

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
    fn multi_get_objects_with_more_accurate_error_return(
        &self,
        object_refs: &[ObjectRef],
    ) -> Result<Vec<Object>, SuiError> {
        let objects = self.multi_get_objects_by_key(
            &object_refs.iter().map(ObjectKey::from).collect::<Vec<_>>(),
        )?;
        let mut result = Vec::new();
        for (object_opt, object_ref) in objects.into_iter().zip(object_refs) {
            match object_opt {
                None => {
                    let lock = self._get_latest_lock_for_object_id(object_ref.0)?;
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
                    input_key.version().unwrap(),
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
    ) -> SuiResult<Option<Object>>;

    fn get_lock(&self, obj_ref: ObjectRef, epoch_id: EpochId) -> SuiLockResult;

    // This method is considered "private" - only used by multi_get_objects_with_more_accurate_error_return
    fn _get_latest_lock_for_object_id(&self, object_id: ObjectID) -> SuiResult<ObjectRef>;

    fn check_owned_object_locks_exist(&self, owned_object_refs: &[ObjectRef]) -> SuiResult;

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
        version: SequenceNumber,
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
        version: SequenceNumber,
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
        match self.get_marker_value(object_id, version, epoch_id)? {
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
        epoch_id: EpochId,
        owned_input_objects: &'a [ObjectRef],
        tx_digest: TransactionDigest,
    ) -> BoxFuture<'a, SuiResult>;
}

pub trait CheckpointCache: Send + Sync {
    // TODO: In addition to the deprecated methods below, this will eventually include access
    // to the CheckpointStore

    // DEPRECATED METHODS
    fn deprecated_get_transaction_checkpoint(
        &self,
        digest: &TransactionDigest,
    ) -> SuiResult<Option<(EpochId, CheckpointSequenceNumber)>>;

    fn deprecated_multi_get_transaction_checkpoint(
        &self,
        digests: &[TransactionDigest],
    ) -> SuiResult<Vec<Option<(EpochId, CheckpointSequenceNumber)>>>;

    fn deprecated_insert_finalized_transactions(
        &self,
        digests: &[TransactionDigest],
        epoch: EpochId,
        sequence: CheckpointSequenceNumber,
    ) -> SuiResult;
}

pub trait ExecutionCacheReconfigAPI: Send + Sync {
    fn insert_genesis_object(&self, object: Object) -> SuiResult;
    fn bulk_insert_genesis_objects(&self, objects: &[Object]) -> SuiResult;

    fn revert_state_update(&self, digest: &TransactionDigest) -> SuiResult;
    fn set_epoch_start_configuration(
        &self,
        epoch_start_config: &EpochStartConfiguration,
    ) -> SuiResult;

    fn update_epoch_flags_metrics(&self, old: &[EpochFlag], new: &[EpochFlag]);

    fn clear_state_end_of_epoch(&self, execution_guard: &ExecutionLockWriteGuard<'_>);

    fn expensive_check_sui_conservation(
        &self,
        old_epoch_store: &AuthorityPerEpochStore,
    ) -> SuiResult;

    fn checkpoint_db(&self, path: &Path) -> SuiResult;

    /// This is a temporary method to be used when we enable simplified_unwrap_then_delete.
    /// It re-accumulates state hash for the new epoch if simplified_unwrap_then_delete is enabled.
    fn maybe_reaccumulate_state_hash(
        &self,
        cur_epoch_store: &AuthorityPerEpochStore,
        new_protocol_version: ProtocolVersion,
    );
}

// StateSyncAPI is for writing any data that was not the result of transaction execution,
// but that arrived via state sync. The fact that it came via state sync implies that it
// is certified output, and can be immediately persisted to the store.
pub trait StateSyncAPI: Send + Sync {
    fn insert_transaction_and_effects(
        &self,
        transaction: &VerifiedTransaction,
        transaction_effects: &TransactionEffects,
    ) -> SuiResult;

    fn multi_insert_transaction_and_effects(
        &self,
        transactions_and_effects: &[VerifiedExecutionData],
    ) -> SuiResult;
}

// TODO: Remove EffectsNotifyRead trait and just use ExecutionCacheRead directly everywhere.
/// This wrapper is used so that we don't have to disambiguate traits at every callsite.
pub struct NotifyReadWrapper<T>(Arc<T>);

impl<T> Clone for NotifyReadWrapper<T> {
    fn clone(&self) -> Self {
        Self(self.0.clone())
    }
}

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
                    self.find_object_lt_or_eq_version(*child, child_version_upper_bound)?
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

// Implement traits for a cache implementation that always go directly to the store.
macro_rules! implement_passthrough_traits {
    ($implementor: ident) => {
        impl CheckpointCache for $implementor {
            fn deprecated_get_transaction_checkpoint(
                &self,
                digest: &TransactionDigest,
            ) -> SuiResult<Option<(EpochId, CheckpointSequenceNumber)>> {
                self.store.deprecated_get_transaction_checkpoint(digest)
            }

            fn deprecated_multi_get_transaction_checkpoint(
                &self,
                digests: &[TransactionDigest],
            ) -> SuiResult<Vec<Option<(EpochId, CheckpointSequenceNumber)>>> {
                self.store
                    .deprecated_multi_get_transaction_checkpoint(digests)
            }

            fn deprecated_insert_finalized_transactions(
                &self,
                digests: &[TransactionDigest],
                epoch: EpochId,
                sequence: CheckpointSequenceNumber,
            ) -> SuiResult {
                self.store
                    .deprecated_insert_finalized_transactions(digests, epoch, sequence)
            }
        }

        impl ExecutionCacheReconfigAPI for $implementor {
            fn insert_genesis_object(&self, object: Object) -> SuiResult {
                self.store.insert_genesis_object(object)
            }

            fn bulk_insert_genesis_objects(&self, objects: &[Object]) -> SuiResult {
                self.store.bulk_insert_genesis_objects(objects)
            }

            fn revert_state_update(&self, digest: &TransactionDigest) -> SuiResult {
                self.revert_state_update_impl(digest)
            }

            fn set_epoch_start_configuration(
                &self,
                epoch_start_config: &EpochStartConfiguration,
            ) -> SuiResult {
                self.store.set_epoch_start_configuration(epoch_start_config)
            }

            fn update_epoch_flags_metrics(&self, old: &[EpochFlag], new: &[EpochFlag]) {
                self.store.update_epoch_flags_metrics(old, new)
            }

            fn clear_state_end_of_epoch(&self, execution_guard: &ExecutionLockWriteGuard<'_>) {
                self.clear_state_end_of_epoch_impl(execution_guard)
            }

            fn expensive_check_sui_conservation(
                &self,
                old_epoch_store: &AuthorityPerEpochStore,
            ) -> SuiResult {
                self.store
                    .expensive_check_sui_conservation(self, old_epoch_store)
            }

            fn checkpoint_db(&self, path: &std::path::Path) -> SuiResult {
                self.store.perpetual_tables.checkpoint_db(path)
            }

            fn maybe_reaccumulate_state_hash(
                &self,
                cur_epoch_store: &AuthorityPerEpochStore,
                new_protocol_version: ProtocolVersion,
            ) {
                self.store
                    .maybe_reaccumulate_state_hash(cur_epoch_store, new_protocol_version)
            }
        }

        impl StateSyncAPI for $implementor {
            fn insert_transaction_and_effects(
                &self,
                transaction: &VerifiedTransaction,
                transaction_effects: &TransactionEffects,
            ) -> SuiResult {
                Ok(self
                    .store
                    .insert_transaction_and_effects(transaction, transaction_effects)?)
            }

            fn multi_insert_transaction_and_effects(
                &self,
                transactions_and_effects: &[VerifiedExecutionData],
            ) -> SuiResult {
                Ok(self
                    .store
                    .multi_insert_transaction_and_effects(transactions_and_effects.iter())?)
            }
        }
    };
}

use implement_passthrough_traits;

implement_storage_traits!(PassthroughCache);
implement_storage_traits!(WritebackCache);

pub trait ExecutionCacheAPI:
    ExecutionCacheRead
    + ExecutionCacheWrite
    + ExecutionCacheCommit
    + ExecutionCacheReconfigAPI
    + CheckpointCache
    + StateSyncAPI
{
}
