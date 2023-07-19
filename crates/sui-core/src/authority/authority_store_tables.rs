// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use super::*;
use crate::authority::authority_store::LockDetailsWrapper;
use rocksdb::Options;
use serde::{Deserialize, Serialize};
use std::path::Path;
use sui_types::accumulator::Accumulator;
use sui_types::base_types::SequenceNumber;
use sui_types::digests::TransactionEventsDigest;
use sui_types::effects::TransactionEffects;
use sui_types::storage::MarkerKind;
use typed_store::metrics::SamplingInterval;
use typed_store::rocks::util::{empty_compaction_filter, reference_count_merge_operator};
use typed_store::rocks::{
    default_db_options, read_size_from_env, DBBatch, DBMap, DBOptions, MetricConf, ReadWriteOptions,
};
use typed_store::traits::{Map, TableSummary, TypedStoreDebug};

use crate::authority::authority_store_types::{
    get_store_object_pair, try_construct_object, ObjectContentDigest, StoreData,
    StoreMoveObjectWrapper, StoreObject, StoreObjectPair, StoreObjectValue, StoreObjectWrapper,
};
use crate::authority::epoch_start_configuration::EpochStartConfiguration;
use typed_store_derive::DBMapUtils;

const ENV_VAR_OBJECTS_BLOCK_CACHE_SIZE: &str = "OBJECTS_BLOCK_CACHE_MB";
const ENV_VAR_LOCKS_BLOCK_CACHE_SIZE: &str = "LOCKS_BLOCK_CACHE_MB";
const ENV_VAR_TRANSACTIONS_BLOCK_CACHE_SIZE: &str = "TRANSACTIONS_BLOCK_CACHE_MB";
const ENV_VAR_EFFECTS_BLOCK_CACHE_SIZE: &str = "EFFECTS_BLOCK_CACHE_MB";
const ENV_VAR_EVENTS_BLOCK_CACHE_SIZE: &str = "EVENTS_BLOCK_CACHE_MB";
const ENV_VAR_INDIRECT_OBJECTS_BLOCK_CACHE_SIZE: &str = "INDIRECT_OBJECTS_BLOCK_CACHE_MB";

/// AuthorityPerpetualTables contains data that must be preserved from one epoch to the next.
#[derive(DBMapUtils)]
pub struct AuthorityPerpetualTables {
    /// This is a map between the object (ID, version) and the latest state of the object, namely the
    /// state that is needed to process new transactions.
    /// State is represented by `StoreObject` enum, which is either a move module, a move object, or
    /// a pointer to an object stored in the `indirect_move_objects` table.
    ///
    /// Note that while this map can store all versions of an object, we will eventually
    /// prune old object versions from the db.
    ///
    /// IMPORTANT: object versions must *only* be pruned if they appear as inputs in some
    /// TransactionEffects. Simply pruning all objects but the most recent is an error!
    /// This is because there can be partially executed transactions whose effects have not yet
    /// been written out, and which must be retried. But, they cannot be retried unless their input
    /// objects are still accessible!
    #[default_options_override_fn = "objects_table_default_config"]
    pub(crate) objects: DBMap<ObjectKey, StoreObjectWrapper>,

    #[default_options_override_fn = "indirect_move_objects_table_default_config"]
    pub(crate) indirect_move_objects: DBMap<ObjectContentDigest, StoreMoveObjectWrapper>,

    /// This is a map between object references of currently active objects that can be mutated,
    /// and the transaction that they are lock on for use by this specific authority. Where an object
    /// lock exists for an object version, but no transaction has been seen using it the lock is set
    /// to None. The safety of consistent broadcast depend on each honest authority never changing
    /// the lock once it is set. After a certificate for this object is processed it can be
    /// forgotten.
    #[default_options_override_fn = "owned_object_transaction_locks_table_default_config"]
    pub(crate) owned_object_transaction_locks: DBMap<ObjectRef, Option<LockDetailsWrapper>>,

    /// This is a map between the transaction digest and the corresponding transaction that's known to be
    /// executable. This means that it may have been executed locally, or it may have been synced through
    /// state-sync but hasn't been executed yet.
    #[default_options_override_fn = "transactions_table_default_config"]
    pub(crate) transactions: DBMap<TransactionDigest, TrustedTransaction>,

    /// A map between the transaction digest of a certificate to the effects of its execution.
    /// We store effects into this table in two different cases:
    /// 1. When a transaction is synced through state_sync, we store the effects here. These effects
    /// are known to be final in the network, but may not have been executed locally yet.
    /// 2. When the transaction is executed locally on this node, we store the effects here. This means that
    /// it's possible to store the same effects twice (once for the synced transaction, and once for the executed).
    /// It's also possible for the effects to be reverted if the transaction didn't make it into the epoch.
    #[default_options_override_fn = "effects_table_default_config"]
    pub(crate) effects: DBMap<TransactionEffectsDigest, TransactionEffects>,

    /// Transactions that have been executed locally on this node. We need this table since the `effects` table
    /// doesn't say anything about the execution status of the transaction on this node. When we wait for transactions
    /// to be executed, we wait for them to appear in this table. When we revert transactions, we remove them from both
    /// tables.
    pub(crate) executed_effects: DBMap<TransactionDigest, TransactionEffectsDigest>,

    // Currently this is needed in the validator for returning events during process certificates.
    // We could potentially remove this if we decided not to provide events in the execution path.
    // TODO: Figure out what to do with this table in the long run.
    // Also we need a pruning policy for this table. We can prune this table along with tx/effects.
    #[default_options_override_fn = "events_table_default_config"]
    pub(crate) events: DBMap<(TransactionEventsDigest, usize), Event>,

    /// DEPRECATED in favor of the table of the same name in authority_per_epoch_store.
    /// Please do not add new accessors/callsites.
    /// When transaction is executed via checkpoint executor, we store association here
    pub(crate) executed_transactions_to_checkpoint:
        DBMap<TransactionDigest, (EpochId, CheckpointSequenceNumber)>,

    // Finalized root state accumulator for epoch, to be included in CheckpointSummary
    // of last checkpoint of epoch. These values should only ever be written once
    // and never changed
    pub(crate) root_state_hash_by_epoch: DBMap<EpochId, (CheckpointSequenceNumber, Accumulator)>,

    /// Parameters of the system fixed at the epoch start
    pub(crate) epoch_start_configuration: DBMap<(), EpochStartConfiguration>,

    /// A singleton table that stores latest pruned checkpoint. Used to keep objects pruner progress
    pub(crate) pruned_checkpoint: DBMap<(), CheckpointSequenceNumber>,

    /// Expected total amount of SUI in the network. This is expected to remain constant
    /// throughout the lifetime of the network. We check it at the end of each epoch if
    /// expensive checks are enabled. We cannot use 10B today because in tests we often
    /// inject extra gas objects into genesis.
    pub(crate) expected_network_sui_amount: DBMap<(), u64>,

    /// Expected imbalance between storage fund balance and the sum of storage rebate of all live objects.
    /// This could be non-zero due to bugs in earlier protocol versions.
    /// This number is the result of storage_fund_balance - sum(storage_rebate).
    pub(crate) expected_storage_fund_imbalance: DBMap<(), i64>,

    /// Table that stores the set of received objects and deleted shared objects and the version at
    /// which they were received. This is used to prevent possible race conditions around receiving
    /// objects (since they are not locked by the transaction manager) and for tracking shared
    /// objects that have been deleted. This table is meant to be pruned per-epoch, and all
    /// previous epochs other than the current epoch may be pruned safely.
    pub(crate) object_per_epoch_marker_table: DBMap<(EpochId, ObjectKey, MarkerKind), ()>,
}

impl AuthorityPerpetualTables {
    pub fn path(parent_path: &Path) -> PathBuf {
        parent_path.join("perpetual")
    }

    pub fn open(parent_path: &Path, db_options: Option<Options>) -> Self {
        Self::open_tables_read_write(
            Self::path(parent_path),
            MetricConf::with_sampling(SamplingInterval::new(Duration::from_secs(60), 0)),
            db_options,
            None,
        )
    }

    pub fn open_readonly(parent_path: &Path) -> AuthorityPerpetualTablesReadOnly {
        Self::get_read_only_handle(Self::path(parent_path), None, None, MetricConf::default())
    }

    // This is used by indexer to find the correct version of dynamic field child object.
    // We do not store the version of the child object, but because of lamport timestamp,
    // we know the child must have version number less then or eq to the parent.
    pub fn find_object_lt_or_eq_version(
        &self,
        object_id: ObjectID,
        version: SequenceNumber,
    ) -> Option<Object> {
        let Ok(iter) = self.objects
            .range_iter(ObjectKey::min_for_id(&object_id)..=ObjectKey::max_for_id(&object_id))
            .skip_prior_to(&ObjectKey(object_id, version))else {
            return None
        };
        iter.reverse()
            .next()
            .and_then(|(key, o)| self.object(&key, o).ok().flatten())
    }

    fn construct_object(
        &self,
        object_key: &ObjectKey,
        store_object: StoreObjectValue,
    ) -> Result<Object, SuiError> {
        let indirect_object = match store_object.data {
            StoreData::IndirectObject(ref metadata) => self
                .indirect_move_objects
                .get(&metadata.digest)?
                .map(|o| o.migrate().into_inner()),
            _ => None,
        };
        try_construct_object(object_key, store_object, indirect_object)
    }

    // Constructs `sui_types::object::Object` from `StoreObjectWrapper`.
    // Returns `None` if object was deleted/wrapped
    pub fn object(
        &self,
        object_key: &ObjectKey,
        store_object: StoreObjectWrapper,
    ) -> Result<Option<Object>, SuiError> {
        let StoreObject::Value(store_object) = store_object.migrate().into_inner() else {return Ok(None)};
        Ok(Some(self.construct_object(object_key, store_object)?))
    }

    pub fn object_reference(
        &self,
        object_key: &ObjectKey,
        store_object: StoreObjectWrapper,
    ) -> Result<ObjectRef, SuiError> {
        let obj_ref = match store_object.migrate().into_inner() {
            StoreObject::Value(object) => self
                .construct_object(object_key, object)?
                .compute_object_reference(),
            StoreObject::Deleted => (
                object_key.0,
                object_key.1,
                ObjectDigest::OBJECT_DIGEST_DELETED,
            ),
            StoreObject::Wrapped => (
                object_key.0,
                object_key.1,
                ObjectDigest::OBJECT_DIGEST_WRAPPED,
            ),
        };
        Ok(obj_ref)
    }

    pub fn tombstone_reference(
        &self,
        object_key: &ObjectKey,
        store_object: &StoreObjectWrapper,
    ) -> Result<Option<ObjectRef>, SuiError> {
        let obj_ref = match store_object.inner() {
            StoreObject::Deleted => Some((
                object_key.0,
                object_key.1,
                ObjectDigest::OBJECT_DIGEST_DELETED,
            )),
            StoreObject::Wrapped => Some((
                object_key.0,
                object_key.1,
                ObjectDigest::OBJECT_DIGEST_WRAPPED,
            )),
            _ => None,
        };
        Ok(obj_ref)
    }

    pub fn get_latest_object_ref_or_tombstone(
        &self,
        object_id: ObjectID,
    ) -> Result<Option<ObjectRef>, SuiError> {
        let mut iterator = self
            .objects
            .unbounded_iter()
            .skip_prior_to(&ObjectKey::max_for_id(&object_id))?;

        if let Some((object_key, value)) = iterator.next() {
            if object_key.0 == object_id {
                return Ok(Some(self.object_reference(&object_key, value)?));
            }
        }
        Ok(None)
    }

    pub fn get_latest_object_or_tombstone(
        &self,
        object_id: ObjectID,
    ) -> Result<Option<(ObjectKey, StoreObjectWrapper)>, SuiError> {
        let mut iterator = self
            .objects
            .unbounded_iter()
            .skip_prior_to(&ObjectKey::max_for_id(&object_id))?;

        if let Some((object_key, value)) = iterator.next() {
            if object_key.0 == object_id {
                return Ok(Some((object_key, value)));
            }
        }
        Ok(None)
    }

    pub fn get_recovery_epoch_at_restart(&self) -> SuiResult<EpochId> {
        Ok(self
            .epoch_start_configuration
            .get(&())?
            .expect("Must have current epoch.")
            .epoch_start_state()
            .epoch())
    }

    pub async fn set_epoch_start_configuration(
        &self,
        epoch_start_configuration: &EpochStartConfiguration,
    ) -> SuiResult {
        let mut wb = self.epoch_start_configuration.batch();
        wb.insert_batch(
            &self.epoch_start_configuration,
            std::iter::once(((), epoch_start_configuration)),
        )?;
        wb.write()?;
        Ok(())
    }

    pub fn get_highest_pruned_checkpoint(&self) -> SuiResult<CheckpointSequenceNumber> {
        Ok(self.pruned_checkpoint.get(&())?.unwrap_or_default())
    }

    pub fn set_highest_pruned_checkpoint(
        &self,
        wb: &mut DBBatch,
        checkpoint_number: CheckpointSequenceNumber,
    ) -> SuiResult {
        wb.insert_batch(&self.pruned_checkpoint, [((), checkpoint_number)])?;
        Ok(())
    }

    pub fn get_transaction(
        &self,
        digest: &TransactionDigest,
    ) -> SuiResult<Option<TrustedTransaction>> {
        let Some(transaction) = self.transactions.get(digest)? else {
            return Ok(None);
        };
        Ok(Some(transaction))
    }

    pub fn get_effects(&self, digest: &TransactionDigest) -> SuiResult<Option<TransactionEffects>> {
        let Some(effect_digest) = self.executed_effects.get(digest)? else {
            return Ok(None);
        };
        Ok(self.effects.get(&effect_digest)?)
    }

    // DEPRECATED as the backing table has been moved to authority_per_epoch_store.
    // Please do not add new accessors/callsites.
    pub fn get_checkpoint_sequence_number(
        &self,
        digest: &TransactionDigest,
    ) -> SuiResult<Option<(EpochId, CheckpointSequenceNumber)>> {
        Ok(self.executed_transactions_to_checkpoint.get(digest)?)
    }

    pub fn get_newer_object_keys(
        &self,
        object: &(ObjectID, SequenceNumber),
    ) -> SuiResult<Vec<ObjectKey>> {
        let mut objects = vec![];
        for (key, _value) in self.objects.iter_with_bounds(
            Some(ObjectKey(object.0, object.1.next())),
            Some(ObjectKey(object.0, VersionNumber::MAX)),
        ) {
            objects.push(key);
        }
        Ok(objects)
    }

    /// Removes executed effects and outputs for a transaction,
    /// and tries to ensure the transaction is replayable.
    ///
    /// WARNING: This method is very subtle and can corrupt the database if used incorrectly.
    /// It should only be used in one-off cases or tests after fully understanding the risk.
    pub fn remove_executed_effects_and_outputs_subtle(
        &self,
        digest: &TransactionDigest,
        objects: &[ObjectKey],
    ) -> SuiResult {
        let mut wb = self.objects.batch();
        for object in objects {
            wb.delete_batch(&self.objects, [object])?;
            if self.has_object_lock(object) {
                self.remove_object_lock_batch(&mut wb, object)?;
            }
        }
        wb.delete_batch(&self.executed_transactions_to_checkpoint, [digest])?;
        wb.delete_batch(&self.executed_effects, [digest])?;
        wb.write()?;
        Ok(())
    }

    pub fn has_object_lock(&self, object: &ObjectKey) -> bool {
        self.owned_object_transaction_locks
            .iter_with_bounds(
                Some((object.0, object.1, ObjectDigest::MIN)),
                Some((object.0, object.1, ObjectDigest::MAX)),
            )
            .next()
            .is_some()
    }

    /// Removes owned object locks and set the lock to the previous version of the object.
    ///
    /// WARNING: This method is very subtle and can corrupt the database if used incorrectly.
    /// It should only be used in one-off cases or tests after fully understanding the risk.
    pub fn remove_object_lock_subtle(&self, object: &ObjectKey) -> SuiResult<ObjectRef> {
        let mut wb = self.objects.batch();
        let object_ref = self.remove_object_lock_batch(&mut wb, object)?;
        wb.write()?;
        Ok(object_ref)
    }

    fn remove_object_lock_batch(
        &self,
        wb: &mut DBBatch,
        object: &ObjectKey,
    ) -> SuiResult<ObjectRef> {
        wb.delete_range(
            &self.owned_object_transaction_locks,
            &(object.0, object.1, ObjectDigest::MIN),
            &(object.0, object.1, ObjectDigest::MAX),
        )?;
        let object_ref = self.get_latest_object_ref_or_tombstone(object.0)?.unwrap();
        wb.insert_batch(&self.owned_object_transaction_locks, [(object_ref, None)])?;
        Ok(object_ref)
    }

    pub fn set_highest_pruned_checkpoint_without_wb(
        &self,
        checkpoint_number: CheckpointSequenceNumber,
    ) -> SuiResult {
        let mut wb = self.pruned_checkpoint.batch();
        self.set_highest_pruned_checkpoint(&mut wb, checkpoint_number)
    }

    pub fn database_is_empty(&self) -> SuiResult<bool> {
        Ok(self
            .objects
            .unbounded_iter()
            .skip_to(&ObjectKey::ZERO)?
            .next()
            .is_none())
    }

    pub fn iter_live_object_set(&self, include_wrapped_object: bool) -> LiveSetIter<'_> {
        LiveSetIter {
            iter: self.objects.unbounded_iter(),
            tables: self,
            prev: None,
            include_wrapped_object,
        }
    }

    pub fn checkpoint_db(&self, path: &Path) -> SuiResult {
        // This checkpoints the entire db and not just objects table
        self.objects
            .checkpoint_db(path)
            .map_err(SuiError::StorageError)
    }

    pub fn reset_db_for_execution_since_genesis(&self) -> SuiResult {
        // TODO: Add new tables that get added to the db automatically
        self.objects.unsafe_clear()?;
        self.indirect_move_objects.unsafe_clear()?;
        self.owned_object_transaction_locks.unsafe_clear()?;
        self.executed_effects.unsafe_clear()?;
        self.events.unsafe_clear()?;
        self.executed_transactions_to_checkpoint.unsafe_clear()?;
        self.root_state_hash_by_epoch.unsafe_clear()?;
        self.epoch_start_configuration.unsafe_clear()?;
        self.pruned_checkpoint.unsafe_clear()?;
        self.expected_network_sui_amount.unsafe_clear()?;
        self.expected_storage_fund_imbalance.unsafe_clear()?;
        self.object_per_epoch_marker_table.unsafe_clear()?;
        self.objects
            .rocksdb
            .flush()
            .map_err(SuiError::StorageError)?;
        Ok(())
    }

    pub fn insert_root_state_hash(
        &self,
        epoch: EpochId,
        last_checkpoint_of_epoch: CheckpointSequenceNumber,
        accumulator: Accumulator,
    ) -> SuiResult {
        self.root_state_hash_by_epoch
            .insert(&epoch, &(last_checkpoint_of_epoch, accumulator))?;
        Ok(())
    }

    pub fn insert_object_test_only(&self, object: Object) -> SuiResult {
        let object_reference = object.compute_object_reference();
        let StoreObjectPair(wrapper, _indirect_object) = get_store_object_pair(object, usize::MAX);
        let mut wb = self.objects.batch();
        wb.insert_batch(
            &self.objects,
            std::iter::once((ObjectKey::from(object_reference), wrapper)),
        )?;
        wb.write()?;
        Ok(())
    }
}

impl ObjectStore for AuthorityPerpetualTables {
    /// Read an object and return it, or Ok(None) if the object was not found.
    fn get_object(&self, object_id: &ObjectID) -> Result<Option<Object>, SuiError> {
        let obj_entry = self
            .objects
            .unbounded_iter()
            .skip_prior_to(&ObjectKey::max_for_id(object_id))?
            .next();

        match obj_entry {
            Some((ObjectKey(obj_id, version), obj)) if obj_id == *object_id => {
                Ok(self.object(&ObjectKey(obj_id, version), obj)?)
            }
            _ => Ok(None),
        }
    }

    fn get_object_by_key(
        &self,
        object_id: &ObjectID,
        version: VersionNumber,
    ) -> Result<Option<Object>, SuiError> {
        Ok(self
            .objects
            .get(&ObjectKey(*object_id, version))?
            .map(|object| self.object(&ObjectKey(*object_id, version), object))
            .transpose()?
            .flatten())
    }
}

pub struct LiveSetIter<'a> {
    iter:
        <DBMap<ObjectKey, StoreObjectWrapper> as Map<'a, ObjectKey, StoreObjectWrapper>>::Iterator,
    tables: &'a AuthorityPerpetualTables,
    prev: Option<(ObjectKey, StoreObjectWrapper)>,
    /// Whether a wrapped object is considered as a live object.
    include_wrapped_object: bool,
}

#[derive(Eq, PartialEq, Debug, Clone, Deserialize, Serialize, Hash)]
pub enum LiveObject {
    Normal(Object),
    Wrapped(ObjectKey),
}

impl LiveObject {
    pub fn object_id(&self) -> ObjectID {
        match self {
            LiveObject::Normal(obj) => obj.id(),
            LiveObject::Wrapped(key) => key.0,
        }
    }

    pub fn version(&self) -> SequenceNumber {
        match self {
            LiveObject::Normal(obj) => obj.version(),
            LiveObject::Wrapped(key) => key.1,
        }
    }

    pub fn object_reference(&self) -> ObjectRef {
        match self {
            LiveObject::Normal(obj) => obj.compute_object_reference(),
            LiveObject::Wrapped(key) => (key.0, key.1, ObjectDigest::OBJECT_DIGEST_WRAPPED),
        }
    }
}

impl LiveSetIter<'_> {
    fn store_object_wrapper_to_live_object(
        &self,
        object_key: ObjectKey,
        store_object: StoreObjectWrapper,
    ) -> Option<LiveObject> {
        match store_object.migrate().into_inner() {
            StoreObject::Value(object) => {
                let object = self
                    .tables
                    .construct_object(&object_key, object)
                    .expect("Constructing object from store cannot fail");
                Some(LiveObject::Normal(object))
            }
            StoreObject::Wrapped => {
                if self.include_wrapped_object {
                    Some(LiveObject::Wrapped(object_key))
                } else {
                    None
                }
            }
            StoreObject::Deleted => None,
        }
    }
}

impl Iterator for LiveSetIter<'_> {
    type Item = LiveObject;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            if let Some((next_key, next_value)) = self.iter.next() {
                let prev = self.prev.take();
                self.prev = Some((next_key, next_value));

                if let Some((prev_key, prev_value)) = prev {
                    if prev_key.0 != next_key.0 {
                        let live_object =
                            self.store_object_wrapper_to_live_object(prev_key, prev_value);
                        if live_object.is_some() {
                            return live_object;
                        }
                    }
                }
                continue;
            }
            if let Some((key, value)) = self.prev.take() {
                let live_object = self.store_object_wrapper_to_live_object(key, value);
                if live_object.is_some() {
                    return live_object;
                }
            }
            return None;
        }
    }
}

// These functions are used to initialize the DB tables
fn owned_object_transaction_locks_table_default_config() -> DBOptions {
    DBOptions {
        options: default_db_options()
            .optimize_for_write_throughput()
            .optimize_for_read(read_size_from_env(ENV_VAR_LOCKS_BLOCK_CACHE_SIZE).unwrap_or(1024))
            .options,
        rw_options: ReadWriteOptions::default().set_ignore_range_deletions(false),
    }
}

fn objects_table_default_config() -> DBOptions {
    default_db_options()
        .optimize_for_write_throughput()
        .optimize_for_read(read_size_from_env(ENV_VAR_OBJECTS_BLOCK_CACHE_SIZE).unwrap_or(5 * 1024))
}

fn transactions_table_default_config() -> DBOptions {
    default_db_options()
        .optimize_for_write_throughput()
        .optimize_for_point_lookup(
            read_size_from_env(ENV_VAR_TRANSACTIONS_BLOCK_CACHE_SIZE).unwrap_or(512),
        )
}

fn effects_table_default_config() -> DBOptions {
    default_db_options()
        .optimize_for_write_throughput()
        .optimize_for_point_lookup(
            read_size_from_env(ENV_VAR_EFFECTS_BLOCK_CACHE_SIZE).unwrap_or(1024),
        )
}

fn events_table_default_config() -> DBOptions {
    default_db_options()
        .optimize_for_write_throughput()
        .optimize_for_read(read_size_from_env(ENV_VAR_EVENTS_BLOCK_CACHE_SIZE).unwrap_or(1024))
}

fn indirect_move_objects_table_default_config() -> DBOptions {
    let mut options = default_db_options()
        .optimize_for_write_throughput()
        .optimize_for_point_lookup(
            read_size_from_env(ENV_VAR_INDIRECT_OBJECTS_BLOCK_CACHE_SIZE).unwrap_or(512),
        );
    options.options.set_merge_operator(
        "refcount operator",
        reference_count_merge_operator,
        reference_count_merge_operator,
    );
    options
        .options
        .set_compaction_filter("empty filter", empty_compaction_filter);
    options
}
