// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::authority::AuthorityState;
use crate::authority::authority_store_tables::AuthorityPerpetualTablesOptions;
use crate::checkpoints::CheckpointStore;
use crate::epoch::committee_store::CommitteeStore;
use crate::execution_cache::{ExecutionCacheTraitPointers, FoundObjectOrTombstone};
use move_core_types::language_storage::StructTag;
use parking_lot::Mutex;
use std::ops::RangeInclusive;
use std::sync::Arc;
use sui_rpc_store::RpcStoreReader;
use sui_types::base_types::ExecutionData;
use sui_types::base_types::ExecutionDigests;
use sui_types::base_types::ObjectID;
use sui_types::base_types::SequenceNumber;
use sui_types::base_types::SuiAddress;
use sui_types::base_types::TransactionDigest;
use sui_types::base_types::VerifiedExecutionData;
use sui_types::committee::Committee;
use sui_types::committee::EpochId;
use sui_types::effects::{TransactionEffects, TransactionEvents};
use sui_types::error::{SuiErrorKind, SuiResult};
use sui_types::messages_checkpoint::CheckpointContents;
use sui_types::messages_checkpoint::CheckpointContentsDigest;
use sui_types::messages_checkpoint::CheckpointDigest;
use sui_types::messages_checkpoint::CheckpointSequenceNumber;
use sui_types::messages_checkpoint::EndOfEpochData;
use sui_types::messages_checkpoint::VerifiedCheckpoint;
use sui_types::messages_checkpoint::VerifiedCheckpointContents;
use sui_types::messages_checkpoint::VersionedFullCheckpointContents;
use sui_types::object::Object;
use sui_types::object::Owner;
use sui_types::storage::BalanceInfo;
use sui_types::storage::BalanceIterator;
use sui_types::storage::CoinInfo;
use sui_types::storage::DynamicFieldKey;
use sui_types::storage::LedgerBitmapBucketIterator;
use sui_types::storage::LedgerTxSeqDigest;
use sui_types::storage::LedgerTxSeqDigestIterator;
use sui_types::storage::ObjectStore;
use sui_types::storage::ObjectVersionsIterator;
use sui_types::storage::OwnedObjectInfo;
use sui_types::storage::RpcIndexes;
use sui_types::storage::RpcStateReader;
use sui_types::storage::RuntimeObjectResolver;
use sui_types::storage::WriteStore;
use sui_types::storage::error::Error as StorageError;
use sui_types::storage::error::Result;
use sui_types::storage::{BackingPackageStore, PackageObject};
use sui_types::storage::{ObjectKey, ObjectOrTombstone, ReadStore};
use sui_types::transaction::VerifiedTransaction;
use tap::TapFallible;
use tracing::error;
use typed_store::TypedStoreError;

#[derive(Clone)]
pub struct RocksDbStore {
    cache_traits: ExecutionCacheTraitPointers,

    committee_store: Arc<CommitteeStore>,
    checkpoint_store: Arc<CheckpointStore>,
    // in memory checkpoint watermark sequence numbers
    highest_verified_checkpoint: Arc<Mutex<Option<u64>>>,
    highest_synced_checkpoint: Arc<Mutex<Option<u64>>>,
}

impl RocksDbStore {
    pub fn new(
        cache_traits: ExecutionCacheTraitPointers,
        committee_store: Arc<CommitteeStore>,
        checkpoint_store: Arc<CheckpointStore>,
    ) -> Self {
        Self {
            cache_traits,
            committee_store,
            checkpoint_store,
            highest_verified_checkpoint: Arc::new(Mutex::new(None)),
            highest_synced_checkpoint: Arc::new(Mutex::new(None)),
        }
    }

    pub fn get_objects(&self, object_keys: &[ObjectKey]) -> Vec<Option<Object>> {
        self.cache_traits
            .object_cache_reader
            .multi_get_objects_by_key(object_keys)
    }

    /// See [`crate::execution_cache::ObjectCacheRead::find_object_or_tombstone_lt_or_eq_version_unlocked`].
    pub fn find_object_or_tombstone_lt_or_eq_version_unlocked(
        &self,
        object_id: ObjectID,
        version: SequenceNumber,
    ) -> Option<FoundObjectOrTombstone> {
        self.cache_traits
            .object_cache_reader
            .find_object_or_tombstone_lt_or_eq_version_unlocked(object_id, version)
    }

    pub fn get_last_executed_checkpoint(&self) -> Option<VerifiedCheckpoint> {
        self.checkpoint_store
            .get_highest_executed_checkpoint()
            .expect("db error")
    }
}

impl ReadStore for RocksDbStore {
    fn get_checkpoint_by_digest(&self, digest: &CheckpointDigest) -> Option<VerifiedCheckpoint> {
        self.checkpoint_store
            .get_checkpoint_by_digest(digest)
            .expect("db error")
    }

    fn get_checkpoint_by_sequence_number(
        &self,
        sequence_number: CheckpointSequenceNumber,
    ) -> Option<VerifiedCheckpoint> {
        self.checkpoint_store
            .get_checkpoint_by_sequence_number(sequence_number)
            .expect("db error")
    }

    fn multi_get_checkpoint_by_sequence_number(
        &self,
        sequence_numbers: &[CheckpointSequenceNumber],
    ) -> Vec<Option<VerifiedCheckpoint>> {
        self.checkpoint_store
            .multi_get_checkpoint_by_sequence_number(sequence_numbers)
            .expect("db error")
    }

    fn get_highest_verified_checkpoint(&self) -> Result<VerifiedCheckpoint, StorageError> {
        self.checkpoint_store
            .get_highest_verified_checkpoint()
            .map(|maybe_checkpoint| {
                maybe_checkpoint
                    .expect("storage should have been initialized with genesis checkpoint")
            })
            .map_err(Into::into)
    }

    fn get_highest_synced_checkpoint(&self) -> Result<VerifiedCheckpoint, StorageError> {
        self.checkpoint_store
            .get_highest_synced_checkpoint()
            .map(|maybe_checkpoint| {
                maybe_checkpoint
                    .expect("storage should have been initialized with genesis checkpoint")
            })
            .map_err(Into::into)
    }

    /// One past the higher of the checkpoint store's `HighestPruned` and the
    /// perpetual store's pruned-effects watermark. The checkpoint pruner deletes
    /// a checkpoint's transaction data (and advances the pruned-effects
    /// watermark) before it advances `HighestPruned` in a separate write, so
    /// `HighestPruned` alone can claim checkpoints whose effects are gone.
    fn get_lowest_available_checkpoint(&self) -> Result<CheckpointSequenceNumber, StorageError> {
        let checkpoints_pruned = self
            .checkpoint_store
            .get_highest_pruned_checkpoint_seq_number()
            .map_err(Into::<StorageError>::into)?;
        let effects_pruned = self
            .cache_traits
            .transaction_cache_reader
            .get_highest_pruned_effects_checkpoint();
        Ok(checkpoints_pruned
            .into_iter()
            .chain(effects_pruned)
            .max()
            .map_or(0, |pruned| pruned + 1))
    }

    fn get_full_checkpoint_contents(
        &self,
        sequence_number: Option<CheckpointSequenceNumber>,
        digest: &CheckpointContentsDigest,
    ) -> Option<VersionedFullCheckpointContents> {
        #[cfg(debug_assertions)]
        if let Some(sequence_number) = sequence_number {
            // When sequence_number is provided as an optimization, we want to ensure that
            // the sequence number we get from the db matches the one we provided.
            // Only check this in debug mode though.
            if let Some(loaded_sequence_number) = self
                .checkpoint_store
                .get_sequence_number_by_contents_digest(digest)
                .expect("db error")
            {
                assert_eq!(loaded_sequence_number, sequence_number);
            }
        }

        let sequence_number = sequence_number.or_else(|| {
            self.checkpoint_store
                .get_sequence_number_by_contents_digest(digest)
                .expect("db error")
        });
        if let Some(sequence_number) = sequence_number {
            // Note: We don't use `?` here because we want to tolerate
            // potential db errors due to data corruption.
            // In that case, we will fallback and construct the contents
            // from the individual components as if we could not find the
            // cached full contents.
            if let Ok(Some(contents)) = self
                .checkpoint_store
                .get_full_checkpoint_contents_by_sequence_number(sequence_number)
                .tap_err(|e| {
                    error!(
                        "error getting full checkpoint contents for checkpoint {:?}: {:?}",
                        sequence_number, e
                    )
                })
            {
                return Some(contents);
            }
        }

        // Otherwise gather it from the individual components.
        // Note we can't insert the constructed contents into `full_checkpoint_content`,
        // because it needs to be inserted along with `checkpoint_sequence_by_contents_digest`
        // and `checkpoint_content`. However at this point it's likely we don't know the
        // corresponding sequence number yet.
        self.checkpoint_store
            .get_checkpoint_contents(digest)
            .expect("db error")
            .and_then(|contents| {
                let mut transactions = Vec::with_capacity(contents.size());
                for tx in contents.iter() {
                    if let (Some(t), Some(e)) = (
                        self.get_transaction(&tx.transaction),
                        self.cache_traits
                            .transaction_cache_reader
                            .get_effects(&tx.effects),
                    ) {
                        transactions.push(sui_types::base_types::ExecutionData::new(
                            (*t).clone().into_inner(),
                            e,
                        ))
                    } else {
                        return None;
                    }
                }
                Some(
                    VersionedFullCheckpointContents::from_contents_and_execution_data(
                        contents,
                        transactions.into_iter(),
                    ),
                )
            })
    }

    fn get_committee(&self, epoch: EpochId) -> Option<Arc<Committee>> {
        self.committee_store.get_committee(&epoch).unwrap()
    }

    fn get_transaction(&self, digest: &TransactionDigest) -> Option<Arc<VerifiedTransaction>> {
        self.cache_traits
            .transaction_cache_reader
            .get_transaction_block(digest)
    }

    fn multi_get_transactions(
        &self,
        digests: &[TransactionDigest],
    ) -> Vec<Option<Arc<VerifiedTransaction>>> {
        self.cache_traits
            .transaction_cache_reader
            .multi_get_transaction_blocks(digests)
    }

    fn get_transaction_effects(&self, digest: &TransactionDigest) -> Option<TransactionEffects> {
        self.cache_traits
            .transaction_cache_reader
            .get_executed_effects(digest)
    }

    fn multi_get_transaction_effects(
        &self,
        digests: &[TransactionDigest],
    ) -> Vec<Option<TransactionEffects>> {
        self.cache_traits
            .transaction_cache_reader
            .multi_get_executed_effects(digests)
    }

    fn get_execution_data(&self, digests: &ExecutionDigests) -> Option<ExecutionData> {
        let reader = &self.cache_traits.transaction_cache_reader;
        let transaction = reader.get_transaction_block(&digests.transaction)?;
        let effects = reader.get_effects(&digests.effects)?;
        Some(ExecutionData::new(
            (*transaction).clone().into_inner(),
            effects,
        ))
    }

    fn get_events(&self, digest: &TransactionDigest) -> Option<TransactionEvents> {
        self.cache_traits
            .transaction_cache_reader
            .get_events(digest)
    }

    fn multi_get_events(&self, digests: &[TransactionDigest]) -> Vec<Option<TransactionEvents>> {
        self.cache_traits
            .transaction_cache_reader
            .multi_get_events(digests)
    }

    fn get_unchanged_loaded_runtime_objects(
        &self,
        digest: &TransactionDigest,
    ) -> Option<Vec<ObjectKey>> {
        self.cache_traits
            .transaction_cache_reader
            .get_unchanged_loaded_runtime_objects(digest)
    }

    fn multi_get_unchanged_loaded_runtime_objects(
        &self,
        digests: &[TransactionDigest],
    ) -> Vec<Option<Vec<ObjectKey>>> {
        self.cache_traits
            .transaction_cache_reader
            .multi_get_unchanged_loaded_runtime_objects(digests)
    }

    fn get_transaction_checkpoint(
        &self,
        digest: &TransactionDigest,
    ) -> Option<CheckpointSequenceNumber> {
        self.cache_traits
            .checkpoint_cache
            .deprecated_get_transaction_checkpoint(digest)
            .map(|(_epoch, checkpoint)| checkpoint)
    }

    fn get_latest_checkpoint(&self) -> sui_types::storage::error::Result<VerifiedCheckpoint> {
        self.checkpoint_store
            .get_highest_executed_checkpoint()
            .expect("db error")
            .ok_or_else(|| {
                sui_types::storage::error::Error::missing("unable to get latest checkpoint")
            })
    }

    fn get_checkpoint_contents_by_digest(
        &self,
        digest: &CheckpointContentsDigest,
    ) -> Option<sui_types::messages_checkpoint::CheckpointContents> {
        self.checkpoint_store
            .get_checkpoint_contents(digest)
            .expect("db error")
    }

    fn get_checkpoint_contents_by_sequence_number(
        &self,
        sequence_number: CheckpointSequenceNumber,
    ) -> Option<sui_types::messages_checkpoint::CheckpointContents> {
        match self.get_checkpoint_by_sequence_number(sequence_number) {
            Some(checkpoint) => self.get_checkpoint_contents_by_digest(&checkpoint.content_digest),
            None => None,
        }
    }
}

impl ObjectStore for RocksDbStore {
    fn get_object(&self, object_id: &sui_types::base_types::ObjectID) -> Option<Object> {
        self.cache_traits.object_store.get_object(object_id)
    }

    fn get_object_by_key(
        &self,
        object_id: &sui_types::base_types::ObjectID,
        version: sui_types::base_types::VersionNumber,
    ) -> Option<Object> {
        self.cache_traits
            .object_store
            .get_object_by_key(object_id, version)
    }

    fn multi_get_objects_by_key(&self, object_keys: &[ObjectKey]) -> Vec<Option<Object>> {
        self.cache_traits
            .object_cache_reader
            .multi_get_objects_by_key(object_keys)
    }
}

impl WriteStore for RocksDbStore {
    fn insert_checkpoint(
        &self,
        checkpoint: &VerifiedCheckpoint,
    ) -> Result<(), sui_types::storage::error::Error> {
        if let Some(EndOfEpochData {
            next_epoch_committee,
            ..
        }) = checkpoint.end_of_epoch_data.as_ref()
        {
            let next_committee = next_epoch_committee.iter().cloned().collect();
            let committee =
                Committee::new(checkpoint.epoch().checked_add(1).unwrap(), next_committee);
            self.insert_committee(committee)?;
        }

        self.checkpoint_store
            .insert_verified_checkpoint(checkpoint)
            .map_err(Into::into)
    }

    fn update_highest_synced_checkpoint(
        &self,
        checkpoint: &VerifiedCheckpoint,
    ) -> Result<(), sui_types::storage::error::Error> {
        let mut locked = self.highest_synced_checkpoint.lock();
        if locked.is_some() && locked.unwrap() >= checkpoint.sequence_number {
            return Ok(());
        }
        self.checkpoint_store
            .update_highest_synced_checkpoint(checkpoint)
            .map_err(sui_types::storage::error::Error::custom)?;
        *locked = Some(checkpoint.sequence_number);
        Ok(())
    }

    fn update_highest_verified_checkpoint(
        &self,
        checkpoint: &VerifiedCheckpoint,
    ) -> Result<(), sui_types::storage::error::Error> {
        let mut locked = self.highest_verified_checkpoint.lock();
        if locked.is_some() && locked.unwrap() >= checkpoint.sequence_number {
            return Ok(());
        }
        self.checkpoint_store
            .update_highest_verified_checkpoint(checkpoint)
            .map_err(sui_types::storage::error::Error::custom)?;
        *locked = Some(checkpoint.sequence_number);
        Ok(())
    }

    fn insert_checkpoint_contents(
        &self,
        checkpoint: &VerifiedCheckpoint,
        contents: VerifiedCheckpointContents,
    ) -> Result<(), sui_types::storage::error::Error> {
        self.cache_traits
            .state_sync_store
            .multi_insert_transaction_and_effects(contents.transactions());
        self.checkpoint_store
            .insert_verified_checkpoint_contents(checkpoint, contents)
            .map_err(Into::into)
    }

    fn insert_transactions_and_effects(
        &self,
        data: &[VerifiedExecutionData],
    ) -> Result<(), sui_types::storage::error::Error> {
        self.cache_traits
            .state_sync_store
            .multi_insert_transaction_and_effects(data);
        Ok(())
    }

    fn insert_checkpoint_contents_digests(
        &self,
        checkpoint: &VerifiedCheckpoint,
        contents: CheckpointContents,
    ) -> Result<(), sui_types::storage::error::Error> {
        debug_assert!(
            contents
                .iter()
                .all(|digests| self.get_execution_data(digests).is_some()),
            "checkpoint {} contents inserted before its transactions and effects",
            checkpoint.sequence_number()
        );
        self.checkpoint_store
            .insert_checkpoint_contents_digests(checkpoint, &contents)
            .map_err(Into::into)
    }

    fn insert_committee(
        &self,
        new_committee: Committee,
    ) -> Result<(), sui_types::storage::error::Error> {
        self.committee_store
            .insert_new_committee(&new_committee)
            .unwrap();
        Ok(())
    }
}

pub struct RestReadStore {
    state: Arc<AuthorityState>,
    rocks: RocksDbStore,
}

impl RestReadStore {
    pub fn new(state: Arc<AuthorityState>, rocks: RocksDbStore) -> Self {
        Self { state, rocks }
    }
}

/// The live object of `entry`, or `None` if it is a tombstone.
fn live_object(entry: ObjectOrTombstone) -> Option<Object> {
    match entry {
        ObjectOrTombstone::Object(object) => Some(object),
        ObjectOrTombstone::Tombstone(_) => None,
    }
}

/// [`RpcStateReader::retains_object_version_history`] for the node `state` runs on: `false` when
/// it runs the tidehunter objects compactor.
fn retains_object_version_history(state: &AuthorityState) -> bool {
    !AuthorityPerpetualTablesOptions::objects_compactor_enabled(
        state.config.intended_node_role().is_validator(),
        state
            .config
            .authority_store_pruning_config
            .num_epochs_to_retain,
    )
}

/// [`RpcStateReader::get_latest_object_or_tombstone`] through the execution cache, which also
/// holds a removal not yet committed to the store.
fn latest_object_or_tombstone(
    state: &AuthorityState,
    object_id: &ObjectID,
) -> Option<ObjectOrTombstone> {
    state
        .get_object_cache_reader()
        .get_latest_object_or_tombstone(*object_id)
        .map(|(_, item)| item)
}

impl ObjectStore for RestReadStore {
    fn get_object(&self, object_id: &sui_types::base_types::ObjectID) -> Option<Object> {
        self.rocks.get_object(object_id)
    }

    fn get_object_by_key(
        &self,
        object_id: &sui_types::base_types::ObjectID,
        version: sui_types::base_types::VersionNumber,
    ) -> Option<Object> {
        self.rocks.get_object_by_key(object_id, version)
    }

    fn multi_get_objects_by_key(&self, object_keys: &[ObjectKey]) -> Vec<Option<Object>> {
        self.rocks.multi_get_objects_by_key(object_keys)
    }
}

impl ReadStore for RestReadStore {
    fn get_committee(&self, epoch: EpochId) -> Option<Arc<Committee>> {
        self.rocks.get_committee(epoch)
    }

    fn get_latest_checkpoint(&self) -> sui_types::storage::error::Result<VerifiedCheckpoint> {
        self.rocks.get_latest_checkpoint()
    }

    fn get_highest_verified_checkpoint(
        &self,
    ) -> sui_types::storage::error::Result<VerifiedCheckpoint> {
        self.rocks.get_highest_verified_checkpoint()
    }

    fn get_highest_synced_checkpoint(
        &self,
    ) -> sui_types::storage::error::Result<VerifiedCheckpoint> {
        self.rocks.get_highest_synced_checkpoint()
    }

    fn get_lowest_available_checkpoint(
        &self,
    ) -> sui_types::storage::error::Result<CheckpointSequenceNumber> {
        self.rocks.get_lowest_available_checkpoint()
    }

    fn get_checkpoint_by_digest(&self, digest: &CheckpointDigest) -> Option<VerifiedCheckpoint> {
        self.rocks.get_checkpoint_by_digest(digest)
    }

    fn get_checkpoint_by_sequence_number(
        &self,
        sequence_number: CheckpointSequenceNumber,
    ) -> Option<VerifiedCheckpoint> {
        self.rocks
            .get_checkpoint_by_sequence_number(sequence_number)
    }

    fn multi_get_checkpoint_by_sequence_number(
        &self,
        sequence_numbers: &[CheckpointSequenceNumber],
    ) -> Vec<Option<VerifiedCheckpoint>> {
        self.rocks
            .multi_get_checkpoint_by_sequence_number(sequence_numbers)
    }

    fn get_checkpoint_contents_by_digest(
        &self,
        digest: &CheckpointContentsDigest,
    ) -> Option<sui_types::messages_checkpoint::CheckpointContents> {
        self.rocks.get_checkpoint_contents_by_digest(digest)
    }

    fn get_checkpoint_contents_by_sequence_number(
        &self,
        sequence_number: CheckpointSequenceNumber,
    ) -> Option<sui_types::messages_checkpoint::CheckpointContents> {
        self.rocks
            .get_checkpoint_contents_by_sequence_number(sequence_number)
    }

    fn get_transaction(&self, digest: &TransactionDigest) -> Option<Arc<VerifiedTransaction>> {
        self.rocks.get_transaction(digest)
    }

    fn multi_get_transactions(
        &self,
        digests: &[TransactionDigest],
    ) -> Vec<Option<Arc<VerifiedTransaction>>> {
        self.rocks.multi_get_transactions(digests)
    }

    fn get_transaction_effects(&self, digest: &TransactionDigest) -> Option<TransactionEffects> {
        self.rocks.get_transaction_effects(digest)
    }

    fn multi_get_transaction_effects(
        &self,
        digests: &[TransactionDigest],
    ) -> Vec<Option<TransactionEffects>> {
        self.rocks.multi_get_transaction_effects(digests)
    }

    fn get_events(&self, digest: &TransactionDigest) -> Option<TransactionEvents> {
        self.rocks.get_events(digest)
    }

    fn multi_get_events(&self, digests: &[TransactionDigest]) -> Vec<Option<TransactionEvents>> {
        self.rocks.multi_get_events(digests)
    }

    fn get_full_checkpoint_contents(
        &self,
        sequence_number: Option<CheckpointSequenceNumber>,
        digest: &CheckpointContentsDigest,
    ) -> Option<VersionedFullCheckpointContents> {
        self.rocks
            .get_full_checkpoint_contents(sequence_number, digest)
    }

    fn get_unchanged_loaded_runtime_objects(
        &self,
        digest: &TransactionDigest,
    ) -> Option<Vec<ObjectKey>> {
        self.rocks.get_unchanged_loaded_runtime_objects(digest)
    }

    fn multi_get_unchanged_loaded_runtime_objects(
        &self,
        digests: &[TransactionDigest],
    ) -> Vec<Option<Vec<ObjectKey>>> {
        self.rocks
            .multi_get_unchanged_loaded_runtime_objects(digests)
    }

    fn get_transaction_checkpoint(
        &self,
        digest: &TransactionDigest,
    ) -> Option<CheckpointSequenceNumber> {
        self.rocks.get_transaction_checkpoint(digest)
    }
}

impl BackingPackageStore for RestReadStore {
    fn get_package_object(&self, _package_id: &ObjectID) -> SuiResult<Option<PackageObject>> {
        Err(SuiErrorKind::UnsupportedFeatureError {
            error: "RestReadStore does not support loading package objects".to_string(),
        }
        .into())
    }
}

impl RuntimeObjectResolver for RestReadStore {
    fn read_child_object(
        &self,
        parent: &ObjectID,
        child: &ObjectID,
        child_version_upper_bound: SequenceNumber,
    ) -> SuiResult<Option<Object>> {
        Ok(self.get_object(child).filter(|o| {
            o.version() <= child_version_upper_bound
                && o.owner == Owner::ObjectOwner((*parent).into())
        }))
    }

    fn get_object_received_at_version(
        &self,
        _owner: &ObjectID,
        _receiving_object_id: &ObjectID,
        _receive_object_at_version: SequenceNumber,
        _epoch_id: EpochId,
    ) -> SuiResult<Option<Object>> {
        Err(SuiErrorKind::UnsupportedFeatureError {
            error: "RestReadStore does not support receiving objects".to_string(),
        }
        .into())
    }
}

impl RpcStateReader for RestReadStore {
    fn get_lowest_available_checkpoint_objects(
        &self,
    ) -> sui_types::storage::error::Result<CheckpointSequenceNumber> {
        Ok(self
            .state
            .get_object_cache_reader()
            .get_highest_pruned_checkpoint()
            .map(|cp| cp + 1)
            .unwrap_or(0))
    }

    fn retains_object_version_history(&self) -> bool {
        retains_object_version_history(&self.state)
    }

    fn get_chain_identifier(&self) -> Result<sui_types::digests::ChainIdentifier> {
        Ok(self.state.get_chain_identifier())
    }

    fn indexes(&self) -> Option<&dyn RpcIndexes> {
        // The legacy `rpc-index` backend has been removed; a node serving
        // reads through `RestReadStore` exposes no index surface. Index
        // reads are served by the embedded rpc-store via `RpcStoreReadStore`.
        None
    }

    fn layout_package_store(&self) -> Box<dyn BackingPackageStore + Send + Sync + '_> {
        Box::new(self.state.get_backing_package_store().as_ref())
    }

    fn get_latest_object_or_tombstone(
        &self,
        object_id: &ObjectID,
    ) -> Result<Option<ObjectOrTombstone>> {
        Ok(latest_object_or_tombstone(&self.state, object_id))
    }

    /// Unchecked: without rpc indexes this node cannot tell a pruned entry apart (see the trait
    /// method). A tombstone at exactly the requested version records a removal there either way.
    fn get_object_or_tombstone_lt_or_eq_version(
        &self,
        object_id: &ObjectID,
        version: SequenceNumber,
    ) -> Result<Option<ObjectOrTombstone>> {
        Ok(self
            .rocks
            .find_object_or_tombstone_lt_or_eq_version_unlocked(*object_id, version)
            .map(|found| found.entry))
    }
}

/// Read store backed by the embedded [`sui_rpc_store`] indexer.
///
/// Like [`RestReadStore`] it serves the `sui-rpc-api` trait stack, but it
/// additionally exposes the index surface (which [`RestReadStore`] no
/// longer does). This wrapper composes two backends:
///
/// - **Raw chain data** — objects, transactions, effects, events,
///   checkpoints, committees, and child-object resolution — is served
///   from the validator's perpetual / checkpoint stores
///   ([`RocksDbStore`]), exactly like [`RestReadStore`]. The embedded
///   rpc-store does not duplicate this data.
/// - **The index surface** ([`RpcIndexes`]) — owner / type / balance /
///   coin / package-version listings, epoch info, and the
///   ledger-history bitmaps — is served from the
///   [`RpcStoreReader`], except listings of an object's versions, which
///   the rpc-store does not hold: they are read through the object cache
///   ([`crate::execution_cache::ObjectCacheRead::object_versions_iter`]),
///   which sees only versions committed to the perpetual store. That is
///   every version a listing reads: the index bounds every listing at a
///   checkpoint the indexer has received, and the indexer receives a
///   checkpoint only after the executor commits its objects (the
///   executor's broadcast follows the commit, and
///   [`crate::rpc_store_ingestion_client::PerpetualStoreIngestionClient`]
///   serves only executed checkpoints).
///
/// The object/state available range is the intersection of the two
/// backends' ranges (`max` of their lower bounds): a consistent read
/// at checkpoint `C` needs both the object bytes (perpetual store) and
/// the index rows (rpc-store) at `C`. Ledger-history-specific
/// availability (bounded by the history backfill watermark) is exposed
/// separately.
pub struct RpcStoreReadStore {
    state: Arc<AuthorityState>,
    rocks: RocksDbStore,
    reader: RpcStoreReader,
}

impl RpcStoreReadStore {
    pub fn new(state: Arc<AuthorityState>, rocks: RocksDbStore, reader: RpcStoreReader) -> Self {
        Self {
            state,
            rocks,
            reader,
        }
    }

    /// The newest entry of `object_id` at or below `version`, the object or the tombstone of a
    /// deletion or wrap, for RPC reads. `None` if it has none, or if the entry found may be a
    /// version below this node's object lower bound, which callers answer as a version that may
    /// have been pruned.
    ///
    /// The store read does not skip the versions the pruner range-deleted: skipping them would
    /// make the first read after each pruner write rebuild the memtable's list of range
    /// deletions. So it can find a pruned version older than the one that replaced it, and
    /// `may_be_below_objects_lower_bound` checks the entry against the rpc indexes.
    ///
    /// While the rpc indexes backfill history after a restore
    /// ([`RpcIndexes::is_history_backfill_pending`]), the version they record at the end of a
    /// checkpoint can be a later one, so the check can refuse a version this node holds. RPC
    /// refuses `max_version` and `at_checkpoint` reads until the backfill finishes. An exact
    /// `version` read reaches the check, through [`Self::find_object_or_tombstone_by_key`], only
    /// to look for a removal after its live read missed, so during the backfill it can answer as
    /// a version that may have been pruned rather than describe that removal.
    pub fn find_object_or_tombstone_lt_or_eq_version(
        &self,
        object_id: ObjectID,
        version: SequenceNumber,
    ) -> Result<Option<ObjectOrTombstone>> {
        let found = self
            .rocks
            .find_object_or_tombstone_lt_or_eq_version_unlocked(object_id, version);
        self.unless_below_objects_lower_bound(object_id, found)
    }

    /// The entry of `object_id` at exactly `version`, as
    /// [`Self::find_object_or_tombstone_lt_or_eq_version`] reads it, but an older entry is
    /// discarded before the check against the object lower bound, which then runs only for an
    /// entry at `version`.
    pub fn find_object_or_tombstone_by_key(
        &self,
        object_id: ObjectID,
        version: SequenceNumber,
    ) -> Result<Option<ObjectOrTombstone>> {
        let found = self
            .rocks
            .find_object_or_tombstone_lt_or_eq_version_unlocked(object_id, version)
            .filter(|found| match &found.entry {
                ObjectOrTombstone::Object(object) => object.version() == version,
                ObjectOrTombstone::Tombstone((_, removed_at, _)) => *removed_at == version,
            });
        self.unless_below_objects_lower_bound(object_id, found)
    }

    /// The entry `found`, unless it may be a version below this node's object lower bound (see
    /// [`may_be_below_objects_lower_bound`]).
    fn unless_below_objects_lower_bound(
        &self,
        object_id: ObjectID,
        found: Option<FoundObjectOrTombstone>,
    ) -> Result<Option<ObjectOrTombstone>> {
        let Some(found) = found else {
            return Ok(None);
        };
        if may_be_below_objects_lower_bound(
            &found,
            || self.get_lowest_available_checkpoint_objects(),
            |checkpoint| {
                self.reader
                    .get_object_version_at_checkpoint(object_id, checkpoint)
            },
            |digest| self.rocks.get_transaction_checkpoint(digest),
            || {
                let Some(row) = self
                    .reader
                    .schema()
                    .iter_object_versions_by_checkpoint(object_id)
                    .map_err(StorageError::custom)?
                    .next()
                else {
                    return Ok(None);
                };
                let (key, value) = row.map_err(StorageError::custom)?;
                Ok(Some((
                    key.checkpoint,
                    SequenceNumber::from_u64(value.into_inner().version),
                )))
            },
        )? {
            return Ok(None);
        }
        Ok(Some(found.entry))
    }
}

/// Whether `found`, the newest entry of an object at or below a bound read without skipping
/// range-deleted versions, may be a version below the object lower bound
/// (`objects_lower_bound`, see [`RpcStateReader::get_lowest_available_checkpoint_objects`]):
/// one the pruner deleted that compaction has not dropped yet, older than a version that
/// replaced it which compaction did drop, or one about to be pruned. The entry is checked
/// against the object's version at the end of the last pruned checkpoint P, the checkpoint
/// before the lower bound, which the rpc indexes record (`version_at_checkpoint`). That is the
/// oldest version the lower bound keeps: every version below it was replaced by the end of P.
///
/// - An uncommitted entry (see [`FoundObjectOrTombstone::committed`]), or any entry on a node
///   whose lower bound is 0, is not. Neither reads the lower bound or the indexes.
/// - An entry at or above the object's version at P is not: every version above it is one the
///   lower bound keeps, so the read skipped none. An entry below it is below the lower bound.
/// - With no version at P, the object did not exist yet, or it was deleted or wrapped by then
///   and pruning removed its index history. A live entry written in a checkpoint after P
///   (`transaction_checkpoint` of the transaction that wrote it) is not: every version above it
///   was written later still. A live entry written at or before P may be.
/// - A tombstone records no transaction, and a node that prunes checkpoints but keeps object
///   versions no longer has the checkpoint of a live entry's transaction once that checkpoint
///   is pruned. With no version at P, such an entry is not below the lower bound if the
///   object's first index row (`first_indexed_change`: its checkpoint and version) is after P
///   and records a version at or below the entry's: the entry was written at or after that
///   change. Otherwise it may be. The first row is read only for such an entry.
///
/// Read the lower bound after the entry: pruning raises it in the same write that range-deletes
/// the versions it prunes, so a pruned version the read found is below the object's version at
/// the P read then.
fn may_be_below_objects_lower_bound(
    found: &FoundObjectOrTombstone,
    objects_lower_bound: impl FnOnce() -> Result<CheckpointSequenceNumber>,
    version_at_checkpoint: impl FnOnce(CheckpointSequenceNumber) -> Result<Option<SequenceNumber>>,
    transaction_checkpoint: impl FnOnce(&TransactionDigest) -> Option<CheckpointSequenceNumber>,
    first_indexed_change: impl FnOnce() -> Result<Option<(CheckpointSequenceNumber, SequenceNumber)>>,
) -> Result<bool> {
    if !found.committed {
        return Ok(false);
    }
    let Some(last_pruned) = objects_lower_bound()?.checked_sub(1) else {
        return Ok(false);
    };
    let may_predate_first_indexed_change = |version: SequenceNumber| -> Result<bool> {
        Ok(
            !first_indexed_change()?.is_some_and(|(checkpoint, first_version)| {
                checkpoint > last_pruned && first_version <= version
            }),
        )
    };
    Ok(match version_at_checkpoint(last_pruned)? {
        Some(version_at_last_pruned) => {
            let found_version = match &found.entry {
                ObjectOrTombstone::Object(object) => object.version(),
                ObjectOrTombstone::Tombstone((_, version, _)) => *version,
            };
            found_version < version_at_last_pruned
        }
        None => match &found.entry {
            ObjectOrTombstone::Object(object) => {
                match transaction_checkpoint(&object.previous_transaction) {
                    Some(checkpoint) => checkpoint <= last_pruned,
                    None => may_predate_first_indexed_change(object.version())?,
                }
            }
            ObjectOrTombstone::Tombstone((_, version, _)) => {
                may_predate_first_indexed_change(*version)?
            }
        },
    })
}

impl ObjectStore for RpcStoreReadStore {
    fn get_object(&self, object_id: &ObjectID) -> Option<Object> {
        self.rocks.get_object(object_id)
    }

    fn get_object_by_key(&self, object_id: &ObjectID, version: SequenceNumber) -> Option<Object> {
        self.rocks.get_object_by_key(object_id, version)
    }

    fn multi_get_objects_by_key(&self, object_keys: &[ObjectKey]) -> Vec<Option<Object>> {
        self.rocks.multi_get_objects_by_key(object_keys)
    }
}

impl ReadStore for RpcStoreReadStore {
    fn get_committee(&self, epoch: EpochId) -> Option<Arc<Committee>> {
        self.rocks.get_committee(epoch)
    }

    fn get_latest_checkpoint(&self) -> Result<VerifiedCheckpoint> {
        let latest = self.rocks.get_latest_checkpoint()?;
        // Bound the reported tip to what the live-object index has committed.
        // The embedded indexer follows the tip asynchronously, so without this
        // the rpc-api could surface a checkpoint -- and the transactions in it
        // -- whose indexed state (owned objects, balances, coins) is not yet
        // readable, breaking read-after-write consistency. The history cohort
        // backfills independently and bounds the ledger-history APIs
        // separately, so it does not constrain this tip.
        match self.reader.highest_live_committed_checkpoint()? {
            Some(indexed) if indexed < latest.sequence_number => self
                .rocks
                .get_checkpoint_by_sequence_number(indexed)
                .ok_or_else(|| {
                    StorageError::missing(format!(
                        "live-indexed checkpoint {indexed} missing from the checkpoint store"
                    ))
                }),
            Some(_) => Ok(latest),
            // Fail closed: no live watermark means the index surface is
            // empty -- a fresh node whose indexer has not committed its
            // first checkpoint yet. Reporting the executed tip here would
            // advertise checkpoints whose indexed state is not readable,
            // the exact inconsistency the bound above exists to prevent.
            // The window closes with the live cohort's first commit,
            // moments after startup.
            None => Err(StorageError::missing(
                "the embedded rpc-store's live index has no committed checkpoint yet",
            )),
        }
    }

    fn get_highest_verified_checkpoint(&self) -> Result<VerifiedCheckpoint> {
        self.rocks.get_highest_verified_checkpoint()
    }

    fn get_highest_synced_checkpoint(&self) -> Result<VerifiedCheckpoint> {
        self.rocks.get_highest_synced_checkpoint()
    }

    fn get_lowest_available_checkpoint(&self) -> Result<CheckpointSequenceNumber> {
        // A consistent read needs both the raw chain data (perpetual
        // store) and the index rows (rpc-store), so the available range
        // starts at the higher of the two lower bounds.
        let perpetual = self.rocks.get_lowest_available_checkpoint()?;
        let rpc_store = self.reader.get_lowest_available_checkpoint()?;
        Ok(perpetual.max(rpc_store))
    }

    fn get_checkpoint_by_digest(&self, digest: &CheckpointDigest) -> Option<VerifiedCheckpoint> {
        self.rocks.get_checkpoint_by_digest(digest)
    }

    fn get_checkpoint_by_sequence_number(
        &self,
        sequence_number: CheckpointSequenceNumber,
    ) -> Option<VerifiedCheckpoint> {
        self.rocks
            .get_checkpoint_by_sequence_number(sequence_number)
    }

    fn multi_get_checkpoint_by_sequence_number(
        &self,
        sequence_numbers: &[CheckpointSequenceNumber],
    ) -> Vec<Option<VerifiedCheckpoint>> {
        self.rocks
            .multi_get_checkpoint_by_sequence_number(sequence_numbers)
    }

    fn get_checkpoint_contents_by_digest(
        &self,
        digest: &CheckpointContentsDigest,
    ) -> Option<sui_types::messages_checkpoint::CheckpointContents> {
        self.rocks.get_checkpoint_contents_by_digest(digest)
    }

    fn get_checkpoint_contents_by_sequence_number(
        &self,
        sequence_number: CheckpointSequenceNumber,
    ) -> Option<sui_types::messages_checkpoint::CheckpointContents> {
        self.rocks
            .get_checkpoint_contents_by_sequence_number(sequence_number)
    }

    fn get_transaction(&self, digest: &TransactionDigest) -> Option<Arc<VerifiedTransaction>> {
        self.rocks.get_transaction(digest)
    }

    fn multi_get_transactions(
        &self,
        digests: &[TransactionDigest],
    ) -> Vec<Option<Arc<VerifiedTransaction>>> {
        self.rocks.multi_get_transactions(digests)
    }

    fn get_transaction_effects(&self, digest: &TransactionDigest) -> Option<TransactionEffects> {
        self.rocks.get_transaction_effects(digest)
    }

    fn multi_get_transaction_effects(
        &self,
        digests: &[TransactionDigest],
    ) -> Vec<Option<TransactionEffects>> {
        self.rocks.multi_get_transaction_effects(digests)
    }

    fn get_events(&self, digest: &TransactionDigest) -> Option<TransactionEvents> {
        self.rocks.get_events(digest)
    }

    fn multi_get_events(&self, digests: &[TransactionDigest]) -> Vec<Option<TransactionEvents>> {
        self.rocks.multi_get_events(digests)
    }

    fn get_full_checkpoint_contents(
        &self,
        sequence_number: Option<CheckpointSequenceNumber>,
        digest: &CheckpointContentsDigest,
    ) -> Option<VersionedFullCheckpointContents> {
        self.rocks
            .get_full_checkpoint_contents(sequence_number, digest)
    }

    fn get_unchanged_loaded_runtime_objects(
        &self,
        digest: &TransactionDigest,
    ) -> Option<Vec<ObjectKey>> {
        self.rocks.get_unchanged_loaded_runtime_objects(digest)
    }

    fn multi_get_unchanged_loaded_runtime_objects(
        &self,
        digests: &[TransactionDigest],
    ) -> Vec<Option<Vec<ObjectKey>>> {
        self.rocks
            .multi_get_unchanged_loaded_runtime_objects(digests)
    }

    fn get_transaction_checkpoint(
        &self,
        digest: &TransactionDigest,
    ) -> Option<CheckpointSequenceNumber> {
        self.rocks.get_transaction_checkpoint(digest)
    }
}

impl BackingPackageStore for RpcStoreReadStore {
    fn get_package_object(&self, _package_id: &ObjectID) -> SuiResult<Option<PackageObject>> {
        Err(SuiErrorKind::UnsupportedFeatureError {
            error: "RpcStoreReadStore does not support loading package objects".to_string(),
        }
        .into())
    }
}

impl RuntimeObjectResolver for RpcStoreReadStore {
    fn read_child_object(
        &self,
        parent: &ObjectID,
        child: &ObjectID,
        child_version_upper_bound: SequenceNumber,
    ) -> SuiResult<Option<Object>> {
        Ok(self.get_object(child).filter(|o| {
            o.version() <= child_version_upper_bound
                && o.owner == Owner::ObjectOwner((*parent).into())
        }))
    }

    fn get_object_received_at_version(
        &self,
        _owner: &ObjectID,
        _receiving_object_id: &ObjectID,
        _receive_object_at_version: SequenceNumber,
        _epoch_id: EpochId,
    ) -> SuiResult<Option<Object>> {
        Err(SuiErrorKind::UnsupportedFeatureError {
            error: "RpcStoreReadStore does not support receiving objects".to_string(),
        }
        .into())
    }
}

impl RpcStateReader for RpcStoreReadStore {
    fn get_lowest_available_checkpoint_objects(&self) -> Result<CheckpointSequenceNumber> {
        let perpetual = self
            .state
            .get_object_cache_reader()
            .get_highest_pruned_checkpoint()
            .map(|cp| cp + 1)
            .unwrap_or(0);
        let rpc_store = self.reader.get_lowest_available_checkpoint_objects()?;
        Ok(perpetual.max(rpc_store))
    }

    fn retains_object_version_history(&self) -> bool {
        retains_object_version_history(&self.state)
    }

    fn get_chain_identifier(&self) -> Result<sui_types::digests::ChainIdentifier> {
        Ok(self.state.get_chain_identifier())
    }

    fn indexes(&self) -> Option<&dyn RpcIndexes> {
        Some(self)
    }

    fn get_object_lt_or_eq_version(
        &self,
        object_id: &ObjectID,
        version: SequenceNumber,
    ) -> Result<Option<Object>> {
        Ok(self
            .find_object_or_tombstone_lt_or_eq_version(*object_id, version)?
            .and_then(live_object))
    }

    fn get_latest_object_or_tombstone(
        &self,
        object_id: &ObjectID,
    ) -> Result<Option<ObjectOrTombstone>> {
        Ok(latest_object_or_tombstone(&self.state, object_id))
    }

    fn get_object_or_tombstone_lt_or_eq_version(
        &self,
        object_id: &ObjectID,
        version: SequenceNumber,
    ) -> Result<Option<ObjectOrTombstone>> {
        self.find_object_or_tombstone_lt_or_eq_version(*object_id, version)
    }

    fn get_object_or_tombstone_by_key(
        &self,
        object_id: &ObjectID,
        version: SequenceNumber,
    ) -> Result<Option<ObjectOrTombstone>> {
        self.find_object_or_tombstone_by_key(*object_id, version)
    }

    fn get_highest_executed_checkpoint_seq_number(&self) -> Result<CheckpointSequenceNumber> {
        // The raw executed tip, read straight from the checkpoint store. Unlike
        // `get_latest_checkpoint`, this is not bounded to the live-object index
        // frontier -- the health check measures how far that frontier trails
        // the executed tip, so it must see the unbounded value.
        Ok(*self.rocks.get_latest_checkpoint()?.sequence_number())
    }

    fn layout_package_store(&self) -> Box<dyn BackingPackageStore + Send + Sync + '_> {
        Box::new(self.state.get_backing_package_store().as_ref())
    }
}

impl RpcIndexes for RpcStoreReadStore {
    fn get_epoch_info(&self, epoch: EpochId) -> Result<Option<sui_types::storage::EpochInfo>> {
        self.reader.get_epoch_info(epoch)
    }

    fn owned_objects_iter(
        &self,
        owner: SuiAddress,
        object_type: Option<StructTag>,
        cursor: Option<OwnedObjectInfo>,
    ) -> Result<Box<dyn Iterator<Item = Result<OwnedObjectInfo, TypedStoreError>> + '_>> {
        self.reader.owned_objects_iter(owner, object_type, cursor)
    }

    fn dynamic_field_iter(
        &self,
        parent: ObjectID,
        cursor: Option<DynamicFieldKey>,
    ) -> Result<Box<dyn Iterator<Item = Result<DynamicFieldKey, TypedStoreError>> + '_>> {
        self.reader.dynamic_field_iter(parent, cursor)
    }

    fn get_coin_info(&self, coin_type: &StructTag) -> Result<Option<CoinInfo>> {
        self.reader.get_coin_info(coin_type)
    }

    fn get_balance(
        &self,
        owner: &SuiAddress,
        coin_type: &StructTag,
    ) -> Result<Option<BalanceInfo>> {
        self.reader.get_balance(owner, coin_type)
    }

    fn balance_iter(
        &self,
        owner: &SuiAddress,
        cursor: Option<(SuiAddress, StructTag)>,
    ) -> Result<BalanceIterator<'_>> {
        self.reader.balance_iter(owner, cursor)
    }

    fn package_versions_iter(
        &self,
        original_id: ObjectID,
        start_version: u64,
        end_version_exclusive: u64,
        descending: bool,
    ) -> Result<sui_types::storage::PackageVersionsIterator<'_>> {
        self.reader.package_versions_iter(
            original_id,
            start_version,
            end_version_exclusive,
            descending,
        )
    }

    fn packages_by_checkpoint_iter(
        &self,
        lower: std::ops::Bound<sui_types::storage::PackagePublishKey>,
        upper: std::ops::Bound<sui_types::storage::PackagePublishKey>,
        descending: bool,
    ) -> Result<sui_types::storage::PackagesByCheckpointIterator<'_>> {
        self.reader
            .packages_by_checkpoint_iter(lower, upper, descending)
    }

    fn get_package_version_storage_id(
        &self,
        original_id: ObjectID,
        version: u64,
    ) -> Result<Option<ObjectID>> {
        self.reader
            .get_package_version_storage_id(original_id, version)
    }

    fn get_package_at_checkpoint(
        &self,
        original_id: ObjectID,
        checkpoint: CheckpointSequenceNumber,
    ) -> Result<Option<(u64, ObjectID)>> {
        self.reader
            .get_package_at_checkpoint(original_id, checkpoint)
    }

    fn get_object_version_at_checkpoint(
        &self,
        object_id: ObjectID,
        checkpoint: CheckpointSequenceNumber,
    ) -> Result<Option<SequenceNumber>> {
        self.reader
            .get_object_version_at_checkpoint(object_id, checkpoint)
    }

    fn first_checkpoint_with_object_version(
        &self,
        object_id: ObjectID,
        checkpoints: RangeInclusive<CheckpointSequenceNumber>,
    ) -> Result<Option<(CheckpointSequenceNumber, SequenceNumber)>> {
        self.reader
            .first_checkpoint_with_object_version(object_id, checkpoints)
    }

    fn object_versions_iter(
        &self,
        object_id: ObjectID,
        versions: RangeInclusive<SequenceNumber>,
        descending: bool,
    ) -> Result<ObjectVersionsIterator<'_>> {
        let versions = self
            .state
            .get_object_cache_reader()
            .object_versions_iter(object_id, versions, descending)
            .map_err(StorageError::custom)?;
        Ok(Box::new(
            versions.map(|version| version.map_err(StorageError::custom)),
        ))
    }

    fn get_highest_indexed_checkpoint_seq_number(
        &self,
    ) -> Result<Option<CheckpointSequenceNumber>> {
        self.reader.get_highest_indexed_checkpoint_seq_number()
    }

    fn is_history_backfill_pending(&self) -> Result<bool> {
        self.reader.is_history_backfill_pending()
    }

    fn get_highest_live_indexed_checkpoint_seq_number(
        &self,
    ) -> Result<Option<CheckpointSequenceNumber>> {
        self.reader.get_highest_live_indexed_checkpoint_seq_number()
    }

    fn get_highest_indexed_checkpoint_timestamp_ms(&self) -> Result<Option<u64>> {
        self.reader.get_highest_indexed_checkpoint_timestamp_ms()
    }

    fn get_highest_live_indexed_checkpoint_timestamp_ms(&self) -> Result<Option<u64>> {
        self.reader
            .get_highest_live_indexed_checkpoint_timestamp_ms()
    }

    fn ledger_tx_seq_digest(&self, tx_seq: u64) -> Result<Option<LedgerTxSeqDigest>> {
        self.reader.ledger_tx_seq_digest(tx_seq)
    }

    fn ledger_tx_seq_by_digest(&self, digest: &TransactionDigest) -> Result<Option<u64>> {
        self.reader.ledger_tx_seq_by_digest(digest)
    }

    fn ledger_tx_seq_digest_multi_get(
        &self,
        tx_seqs: &[u64],
    ) -> Result<Vec<Option<LedgerTxSeqDigest>>> {
        self.reader.ledger_tx_seq_digest_multi_get(tx_seqs)
    }

    fn ledger_tx_seq_digest_iter(
        &self,
        start: u64,
        end_exclusive: u64,
        descending: bool,
    ) -> Result<LedgerTxSeqDigestIterator<'_>> {
        self.reader
            .ledger_tx_seq_digest_iter(start, end_exclusive, descending)
    }

    fn transaction_bitmap_bucket_iter(
        &self,
        dimension_key: Vec<u8>,
        start_bucket: u64,
        end_bucket_exclusive: u64,
        descending: bool,
    ) -> Result<LedgerBitmapBucketIterator<'_>> {
        self.reader.transaction_bitmap_bucket_iter(
            dimension_key,
            start_bucket,
            end_bucket_exclusive,
            descending,
        )
    }

    fn event_bitmap_bucket_iter(
        &self,
        dimension_key: Vec<u8>,
        start_bucket: u64,
        end_bucket_exclusive: u64,
        descending: bool,
    ) -> Result<LedgerBitmapBucketIterator<'_>> {
        self.reader.event_bitmap_bucket_iter(
            dimension_key,
            start_bucket,
            end_bucket_exclusive,
            descending,
        )
    }
}

#[cfg(all(test, not(tidehunter)))]
mod tests {
    use super::*;
    use crate::authority::authority_store_tables::tests::{
        object_version, write_and_prune_all_but_newest,
    };
    use crate::authority::authority_store_types::{
        StoreObject, StoreObjectWrapper, get_store_object,
    };
    use crate::authority::test_authority_builder::TestAuthorityBuilder;
    use sui_consistent_store::{Db, DbOptions};
    use sui_rpc_store::RpcStoreSchema;
    use sui_rpc_store::schema::object_version_by_checkpoint;
    use sui_types::digests::ObjectDigest;
    use typed_store::Map;

    /// The last checkpoint the objects pruner has pruned.
    const LAST_PRUNED: CheckpointSequenceNumber = 10;

    /// `get_object_lt_or_eq_version` on a pruned node, where the pruner range-deleted versions
    /// that compaction has dropped only some of. The reader reads past range deletions and
    /// refuses what may be a pruned version, by the object's version at the last pruned
    /// checkpoint or, with none, by the checkpoint of the transaction that wrote the version
    /// found or, when this node no longer has that checkpoint, by the object's first index row.
    #[tokio::test]
    async fn lt_or_eq_reads_refuse_pruned_versions() {
        let state = TestAuthorityBuilder::new().build().await;
        let store = state.database_for_testing();
        let tables = &store.perpetual_tables;
        let dir = tempfile::tempdir().unwrap();
        let (db, schema) =
            Db::open::<RpcStoreSchema>(dir.path().join("rpc-store"), DbOptions::default()).unwrap();
        let schema = Arc::new(schema);
        let rocks = RocksDbStore::new(
            state.execution_cache_trait_pointers_for_testing().clone(),
            state.committee_store().clone(),
            CheckpointStore::new_for_tests(),
        );
        let with_indexes = RpcStoreReadStore::new(
            state.clone(),
            rocks,
            RpcStoreReader::new(db.clone(), schema.clone()),
        );

        let mut index_rows = db.batch();
        let mut index = |object_id, version: u64, checkpoint| {
            let (key, value) = object_version_by_checkpoint::store(
                object_id,
                checkpoint,
                SequenceNumber::from_u64(version),
            );
            index_rows
                .put(&schema.object_version_by_checkpoint, &key, &value)
                .unwrap();
        };
        let finalize = |object: &Object, checkpoint| {
            store
                .deprecated_insert_finalized_transactions(
                    &[object.previous_transaction],
                    0,
                    checkpoint,
                )
                .unwrap();
        };
        let write = |object: &Object| {
            tables
                .objects
                .insert(
                    &ObjectKey(object.id(), object.version()),
                    &get_store_object(object.clone()),
                )
                .unwrap();
        };
        let write_deletion = |object_id, version: u64| {
            tables
                .objects
                .insert(
                    &ObjectKey(object_id, SequenceNumber::from_u64(version)),
                    &StoreObjectWrapper::from(StoreObject::Deleted),
                )
                .unwrap();
        };

        // Versions 1, 2 and 3, written in checkpoints 3, 5 and 8: 3 is the object's version at
        // the end of the last pruned checkpoint, and the pruner range-deleted 1 and 2 and
        // retracted their index rows.
        let pruned = ObjectID::random();
        let pruned_versions =
            [1, 2, 3].map(|version| object_version(pruned, version, TransactionDigest::random()));
        write_and_prune_all_but_newest(tables, dir.path(), &pruned_versions);
        index(pruned, 3, 8);
        finalize(&pruned_versions[2], 8);

        // Created after the last pruned checkpoint, so it has no version at it.
        let created = object_version(ObjectID::random(), 5, TransactionDigest::random());
        write(&created);
        index(created.id(), 5, LAST_PRUNED + 2);
        finalize(&created, LAST_PRUNED + 2);

        // No version at the last pruned checkpoint, as when the object was deleted or wrapped by
        // then and pruning removed its index history: a version written before it, and one
        // whose transaction's checkpoint this node no longer has and with no index row.
        let written_before = object_version(ObjectID::random(), 4, TransactionDigest::random());
        write(&written_before);
        finalize(&written_before, LAST_PRUNED - 3);
        let checkpoint_unknown = object_version(ObjectID::random(), 6, TransactionDigest::random());
        write(&checkpoint_unknown);

        // Deleted in checkpoint 12, after the last pruned checkpoint: at version 3, after version
        // 2, written in checkpoint 8 and so the object's version at the end of the last pruned
        // checkpoint; and at version 6, after version 5, created in checkpoint 11 and so with no
        // version at the end of it.
        let deleted_after_live = object_version(ObjectID::random(), 2, TransactionDigest::random());
        write(&deleted_after_live);
        index(deleted_after_live.id(), 2, LAST_PRUNED - 2);
        finalize(&deleted_after_live, LAST_PRUNED - 2);
        write_deletion(deleted_after_live.id(), 3);
        index(deleted_after_live.id(), 3, LAST_PRUNED + 2);
        let created_and_deleted =
            object_version(ObjectID::random(), 5, TransactionDigest::random());
        write(&created_and_deleted);
        index(created_and_deleted.id(), 5, LAST_PRUNED + 1);
        finalize(&created_and_deleted, LAST_PRUNED + 1);
        write_deletion(created_and_deleted.id(), 6);
        index(created_and_deleted.id(), 6, LAST_PRUNED + 2);
        // Deleted at version 4 with no index row: its history was pruned, or it was never
        // indexed.
        let deleted_unindexed = ObjectID::random();
        write_deletion(deleted_unindexed, 4);
        // Wrapped at version 3 with its index history pruned, then unwrapped at version 7 in
        // checkpoint 12: the first index row is after the last pruned checkpoint but above the
        // tombstone, so the wrap may have come before it.
        let unwrapped_after = ObjectID::random();
        write_deletion(unwrapped_after, 3);
        let unwrapped = object_version(unwrapped_after, 7, TransactionDigest::random());
        write(&unwrapped);
        index(unwrapped_after, 7, LAST_PRUNED + 2);
        finalize(&unwrapped, LAST_PRUNED + 2);

        // Version 2, written in checkpoint 8, then version 4, written in checkpoint 12: above the
        // object's version at the end of the last pruned checkpoint, it needs no transaction
        // checkpoint.
        let mutated_after = ObjectID::random();
        let before_floor = object_version(mutated_after, 2, TransactionDigest::random());
        write(&before_floor);
        index(mutated_after, 2, LAST_PRUNED - 2);
        write(&object_version(
            mutated_after,
            4,
            TransactionDigest::random(),
        ));
        index(mutated_after, 4, LAST_PRUNED + 2);

        // A node that keeps every object version but prunes checkpoints (restored from a
        // snapshot, so its objects lower bound is above 0) no longer has the checkpoints of
        // these versions' transactions. Version 6, created in checkpoint 12: its first index
        // row is after the last pruned checkpoint and records it. Version 3, written by the last
        // pruned checkpoint and replaced after a pruned deletion or wrap by version 7 in
        // checkpoint 12: the first index row records a later version.
        let checkpoint_pruned = object_version(ObjectID::random(), 6, TransactionDigest::random());
        write(&checkpoint_pruned);
        index(checkpoint_pruned.id(), 6, LAST_PRUNED + 2);
        let rewritten_after = ObjectID::random();
        write(&object_version(
            rewritten_after,
            3,
            TransactionDigest::random(),
        ));
        write(&object_version(
            rewritten_after,
            7,
            TransactionDigest::random(),
        ));
        index(rewritten_after, 7, LAST_PRUNED + 2);
        // Deleted at version 9 in checkpoint 12, after the last pruned checkpoint, and re-created
        // at version 11 in checkpoint 13, after version 5 written in checkpoint 8. The deleting
        // transaction's checkpoint is unknown, as when it is above the indexed tip or its effects
        // are pruned.
        let removed_then_recreated = ObjectID::random();
        let before_removal = object_version(removed_then_recreated, 5, TransactionDigest::random());
        write(&before_removal);
        index(removed_then_recreated, 5, LAST_PRUNED - 2);
        finalize(&before_removal, LAST_PRUNED - 2);
        write_deletion(removed_then_recreated, 9);
        index(removed_then_recreated, 9, LAST_PRUNED + 2);
        let recreated = object_version(removed_then_recreated, 11, TransactionDigest::random());
        write(&recreated);
        index(removed_then_recreated, 11, LAST_PRUNED + 3);
        finalize(&recreated, LAST_PRUNED + 3);
        // Deleted at version 9 in checkpoint 7, re-created at version 10 in checkpoint 8 and
        // changed to version 11 in checkpoint 9, all at or before the last pruned checkpoint. The
        // pruner deleted version 10 and compaction dropped it, while the deletion's tombstone
        // stayed (as with `killswitch_tombstone_pruning`), and only version 11's index row is
        // left.
        let recreated_and_pruned = ObjectID::random();
        write_deletion(recreated_and_pruned, 9);
        let replaced = object_version(recreated_and_pruned, 11, TransactionDigest::random());
        write(&replaced);
        index(recreated_and_pruned, 11, LAST_PRUNED - 1);
        finalize(&replaced, LAST_PRUNED - 1);

        index_rows.commit().unwrap();

        let found = |reader: &dyn RpcStateReader, object_id, bound: u64| {
            reader
                .get_object_lt_or_eq_version(&object_id, SequenceNumber::from_u64(bound))
                .unwrap()
                .map(|object| object.version().value())
        };

        // With nothing pruned there is nothing to check, and the scan's answer stands.
        assert_eq!(found(&with_indexes, pruned, 2), Some(1));

        tables
            .set_highest_pruned_checkpoint_without_wb(LAST_PRUNED)
            .unwrap();
        // 1 is below the object's version at the last pruned checkpoint.
        assert_eq!(found(&with_indexes, pruned, 1), None);
        assert_eq!(found(&with_indexes, pruned, 2), None);
        assert_eq!(found(&with_indexes, pruned, 3), Some(3));
        assert_eq!(found(&with_indexes, created.id(), 5), Some(5));
        assert_eq!(found(&with_indexes, written_before.id(), 4), None);
        assert_eq!(found(&with_indexes, checkpoint_unknown.id(), 6), None);
        assert_eq!(found(&with_indexes, checkpoint_pruned.id(), 6), Some(6));
        assert_eq!(found(&with_indexes, rewritten_after, 5), None);
        assert_eq!(found(&with_indexes, rewritten_after, 7), Some(7));
        assert_eq!(found(&with_indexes, mutated_after, 2), Some(2));
        assert_eq!(found(&with_indexes, mutated_after, 5), Some(4));

        // Tombstones, which `get_object_lt_or_eq_version` reads as no object. One at or above the
        // object's version at the last pruned checkpoint is kept. With no version there, one is
        // kept if the object's first index row is after the last pruned checkpoint and records a
        // version at or below it, and refused if the object has no index row, or the first one
        // records a later version.
        let summary = |entry: Option<ObjectOrTombstone>| {
            entry.map(|entry| match entry {
                ObjectOrTombstone::Object(object) => (object.version().value(), None),
                ObjectOrTombstone::Tombstone((_, version, digest)) => {
                    (version.value(), Some(digest))
                }
            })
        };
        let entry = |object_id, bound: u64| {
            summary(
                with_indexes
                    .find_object_or_tombstone_lt_or_eq_version(
                        object_id,
                        SequenceNumber::from_u64(bound),
                    )
                    .unwrap(),
            )
        };
        assert_eq!(entry(deleted_after_live.id(), 2), Some((2, None)));
        assert_eq!(
            entry(deleted_after_live.id(), 3),
            Some((3, Some(ObjectDigest::OBJECT_DIGEST_DELETED))),
        );
        assert_eq!(entry(created_and_deleted.id(), 5), Some((5, None)));
        assert_eq!(
            entry(created_and_deleted.id(), 6),
            Some((6, Some(ObjectDigest::OBJECT_DIGEST_DELETED))),
        );
        assert_eq!(entry(deleted_unindexed, 4), None);
        assert_eq!(entry(unwrapped_after, 5), None);
        assert_eq!(entry(unwrapped_after, 7), Some((7, None)));

        // A `max_version` of 10 selects the deletion at 9. After the last pruned checkpoint it is
        // kept, though its transaction's checkpoint is unknown: no version after it was pruned.
        // At or before it, with the version that replaced it pruned, it is refused: that version,
        // not the deletion, is the answer.
        assert_eq!(
            entry(removed_then_recreated, 10),
            Some((9, Some(ObjectDigest::OBJECT_DIGEST_DELETED))),
        );
        assert_eq!(entry(removed_then_recreated, 11), Some((11, None)));
        assert_eq!(entry(recreated_and_pruned, 10), None);
        assert_eq!(entry(recreated_and_pruned, 11), Some((11, None)));

        // Exactly a version: the entry at it, checked the same way, and nothing for a version the
        // object has no entry at, though an older entry is at or above its version at the last
        // pruned checkpoint. A deletion that may have a pruned version after it is refused.
        let by_key = |object_id, version: u64| {
            summary(
                with_indexes
                    .get_object_or_tombstone_by_key(&object_id, SequenceNumber::from_u64(version))
                    .unwrap(),
            )
        };
        assert_eq!(by_key(pruned, 1), None);
        assert_eq!(by_key(pruned, 3), Some((3, None)));
        assert_eq!(by_key(mutated_after, 2), Some((2, None)));
        assert_eq!(by_key(mutated_after, 3), None);
        assert_eq!(
            by_key(deleted_after_live.id(), 3),
            Some((3, Some(ObjectDigest::OBJECT_DIGEST_DELETED))),
        );
        assert_eq!(
            by_key(removed_then_recreated, 9),
            Some((9, Some(ObjectDigest::OBJECT_DIGEST_DELETED))),
        );
        assert_eq!(by_key(recreated_and_pruned, 9), None);
        assert_eq!(by_key(recreated_and_pruned, 10), None);
    }

    /// An uncommitted entry is newer than every version below the lower bound, so it is kept
    /// without reading the lower bound, the indexes, or its transaction's checkpoint.
    #[test]
    fn uncommitted_entries_skip_the_lower_bound_check() {
        let found = FoundObjectOrTombstone {
            entry: ObjectOrTombstone::Object(object_version(
                ObjectID::random(),
                1,
                TransactionDigest::random(),
            )),
            committed: false,
        };
        assert!(
            !may_be_below_objects_lower_bound(
                &found,
                || panic!("read the lower bound"),
                |_| panic!("read the indexes"),
                |_| panic!("read the transaction's checkpoint"),
                || panic!("read the first index row"),
            )
            .unwrap()
        );
    }

    /// A committed tombstone with no version at the last pruned checkpoint is kept only if the
    /// object's first index row is after that checkpoint and records a version at or below the
    /// tombstone's.
    #[test]
    fn tombstones_without_a_version_at_the_last_pruned_checkpoint() {
        let found = FoundObjectOrTombstone {
            entry: ObjectOrTombstone::Tombstone((
                ObjectID::random(),
                SequenceNumber::from_u64(5),
                ObjectDigest::OBJECT_DIGEST_DELETED,
            )),
            committed: true,
        };
        let may_be_below = |first_indexed_change: Option<(u64, u64)>| {
            may_be_below_objects_lower_bound(
                &found,
                || Ok(LAST_PRUNED + 1),
                |_| Ok(None),
                |_| panic!("read the transaction's checkpoint"),
                || {
                    Ok(first_indexed_change.map(|(checkpoint, version)| {
                        (checkpoint, SequenceNumber::from_u64(version))
                    }))
                },
            )
            .unwrap()
        };
        assert!(!may_be_below(Some((LAST_PRUNED + 1, 5))));
        assert!(!may_be_below(Some((LAST_PRUNED + 1, 4))));
        assert!(may_be_below(Some((LAST_PRUNED + 1, 6))));
        assert!(may_be_below(Some((LAST_PRUNED, 4))));
        assert!(may_be_below(None));
    }
}
