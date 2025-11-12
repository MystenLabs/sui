// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::authority::AuthorityState;
use crate::checkpoints::CheckpointStore;
use crate::epoch::committee_store::CommitteeStore;
use crate::execution_cache::ExecutionCacheTraitPointers;
use crate::rpc_index::CoinIndexInfo;
use crate::rpc_index::OwnerIndexInfo;
use crate::rpc_index::OwnerIndexKey;
use crate::rpc_index::RpcIndexStore;
use move_core_types::language_storage::StructTag;
use parking_lot::Mutex;
use std::sync::Arc;
use sui_types::base_types::ObjectID;
use sui_types::base_types::SuiAddress;
use sui_types::base_types::TransactionDigest;
use sui_types::committee::Committee;
use sui_types::committee::EpochId;
use sui_types::effects::{TransactionEffects, TransactionEvents};
use sui_types::messages_checkpoint::CheckpointContentsDigest;
use sui_types::messages_checkpoint::CheckpointDigest;
use sui_types::messages_checkpoint::CheckpointSequenceNumber;
use sui_types::messages_checkpoint::EndOfEpochData;
use sui_types::messages_checkpoint::FullCheckpointContents;
use sui_types::messages_checkpoint::VerifiedCheckpoint;
use sui_types::messages_checkpoint::VerifiedCheckpointContents;
use sui_types::object::Object;
use sui_types::storage::BalanceInfo;
use sui_types::storage::BalanceIterator;
use sui_types::storage::CoinInfo;
use sui_types::storage::DynamicFieldKey;
use sui_types::storage::ObjectStore;
use sui_types::storage::OwnedObjectInfo;
use sui_types::storage::RpcIndexes;
use sui_types::storage::RpcStateReader;
use sui_types::storage::TransactionInfo;
use sui_types::storage::WriteStore;
use sui_types::storage::error::Error as StorageError;
use sui_types::storage::error::Result;
use sui_types::storage::{ObjectKey, ReadStore};
use sui_types::transaction::VerifiedTransaction;
use tap::Pipe;
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
    ) -> Option<FullCheckpointContents> {
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
                Some(FullCheckpointContents::from_contents_and_execution_data(
                    contents,
                    transactions.into_iter(),
                ))
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

    fn get_transaction_effects(&self, digest: &TransactionDigest) -> Option<TransactionEffects> {
        self.cache_traits
            .transaction_cache_reader
            .get_executed_effects(digest)
    }

    fn get_events(&self, digest: &TransactionDigest) -> Option<TransactionEvents> {
        self.cache_traits
            .transaction_cache_reader
            .get_events(digest)
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

    fn index(&self) -> sui_types::storage::error::Result<&RpcIndexStore> {
        self.state
            .rpc_index
            .as_deref()
            .ok_or_else(|| sui_types::storage::error::Error::custom("rest index store is disabled"))
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

    fn get_transaction_effects(&self, digest: &TransactionDigest) -> Option<TransactionEffects> {
        self.rocks.get_transaction_effects(digest)
    }

    fn get_events(&self, digest: &TransactionDigest) -> Option<TransactionEvents> {
        self.rocks.get_events(digest)
    }

    fn get_full_checkpoint_contents(
        &self,
        sequence_number: Option<CheckpointSequenceNumber>,
        digest: &CheckpointContentsDigest,
    ) -> Option<FullCheckpointContents> {
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
        Some(self)
    }

    fn get_struct_layout(
        &self,
        struct_tag: &move_core_types::language_storage::StructTag,
    ) -> Result<Option<move_core_types::annotated_value::MoveTypeLayout>> {
        self.state
            .load_epoch_store_one_call_per_task()
            .executor()
            // TODO(cache) - must read through cache
            .type_layout_resolver(Box::new(self.state.get_backing_package_store().as_ref()))
            .get_annotated_layout(struct_tag)
            .map(|layout| layout.into_layout())
            .map(Some)
            .map_err(StorageError::custom)
    }
}

struct BatchedEventIterator<'a, I>
where
    I: Iterator<Item = Result<crate::rpc_index::EventIndexKey, TypedStoreError>>,
{
    key_iter: I,
    rocks: &'a RocksDbStore,
    current_checkpoint: Option<u64>,
    current_checkpoint_contents: Option<sui_types::messages_checkpoint::CheckpointContents>,
    cached_tx_events: Option<TransactionEvents>,
    cached_tx_digest: Option<TransactionDigest>,
}

impl<I> Iterator for BatchedEventIterator<'_, I>
where
    I: Iterator<Item = Result<crate::rpc_index::EventIndexKey, TypedStoreError>>,
{
    type Item = Result<(u64, u32, u32, sui_types::event::Event), TypedStoreError>;

    fn next(&mut self) -> Option<Self::Item> {
        let key = match self.key_iter.next()? {
            Ok(k) => k,
            Err(e) => return Some(Err(e)),
        };

        if self.current_checkpoint != Some(key.checkpoint_seq) {
            self.current_checkpoint = Some(key.checkpoint_seq);
            self.current_checkpoint_contents = self
                .rocks
                .get_checkpoint_contents_by_sequence_number(key.checkpoint_seq);
            self.cached_tx_events = None;
            self.cached_tx_digest = None;
        }

        let checkpoint_contents = self.current_checkpoint_contents.as_ref()?;

        let exec_digest = checkpoint_contents
            .iter()
            .nth(key.transaction_idx as usize)?;
        let tx_digest = exec_digest.transaction;

        if self.cached_tx_digest != Some(tx_digest) {
            self.cached_tx_digest = Some(tx_digest);
            self.cached_tx_events = self.rocks.get_events(&tx_digest);
        }

        let tx_events = self.cached_tx_events.as_ref()?;
        let event = tx_events.data.get(key.event_index as usize)?.clone();

        Some(Ok((
            key.checkpoint_seq,
            key.transaction_idx,
            key.event_index,
            event,
        )))
    }
}

impl RpcIndexes for RestReadStore {
    fn get_epoch_info(&self, epoch: EpochId) -> Result<Option<sui_types::storage::EpochInfo>> {
        self.index()?
            .get_epoch_info(epoch)
            .map_err(StorageError::custom)
    }

    fn get_transaction_info(
        &self,
        digest: &TransactionDigest,
    ) -> sui_types::storage::error::Result<Option<TransactionInfo>> {
        self.index()?
            .get_transaction_info(digest)
            .map_err(StorageError::custom)
    }

    fn owned_objects_iter(
        &self,
        owner: SuiAddress,
        object_type: Option<StructTag>,
        cursor: Option<OwnedObjectInfo>,
    ) -> Result<Box<dyn Iterator<Item = Result<OwnedObjectInfo, TypedStoreError>> + '_>> {
        let cursor = cursor.map(|cursor| OwnerIndexKey {
            owner: cursor.owner,
            object_type: cursor.object_type,
            inverted_balance: cursor.balance.map(std::ops::Not::not),
            object_id: cursor.object_id,
        });

        let iter = self
            .index()?
            .owner_iter(owner, object_type, cursor)?
            .map(|result| {
                result.map(
                    |(
                        OwnerIndexKey {
                            owner,
                            object_id,
                            object_type,
                            inverted_balance,
                        },
                        OwnerIndexInfo { version },
                    )| {
                        OwnedObjectInfo {
                            owner,
                            object_type,
                            balance: inverted_balance.map(std::ops::Not::not),
                            object_id,
                            version,
                        }
                    },
                )
            });

        Ok(Box::new(iter) as _)
    }

    fn dynamic_field_iter(
        &self,
        parent: ObjectID,
        cursor: Option<ObjectID>,
    ) -> sui_types::storage::error::Result<
        Box<dyn Iterator<Item = Result<DynamicFieldKey, TypedStoreError>> + '_>,
    > {
        let iter = self.index()?.dynamic_field_iter(parent, cursor)?;
        Ok(Box::new(iter) as _)
    }

    fn get_coin_info(
        &self,
        coin_type: &StructTag,
    ) -> sui_types::storage::error::Result<Option<CoinInfo>> {
        self.index()?
            .get_coin_info(coin_type)?
            .map(
                |CoinIndexInfo {
                     coin_metadata_object_id,
                     treasury_object_id,
                     regulated_coin_metadata_object_id,
                 }| CoinInfo {
                    coin_metadata_object_id,
                    treasury_object_id,
                    regulated_coin_metadata_object_id,
                },
            )
            .pipe(Ok)
    }

    fn get_balance(
        &self,
        owner: &SuiAddress,
        coin_type: &StructTag,
    ) -> sui_types::storage::error::Result<Option<BalanceInfo>> {
        self.index()?
            .get_balance(owner, coin_type)?
            .map(|info| info.into())
            .pipe(Ok)
    }

    fn balance_iter(
        &self,
        owner: &SuiAddress,
        cursor: Option<(SuiAddress, StructTag)>,
    ) -> sui_types::storage::error::Result<BalanceIterator<'_>> {
        let cursor_key =
            cursor.map(|(owner, coin_type)| crate::rpc_index::BalanceKey { owner, coin_type });

        Ok(Box::new(
            self.index()?
                .balance_iter(*owner, cursor_key)?
                .map(|result| {
                    result
                        .map(|(key, info)| (key.coin_type, info.into()))
                        .map_err(Into::into)
                }),
        ))
    }

    fn package_versions_iter(
        &self,
        original_id: ObjectID,
        cursor: Option<u64>,
    ) -> sui_types::storage::error::Result<
        Box<dyn Iterator<Item = Result<(u64, ObjectID), TypedStoreError>> + '_>,
    > {
        let iter = self.index()?.package_versions_iter(original_id, cursor)?;
        Ok(
            Box::new(iter.map(|result| result.map(|(key, info)| (key.version, info.storage_id))))
                as _,
        )
    }

    fn get_highest_indexed_checkpoint_seq_number(
        &self,
    ) -> sui_types::storage::error::Result<Option<CheckpointSequenceNumber>> {
        self.index()?
            .get_highest_indexed_checkpoint_seq_number()
            .map_err(Into::into)
    }

    fn authenticated_event_iter(
        &self,
        stream_id: SuiAddress,
        start_checkpoint: u64,
        start_transaction_idx: Option<u32>,
        start_event_idx: Option<u32>,
        end_checkpoint: u64,
        limit: u32,
    ) -> sui_types::storage::error::Result<
        Box<
            dyn Iterator<Item = Result<(u64, u32, u32, sui_types::event::Event), TypedStoreError>>
                + '_,
        >,
    > {
        let index = self.index()?;
        let key_iter = index.event_iter(
            stream_id,
            start_checkpoint,
            start_transaction_idx.unwrap_or(0),
            start_event_idx.unwrap_or(0),
            end_checkpoint,
            limit,
        )?;

        let rocks = &self.rocks;
        let iter = BatchedEventIterator {
            key_iter,
            rocks,
            current_checkpoint: None,
            current_checkpoint_contents: None,
            cached_tx_events: None,
            cached_tx_digest: None,
        };

        Ok(Box::new(iter))
    }
}
