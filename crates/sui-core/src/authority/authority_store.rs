// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::cmp::Ordering;
use std::collections::BTreeSet;
use std::ops::Not;
use std::sync::Arc;
use std::{iter, mem, thread};

use crate::authority::authority_per_epoch_store::AuthorityPerEpochStore;
use crate::authority::authority_store_types::{
    get_store_object_pair, ObjectContentDigest, StoreObject, StoreObjectPair, StoreObjectWrapper,
};
use crate::authority::epoch_start_configuration::{EpochFlag, EpochStartConfiguration};
use either::Either;
use fastcrypto::hash::{HashFunction, MultisetHash, Sha3_256};
use futures::stream::FuturesUnordered;
use move_core_types::resolver::ModuleResolver;
use serde::{Deserialize, Serialize};
use sui_storage::mutex_table::{MutexGuard, MutexTable, RwLockGuard, RwLockTable};
use sui_types::accumulator::Accumulator;
use sui_types::digests::TransactionEventsDigest;
use sui_types::error::UserInputError;
use sui_types::message_envelope::Message;
use sui_types::messages_checkpoint::ECMHLiveObjectSetDigest;
use sui_types::object::Owner;
use sui_types::storage::{
    get_module, BackingPackageStore, ChildObjectResolver, InputKey, MarkerValue, ObjectKey,
    ObjectStore, PackageObject,
};
use sui_types::sui_system_state::get_sui_system_state;
use sui_types::{base_types::SequenceNumber, fp_bail, fp_ensure, storage::ParentSync};
use tokio::sync::{RwLock, RwLockReadGuard, RwLockWriteGuard};
use tokio::time::Instant;
use tracing::{debug, info, trace};
use typed_store::rocks::errors::typed_store_err_from_bcs_err;
use typed_store::traits::Map;
use typed_store::{
    rocks::{DBBatch, DBMap},
    TypedStoreError,
};

use super::authority_store_tables::LiveObject;
use super::{authority_store_tables::AuthorityPerpetualTables, *};
use mysten_common::sync::notify_read::NotifyRead;
use sui_storage::package_object_cache::PackageObjectCache;
use sui_types::effects::{TransactionEffects, TransactionEvents};
use sui_types::gas_coin::TOTAL_SUPPLY_MIST;
use typed_store::rocks::util::is_ref_count_value;

const NUM_SHARDS: usize = 4096;

struct AuthorityStoreMetrics {
    pending_notify_read: IntGauge,

    sui_conservation_check_latency: IntGauge,
    sui_conservation_live_object_count: IntGauge,
    sui_conservation_live_object_size: IntGauge,
    sui_conservation_imbalance: IntGauge,
    sui_conservation_storage_fund: IntGauge,
    sui_conservation_storage_fund_imbalance: IntGauge,
    epoch_flags: IntGaugeVec,
}

impl AuthorityStoreMetrics {
    pub fn new(registry: &Registry) -> Self {
        Self {
            pending_notify_read: register_int_gauge_with_registry!(
                "pending_notify_read",
                "Pending notify read requests",
                registry,
            )
                .unwrap(),
            sui_conservation_check_latency: register_int_gauge_with_registry!(
                "sui_conservation_check_latency",
                "Number of seconds took to scan all live objects in the store for SUI conservation check",
                registry,
            ).unwrap(),
            sui_conservation_live_object_count: register_int_gauge_with_registry!(
                "sui_conservation_live_object_count",
                "Number of live objects in the store",
                registry,
            ).unwrap(),
            sui_conservation_live_object_size: register_int_gauge_with_registry!(
                "sui_conservation_live_object_size",
                "Size in bytes of live objects in the store",
                registry,
            ).unwrap(),
            sui_conservation_imbalance: register_int_gauge_with_registry!(
                "sui_conservation_imbalance",
                "Total amount of SUI in the network - 10B * 10^9. This delta shows the amount of imbalance",
                registry,
            ).unwrap(),
            sui_conservation_storage_fund: register_int_gauge_with_registry!(
                "sui_conservation_storage_fund",
                "Storage Fund pool balance (only includes the storage fund proper that represents object storage)",
                registry,
            ).unwrap(),
            sui_conservation_storage_fund_imbalance: register_int_gauge_with_registry!(
                "sui_conservation_storage_fund_imbalance",
                "Imbalance of storage fund, computed with storage_fund_balance - total_object_storage_rebates",
                registry,
            ).unwrap(),
            epoch_flags: register_int_gauge_vec_with_registry!(
                "epoch_flags",
                "Local flags of the currently running epoch",
                &["flag"],
                registry,
            ).unwrap(),
        }
    }
}

/// ALL_OBJ_VER determines whether we want to store all past
/// versions of every object in the store. Authority doesn't store
/// them, but other entities such as replicas will.
/// S is a template on Authority signature state. This allows SuiDataStore to be used on either
/// authorities or non-authorities. Specifically, when storing transactions and effects,
/// S allows SuiDataStore to either store the authority signed version or unsigned version.
pub struct AuthorityStore {
    /// Internal vector of locks to manage concurrent writes to the database
    mutex_table: MutexTable<ObjectDigest>,

    pub(crate) perpetual_tables: Arc<AuthorityPerpetualTables>,

    // Implementation detail to support notify_read_effects().
    pub(crate) executed_effects_notify_read: NotifyRead<TransactionDigest, TransactionEffects>,
    pub(crate) executed_effects_digests_notify_read:
        NotifyRead<TransactionDigest, TransactionEffectsDigest>,

    pub(crate) root_state_notify_read: NotifyRead<EpochId, (CheckpointSequenceNumber, Accumulator)>,
    /// This lock denotes current 'execution epoch'.
    /// Execution acquires read lock, checks certificate epoch and holds it until all writes are complete.
    /// Reconfiguration acquires write lock, changes the epoch and revert all transactions
    /// from previous epoch that are executed but did not make into checkpoint.
    execution_lock: RwLock<EpochId>,

    /// Guards reference count updates to `indirect_move_objects` table
    pub(crate) objects_lock_table: Arc<RwLockTable<ObjectContentDigest>>,

    indirect_objects_threshold: usize,

    /// Whether to enable expensive SUI conservation check at epoch boundaries.
    enable_epoch_sui_conservation_check: bool,

    metrics: AuthorityStoreMetrics,

    package_cache: Arc<PackageObjectCache>,
}

pub type ExecutionLockReadGuard<'a> = RwLockReadGuard<'a, EpochId>;
pub type ExecutionLockWriteGuard<'a> = RwLockWriteGuard<'a, EpochId>;

impl AuthorityStore {
    /// Open an authority store by directory path.
    /// If the store is empty, initialize it using genesis.
    pub async fn open(
        perpetual_tables: Arc<AuthorityPerpetualTables>,
        genesis: &Genesis,
        committee_store: &Arc<CommitteeStore>,
        indirect_objects_threshold: usize,
        enable_epoch_sui_conservation_check: bool,
        registry: &Registry,
    ) -> SuiResult<Arc<Self>> {
        let epoch_start_configuration = if perpetual_tables.database_is_empty()? {
            info!("Creating new epoch start config from genesis");

            let epoch_start_configuration = EpochStartConfiguration::new(
                genesis.sui_system_object().into_epoch_start_state(),
                *genesis.checkpoint().digest(),
                genesis.authenticator_state_obj_initial_shared_version(),
                genesis.randomness_state_obj_initial_shared_version(),
                genesis.bridge_obj_initial_shared_version(),
            );
            perpetual_tables
                .set_epoch_start_configuration(&epoch_start_configuration)
                .await?;
            epoch_start_configuration
        } else {
            info!("Loading epoch start config from DB");
            perpetual_tables
                .epoch_start_configuration
                .get(&())?
                .expect("Epoch start configuration must be set in non-empty DB")
        };
        let cur_epoch = perpetual_tables.get_recovery_epoch_at_restart()?;
        info!("Epoch start config: {:?}", epoch_start_configuration);
        info!("Cur epoch: {:?}", cur_epoch);
        let committee = committee_store
            .get_committee(&cur_epoch)?
            .unwrap_or_else(|| panic!("Committee of the current epoch ({}) must exist", cur_epoch));
        let this = Self::open_inner(
            genesis,
            perpetual_tables,
            &committee,
            indirect_objects_threshold,
            enable_epoch_sui_conservation_check,
            registry,
        )
        .await?;
        this.update_epoch_flags_metrics(&[], epoch_start_configuration.flags());
        Ok(this)
    }

    pub fn update_epoch_flags_metrics(&self, old: &[EpochFlag], new: &[EpochFlag]) {
        for flag in old {
            self.metrics
                .epoch_flags
                .with_label_values(&[&flag.to_string()])
                .set(0);
        }
        for flag in new {
            self.metrics
                .epoch_flags
                .with_label_values(&[&flag.to_string()])
                .set(1);
        }
    }

    pub async fn open_with_committee_for_testing(
        perpetual_tables: Arc<AuthorityPerpetualTables>,
        committee: &Committee,
        genesis: &Genesis,
        indirect_objects_threshold: usize,
    ) -> SuiResult<Arc<Self>> {
        // TODO: Since we always start at genesis, the committee should be technically the same
        // as the genesis committee.
        assert_eq!(committee.epoch, 0);
        Self::open_inner(
            genesis,
            perpetual_tables,
            committee,
            indirect_objects_threshold,
            true,
            &Registry::new(),
        )
        .await
    }

    async fn open_inner(
        genesis: &Genesis,
        perpetual_tables: Arc<AuthorityPerpetualTables>,
        committee: &Committee,
        indirect_objects_threshold: usize,
        enable_epoch_sui_conservation_check: bool,
        registry: &Registry,
    ) -> SuiResult<Arc<Self>> {
        let epoch = committee.epoch;

        let store = Arc::new(Self {
            mutex_table: MutexTable::new(NUM_SHARDS),
            perpetual_tables,
            executed_effects_notify_read: NotifyRead::new(),
            executed_effects_digests_notify_read: NotifyRead::new(),
            root_state_notify_read:
                NotifyRead::<EpochId, (CheckpointSequenceNumber, Accumulator)>::new(),
            execution_lock: RwLock::new(epoch),
            objects_lock_table: Arc::new(RwLockTable::new(NUM_SHARDS)),
            indirect_objects_threshold,
            enable_epoch_sui_conservation_check,
            metrics: AuthorityStoreMetrics::new(registry),
            package_cache: PackageObjectCache::new(),
        });
        // Only initialize an empty database.
        if store
            .database_is_empty()
            .expect("Database read should not fail at init.")
        {
            store
                .bulk_insert_genesis_objects(genesis.objects())
                .await
                .expect("Cannot bulk insert genesis objects");

            // insert txn and effects of genesis
            let transaction = VerifiedTransaction::new_unchecked(genesis.transaction().clone());

            store
                .perpetual_tables
                .transactions
                .insert(transaction.digest(), transaction.serializable_ref())
                .unwrap();

            store
                .perpetual_tables
                .effects
                .insert(&genesis.effects().digest(), genesis.effects())
                .unwrap();
            // We don't insert the effects to executed_effects yet because the genesis tx hasn't but will be executed.
            // This is important for fullnodes to be able to generate indexing data right now.

            let event_digests = genesis.events().digest();
            let events = genesis
                .events()
                .data
                .iter()
                .enumerate()
                .map(|(i, e)| ((event_digests, i), e));
            store.perpetual_tables.events.multi_insert(events).unwrap();
        }

        Ok(store)
    }

    pub fn get_root_state_hash(&self, epoch: EpochId) -> SuiResult<ECMHLiveObjectSetDigest> {
        let acc = self
            .perpetual_tables
            .root_state_hash_by_epoch
            .get(&epoch)?
            .expect("Root state hash for this epoch does not exist");
        Ok(acc.1.digest().into())
    }

    pub fn get_root_state_accumulator(
        &self,
        epoch: EpochId,
    ) -> (CheckpointSequenceNumber, Accumulator) {
        self.perpetual_tables
            .root_state_hash_by_epoch
            .get(&epoch)
            .unwrap()
            .unwrap()
    }

    pub fn get_recovery_epoch_at_restart(&self) -> SuiResult<EpochId> {
        self.perpetual_tables.get_recovery_epoch_at_restart()
    }

    pub fn get_effects(
        &self,
        effects_digest: &TransactionEffectsDigest,
    ) -> SuiResult<Option<TransactionEffects>> {
        Ok(self.perpetual_tables.effects.get(effects_digest)?)
    }

    /// Returns true if we have an effects structure for this transaction digest
    pub fn effects_exists(&self, effects_digest: &TransactionEffectsDigest) -> SuiResult<bool> {
        self.perpetual_tables
            .effects
            .contains_key(effects_digest)
            .map_err(|e| e.into())
    }

    pub(crate) fn get_events(
        &self,
        event_digest: &TransactionEventsDigest,
    ) -> Result<Option<TransactionEvents>, TypedStoreError> {
        let data = self
            .perpetual_tables
            .events
            .safe_range_iter((*event_digest, 0)..=(*event_digest, usize::MAX))
            .map_ok(|(_, event)| event)
            .collect::<Result<Vec<_>, TypedStoreError>>()?;
        Ok(data.is_empty().not().then_some(TransactionEvents { data }))
    }

    pub fn multi_get_events(
        &self,
        event_digests: &[TransactionEventsDigest],
    ) -> SuiResult<Vec<Option<TransactionEvents>>> {
        Ok(event_digests
            .iter()
            .map(|digest| self.get_events(digest))
            .collect::<Result<Vec<_>, _>>()?)
    }

    pub fn multi_get_effects<'a>(
        &self,
        effects_digests: impl Iterator<Item = &'a TransactionEffectsDigest>,
    ) -> SuiResult<Vec<Option<TransactionEffects>>> {
        Ok(self.perpetual_tables.effects.multi_get(effects_digests)?)
    }

    pub fn get_executed_effects(
        &self,
        tx_digest: &TransactionDigest,
    ) -> SuiResult<Option<TransactionEffects>> {
        let effects_digest = self.perpetual_tables.executed_effects.get(tx_digest)?;
        match effects_digest {
            Some(digest) => Ok(self.perpetual_tables.effects.get(&digest)?),
            None => Ok(None),
        }
    }

    /// Given a list of transaction digests, returns a list of the corresponding effects only if they have been
    /// executed. For transactions that have not been executed, None is returned.
    pub fn multi_get_executed_effects_digests(
        &self,
        digests: &[TransactionDigest],
    ) -> SuiResult<Vec<Option<TransactionEffectsDigest>>> {
        Ok(self.perpetual_tables.executed_effects.multi_get(digests)?)
    }

    /// Given a list of transaction digests, returns a list of the corresponding effects only if they have been
    /// executed. For transactions that have not been executed, None is returned.
    pub fn multi_get_executed_effects(
        &self,
        digests: &[TransactionDigest],
    ) -> SuiResult<Vec<Option<TransactionEffects>>> {
        let executed_effects_digests = self.perpetual_tables.executed_effects.multi_get(digests)?;
        let effects = self.multi_get_effects(executed_effects_digests.iter().flatten())?;
        let mut tx_to_effects_map = effects
            .into_iter()
            .flatten()
            .map(|effects| (*effects.transaction_digest(), effects))
            .collect::<HashMap<_, _>>();
        Ok(digests
            .iter()
            .map(|digest| tx_to_effects_map.remove(digest))
            .collect())
    }

    pub fn is_tx_already_executed(&self, digest: &TransactionDigest) -> SuiResult<bool> {
        Ok(self
            .perpetual_tables
            .executed_effects
            .contains_key(digest)?)
    }

    pub fn get_deleted_shared_object_previous_tx_digest(
        &self,
        object_id: &ObjectID,
        version: &SequenceNumber,
        epoch_id: EpochId,
    ) -> Result<Option<TransactionDigest>, TypedStoreError> {
        let object_key = (epoch_id, ObjectKey(*object_id, *version));

        match self
            .perpetual_tables
            .object_per_epoch_marker_table
            .get(&object_key)?
        {
            Some(MarkerValue::SharedDeleted(digest)) => Ok(Some(digest)),
            _ => Ok(None),
        }
    }

    pub fn get_last_shared_object_deletion_info(
        &self,
        object_id: &ObjectID,
        epoch_id: EpochId,
    ) -> SuiResult<Option<(SequenceNumber, TransactionDigest)>> {
        let object_key = ObjectKey::max_for_id(object_id);
        let marker_key = (epoch_id, object_key);

        let marker_entry = self
            .perpetual_tables
            .object_per_epoch_marker_table
            .unbounded_iter()
            .skip_prior_to(&marker_key)?
            .next();
        match marker_entry {
            // Make sure the object was deleted or wrapped.
            Some(((epoch, key), MarkerValue::SharedDeleted(digest))) => {
                // Make sure object id matches and version is >= `version`
                let object_id_matches = key.0 == *object_id;
                // Make sure we don't have a stale epoch for some reason (e.g., a revert)
                let epoch_data_ok = epoch == epoch_id;
                if object_id_matches && epoch_data_ok {
                    Ok(Some((key.1, digest)))
                } else {
                    Ok(None)
                }
            }
            _ => Ok(None),
        }
    }

    /// Returns future containing the state hash for the given epoch
    /// once available
    pub async fn notify_read_root_state_hash(
        &self,
        epoch: EpochId,
    ) -> SuiResult<(CheckpointSequenceNumber, Accumulator)> {
        // We need to register waiters _before_ reading from the database to avoid race conditions
        let registration = self.root_state_notify_read.register_one(&epoch);
        let hash = self.perpetual_tables.root_state_hash_by_epoch.get(&epoch)?;

        let result = match hash {
            // Note that Some() clause also drops registration that is already fulfilled
            Some(ready) => Either::Left(futures::future::ready(ready)),
            None => Either::Right(registration),
        }
        .await;

        Ok(result)
    }

    // DEPRECATED -- use function of same name in AuthorityPerEpochStore
    pub fn deprecated_insert_finalized_transactions(
        &self,
        digests: &[TransactionDigest],
        epoch: EpochId,
        sequence: CheckpointSequenceNumber,
    ) -> SuiResult {
        let mut batch = self
            .perpetual_tables
            .executed_transactions_to_checkpoint
            .batch();
        batch.insert_batch(
            &self.perpetual_tables.executed_transactions_to_checkpoint,
            digests.iter().map(|d| (*d, (epoch, sequence))),
        )?;
        batch.write()?;
        trace!("Transactions {digests:?} finalized at checkpoint {sequence} epoch {epoch}");
        Ok(())
    }

    // DEPRECATED -- use function of same name in AuthorityPerEpochStore
    pub fn deprecated_is_transaction_executed_in_checkpoint(
        &self,
        digest: &TransactionDigest,
    ) -> SuiResult<bool> {
        Ok(self
            .perpetual_tables
            .executed_transactions_to_checkpoint
            .contains_key(digest)?)
    }

    // DEPRECATED -- use function of same name in AuthorityPerEpochStore
    pub fn deprecated_get_transaction_checkpoint(
        &self,
        digest: &TransactionDigest,
    ) -> SuiResult<Option<(EpochId, CheckpointSequenceNumber)>> {
        Ok(self
            .perpetual_tables
            .executed_transactions_to_checkpoint
            .get(digest)?)
    }

    // DEPRECATED -- use function of same name in AuthorityPerEpochStore
    pub fn deprecated_multi_get_transaction_checkpoint(
        &self,
        digests: &[TransactionDigest],
    ) -> SuiResult<Vec<Option<(EpochId, CheckpointSequenceNumber)>>> {
        Ok(self
            .perpetual_tables
            .executed_transactions_to_checkpoint
            .multi_get(digests)?
            .into_iter()
            .collect())
    }

    /// Returns true if there are no objects in the database
    pub fn database_is_empty(&self) -> SuiResult<bool> {
        self.perpetual_tables.database_is_empty()
    }

    /// A function that acquires all locks associated with the objects (in order to avoid deadlocks).
    async fn acquire_locks(&self, input_objects: &[ObjectRef]) -> Vec<MutexGuard> {
        self.mutex_table
            .acquire_locks(input_objects.iter().map(|(_, _, digest)| *digest))
            .await
    }

    pub fn get_object_ref_prior_to_key(
        &self,
        object_id: &ObjectID,
        version: VersionNumber,
    ) -> Result<Option<ObjectRef>, SuiError> {
        let Some(prior_version) = version.one_before() else {
            return Ok(None);
        };
        let mut iterator = self
            .perpetual_tables
            .objects
            .unbounded_iter()
            .skip_prior_to(&ObjectKey(*object_id, prior_version))?;

        if let Some((object_key, value)) = iterator.next() {
            if object_key.0 == *object_id {
                return Ok(Some(
                    self.perpetual_tables.object_reference(&object_key, value)?,
                ));
            }
        }
        Ok(None)
    }

    pub fn multi_get_object_by_key(
        &self,
        object_keys: &[ObjectKey],
    ) -> Result<Vec<Option<Object>>, SuiError> {
        let wrappers = self
            .perpetual_tables
            .objects
            .multi_get(object_keys.to_vec())?;
        let mut ret = vec![];

        for (idx, w) in wrappers.into_iter().enumerate() {
            ret.push(
                w.map(|object| self.perpetual_tables.object(&object_keys[idx], object))
                    .transpose()?
                    .flatten(),
            );
        }
        Ok(ret)
    }

    /// Load a list of objects from the store by object reference.
    /// If they exist in the store, they are returned directly.
    /// If any object missing, we try to figure out the best error to return.
    /// If the object we are asking is currently locked at a future version, we know this
    /// transaction is out-of-date and we return a ObjectVersionUnavailableForConsumption,
    /// which indicates this is not retriable.
    /// Otherwise, we return a ObjectNotFound error, which indicates this is retriable.
    pub fn multi_get_object_with_more_accurate_error_return(
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

    /// Get many objects
    pub fn get_objects(&self, objects: &[ObjectID]) -> Result<Vec<Option<Object>>, SuiError> {
        let mut result = Vec::new();
        for id in objects {
            result.push(self.get_object(id)?);
        }
        Ok(result)
    }

    pub fn have_received_object_at_version(
        &self,
        object_id: &ObjectID,
        version: VersionNumber,
        epoch_id: EpochId,
    ) -> Result<bool, SuiError> {
        let marker_key = (epoch_id, ObjectKey(*object_id, version));
        Ok(self
            .perpetual_tables
            .object_per_epoch_marker_table
            .get(&marker_key)?
            .is_some_and(|marker_value| marker_value == MarkerValue::Received))
    }

    pub fn have_deleted_owned_object_at_version_or_after(
        &self,
        object_id: &ObjectID,
        version: VersionNumber,
        epoch_id: EpochId,
    ) -> Result<bool, SuiError> {
        let object_key = ObjectKey::max_for_id(object_id);
        let marker_key = (epoch_id, object_key);

        // Find the most recent version of the object that was deleted or wrapped.
        // Return true if the version is >= `version`. Otherwise return false.
        let marker_entry = self
            .perpetual_tables
            .object_per_epoch_marker_table
            .unbounded_iter()
            .skip_prior_to(&marker_key)?
            .next();
        match marker_entry {
            Some(((epoch, key), marker)) => {
                // Make sure object id matches and version is >= `version`
                let object_data_ok = key.0 == *object_id && key.1 >= version;
                // Make sure we don't have a stale epoch for some reason (e.g., a revert)
                let epoch_data_ok = epoch == epoch_id;
                // Make sure the object was deleted or wrapped.
                let mark_data_ok = marker == MarkerValue::OwnedDeleted;
                Ok(object_data_ok && epoch_data_ok && mark_data_ok)
            }
            None => Ok(false),
        }
    }

    /// Gets the input object keys from input object kinds, by determining the versions of owned,
    /// shared and package objects.
    /// When making changes, please see if check_sequenced_input_objects() below needs
    /// similar changes as well.
    pub fn get_input_object_keys(
        &self,
        digest: &TransactionDigest,
        objects: &[InputObjectKind],
        epoch_store: &AuthorityPerEpochStore,
    ) -> BTreeSet<InputKey> {
        let mut shared_locks = HashMap::<ObjectID, SequenceNumber>::new();
        objects
            .iter()
            .map(|kind| {
                match kind {
                    InputObjectKind::SharedMoveObject { id, .. } => {
                        if shared_locks.is_empty() {
                            shared_locks = epoch_store
                                .get_shared_locks(digest)
                                .expect("Read from storage should not fail!")
                                .into_iter()
                                .collect();
                        }
                        // If we can't find the locked version, it means
                        // 1. either we have a bug that skips shared object version assignment
                        // 2. or we have some DB corruption
                        let Some(version) = shared_locks.get(id) else {
                            panic!(
                                "Shared object locks should have been set. tx_digset: {digest:?}, obj \
                                id: {id:?}",
                            )
                        };
                        InputKey::VersionedObject{ id: *id, version: *version}
                    }
                    InputObjectKind::MovePackage(id) => InputKey::Package { id: *id },
                    InputObjectKind::ImmOrOwnedMoveObject(objref) => InputKey::VersionedObject {id: objref.0, version: objref.1},
                }
            })
            .collect()
    }

    /// Checks if the input object identified by the InputKey exists, with support for non-system
    /// packages i.e. when version is None. If the input object doesn't exist and it's a receiving
    /// object, we also check if the object exists in the object marker table and view it as
    /// existing if it is in the table.
    #[instrument(level = "trace", skip_all)]
    pub fn multi_input_objects_available(
        &self,
        keys: impl Iterator<Item = InputKey> + Clone,
        receiving_objects: HashSet<InputKey>,
        epoch_store: &AuthorityPerEpochStore,
    ) -> Result<Vec<bool>, SuiError> {
        let (keys_with_version, keys_without_version): (Vec<_>, Vec<_>) = keys
            .enumerate()
            .partition(|(_, key)| key.version().is_some());

        let mut versioned_results = vec![];
        for ((idx, input_key), has_key) in keys_with_version.iter().zip(
            self.perpetual_tables
                .objects
                .multi_contains_keys(
                    keys_with_version
                        .iter()
                        .map(|(_, k)| ObjectKey(k.id(), k.version().unwrap())),
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
                        epoch_store.epoch(),
                    )?;
                versioned_results.push((*idx, is_available));
            } else if self
                .get_deleted_shared_object_previous_tx_digest(
                    &input_key.id(),
                    &input_key.version().unwrap(),
                    epoch_store.epoch(),
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

    /// Attempts to acquire execution lock for an executable transaction.
    /// Returns the lock if the transaction is matching current executed epoch
    /// Returns None otherwise
    pub async fn execution_lock_for_executable_transaction(
        &self,
        transaction: &VerifiedExecutableTransaction,
    ) -> SuiResult<ExecutionLockReadGuard> {
        let lock = self.execution_lock.read().await;
        if *lock == transaction.auth_sig().epoch() {
            Ok(lock)
        } else {
            Err(SuiError::WrongEpoch {
                expected_epoch: *lock,
                actual_epoch: transaction.auth_sig().epoch(),
            })
        }
    }

    pub async fn execution_lock_for_reconfiguration(&self) -> ExecutionLockWriteGuard {
        self.execution_lock.write().await
    }

    // Methods to mutate the store

    /// Insert a genesis object.
    /// TODO: delete this method entirely (still used by authority_tests.rs)
    pub(crate) fn insert_genesis_object(&self, object: Object) -> SuiResult {
        // We only side load objects with a genesis parent transaction.
        debug_assert!(object.previous_transaction == TransactionDigest::genesis_marker());
        let object_ref = object.compute_object_reference();
        self.insert_object_direct(object_ref, &object)
    }

    /// Insert an object directly into the store, and also update relevant tables
    /// NOTE: does not handle transaction lock.
    /// This is used to insert genesis objects
    fn insert_object_direct(&self, object_ref: ObjectRef, object: &Object) -> SuiResult {
        let mut write_batch = self.perpetual_tables.objects.batch();

        // Insert object
        let StoreObjectPair(store_object, indirect_object) =
            get_store_object_pair(object.clone(), self.indirect_objects_threshold);
        write_batch.insert_batch(
            &self.perpetual_tables.objects,
            std::iter::once((ObjectKey::from(object_ref), store_object)),
        )?;
        if let Some(indirect_obj) = indirect_object {
            write_batch.insert_batch(
                &self.perpetual_tables.indirect_move_objects,
                std::iter::once((indirect_obj.inner().digest(), indirect_obj)),
            )?;
        }

        // Update the index
        if object.get_single_owner().is_some() {
            // Only initialize lock for address owned objects.
            if !object.is_child_object() {
                self.initialize_locks_impl(&mut write_batch, &[object_ref], false)?;
            }
        }

        write_batch.write()?;

        Ok(())
    }

    /// NOTE: this function is only to be used for fuzzing and testing. Never use in prod
    pub async fn insert_objects_unsafe_for_testing_only(&self, objects: &[Object]) -> SuiResult {
        self.bulk_insert_genesis_objects(objects).await?;
        self.force_reload_system_packages_into_cache();
        Ok(())
    }

    /// This function should only be used for initializing genesis and should remain private.
    async fn bulk_insert_genesis_objects(&self, objects: &[Object]) -> SuiResult<()> {
        let mut batch = self.perpetual_tables.objects.batch();
        let ref_and_objects: Vec<_> = objects
            .iter()
            .map(|o| (o.compute_object_reference(), o))
            .collect();

        batch
            .insert_batch(
                &self.perpetual_tables.objects,
                ref_and_objects.iter().map(|(oref, o)| {
                    (
                        ObjectKey::from(oref),
                        get_store_object_pair((*o).clone(), self.indirect_objects_threshold).0,
                    )
                }),
            )?
            .insert_batch(
                &self.perpetual_tables.indirect_move_objects,
                ref_and_objects.iter().filter_map(|(_, o)| {
                    let StoreObjectPair(_, indirect_object) =
                        get_store_object_pair((*o).clone(), self.indirect_objects_threshold);
                    indirect_object.map(|obj| (obj.inner().digest(), obj))
                }),
            )?;

        let non_child_object_refs: Vec<_> = ref_and_objects
            .iter()
            .filter(|(_, object)| !object.is_child_object())
            .map(|(oref, _)| *oref)
            .collect();

        self.initialize_locks_impl(
            &mut batch,
            &non_child_object_refs,
            false, // is_force_reset
        )?;

        batch.write()?;

        Ok(())
    }

    pub fn bulk_insert_live_objects(
        perpetual_db: &AuthorityPerpetualTables,
        live_objects: impl Iterator<Item = LiveObject>,
        indirect_objects_threshold: usize,
        expected_sha3_digest: &[u8; 32],
    ) -> SuiResult<()> {
        let mut hasher = Sha3_256::default();
        let mut batch = perpetual_db.objects.batch();
        for object in live_objects {
            hasher.update(object.object_reference().2.inner());
            match object {
                LiveObject::Normal(object) => {
                    let StoreObjectPair(store_object_wrapper, indirect_object) =
                        get_store_object_pair(object.clone(), indirect_objects_threshold);
                    batch.insert_batch(
                        &perpetual_db.objects,
                        std::iter::once((
                            ObjectKey::from(object.compute_object_reference()),
                            store_object_wrapper,
                        )),
                    )?;
                    if let Some(indirect_object) = indirect_object {
                        batch.merge_batch(
                            &perpetual_db.indirect_move_objects,
                            iter::once((indirect_object.inner().digest(), indirect_object)),
                        )?;
                    }
                    if !object.is_child_object() {
                        Self::initialize_locks(
                            &perpetual_db.owned_object_transaction_locks,
                            &mut batch,
                            &[object.compute_object_reference()],
                            false, // is_force_reset
                        )?;
                    }
                }
                LiveObject::Wrapped(object_key) => {
                    batch.insert_batch(
                        &perpetual_db.objects,
                        std::iter::once::<(ObjectKey, StoreObjectWrapper)>((
                            object_key,
                            StoreObject::Wrapped.into(),
                        )),
                    )?;
                }
            }
        }
        let sha3_digest = hasher.finalize().digest;
        if *expected_sha3_digest != sha3_digest {
            error!(
                "Sha does not match! expected: {:?}, actual: {:?}",
                expected_sha3_digest, sha3_digest
            );
            return Err(SuiError::from("Sha does not match"));
        }
        batch.write()?;
        Ok(())
    }

    pub async fn set_epoch_start_configuration(
        &self,
        epoch_start_configuration: &EpochStartConfiguration,
    ) -> SuiResult {
        self.perpetual_tables
            .set_epoch_start_configuration(epoch_start_configuration)
            .await?;
        Ok(())
    }

    pub fn get_epoch_start_configuration(&self) -> SuiResult<Option<EpochStartConfiguration>> {
        Ok(self.perpetual_tables.epoch_start_configuration.get(&())?)
    }

    /// Updates the state resulting from the execution of a certificate.
    ///
    /// Internally it checks that all locks for active inputs are at the correct
    /// version, and then writes objects, certificates, parents and clean up locks atomically.
    #[instrument(level = "debug", skip_all)]
    pub async fn update_state(
        &self,
        inner_temporary_store: InnerTemporaryStore,
        transaction: &VerifiedTransaction,
        effects: &TransactionEffects,
        epoch_id: EpochId,
    ) -> SuiResult {
        let _locks = self
            .acquire_read_locks_for_indirect_objects(&inner_temporary_store)
            .await;
        // Extract the new state from the execution
        let mut write_batch = self.perpetual_tables.transactions.batch();

        // Store the certificate indexed by transaction digest
        let transaction_digest = transaction.digest();
        write_batch.insert_batch(
            &self.perpetual_tables.transactions,
            iter::once((transaction_digest, transaction.serializable_ref())),
        )?;

        // Add batched writes for objects and locks.
        let effects_digest = effects.digest();
        self.update_objects_and_locks(
            &mut write_batch,
            inner_temporary_store,
            effects,
            transaction,
            epoch_id,
        )
        .await?;

        // Store the signed effects of the transaction
        // We can't write this until after sequencing succeeds (which happens in
        // batch_update_objects), as effects_exists is used as a check in many places
        // for "did the tx finish".
        write_batch
            .insert_batch(&self.perpetual_tables.effects, [(effects_digest, effects)])?
            .insert_batch(
                &self.perpetual_tables.executed_effects,
                [(transaction_digest, effects_digest)],
            )?;

        // test crashing before writing the batch
        fail_point_async!("crash");

        // Commit.
        write_batch.write()?;

        if transaction.transaction_data().is_end_of_epoch_tx() {
            // At the end of epoch, since system packages may have been upgraded, force
            // reload them in the cache.
            self.force_reload_system_packages_into_cache();
        }

        // test crashing before notifying
        fail_point_async!("crash");

        self.executed_effects_digests_notify_read
            .notify(transaction_digest, &effects_digest);
        self.executed_effects_notify_read
            .notify(transaction_digest, effects);

        self.metrics
            .pending_notify_read
            .set(self.executed_effects_notify_read.num_pending() as i64);

        debug!(effects_digest = ?effects.digest(), "commit_certificate finished");

        Ok(())
    }

    fn force_reload_system_packages_into_cache(&self) {
        info!("Reload all system packages in the cache");
        self.package_cache
            .force_reload_system_packages(BuiltInFramework::all_package_ids(), self);
    }

    /// Acquires read locks for affected indirect objects
    async fn acquire_read_locks_for_indirect_objects(
        &self,
        inner_temporary_store: &InnerTemporaryStore,
    ) -> Vec<RwLockGuard> {
        // locking is required to avoid potential race conditions with the pruner
        // potential race:
        //   - transaction execution branches to reference count increment
        //   - pruner decrements ref count to 0
        //   - compaction job compresses existing merge values to an empty vector
        //   - tx executor commits ref count increment instead of the full value making object inaccessible
        // read locks are sufficient because ref count increments are safe,
        // concurrent transaction executions produce independent ref count increments and don't corrupt the state
        let digests = inner_temporary_store
            .written
            .values()
            .filter_map(|object| {
                let StoreObjectPair(_, indirect_object) =
                    get_store_object_pair(object.clone(), self.indirect_objects_threshold);
                indirect_object.map(|obj| obj.inner().digest())
            })
            .collect();
        self.objects_lock_table.acquire_read_locks(digests).await
    }

    /// Helper function for updating the objects and locks in the state
    async fn update_objects_and_locks(
        &self,
        write_batch: &mut DBBatch,
        inner_temporary_store: InnerTemporaryStore,
        effects: &TransactionEffects,
        transaction: &VerifiedTransaction,
        epoch_id: EpochId,
    ) -> SuiResult {
        let InnerTemporaryStore {
            input_objects,
            mutable_inputs,
            written,
            events,
            max_binary_format_version: _,
            loaded_runtime_objects: _,
            no_extraneous_module_bytes: _,
            runtime_packages_loaded_from_db: _,
            lamport_version,
        } = inner_temporary_store;
        trace!(written =? written.iter().map(|(obj_id, obj)| (obj_id, obj.version())).collect::<Vec<_>>(),
               "batch_update_objects: temp store written");

        let deleted: HashMap<_, _> = effects.all_tombstones().into_iter().collect();

        // Get the actual set of objects that have been received -- any received
        // object will show up in the modified-at set.
        let received_objects: Vec<_> = {
            let modified_at: HashSet<_> = effects.modified_at_versions().into_iter().collect();
            let possible_to_receive = transaction.transaction_data().receiving_objects();
            possible_to_receive
                .into_iter()
                .filter(|obj_ref| modified_at.contains(&(obj_ref.0, obj_ref.1)))
                .collect()
        };

        // We record any received or deleted objects since they could be pruned, and smear shared
        // object deletions in the marker table. For deleted entries in the marker table we need to
        // make sure we don't accidentally overwrite entries.
        let markers_to_place = {
            let received = received_objects.iter().map(|(object_id, version, _)| {
                (
                    (epoch_id, ObjectKey(*object_id, *version)),
                    MarkerValue::Received,
                )
            });

            let deleted = deleted.into_iter().map(|(object_id, version)| {
                let object_key = (epoch_id, ObjectKey(object_id, version));
                if input_objects
                    .get(&object_id)
                    .is_some_and(|object| object.is_shared())
                {
                    (
                        object_key,
                        MarkerValue::SharedDeleted(*transaction.digest()),
                    )
                } else {
                    (object_key, MarkerValue::OwnedDeleted)
                }
            });

            // We "smear" shared deleted objects in the marker table to allow for proper sequencing
            // of transactions that are submitted after the deletion of the shared object.
            // NB: that we do _not_ smear shared objects that were taken immutably in the
            // transaction.
            let smeared_objects = effects.deleted_mutably_accessed_shared_objects();
            let shared_smears = smeared_objects.into_iter().map(move |object_id| {
                let object_key = (epoch_id, ObjectKey(object_id, lamport_version));
                (
                    object_key,
                    MarkerValue::SharedDeleted(*transaction.digest()),
                )
            });

            received.chain(deleted).chain(shared_smears)
        };

        write_batch.insert_batch(
            &self.perpetual_tables.object_per_epoch_marker_table,
            markers_to_place,
        )?;

        let owned_inputs: Vec<_> = mutable_inputs
            .into_iter()
            .filter_map(|(id, ((version, digest), owner))| {
                owner.is_address_owned().then_some((id, version, digest))
            })
            .collect();

        let tombstones = effects
            .deleted()
            .into_iter()
            .chain(effects.unwrapped_then_deleted())
            .map(|oref| (oref, StoreObject::Deleted))
            .chain(
                effects
                    .wrapped()
                    .into_iter()
                    .map(|oref| (oref, StoreObject::Wrapped)),
            )
            .map(|(oref, store_object)| {
                (
                    ObjectKey::from(oref),
                    StoreObjectWrapper::from(store_object),
                )
            });
        write_batch.insert_batch(&self.perpetual_tables.objects, tombstones)?;

        // Insert each output object into the stores
        let (new_objects, new_indirect_move_objects): (Vec<_>, Vec<_>) = written
            .iter()
            .map(|(id, new_object)| {
                let version = new_object.version();
                debug!(?id, ?version, "writing object");
                let StoreObjectPair(store_object, indirect_object) =
                    get_store_object_pair(new_object.clone(), self.indirect_objects_threshold);
                (
                    (ObjectKey(*id, version), store_object),
                    indirect_object.map(|obj| (obj.inner().digest(), obj)),
                )
            })
            .unzip();

        let indirect_objects: Vec<_> = new_indirect_move_objects.into_iter().flatten().collect();
        let existing_digests = self
            .perpetual_tables
            .indirect_move_objects
            .multi_get_raw_bytes(indirect_objects.iter().map(|(digest, _)| digest))?;
        // split updates to existing and new indirect objects
        // for new objects full merge needs to be triggered. For existing ref count increment is sufficient
        let (existing_indirect_objects, new_indirect_objects): (Vec<_>, Vec<_>) = indirect_objects
            .into_iter()
            .enumerate()
            .partition(|(idx, _)| matches!(&existing_digests[*idx], Some(value) if !is_ref_count_value(value)));

        write_batch.insert_batch(&self.perpetual_tables.objects, new_objects.into_iter())?;
        if !new_indirect_objects.is_empty() {
            write_batch.merge_batch(
                &self.perpetual_tables.indirect_move_objects,
                new_indirect_objects.into_iter().map(|(_, pair)| pair),
            )?;
        }
        if !existing_indirect_objects.is_empty() {
            write_batch.partial_merge_batch(
                &self.perpetual_tables.indirect_move_objects,
                existing_indirect_objects
                    .into_iter()
                    .map(|(_, (digest, _))| (digest, 1_u64.to_le_bytes())),
            )?;
        }

        let event_digest = events.digest();
        let events = events
            .data
            .into_iter()
            .enumerate()
            .map(|(i, e)| ((event_digest, i), e));

        write_batch.insert_batch(&self.perpetual_tables.events, events)?;

        let new_locks_to_init: Vec<_> = written
            .values()
            .filter_map(|new_object| {
                if new_object.is_address_owned() {
                    Some(new_object.compute_object_reference())
                } else {
                    None
                }
            })
            .collect();

        // NOTE: We just check here that locks exist, not that they are locked to a specific TX. Why?
        // 1. Lock existence prevents re-execution of old certs when objects have been upgraded
        // 2. Not all validators lock, just 2f+1, so transaction should proceed regardless
        //    (But the lock should exist which means previous transactions finished)
        // 3. Equivocation possible (different TX) but as long as 2f+1 approves current TX its
        //    fine
        // 4. Locks may have existed when we started processing this tx, but could have since
        //    been deleted by a concurrent tx that finished first. In that case, check if the
        //    tx effects exist.
        self.check_owned_object_locks_exist(&owned_inputs)?;

        self.initialize_locks_impl(write_batch, &new_locks_to_init, false)?;
        self.delete_locks(write_batch, &owned_inputs)?;

        // Make sure to delete the locks for any received objects.
        // Any objects that occur as a `Receiving` argument but have not been received will not
        // have their locks touched.
        self.delete_locks(write_batch, &received_objects)
    }

    /// Acquires a lock for a transaction on the given objects if they have all been initialized previously
    pub(crate) async fn acquire_transaction_locks(
        &self,
        epoch: EpochId,
        owned_input_objects: &[ObjectRef],
        tx_digest: TransactionDigest,
    ) -> SuiResult {
        // Other writers may be attempting to acquire locks on the same objects, so a mutex is
        // required.
        // TODO: replace with optimistic db_transactions (i.e. set lock to tx if none)
        let _mutexes = self.acquire_locks(owned_input_objects).await;

        trace!(?owned_input_objects, "acquire_locks");
        let mut locks_to_write = Vec::new();

        let locks = self
            .perpetual_tables
            .owned_object_transaction_locks
            .multi_get(owned_input_objects)?;

        for ((i, lock), obj_ref) in locks.into_iter().enumerate().zip(owned_input_objects) {
            // The object / version must exist, and therefore lock initialized.
            if lock.is_none() {
                let latest_lock = self.get_latest_lock_for_object_id(obj_ref.0)?;
                fp_bail!(UserInputError::ObjectVersionUnavailableForConsumption {
                    provided_obj_ref: *obj_ref,
                    current_version: latest_lock.1
                }
                .into());
            }
            // Safe to unwrap as it is checked above
            let lock = lock.unwrap().map(|l| l.migrate().into_inner());

            if let Some(LockDetails {
                epoch: previous_epoch,
                tx_digest: previous_tx_digest,
            }) = &lock
            {
                fp_ensure!(
                    &epoch >= previous_epoch,
                    SuiError::ObjectLockedAtFutureEpoch {
                        obj_refs: owned_input_objects.to_vec(),
                        locked_epoch: *previous_epoch,
                        new_epoch: epoch,
                        locked_by_tx: *previous_tx_digest,
                    }
                );
                // Lock already set to different transaction from the same epoch.
                // If the lock is set in a previous epoch, it's ok to override it.
                if previous_epoch == &epoch && previous_tx_digest != &tx_digest {
                    // TODO: add metrics here
                    info!(prev_tx_digest = ?previous_tx_digest,
                          cur_tx_digest = ?tx_digest,
                          "Cannot acquire lock: conflicting transaction!");
                    return Err(SuiError::ObjectLockConflict {
                        obj_ref: *obj_ref,
                        pending_transaction: *previous_tx_digest,
                    });
                }
                if &epoch == previous_epoch {
                    // Exactly the same epoch and same transaction, nothing to lock here.
                    continue;
                } else {
                    info!(prev_epoch =? previous_epoch, cur_epoch =? epoch, "Overriding an old lock from previous epoch");
                    // Fall through and override the old lock.
                }
            }
            let obj_ref = owned_input_objects[i];
            let lock_details = LockDetails { epoch, tx_digest };
            locks_to_write.push((obj_ref, Some(lock_details.into())));
        }

        if !locks_to_write.is_empty() {
            trace!(?locks_to_write, "Writing locks");
            let mut batch = self.perpetual_tables.owned_object_transaction_locks.batch();
            batch.insert_batch(
                &self.perpetual_tables.owned_object_transaction_locks,
                locks_to_write,
            )?;
            batch.write()?;
        }

        Ok(())
    }

    /// Gets ObjectLockInfo that represents state of lock on an object.
    /// Returns UserInputError::ObjectNotFound if cannot find lock record for this object
    pub(crate) fn get_lock(&self, obj_ref: ObjectRef, epoch_id: EpochId) -> SuiLockResult {
        Ok(
            if let Some(lock_info) = self
                .perpetual_tables
                .owned_object_transaction_locks
                .get(&obj_ref)
                .map_err(SuiError::StorageError)?
            {
                match lock_info {
                    Some(lock_info) => {
                        let lock_info = lock_info.migrate().into_inner();
                        match Ord::cmp(&lock_info.epoch, &epoch_id) {
                            // If the object was locked in a previous epoch, we can say that it's
                            // no longer locked and is considered as just Initialized.
                            Ordering::Less => ObjectLockStatus::Initialized,
                            Ordering::Equal => ObjectLockStatus::LockedToTx {
                                locked_by_tx: lock_info,
                            },
                            Ordering::Greater => {
                                return Err(SuiError::ObjectLockedAtFutureEpoch {
                                    obj_refs: vec![obj_ref],
                                    locked_epoch: lock_info.epoch,
                                    new_epoch: epoch_id,
                                    locked_by_tx: lock_info.tx_digest,
                                });
                            }
                        }
                    }
                    None => ObjectLockStatus::Initialized,
                }
            } else {
                ObjectLockStatus::LockedAtDifferentVersion {
                    locked_ref: self.get_latest_lock_for_object_id(obj_ref.0)?,
                }
            },
        )
    }

    /// Returns UserInputError::ObjectNotFound if no lock records found for this object.
    fn get_latest_lock_for_object_id(&self, object_id: ObjectID) -> SuiResult<ObjectRef> {
        let mut iterator = self
            .perpetual_tables
            .owned_object_transaction_locks
            .unbounded_iter()
            // Make the max possible entry for this object ID.
            .skip_prior_to(&(object_id, SequenceNumber::MAX, ObjectDigest::MAX))?;
        Ok(iterator
            .next()
            .and_then(|value| {
                if value.0 .0 == object_id {
                    Some(value)
                } else {
                    None
                }
            })
            .ok_or_else(|| {
                SuiError::from(UserInputError::ObjectNotFound {
                    object_id,
                    version: None,
                })
            })?
            .0)
    }

    /// Checks multiple object locks exist.
    /// Returns UserInputError::ObjectNotFound if cannot find lock record for at least one of the objects.
    /// Returns UserInputError::ObjectVersionUnavailableForConsumption if at least one object lock is not initialized
    ///     at the given version.
    pub fn check_owned_object_locks_exist(&self, objects: &[ObjectRef]) -> SuiResult {
        let locks = self
            .perpetual_tables
            .owned_object_transaction_locks
            .multi_get(objects)?;
        for (lock, obj_ref) in locks.into_iter().zip(objects) {
            if lock.is_none() {
                let latest_lock = self.get_latest_lock_for_object_id(obj_ref.0)?;
                fp_bail!(UserInputError::ObjectVersionUnavailableForConsumption {
                    provided_obj_ref: *obj_ref,
                    current_version: latest_lock.1
                }
                .into());
            }
        }
        Ok(())
    }

    /// Initialize a lock to None (but exists) for a given list of ObjectRefs.
    /// Returns SuiError::ObjectLockAlreadyInitialized if the lock already exists and is locked to a transaction
    fn initialize_locks_impl(
        &self,
        write_batch: &mut DBBatch,
        objects: &[ObjectRef],
        is_force_reset: bool,
    ) -> SuiResult {
        trace!(?objects, "initialize_locks");
        AuthorityStore::initialize_locks(
            &self.perpetual_tables.owned_object_transaction_locks,
            write_batch,
            objects,
            is_force_reset,
        )
    }

    pub fn initialize_locks(
        locks_table: &DBMap<ObjectRef, Option<LockDetailsWrapper>>,
        write_batch: &mut DBBatch,
        objects: &[ObjectRef],
        is_force_reset: bool,
    ) -> SuiResult {
        trace!(?objects, "initialize_locks");

        let locks = locks_table.multi_get(objects)?;

        if !is_force_reset {
            // If any locks exist and are not None, return errors for them
            let existing_locks: Vec<ObjectRef> = locks
                .iter()
                .zip(objects)
                .filter_map(|(lock_opt, objref)| {
                    lock_opt.clone().flatten().map(|_tx_digest| *objref)
                })
                .collect();
            if !existing_locks.is_empty() {
                info!(
                    ?existing_locks,
                    "Cannot initialize locks because some exist already"
                );
                return Err(SuiError::ObjectLockAlreadyInitialized {
                    refs: existing_locks,
                });
            }
        }

        write_batch.insert_batch(locks_table, objects.iter().map(|obj_ref| (obj_ref, None)))?;
        Ok(())
    }

    /// Removes locks for a given list of ObjectRefs.
    fn delete_locks(&self, write_batch: &mut DBBatch, objects: &[ObjectRef]) -> SuiResult {
        trace!(?objects, "delete_locks");
        write_batch.delete_batch(
            &self.perpetual_tables.owned_object_transaction_locks,
            objects.iter(),
        )?;
        Ok(())
    }

    #[cfg(test)]
    pub(crate) fn reset_locks_for_test(
        &self,
        transactions: &[TransactionDigest],
        objects: &[ObjectRef],
        epoch_store: &AuthorityPerEpochStore,
    ) {
        for tx in transactions {
            epoch_store.delete_signed_transaction_for_test(tx);
        }

        let mut batch = self.perpetual_tables.owned_object_transaction_locks.batch();
        batch
            .delete_batch(
                &self.perpetual_tables.owned_object_transaction_locks,
                objects.iter(),
            )
            .unwrap();
        batch.write().unwrap();

        let mut batch = self.perpetual_tables.owned_object_transaction_locks.batch();
        self.initialize_locks_impl(&mut batch, objects, false)
            .unwrap();
        batch.write().unwrap();
    }

    /// This function is called at the end of epoch for each transaction that's
    /// executed locally on the validator but didn't make to the last checkpoint.
    /// The effects of the execution is reverted here.
    /// The following things are reverted:
    /// 1. All new object states are deleted.
    /// 2. owner_index table change is reverted.
    ///
    /// NOTE: transaction and effects are intentionally not deleted. It's
    /// possible that if this node is behind, the network will execute the
    /// transaction in a later epoch. In that case, we need to keep it saved
    /// so that when we receive the checkpoint that includes it from state
    /// sync, we are able to execute the checkpoint.
    /// TODO: implement GC for transactions that are no longer needed.
    pub async fn revert_state_update(&self, tx_digest: &TransactionDigest) -> SuiResult {
        let Some(effects) = self.get_executed_effects(tx_digest)? else {
            debug!("Not reverting {:?} as it was not executed", tx_digest);
            return Ok(());
        };

        info!(?tx_digest, ?effects, "reverting transaction");

        // We should never be reverting shared object transactions.
        assert!(effects.input_shared_objects().is_empty());

        let mut write_batch = self.perpetual_tables.transactions.batch();
        write_batch.delete_batch(
            &self.perpetual_tables.executed_effects,
            iter::once(tx_digest),
        )?;
        if let Some(events_digest) = effects.events_digest() {
            write_batch.schedule_delete_range(
                &self.perpetual_tables.events,
                &(*events_digest, usize::MIN),
                &(*events_digest, usize::MAX),
            )?;
        }

        let tombstones = effects
            .all_tombstones()
            .into_iter()
            .map(|(id, version)| ObjectKey(id, version));
        write_batch.delete_batch(&self.perpetual_tables.objects, tombstones)?;

        let all_new_object_keys = effects
            .all_changed_objects()
            .into_iter()
            .map(|((id, version, _), _, _)| ObjectKey(id, version));
        write_batch.delete_batch(&self.perpetual_tables.objects, all_new_object_keys.clone())?;

        let modified_object_keys = effects
            .modified_at_versions()
            .into_iter()
            .map(|(id, version)| ObjectKey(id, version));

        macro_rules! get_objects_and_locks {
            ($object_keys: expr) => {
                self.perpetual_tables
                    .objects
                    .multi_get($object_keys.clone())?
                    .into_iter()
                    .zip($object_keys)
                    .filter_map(|(obj_opt, key)| {
                        let obj = self
                            .perpetual_tables
                            .object(
                                &key,
                                obj_opt.unwrap_or_else(|| {
                                    panic!("Older object version not found: {:?}", key)
                                }),
                            )
                            .expect("Matching indirect object not found")?;

                        if obj.is_immutable() {
                            return None;
                        }

                        let obj_ref = obj.compute_object_reference();
                        Some(obj.is_address_owned().then_some(obj_ref))
                    })
            };
        }

        let old_locks = get_objects_and_locks!(modified_object_keys);
        let new_locks = get_objects_and_locks!(all_new_object_keys);

        let old_locks: Vec<_> = old_locks.flatten().collect();

        // Re-create old locks.
        self.initialize_locks_impl(&mut write_batch, &old_locks, true)?;

        // Delete new locks
        write_batch.delete_batch(
            &self.perpetual_tables.owned_object_transaction_locks,
            new_locks.flatten(),
        )?;

        write_batch.write()?;

        Ok(())
    }

    /// Return the object with version less then or eq to the provided seq number.
    /// This is used by indexer to find the correct version of dynamic field child object.
    /// We do not store the version of the child object, but because of lamport timestamp,
    /// we know the child must have version number less then or eq to the parent.
    pub fn find_object_lt_or_eq_version(
        &self,
        object_id: ObjectID,
        version: SequenceNumber,
    ) -> Option<Object> {
        self.perpetual_tables
            .find_object_lt_or_eq_version(object_id, version)
    }

    /// Returns the latest object reference we have for this object_id in the objects table.
    ///
    /// The method may also return the reference to a deleted object with a digest of
    /// ObjectDigest::deleted() or ObjectDigest::wrapped() and lamport version
    /// of a transaction that deleted the object.
    /// Note that a deleted object may re-appear if the deletion was the result of the object
    /// being wrapped in another object.
    ///
    /// If no entry for the object_id is found, return None.
    pub fn get_latest_object_ref_or_tombstone(
        &self,
        object_id: ObjectID,
    ) -> Result<Option<ObjectRef>, SuiError> {
        self.perpetual_tables
            .get_latest_object_ref_or_tombstone(object_id)
    }

    /// Returns the latest object reference if and only if the object is still live (i.e. it does
    /// not return tombstones)
    pub fn get_latest_object_ref_if_alive(
        &self,
        object_id: ObjectID,
    ) -> Result<Option<ObjectRef>, SuiError> {
        match self.get_latest_object_ref_or_tombstone(object_id)? {
            Some(objref) if objref.2.is_alive() => Ok(Some(objref)),
            _ => Ok(None),
        }
    }

    /// Returns the latest object we have for this object_id in the objects table.
    ///
    /// If no entry for the object_id is found, return None.
    pub fn get_latest_object_or_tombstone(
        &self,
        object_id: ObjectID,
    ) -> Result<Option<(ObjectKey, StoreObjectWrapper)>, SuiError> {
        self.perpetual_tables
            .get_latest_object_or_tombstone(object_id)
    }

    pub fn insert_transaction_and_effects(
        &self,
        transaction: &VerifiedTransaction,
        transaction_effects: &TransactionEffects,
    ) -> Result<(), TypedStoreError> {
        let mut write_batch = self.perpetual_tables.transactions.batch();
        write_batch
            .insert_batch(
                &self.perpetual_tables.transactions,
                [(transaction.digest(), transaction.serializable_ref())],
            )?
            .insert_batch(
                &self.perpetual_tables.effects,
                [(transaction_effects.digest(), transaction_effects)],
            )?;

        write_batch.write()?;
        Ok(())
    }

    pub fn multi_insert_transaction_and_effects<'a>(
        &self,
        transactions: impl Iterator<Item = &'a VerifiedExecutionData>,
    ) -> Result<(), TypedStoreError> {
        let mut write_batch = self.perpetual_tables.transactions.batch();
        for tx in transactions {
            write_batch
                .insert_batch(
                    &self.perpetual_tables.transactions,
                    [(tx.transaction.digest(), tx.transaction.serializable_ref())],
                )?
                .insert_batch(
                    &self.perpetual_tables.effects,
                    [(tx.effects.digest(), &tx.effects)],
                )?;
        }

        write_batch.write()?;
        Ok(())
    }

    pub fn multi_get_transaction_blocks(
        &self,
        tx_digests: &[TransactionDigest],
    ) -> SuiResult<Vec<Option<VerifiedTransaction>>> {
        Ok(self
            .perpetual_tables
            .transactions
            .multi_get(tx_digests)
            .map(|v| v.into_iter().map(|v| v.map(|v| v.into())).collect())?)
    }

    pub fn get_transaction_block(
        &self,
        tx_digest: &TransactionDigest,
    ) -> Result<Option<VerifiedTransaction>, TypedStoreError> {
        self.perpetual_tables
            .transactions
            .get(tx_digest)
            .map(|v| v.map(|v| v.into()))
    }

    pub fn get_transactions_and_serialized_sizes<'a>(
        &self,
        digests: impl IntoIterator<Item = &'a TransactionDigest>,
    ) -> Result<Vec<Option<(VerifiedTransaction, usize)>>, TypedStoreError> {
        self.perpetual_tables
            .transactions
            .multi_get_raw_bytes(digests)?
            .into_iter()
            .map(|raw_bytes_option| {
                raw_bytes_option
                    .map(|tx_bytes| {
                        let tx: VerifiedTransaction =
                            bcs::from_bytes::<TrustedTransaction>(&tx_bytes)
                                .map_err(typed_store_err_from_bcs_err)?
                                .into();
                        Ok((tx, tx_bytes.len()))
                    })
                    .transpose()
            })
            .collect()
    }

    // TODO: Transaction Orchestrator also calls this, which is not ideal.
    // Instead of this function use AuthorityEpochStore::epoch_start_configuration() to access this object everywhere
    // besides when we are reading fields for the current epoch
    pub fn get_sui_system_state_object(&self) -> SuiResult<SuiSystemState> {
        get_sui_system_state(self.perpetual_tables.as_ref())
    }

    pub fn iter_live_object_set(
        &self,
        include_wrapped_object: bool,
    ) -> impl Iterator<Item = LiveObject> + '_ {
        self.perpetual_tables
            .iter_live_object_set(include_wrapped_object)
    }

    pub fn expensive_check_sui_conservation(
        self: &Arc<Self>,
        old_epoch_store: &AuthorityPerEpochStore,
    ) -> SuiResult {
        if !self.enable_epoch_sui_conservation_check {
            return Ok(());
        }

        let executor = old_epoch_store.executor();
        info!("Starting SUI conservation check. This may take a while..");
        let cur_time = Instant::now();
        let mut pending_objects = vec![];
        let mut count = 0;
        let mut size = 0;
        let (mut total_sui, mut total_storage_rebate) = thread::scope(|s| {
            let pending_tasks = FuturesUnordered::new();
            for o in self.iter_live_object_set(false) {
                match o {
                    LiveObject::Normal(object) => {
                        size += object.object_size_for_gas_metering();
                        count += 1;
                        pending_objects.push(object);
                        if count % 1_000_000 == 0 {
                            let mut task_objects = vec![];
                            mem::swap(&mut pending_objects, &mut task_objects);
                            pending_tasks.push(s.spawn(move || {
                                let mut layout_resolver =
                                    executor.type_layout_resolver(Box::new(self.as_ref()));
                                let mut total_storage_rebate = 0;
                                let mut total_sui = 0;
                                for object in task_objects {
                                    total_storage_rebate += object.storage_rebate;
                                    // get_total_sui includes storage rebate, however all storage rebate is
                                    // also stored in the storage fund, so we need to subtract it here.
                                    total_sui +=
                                        object.get_total_sui(layout_resolver.as_mut()).unwrap()
                                            - object.storage_rebate;
                                }
                                if count % 50_000_000 == 0 {
                                    info!("Processed {} objects", count);
                                }
                                (total_sui, total_storage_rebate)
                            }));
                        }
                    }
                    LiveObject::Wrapped(_) => {
                        unreachable!("Explicitly asked to not include wrapped tombstones")
                    }
                }
            }
            pending_tasks.into_iter().fold((0, 0), |init, result| {
                let result = result.join().unwrap();
                (init.0 + result.0, init.1 + result.1)
            })
        });
        let mut layout_resolver = executor.type_layout_resolver(Box::new(self.as_ref()));
        for object in pending_objects {
            total_storage_rebate += object.storage_rebate;
            total_sui +=
                object.get_total_sui(layout_resolver.as_mut()).unwrap() - object.storage_rebate;
        }
        info!(
            "Scanned {} live objects, took {:?}",
            count,
            cur_time.elapsed()
        );
        self.metrics
            .sui_conservation_live_object_count
            .set(count as i64);
        self.metrics
            .sui_conservation_live_object_size
            .set(size as i64);
        self.metrics
            .sui_conservation_check_latency
            .set(cur_time.elapsed().as_secs() as i64);

        let system_state = self
            .get_sui_system_state_object()
            .expect("Reading sui system state object cannot fail")
            .into_sui_system_state_summary();
        let storage_fund_balance = system_state.storage_fund_total_object_storage_rebates;
        info!(
            "Total SUI amount in the network: {}, storage fund balance: {}, total storage rebate: {} at beginning of epoch {}",
            total_sui, storage_fund_balance, total_storage_rebate, system_state.epoch
        );

        let imbalance = (storage_fund_balance as i64) - (total_storage_rebate as i64);
        self.metrics
            .sui_conservation_storage_fund
            .set(storage_fund_balance as i64);
        self.metrics
            .sui_conservation_storage_fund_imbalance
            .set(imbalance);
        self.metrics
            .sui_conservation_imbalance
            .set((total_sui as i128 - TOTAL_SUPPLY_MIST as i128) as i64);

        if let Some(expected_imbalance) = self
            .perpetual_tables
            .expected_storage_fund_imbalance
            .get(&())
            .expect("DB read cannot fail")
        {
            fp_ensure!(
                imbalance == expected_imbalance,
                SuiError::from(
                    format!(
                        "Inconsistent state detected at epoch {}: total storage rebate: {}, storage fund balance: {}, expected imbalance: {}",
                        system_state.epoch, total_storage_rebate, storage_fund_balance, expected_imbalance
                    ).as_str()
                )
            );
        } else {
            self.perpetual_tables
                .expected_storage_fund_imbalance
                .insert(&(), &imbalance)
                .expect("DB write cannot fail");
        }

        if let Some(expected_sui) = self
            .perpetual_tables
            .expected_network_sui_amount
            .get(&())
            .expect("DB read cannot fail")
        {
            fp_ensure!(
                total_sui == expected_sui,
                SuiError::from(
                    format!(
                        "Inconsistent state detected at epoch {}: total sui: {}, expecting {}",
                        system_state.epoch, total_sui, expected_sui
                    )
                    .as_str()
                )
            );
        } else {
            self.perpetual_tables
                .expected_network_sui_amount
                .insert(&(), &total_sui)
                .expect("DB write cannot fail");
        }

        Ok(())
    }

    pub fn expensive_check_is_consistent_state(
        &self,
        checkpoint_executor: &CheckpointExecutor,
        accumulator: Arc<StateAccumulator>,
        cur_epoch_store: &AuthorityPerEpochStore,
        panic: bool,
    ) {
        let live_object_set_hash = accumulator.digest_live_object_set(
            !cur_epoch_store
                .protocol_config()
                .simplified_unwrap_then_delete(),
        );

        let root_state_hash = self
            .get_root_state_hash(cur_epoch_store.epoch())
            .expect("Retrieving root state hash cannot fail");

        let is_inconsistent = root_state_hash != live_object_set_hash;
        if is_inconsistent {
            if panic {
                panic!(
                    "Inconsistent state detected: root state hash: {:?}, live object set hash: {:?}",
                    root_state_hash, live_object_set_hash
                );
            } else {
                error!(
                    "Inconsistent state detected: root state hash: {:?}, live object set hash: {:?}",
                    root_state_hash, live_object_set_hash
                );
            }
        } else {
            info!("State consistency check passed");
        }

        if !panic {
            checkpoint_executor.set_inconsistent_state(is_inconsistent);
        }
    }

    #[cfg(test)]
    pub async fn prune_objects_immediately_for_testing(
        &self,
        transaction_effects: Vec<TransactionEffects>,
    ) -> anyhow::Result<()> {
        let mut wb = self.perpetual_tables.objects.batch();

        let mut object_keys_to_prune = vec![];
        for effects in &transaction_effects {
            for (object_id, seq_number) in effects.modified_at_versions() {
                info!("Pruning object {:?} version {:?}", object_id, seq_number);
                object_keys_to_prune.push(ObjectKey(object_id, seq_number));
            }
        }

        wb.delete_batch(
            &self.perpetual_tables.objects,
            object_keys_to_prune.into_iter(),
        )?;
        wb.write()?;
        Ok(())
    }

    #[cfg(msim)]
    pub fn remove_all_versions_of_object(&self, object_id: ObjectID) {
        let entries: Vec<_> = self
            .perpetual_tables
            .objects
            .unbounded_iter()
            .filter_map(|(key, _)| if key.0 == object_id { Some(key) } else { None })
            .collect();
        info!("Removing all versions of object: {:?}", entries);
        self.perpetual_tables.objects.multi_remove(entries).unwrap();
    }

    // Counts the number of versions exist in object store for `object_id`. This includes tombstone.
    #[cfg(msim)]
    pub fn count_object_versions(&self, object_id: ObjectID) -> usize {
        self.perpetual_tables
            .objects
            .iter_with_bounds(
                Some(ObjectKey(object_id, VersionNumber::MIN)),
                Some(ObjectKey(object_id, VersionNumber::MAX)),
            )
            .collect::<Vec<_>>()
            .len()
    }
}

impl BackingPackageStore for AuthorityStore {
    fn get_package_object(&self, package_id: &ObjectID) -> SuiResult<Option<PackageObject>> {
        self.package_cache.get_package_object(package_id, self)
    }
}

impl ObjectStore for AuthorityStore {
    /// Read an object and return it, or Ok(None) if the object was not found.
    fn get_object(&self, object_id: &ObjectID) -> Result<Option<Object>, SuiError> {
        self.perpetual_tables.as_ref().get_object(object_id)
    }

    fn get_object_by_key(
        &self,
        object_id: &ObjectID,
        version: VersionNumber,
    ) -> Result<Option<Object>, SuiError> {
        self.perpetual_tables.get_object_by_key(object_id, version)
    }
}

impl ChildObjectResolver for AuthorityStore {
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
        let Some(recv_object) =
            self.get_object_by_key(receiving_object_id, receive_object_at_version)?
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

impl ParentSync for AuthorityStore {
    fn get_latest_parent_entry_ref_deprecated(
        &self,
        object_id: ObjectID,
    ) -> SuiResult<Option<ObjectRef>> {
        self.get_latest_object_ref_or_tombstone(object_id)
    }
}

/// A wrapper to make Orphan Rule happy
pub struct ResolverWrapper<T: BackingPackageStore> {
    pub resolver: Arc<T>,
    pub metrics: Arc<ResolverMetrics>,
}

impl<T: BackingPackageStore> ResolverWrapper<T> {
    pub fn new(resolver: Arc<T>, metrics: Arc<ResolverMetrics>) -> Self {
        metrics.module_cache_size.set(0);
        ResolverWrapper { resolver, metrics }
    }

    fn inc_cache_size_gauge(&self) {
        // reset the gauge after a restart of the cache
        let current = self.metrics.module_cache_size.get();
        self.metrics.module_cache_size.set(current + 1);
    }
}

impl<T: BackingPackageStore> ModuleResolver for ResolverWrapper<T> {
    type Error = SuiError;
    fn get_module(&self, module_id: &ModuleId) -> Result<Option<Vec<u8>>, Self::Error> {
        self.inc_cache_size_gauge();
        get_module(&self.resolver, module_id)
    }
}

pub enum UpdateType {
    Transaction(TransactionEffectsDigest),
    Genesis,
}

pub type SuiLockResult = SuiResult<ObjectLockStatus>;

#[derive(Debug, PartialEq, Eq)]
pub enum ObjectLockStatus {
    Initialized,
    LockedToTx { locked_by_tx: LockDetails },
    LockedAtDifferentVersion { locked_ref: ObjectRef },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum LockDetailsWrapper {
    V1(LockDetailsV1),
}

impl LockDetailsWrapper {
    pub fn migrate(self) -> Self {
        // TODO: when there are multiple versions, we must iteratively migrate from version N to
        // N+1 until we arrive at the latest version
        self
    }

    // Always returns the most recent version. Older versions are migrated to the latest version at
    // read time, so there is never a need to access older versions.
    pub fn inner(&self) -> &LockDetails {
        match self {
            Self::V1(v1) => v1,

            // can remove #[allow] when there are multiple versions
            #[allow(unreachable_patterns)]
            _ => panic!("lock details should have been migrated to latest version at read time"),
        }
    }
    pub fn into_inner(self) -> LockDetails {
        match self {
            Self::V1(v1) => v1,

            // can remove #[allow] when there are multiple versions
            #[allow(unreachable_patterns)]
            _ => panic!("lock details should have been migrated to latest version at read time"),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct LockDetailsV1 {
    pub epoch: EpochId,
    pub tx_digest: TransactionDigest,
}

pub type LockDetails = LockDetailsV1;

impl From<LockDetails> for LockDetailsWrapper {
    fn from(details: LockDetails) -> Self {
        // always use latest version.
        LockDetailsWrapper::V1(details)
    }
}
