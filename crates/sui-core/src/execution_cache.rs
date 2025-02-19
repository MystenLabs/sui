// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::authority::authority_per_epoch_store::AuthorityPerEpochStore;
use crate::authority::authority_store::{ExecutionLockWriteGuard, SuiLockResult};
use crate::authority::backpressure::BackpressureManager;
use crate::authority::epoch_start_configuration::EpochFlag;
use crate::authority::epoch_start_configuration::EpochStartConfiguration;
use crate::authority::AuthorityStore;
use crate::state_accumulator::AccumulatorStore;
use crate::transaction_outputs::TransactionOutputs;
use mysten_common::fatal;
use sui_types::bridge::Bridge;

use futures::{future::BoxFuture, FutureExt};
use prometheus::Registry;
use std::collections::HashSet;
use std::path::Path;
use std::sync::Arc;
use sui_config::ExecutionCacheConfig;
use sui_protocol_config::ProtocolVersion;
use sui_types::base_types::{FullObjectID, VerifiedExecutionData};
use sui_types::digests::{TransactionDigest, TransactionEffectsDigest, TransactionEventsDigest};
use sui_types::effects::{TransactionEffects, TransactionEvents};
use sui_types::error::{SuiError, SuiResult, UserInputError};
use sui_types::executable_transaction::VerifiedExecutableTransaction;
use sui_types::messages_checkpoint::CheckpointSequenceNumber;
use sui_types::object::Object;
use sui_types::storage::{
    BackingPackageStore, BackingStore, ChildObjectResolver, FullObjectKey, MarkerValue, ObjectKey,
    ObjectOrTombstone, ObjectStore, PackageObject, ParentSync,
};
use sui_types::sui_system_state::SuiSystemState;
use sui_types::transaction::{VerifiedSignedTransaction, VerifiedTransaction};
use sui_types::{
    base_types::{EpochId, ObjectID, ObjectRef, SequenceNumber},
    object::Owner,
    storage::InputKey,
};
use tracing::instrument;

pub(crate) mod cache_types;
pub mod metrics;
mod object_locks;
pub mod writeback_cache;

pub use writeback_cache::WritebackCache;

use metrics::ExecutionCacheMetrics;

// If you have Arc<ExecutionCache>, you cannot return a reference to it as
// an &Arc<dyn ExecutionCacheRead> (for example), because the trait object is a fat pointer.
// So, in order to be able to return &Arc<dyn T>, we create all the converted trait objects
// (aka fat pointers) up front and return references to them.
#[derive(Clone)]
pub struct ExecutionCacheTraitPointers {
    pub object_cache_reader: Arc<dyn ObjectCacheRead>,
    pub transaction_cache_reader: Arc<dyn TransactionCacheRead>,
    pub cache_writer: Arc<dyn ExecutionCacheWrite>,
    pub backing_store: Arc<dyn BackingStore + Send + Sync>,
    pub backing_package_store: Arc<dyn BackingPackageStore + Send + Sync>,
    pub object_store: Arc<dyn ObjectStore + Send + Sync>,
    pub reconfig_api: Arc<dyn ExecutionCacheReconfigAPI>,
    pub accumulator_store: Arc<dyn AccumulatorStore>,
    pub checkpoint_cache: Arc<dyn CheckpointCache>,
    pub state_sync_store: Arc<dyn StateSyncAPI>,
    pub cache_commit: Arc<dyn ExecutionCacheCommit>,
    pub testing_api: Arc<dyn TestingAPI>,
}

impl ExecutionCacheTraitPointers {
    pub fn new<T>(cache: Arc<T>) -> Self
    where
        T: ObjectCacheRead
            + TransactionCacheRead
            + ExecutionCacheWrite
            + BackingStore
            + BackingPackageStore
            + ObjectStore
            + ExecutionCacheReconfigAPI
            + AccumulatorStore
            + CheckpointCache
            + StateSyncAPI
            + ExecutionCacheCommit
            + TestingAPI
            + 'static,
    {
        Self {
            object_cache_reader: cache.clone(),
            transaction_cache_reader: cache.clone(),
            cache_writer: cache.clone(),
            backing_store: cache.clone(),
            backing_package_store: cache.clone(),
            object_store: cache.clone(),
            reconfig_api: cache.clone(),
            accumulator_store: cache.clone(),
            checkpoint_cache: cache.clone(),
            state_sync_store: cache.clone(),
            cache_commit: cache.clone(),
            testing_api: cache.clone(),
        }
    }
}

pub fn build_execution_cache(
    cache_config: &ExecutionCacheConfig,
    prometheus_registry: &Registry,
    store: &Arc<AuthorityStore>,
    backpressure_manager: Arc<BackpressureManager>,
) -> ExecutionCacheTraitPointers {
    let execution_cache_metrics = Arc::new(ExecutionCacheMetrics::new(prometheus_registry));

    ExecutionCacheTraitPointers::new(
        WritebackCache::new(
            cache_config,
            store.clone(),
            execution_cache_metrics,
            backpressure_manager,
        )
        .into(),
    )
}

/// Should only be used for sui-tool or tests. Nodes must use build_execution_cache which
/// uses the epoch_start_config to prevent cache impl from switching except at epoch boundaries.
pub fn build_execution_cache_from_env(
    prometheus_registry: &Registry,
    store: &Arc<AuthorityStore>,
) -> ExecutionCacheTraitPointers {
    let execution_cache_metrics = Arc::new(ExecutionCacheMetrics::new(prometheus_registry));

    ExecutionCacheTraitPointers::new(
        WritebackCache::new(
            &Default::default(),
            store.clone(),
            execution_cache_metrics,
            BackpressureManager::new_for_tests(),
        )
        .into(),
    )
}

pub trait ExecutionCacheCommit: Send + Sync {
    /// Durably commit the outputs of the given transactions to the database.
    /// Will be called by CheckpointExecutor to ensure that transaction outputs are
    /// written durably before marking a checkpoint as finalized.
    fn commit_transaction_outputs(
        &self,
        epoch: EpochId,
        digests: &[TransactionDigest],
        // TODO: Delete this parameter once table migration is complete.
        use_object_per_epoch_marker_table_v2: bool,
    );

    /// Durably commit a transaction to the database. Used to store any transactions
    /// that cannot be reconstructed at start-up by consensus replay. Currently the only
    /// case of this is RandomnessStateUpdate.
    fn persist_transaction(&self, transaction: &VerifiedExecutableTransaction);

    // Number of pending uncommitted transactions
    fn approximate_pending_transaction_count(&self) -> u64;
}

pub trait ObjectCacheRead: Send + Sync {
    fn get_package_object(&self, id: &ObjectID) -> SuiResult<Option<PackageObject>>;
    fn force_reload_system_packages(&self, system_package_ids: &[ObjectID]);

    fn get_object(&self, id: &ObjectID) -> Option<Object>;

    fn get_objects(&self, objects: &[ObjectID]) -> Vec<Option<Object>> {
        let mut ret = Vec::with_capacity(objects.len());
        for object_id in objects {
            ret.push(self.get_object(object_id));
        }
        ret
    }

    fn get_latest_object_ref_or_tombstone(&self, object_id: ObjectID) -> Option<ObjectRef>;

    fn get_latest_object_or_tombstone(
        &self,
        object_id: ObjectID,
    ) -> Option<(ObjectKey, ObjectOrTombstone)>;

    fn get_object_by_key(&self, object_id: &ObjectID, version: SequenceNumber) -> Option<Object>;

    fn multi_get_objects_by_key(&self, object_keys: &[ObjectKey]) -> Vec<Option<Object>>;

    fn object_exists_by_key(&self, object_id: &ObjectID, version: SequenceNumber) -> bool;

    fn multi_object_exists_by_key(&self, object_keys: &[ObjectKey]) -> Vec<bool>;

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
        let objects = self
            .multi_get_objects_by_key(&object_refs.iter().map(ObjectKey::from).collect::<Vec<_>>());
        let mut result = Vec::new();
        for (object_opt, object_ref) in objects.into_iter().zip(object_refs) {
            match object_opt {
                None => {
                    let live_objref = self._get_live_objref(object_ref.0)?;
                    let error = if live_objref.1 >= object_ref.1 {
                        UserInputError::ObjectVersionUnavailableForConsumption {
                            provided_obj_ref: *object_ref,
                            current_version: live_objref.1,
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
        // TODO: Delete this parameter once table migration is complete.
        use_object_per_epoch_marker_table_v2: bool,
    ) -> Vec<bool> {
        let (keys_with_version, keys_without_version): (Vec<_>, Vec<_>) = keys
            .iter()
            .enumerate()
            .partition(|(_, key)| key.version().is_some());

        let mut versioned_results = vec![];
        for ((idx, input_key), has_key) in keys_with_version.iter().zip(
            self.multi_object_exists_by_key(
                &keys_with_version
                    .iter()
                    .map(|(_, k)| ObjectKey(k.id().id(), k.version().unwrap()))
                    .collect::<Vec<_>>(),
            )
            .into_iter(),
        ) {
            assert!(
                input_key.version().is_none() || input_key.version().unwrap().is_valid(),
                "Shared objects in cancelled transaction should always be available immediately, 
                 but it appears that transaction manager is waiting for {:?} to become available",
                input_key
            );
            // If the key exists at the specified version, then the object is available.
            if has_key {
                versioned_results.push((*idx, true))
            } else if receiving_objects.contains(input_key) {
                // There could be a more recent version of this object, and the object at the
                // specified version could have already been pruned. In such a case `has_key` will
                // be false, but since this is a receiving object we should mark it as available if
                // we can determine that an object with a version greater than or equal to the
                // specified version exists or was deleted. We will then let mark it as available
                // to let the transaction through so it can fail at execution.
                let is_available = self
                    .get_object(&input_key.id().id())
                    .map(|obj| obj.version() >= input_key.version().unwrap())
                    .unwrap_or(false)
                    || self.have_deleted_fastpath_object_at_version_or_after(
                        input_key.id().id(),
                        input_key.version().unwrap(),
                        epoch,
                        use_object_per_epoch_marker_table_v2,
                    );
                versioned_results.push((*idx, is_available));
            } else if self
                .get_deleted_shared_object_previous_tx_digest(
                    FullObjectKey::new(input_key.id(), input_key.version().unwrap()),
                    epoch,
                    use_object_per_epoch_marker_table_v2,
                )
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
                match self.get_latest_object_ref_or_tombstone(key.id().id()) {
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
        results.into_iter().map(|(_, result)| result).collect()
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

    fn get_lock(&self, obj_ref: ObjectRef, epoch_store: &AuthorityPerEpochStore) -> SuiLockResult;

    // This method is considered "private" - only used by multi_get_objects_with_more_accurate_error_return
    fn _get_live_objref(&self, object_id: ObjectID) -> SuiResult<ObjectRef>;

    // Check that the given set of objects are live at the given version. This is used as a
    // safety check before execution, and could potentially be deleted or changed to a debug_assert
    fn check_owned_objects_are_live(&self, owned_object_refs: &[ObjectRef]) -> SuiResult;

    fn get_sui_system_state_object_unsafe(&self) -> SuiResult<SuiSystemState>;

    fn get_bridge_object_unsafe(&self) -> SuiResult<Bridge>;

    // Marker methods

    /// Get the marker at a specific version
    fn get_marker_value(
        &self,
        object_key: FullObjectKey,
        epoch_id: EpochId,
        // TODO: Delete this parameter once table migration is complete.
        use_object_per_epoch_marker_table_v2: bool,
    ) -> Option<MarkerValue>;

    /// Get the latest marker for a given object.
    fn get_latest_marker(
        &self,
        object_id: FullObjectID,
        epoch_id: EpochId,
        // TODO: Delete this parameter once table migration is complete.
        use_object_per_epoch_marker_table_v2: bool,
    ) -> Option<(SequenceNumber, MarkerValue)>;

    /// If the shared object was deleted, return deletion info for the current live version
    fn get_last_shared_object_deletion_info(
        &self,
        object_id: FullObjectID,
        epoch_id: EpochId,
        // TODO: Delete this parameter once table migration is complete.
        use_object_per_epoch_marker_table_v2: bool,
    ) -> Option<(SequenceNumber, TransactionDigest)> {
        match self.get_latest_marker(object_id, epoch_id, use_object_per_epoch_marker_table_v2) {
            Some((version, MarkerValue::SharedDeleted(digest))) => Some((version, digest)),
            _ => None,
        }
    }

    /// If the shared object was deleted, return deletion info for the specified version.
    fn get_deleted_shared_object_previous_tx_digest(
        &self,
        object_key: FullObjectKey,
        epoch_id: EpochId,
        // TODO: Delete this parameter once table migration is complete.
        use_object_per_epoch_marker_table_v2: bool,
    ) -> Option<TransactionDigest> {
        match self.get_marker_value(object_key, epoch_id, use_object_per_epoch_marker_table_v2) {
            Some(MarkerValue::SharedDeleted(digest)) => Some(digest),
            _ => None,
        }
    }

    fn have_received_object_at_version(
        &self,
        object_key: FullObjectKey,
        epoch_id: EpochId,
        // TODO: Delete this parameter once table migration is complete.
        use_object_per_epoch_marker_table_v2: bool,
    ) -> bool {
        matches!(
            self.get_marker_value(object_key, epoch_id, use_object_per_epoch_marker_table_v2),
            Some(MarkerValue::Received)
        )
    }

    fn have_deleted_fastpath_object_at_version_or_after(
        &self,
        object_id: ObjectID,
        version: SequenceNumber,
        epoch_id: EpochId,
        // TODO: Delete this parameter once table migration is complete.
        use_object_per_epoch_marker_table_v2: bool,
    ) -> bool {
        let full_id = FullObjectID::Fastpath(object_id); // function explicilty assumes "fastpath"
        matches!(
            self.get_latest_marker(full_id, epoch_id, use_object_per_epoch_marker_table_v2),
            Some((marker_version, MarkerValue::OwnedDeleted)) if marker_version >= version
        )
    }

    /// Return the watermark for the highest checkpoint for which we've pruned objects.
    fn get_highest_pruned_checkpoint(&self) -> CheckpointSequenceNumber;
}

pub trait TransactionCacheRead: Send + Sync {
    fn multi_get_transaction_blocks(
        &self,
        digests: &[TransactionDigest],
    ) -> Vec<Option<Arc<VerifiedTransaction>>>;

    fn get_transaction_block(
        &self,
        digest: &TransactionDigest,
    ) -> Option<Arc<VerifiedTransaction>> {
        self.multi_get_transaction_blocks(&[*digest])
            .pop()
            .expect("multi-get must return correct number of items")
    }

    #[instrument(level = "trace", skip_all)]
    fn get_transactions_and_serialized_sizes(
        &self,
        digests: &[TransactionDigest],
    ) -> SuiResult<Vec<Option<(VerifiedTransaction, usize)>>> {
        let txns = self.multi_get_transaction_blocks(digests);
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
    ) -> Vec<Option<TransactionEffectsDigest>>;

    fn is_tx_already_executed(&self, digest: &TransactionDigest) -> bool {
        self.multi_get_executed_effects_digests(&[*digest])
            .pop()
            .expect("multi-get must return correct number of items")
            .is_some()
    }

    fn multi_get_executed_effects(
        &self,
        digests: &[TransactionDigest],
    ) -> Vec<Option<TransactionEffects>> {
        let effects_digests = self.multi_get_executed_effects_digests(digests);
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

        let effects = self.multi_get_effects(&fetch_digests);
        for (i, effects) in fetch_indices.into_iter().zip(effects.into_iter()) {
            results[i] = effects;
        }

        results
    }

    fn get_executed_effects(&self, digest: &TransactionDigest) -> Option<TransactionEffects> {
        self.multi_get_executed_effects(&[*digest])
            .pop()
            .expect("multi-get must return correct number of items")
    }

    fn multi_get_effects(
        &self,
        digests: &[TransactionEffectsDigest],
    ) -> Vec<Option<TransactionEffects>>;

    fn get_effects(&self, digest: &TransactionEffectsDigest) -> Option<TransactionEffects> {
        self.multi_get_effects(&[*digest])
            .pop()
            .expect("multi-get must return correct number of items")
    }

    fn multi_get_events(
        &self,
        event_digests: &[TransactionEventsDigest],
    ) -> Vec<Option<TransactionEvents>>;

    fn get_events(&self, digest: &TransactionEventsDigest) -> Option<TransactionEvents> {
        self.multi_get_events(&[*digest])
            .pop()
            .expect("multi-get must return correct number of items")
    }

    fn notify_read_executed_effects_digests<'a>(
        &'a self,
        digests: &'a [TransactionDigest],
    ) -> BoxFuture<'a, Vec<TransactionEffectsDigest>>;

    /// Wait until the effects of the given transactions are available and return them.
    /// WARNING: If calling this on a transaction that could be reverted, you must be
    /// sure that this function cannot be called during reconfiguration. The best way to
    /// do this is to wrap your future in EpochStore::within_alive_epoch. Holding an
    /// ExecutionLockReadGuard would also prevent reconfig from happening while waiting,
    /// but this is very dangerous, as it could prevent reconfiguration from ever
    /// occurring!
    fn notify_read_executed_effects<'a>(
        &'a self,
        digests: &'a [TransactionDigest],
    ) -> BoxFuture<'a, Vec<TransactionEffects>> {
        async move {
            let digests = self.notify_read_executed_effects_digests(digests).await;
            // once digests are available, effects must be present as well
            self.multi_get_effects(&digests)
                .into_iter()
                .map(|e| e.unwrap_or_else(|| fatal!("digests must exist")))
                .collect()
        }
        .boxed()
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
        // TODO: Delete this parameter once table migration is complete.
        use_object_per_epoch_marker_table_v2: bool,
    );

    /// Attempt to acquire object locks for all of the owned input locks.
    fn acquire_transaction_locks(
        &self,
        epoch_store: &AuthorityPerEpochStore,
        owned_input_objects: &[ObjectRef],
        tx_digest: TransactionDigest,
        signed_transaction: Option<VerifiedSignedTransaction>,
    ) -> SuiResult;
}

pub trait CheckpointCache: Send + Sync {
    // TODO: In addition to the deprecated methods below, this will eventually include access
    // to the CheckpointStore

    // DEPRECATED METHODS
    fn deprecated_get_transaction_checkpoint(
        &self,
        digest: &TransactionDigest,
    ) -> Option<(EpochId, CheckpointSequenceNumber)>;

    fn deprecated_multi_get_transaction_checkpoint(
        &self,
        digests: &[TransactionDigest],
    ) -> Vec<Option<(EpochId, CheckpointSequenceNumber)>>;

    fn deprecated_insert_finalized_transactions(
        &self,
        digests: &[TransactionDigest],
        epoch: EpochId,
        sequence: CheckpointSequenceNumber,
    );
}

pub trait ExecutionCacheReconfigAPI: Send + Sync {
    fn insert_genesis_object(&self, object: Object);
    fn bulk_insert_genesis_objects(&self, objects: &[Object]);

    fn revert_state_update(&self, digest: &TransactionDigest);
    fn set_epoch_start_configuration(&self, epoch_start_config: &EpochStartConfiguration);

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

    /// Reconfigure the cache itself.
    /// TODO: this is only needed for ProxyCache to switch between cache impls. It can be removed
    /// once WritebackCache is the sole cache impl.
    fn reconfigure_cache<'a>(
        &'a self,
        epoch_start_config: &'a EpochStartConfiguration,
    ) -> BoxFuture<'a, ()>;
}

// StateSyncAPI is for writing any data that was not the result of transaction execution,
// but that arrived via state sync. The fact that it came via state sync implies that it
// is certified output, and can be immediately persisted to the store.
pub trait StateSyncAPI: Send + Sync {
    fn insert_transaction_and_effects(
        &self,
        transaction: &VerifiedTransaction,
        transaction_effects: &TransactionEffects,
    );

    fn multi_insert_transaction_and_effects(
        &self,
        transactions_and_effects: &[VerifiedExecutionData],
    );
}

pub trait TestingAPI: Send + Sync {
    fn database_for_testing(&self) -> Arc<AuthorityStore>;
}

macro_rules! implement_storage_traits {
    ($implementor: ident) => {
        impl ObjectStore for $implementor {
            fn get_object(&self, object_id: &ObjectID) -> Option<Object> {
                ObjectCacheRead::get_object(self, object_id)
            }

            fn get_object_by_key(
                &self,
                object_id: &ObjectID,
                version: sui_types::base_types::VersionNumber,
            ) -> Option<Object> {
                ObjectCacheRead::get_object_by_key(self, object_id, version)
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
                        actual_owner: child_object.owner.clone(),
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
                // TODO: Delete this parameter once table migration is complete.
                use_object_per_epoch_marker_table_v2: bool,
            ) -> SuiResult<Option<Object>> {
                let Some(recv_object) = ObjectCacheRead::get_object_by_key(
                    self,
                    receiving_object_id,
                    receive_object_at_version,
                ) else {
                    return Ok(None);
                };

                // Check for:
                // * Invalid access -- treat as the object does not exist. Or;
                // * If we've already received the object at the version -- then treat it as though it doesn't exist.
                // These two cases must remain indisguishable to the caller otherwise we risk forks in
                // transaction replay due to possible reordering of transactions during replay.
                if recv_object.owner != Owner::AddressOwner((*owner).into())
                    || self.have_received_object_at_version(
                        // TODO: Add support for receiving ConsensusV2 objects. For now this assumes fastpath.
                        FullObjectKey::new(
                            FullObjectID::new(*receiving_object_id, None),
                            receive_object_at_version,
                        ),
                        epoch_id,
                        use_object_per_epoch_marker_table_v2,
                    )
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
                ObjectCacheRead::get_package_object(self, package_id)
            }
        }

        impl ParentSync for $implementor {
            fn get_latest_parent_entry_ref_deprecated(
                &self,
                object_id: ObjectID,
            ) -> Option<ObjectRef> {
                ObjectCacheRead::get_latest_object_ref_or_tombstone(self, object_id)
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
            ) -> Option<(EpochId, CheckpointSequenceNumber)> {
                self.store
                    .deprecated_get_transaction_checkpoint(digest)
                    .expect("db error")
            }

            fn deprecated_multi_get_transaction_checkpoint(
                &self,
                digests: &[TransactionDigest],
            ) -> Vec<Option<(EpochId, CheckpointSequenceNumber)>> {
                self.store
                    .deprecated_multi_get_transaction_checkpoint(digests)
                    .expect("db error")
            }

            fn deprecated_insert_finalized_transactions(
                &self,
                digests: &[TransactionDigest],
                epoch: EpochId,
                sequence: CheckpointSequenceNumber,
            ) {
                self.store
                    .deprecated_insert_finalized_transactions(digests, epoch, sequence)
                    .expect("db error");
            }
        }

        impl ExecutionCacheReconfigAPI for $implementor {
            fn insert_genesis_object(&self, object: Object) {
                self.insert_genesis_object_impl(object)
            }

            fn bulk_insert_genesis_objects(&self, objects: &[Object]) {
                self.bulk_insert_genesis_objects_impl(objects)
            }

            fn revert_state_update(&self, digest: &TransactionDigest) {
                self.revert_state_update_impl(digest)
            }

            fn set_epoch_start_configuration(&self, epoch_start_config: &EpochStartConfiguration) {
                self.store
                    .set_epoch_start_configuration(epoch_start_config)
                    .expect("db error");
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

            fn reconfigure_cache<'a>(
                &'a self,
                _: &'a EpochStartConfiguration,
            ) -> BoxFuture<'a, ()> {
                // Since we now use WritebackCache directly at startup (if the epoch flag is set),
                // this can be called at reconfiguration time. It is a no-op.
                // TODO: remove this once we completely remove ProxyCache.
                std::future::ready(()).boxed()
            }
        }

        impl TestingAPI for $implementor {
            fn database_for_testing(&self) -> Arc<AuthorityStore> {
                self.store.clone()
            }
        }
    };
}

use implement_passthrough_traits;

implement_storage_traits!(WritebackCache);

pub trait ExecutionCacheAPI:
    ObjectCacheRead
    + ExecutionCacheWrite
    + ExecutionCacheCommit
    + ExecutionCacheReconfigAPI
    + CheckpointCache
    + StateSyncAPI
{
}
