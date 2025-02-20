// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use super::*;
use crate::authority::authority_store::LockDetailsWrapperDeprecated;
use serde::{Deserialize, Serialize};
use std::path::Path;
use sui_types::accumulator::Accumulator;
use sui_types::base_types::SequenceNumber;
use sui_types::digests::TransactionEventsDigest;
use sui_types::effects::TransactionEffects;
use sui_types::storage::{FullObjectKey, MarkerValue};
use tracing::error;
use typed_store::metrics::SamplingInterval;
use typed_store::rocks::{
    default_db_options, read_size_from_env, DBBatch, DBMap, DBMapTableConfigMap, DBOptions,
    MetricConf,
};
use typed_store::traits::{Map, TableSummary, TypedStoreDebug};

use crate::authority::authority_store_pruner::ObjectsCompactionFilter;
use crate::authority::authority_store_types::{
    get_store_object, try_construct_object, StoreObject, StoreObjectValue, StoreObjectWrapper,
};
use crate::authority::epoch_start_configuration::EpochStartConfiguration;
use typed_store::rocksdb::compaction_filter::Decision;
use typed_store::DBMapUtils;

const ENV_VAR_OBJECTS_BLOCK_CACHE_SIZE: &str = "OBJECTS_BLOCK_CACHE_MB";
pub(crate) const ENV_VAR_LOCKS_BLOCK_CACHE_SIZE: &str = "LOCKS_BLOCK_CACHE_MB";
const ENV_VAR_TRANSACTIONS_BLOCK_CACHE_SIZE: &str = "TRANSACTIONS_BLOCK_CACHE_MB";
const ENV_VAR_EFFECTS_BLOCK_CACHE_SIZE: &str = "EFFECTS_BLOCK_CACHE_MB";
const ENV_VAR_EVENTS_BLOCK_CACHE_SIZE: &str = "EVENTS_BLOCK_CACHE_MB";

/// Options to apply to every column family of the `perpetual` DB.
#[derive(Default)]
pub struct AuthorityPerpetualTablesOptions {
    /// Whether to enable write stalling on all column families.
    pub enable_write_stall: bool,
    pub compaction_filter: Option<ObjectsCompactionFilter>,
}

impl AuthorityPerpetualTablesOptions {
    fn apply_to(&self, mut db_options: DBOptions) -> DBOptions {
        if !self.enable_write_stall {
            db_options = db_options.disable_write_throttling();
        }
        db_options
    }
}

/// AuthorityPerpetualTables contains data that must be preserved from one epoch to the next.
#[derive(DBMapUtils)]
pub struct AuthorityPerpetualTables {
    /// This is a map between the object (ID, version) and the latest state of the object, namely the
    /// state that is needed to process new transactions.
    /// State is represented by `StoreObject` enum, which is either a move module or a move object.
    ///
    /// Note that while this map can store all versions of an object, we will eventually
    /// prune old object versions from the db.
    ///
    /// IMPORTANT: object versions must *only* be pruned if they appear as inputs in some
    /// TransactionEffects. Simply pruning all objects but the most recent is an error!
    /// This is because there can be partially executed transactions whose effects have not yet
    /// been written out, and which must be retried. But, they cannot be retried unless their input
    /// objects are still accessible!
    pub(crate) objects: DBMap<ObjectKey, StoreObjectWrapper>,

    /// This is a map between object references of currently active objects that can be mutated.
    ///
    /// For old epochs, it may also contain the transaction that they are lock on for use by this
    /// specific validator. The transaction locks themselves are now in AuthorityPerEpochStore.
    #[rename = "owned_object_transaction_locks"]
    pub(crate) live_owned_object_markers: DBMap<ObjectRef, Option<LockDetailsWrapperDeprecated>>,

    /// This is a map between the transaction digest and the corresponding transaction that's known to be
    /// executable. This means that it may have been executed locally, or it may have been synced through
    /// state-sync but hasn't been executed yet.
    pub(crate) transactions: DBMap<TransactionDigest, TrustedTransaction>,

    /// A map between the transaction digest of a certificate to the effects of its execution.
    /// We store effects into this table in two different cases:
    /// 1. When a transaction is synced through state_sync, we store the effects here. These effects
    ///     are known to be final in the network, but may not have been executed locally yet.
    /// 2. When the transaction is executed locally on this node, we store the effects here. This means that
    ///     it's possible to store the same effects twice (once for the synced transaction, and once for the executed).
    ///
    /// It's also possible for the effects to be reverted if the transaction didn't make it into the epoch.
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

    /// Table that stores the set of received objects and deleted objects and the version at
    /// which they were received. This is used to prevent possible race conditions around receiving
    /// objects (since they are not locked by the transaction manager) and for tracking shared
    /// objects that have been deleted. This table is meant to be pruned per-epoch, and all
    /// previous epochs other than the current epoch may be pruned safely.
    pub(crate) object_per_epoch_marker_table: DBMap<(EpochId, ObjectKey), MarkerValue>,
    pub(crate) object_per_epoch_marker_table_v2: DBMap<(EpochId, FullObjectKey), MarkerValue>,
}

#[derive(DBMapUtils)]
pub struct AuthorityPrunerTables {
    pub(crate) object_tombstones: DBMap<ObjectID, SequenceNumber>,
}

impl AuthorityPrunerTables {
    pub fn path(parent_path: &Path) -> PathBuf {
        parent_path.join("pruner")
    }

    pub fn open(parent_path: &Path) -> Self {
        Self::open_tables_read_write(
            Self::path(parent_path),
            MetricConf::new("pruner")
                .with_sampling(SamplingInterval::new(Duration::from_secs(60), 0)),
            None,
            None,
        )
    }
}

impl AuthorityPerpetualTables {
    pub fn path(parent_path: &Path) -> PathBuf {
        parent_path.join("perpetual")
    }

    pub fn open(
        parent_path: &Path,
        db_options_override: Option<AuthorityPerpetualTablesOptions>,
    ) -> Self {
        let db_options_override = db_options_override.unwrap_or_default();
        let db_options =
            db_options_override.apply_to(default_db_options().optimize_db_for_write_throughput(4));
        let table_options = DBMapTableConfigMap::new(BTreeMap::from([
            (
                "objects".to_string(),
                objects_table_config(db_options.clone(), db_options_override.compaction_filter),
            ),
            (
                "owned_object_transaction_locks".to_string(),
                owned_object_transaction_locks_table_config(db_options.clone()),
            ),
            (
                "transactions".to_string(),
                transactions_table_config(db_options.clone()),
            ),
            (
                "effects".to_string(),
                effects_table_config(db_options.clone()),
            ),
            (
                "events".to_string(),
                events_table_config(db_options.clone()),
            ),
        ]));
        Self::open_tables_read_write(
            Self::path(parent_path),
            MetricConf::new("perpetual")
                .with_sampling(SamplingInterval::new(Duration::from_secs(60), 0)),
            Some(db_options.options),
            Some(table_options),
        )
    }

    pub fn open_readonly(parent_path: &Path) -> AuthorityPerpetualTablesReadOnly {
        Self::get_read_only_handle(
            Self::path(parent_path),
            None,
            None,
            MetricConf::new("perpetual_readonly"),
        )
    }

    // This is used by indexer to find the correct version of dynamic field child object.
    // We do not store the version of the child object, but because of lamport timestamp,
    // we know the child must have version number less then or eq to the parent.
    pub fn find_object_lt_or_eq_version(
        &self,
        object_id: ObjectID,
        version: SequenceNumber,
    ) -> SuiResult<Option<Object>> {
        let iter = self
            .objects
            .safe_range_iter(ObjectKey::min_for_id(&object_id)..=ObjectKey::max_for_id(&object_id))
            .skip_prior_to(&ObjectKey(object_id, version))?;
        match iter.reverse().next() {
            Some(Ok((key, o))) => self.object(&key, o),
            Some(Err(e)) => Err(e.into()),
            None => Ok(None),
        }
    }

    fn construct_object(
        &self,
        object_key: &ObjectKey,
        store_object: StoreObjectValue,
    ) -> Result<Object, SuiError> {
        try_construct_object(object_key, store_object)
    }

    // Constructs `sui_types::object::Object` from `StoreObjectWrapper`.
    // Returns `None` if object was deleted/wrapped
    pub fn object(
        &self,
        object_key: &ObjectKey,
        store_object: StoreObjectWrapper,
    ) -> Result<Option<Object>, SuiError> {
        let StoreObject::Value(store_object) = store_object.migrate().into_inner() else {
            return Ok(None);
        };
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

    pub fn set_epoch_start_configuration(
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
        for result in self.objects.safe_iter_with_bounds(
            Some(ObjectKey(object.0, object.1.next())),
            Some(ObjectKey(object.0, VersionNumber::MAX)),
        ) {
            let (key, _) = result?;
            objects.push(key);
        }
        Ok(objects)
    }

    pub fn set_highest_pruned_checkpoint_without_wb(
        &self,
        checkpoint_number: CheckpointSequenceNumber,
    ) -> SuiResult {
        let mut wb = self.pruned_checkpoint.batch();
        self.set_highest_pruned_checkpoint(&mut wb, checkpoint_number)?;
        wb.write()?;
        Ok(())
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

    pub fn range_iter_live_object_set(
        &self,
        lower_bound: Option<ObjectID>,
        upper_bound: Option<ObjectID>,
        include_wrapped_object: bool,
    ) -> LiveSetIter<'_> {
        let lower_bound = lower_bound.as_ref().map(ObjectKey::min_for_id);
        let upper_bound = upper_bound.as_ref().map(ObjectKey::max_for_id);

        LiveSetIter {
            iter: self.objects.iter_with_bounds(lower_bound, upper_bound),
            tables: self,
            prev: None,
            include_wrapped_object,
        }
    }

    pub fn checkpoint_db(&self, path: &Path) -> SuiResult {
        // This checkpoints the entire db and not just objects table
        self.objects.checkpoint_db(path).map_err(Into::into)
    }

    pub fn reset_db_for_execution_since_genesis(&self) -> SuiResult {
        // TODO: Add new tables that get added to the db automatically
        self.objects.unsafe_clear()?;
        self.live_owned_object_markers.unsafe_clear()?;
        self.executed_effects.unsafe_clear()?;
        self.events.unsafe_clear()?;
        self.executed_transactions_to_checkpoint.unsafe_clear()?;
        self.root_state_hash_by_epoch.unsafe_clear()?;
        self.epoch_start_configuration.unsafe_clear()?;
        self.pruned_checkpoint.unsafe_clear()?;
        self.expected_network_sui_amount.unsafe_clear()?;
        self.expected_storage_fund_imbalance.unsafe_clear()?;
        self.object_per_epoch_marker_table.unsafe_clear()?;
        self.object_per_epoch_marker_table_v2.unsafe_clear()?;
        self.objects.rocksdb.flush()?;
        Ok(())
    }

    pub fn get_root_state_hash(
        &self,
        epoch: EpochId,
    ) -> SuiResult<Option<(CheckpointSequenceNumber, Accumulator)>> {
        Ok(self.root_state_hash_by_epoch.get(&epoch)?)
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
        let wrapper = get_store_object(object);
        let mut wb = self.objects.batch();
        wb.insert_batch(
            &self.objects,
            std::iter::once((ObjectKey::from(object_reference), wrapper)),
        )?;
        wb.write()?;
        Ok(())
    }

    // fallible get object methods for sui-tool, which may need to attempt to read a corrupted database
    pub fn get_object_fallible(&self, object_id: &ObjectID) -> SuiResult<Option<Object>> {
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

    pub fn get_object_by_key_fallible(
        &self,
        object_id: &ObjectID,
        version: VersionNumber,
    ) -> SuiResult<Option<Object>> {
        Ok(self
            .objects
            .get(&ObjectKey(*object_id, version))?
            .and_then(|object| {
                self.object(&ObjectKey(*object_id, version), object)
                    .expect("object construction error")
            }))
    }
}

impl ObjectStore for AuthorityPerpetualTables {
    /// Read an object and return it, or Ok(None) if the object was not found.
    fn get_object(&self, object_id: &ObjectID) -> Option<Object> {
        self.get_object_fallible(object_id).expect("db error")
    }

    fn get_object_by_key(&self, object_id: &ObjectID, version: VersionNumber) -> Option<Object> {
        self.get_object_by_key_fallible(object_id, version)
            .expect("db error")
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

    pub fn to_normal(self) -> Option<Object> {
        match self {
            LiveObject::Normal(object) => Some(object),
            LiveObject::Wrapped(_) => None,
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
fn owned_object_transaction_locks_table_config(db_options: DBOptions) -> DBOptions {
    DBOptions {
        options: db_options
            .clone()
            .optimize_for_write_throughput()
            .optimize_for_read(read_size_from_env(ENV_VAR_LOCKS_BLOCK_CACHE_SIZE).unwrap_or(1024))
            .options,
        rw_options: db_options.rw_options.set_ignore_range_deletions(false),
    }
}

fn objects_table_config(
    mut db_options: DBOptions,
    compaction_filter: Option<ObjectsCompactionFilter>,
) -> DBOptions {
    if let Some(mut compaction_filter) = compaction_filter {
        db_options
            .options
            .set_compaction_filter("objects", move |_, key, value| {
                match compaction_filter.filter(key, value) {
                    Ok(decision) => decision,
                    Err(err) => {
                        error!("Compaction error: {:?}", err);
                        Decision::Keep
                    }
                }
            });
    }
    db_options
        .optimize_for_write_throughput()
        .optimize_for_read(read_size_from_env(ENV_VAR_OBJECTS_BLOCK_CACHE_SIZE).unwrap_or(5 * 1024))
}

fn transactions_table_config(db_options: DBOptions) -> DBOptions {
    db_options
        .optimize_for_write_throughput()
        .optimize_for_point_lookup(
            read_size_from_env(ENV_VAR_TRANSACTIONS_BLOCK_CACHE_SIZE).unwrap_or(512),
        )
}

fn effects_table_config(db_options: DBOptions) -> DBOptions {
    db_options
        .optimize_for_write_throughput()
        .optimize_for_point_lookup(
            read_size_from_env(ENV_VAR_EFFECTS_BLOCK_CACHE_SIZE).unwrap_or(1024),
        )
}

fn events_table_config(db_options: DBOptions) -> DBOptions {
    db_options
        .optimize_for_write_throughput()
        .optimize_for_read(read_size_from_env(ENV_VAR_EVENTS_BLOCK_CACHE_SIZE).unwrap_or(1024))
}
