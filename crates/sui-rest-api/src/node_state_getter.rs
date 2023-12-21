// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::{HashMap, HashSet};
use sui_core::authority::AuthorityState;
use sui_types::committee::EpochId;
use sui_types::effects::TransactionEffectsAPI;
use sui_types::error::UserInputError;
use sui_types::full_checkpoint_content::{CheckpointData, CheckpointTransaction};
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
    fn get_latest_epoch_id(&self) -> SuiResult<EpochId> {
        let latest_checkpoint_id = self.get_latest_checkpoint_sequence_number()?;
        let latest_checkpoint =
            self.get_verified_checkpoint_by_sequence_number(latest_checkpoint_id)?;
        Ok(latest_checkpoint.epoch())
    }

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

    fn get_checkpoint_data(
        &self,
        checkpoint: VerifiedCheckpoint,
        checkpoint_contents: CheckpointContents,
    ) -> anyhow::Result<CheckpointData> {
        let transaction_digests = checkpoint_contents
            .iter()
            .map(|execution_digests| execution_digests.transaction)
            .collect::<Vec<_>>();
        let transactions = self
            .multi_get_transaction_blocks(&transaction_digests)?
            .into_iter()
            .map(|maybe_transaction| {
                maybe_transaction.ok_or_else(|| anyhow::anyhow!("missing transaction"))
            })
            .collect::<anyhow::Result<Vec<_>>>()?;

        let effects = self
            .multi_get_executed_effects(&transaction_digests)?
            .into_iter()
            .map(|maybe_effects| maybe_effects.ok_or_else(|| anyhow::anyhow!("missing effects")))
            .collect::<anyhow::Result<Vec<_>>>()?;

        let event_digests = effects
            .iter()
            .flat_map(|fx| fx.events_digest().copied())
            .collect::<Vec<_>>();

        let events = self
            .multi_get_events(&event_digests)?
            .into_iter()
            .map(|maybe_event| maybe_event.ok_or_else(|| anyhow::anyhow!("missing event")))
            .collect::<anyhow::Result<Vec<_>>>()?;

        let events = event_digests
            .into_iter()
            .zip(events)
            .collect::<HashMap<_, _>>();
        let mut full_transactions = Vec::with_capacity(transactions.len());
        for (tx, fx) in transactions.into_iter().zip(effects) {
            let events = fx.events_digest().map(|event_digest| {
                events
                    .get(event_digest)
                    .cloned()
                    .expect("event was already checked to be present")
            });
            // Note unwrapped_then_deleted contains **updated** versions.
            let unwrapped_then_deleted_obj_ids = fx
                .unwrapped_then_deleted()
                .into_iter()
                .map(|k| k.0)
                .collect::<HashSet<_>>();

            let input_object_keys = fx
                .input_shared_objects()
                .into_iter()
                .map(|kind| {
                    let (id, version) = kind.id_and_version();
                    ObjectKey(id, version)
                })
                .chain(
                    fx.modified_at_versions()
                        .into_iter()
                        .map(|(object_id, version)| ObjectKey(object_id, version)),
                )
                .collect::<HashSet<_>>()
                .into_iter()
                // Unwrapped-then-deleted objects are not stored in state before the tx, so we have nothing to fetch.
                .filter(|key| !unwrapped_then_deleted_obj_ids.contains(&key.0))
                .collect::<Vec<_>>();

            let input_objects = self
                .multi_get_object_by_key(&input_object_keys)?
                .into_iter()
                .enumerate()
                .map(|(idx, maybe_object)| {
                    maybe_object.ok_or_else(|| {
                        anyhow::anyhow!(
                            "missing input object key {:?} from tx {}",
                            input_object_keys[idx],
                            tx.digest()
                        )
                    })
                })
                .collect::<anyhow::Result<Vec<_>>>()?;

            let output_object_keys = fx
                .all_changed_objects()
                .into_iter()
                .map(|(object_ref, _owner, _kind)| ObjectKey::from(object_ref))
                .collect::<Vec<_>>();

            let output_objects = self
                .multi_get_object_by_key(&output_object_keys)?
                .into_iter()
                .enumerate()
                .map(|(idx, maybe_object)| {
                    maybe_object.ok_or_else(|| {
                        anyhow::anyhow!(
                            "missing output object key {:?} from tx {}",
                            output_object_keys[idx],
                            tx.digest()
                        )
                    })
                })
                .collect::<anyhow::Result<Vec<_>>>()?;

            let full_transaction = CheckpointTransaction {
                transaction: tx.into(),
                effects: fx,
                events,
                input_objects,
                output_objects,
            };

            full_transactions.push(full_transaction);
        }

        let checkpoint_data = CheckpointData {
            checkpoint_summary: checkpoint.clone().into(),
            checkpoint_contents: self.get_checkpoint_contents(checkpoint.content_digest)?,
            transactions: full_transactions,
        };

        Ok(checkpoint_data)
    }
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
