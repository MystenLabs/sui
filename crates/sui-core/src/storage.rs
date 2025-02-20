// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use move_core_types::language_storage::StructTag;
use parking_lot::Mutex;
use std::sync::Arc;
use sui_types::base_types::ObjectID;
use sui_types::base_types::SuiAddress;
use sui_types::base_types::TransactionDigest;
use sui_types::committee::Committee;
use sui_types::committee::EpochId;
use sui_types::digests::TransactionEventsDigest;
use sui_types::effects::{TransactionEffects, TransactionEvents};
use sui_types::messages_checkpoint::CheckpointContentsDigest;
use sui_types::messages_checkpoint::CheckpointDigest;
use sui_types::messages_checkpoint::CheckpointSequenceNumber;
use sui_types::messages_checkpoint::EndOfEpochData;
use sui_types::messages_checkpoint::FullCheckpointContents;
use sui_types::messages_checkpoint::VerifiedCheckpoint;
use sui_types::messages_checkpoint::VerifiedCheckpointContents;
use sui_types::object::Object;
use sui_types::storage::error::Error as StorageError;
use sui_types::storage::error::Result;
use sui_types::storage::AccountOwnedObjectInfo;
use sui_types::storage::CoinInfo;
use sui_types::storage::DynamicFieldIndexInfo;
use sui_types::storage::DynamicFieldKey;
use sui_types::storage::ObjectStore;
use sui_types::storage::RpcIndexes;
use sui_types::storage::RpcStateReader;
use sui_types::storage::WriteStore;
use sui_types::storage::{ObjectKey, ReadStore};
use sui_types::transaction::VerifiedTransaction;
use tap::Pipe;

use crate::authority::AuthorityState;
use crate::checkpoints::CheckpointStore;
use crate::epoch::committee_store::CommitteeStore;
use crate::execution_cache::ExecutionCacheTraitPointers;
use crate::rpc_index::CoinIndexInfo;
use crate::rpc_index::OwnerIndexInfo;
use crate::rpc_index::OwnerIndexKey;
use crate::rpc_index::RpcIndexStore;

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
        let highest_pruned_cp = self
            .checkpoint_store
            .get_highest_pruned_checkpoint_seq_number()
            .map_err(Into::<StorageError>::into)?;

        if highest_pruned_cp == 0 {
            Ok(0)
        } else {
            Ok(highest_pruned_cp + 1)
        }
    }

    fn get_full_checkpoint_contents_by_sequence_number(
        &self,
        sequence_number: CheckpointSequenceNumber,
    ) -> Option<FullCheckpointContents> {
        self.checkpoint_store
            .get_full_checkpoint_contents_by_sequence_number(sequence_number)
            .expect("db error")
    }

    fn get_full_checkpoint_contents(
        &self,
        digest: &CheckpointContentsDigest,
    ) -> Option<FullCheckpointContents> {
        // First look to see if we saved the complete contents already.
        if let Some(seq_num) = self
            .checkpoint_store
            .get_sequence_number_by_contents_digest(digest)
            .expect("db error")
        {
            let contents = self
                .checkpoint_store
                .get_full_checkpoint_contents_by_sequence_number(seq_num)
                .expect("db error");
            if contents.is_some() {
                return contents;
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

    fn get_events(&self, digest: &TransactionEventsDigest) -> Option<TransactionEvents> {
        self.cache_traits
            .transaction_cache_reader
            .get_events(digest)
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

    fn get_events(&self, digest: &TransactionEventsDigest) -> Option<TransactionEvents> {
        self.rocks.get_events(digest)
    }

    fn get_full_checkpoint_contents_by_sequence_number(
        &self,
        sequence_number: CheckpointSequenceNumber,
    ) -> Option<FullCheckpointContents> {
        self.rocks
            .get_full_checkpoint_contents_by_sequence_number(sequence_number)
    }

    fn get_full_checkpoint_contents(
        &self,
        digest: &CheckpointContentsDigest,
    ) -> Option<FullCheckpointContents> {
        self.rocks.get_full_checkpoint_contents(digest)
    }
}

impl RpcStateReader for RestReadStore {
    fn get_lowest_available_checkpoint_objects(
        &self,
    ) -> sui_types::storage::error::Result<CheckpointSequenceNumber> {
        let highest_pruned_cp = self
            .state
            .get_object_cache_reader()
            .get_highest_pruned_checkpoint();

        if highest_pruned_cp == 0 {
            Ok(0)
        } else {
            Ok(highest_pruned_cp + 1)
        }
    }

    fn get_chain_identifier(&self) -> Result<sui_types::digests::ChainIdentifier> {
        Ok(self.state.get_chain_identifier())
    }

    fn indexes(&self) -> Option<&dyn RpcIndexes> {
        self.index().ok().map(|index| index as _)
    }
}

impl RpcIndexes for RpcIndexStore {
    fn get_transaction_checkpoint(
        &self,
        digest: &TransactionDigest,
    ) -> sui_types::storage::error::Result<Option<CheckpointSequenceNumber>> {
        self.get_transaction_info(digest)
            .map(|maybe_info| maybe_info.map(|info| info.checkpoint))
            .map_err(StorageError::custom)
    }

    fn account_owned_objects_info_iter(
        &self,
        owner: SuiAddress,
        cursor: Option<ObjectID>,
    ) -> Result<Box<dyn Iterator<Item = AccountOwnedObjectInfo> + '_>> {
        let iter = self.owner_iter(owner, cursor)?.map(
            |(OwnerIndexKey { owner, object_id }, OwnerIndexInfo { version, type_ })| {
                AccountOwnedObjectInfo {
                    owner,
                    object_id,
                    version,
                    type_,
                }
            },
        );

        Ok(Box::new(iter) as _)
    }

    fn dynamic_field_iter(
        &self,
        parent: ObjectID,
        cursor: Option<ObjectID>,
    ) -> sui_types::storage::error::Result<
        Box<dyn Iterator<Item = (DynamicFieldKey, DynamicFieldIndexInfo)> + '_>,
    > {
        let iter = self.dynamic_field_iter(parent, cursor)?;

        Ok(Box::new(iter) as _)
    }

    fn get_coin_info(
        &self,
        coin_type: &StructTag,
    ) -> sui_types::storage::error::Result<Option<CoinInfo>> {
        self.get_coin_info(coin_type)?
            .map(
                |CoinIndexInfo {
                     coin_metadata_object_id,
                     treasury_object_id,
                 }| CoinInfo {
                    coin_metadata_object_id,
                    treasury_object_id,
                },
            )
            .pipe(Ok)
    }
}
