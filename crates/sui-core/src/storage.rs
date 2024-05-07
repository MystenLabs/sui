// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use parking_lot::Mutex;
use std::sync::Arc;
use sui_types::storage::ObjectStore;

use sui_types::base_types::TransactionDigest;
use sui_types::committee::Committee;
use sui_types::committee::EpochId;
use sui_types::digests::TransactionEventsDigest;
use sui_types::effects::{TransactionEffects, TransactionEvents};
use sui_types::error::SuiError;
use sui_types::messages_checkpoint::CheckpointContentsDigest;
use sui_types::messages_checkpoint::CheckpointDigest;
use sui_types::messages_checkpoint::CheckpointSequenceNumber;
use sui_types::messages_checkpoint::EndOfEpochData;
use sui_types::messages_checkpoint::FullCheckpointContents;
use sui_types::messages_checkpoint::VerifiedCheckpoint;
use sui_types::messages_checkpoint::VerifiedCheckpointContents;
use sui_types::object::Object;
use sui_types::storage::error::Error as StorageError;
use sui_types::storage::WriteStore;
use sui_types::storage::{ObjectKey, ReadStore};
use sui_types::transaction::VerifiedTransaction;

use crate::checkpoints::CheckpointStore;
use crate::epoch::committee_store::CommitteeStore;
use crate::execution_cache::ExecutionCacheRead;
use crate::execution_cache::StateSyncAPI;

#[derive(Clone)]
pub struct RocksDbStore {
    // Note: All three of these Arcs point to the same object,
    // but we need to store them separately to satisfy the trait bounds.
    execution_cache: Arc<dyn ExecutionCacheRead>,
    object_store: Arc<dyn ObjectStore + Send + Sync>,
    state_sync_store: Arc<dyn StateSyncAPI>,

    committee_store: Arc<CommitteeStore>,
    checkpoint_store: Arc<CheckpointStore>,
    // in memory checkpoint watermark sequence numbers
    highest_verified_checkpoint: Arc<Mutex<Option<u64>>>,
    highest_synced_checkpoint: Arc<Mutex<Option<u64>>>,
}

impl RocksDbStore {
    pub fn new(
        execution_cache: Arc<impl ExecutionCacheRead + ObjectStore + StateSyncAPI + 'static>,
        committee_store: Arc<CommitteeStore>,
        checkpoint_store: Arc<CheckpointStore>,
    ) -> Self {
        Self {
            execution_cache: execution_cache.clone(),
            object_store: execution_cache.clone(),
            state_sync_store: execution_cache.clone(),
            committee_store,
            checkpoint_store,
            highest_verified_checkpoint: Arc::new(Mutex::new(None)),
            highest_synced_checkpoint: Arc::new(Mutex::new(None)),
        }
    }

    pub fn get_objects(&self, object_keys: &[ObjectKey]) -> Result<Vec<Option<Object>>, SuiError> {
        self.execution_cache.multi_get_objects_by_key(object_keys)
    }

    pub fn get_last_executed_checkpoint(&self) -> Result<Option<VerifiedCheckpoint>, SuiError> {
        Ok(self.checkpoint_store.get_highest_executed_checkpoint()?)
    }
}

impl ReadStore for RocksDbStore {
    fn get_checkpoint_by_digest(
        &self,
        digest: &CheckpointDigest,
    ) -> Result<Option<VerifiedCheckpoint>, StorageError> {
        self.checkpoint_store
            .get_checkpoint_by_digest(digest)
            .map_err(Into::into)
    }

    fn get_checkpoint_by_sequence_number(
        &self,
        sequence_number: CheckpointSequenceNumber,
    ) -> Result<Option<VerifiedCheckpoint>, StorageError> {
        self.checkpoint_store
            .get_checkpoint_by_sequence_number(sequence_number)
            .map_err(Into::into)
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
        self.checkpoint_store
            .get_highest_pruned_checkpoint_seq_number()
            .map(|seq| seq + 1)
            .map_err(Into::into)
    }

    fn get_full_checkpoint_contents_by_sequence_number(
        &self,
        sequence_number: CheckpointSequenceNumber,
    ) -> Result<Option<FullCheckpointContents>, StorageError> {
        self.checkpoint_store
            .get_full_checkpoint_contents_by_sequence_number(sequence_number)
            .map_err(Into::into)
    }

    fn get_full_checkpoint_contents(
        &self,
        digest: &CheckpointContentsDigest,
    ) -> Result<Option<FullCheckpointContents>, StorageError> {
        // First look to see if we saved the complete contents already.
        if let Some(seq_num) = self
            .checkpoint_store
            .get_sequence_number_by_contents_digest(digest)
            .map_err(sui_types::storage::error::Error::custom)?
        {
            let contents = self
                .checkpoint_store
                .get_full_checkpoint_contents_by_sequence_number(seq_num)
                .map_err(sui_types::storage::error::Error::custom)?;
            if contents.is_some() {
                return Ok(contents);
            }
        }

        // Otherwise gather it from the individual components.
        // Note we can't insert the constructed contents into `full_checkpoint_content`,
        // because it needs to be inserted along with `checkpoint_sequence_by_contents_digest`
        // and `checkpoint_content`. However at this point it's likely we don't know the
        // corresponding sequence number yet.
        self.checkpoint_store
            .get_checkpoint_contents(digest)
            .map_err(sui_types::storage::error::Error::custom)?
            .map(|contents| {
                let mut transactions = Vec::with_capacity(contents.size());
                for tx in contents.iter() {
                    if let (Some(t), Some(e)) = (
                        self.get_transaction(&tx.transaction)?,
                        self.execution_cache
                            .get_effects(&tx.effects)
                            .map_err(sui_types::storage::error::Error::custom)?,
                    ) {
                        transactions.push(sui_types::base_types::ExecutionData::new(
                            (*t).clone().into_inner(),
                            e,
                        ))
                    } else {
                        return Result::<
                            Option<FullCheckpointContents>,
                            sui_types::storage::error::Error,
                        >::Ok(None);
                    }
                }
                Ok(Some(
                    FullCheckpointContents::from_contents_and_execution_data(
                        contents,
                        transactions.into_iter(),
                    ),
                ))
            })
            .transpose()
            .map(|contents| contents.flatten())
            .map_err(sui_types::storage::error::Error::custom)
    }

    fn get_committee(
        &self,
        epoch: EpochId,
    ) -> Result<Option<Arc<Committee>>, sui_types::storage::error::Error> {
        Ok(self.committee_store.get_committee(&epoch).unwrap())
    }

    fn get_transaction(
        &self,
        digest: &TransactionDigest,
    ) -> Result<Option<Arc<VerifiedTransaction>>, StorageError> {
        self.execution_cache
            .get_transaction_block(digest)
            .map_err(StorageError::custom)
    }

    fn get_transaction_effects(
        &self,
        digest: &TransactionDigest,
    ) -> Result<Option<TransactionEffects>, StorageError> {
        self.execution_cache
            .get_executed_effects(digest)
            .map_err(StorageError::custom)
    }

    fn get_events(
        &self,
        digest: &TransactionEventsDigest,
    ) -> Result<Option<TransactionEvents>, StorageError> {
        self.execution_cache
            .get_events(digest)
            .map_err(StorageError::custom)
    }

    fn get_latest_checkpoint(&self) -> sui_types::storage::error::Result<VerifiedCheckpoint> {
        self.checkpoint_store
            .get_latest_certified_checkpoint()
            .ok_or_else(|| {
                sui_types::storage::error::Error::missing("unable to get latest checkpoint")
            })
    }

    fn get_checkpoint_contents_by_digest(
        &self,
        digest: &CheckpointContentsDigest,
    ) -> sui_types::storage::error::Result<Option<sui_types::messages_checkpoint::CheckpointContents>>
    {
        self.checkpoint_store
            .get_checkpoint_contents(digest)
            .map_err(sui_types::storage::error::Error::custom)
    }

    fn get_checkpoint_contents_by_sequence_number(
        &self,
        _sequence_number: CheckpointSequenceNumber,
    ) -> sui_types::storage::error::Result<Option<sui_types::messages_checkpoint::CheckpointContents>>
    {
        todo!()
    }
}

impl ObjectStore for RocksDbStore {
    fn get_object(
        &self,
        object_id: &sui_types::base_types::ObjectID,
    ) -> sui_types::storage::error::Result<Option<Object>> {
        self.object_store.get_object(object_id)
    }

    fn get_object_by_key(
        &self,
        object_id: &sui_types::base_types::ObjectID,
        version: sui_types::base_types::VersionNumber,
    ) -> sui_types::storage::error::Result<Option<Object>> {
        self.object_store.get_object_by_key(object_id, version)
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
        self.state_sync_store
            .multi_insert_transaction_and_effects(contents.transactions())
            .map_err(sui_types::storage::error::Error::custom)?;
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
