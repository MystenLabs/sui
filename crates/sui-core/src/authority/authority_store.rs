// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::cmp::Ordering;
use std::ops::Not;
use std::path::Path;
use std::sync::Arc;
use std::{iter, mem, thread};

use either::Either;
use fastcrypto::hash::MultisetHash;
use futures::stream::FuturesUnordered;
use move_bytecode_utils::module_cache::GetModule;
use move_core_types::resolver::ModuleResolver;
use once_cell::sync::OnceCell;
use rocksdb::Options;
use serde::{Deserialize, Serialize};
use sui_types::messages_checkpoint::ECMHLiveObjectSetDigest;
use tokio::sync::{RwLock, RwLockReadGuard, RwLockWriteGuard};
use tokio::time::Instant;
use tracing::{debug, info, trace};

use sui_protocol_config::ProtocolConfig;
use sui_storage::mutex_table::{MutexGuard, MutexTable, RwLockGuard, RwLockTable};
use sui_types::accumulator::Accumulator;
use sui_types::digests::TransactionEventsDigest;
use sui_types::error::UserInputError;
use sui_types::message_envelope::Message;
use sui_types::object::Owner;
use sui_types::storage::{
    get_module_by_id, BackingPackageStore, ChildObjectResolver, DeleteKind, ObjectKey, ObjectStore,
};
use sui_types::sui_system_state::get_sui_system_state;
use sui_types::{base_types::SequenceNumber, fp_bail, fp_ensure, storage::ParentSync};
use typed_store::rocks::{DBBatch, TypedStoreError};
use typed_store::traits::Map;

use crate::authority::authority_per_epoch_store::AuthorityPerEpochStore;
use crate::authority::authority_store_types::{
    get_store_object_pair, ObjectContentDigest, StoreObject, StoreObjectPair, StoreObjectWrapper,
};
use crate::authority::epoch_start_configuration::{EpochFlag, EpochStartConfiguration};

use super::authority_store_tables::LiveObject;
use super::{authority_store_tables::AuthorityPerpetualTables, *};
use mysten_common::sync::notify_read::NotifyRead;
use sui_storage::package_object_cache::PackageObjectCache;
use sui_types::effects::{TransactionEffects, TransactionEvents};
use sui_types::gas_coin::TOTAL_SUPPLY_MIST;
use typed_store::rocks::util::is_ref_count_value;

const NUM_SHARDS: usize = 4096;

struct AuthorityStoreMetrics {
    sui_conservation_check_latency: IntGauge,
    sui_conservation_live_object_count: IntGauge,
    sui_conservation_imbalance: IntGauge,
    sui_conservation_storage_fund: IntGauge,
    sui_conservation_storage_fund_imbalance: IntGauge,
    epoch_flags: IntGaugeVec,
}

impl AuthorityStoreMetrics {
    pub fn new(registry: &Registry) -> Self {
        Self {
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
}

pub type ExecutionLockReadGuard<'a> = RwLockReadGuard<'a, EpochId>;
pub type ExecutionLockWriteGuard<'a> = RwLockWriteGuard<'a, EpochId>;

impl AuthorityStore {
    /// Open an authority store by directory path.
    /// If the store is empty, initialize it using genesis.
    pub async fn open(
        path: &Path,
        db_options: Option<Options>,
        genesis: &Genesis,
        committee_store: &Arc<CommitteeStore>,
        indirect_objects_threshold: usize,
        enable_epoch_sui_conservation_check: bool,
        registry: &Registry,
    ) -> SuiResult<Arc<Self>> {
        let perpetual_tables = Arc::new(AuthorityPerpetualTables::open(path, db_options.clone()));
        let epoch_start_configuration = if perpetual_tables.database_is_empty()? {
            let epoch_start_configuration = EpochStartConfiguration::new(
                genesis.sui_system_object().into_epoch_start_state(),
                *genesis.checkpoint().digest(),
            );
            perpetual_tables
                .set_epoch_start_configuration(&epoch_start_configuration)
                .await?;
            epoch_start_configuration
        } else {
            perpetual_tables
                .epoch_start_configuration
                .get(&())?
                .expect("Epoch start configuration must be set in non-empty DB")
        };
        let cur_epoch = perpetual_tables.get_recovery_epoch_at_restart()?;
        let committee = committee_store
            .get_committee(&cur_epoch)?
            .expect("Committee of the current epoch must exist");
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
        path: &Path,
        db_options: Option<Options>,
        committee: &Committee,
        genesis: &Genesis,
        indirect_objects_threshold: usize,
    ) -> SuiResult<Arc<Self>> {
        // TODO: Since we always start at genesis, the committee should be technically the same
        // as the genesis committee.
        assert_eq!(committee.epoch, 0);
        let perpetual_tables = Arc::new(AuthorityPerpetualTables::open(path, db_options.clone()));
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
        });
        // Only initialize an empty database.
        if store
            .database_is_empty()
            .expect("Database read should not fail at init.")
        {
            store
                .bulk_object_insert(&genesis.objects().iter().collect::<Vec<_>>())
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
            // When we are opening the db table, the only time when it's safe to
            // check SUI conservation is at genesis. Otherwise we may be in the middle of
            // an epoch and the SUI conservation check will fail. This also initialize
            // the expected_network_sui_amount table.
            store
                .expensive_check_sui_conservation()
                .expect("SUI conservation check cannot fail at genesis");
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
            .iter()
            .skip_to(&(*event_digest, 0))?
            .take_while(|((digest, _), _)| digest == event_digest)
            .map(|(_, e)| e)
            .collect::<Vec<_>>();
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

    pub fn insert_finalized_transactions(
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

    pub fn is_transaction_executed_in_checkpoint(
        &self,
        digest: &TransactionDigest,
    ) -> SuiResult<bool> {
        Ok(self
            .perpetual_tables
            .executed_transactions_to_checkpoint
            .contains_key(digest)?)
    }

    pub fn get_transaction_checkpoint(
        &self,
        digest: &TransactionDigest,
    ) -> SuiResult<Option<(EpochId, CheckpointSequenceNumber)>> {
        Ok(self
            .perpetual_tables
            .executed_transactions_to_checkpoint
            .get(digest)?)
    }

    pub fn multi_get_transaction_checkpoint(
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
            .iter()
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

    /// Get many objects
    pub fn get_objects(&self, objects: &[ObjectID]) -> Result<Vec<Option<Object>>, SuiError> {
        let mut result = Vec::new();
        for id in objects {
            result.push(self.get_object(id)?);
        }
        Ok(result)
    }

    pub fn check_input_objects(
        &self,
        objects: &[InputObjectKind],
        protocol_config: &ProtocolConfig,
    ) -> Result<Vec<Object>, SuiError> {
        let mut result = Vec::new();

        fp_ensure!(
            objects.len() <= protocol_config.max_input_objects() as usize,
            UserInputError::SizeLimitExceeded {
                limit: "maximum input objects in a transaction".to_string(),
                value: protocol_config.max_input_objects().to_string()
            }
            .into()
        );

        for kind in objects {
            let obj = match kind {
                InputObjectKind::MovePackage(id) | InputObjectKind::SharedMoveObject { id, .. } => {
                    self.get_object(id)?
                }
                InputObjectKind::ImmOrOwnedMoveObject(objref) => {
                    self.get_object_by_key(&objref.0, objref.1)?
                }
            }
            .ok_or_else(|| SuiError::from(kind.object_not_found_error()))?;
            result.push(obj);
        }
        Ok(result)
    }

    /// Gets the input object keys and lock modes from input object kinds, by determining the
    /// versions and types of owned, shared and package objects.
    /// When making changes, please see if check_sequenced_input_objects() below needs
    /// similar changes as well.
    pub fn get_input_object_locks(
        &self,
        digest: &TransactionDigest,
        objects: &[InputObjectKind],
        epoch_store: &AuthorityPerEpochStore,
    ) -> BTreeMap<InputKey, LockMode> {
        let mut shared_locks = HashMap::<ObjectID, SequenceNumber>::new();
        objects
            .iter()
            .map(|kind| {
                match kind {
                    InputObjectKind::SharedMoveObject { id, initial_shared_version: _, mutable } => {
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
                        let lock_mode = if *mutable {
                            LockMode::Default
                        } else {
                            LockMode::ReadOnly
                        };
                        (InputKey(*id, Some(*version)), lock_mode)
                    }
                    // TODO: use ReadOnly lock?
                    InputObjectKind::MovePackage(id) => (InputKey(*id, None), LockMode::Default),
                    // Cannot use ReadOnly lock because we do not know if the object is immutable.
                    InputObjectKind::ImmOrOwnedMoveObject(objref) => (InputKey(objref.0, Some(objref.1)), LockMode::Default),
                }
            })
            .collect()
    }

    /// Checks if the input object identified by the InputKey exists, with support for non-system
    /// packages i.e. when version is None.
    pub fn input_object_exists(&self, key: &InputKey) -> Result<bool, SuiError> {
        match key.1 {
            Some(version) => Ok(self
                .perpetual_tables
                .objects
                .contains_key(&ObjectKey(key.0, version))?),
            None => match self.get_object_or_tombstone(key.0)? {
                None => Ok(false),
                Some(entry) => Ok(entry.2.is_alive()),
            },
        }
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

    /// When making changes, please see if get_input_object_keys() above needs
    /// similar changes as well.
    ///
    /// Before this function is invoked, TransactionManager must ensure all depended
    /// objects are present. Thus any missing object will panic.
    pub fn check_sequenced_input_objects(
        &self,
        digest: &TransactionDigest,
        objects: &[InputObjectKind],
        epoch_store: &AuthorityPerEpochStore,
    ) -> Result<Vec<Object>, SuiError> {
        let shared_locks_cell: OnceCell<HashMap<_, _>> = OnceCell::new();

        let mut result = Vec::new();
        for kind in objects {
            let obj = match kind {
                InputObjectKind::SharedMoveObject { id, .. } => {
                    let shared_locks = shared_locks_cell.get_or_try_init(|| {
                        Ok::<HashMap<ObjectID, SequenceNumber>, SuiError>(
                            epoch_store.get_shared_locks(digest)?.into_iter().collect(),
                        )
                    })?;
                    // If we can't find the locked version, it means
                    // 1. either we have a bug that skips shared object version assignment
                    // 2. or we have some DB corruption
                    let version = shared_locks.get(id).unwrap_or_else(|| {
                        panic!(
                        "Shared object locks should have been set. tx_digset: {:?}, obj id: {:?}",
                        digest, id
                    )
                    });
                    self.get_object_by_key(id, *version)?.unwrap_or_else(|| {
                        panic!("All dependencies of tx {:?} should have been executed now, but Shared Object id: {}, version: {} is absent", digest, *id, *version);
                    })
                }
                InputObjectKind::MovePackage(id) => self.get_object(id)?.unwrap_or_else(|| {
                    panic!("All dependencies of tx {:?} should have been executed now, but Move Package id: {} is absent", digest, id);
                }),
                InputObjectKind::ImmOrOwnedMoveObject(objref) => {
                    self.get_object_by_key(&objref.0, objref.1)?.unwrap_or_else(|| {
                        panic!("All dependencies of tx {:?} should have been executed now, but Immutable or Owned Object id: {}, version: {} is absent", digest, objref.0, objref.1);
                    })
                }
            };
            result.push(obj);
        }
        Ok(result)
    }

    // Methods to mutate the store

    /// Insert a genesis object.
    /// TODO: delete this method entirely (still used by authority_tests.rs)
    pub(crate) fn insert_genesis_object(&self, object: Object) -> SuiResult {
        // We only side load objects with a genesis parent transaction.
        debug_assert!(object.previous_transaction == TransactionDigest::genesis());
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

    /// Insert objects directly into the object table, but do not touch other tables.
    /// This is used in fullnode to insert objects from validators certificate handling response
    /// in fast path execution.
    /// This is best-efforts. If the object needs to be stored as an indirect object then we
    /// do not insert this object at all.
    ///
    /// Caveat: if an Object is regularly inserted as an indirect object in the stiore, but the threshold
    /// changes in the fullnode which causes it to be considered as non-indirect, and only inserted
    /// to the object store, this would cause the reference counting to be incorrect.
    ///
    /// TODO: handle this in a more resilient way.
    pub(crate) fn fullnode_fast_path_insert_objects_to_object_store_maybe(
        &self,
        objects: &Vec<Object>,
    ) -> SuiResult {
        let mut write_batch = self.perpetual_tables.objects.batch();

        for obj in objects {
            let StoreObjectPair(store_object, indirect_object) =
                get_store_object_pair(obj.clone(), self.indirect_objects_threshold);
            // Do not insert to store if the object needs to stored as indirect object too.
            if indirect_object.is_some() {
                continue;
            }
            write_batch.insert_batch(
                &self.perpetual_tables.objects,
                std::iter::once((ObjectKey(obj.id(), obj.version()), store_object)),
            )?;
        }

        write_batch.write()?;
        Ok(())
    }

    /// This function should only be used for initializing genesis and should remain private.
    async fn bulk_object_insert(&self, objects: &[&Object]) -> SuiResult<()> {
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
                        get_store_object_pair((**o).clone(), self.indirect_objects_threshold).0,
                    )
                }),
            )?
            .insert_batch(
                &self.perpetual_tables.indirect_move_objects,
                ref_and_objects.iter().filter_map(|(_, o)| {
                    let StoreObjectPair(_, indirect_object) =
                        get_store_object_pair((**o).clone(), self.indirect_objects_threshold);
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
    pub async fn update_state(
        &self,
        inner_temporary_store: InnerTemporaryStore,
        transaction: &VerifiedTransaction,
        effects: &TransactionEffects,
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
        self.update_objects_and_locks(&mut write_batch, inner_temporary_store)
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

        // test crashing before notifying
        fail_point_async!("crash");

        self.executed_effects_notify_read
            .notify(transaction_digest, effects);

        Ok(())
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
            .iter()
            .filter_map(|(_, (_, object, _))| {
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
    ) -> SuiResult {
        let InnerTemporaryStore {
            objects,
            mutable_inputs: active_inputs,
            written,
            deleted,
            events,
            max_binary_format_version: _,
            loaded_child_objects: _,
            no_extraneous_module_bytes: _,
        } = inner_temporary_store;
        trace!(written =? written.values().map(|((obj_id, ver, _), _, _)| (obj_id, ver)).collect::<Vec<_>>(),
               "batch_update_objects: temp store written");

        let owned_inputs: Vec<_> = active_inputs
            .iter()
            .filter(|(id, _, _)| objects.get(id).unwrap().is_address_owned())
            .cloned()
            .collect();

        write_batch.insert_batch(
            &self.perpetual_tables.objects,
            deleted.iter().map(|(object_id, (version, kind))| {
                let tombstone: StoreObjectWrapper = if *kind == DeleteKind::Wrap {
                    StoreObject::Wrapped.into()
                } else {
                    StoreObject::Deleted.into()
                };
                (ObjectKey(*object_id, *version), tombstone)
            }),
        )?;

        // Insert each output object into the stores
        let (new_objects, new_indirect_move_objects): (Vec<_>, Vec<_>) = written
            .iter()
            .map(|(_, (obj_ref, new_object, _))| {
                debug!(?obj_ref, "writing object");
                let StoreObjectPair(store_object, indirect_object) =
                    get_store_object_pair(new_object.clone(), self.indirect_objects_threshold);
                (
                    (ObjectKey::from(obj_ref), store_object),
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
            .iter()
            .filter_map(|(_, (object_ref, new_object, _kind))| {
                if new_object.is_address_owned() {
                    Some(*object_ref)
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
        self.delete_locks(write_batch, &owned_inputs)
    }

    /// Acquires a lock for a transaction on the given objects if they have all been initialized previously
    /// to None state.  It is also OK if they have been set to the same transaction.
    /// The locks are all set to the given transaction digest.
    /// Returns UserInputError::ObjectNotFound if no lock record can be found for one of the objects.
    /// Returns UserInputError::ObjectVersionUnavailableForConsumption if one of the objects is not locked at the given version.
    /// Returns SuiError::ObjectLockConflict if one of the objects is locked by a different transaction in the same epoch.
    /// Returns SuiError::ObjectLockedAtFutureEpoch if one of the objects is locked in a future epoch (bug).
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
            .iter()
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

        let locks = self
            .perpetual_tables
            .owned_object_transaction_locks
            .multi_get(objects)?;

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

        write_batch.insert_batch(
            &self.perpetual_tables.owned_object_transaction_locks,
            objects.iter().map(|obj_ref| (obj_ref, None)),
        )?;
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
            return Ok(())
        };

        info!(?tx_digest, ?effects, "reverting transaction");

        // We should never be reverting shared object transactions.
        assert!(effects.shared_objects().is_empty());

        let mut write_batch = self.perpetual_tables.transactions.batch();
        write_batch.delete_batch(
            &self.perpetual_tables.executed_effects,
            iter::once(tx_digest),
        )?;
        if let Some(events_digest) = effects.events_digest() {
            write_batch.delete_range(
                &self.perpetual_tables.events,
                &(*events_digest, usize::MIN),
                &(*events_digest, usize::MAX),
            )?;
        }

        let tombstones = effects
            .deleted()
            .iter()
            .chain(effects.wrapped().iter())
            .map(|obj_ref| ObjectKey(obj_ref.0, obj_ref.1));
        write_batch.delete_batch(&self.perpetual_tables.objects, tombstones)?;

        let all_new_object_keys = effects
            .mutated()
            .iter()
            .chain(effects.created().iter())
            .chain(effects.unwrapped().iter())
            .map(|((id, version, _), _)| ObjectKey(*id, *version));
        write_batch.delete_batch(&self.perpetual_tables.objects, all_new_object_keys.clone())?;

        let modified_object_keys = effects
            .modified_at_versions()
            .iter()
            .map(|(id, version)| ObjectKey(*id, *version));

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
                                obj_opt
                                    .expect(&format!("Older object version not found: {:?}", key)),
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
    pub fn get_object_or_tombstone(
        &self,
        object_id: ObjectID,
    ) -> Result<Option<ObjectRef>, SuiError> {
        self.perpetual_tables.get_object_or_tombstone(object_id)
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
    ) -> Result<Vec<Option<VerifiedTransaction>>, SuiError> {
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

    pub fn get_transaction_and_serialized_size(
        &self,
        tx_digest: &TransactionDigest,
    ) -> Result<Option<(VerifiedTransaction, usize)>, TypedStoreError> {
        self.perpetual_tables
            .transactions
            .get_raw_bytes(tx_digest)
            .and_then(|v| match v {
                Some(tx_bytes) => {
                    let tx: VerifiedTransaction =
                        bcs::from_bytes::<TrustedTransaction>(&tx_bytes)?.into();
                    Ok(Some((tx, tx_bytes.len())))
                }
                None => Ok(None),
            })
    }

    // TODO: Transaction Orchestrator also calls this, which is not ideal.
    // Instead of this function use AuthorityEpochStore::epoch_start_configuration() to access this object everywhere
    // besides when we are reading fields for the current epoch
    pub fn get_sui_system_state_object(&self) -> SuiResult<SuiSystemState> {
        get_sui_system_state(self.perpetual_tables.as_ref())
    }

    pub fn iter_live_object_set(&self) -> impl Iterator<Item = LiveObject> + '_ {
        self.perpetual_tables.iter_live_object_set()
    }

    pub fn expensive_check_sui_conservation(self: &Arc<Self>) -> SuiResult {
        if !self.enable_epoch_sui_conservation_check {
            return Ok(());
        }
        let protocol_version = ProtocolVersion::new(
            self.get_sui_system_state_object()
                .expect("Read sui system state object cannot fail")
                .protocol_version(),
        );
        // Prior to gas model v2, SUI conservation is not guaranteed.
        if ProtocolConfig::get_for_version(protocol_version).gas_model_version() <= 1 {
            return Ok(());
        }

        info!("Starting SUI conservation check. This may take a while..");
        let cur_time = Instant::now();
        let mut pending_objects = vec![];
        let mut count = 0;
        let package_cache = PackageObjectCache::new(self.clone());
        let (mut total_sui, mut total_storage_rebate) = thread::scope(|s| {
            let pending_tasks = FuturesUnordered::new();
            for o in self.iter_live_object_set() {
                match o {
                    LiveObject::Normal(object) => {
                        pending_objects.push(object);
                        count += 1;
                        if count % 1_000_000 == 0 {
                            let mut task_objects = vec![];
                            mem::swap(&mut pending_objects, &mut task_objects);
                            let package_cache_clone = package_cache.clone();
                            pending_tasks.push(s.spawn(move || {
                                let mut total_storage_rebate = 0;
                                let mut total_sui = 0;
                                for object in task_objects {
                                    total_storage_rebate += object.storage_rebate;
                                    // get_total_sui includes storage rebate, however all storage rebate is
                                    // also stored in the storage fund, so we need to subtract it here.
                                    total_sui +=
                                        object.get_total_sui(&package_cache_clone).unwrap()
                                            - object.storage_rebate;
                                }
                                if count % 50_000_000 == 0 {
                                    info!("Processed {} objects", count);
                                }
                                (total_sui, total_storage_rebate)
                            }));
                        }
                    }
                    LiveObject::Wrapped(_) => (),
                }
            }
            pending_tasks.into_iter().fold((0, 0), |init, result| {
                let result = result.join().unwrap();
                (init.0 + result.0, init.1 + result.1)
            })
        });
        for object in pending_objects {
            total_storage_rebate += object.storage_rebate;
            total_sui += object.get_total_sui(self).unwrap() - object.storage_rebate;
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
        epoch: EpochId,
        panic: bool,
    ) {
        let live_object_set_hash = accumulator.digest_live_object_set();

        let root_state_hash = self
            .get_root_state_hash(epoch)
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
}

impl BackingPackageStore for AuthorityStore {
    fn get_package_object(&self, package_id: &ObjectID) -> SuiResult<Option<Object>> {
        let package = self.get_object(package_id)?;
        if let Some(obj) = &package {
            fp_ensure!(
                obj.is_package(),
                SuiError::BadObjectType {
                    error: format!("Package expected, Move object found: {package_id}"),
                }
            );
        }
        Ok(package)
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
    fn read_child_object(&self, parent: &ObjectID, child: &ObjectID) -> SuiResult<Option<Object>> {
        let child_object = match self.get_object(child)? {
            None => return Ok(None),
            Some(o) => o,
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
}

impl ParentSync for AuthorityStore {
    fn get_latest_parent_entry_ref(&self, object_id: ObjectID) -> SuiResult<Option<ObjectRef>> {
        self.get_object_or_tombstone(object_id)
    }
}

impl ModuleResolver for AuthorityStore {
    type Error = SuiError;

    fn get_module(&self, module_id: &ModuleId) -> Result<Option<Vec<u8>>, Self::Error> {
        // TODO: We should cache the deserialized modules to avoid
        // fetching from the store / re-deserializing them every time.
        // https://github.com/MystenLabs/sui/issues/809
        Ok(self
            .get_package_object(&ObjectID::from(*module_id.address()))?
            .and_then(|package| {
                // unwrap safe since get_package() ensures it's a package object.
                package
                    .data
                    .try_as_package()
                    .unwrap()
                    .serialized_module_map()
                    .get(module_id.name().as_str())
                    .cloned()
            }))
    }
}

impl GetModule for AuthorityStore {
    type Error = SuiError;
    type Item = CompiledModule;

    fn get_module_by_id(&self, id: &ModuleId) -> anyhow::Result<Option<Self::Item>, Self::Error> {
        get_module_by_id(self, id)
    }
}

/// A wrapper to make Orphan Rule happy
pub struct ResolverWrapper<T: ModuleResolver> {
    pub resolver: Arc<T>,
    pub metrics: Arc<ResolverMetrics>,
}

impl<T: ModuleResolver> ResolverWrapper<T> {
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

impl<T: ModuleResolver> ModuleResolver for ResolverWrapper<T> {
    type Error = T::Error;
    fn get_module(&self, module_id: &ModuleId) -> Result<Option<Vec<u8>>, Self::Error> {
        self.inc_cache_size_gauge();
        self.resolver.get_module(module_id)
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

/// A potential input to a transaction.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct InputKey(pub ObjectID, pub Option<SequenceNumber>);

impl From<&Object> for InputKey {
    fn from(obj: &Object) -> Self {
        if obj.is_package() {
            InputKey(obj.id(), None)
        } else {
            InputKey(obj.id(), Some(obj.version()))
        }
    }
}

/// How a transaction should lock a given input object.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub enum LockMode {
    /// In the default mode, the transaction can acquire the lock whenever the object is available
    /// and there is no pending or executing transaction with ReadOnly locks.
    Default,
    /// In the ReadOnly mode, the transaction can acquire the lock whenever the object is available.
    /// The invariant is that no transaction should have locks on the object in default mode.
    ReadOnly,
}
