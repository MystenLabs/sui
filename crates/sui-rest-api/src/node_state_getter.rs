// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use sui_core::authority::AuthorityState;
use sui_types::error::UserInputError;
use sui_types::{
    base_types::{ObjectID, VersionNumber},
    digests::{TransactionDigest, TransactionEventsDigest},
    effects::{TransactionEffects, TransactionEvents},
    error::{SuiError, SuiResult},
    messages_checkpoint::{
        CheckpointContents, CheckpointContentsDigest, CheckpointSequenceNumber, VerifiedCheckpoint,
    },
    object::Object,
    storage::{ObjectKey, ObjectStore},
    transaction::VerifiedTransaction,
};

/// Trait for getting data from the node state.
/// TODO: need a better name for this?
pub trait NodeStateGetter: Sync + Send {
    fn get_verified_checkpoint_by_sequence_number(
        &self,
        sequence_number: CheckpointSequenceNumber,
    ) -> SuiResult<VerifiedCheckpoint>;

    fn get_latest_checkpoint_sequence_number(&self) -> SuiResult<CheckpointSequenceNumber>;

    fn get_checkpoint_contents(
        &self,
        content_digest: CheckpointContentsDigest,
    ) -> SuiResult<CheckpointContents>;

    fn multi_get_transaction_blocks(
        &self,
        tx_digests: &[TransactionDigest],
    ) -> SuiResult<Vec<Option<VerifiedTransaction>>>;

    fn multi_get_executed_effects(
        &self,
        digests: &[TransactionDigest],
    ) -> SuiResult<Vec<Option<TransactionEffects>>>;

    fn multi_get_events(
        &self,
        event_digests: &[TransactionEventsDigest],
    ) -> SuiResult<Vec<Option<TransactionEvents>>>;

    fn multi_get_object_by_key(
        &self,
        object_keys: &[ObjectKey],
    ) -> Result<Vec<Option<Object>>, SuiError>;

    fn get_object_by_key(
        &self,
        object_id: &ObjectID,
        version: VersionNumber,
    ) -> Result<Option<Object>, SuiError>;

    fn get_object(&self, object_id: &ObjectID) -> Result<Option<Object>, SuiError>;
}

impl NodeStateGetter for AuthorityState {
    fn get_verified_checkpoint_by_sequence_number(
        &self,
        sequence_number: CheckpointSequenceNumber,
    ) -> SuiResult<VerifiedCheckpoint> {
        self.get_verified_checkpoint_by_sequence_number(sequence_number)
    }

    fn get_latest_checkpoint_sequence_number(&self) -> SuiResult<CheckpointSequenceNumber> {
        self.get_latest_checkpoint_sequence_number()
    }

    fn get_checkpoint_contents(
        &self,
        content_digest: CheckpointContentsDigest,
    ) -> SuiResult<CheckpointContents> {
        self.get_checkpoint_contents(content_digest)
    }

    fn multi_get_transaction_blocks(
        &self,
        tx_digests: &[TransactionDigest],
    ) -> SuiResult<Vec<Option<VerifiedTransaction>>> {
        self.database.multi_get_transaction_blocks(tx_digests)
    }

    fn multi_get_executed_effects(
        &self,
        digests: &[TransactionDigest],
    ) -> SuiResult<Vec<Option<TransactionEffects>>> {
        self.database.multi_get_executed_effects(digests)
    }

    fn multi_get_events(
        &self,
        event_digests: &[TransactionEventsDigest],
    ) -> SuiResult<Vec<Option<TransactionEvents>>> {
        self.database.multi_get_events(event_digests)
    }

    fn multi_get_object_by_key(
        &self,
        object_keys: &[ObjectKey],
    ) -> Result<Vec<Option<Object>>, SuiError> {
        self.database.multi_get_object_by_key(object_keys)
    }

    fn get_object_by_key(
        &self,
        object_id: &ObjectID,
        version: VersionNumber,
    ) -> Result<Option<Object>, SuiError> {
        self.database.get_object_by_key(object_id, version)
    }

    fn get_object(&self, object_id: &ObjectID) -> Result<Option<Object>, SuiError> {
        self.database.get_object(object_id)
    }
}

impl<T: Sync + Send, W: simulacrum::SimulatorStore + Sync + Send> NodeStateGetter
    for simulacrum::Simulacrum<T, W>
{
    fn get_verified_checkpoint_by_sequence_number(
        &self,
        sequence_number: CheckpointSequenceNumber,
    ) -> SuiResult<VerifiedCheckpoint> {
        self.store()
            .get_checkpoint_by_sequence_number(sequence_number)
            .ok_or(SuiError::UserInputError {
                error: UserInputError::VerifiedCheckpointNotFound(sequence_number),
            })
    }

    fn get_latest_checkpoint_sequence_number(&self) -> SuiResult<CheckpointSequenceNumber> {
        Ok(self
            .store()
            .get_highest_checkpint()
            .map(|checkpoint| *checkpoint.sequence_number())
            .unwrap_or(0))
    }

    fn get_checkpoint_contents(
        &self,
        content_digest: CheckpointContentsDigest,
    ) -> SuiResult<CheckpointContents> {
        self.store()
            .get_checkpoint_contents(&content_digest)
            .ok_or(SuiError::UserInputError {
                error: UserInputError::CheckpointContentsNotFound(content_digest),
            })
    }

    fn multi_get_transaction_blocks(
        &self,
        tx_digests: &[TransactionDigest],
    ) -> SuiResult<Vec<Option<VerifiedTransaction>>> {
        Ok(tx_digests
            .iter()
            .map(|digest| self.store().get_transaction(digest))
            .collect())
    }

    fn multi_get_executed_effects(
        &self,
        digests: &[TransactionDigest],
    ) -> SuiResult<Vec<Option<TransactionEffects>>> {
        Ok(digests
            .iter()
            .map(|digest| self.store().get_transaction_effects(digest))
            .collect())
    }

    fn multi_get_events(
        &self,
        event_digests: &[TransactionEventsDigest],
    ) -> SuiResult<Vec<Option<TransactionEvents>>> {
        Ok(event_digests
            .iter()
            .map(|digest| self.store().get_transaction_events(digest))
            .collect())
    }

    fn multi_get_object_by_key(
        &self,
        object_keys: &[ObjectKey],
    ) -> Result<Vec<Option<Object>>, SuiError> {
        object_keys
            .iter()
            .map(|key| self.store().get_object_by_key(&key.0, key.1))
            .collect::<Result<Vec<_>, SuiError>>()
    }

    fn get_object_by_key(
        &self,
        object_id: &ObjectID,
        version: VersionNumber,
    ) -> Result<Option<Object>, SuiError> {
        Ok(self.store().get_object_at_version(object_id, version))
    }

    fn get_object(&self, object_id: &ObjectID) -> Result<Option<Object>, SuiError> {
        Ok(simulacrum::SimulatorStore::get_object(
            self.store(),
            object_id,
        ))
    }
}
