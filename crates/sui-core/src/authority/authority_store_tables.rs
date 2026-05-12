// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use super::*;
use crate::authority::authority_store::LockDetailsWrapperDeprecated;
#[cfg(tidehunter)]
use crate::authority::epoch_marker_key::EPOCH_MARKER_KEY_SIZE;
use crate::authority::epoch_marker_key::EpochMarkerKey;
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::sync::atomic::AtomicU64;
use sui_types::base_types::SequenceNumber;
use sui_types::effects::{TransactionEffects, TransactionEvents};
use sui_types::global_state_hash::GlobalStateHash;
use sui_types::storage::MarkerValue;
use typed_store::metrics::SamplingInterval;
use typed_store::rocks::{
    DBBatch, DBMap, DBMapTableConfigMap, DBOptions, MetricConf, SafeRawIter, default_db_options,
    read_size_from_env,
};
use typed_store::traits::Map;

use crate::authority::authority_store_types::{
    StoreObject, StoreObjectValue, StoreObjectWrapper, get_store_object, try_construct_object,
};
use crate::authority::epoch_start_configuration::EpochStartConfiguration;
use typed_store::{DBMapUtils, DbIterator};

const ENV_VAR_OBJECTS_BLOCK_CACHE_SIZE: &str = "OBJECTS_BLOCK_CACHE_MB";
pub(crate) const ENV_VAR_LOCKS_BLOCK_CACHE_SIZE: &str = "LOCKS_BLOCK_CACHE_MB";
const ENV_VAR_TRANSACTIONS_BLOCK_CACHE_SIZE: &str = "TRANSACTIONS_BLOCK_CACHE_MB";
const ENV_VAR_EFFECTS_BLOCK_CACHE_SIZE: &str = "EFFECTS_BLOCK_CACHE_MB";

/// Options to apply to every column family of the `perpetual` DB.
#[derive(Default)]
pub struct AuthorityPerpetualTablesOptions {
    /// Whether to enable write stalling on all column families.
    pub enable_write_stall: bool,
    pub is_validator: bool,
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
#[cfg_attr(tidehunter, tidehunter)]
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
    ///    are known to be final in the network, but may not have been executed locally yet.
    /// 2. When the transaction is executed locally on this node, we store the effects here. This means that
    ///    it's possible to store the same effects twice (once for the synced transaction, and once for the executed).
    ///
    /// It's also possible for the effects to be reverted if the transaction didn't make it into the epoch.
    pub(crate) effects: DBMap<TransactionEffectsDigest, TransactionEffects>,

    /// Transactions that have been executed locally on this node. We need this table since the `effects` table
    /// doesn't say anything about the execution status of the transaction on this node. When we wait for transactions
    /// to be executed, we wait for them to appear in this table. When we revert transactions, we remove them from both
    /// tables.
    pub(crate) executed_effects: DBMap<TransactionDigest, TransactionEffectsDigest>,

    // Events keyed by the digest of the transaction that produced them.
    pub(crate) events_2: DBMap<TransactionDigest, TransactionEvents>,

    // Loaded (and unchanged) runtime object references.
    pub(crate) unchanged_loaded_runtime_objects: DBMap<TransactionDigest, Vec<ObjectKey>>,

    /// DEPRECATED in favor of the table of the same name in authority_per_epoch_store.
    /// Please do not add new accessors/callsites.
    /// When transaction is executed via checkpoint executor, we store association here
    pub(crate) executed_transactions_to_checkpoint:
        DBMap<TransactionDigest, (EpochId, CheckpointSequenceNumber)>,

    // Finalized root state hash for epoch, to be included in CheckpointSummary
    // of last checkpoint of epoch. These values should only ever be written once
    // and never changed
    pub(crate) root_state_hash_by_epoch:
        DBMap<EpochId, (CheckpointSequenceNumber, GlobalStateHash)>,

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
    pub(crate) object_per_epoch_marker_table_v2: DBMap<EpochMarkerKey, MarkerValue>,

    /// Tracks executed transaction digests across epochs.
    /// Used to support address balance gas payments feature.
    /// This table uses epoch-prefixed keys to support efficient pruning via range delete.
    pub(crate) executed_transaction_digests: DBMap<(EpochId, TransactionDigest), ()>,
}

impl AuthorityPerpetualTables {
    pub fn path(parent_path: &Path) -> PathBuf {
        parent_path.join("perpetual")
    }

    #[cfg(not(tidehunter))]
    pub fn open(
        parent_path: &Path,
        db_options_override: Option<AuthorityPerpetualTablesOptions>,
        _pruner_watermark: Option<Arc<AtomicU64>>,
    ) -> Self {
        let db_options_override = db_options_override.unwrap_or_default();
        let db_options = db_options_override
            .apply_to(default_db_options().optimize_db_for_write_throughput(4, false));
        let table_options = DBMapTableConfigMap::new(BTreeMap::from([
            (
                "objects".to_string(),
                objects_table_config(db_options.clone()),
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
        ]));

        Self::open_tables_read_write(
            Self::path(parent_path),
            MetricConf::new("perpetual")
                .with_sampling(SamplingInterval::new(Duration::from_secs(60), 0)),
            Some(db_options.options),
            Some(table_options),
        )
    }

    #[cfg(tidehunter)]
    pub fn open(
        parent_path: &Path,
        db_options_override: Option<AuthorityPerpetualTablesOptions>,
        pruner_watermark: Option<Arc<AtomicU64>>,
    ) -> Self {
        use crate::authority::authority_store_pruner::apply_relocation_filter;
        tracing::warn!("AuthorityPerpetualTables using tidehunter");
        use typed_store::tidehunter_util::{
            Bytes, Decision, KeyIndexing, KeySpaceConfig, KeyType, ThConfig,
            default_cells_per_mutex, default_max_dirty_keys, default_mutex_count,
            default_value_cache_size,
        };
        let mutexes = default_mutex_count() * 2;
        let transaction_mutexes = mutexes * 4;
        let value_cache_size = default_value_cache_size();
        // effectively disables pruning if not set
        let pruner_watermark = pruner_watermark.unwrap_or(Arc::new(AtomicU64::new(0)));

        let bloom_config = KeySpaceConfig::new().with_bloom_filter(0.001, 32_000);
        let objects_compactor = |iter: &mut dyn DoubleEndedIterator<Item = &Bytes>| {
            let mut retain = HashSet::new();
            let mut previous: Option<&[u8]> = None;
            const OID_SIZE: usize = 32;
            for key in iter.rev() {
                if let Some(prev) = previous {
                    if prev == &key[..OID_SIZE] {
                        continue;
                    }
                }
                previous = Some(&key[..OID_SIZE]);
                retain.insert(key.clone());
            }
            retain
        };
        let mut digest_prefix = vec![0; 8];
        digest_prefix[7] = 32;
        let uniform_key = KeyType::uniform(default_cells_per_mutex());
        let epoch_prefix_key = KeyType::from_prefix_bits(9 * 8 + 4);
        // TransactionDigest is serialized with an 8-byte prefix, so we include it in the key calculation
        let epoch_tx_digest_prefix_key =
            KeyType::from_prefix_bits((8/*EpochId*/ + 8/*TransactionDigest prefix*/) * 8 + 12);
        let object_indexing = KeyIndexing::fixed(32 + 8); //  KeyIndexing::key_reduction(32 + 8, 16..(32 + 8));
        // todo can figure way to scramble off 8 bytes in the middle
        let obj_ref_size = 32 + 8 + 32 + 8;
        let owned_object_transaction_locks_indexing =
            KeyIndexing::key_reduction(obj_ref_size, 16..(obj_ref_size - 16));

        let mut objects_config = KeySpaceConfig::new()
            .with_max_dirty_keys(4 * default_max_dirty_keys())
            .with_value_cache_size(value_cache_size);
        if matches!(db_options_override, Some(options) if options.is_validator) {
            objects_config = objects_config.with_compactor(Box::new(objects_compactor));
        }

        let configs = vec![
            (
                "objects".to_string(),
                ThConfig::new_with_config_indexing(
                    object_indexing,
                    mutexes * 4,
                    KeyType::uniform(1),
                    objects_config,
                ),
            ),
            (
                "owned_object_transaction_locks".to_string(),
                ThConfig::new_with_config_indexing(
                    owned_object_transaction_locks_indexing,
                    mutexes * 16,
                    KeyType::uniform(default_cells_per_mutex()),
                    bloom_config
                        .clone()
                        .with_max_dirty_keys(16 * default_max_dirty_keys()),
                ),
            ),
            (
                "transactions".to_string(),
                ThConfig::new_with_rm_prefix_indexing(
                    KeyIndexing::key_reduction(32, 0..16),
                    transaction_mutexes,
                    uniform_key,
                    KeySpaceConfig::new()
                        .with_value_cache_size(value_cache_size)
                        .with_relocation_filter(|_, _| Decision::Remove),
                    digest_prefix.clone(),
                ),
            ),
            (
                "effects".to_string(),
                ThConfig::new_with_rm_prefix_indexing(
                    KeyIndexing::key_reduction(32, 0..16),
                    transaction_mutexes,
                    uniform_key,
                    apply_relocation_filter(
                        bloom_config.clone().with_value_cache_size(value_cache_size),
                        pruner_watermark.clone(),
                        |effects: TransactionEffects| effects.executed_epoch(),
                        false,
                    ),
                    digest_prefix.clone(),
                ),
            ),
            (
                "executed_effects".to_string(),
                ThConfig::new_with_rm_prefix_indexing(
                    KeyIndexing::key_reduction(32, 0..16),
                    transaction_mutexes,
                    uniform_key,
                    bloom_config
                        .clone()
                        .with_value_cache_size(value_cache_size)
                        .with_relocation_filter(|_, _| Decision::Remove),
                    digest_prefix.clone(),
                ),
            ),
            (
                "events".to_string(),
                ThConfig::new_with_rm_prefix(
                    32 + 8,
                    mutexes,
                    uniform_key,
                    KeySpaceConfig::default().with_relocation_filter(|_, _| Decision::Remove),
                    digest_prefix.clone(),
                ),
            ),
            (
                "events_2".to_string(),
                ThConfig::new_with_rm_prefix(
                    32,
                    mutexes,
                    uniform_key,
                    KeySpaceConfig::default().with_relocation_filter(|_, _| Decision::Remove),
                    digest_prefix.clone(),
                ),
            ),
            (
                "unchanged_loaded_runtime_objects".to_string(),
                ThConfig::new_with_rm_prefix(
                    32,
                    mutexes,
                    uniform_key,
                    KeySpaceConfig::default().with_relocation_filter(|_, _| Decision::Remove),
                    digest_prefix.clone(),
                ),
            ),
            (
                "executed_transactions_to_checkpoint".to_string(),
                ThConfig::new_with_rm_prefix(
                    32,
                    mutexes,
                    uniform_key,
                    apply_relocation_filter(
                        KeySpaceConfig::default(),
                        pruner_watermark.clone(),
                        |(epoch_id, _): (EpochId, CheckpointSequenceNumber)| epoch_id,
                        false,
                    ),
                    digest_prefix.clone(),
                ),
            ),
            (
                "root_state_hash_by_epoch".to_string(),
                ThConfig::new(8, 1, KeyType::uniform(1)),
            ),
            (
                "epoch_start_configuration".to_string(),
                ThConfig::new(0, 1, KeyType::uniform(1)),
            ),
            (
                "pruned_checkpoint".to_string(),
                ThConfig::new(0, 1, KeyType::uniform(1)),
            ),
            (
                "expected_network_sui_amount".to_string(),
                ThConfig::new(0, 1, KeyType::uniform(1)),
            ),
            (
                "expected_storage_fund_imbalance".to_string(),
                ThConfig::new(0, 1, KeyType::uniform(1)),
            ),
            (
                "object_per_epoch_marker_table".to_string(),
                ThConfig::new_with_config_indexing(
                    KeyIndexing::VariableLength,
                    mutexes,
                    epoch_prefix_key,
                    apply_relocation_filter(
                        KeySpaceConfig::default(),
                        pruner_watermark.clone(),
                        |(epoch_id, _): (EpochId, ObjectKey)| epoch_id,
                        true,
                    ),
                ),
            ),
            (
                "object_per_epoch_marker_table_v2".to_string(),
                ThConfig::new_with_config_indexing(
                    KeyIndexing::fixed(EPOCH_MARKER_KEY_SIZE),
                    mutexes,
                    epoch_prefix_key,
                    apply_relocation_filter(
                        bloom_config.clone(),
                        pruner_watermark.clone(),
                        |k: EpochMarkerKey| k.0,
                        true,
                    ),
                ),
            ),
            (
                "executed_transaction_digests".to_string(),
                ThConfig::new_with_config_indexing(
                    // EpochId + (TransactionDigest)
                    KeyIndexing::fixed(8 + (32 + 8)),
                    transaction_mutexes,
                    epoch_tx_digest_prefix_key,
                    apply_relocation_filter(
                        bloom_config.clone(),
                        pruner_watermark.clone(),
                        |(epoch_id, _): (EpochId, TransactionDigest)| epoch_id,
                        true,
                    ),
                ),
            ),
        ];
        Self::open_tables_read_write(
            Self::path(parent_path),
            MetricConf::new("perpetual")
                .with_sampling(SamplingInterval::new(Duration::from_secs(60), 0)),
            configs.into_iter().collect(),
        )
    }

    #[cfg(not(tidehunter))]
    pub fn open_readonly(parent_path: &Path) -> AuthorityPerpetualTablesReadOnly {
        Self::get_read_only_handle(
            Self::path(parent_path),
            None,
            None,
            MetricConf::new("perpetual_readonly"),
        )
    }

    #[cfg(tidehunter)]
    pub fn open_readonly(parent_path: &Path) -> Self {
        Self::open(parent_path, None, None)
    }

    #[cfg(tidehunter)]
    pub fn force_rebuild_control_region(&self) -> anyhow::Result<()> {
        self.objects.db.force_rebuild_control_region()
    }

    // This is used by indexer to find the correct version of dynamic field child object.
    // We do not store the version of the child object, but because of lamport timestamp,
    // we know the child must have version number less then or eq to the parent.
    pub fn find_object_lt_or_eq_version(
        &self,
        object_id: ObjectID,
        version: SequenceNumber,
    ) -> SuiResult<Option<Object>> {
        let mut iter = self.objects.reversed_safe_iter_with_bounds(
            Some(ObjectKey::min_for_id(&object_id)),
            Some(ObjectKey(object_id, version)),
        )?;
        match iter.next() {
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
        Ok(Some(self.construct_object(object_key, *store_object)?))
    }

    pub fn object_reference(
        &self,
        object_key: &ObjectKey,
        store_object: StoreObjectWrapper,
    ) -> Result<ObjectRef, SuiError> {
        let obj_ref = match store_object.migrate().into_inner() {
            StoreObject::Value(object) => self
                .construct_object(object_key, *object)?
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
        let mut iterator = self.objects.reversed_safe_iter_with_bounds(
            Some(ObjectKey::min_for_id(&object_id)),
            Some(ObjectKey::max_for_id(&object_id)),
        )?;

        if let Some(Ok((object_key, value))) = iterator.next()
            && object_key.0 == object_id
        {
            return Ok(Some(self.object_reference(&object_key, value)?));
        }
        Ok(None)
    }

    pub fn get_latest_object_or_tombstone(
        &self,
        object_id: ObjectID,
    ) -> Result<Option<(ObjectKey, StoreObjectWrapper)>, SuiError> {
        let mut iterator = self.objects.reversed_safe_iter_with_bounds(
            Some(ObjectKey::min_for_id(&object_id)),
            Some(ObjectKey::max_for_id(&object_id)),
        )?;

        if let Some(Ok((object_key, value))) = iterator.next()
            && object_key.0 == object_id
        {
            return Ok(Some((object_key, value)));
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

    pub fn get_highest_pruned_checkpoint(
        &self,
    ) -> Result<Option<CheckpointSequenceNumber>, TypedStoreError> {
        self.pruned_checkpoint.get(&())
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

    /// Batch insert executed transaction digests for a given epoch.
    /// Used by formal snapshot restore to backfill transaction digests from the previous epoch.
    pub fn insert_executed_transaction_digests_batch(
        &self,
        epoch: EpochId,
        digests: impl Iterator<Item = TransactionDigest>,
    ) -> SuiResult {
        let mut batch = self.executed_transaction_digests.batch();
        batch.insert_batch(
            &self.executed_transaction_digests,
            digests.map(|digest| ((epoch, digest), ())),
        )?;
        batch.write()?;
        Ok(())
    }

    pub fn get_effects(&self, digest: &TransactionDigest) -> SuiResult<Option<TransactionEffects>> {
        let Some(effect_digest) = self.executed_effects.get(digest)? else {
            return Ok(None);
        };
        Ok(self.effects.get(&effect_digest)?)
    }

    pub(crate) fn was_transaction_executed_in_last_epoch(
        &self,
        digest: &TransactionDigest,
        current_epoch: EpochId,
    ) -> bool {
        if current_epoch == 0 {
            return false;
        }
        self.executed_transaction_digests
            .contains_key(&(current_epoch - 1, *digest))
            .expect("db error")
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
        Ok(self.objects.safe_iter().next().is_none())
    }

    pub fn iter_live_object_set(&self, include_wrapped_object: bool) -> LiveSetIter<'_> {
        self.new_live_set_iter(None, None, include_wrapped_object)
    }

    pub fn range_iter_live_object_set(
        &self,
        lower_bound: Option<ObjectID>,
        upper_bound: Option<ObjectID>,
        include_wrapped_object: bool,
    ) -> LiveSetIter<'_> {
        self.new_live_set_iter(lower_bound, upper_bound, include_wrapped_object)
    }

    fn new_live_set_iter(
        &self,
        lower_bound: Option<ObjectID>,
        upper_bound: Option<ObjectID>,
        include_wrapped_object: bool,
    ) -> LiveSetIter<'_> {
        let lower_key = lower_bound.as_ref().map(ObjectKey::min_for_id);
        let upper_key = upper_bound.as_ref().map(ObjectKey::max_for_id);
        // Prefer the raw-iterator fast path on RocksDB, which avoids decoding
        // non-latest versions of each object. Other backends (InMemory,
        // TideHunter) fall back to the linear-scan lookahead implementation.
        let state = match self
            .objects
            .safe_raw_iter_with_bounds(lower_key, upper_key)
        {
            Some(raw_iter) => LiveSetIterState::Raw {
                iter: raw_iter,
                initialized: false,
            },
            None => LiveSetIterState::Fallback {
                iter: Box::new(self.objects.safe_iter_with_bounds(lower_key, upper_key)),
                prev: None,
            },
        };
        LiveSetIter {
            state,
            tables: self,
            include_wrapped_object,
        }
    }

    pub fn checkpoint_db(&self, path: &Path) -> SuiResult {
        // This checkpoints the entire db and not just objects table
        self.objects.checkpoint_db(path).map_err(Into::into)
    }

    pub fn get_root_state_hash(
        &self,
        epoch: EpochId,
    ) -> SuiResult<Option<(CheckpointSequenceNumber, GlobalStateHash)>> {
        Ok(self.root_state_hash_by_epoch.get(&epoch)?)
    }

    pub fn insert_root_state_hash(
        &self,
        epoch: EpochId,
        last_checkpoint_of_epoch: CheckpointSequenceNumber,
        hash: GlobalStateHash,
    ) -> SuiResult {
        self.root_state_hash_by_epoch
            .insert(&epoch, &(last_checkpoint_of_epoch, hash))?;
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
            .reversed_safe_iter_with_bounds(None, Some(ObjectKey::max_for_id(object_id)))?
            .next();

        match obj_entry.transpose()? {
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
    state: LiveSetIterState<'a>,
    tables: &'a AuthorityPerpetualTables,
    /// Whether a wrapped object is considered as a live object.
    include_wrapped_object: bool,
}

enum LiveSetIterState<'a> {
    /// RocksDB fast path: drive a raw iterator directly so we only decode the
    /// latest version of each object.
    Raw {
        iter: SafeRawIter<'a, ObjectKey>,
        initialized: bool,
    },
    /// Fallback path for non-RocksDB backends: scan every row and emit the
    /// previous one on each `ObjectID` boundary.
    Fallback {
        iter: DbIterator<'a, (ObjectKey, StoreObjectWrapper)>,
        prev: Option<(ObjectKey, StoreObjectWrapper)>,
    },
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
                    .construct_object(&object_key, *object)
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

    /// Raw-iterator fast path: walk forward through versions of the current
    /// object reading only keys, tracking the largest one. Once we leave the
    /// object (or run out of rows), seek back to that key and decode just
    /// its value. Intermediate-version values are never decoded.
    fn fetch_next_raw(&mut self) -> Option<(ObjectKey, StoreObjectWrapper)> {
        let LiveSetIterState::Raw { iter, initialized } = &mut self.state else {
            unreachable!("fetch_next_raw called on non-raw state");
        };
        if !*initialized {
            iter.seek_to_first();
            *initialized = true;
        }
        if !iter.valid() {
            return None;
        }
        let mut latest_key = match iter.key()? {
            Ok(key) => key,
            Err(_) => return None,
        };
        let cur_id = latest_key.0;
        // Advance reading only keys until we leave `cur_id` or hit the end /
        // upper bound. We can't rely on `prev()` once the iterator goes
        // invalid, so we track the largest in-id key seen and seek back to it.
        loop {
            iter.next();
            if !iter.valid() {
                break;
            }
            let k = match iter.key()? {
                Ok(key) => key,
                Err(_) => return None,
            };
            if k.0 != cur_id {
                break;
            }
            latest_key = k;
        }
        // Reposition on the latest-version row of `cur_id`.
        iter.seek(&latest_key);
        debug_assert!(iter.valid(), "seek must land on a known existing key");
        let value_bytes = iter.value()?;
        let wrapper: StoreObjectWrapper = bcs::from_bytes(value_bytes).unwrap_or_else(|e| {
            panic!(
                "Failed to deserialize StoreObjectWrapper for {:?}: {e}",
                latest_key
            )
        });
        // Advance past the latest version to the first row of the next object.
        iter.next();
        Some((latest_key, wrapper))
    }

    /// Legacy lookahead helper for non-RocksDB backends. Returns the next
    /// row that the original linear-scan algorithm would have emitted.
    fn fetch_next_fallback(&mut self) -> Option<(ObjectKey, StoreObjectWrapper)> {
        let LiveSetIterState::Fallback { iter, prev } = &mut self.state else {
            unreachable!("fetch_next_fallback called on non-fallback state");
        };
        loop {
            match iter.next() {
                Some(Ok((next_key, next_value))) => {
                    let prev_entry = prev.take();
                    *prev = Some((next_key, next_value));
                    if let Some((prev_key, prev_value)) = prev_entry
                        && prev_key.0 != next_key.0
                    {
                        return Some((prev_key, prev_value));
                    }
                    continue;
                }
                // Treat decode errors as end-of-iteration (matches the previous
                // implementation, which silently dropped them).
                Some(Err(_)) | None => return prev.take(),
            }
        }
    }
}

impl Iterator for LiveSetIter<'_> {
    type Item = LiveObject;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            let (key, wrapper) = match &self.state {
                LiveSetIterState::Raw { .. } => self.fetch_next_raw()?,
                LiveSetIterState::Fallback { .. } => self.fetch_next_fallback()?,
            };
            if let Some(live) = self.store_object_wrapper_to_live_object(key, wrapper) {
                return Some(live);
            }
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

fn objects_table_config(db_options: DBOptions) -> DBOptions {
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

#[cfg(test)]
#[cfg(not(tidehunter))]
mod live_object_iter_tests {
    use super::*;
    use crate::authority::authority_store_types::StoreObject;
    use std::collections::HashSet;
    use sui_types::base_types::ObjectID;
    use sui_types::object::Object;

    fn open_db() -> Arc<AuthorityPerpetualTables> {
        let path = tempfile::tempdir().unwrap().keep();
        Arc::new(AuthorityPerpetualTables::open(&path, None, None))
    }

    fn write_row(db: &AuthorityPerpetualTables, key: ObjectKey, wrapper: StoreObjectWrapper) {
        let mut batch = db.objects.batch();
        batch
            .insert_batch(&db.objects, [(key, wrapper)])
            .expect("schedule insert");
        batch.write().expect("write batch");
    }

    fn insert_value(db: &AuthorityPerpetualTables, id: ObjectID, version: u64) {
        write_row(
            db,
            ObjectKey(id, SequenceNumber::from_u64(version)),
            get_store_object(Object::immutable_with_id_for_testing(id)),
        );
    }

    fn insert_deleted(db: &AuthorityPerpetualTables, id: ObjectID, version: u64) {
        write_row(
            db,
            ObjectKey(id, SequenceNumber::from_u64(version)),
            StoreObjectWrapper::V1(StoreObject::Deleted),
        );
    }

    fn insert_wrapped(db: &AuthorityPerpetualTables, id: ObjectID, version: u64) {
        write_row(
            db,
            ObjectKey(id, SequenceNumber::from_u64(version)),
            StoreObjectWrapper::V1(StoreObject::Wrapped),
        );
    }

    fn id_with_first_byte(b: u8) -> ObjectID {
        let mut bytes = [0u8; ObjectID::LENGTH];
        bytes[0] = b;
        ObjectID::new(bytes)
    }

    fn ids_and_versions(
        db: &AuthorityPerpetualTables,
        include_wrapped: bool,
    ) -> Vec<(ObjectID, SequenceNumber)> {
        db.iter_live_object_set(include_wrapped)
            .map(|live| (live.object_id(), live.version()))
            .collect()
    }

    #[tokio::test]
    async fn empty_db_yields_nothing() {
        let db = open_db();
        assert!(ids_and_versions(&db, false).is_empty());
        assert!(ids_and_versions(&db, true).is_empty());
    }

    #[tokio::test]
    async fn single_object_single_version() {
        let db = open_db();
        let id = ObjectID::random();
        insert_value(&db, id, 1);
        assert_eq!(
            ids_and_versions(&db, false),
            vec![(id, SequenceNumber::from_u64(1))]
        );
    }

    #[tokio::test]
    async fn emits_only_latest_version() {
        let db = open_db();
        let id = ObjectID::random();
        for v in [1u64, 2, 7, 11, 42] {
            insert_value(&db, id, v);
        }
        assert_eq!(
            ids_and_versions(&db, false),
            vec![(id, SequenceNumber::from_u64(42))]
        );
    }

    #[tokio::test]
    async fn deleted_tombstone_excludes_object() {
        let db = open_db();
        let id = ObjectID::random();
        insert_value(&db, id, 1);
        insert_value(&db, id, 2);
        insert_deleted(&db, id, 3);
        assert!(ids_and_versions(&db, false).is_empty());
        assert!(ids_and_versions(&db, true).is_empty());
    }

    #[tokio::test]
    async fn wrapped_tombstone_respects_include_flag() {
        let db = open_db();
        let id = ObjectID::random();
        insert_value(&db, id, 1);
        insert_wrapped(&db, id, 2);

        assert!(ids_and_versions(&db, false).is_empty());
        assert_eq!(
            ids_and_versions(&db, true),
            vec![(id, SequenceNumber::from_u64(2))]
        );

        // Wrapped objects are surfaced via `LiveObject::Wrapped`, not `Normal`.
        let live: Vec<_> = db.iter_live_object_set(true).collect();
        assert!(matches!(live[..], [LiveObject::Wrapped(_)]));
    }

    #[tokio::test]
    async fn emits_objects_in_id_order() {
        let db = open_db();
        let mut ids: Vec<ObjectID> = (0..20).map(|_| ObjectID::random()).collect();
        for id in &ids {
            insert_value(&db, *id, 1);
            insert_value(&db, *id, 2);
            insert_value(&db, *id, 3);
        }
        ids.sort();
        let expected: Vec<_> = ids
            .into_iter()
            .map(|id| (id, SequenceNumber::from_u64(3)))
            .collect();
        assert_eq!(ids_and_versions(&db, false), expected);
    }

    #[tokio::test]
    async fn mixed_states_filter_correctly() {
        let db = open_db();
        let alive: Vec<_> = (0..7).map(|_| ObjectID::random()).collect();
        let wrapped: Vec<_> = (0..5).map(|_| ObjectID::random()).collect();
        let deleted: Vec<_> = (0..5).map(|_| ObjectID::random()).collect();

        for id in &alive {
            insert_value(&db, *id, 1);
            insert_value(&db, *id, 2);
        }
        for id in &wrapped {
            insert_value(&db, *id, 1);
            insert_wrapped(&db, *id, 2);
        }
        for id in &deleted {
            insert_value(&db, *id, 1);
            insert_deleted(&db, *id, 2);
        }

        let got_alive: HashSet<_> = db
            .iter_live_object_set(false)
            .map(|lo| lo.object_id())
            .collect();
        assert_eq!(got_alive, alive.iter().copied().collect::<HashSet<_>>());

        let got_all_live: HashSet<_> = db
            .iter_live_object_set(true)
            .map(|lo| lo.object_id())
            .collect();
        let mut expected_all = alive.iter().copied().collect::<HashSet<_>>();
        expected_all.extend(wrapped.iter().copied());
        assert_eq!(got_all_live, expected_all);
    }

    #[tokio::test]
    async fn range_iter_includes_only_objects_in_range() {
        let db = open_db();
        // 16 evenly spread ids across the address space's first byte.
        let ids: Vec<ObjectID> = (0u8..16).map(|i| id_with_first_byte(i * 16)).collect();
        for id in &ids {
            insert_value(&db, *id, 1);
            insert_value(&db, *id, 2);
        }
        let lower = ids[4];
        let upper = ids[12];
        let got: Vec<(ObjectID, SequenceNumber)> = db
            .range_iter_live_object_set(Some(lower), Some(upper), false)
            .map(|lo| (lo.object_id(), lo.version()))
            .collect();
        let expected: Vec<_> = ids[4..=12]
            .iter()
            .map(|id| (*id, SequenceNumber::from_u64(2)))
            .collect();
        assert_eq!(got, expected);
    }

    #[tokio::test]
    async fn range_iter_upper_bound_matches_object_id() {
        // Boundary: the requested upper-bound ObjectID is one that exists in
        // the table. `range_iter_live_object_set` converts that to
        // `max_for_id(upper)`, which is the exclusive `iterate_upper_bound`
        // passed to RocksDB. All rows of `upper_id` (which all have versions
        // < u64::MAX) should still be visible, and the iterator should emit
        // `upper_id`'s latest version.
        let db = open_db();
        let ids: Vec<ObjectID> = (0u8..4).map(|i| id_with_first_byte(i * 64)).collect();
        for id in &ids {
            for v in [1u64, 2, 3] {
                insert_value(&db, *id, v);
            }
        }
        let got: Vec<_> = db
            .range_iter_live_object_set(Some(ids[0]), Some(ids[2]), false)
            .map(|lo| (lo.object_id(), lo.version()))
            .collect();
        let expected: Vec<_> = ids[0..=2]
            .iter()
            .map(|id| (*id, SequenceNumber::from_u64(3)))
            .collect();
        assert_eq!(got, expected);
    }

    #[tokio::test]
    async fn range_iter_handles_objects_with_many_versions() {
        // Stress the version-skipping logic by inserting many versions and
        // verifying that only the latest is yielded.
        let db = open_db();
        let id = ObjectID::random();
        for v in 0..200u64 {
            insert_value(&db, id, v);
        }
        assert_eq!(
            ids_and_versions(&db, false),
            vec![(id, SequenceNumber::from_u64(199))]
        );
    }

    #[tokio::test]
    async fn range_iter_empty_range_returns_nothing() {
        let db = open_db();
        for i in 0u8..8 {
            insert_value(&db, id_with_first_byte(i * 16), 1);
        }
        // A range that contains no inserted ids.
        let lower = id_with_first_byte(200);
        let upper = id_with_first_byte(220);
        assert!(
            db.range_iter_live_object_set(Some(lower), Some(upper), false)
                .next()
                .is_none()
        );
    }

    /// Reimplements the partitioning used by `par_index_live_object_set` so the
    /// test does not need an `AuthorityStore`. The test verifies:
    ///   * partition ranges are disjoint at the ObjectID level,
    ///   * each partition only yields ids inside its `[start_id, end_id]`,
    ///   * the union of all partitions equals the full live object set.
    fn par_iter_partitions(db: &AuthorityPerpetualTables) -> HashSet<(ObjectID, SequenceNumber)> {
        const BITS: u8 = 5;
        std::thread::scope(|scope| {
            let mut handles = Vec::new();
            for task in 0u8..(1 << BITS) {
                handles.push(scope.spawn(move || {
                    let mut start_bytes = [0u8; ObjectID::LENGTH];
                    start_bytes[0] = task << (8 - BITS);
                    let start = ObjectID::new(start_bytes);
                    let mut end_bytes = start_bytes;
                    end_bytes[0] |= (1 << (8 - BITS)) - 1;
                    for b in end_bytes.iter_mut().skip(1) {
                        *b = u8::MAX;
                    }
                    let end = ObjectID::new(end_bytes);
                    let part: Vec<(ObjectID, SequenceNumber)> = db
                        .range_iter_live_object_set(Some(start), Some(end), false)
                        .map(|lo| (lo.object_id(), lo.version()))
                        .collect();
                    (start, end, part)
                }));
            }
            let mut seen_ids: HashSet<ObjectID> = HashSet::new();
            let mut combined: HashSet<(ObjectID, SequenceNumber)> = HashSet::new();
            for handle in handles {
                let (start, end, part) = handle.join().expect("partition thread");
                for (id, _) in &part {
                    assert!(
                        *id >= start && *id <= end,
                        "id {id:?} outside partition [{start:?}, {end:?}]"
                    );
                    assert!(
                        seen_ids.insert(*id),
                        "id {id:?} appeared in more than one partition"
                    );
                }
                combined.extend(part);
            }
            combined
        })
    }

    #[tokio::test]
    async fn partitioned_iter_matches_full_iter() {
        let db = open_db();
        let alive: Vec<_> = (0..500).map(|_| ObjectID::random()).collect();
        let deleted: Vec<_> = (0..100).map(|_| ObjectID::random()).collect();
        let wrapped: Vec<_> = (0..100).map(|_| ObjectID::random()).collect();

        for id in &alive {
            for v in [1u64, 2, 3] {
                insert_value(&db, *id, v);
            }
        }
        for id in &deleted {
            insert_value(&db, *id, 1);
            insert_deleted(&db, *id, 2);
        }
        for id in &wrapped {
            insert_value(&db, *id, 1);
            insert_wrapped(&db, *id, 2);
        }

        let full: HashSet<(ObjectID, SequenceNumber)> = db
            .iter_live_object_set(false)
            .map(|lo| (lo.object_id(), lo.version()))
            .collect();
        let parts = par_iter_partitions(&db);
        assert_eq!(parts, full);
    }
}
