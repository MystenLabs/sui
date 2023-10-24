// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::base_types::EpochId;
use crate::committee::Committee;
use crate::digests::{
    CheckpointContentsDigest, CheckpointDigest, TransactionDigest, TransactionEffectsDigest,
    TransactionEventsDigest,
};
use crate::effects::{TransactionEffects, TransactionEvents};
use crate::messages_checkpoint::{
    CheckpointSequenceNumber, FullCheckpointContents, VerifiedCheckpoint,
};
use crate::transaction::VerifiedTransaction;
use std::sync::Arc;

pub trait ReadStore {
    type Error;

    fn get_checkpoint_by_digest(
        &self,
        digest: &CheckpointDigest,
    ) -> Result<Option<VerifiedCheckpoint>, Self::Error>;

    fn get_checkpoint_by_sequence_number(
        &self,
        sequence_number: CheckpointSequenceNumber,
    ) -> Result<Option<VerifiedCheckpoint>, Self::Error>;

    fn get_highest_verified_checkpoint(&self) -> Result<VerifiedCheckpoint, Self::Error>;

    fn get_highest_synced_checkpoint(&self) -> Result<VerifiedCheckpoint, Self::Error>;

    fn get_lowest_available_checkpoint(&self) -> Result<CheckpointSequenceNumber, Self::Error>;

    fn get_full_checkpoint_contents_by_sequence_number(
        &self,
        sequence_number: CheckpointSequenceNumber,
    ) -> Result<Option<FullCheckpointContents>, Self::Error>;

    fn get_full_checkpoint_contents(
        &self,
        digest: &CheckpointContentsDigest,
    ) -> Result<Option<FullCheckpointContents>, Self::Error>;

    fn get_committee(&self, epoch: EpochId) -> Result<Option<Arc<Committee>>, Self::Error>;

    fn get_transaction_block(
        &self,
        digest: &TransactionDigest,
    ) -> Result<Option<VerifiedTransaction>, Self::Error>;

    fn get_transaction_effects(
        &self,
        digest: &TransactionEffectsDigest,
    ) -> Result<Option<TransactionEffects>, Self::Error>;

    fn get_transaction_events(
        &self,
        digest: &TransactionEventsDigest,
    ) -> Result<Option<TransactionEvents>, Self::Error>;
}

impl<T: ReadStore> ReadStore for &T {
    type Error = T::Error;

    fn get_checkpoint_by_digest(
        &self,
        digest: &CheckpointDigest,
    ) -> Result<Option<VerifiedCheckpoint>, Self::Error> {
        ReadStore::get_checkpoint_by_digest(*self, digest)
    }

    fn get_checkpoint_by_sequence_number(
        &self,
        sequence_number: CheckpointSequenceNumber,
    ) -> Result<Option<VerifiedCheckpoint>, Self::Error> {
        ReadStore::get_checkpoint_by_sequence_number(*self, sequence_number)
    }

    fn get_highest_verified_checkpoint(&self) -> Result<VerifiedCheckpoint, Self::Error> {
        ReadStore::get_highest_verified_checkpoint(*self)
    }

    fn get_highest_synced_checkpoint(&self) -> Result<VerifiedCheckpoint, Self::Error> {
        ReadStore::get_highest_synced_checkpoint(*self)
    }

    fn get_lowest_available_checkpoint(&self) -> Result<CheckpointSequenceNumber, Self::Error> {
        ReadStore::get_lowest_available_checkpoint(*self)
    }

    fn get_full_checkpoint_contents_by_sequence_number(
        &self,
        sequence_number: CheckpointSequenceNumber,
    ) -> Result<Option<FullCheckpointContents>, Self::Error> {
        ReadStore::get_full_checkpoint_contents_by_sequence_number(*self, sequence_number)
    }

    fn get_full_checkpoint_contents(
        &self,
        digest: &CheckpointContentsDigest,
    ) -> Result<Option<FullCheckpointContents>, Self::Error> {
        ReadStore::get_full_checkpoint_contents(*self, digest)
    }

    fn get_committee(&self, epoch: EpochId) -> Result<Option<Arc<Committee>>, Self::Error> {
        ReadStore::get_committee(*self, epoch)
    }

    fn get_transaction_block(
        &self,
        digest: &TransactionDigest,
    ) -> Result<Option<VerifiedTransaction>, Self::Error> {
        ReadStore::get_transaction_block(*self, digest)
    }

    fn get_transaction_effects(
        &self,
        digest: &TransactionEffectsDigest,
    ) -> Result<Option<TransactionEffects>, Self::Error> {
        ReadStore::get_transaction_effects(*self, digest)
    }

    fn get_transaction_events(
        &self,
        digest: &TransactionEventsDigest,
    ) -> Result<Option<TransactionEvents>, Self::Error> {
        ReadStore::get_transaction_events(*self, digest)
    }
}
