// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use super::error::Result;
use super::ObjectStore;
use crate::base_types::EpochId;
use crate::committee::Committee;
use crate::digests::{
    CheckpointContentsDigest, CheckpointDigest, TransactionDigest, TransactionEventsDigest,
};
use crate::effects::{TransactionEffects, TransactionEvents};
use crate::full_checkpoint_content::CheckpointData;
use crate::messages_checkpoint::{
    CheckpointContents, CheckpointSequenceNumber, FullCheckpointContents, VerifiedCheckpoint,
};
use crate::transaction::VerifiedTransaction;
use std::sync::Arc;

pub trait ReadStore: ObjectStore {
    //
    // Committee Getters
    //

    fn get_committee(&self, epoch: EpochId) -> Result<Option<Arc<Committee>>>;

    //
    // Checkpoint Getters
    //

    /// Get the latest available checkpoint. This is the latest executed checkpoint.
    ///
    /// All transactions, effects, objects and events are guaranteed to be available for the
    /// returned checkpoint.
    fn get_latest_checkpoint(&self) -> Result<VerifiedCheckpoint>;

    /// Get the latest available checkpoint sequence number. This is the sequence number of the latest executed checkpoint.
    fn get_latest_checkpoint_sequence_number(&self) -> Result<CheckpointSequenceNumber> {
        let latest_checkpoint = self.get_latest_checkpoint()?;
        Ok(*latest_checkpoint.sequence_number())
    }

    /// Get the epoch of the latest checkpoint
    fn get_latest_epoch_id(&self) -> Result<EpochId> {
        let latest_checkpoint = self.get_latest_checkpoint()?;
        Ok(latest_checkpoint.epoch())
    }

    /// Get the highest verified checkpint. This is the highest checkpoint summary that has been
    /// verified, generally by state-sync. Only the checkpoint header is guaranteed to be present in
    /// the store.
    fn get_highest_verified_checkpoint(&self) -> Result<VerifiedCheckpoint>;

    /// Get the highest synced checkpint. This is the highest checkpoint that has been synced from
    /// state-synce. The checkpoint header, contents, transactions, and effects of this checkpoint
    /// are guaranteed to be present in the store
    fn get_highest_synced_checkpoint(&self) -> Result<VerifiedCheckpoint>;

    /// The lowest available checkpoint that hasn't yet been pruned.
    fn get_lowest_available_checkpoint(&self) -> Result<CheckpointSequenceNumber>;

    fn get_checkpoint_by_digest(
        &self,
        digest: &CheckpointDigest,
    ) -> Result<Option<VerifiedCheckpoint>>;

    fn get_checkpoint_by_sequence_number(
        &self,
        sequence_number: CheckpointSequenceNumber,
    ) -> Result<Option<VerifiedCheckpoint>>;

    fn get_checkpoint_contents_by_digest(
        &self,
        digest: &CheckpointContentsDigest,
    ) -> Result<Option<CheckpointContents>>;

    fn get_checkpoint_contents_by_sequence_number(
        &self,
        sequence_number: CheckpointSequenceNumber,
    ) -> Result<Option<CheckpointContents>>;

    //
    // Transaction Getters
    //

    fn get_transaction(
        &self,
        tx_digest: &TransactionDigest,
    ) -> Result<Option<Arc<VerifiedTransaction>>>;

    fn multi_get_transactions(
        &self,
        tx_digests: &[TransactionDigest],
    ) -> Result<Vec<Option<Arc<VerifiedTransaction>>>> {
        tx_digests
            .iter()
            .map(|digest| self.get_transaction(digest))
            .collect::<Result<Vec<_>, _>>()
    }

    fn get_transaction_effects(
        &self,
        tx_digest: &TransactionDigest,
    ) -> Result<Option<TransactionEffects>>;

    fn multi_get_transaction_effects(
        &self,
        tx_digests: &[TransactionDigest],
    ) -> Result<Vec<Option<TransactionEffects>>> {
        tx_digests
            .iter()
            .map(|digest| self.get_transaction_effects(digest))
            .collect::<Result<Vec<_>, _>>()
    }

    fn get_events(
        &self,
        event_digest: &TransactionEventsDigest,
    ) -> Result<Option<TransactionEvents>>;

    fn multi_get_events(
        &self,
        event_digests: &[TransactionEventsDigest],
    ) -> Result<Vec<Option<TransactionEvents>>> {
        event_digests
            .iter()
            .map(|digest| self.get_events(digest))
            .collect::<Result<Vec<_>, _>>()
    }

    //
    // Extra Checkpoint fetching apis
    //

    /// Get a "full" checkpoint for purposes of state-sync
    /// "full" checkpoints include: header, contents, transactions, effects
    fn get_full_checkpoint_contents_by_sequence_number(
        &self,
        sequence_number: CheckpointSequenceNumber,
    ) -> Result<Option<FullCheckpointContents>>;

    /// Get a "full" checkpoint for purposes of state-sync
    /// "full" checkpoints include: header, contents, transactions, effects
    fn get_full_checkpoint_contents(
        &self,
        digest: &CheckpointContentsDigest,
    ) -> Result<Option<FullCheckpointContents>>;

    // Fetch all checkpoint data
    // TODO fix return type to not be anyhow
    fn get_checkpoint_data(
        &self,
        checkpoint: VerifiedCheckpoint,
        checkpoint_contents: CheckpointContents,
    ) -> anyhow::Result<CheckpointData> {
        use super::ObjectKey;
        use crate::effects::TransactionEffectsAPI;
        use crate::full_checkpoint_content::CheckpointTransaction;
        use std::collections::{HashMap, HashSet};

        let transaction_digests = checkpoint_contents
            .iter()
            .map(|execution_digests| execution_digests.transaction)
            .collect::<Vec<_>>();
        let transactions = self
            .multi_get_transactions(&transaction_digests)?
            .into_iter()
            .map(|maybe_transaction| {
                maybe_transaction.ok_or_else(|| anyhow::anyhow!("missing transaction"))
            })
            .collect::<anyhow::Result<Vec<_>>>()?;

        let effects = self
            .multi_get_transaction_effects(&transaction_digests)?
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
                .multi_get_objects_by_key(&input_object_keys)?
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
                .multi_get_objects_by_key(&output_object_keys)?
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
                transaction: (*tx).clone().into(),
                effects: fx,
                events,
                input_objects,
                output_objects,
            };

            full_transactions.push(full_transaction);
        }

        let checkpoint_data = CheckpointData {
            checkpoint_summary: checkpoint.into(),
            checkpoint_contents,
            transactions: full_transactions,
        };

        Ok(checkpoint_data)
    }
}

impl<T: ReadStore + ?Sized> ReadStore for &T {
    fn get_committee(&self, epoch: EpochId) -> Result<Option<Arc<Committee>>> {
        (*self).get_committee(epoch)
    }

    fn get_latest_checkpoint(&self) -> Result<VerifiedCheckpoint> {
        (*self).get_latest_checkpoint()
    }

    fn get_latest_checkpoint_sequence_number(&self) -> Result<CheckpointSequenceNumber> {
        (*self).get_latest_checkpoint_sequence_number()
    }

    fn get_latest_epoch_id(&self) -> Result<EpochId> {
        (*self).get_latest_epoch_id()
    }

    fn get_highest_verified_checkpoint(&self) -> Result<VerifiedCheckpoint> {
        (*self).get_highest_verified_checkpoint()
    }

    fn get_highest_synced_checkpoint(&self) -> Result<VerifiedCheckpoint> {
        (*self).get_highest_synced_checkpoint()
    }

    fn get_lowest_available_checkpoint(&self) -> Result<CheckpointSequenceNumber> {
        (*self).get_lowest_available_checkpoint()
    }

    fn get_checkpoint_by_digest(
        &self,
        digest: &CheckpointDigest,
    ) -> Result<Option<VerifiedCheckpoint>> {
        (*self).get_checkpoint_by_digest(digest)
    }

    fn get_checkpoint_by_sequence_number(
        &self,
        sequence_number: CheckpointSequenceNumber,
    ) -> Result<Option<VerifiedCheckpoint>> {
        (*self).get_checkpoint_by_sequence_number(sequence_number)
    }

    fn get_checkpoint_contents_by_digest(
        &self,
        digest: &CheckpointContentsDigest,
    ) -> Result<Option<CheckpointContents>> {
        (*self).get_checkpoint_contents_by_digest(digest)
    }

    fn get_checkpoint_contents_by_sequence_number(
        &self,
        sequence_number: CheckpointSequenceNumber,
    ) -> Result<Option<CheckpointContents>> {
        (*self).get_checkpoint_contents_by_sequence_number(sequence_number)
    }

    fn get_transaction(
        &self,
        tx_digest: &TransactionDigest,
    ) -> Result<Option<Arc<VerifiedTransaction>>> {
        (*self).get_transaction(tx_digest)
    }

    fn multi_get_transactions(
        &self,
        tx_digests: &[TransactionDigest],
    ) -> Result<Vec<Option<Arc<VerifiedTransaction>>>> {
        (*self).multi_get_transactions(tx_digests)
    }

    fn get_transaction_effects(
        &self,
        tx_digest: &TransactionDigest,
    ) -> Result<Option<TransactionEffects>> {
        (*self).get_transaction_effects(tx_digest)
    }

    fn multi_get_transaction_effects(
        &self,
        tx_digests: &[TransactionDigest],
    ) -> Result<Vec<Option<TransactionEffects>>> {
        (*self).multi_get_transaction_effects(tx_digests)
    }

    fn get_events(
        &self,
        event_digest: &TransactionEventsDigest,
    ) -> Result<Option<TransactionEvents>> {
        (*self).get_events(event_digest)
    }

    fn multi_get_events(
        &self,
        event_digests: &[TransactionEventsDigest],
    ) -> Result<Vec<Option<TransactionEvents>>> {
        (*self).multi_get_events(event_digests)
    }

    fn get_full_checkpoint_contents_by_sequence_number(
        &self,
        sequence_number: CheckpointSequenceNumber,
    ) -> Result<Option<FullCheckpointContents>> {
        (*self).get_full_checkpoint_contents_by_sequence_number(sequence_number)
    }

    fn get_full_checkpoint_contents(
        &self,
        digest: &CheckpointContentsDigest,
    ) -> Result<Option<FullCheckpointContents>> {
        (*self).get_full_checkpoint_contents(digest)
    }

    fn get_checkpoint_data(
        &self,
        checkpoint: VerifiedCheckpoint,
        checkpoint_contents: CheckpointContents,
    ) -> anyhow::Result<CheckpointData> {
        (*self).get_checkpoint_data(checkpoint, checkpoint_contents)
    }
}

impl<T: ReadStore + ?Sized> ReadStore for Box<T> {
    fn get_committee(&self, epoch: EpochId) -> Result<Option<Arc<Committee>>> {
        (**self).get_committee(epoch)
    }

    fn get_latest_checkpoint(&self) -> Result<VerifiedCheckpoint> {
        (**self).get_latest_checkpoint()
    }

    fn get_latest_checkpoint_sequence_number(&self) -> Result<CheckpointSequenceNumber> {
        (**self).get_latest_checkpoint_sequence_number()
    }

    fn get_latest_epoch_id(&self) -> Result<EpochId> {
        (**self).get_latest_epoch_id()
    }

    fn get_highest_verified_checkpoint(&self) -> Result<VerifiedCheckpoint> {
        (**self).get_highest_verified_checkpoint()
    }

    fn get_highest_synced_checkpoint(&self) -> Result<VerifiedCheckpoint> {
        (**self).get_highest_synced_checkpoint()
    }

    fn get_lowest_available_checkpoint(&self) -> Result<CheckpointSequenceNumber> {
        (**self).get_lowest_available_checkpoint()
    }

    fn get_checkpoint_by_digest(
        &self,
        digest: &CheckpointDigest,
    ) -> Result<Option<VerifiedCheckpoint>> {
        (**self).get_checkpoint_by_digest(digest)
    }

    fn get_checkpoint_by_sequence_number(
        &self,
        sequence_number: CheckpointSequenceNumber,
    ) -> Result<Option<VerifiedCheckpoint>> {
        (**self).get_checkpoint_by_sequence_number(sequence_number)
    }

    fn get_checkpoint_contents_by_digest(
        &self,
        digest: &CheckpointContentsDigest,
    ) -> Result<Option<CheckpointContents>> {
        (**self).get_checkpoint_contents_by_digest(digest)
    }

    fn get_checkpoint_contents_by_sequence_number(
        &self,
        sequence_number: CheckpointSequenceNumber,
    ) -> Result<Option<CheckpointContents>> {
        (**self).get_checkpoint_contents_by_sequence_number(sequence_number)
    }

    fn get_transaction(
        &self,
        tx_digest: &TransactionDigest,
    ) -> Result<Option<Arc<VerifiedTransaction>>> {
        (**self).get_transaction(tx_digest)
    }

    fn multi_get_transactions(
        &self,
        tx_digests: &[TransactionDigest],
    ) -> Result<Vec<Option<Arc<VerifiedTransaction>>>> {
        (**self).multi_get_transactions(tx_digests)
    }

    fn get_transaction_effects(
        &self,
        tx_digest: &TransactionDigest,
    ) -> Result<Option<TransactionEffects>> {
        (**self).get_transaction_effects(tx_digest)
    }

    fn multi_get_transaction_effects(
        &self,
        tx_digests: &[TransactionDigest],
    ) -> Result<Vec<Option<TransactionEffects>>> {
        (**self).multi_get_transaction_effects(tx_digests)
    }

    fn get_events(
        &self,
        event_digest: &TransactionEventsDigest,
    ) -> Result<Option<TransactionEvents>> {
        (**self).get_events(event_digest)
    }

    fn multi_get_events(
        &self,
        event_digests: &[TransactionEventsDigest],
    ) -> Result<Vec<Option<TransactionEvents>>> {
        (**self).multi_get_events(event_digests)
    }

    fn get_full_checkpoint_contents_by_sequence_number(
        &self,
        sequence_number: CheckpointSequenceNumber,
    ) -> Result<Option<FullCheckpointContents>> {
        (**self).get_full_checkpoint_contents_by_sequence_number(sequence_number)
    }

    fn get_full_checkpoint_contents(
        &self,
        digest: &CheckpointContentsDigest,
    ) -> Result<Option<FullCheckpointContents>> {
        (**self).get_full_checkpoint_contents(digest)
    }

    fn get_checkpoint_data(
        &self,
        checkpoint: VerifiedCheckpoint,
        checkpoint_contents: CheckpointContents,
    ) -> anyhow::Result<CheckpointData> {
        (**self).get_checkpoint_data(checkpoint, checkpoint_contents)
    }
}

impl<T: ReadStore + ?Sized> ReadStore for Arc<T> {
    fn get_committee(&self, epoch: EpochId) -> Result<Option<Arc<Committee>>> {
        (**self).get_committee(epoch)
    }

    fn get_latest_checkpoint(&self) -> Result<VerifiedCheckpoint> {
        (**self).get_latest_checkpoint()
    }

    fn get_latest_checkpoint_sequence_number(&self) -> Result<CheckpointSequenceNumber> {
        (**self).get_latest_checkpoint_sequence_number()
    }

    fn get_latest_epoch_id(&self) -> Result<EpochId> {
        (**self).get_latest_epoch_id()
    }

    fn get_highest_verified_checkpoint(&self) -> Result<VerifiedCheckpoint> {
        (**self).get_highest_verified_checkpoint()
    }

    fn get_highest_synced_checkpoint(&self) -> Result<VerifiedCheckpoint> {
        (**self).get_highest_synced_checkpoint()
    }

    fn get_lowest_available_checkpoint(&self) -> Result<CheckpointSequenceNumber> {
        (**self).get_lowest_available_checkpoint()
    }

    fn get_checkpoint_by_digest(
        &self,
        digest: &CheckpointDigest,
    ) -> Result<Option<VerifiedCheckpoint>> {
        (**self).get_checkpoint_by_digest(digest)
    }

    fn get_checkpoint_by_sequence_number(
        &self,
        sequence_number: CheckpointSequenceNumber,
    ) -> Result<Option<VerifiedCheckpoint>> {
        (**self).get_checkpoint_by_sequence_number(sequence_number)
    }

    fn get_checkpoint_contents_by_digest(
        &self,
        digest: &CheckpointContentsDigest,
    ) -> Result<Option<CheckpointContents>> {
        (**self).get_checkpoint_contents_by_digest(digest)
    }

    fn get_checkpoint_contents_by_sequence_number(
        &self,
        sequence_number: CheckpointSequenceNumber,
    ) -> Result<Option<CheckpointContents>> {
        (**self).get_checkpoint_contents_by_sequence_number(sequence_number)
    }

    fn get_transaction(
        &self,
        tx_digest: &TransactionDigest,
    ) -> Result<Option<Arc<VerifiedTransaction>>> {
        (**self).get_transaction(tx_digest)
    }

    fn multi_get_transactions(
        &self,
        tx_digests: &[TransactionDigest],
    ) -> Result<Vec<Option<Arc<VerifiedTransaction>>>> {
        (**self).multi_get_transactions(tx_digests)
    }

    fn get_transaction_effects(
        &self,
        tx_digest: &TransactionDigest,
    ) -> Result<Option<TransactionEffects>> {
        (**self).get_transaction_effects(tx_digest)
    }

    fn multi_get_transaction_effects(
        &self,
        tx_digests: &[TransactionDigest],
    ) -> Result<Vec<Option<TransactionEffects>>> {
        (**self).multi_get_transaction_effects(tx_digests)
    }

    fn get_events(
        &self,
        event_digest: &TransactionEventsDigest,
    ) -> Result<Option<TransactionEvents>> {
        (**self).get_events(event_digest)
    }

    fn multi_get_events(
        &self,
        event_digests: &[TransactionEventsDigest],
    ) -> Result<Vec<Option<TransactionEvents>>> {
        (**self).multi_get_events(event_digests)
    }

    fn get_full_checkpoint_contents_by_sequence_number(
        &self,
        sequence_number: CheckpointSequenceNumber,
    ) -> Result<Option<FullCheckpointContents>> {
        (**self).get_full_checkpoint_contents_by_sequence_number(sequence_number)
    }

    fn get_full_checkpoint_contents(
        &self,
        digest: &CheckpointContentsDigest,
    ) -> Result<Option<FullCheckpointContents>> {
        (**self).get_full_checkpoint_contents(digest)
    }

    fn get_checkpoint_data(
        &self,
        checkpoint: VerifiedCheckpoint,
        checkpoint_contents: CheckpointContents,
    ) -> anyhow::Result<CheckpointData> {
        (**self).get_checkpoint_data(checkpoint, checkpoint_contents)
    }
}
