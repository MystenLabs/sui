// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::authority::AuthorityState;
use crate::checkpoints::CheckpointStore;
use crate::epoch::committee_store::CommitteeStore;
use crate::execution_cache::ExecutionCacheTraitPointers;
use move_core_types::language_storage::StructTag;
use parking_lot::Mutex;
use std::sync::Arc;
use sui_rpc_store::RpcStoreReader;
use sui_types::base_types::ObjectID;
use sui_types::base_types::SequenceNumber;
use sui_types::base_types::SuiAddress;
use sui_types::base_types::TransactionDigest;
use sui_types::committee::Committee;
use sui_types::committee::EpochId;
use sui_types::effects::{TransactionEffects, TransactionEvents};
use sui_types::error::{SuiErrorKind, SuiResult};
use sui_types::full_checkpoint_content::ObjectSet;
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
use sui_types::storage::ChildObjectResolver;
use sui_types::storage::CoinInfo;
use sui_types::storage::DynamicFieldKey;
use sui_types::storage::LedgerBitmapBucketIterator;
use sui_types::storage::LedgerTxSeqDigest;
use sui_types::storage::LedgerTxSeqDigestIterator;
use sui_types::storage::ObjectStore;
use sui_types::storage::OwnedObjectInfo;
use sui_types::storage::RpcIndexes;
use sui_types::storage::RpcStateReader;
use sui_types::storage::WriteStore;
use sui_types::storage::error::Error as StorageError;
use sui_types::storage::error::Result;
use sui_types::storage::{ObjectKey, OverlayBackingPackageStore, ReadStore};
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

    fn get_lowest_available_checkpoint(&self) -> Result<CheckpointSequenceNumber, StorageError> {
        if let Some(highest_pruned_cp) = self
            .checkpoint_store
            .get_highest_pruned_checkpoint_seq_number()
            .map_err(Into::<StorageError>::into)?
        {
            Ok(highest_pruned_cp + 1)
        } else {
            Ok(0)
        }
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

    fn get_transaction_checkpoint(
        &self,
        digest: &TransactionDigest,
    ) -> Option<CheckpointSequenceNumber> {
        self.rocks.get_transaction_checkpoint(digest)
    }
}

impl ChildObjectResolver for RestReadStore {
    fn read_child_object(
        &self,
        parent: &ObjectID,
        child: &ObjectID,
        child_version_upper_bound: SequenceNumber,
    ) -> SuiResult<Option<Object>> {
        Ok(self.get_object(child).and_then(|o| {
            if o.version() <= child_version_upper_bound
                && o.owner == Owner::ObjectOwner((*parent).into())
            {
                Some(o)
            } else {
                None
            }
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

    fn get_chain_identifier(&self) -> Result<sui_types::digests::ChainIdentifier> {
        Ok(self.state.get_chain_identifier())
    }

    fn indexes(&self) -> Option<&dyn RpcIndexes> {
        // The legacy `rpc-index` backend has been removed; a node serving
        // reads through `RestReadStore` exposes no index surface. Index
        // reads are served by the embedded rpc-store via `RpcStoreReadStore`.
        None
    }

    fn get_struct_layout_with_overlay(
        &self,
        struct_tag: &move_core_types::language_storage::StructTag,
        overlay: &ObjectSet,
    ) -> Result<Option<move_core_types::annotated_value::MoveTypeLayout>> {
        let backing_store = self.state.get_backing_package_store();
        let overlay_store = OverlayBackingPackageStore::new(overlay, backing_store.as_ref());
        let epoch_store = self.state.load_epoch_store_one_call_per_task();
        epoch_store
            .executor()
            // TODO(cache) - must read through cache
            .type_layout_resolver(epoch_store.protocol_config(), Box::new(overlay_store))
            .get_annotated_layout(struct_tag)
            .map(|layout| layout.into_layout())
            .map(Some)
            .map_err(StorageError::custom)
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
///   [`RpcStoreReader`].
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
}

impl ObjectStore for RpcStoreReadStore {
    fn get_object(&self, object_id: &ObjectID) -> Option<Object> {
        self.rocks.get_object(object_id)
    }

    fn get_object_by_key(&self, object_id: &ObjectID, version: SequenceNumber) -> Option<Object> {
        self.rocks.get_object_by_key(object_id, version)
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
            _ => Ok(latest),
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

    fn get_transaction_checkpoint(
        &self,
        digest: &TransactionDigest,
    ) -> Option<CheckpointSequenceNumber> {
        self.rocks.get_transaction_checkpoint(digest)
    }
}

impl ChildObjectResolver for RpcStoreReadStore {
    fn read_child_object(
        &self,
        parent: &ObjectID,
        child: &ObjectID,
        child_version_upper_bound: SequenceNumber,
    ) -> SuiResult<Option<Object>> {
        Ok(self.get_object(child).and_then(|o| {
            if o.version() <= child_version_upper_bound
                && o.owner == Owner::ObjectOwner((*parent).into())
            {
                Some(o)
            } else {
                None
            }
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

    fn get_chain_identifier(&self) -> Result<sui_types::digests::ChainIdentifier> {
        Ok(self.state.get_chain_identifier())
    }

    fn indexes(&self) -> Option<&dyn RpcIndexes> {
        Some(self)
    }

    fn get_struct_layout_with_overlay(
        &self,
        struct_tag: &move_core_types::language_storage::StructTag,
        overlay: &ObjectSet,
    ) -> Result<Option<move_core_types::annotated_value::MoveTypeLayout>> {
        // Resolve through the authority's live executor and backing
        // package store, matching `RestReadStore`: the perpetual store
        // backs the package reads and the loaded epoch store carries
        // the current protocol config.
        let backing_store = self.state.get_backing_package_store();
        let overlay_store = OverlayBackingPackageStore::new(overlay, backing_store.as_ref());
        let epoch_store = self.state.load_epoch_store_one_call_per_task();
        epoch_store
            .executor()
            .type_layout_resolver(epoch_store.protocol_config(), Box::new(overlay_store))
            .get_annotated_layout(struct_tag)
            .map(|layout| layout.into_layout())
            .map(Some)
            .map_err(StorageError::custom)
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
        cursor: Option<ObjectID>,
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
        cursor: Option<u64>,
    ) -> Result<Box<dyn Iterator<Item = Result<(u64, ObjectID), TypedStoreError>> + '_>> {
        self.reader.package_versions_iter(original_id, cursor)
    }

    fn get_highest_indexed_checkpoint_seq_number(
        &self,
    ) -> Result<Option<CheckpointSequenceNumber>> {
        self.reader.get_highest_indexed_checkpoint_seq_number()
    }

    fn ledger_tx_seq_digest(&self, tx_seq: u64) -> Result<Option<LedgerTxSeqDigest>> {
        self.reader.ledger_tx_seq_digest(tx_seq)
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
