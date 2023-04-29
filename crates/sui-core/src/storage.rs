// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

use sui_types::base_types::TransactionDigest;
use sui_types::committee::Committee;
use sui_types::committee::EpochId;
use sui_types::digests::{TransactionEffectsDigest, TransactionEventsDigest};
use sui_types::effects::{TransactionEffects, TransactionEvents};
use sui_types::messages::VerifiedTransaction;
use sui_types::messages_checkpoint::CheckpointContentsDigest;
use sui_types::messages_checkpoint::CheckpointDigest;
use sui_types::messages_checkpoint::CheckpointSequenceNumber;
use sui_types::messages_checkpoint::EndOfEpochData;
use sui_types::messages_checkpoint::FullCheckpointContents;
use sui_types::messages_checkpoint::VerifiedCheckpoint;
use sui_types::messages_checkpoint::VerifiedCheckpointContents;
use sui_types::storage::ReadStore;
use sui_types::storage::WriteStore;
use typed_store::Map;

use crate::authority::AuthorityStore;
use crate::checkpoints::CheckpointStore;
use crate::epoch::committee_store::CommitteeStore;

#[derive(Clone)]
pub struct RocksDbStore {
    authority_store: Arc<AuthorityStore>,
    committee_store: Arc<CommitteeStore>,
    checkpoint_store: Arc<CheckpointStore>,
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
        }
    }
}

impl ReadStore for RocksDbStore {
    type Error = typed_store::rocks::TypedStoreError;

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
        self.checkpoint_store
            .get_checkpoint_contents(digest)?
            .map(|contents| FullCheckpointContents::from_checkpoint_contents(&self, contents))
            .transpose()
            .map(|contents| contents.flatten())
    }

    fn get_committee(&self, epoch: EpochId) -> Result<Option<Arc<Committee>>, Self::Error> {
        Ok(self.committee_store.get_committee(&epoch).unwrap())
    }

    fn get_transaction_block(
        &self,
        digest: &TransactionDigest,
    ) -> Result<Option<VerifiedTransaction>, Self::Error> {
        self.authority_store.get_transaction_block(digest)
    }

    fn get_transaction_effects(
        &self,
        digest: &TransactionEffectsDigest,
    ) -> Result<Option<TransactionEffects>, Self::Error> {
        self.authority_store.perpetual_tables.effects.get(digest)
    }

    fn get_transaction_events(
        &self,
        digest: &TransactionEventsDigest,
    ) -> Result<Option<TransactionEvents>, Self::Error> {
        self.authority_store.get_events(digest)
    }
}

impl WriteStore for RocksDbStore {
    fn insert_checkpoint(&self, checkpoint: VerifiedCheckpoint) -> Result<(), Self::Error> {
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
        self.checkpoint_store
            .update_highest_synced_checkpoint(checkpoint)
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
