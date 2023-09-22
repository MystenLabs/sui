// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use parking_lot::Mutex;
use std::collections::HashMap;
use std::sync::Arc;
use sui_types::effects::TransactionEffectsAPI;

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
use sui_types::storage::ReadStore;
use sui_types::storage::WriteStore;
use sui_types::transaction::VerifiedTransaction;
use typed_store::Map;

use crate::authority::AuthorityStore;
use crate::checkpoints::CheckpointStore;
use crate::epoch::committee_store::CommitteeStore;

#[derive(Clone)]
pub struct RocksDbStore {
    authority_store: Arc<AuthorityStore>,
    committee_store: Arc<CommitteeStore>,
    checkpoint_store: Arc<CheckpointStore>,
    // in memory checkpoint watermark sequence numbers
    highest_verified_checkpoint: Arc<Mutex<Option<u64>>>,
    highest_synced_checkpoint: Arc<Mutex<Option<u64>>>,
}

impl RocksDbStore {
    pub fn new(
        authority_store: Arc<AuthorityStore>,
        committee_store: Arc<CommitteeStore>,
        checkpoint_store: Arc<CheckpointStore>,
    ) -> Self {
        Self {
            authority_store,
            committee_store,
            checkpoint_store,
            highest_verified_checkpoint: Arc::new(Mutex::new(None)),
            highest_synced_checkpoint: Arc::new(Mutex::new(None)),
        }
    }
}

impl sui_types::storage::Store for RocksDbStore {
    type Error = typed_store::rocks::TypedStoreError;
}

impl sui_types::storage::CommitteeStore for RocksDbStore {
    fn get_committee(&self, epoch: EpochId) -> Result<Option<Arc<Committee>>, Self::Error> {
        Ok(self.committee_store.get_committee(&epoch).unwrap())
    }
}

impl sui_types::storage::TransactionStore for RocksDbStore {
    fn get_transaction(
        &self,
        digest: &TransactionDigest,
    ) -> Result<Option<VerifiedTransaction>, Self::Error> {
        self.authority_store.get_transaction_block(digest)
    }

    fn get_transaction_effects(
        &self,
        digest: &TransactionDigest,
    ) -> Result<Option<TransactionEffects>, Self::Error> {
        let digest = match self
            .authority_store
            .perpetual_tables
            .executed_effects
            .get(digest)
        {
            Ok(Some(digest)) => digest,
            Ok(None) => return Ok(None),
            Err(e) => return Err(e),
        };
        self.authority_store.perpetual_tables.effects.get(&digest)
    }

    fn multi_get_transactions(
        &self,
        tx_digests: &[TransactionDigest],
    ) -> Result<Vec<Option<VerifiedTransaction>>, Self::Error> {
        self.authority_store
            .perpetual_tables
            .transactions
            .multi_get(tx_digests)
            .map(|v| v.into_iter().map(|v| v.map(|v| v.into())).collect())
    }

    fn multi_get_transaction_effects(
        &self,
        tx_digests: &[TransactionDigest],
    ) -> Result<Vec<Option<TransactionEffects>>, Self::Error> {
        let executed_effects_digests = self
            .authority_store
            .perpetual_tables
            .executed_effects
            .multi_get(tx_digests)?;
        let effects = self
            .authority_store
            .perpetual_tables
            .effects
            .multi_get(executed_effects_digests.iter().flatten())?;
        let mut tx_to_effects_map = effects
            .into_iter()
            .flatten()
            .map(|effects| (*effects.transaction_digest(), effects))
            .collect::<HashMap<_, _>>();
        Ok(tx_digests
            .iter()
            .map(|digest| tx_to_effects_map.remove(digest))
            .collect())
    }
}

impl sui_types::storage::EventStore for RocksDbStore {
    fn get_events(
        &self,
        event_digest: &TransactionEventsDigest,
    ) -> Result<Option<TransactionEvents>, Self::Error> {
        self.authority_store.get_events(event_digest)
    }
}

impl sui_types::storage::CheckpointStore for RocksDbStore {
    fn get_latest_checkpoint(&self) -> Result<VerifiedCheckpoint, Self::Error> {
        Ok(self
            .checkpoint_store
            .get_highest_executed_checkpoint()?
            .expect("should always have at least 1 executed checkpoint"))
    }

    fn get_checkpoint_by_digest(
        &self,
        digest: &CheckpointDigest,
    ) -> Result<Option<VerifiedCheckpoint>, Self::Error> {
        self.checkpoint_store.get_checkpoint_by_digest(digest)
    }

    fn get_checkpoint_by_sequence_number(
        &self,
        sequence_number: CheckpointSequenceNumber,
    ) -> Result<Option<VerifiedCheckpoint>, Self::Error> {
        self.checkpoint_store
            .get_checkpoint_by_sequence_number(sequence_number)
    }

    fn get_checkpoint_contents_by_digest(
        &self,
        digest: &CheckpointContentsDigest,
    ) -> Result<Option<sui_types::messages_checkpoint::CheckpointContents>, Self::Error> {
        self.checkpoint_store.get_checkpoint_contents(digest)
    }

    fn get_checkpoint_contents_by_sequence_number(
        &self,
        sequence_number: CheckpointSequenceNumber,
    ) -> Result<Option<sui_types::messages_checkpoint::CheckpointContents>, Self::Error> {
        let checkpoint = match self
            .checkpoint_store
            .get_checkpoint_by_sequence_number(sequence_number)
        {
            Ok(Some(checkpoint)) => checkpoint,
            Ok(None) => return Ok(None),
            Err(e) => return Err(e),
        };
        self.checkpoint_store
            .get_checkpoint_contents(&checkpoint.content_digest)
    }
}

impl sui_types::storage::ObjectStore2 for RocksDbStore {
    fn get_object(
        &self,
        object_id: &sui_types::base_types::ObjectID,
    ) -> Result<Option<sui_types::object::Object>, Self::Error> {
        self.authority_store.perpetual_tables.get_object(object_id)
    }

    fn get_object_by_key(
        &self,
        object_id: &sui_types::base_types::ObjectID,
        version: sui_types::base_types::VersionNumber,
    ) -> Result<Option<sui_types::object::Object>, Self::Error> {
        self.authority_store
            .perpetual_tables
            .get_object_by_key(object_id, version)
    }
}

impl ReadStore for RocksDbStore {
    fn get_checkpoint_by_digest(
        &self,
        digest: &CheckpointDigest,
    ) -> Result<Option<VerifiedCheckpoint>, Self::Error> {
        self.checkpoint_store.get_checkpoint_by_digest(digest)
    }

    fn get_checkpoint_by_sequence_number(
        &self,
        sequence_number: CheckpointSequenceNumber,
    ) -> Result<Option<VerifiedCheckpoint>, Self::Error> {
        self.checkpoint_store
            .get_checkpoint_by_sequence_number(sequence_number)
    }

    fn get_highest_verified_checkpoint(&self) -> Result<VerifiedCheckpoint, Self::Error> {
        self.checkpoint_store
            .get_highest_verified_checkpoint()
            .map(|maybe_checkpoint| {
                maybe_checkpoint
                    .expect("storage should have been initialized with genesis checkpoint")
            })
    }

    fn get_highest_synced_checkpoint(&self) -> Result<VerifiedCheckpoint, Self::Error> {
        self.checkpoint_store
            .get_highest_synced_checkpoint()
            .map(|maybe_checkpoint| {
                maybe_checkpoint
                    .expect("storage should have been initialized with genesis checkpoint")
            })
    }

    fn get_lowest_available_checkpoint(&self) -> Result<CheckpointSequenceNumber, Self::Error> {
        self.checkpoint_store
            .get_highest_pruned_checkpoint_seq_number()
            .map(|seq| seq + 1)
    }

    fn get_full_checkpoint_contents_by_sequence_number(
        &self,
        sequence_number: CheckpointSequenceNumber,
    ) -> Result<Option<FullCheckpointContents>, Self::Error> {
        self.checkpoint_store
            .get_full_checkpoint_contents_by_sequence_number(sequence_number)
    }

    fn get_full_checkpoint_contents(
        &self,
        digest: &CheckpointContentsDigest,
    ) -> Result<Option<FullCheckpointContents>, Self::Error> {
        // First look to see if we saved the complete contents already.
        if let Some(seq_num) = self
            .checkpoint_store
            .get_sequence_number_by_contents_digest(digest)?
        {
            let contents = self
                .checkpoint_store
                .get_full_checkpoint_contents_by_sequence_number(seq_num)?;
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
            .get_checkpoint_contents(digest)?
            .map(|contents| FullCheckpointContents::from_checkpoint_contents(&self, contents))
            .transpose()
            .map(|contents| contents.flatten())
    }
}

impl WriteStore for RocksDbStore {
    fn insert_checkpoint(&self, checkpoint: &VerifiedCheckpoint) -> Result<(), Self::Error> {
        if let Some(EndOfEpochData {
            next_epoch_committee,
            ..
        }) = checkpoint.end_of_epoch_data.as_ref()
        {
            let next_committee = next_epoch_committee.iter().cloned().collect();
            let committee = Committee::new(checkpoint.epoch().saturating_add(1), next_committee);
            self.insert_committee(committee)?;
        }

        self.checkpoint_store.insert_verified_checkpoint(checkpoint)
    }

    fn update_highest_synced_checkpoint(
        &self,
        checkpoint: &VerifiedCheckpoint,
    ) -> Result<(), Self::Error> {
        let mut locked = self.highest_synced_checkpoint.lock();
        if locked.is_some() && locked.unwrap() >= checkpoint.sequence_number {
            return Ok(());
        }
        self.checkpoint_store
            .update_highest_synced_checkpoint(checkpoint)?;
        *locked = Some(checkpoint.sequence_number);
        Ok(())
    }

    fn update_highest_verified_checkpoint(
        &self,
        checkpoint: &VerifiedCheckpoint,
    ) -> Result<(), Self::Error> {
        let mut locked = self.highest_verified_checkpoint.lock();
        if locked.is_some() && locked.unwrap() >= checkpoint.sequence_number {
            return Ok(());
        }
        self.checkpoint_store
            .update_highest_verified_checkpoint(checkpoint)?;
        *locked = Some(checkpoint.sequence_number);
        Ok(())
    }

    fn insert_checkpoint_contents(
        &self,
        checkpoint: &VerifiedCheckpoint,
        contents: VerifiedCheckpointContents,
    ) -> Result<(), Self::Error> {
        self.authority_store
            .multi_insert_transaction_and_effects(contents.iter())?;
        self.checkpoint_store
            .insert_verified_checkpoint_contents(checkpoint, contents)
    }

    fn insert_committee(&self, new_committee: Committee) -> Result<(), Self::Error> {
        self.committee_store
            .insert_new_committee(&new_committee)
            .unwrap();
        Ok(())
    }
}
