// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::ops::Not;
use std::sync::Arc;
use std::{iter, mem, thread};

use crate::authority::authority_per_epoch_store::AuthorityPerEpochStore;
use crate::authority::authority_store_pruner::{
    AuthorityStorePruner, AuthorityStorePruningMetrics, EPOCH_DURATION_MS_FOR_TESTING,
};
use crate::authority::authority_store_types::{get_store_object, StoreObject, StoreObjectWrapper};
use crate::authority::epoch_start_configuration::{EpochFlag, EpochStartConfiguration};
use crate::rpc_index::RpcIndexStore;
use crate::state_accumulator::AccumulatorStore;
use crate::transaction_outputs::TransactionOutputs;
use either::Either;
use fastcrypto::hash::{HashFunction, MultisetHash, Sha3_256};
use futures::stream::FuturesUnordered;
use itertools::izip;
use move_core_types::resolver::ModuleResolver;
use serde::{Deserialize, Serialize};
use sui_config::node::AuthorityStorePruningConfig;
use sui_macros::fail_point_arg;
use sui_storage::mutex_table::{MutexGuard, MutexTable};
use sui_types::accumulator::Accumulator;
use sui_types::digests::TransactionEventsDigest;
use sui_types::error::UserInputError;
use sui_types::execution::TypeLayoutStore;
use sui_types::message_envelope::Message;
use sui_types::storage::{
    get_module, BackingPackageStore, FullObjectKey, MarkerValue, ObjectKey, ObjectOrTombstone,
    ObjectStore,
};
use sui_types::sui_system_state::get_sui_system_state;
use sui_types::{base_types::SequenceNumber, fp_bail, fp_ensure};
use tokio::time::Instant;
use tracing::{debug, info, trace};
use typed_store::traits::Map;
use typed_store::{
    rocks::{DBBatch, DBMap},
    TypedStoreError,
};

use super::authority_store_tables::LiveObject;
use super::{authority_store_tables::AuthorityPerpetualTables, *};
use mysten_common::sync::notify_read::NotifyRead;
use sui_types::effects::{TransactionEffects, TransactionEvents};
use sui_types::gas_coin::TOTAL_SUPPLY_MIST;

const NUM_SHARDS: usize = 4096;

struct AuthorityStoreMetrics {
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

    pub(crate) root_state_notify_read: NotifyRead<EpochId, (CheckpointSequenceNumber, Accumulator)>,

    /// Whether to enable expensive SUI conservation check at epoch boundaries.
    enable_epoch_sui_conservation_check: bool,

    metrics: AuthorityStoreMetrics,
}

pub type ExecutionLockReadGuard<'a> = tokio::sync::RwLockReadGuard<'a, EpochId>;
pub type ExecutionLockWriteGuard<'a> = tokio::sync::RwLockWriteGuard<'a, EpochId>;

impl AuthorityStore {
    /// Open an authority store by directory path.
    /// If the store is empty, initialize it using genesis.
    pub async fn open(
        perpetual_tables: Arc<AuthorityPerpetualTables>,
        genesis: &Genesis,
        config: &NodeConfig,
        registry: &Registry,
    ) -> SuiResult<Arc<Self>> {
        let enable_epoch_sui_conservation_check = config
            .expensive_safety_check_config
            .enable_epoch_sui_conservation_check();

        let epoch_start_configuration = if perpetual_tables.database_is_empty()? {
            info!("Creating new epoch start config from genesis");

            #[allow(unused_mut)]
            let mut initial_epoch_flags = EpochFlag::default_flags_for_new_epoch(config);
            fail_point_arg!("initial_epoch_flags", |flags: Vec<EpochFlag>| {
                info!("Setting initial epoch flags to {:?}", flags);
                initial_epoch_flags = flags;
            });

            let epoch_start_configuration = EpochStartConfiguration::new(
                genesis.sui_system_object().into_epoch_start_state(),
                *genesis.checkpoint().digest(),
                &genesis.objects(),
                initial_epoch_flags,
            )?;
            perpetual_tables.set_epoch_start_configuration(&epoch_start_configuration)?;
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
        let this = Self::open_inner(
            genesis,
            perpetual_tables,
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

    // NB: This must only be called at time of reconfiguration. We take the execution lock write
    // guard as an argument to ensure that this is the case.
    pub fn clear_object_per_epoch_marker_table(
        &self,
        _execution_guard: &ExecutionLockWriteGuard<'_>,
    ) -> SuiResult<()> {
        // We can safely delete all entries in the per epoch marker table since this is only called
        // at epoch boundaries (during reconfiguration). Therefore any entries that currently
        // exist can be removed. Because of this we can use the `schedule_delete_all` method.
        self.perpetual_tables
            .object_per_epoch_marker_table
            .schedule_delete_all()?;
        Ok(self
            .perpetual_tables
            .object_per_epoch_marker_table_v2
            .schedule_delete_all()?)
    }

    pub async fn open_with_committee_for_testing(
        perpetual_tables: Arc<AuthorityPerpetualTables>,
        committee: &Committee,
        genesis: &Genesis,
    ) -> SuiResult<Arc<Self>> {
        // TODO: Since we always start at genesis, the committee should be technically the same
        // as the genesis committee.
        assert_eq!(committee.epoch, 0);
        Self::open_inner(genesis, perpetual_tables, true, &Registry::new()).await
    }

    async fn open_inner(
        genesis: &Genesis,
        perpetual_tables: Arc<AuthorityPerpetualTables>,
        enable_epoch_sui_conservation_check: bool,
        registry: &Registry,
    ) -> SuiResult<Arc<Self>> {
        let store = Arc::new(Self {
            mutex_table: MutexTable::new(NUM_SHARDS),
            perpetual_tables,
            root_state_notify_read:
                NotifyRead::<EpochId, (CheckpointSequenceNumber, Accumulator)>::new(),
            enable_epoch_sui_conservation_check,
            metrics: AuthorityStoreMetrics::new(registry),
        });
        // Only initialize an empty database.
        if store
            .database_is_empty()
            .expect("Database read should not fail at init.")
        {
            store
                .bulk_insert_genesis_objects(genesis.objects())
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

    /// Open authority store without any operations that require
    /// genesis, such as constructing EpochStartConfiguration
    /// or inserting genesis objects.
    pub fn open_no_genesis(
        perpetual_tables: Arc<AuthorityPerpetualTables>,
        enable_epoch_sui_conservation_check: bool,
        registry: &Registry,
    ) -> SuiResult<Arc<Self>> {
        let store = Arc::new(Self {
            mutex_table: MutexTable::new(NUM_SHARDS),
            perpetual_tables,
            root_state_notify_read:
                NotifyRead::<EpochId, (CheckpointSequenceNumber, Accumulator)>::new(),
            enable_epoch_sui_conservation_check,
            metrics: AuthorityStoreMetrics::new(registry),
        });
        Ok(store)
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

    pub fn get_events(
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

    pub fn get_marker_value(
        &self,
        object_key: FullObjectKey,
        epoch_id: EpochId,
        // TODO: Delete this parameter once table migration is complete.
        use_object_per_epoch_marker_table_v2: bool,
    ) -> SuiResult<Option<MarkerValue>> {
        if use_object_per_epoch_marker_table_v2 {
            Ok(self
                .perpetual_tables
                .object_per_epoch_marker_table_v2
                .get(&(epoch_id, object_key))?)
        } else {
            Ok(self
                .perpetual_tables
                .object_per_epoch_marker_table
                .get(&(epoch_id, object_key.into_object_key()))?)
        }
    }

    pub fn get_latest_marker(
        &self,
        object_id: FullObjectID,
        epoch_id: EpochId,
        use_object_per_epoch_marker_table_v2: bool,
    ) -> SuiResult<Option<(SequenceNumber, MarkerValue)>> {
        if use_object_per_epoch_marker_table_v2 {
            let min_key = (epoch_id, FullObjectKey::min_for_id(&object_id));
            let max_key = (epoch_id, FullObjectKey::max_for_id(&object_id));

            let marker_entry = self
                .perpetual_tables
                .object_per_epoch_marker_table_v2
                .safe_iter_with_bounds(Some(min_key), Some(max_key))
                .skip_prior_to(&max_key)?
                .next();
            match marker_entry {
                Some(Ok(((epoch, key), marker))) => {
                    // because of the iterator bounds these cannot fail
                    assert_eq!(epoch, epoch_id);
                    assert_eq!(key.id(), object_id);
                    Ok(Some((key.version(), marker)))
                }
                Some(Err(e)) => Err(e.into()),
                None => Ok(None),
            }
        } else {
            let min_key = (epoch_id, ObjectKey::min_for_id(&object_id.id()));
            let max_key = (epoch_id, ObjectKey::max_for_id(&object_id.id()));

            let marker_entry = self
                .perpetual_tables
                .object_per_epoch_marker_table
                .safe_iter_with_bounds(Some(min_key), Some(max_key))
                .skip_prior_to(&max_key)?
                .next();
            match marker_entry {
                Some(Ok(((epoch, key), marker))) => {
                    // because of the iterator bounds these cannot fail
                    assert_eq!(epoch, epoch_id);
                    assert_eq!(key.0, object_id.id());
                    Ok(Some((key.1, marker)))
                }
                Some(Err(e)) => Err(e.into()),
                None => Ok(None),
            }
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
    fn acquire_locks(&self, input_objects: &[ObjectRef]) -> Vec<MutexGuard> {
        self.mutex_table
            .acquire_locks(input_objects.iter().map(|(_, _, digest)| *digest))
    }

    pub fn object_exists_by_key(
        &self,
        object_id: &ObjectID,
        version: VersionNumber,
    ) -> SuiResult<bool> {
        Ok(self
            .perpetual_tables
            .objects
            .contains_key(&ObjectKey(*object_id, version))?)
    }

    pub fn multi_object_exists_by_key(&self, object_keys: &[ObjectKey]) -> SuiResult<Vec<bool>> {
        Ok(self
            .perpetual_tables
            .objects
            .multi_contains_keys(object_keys.to_vec())?
            .into_iter()
            .collect())
    }

    fn get_object_ref_prior_to_key(
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

    pub fn multi_get_objects_by_key(
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
            result.push(self.get_object(id));
        }
        Ok(result)
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
        let store_object = get_store_object(object.clone());
        write_batch.insert_batch(
            &self.perpetual_tables.objects,
            std::iter::once((ObjectKey::from(object_ref), store_object)),
        )?;

        // Update the index
        if object.get_single_owner().is_some() {
            // Only initialize lock for address owned objects.
            if !object.is_child_object() {
                self.initialize_live_object_markers_impl(&mut write_batch, &[object_ref], false)?;
            }
        }

        write_batch.write()?;

        Ok(())
    }

    /// This function should only be used for initializing genesis and should remain private.
    #[instrument(level = "debug", skip_all)]
    pub(crate) fn bulk_insert_genesis_objects(&self, objects: &[Object]) -> SuiResult<()> {
        let mut batch = self.perpetual_tables.objects.batch();
        let ref_and_objects: Vec<_> = objects
            .iter()
            .map(|o| (o.compute_object_reference(), o))
            .collect();

        batch.insert_batch(
            &self.perpetual_tables.objects,
            ref_and_objects
                .iter()
                .map(|(oref, o)| (ObjectKey::from(oref), get_store_object((*o).clone()))),
        )?;

        let non_child_object_refs: Vec<_> = ref_and_objects
            .iter()
            .filter(|(_, object)| !object.is_child_object())
            .map(|(oref, _)| *oref)
            .collect();

        self.initialize_live_object_markers_impl(
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
        expected_sha3_digest: &[u8; 32],
    ) -> SuiResult<()> {
        let mut hasher = Sha3_256::default();
        let mut batch = perpetual_db.objects.batch();
        for object in live_objects {
            hasher.update(object.object_reference().2.inner());
            match object {
                LiveObject::Normal(object) => {
                    let store_object_wrapper = get_store_object(object.clone());
                    batch.insert_batch(
                        &perpetual_db.objects,
                        std::iter::once((
                            ObjectKey::from(object.compute_object_reference()),
                            store_object_wrapper,
                        )),
                    )?;
                    if !object.is_child_object() {
                        Self::initialize_live_object_markers(
                            &perpetual_db.live_owned_object_markers,
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

    pub fn set_epoch_start_configuration(
        &self,
        epoch_start_configuration: &EpochStartConfiguration,
    ) -> SuiResult {
        self.perpetual_tables
            .set_epoch_start_configuration(epoch_start_configuration)?;
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
    pub fn write_transaction_outputs(
        &self,
        epoch_id: EpochId,
        tx_outputs: &[Arc<TransactionOutputs>],
        // TODO: Delete this parameter once table migration is complete.
        use_object_per_epoch_marker_table_v2: bool,
    ) -> SuiResult {
        let mut written = Vec::with_capacity(tx_outputs.len());
        for outputs in tx_outputs {
            written.extend(outputs.written.values().cloned());
        }

        let mut write_batch = self.perpetual_tables.transactions.batch();
        for outputs in tx_outputs {
            self.write_one_transaction_outputs(
                &mut write_batch,
                epoch_id,
                outputs,
                use_object_per_epoch_marker_table_v2,
            )?;
        }
        // test crashing before writing the batch
        fail_point!("crash");

        write_batch.write()?;
        trace!(
            "committed transactions: {:?}",
            tx_outputs
                .iter()
                .map(|tx| tx.transaction.digest())
                .collect::<Vec<_>>()
        );

        // test crashing before notifying
        fail_point!("crash");

        Ok(())
    }

    fn write_one_transaction_outputs(
        &self,
        write_batch: &mut DBBatch,
        epoch_id: EpochId,
        tx_outputs: &TransactionOutputs,
        // TODO: Delete this parameter once table migration is complete.
        use_object_per_epoch_marker_table_v2: bool,
    ) -> SuiResult {
        let TransactionOutputs {
            transaction,
            effects,
            markers,
            wrapped,
            deleted,
            written,
            events,
            locks_to_delete,
            new_locks_to_init,
            ..
        } = tx_outputs;

        // Store the certificate indexed by transaction digest
        let transaction_digest = transaction.digest();
        write_batch.insert_batch(
            &self.perpetual_tables.transactions,
            iter::once((transaction_digest, transaction.serializable_ref())),
        )?;

        // Add batched writes for objects and locks.
        let effects_digest = effects.digest();

        if use_object_per_epoch_marker_table_v2 {
            write_batch.insert_batch(
                &self.perpetual_tables.object_per_epoch_marker_table_v2,
                markers
                    .iter()
                    .map(|(key, marker_value)| ((epoch_id, *key), *marker_value)),
            )?;
        } else {
            write_batch.insert_batch(
                &self.perpetual_tables.object_per_epoch_marker_table,
                markers
                    .iter()
                    .map(|(key, marker_value)| ((epoch_id, key.into_object_key()), *marker_value)),
            )?;
        }

        write_batch.insert_batch(
            &self.perpetual_tables.objects,
            deleted
                .iter()
                .map(|key| (key, StoreObject::Deleted))
                .chain(wrapped.iter().map(|key| (key, StoreObject::Wrapped)))
                .map(|(key, store_object)| (key, StoreObjectWrapper::from(store_object))),
        )?;

        // Insert each output object into the stores
        let new_objects = written.iter().map(|(id, new_object)| {
            let version = new_object.version();
            trace!(?id, ?version, "writing object");
            let store_object = get_store_object(new_object.clone());
            (ObjectKey(*id, version), store_object)
        });

        write_batch.insert_batch(&self.perpetual_tables.objects, new_objects)?;

        let event_digest = events.digest();
        let events = events
            .data
            .iter()
            .enumerate()
            .map(|(i, e)| ((event_digest, i), e));

        write_batch.insert_batch(&self.perpetual_tables.events, events)?;

        self.initialize_live_object_markers_impl(write_batch, new_locks_to_init, false)?;

        // Note: deletes locks for received objects as well (but not for objects that were in
        // `Receiving` arguments which were not received)
        self.delete_live_object_markers(write_batch, locks_to_delete)?;

        write_batch
            .insert_batch(
                &self.perpetual_tables.effects,
                [(effects_digest, effects.clone())],
            )?
            .insert_batch(
                &self.perpetual_tables.executed_effects,
                [(transaction_digest, effects_digest)],
            )?;

        debug!(effects_digest = ?effects.digest(), "commit_certificate finished");

        Ok(())
    }

    /// Commits transactions only (not effects or other transaction outputs) to the db.
    /// See ExecutionCache::persist_transaction for more info
    pub(crate) fn persist_transaction(&self, tx: &VerifiedExecutableTransaction) -> SuiResult {
        let mut batch = self.perpetual_tables.transactions.batch();
        batch.insert_batch(
            &self.perpetual_tables.transactions,
            [(tx.digest(), tx.clone().into_unsigned().serializable_ref())],
        )?;
        batch.write()?;
        Ok(())
    }

    pub fn acquire_transaction_locks(
        &self,
        epoch_store: &AuthorityPerEpochStore,
        owned_input_objects: &[ObjectRef],
        tx_digest: TransactionDigest,
        signed_transaction: Option<VerifiedSignedTransaction>,
    ) -> SuiResult {
        let epoch = epoch_store.epoch();
        // Other writers may be attempting to acquire locks on the same objects, so a mutex is
        // required.
        // TODO: replace with optimistic db_transactions (i.e. set lock to tx if none)
        let _mutexes = self.acquire_locks(owned_input_objects);

        trace!(?owned_input_objects, "acquire_locks");
        let mut locks_to_write = Vec::new();

        let live_object_markers = self
            .perpetual_tables
            .live_owned_object_markers
            .multi_get(owned_input_objects)?;

        let epoch_tables = epoch_store.tables()?;

        let locks = epoch_tables.multi_get_locked_transactions(owned_input_objects)?;

        assert_eq!(locks.len(), live_object_markers.len());

        for (live_marker, lock, obj_ref) in izip!(
            live_object_markers.into_iter(),
            locks.into_iter(),
            owned_input_objects
        ) {
            let Some(live_marker) = live_marker else {
                let latest_lock = self.get_latest_live_version_for_object_id(obj_ref.0)?;
                fp_bail!(UserInputError::ObjectVersionUnavailableForConsumption {
                    provided_obj_ref: *obj_ref,
                    current_version: latest_lock.1
                }
                .into());
            };

            let live_marker = live_marker.map(|l| l.migrate().into_inner());

            if let Some(LockDetailsDeprecated {
                epoch: previous_epoch,
                ..
            }) = &live_marker
            {
                // this must be from a prior epoch, because we no longer write LockDetails to
                // owned_object_transaction_locks
                assert!(
                    previous_epoch < &epoch,
                    "lock for {:?} should be from a prior epoch",
                    obj_ref
                );
            }

            if let Some(previous_tx_digest) = &lock {
                if previous_tx_digest == &tx_digest {
                    // no need to re-write lock
                    continue;
                } else {
                    // TODO: add metrics here
                    info!(prev_tx_digest = ?previous_tx_digest,
                          cur_tx_digest = ?tx_digest,
                          "Cannot acquire lock: conflicting transaction!");
                    return Err(SuiError::ObjectLockConflict {
                        obj_ref: *obj_ref,
                        pending_transaction: *previous_tx_digest,
                    });
                }
            }

            locks_to_write.push((*obj_ref, tx_digest));
        }

        if !locks_to_write.is_empty() {
            trace!(?locks_to_write, "Writing locks");
            epoch_tables.write_transaction_locks(signed_transaction, locks_to_write.into_iter())?;
        }

        Ok(())
    }

    /// Gets ObjectLockInfo that represents state of lock on an object.
    /// Returns UserInputError::ObjectNotFound if cannot find lock record for this object
    pub(crate) fn get_lock(
        &self,
        obj_ref: ObjectRef,
        epoch_store: &AuthorityPerEpochStore,
    ) -> SuiLockResult {
        if self
            .perpetual_tables
            .live_owned_object_markers
            .get(&obj_ref)?
            .is_none()
        {
            return Ok(ObjectLockStatus::LockedAtDifferentVersion {
                locked_ref: self.get_latest_live_version_for_object_id(obj_ref.0)?,
            });
        }

        let tables = epoch_store.tables()?;
        let epoch_id = epoch_store.epoch();

        if let Some(tx_digest) = tables.get_locked_transaction(&obj_ref)? {
            Ok(ObjectLockStatus::LockedToTx {
                locked_by_tx: LockDetailsDeprecated {
                    epoch: epoch_id,
                    tx_digest,
                },
            })
        } else {
            Ok(ObjectLockStatus::Initialized)
        }
    }

    /// Returns UserInputError::ObjectNotFound if no lock records found for this object.
    pub(crate) fn get_latest_live_version_for_object_id(
        &self,
        object_id: ObjectID,
    ) -> SuiResult<ObjectRef> {
        let mut iterator = self
            .perpetual_tables
            .live_owned_object_markers
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
    pub fn check_owned_objects_are_live(&self, objects: &[ObjectRef]) -> SuiResult {
        let locks = self
            .perpetual_tables
            .live_owned_object_markers
            .multi_get(objects)?;
        for (lock, obj_ref) in locks.into_iter().zip(objects) {
            if lock.is_none() {
                let latest_lock = self.get_latest_live_version_for_object_id(obj_ref.0)?;
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
    fn initialize_live_object_markers_impl(
        &self,
        write_batch: &mut DBBatch,
        objects: &[ObjectRef],
        is_force_reset: bool,
    ) -> SuiResult {
        AuthorityStore::initialize_live_object_markers(
            &self.perpetual_tables.live_owned_object_markers,
            write_batch,
            objects,
            is_force_reset,
        )
    }

    pub fn initialize_live_object_markers(
        live_object_marker_table: &DBMap<ObjectRef, Option<LockDetailsWrapperDeprecated>>,
        write_batch: &mut DBBatch,
        objects: &[ObjectRef],
        is_force_reset: bool,
    ) -> SuiResult {
        trace!(?objects, "initialize_locks");

        let live_object_markers = live_object_marker_table.multi_get(objects)?;

        if !is_force_reset {
            // If any live_object_markers exist and are not None, return errors for them
            // Note we don't check if there is a pre-existing lock. this is because initializing the live
            // object marker will not overwrite the lock and cause the validator to equivocate.
            let existing_live_object_markers: Vec<ObjectRef> = live_object_markers
                .iter()
                .zip(objects)
                .filter_map(|(lock_opt, objref)| {
                    lock_opt.clone().flatten().map(|_tx_digest| *objref)
                })
                .collect();
            if !existing_live_object_markers.is_empty() {
                info!(
                    ?existing_live_object_markers,
                    "Cannot initialize live_object_markers because some exist already"
                );
                return Err(SuiError::ObjectLockAlreadyInitialized {
                    refs: existing_live_object_markers,
                });
            }
        }

        write_batch.insert_batch(
            live_object_marker_table,
            objects.iter().map(|obj_ref| (obj_ref, None)),
        )?;
        Ok(())
    }

    /// Removes locks for a given list of ObjectRefs.
    fn delete_live_object_markers(
        &self,
        write_batch: &mut DBBatch,
        objects: &[ObjectRef],
    ) -> SuiResult {
        trace!(?objects, "delete_locks");
        write_batch.delete_batch(
            &self.perpetual_tables.live_owned_object_markers,
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
            epoch_store.delete_object_locks_for_test(objects);
        }

        let mut batch = self.perpetual_tables.live_owned_object_markers.batch();
        batch
            .delete_batch(
                &self.perpetual_tables.live_owned_object_markers,
                objects.iter(),
            )
            .unwrap();
        batch.write().unwrap();

        let mut batch = self.perpetual_tables.live_owned_object_markers.batch();
        self.initialize_live_object_markers_impl(&mut batch, objects, false)
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
    pub fn revert_state_update(&self, tx_digest: &TransactionDigest) -> SuiResult {
        let Some(effects) = self.get_executed_effects(tx_digest)? else {
            info!("Not reverting {:?} as it was not executed", tx_digest);
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
        self.initialize_live_object_markers_impl(&mut write_batch, &old_locks, true)?;

        // Delete new locks
        write_batch.delete_batch(
            &self.perpetual_tables.live_owned_object_markers,
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
    ) -> SuiResult<Option<Object>> {
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
    ) -> Result<Option<(ObjectKey, ObjectOrTombstone)>, SuiError> {
        let Some((object_key, store_object)) = self
            .perpetual_tables
            .get_latest_object_or_tombstone(object_id)?
        else {
            return Ok(None);
        };

        if let Some(object_ref) = self
            .perpetual_tables
            .tombstone_reference(&object_key, &store_object)?
        {
            return Ok(Some((object_key, ObjectOrTombstone::Tombstone(object_ref))));
        }

        let object = self
            .perpetual_tables
            .object(&object_key, store_object)?
            .expect("Non tombstone store object could not be converted to object");

        Ok(Some((object_key, ObjectOrTombstone::Object(object))))
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

    /// This function reads the DB directly to get the system state object.
    /// If reconfiguration is happening at the same time, there is no guarantee whether we would be getting
    /// the old or the new system state object.
    /// Hence this function should only be called during RPC reads where data race is not a major concern.
    /// In general we should avoid this as much as possible.
    /// If the intent is for testing, you can use AuthorityState:: get_sui_system_state_object_for_testing.
    pub fn get_sui_system_state_object_unsafe(&self) -> SuiResult<SuiSystemState> {
        get_sui_system_state(self.perpetual_tables.as_ref())
    }

    pub fn expensive_check_sui_conservation<T>(
        self: &Arc<Self>,
        type_layout_store: T,
        old_epoch_store: &AuthorityPerEpochStore,
    ) -> SuiResult
    where
        T: TypeLayoutStore + Send + Copy,
    {
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
                                    executor.type_layout_resolver(Box::new(type_layout_store));
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
        let mut layout_resolver = executor.type_layout_resolver(Box::new(type_layout_store));
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

        // It is safe to call this function because we are in the middle of reconfiguration.
        let system_state = self
            .get_sui_system_state_object_unsafe()
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

    /// This is a temporary method to be used when we enable simplified_unwrap_then_delete.
    /// It re-accumulates state hash for the new epoch if simplified_unwrap_then_delete is enabled.
    #[instrument(level = "error", skip_all)]
    pub fn maybe_reaccumulate_state_hash(
        &self,
        cur_epoch_store: &AuthorityPerEpochStore,
        new_protocol_version: ProtocolVersion,
    ) {
        let old_simplified_unwrap_then_delete = cur_epoch_store
            .protocol_config()
            .simplified_unwrap_then_delete();
        let new_simplified_unwrap_then_delete = ProtocolConfig::get_for_version(
            new_protocol_version,
            cur_epoch_store.get_chain_identifier().chain(),
        )
        .simplified_unwrap_then_delete();
        // If in the new epoch the simplified_unwrap_then_delete is enabled for the first time,
        // we re-accumulate state root.
        let should_reaccumulate =
            !old_simplified_unwrap_then_delete && new_simplified_unwrap_then_delete;
        if !should_reaccumulate {
            return;
        }
        info!("[Re-accumulate] simplified_unwrap_then_delete is enabled in the new protocol version, re-accumulating state hash");
        let cur_time = Instant::now();
        std::thread::scope(|s| {
            let pending_tasks = FuturesUnordered::new();
            // Shard the object IDs into different ranges so that we can process them in parallel.
            // We divide the range into 2^BITS number of ranges. To do so we use the highest BITS bits
            // to mark the starting/ending point of the range. For example, when BITS = 5, we
            // divide the range into 32 ranges, and the first few ranges are:
            // 00000000_.... to 00000111_....
            // 00001000_.... to 00001111_....
            // 00010000_.... to 00010111_....
            // and etc.
            const BITS: u8 = 5;
            for index in 0u8..(1 << BITS) {
                pending_tasks.push(s.spawn(move || {
                    let mut id_bytes = [0; ObjectID::LENGTH];
                    id_bytes[0] = index << (8 - BITS);
                    let start_id = ObjectID::new(id_bytes);

                    id_bytes[0] |= (1 << (8 - BITS)) - 1;
                    for element in id_bytes.iter_mut().skip(1) {
                        *element = u8::MAX;
                    }
                    let end_id = ObjectID::new(id_bytes);

                    info!(
                        "[Re-accumulate] Scanning object ID range {:?}..{:?}",
                        start_id, end_id
                    );
                    let mut prev = (
                        ObjectKey::min_for_id(&ObjectID::ZERO),
                        StoreObjectWrapper::V1(StoreObject::Deleted),
                    );
                    let mut object_scanned: u64 = 0;
                    let mut wrapped_objects_to_remove = vec![];
                    for db_result in self.perpetual_tables.objects.safe_range_iter(
                        ObjectKey::min_for_id(&start_id)..=ObjectKey::max_for_id(&end_id),
                    ) {
                        match db_result {
                            Ok((object_key, object)) => {
                                object_scanned += 1;
                                if object_scanned % 100000 == 0 {
                                    info!(
                                        "[Re-accumulate] Task {}: object scanned: {}",
                                        index, object_scanned,
                                    );
                                }
                                if matches!(prev.1.inner(), StoreObject::Wrapped)
                                    && object_key.0 != prev.0 .0
                                {
                                    wrapped_objects_to_remove
                                        .push(WrappedObject::new(prev.0 .0, prev.0 .1));
                                }

                                prev = (object_key, object);
                            }
                            Err(err) => {
                                warn!("Object iterator encounter RocksDB error {:?}", err);
                                return Err(err);
                            }
                        }
                    }
                    if matches!(prev.1.inner(), StoreObject::Wrapped) {
                        wrapped_objects_to_remove.push(WrappedObject::new(prev.0 .0, prev.0 .1));
                    }
                    info!(
                        "[Re-accumulate] Task {}: object scanned: {}, wrapped objects: {}",
                        index,
                        object_scanned,
                        wrapped_objects_to_remove.len(),
                    );
                    Ok((wrapped_objects_to_remove, object_scanned))
                }));
            }
            let (last_checkpoint_of_epoch, cur_accumulator) = self
                .get_root_state_accumulator_for_epoch(cur_epoch_store.epoch())
                .expect("read cannot fail")
                .expect("accumulator must exist");
            let (accumulator, total_objects_scanned, total_wrapped_objects) =
                pending_tasks.into_iter().fold(
                    (cur_accumulator, 0u64, 0usize),
                    |(mut accumulator, total_objects_scanned, total_wrapped_objects), task| {
                        let (wrapped_objects_to_remove, object_scanned) =
                            task.join().unwrap().unwrap();
                        accumulator.remove_all(
                            wrapped_objects_to_remove
                                .iter()
                                .map(|wrapped| bcs::to_bytes(wrapped).unwrap().to_vec())
                                .collect::<Vec<Vec<u8>>>(),
                        );
                        (
                            accumulator,
                            total_objects_scanned + object_scanned,
                            total_wrapped_objects + wrapped_objects_to_remove.len(),
                        )
                    },
                );
            info!(
                "[Re-accumulate] Total objects scanned: {}, total wrapped objects: {}",
                total_objects_scanned, total_wrapped_objects,
            );
            info!(
                "[Re-accumulate] New accumulator value: {:?}",
                accumulator.digest()
            );
            self.insert_state_accumulator_for_epoch(
                cur_epoch_store.epoch(),
                &last_checkpoint_of_epoch,
                &accumulator,
            )
            .unwrap();
        });
        info!(
            "[Re-accumulate] Re-accumulating took {}seconds",
            cur_time.elapsed().as_secs()
        );
    }

    pub async fn prune_objects_and_compact_for_testing(
        &self,
        checkpoint_store: &Arc<CheckpointStore>,
        rpc_index: Option<&RpcIndexStore>,
    ) {
        let pruning_config = AuthorityStorePruningConfig {
            num_epochs_to_retain: 0,
            ..Default::default()
        };
        let _ = AuthorityStorePruner::prune_objects_for_eligible_epochs(
            &self.perpetual_tables,
            checkpoint_store,
            rpc_index,
            None,
            pruning_config,
            AuthorityStorePruningMetrics::new_for_test(),
            EPOCH_DURATION_MS_FOR_TESTING,
        )
        .await;
        let _ = AuthorityStorePruner::compact(&self.perpetual_tables);
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
            .safe_iter_with_bounds(
                Some(ObjectKey(object_id, VersionNumber::MIN)),
                Some(ObjectKey(object_id, VersionNumber::MAX)),
            )
            .collect::<Result<Vec<_>, _>>()
            .unwrap()
            .len()
    }
}

impl AccumulatorStore for AuthorityStore {
    fn get_object_ref_prior_to_key_deprecated(
        &self,
        object_id: &ObjectID,
        version: VersionNumber,
    ) -> SuiResult<Option<ObjectRef>> {
        self.get_object_ref_prior_to_key(object_id, version)
    }

    fn get_root_state_accumulator_for_epoch(
        &self,
        epoch: EpochId,
    ) -> SuiResult<Option<(CheckpointSequenceNumber, Accumulator)>> {
        self.perpetual_tables
            .root_state_hash_by_epoch
            .get(&epoch)
            .map_err(Into::into)
    }

    fn get_root_state_accumulator_for_highest_epoch(
        &self,
    ) -> SuiResult<Option<(EpochId, (CheckpointSequenceNumber, Accumulator))>> {
        Ok(self
            .perpetual_tables
            .root_state_hash_by_epoch
            .safe_iter()
            .skip_to_last()
            .next()
            .transpose()?)
    }

    fn insert_state_accumulator_for_epoch(
        &self,
        epoch: EpochId,
        last_checkpoint_of_epoch: &CheckpointSequenceNumber,
        acc: &Accumulator,
    ) -> SuiResult {
        self.perpetual_tables
            .root_state_hash_by_epoch
            .insert(&epoch, &(*last_checkpoint_of_epoch, acc.clone()))?;
        self.root_state_notify_read
            .notify(&epoch, &(*last_checkpoint_of_epoch, acc.clone()));

        Ok(())
    }

    fn iter_live_object_set(
        &self,
        include_wrapped_object: bool,
    ) -> Box<dyn Iterator<Item = LiveObject> + '_> {
        Box::new(
            self.perpetual_tables
                .iter_live_object_set(include_wrapped_object),
        )
    }
}

impl ObjectStore for AuthorityStore {
    /// Read an object and return it, or Ok(None) if the object was not found.
    fn get_object(&self, object_id: &ObjectID) -> Option<Object> {
        self.perpetual_tables.as_ref().get_object(object_id)
    }

    fn get_object_by_key(&self, object_id: &ObjectID, version: VersionNumber) -> Option<Object> {
        self.perpetual_tables.get_object_by_key(object_id, version)
    }
}

/// A wrapper to make Orphan Rule happy
pub struct ResolverWrapper {
    pub resolver: Arc<dyn BackingPackageStore + Send + Sync>,
    pub metrics: Arc<ResolverMetrics>,
}

impl ResolverWrapper {
    pub fn new(
        resolver: Arc<dyn BackingPackageStore + Send + Sync>,
        metrics: Arc<ResolverMetrics>,
    ) -> Self {
        metrics.module_cache_size.set(0);
        ResolverWrapper { resolver, metrics }
    }

    fn inc_cache_size_gauge(&self) {
        // reset the gauge after a restart of the cache
        let current = self.metrics.module_cache_size.get();
        self.metrics.module_cache_size.set(current + 1);
    }
}

impl ModuleResolver for ResolverWrapper {
    type Error = SuiError;
    fn get_module(&self, module_id: &ModuleId) -> Result<Option<Vec<u8>>, Self::Error> {
        self.inc_cache_size_gauge();
        get_module(&*self.resolver, module_id)
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
    LockedToTx { locked_by_tx: LockDetailsDeprecated },
    LockedAtDifferentVersion { locked_ref: ObjectRef },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum LockDetailsWrapperDeprecated {
    V1(LockDetailsV1Deprecated),
}

impl LockDetailsWrapperDeprecated {
    pub fn migrate(self) -> Self {
        // TODO: when there are multiple versions, we must iteratively migrate from version N to
        // N+1 until we arrive at the latest version
        self
    }

    // Always returns the most recent version. Older versions are migrated to the latest version at
    // read time, so there is never a need to access older versions.
    pub fn inner(&self) -> &LockDetailsDeprecated {
        match self {
            Self::V1(v1) => v1,

            // can remove #[allow] when there are multiple versions
            #[allow(unreachable_patterns)]
            _ => panic!("lock details should have been migrated to latest version at read time"),
        }
    }
    pub fn into_inner(self) -> LockDetailsDeprecated {
        match self {
            Self::V1(v1) => v1,

            // can remove #[allow] when there are multiple versions
            #[allow(unreachable_patterns)]
            _ => panic!("lock details should have been migrated to latest version at read time"),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct LockDetailsV1Deprecated {
    pub epoch: EpochId,
    pub tx_digest: TransactionDigest,
}

pub type LockDetailsDeprecated = LockDetailsV1Deprecated;

impl From<LockDetailsDeprecated> for LockDetailsWrapperDeprecated {
    fn from(details: LockDetailsDeprecated) -> Self {
        // always use latest version.
        LockDetailsWrapperDeprecated::V1(details)
    }
}
