// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use super::error::Result;
use super::ObjectStore;
use crate::base_types::{EpochId, MoveObjectType, ObjectID, SequenceNumber, SuiAddress};
use crate::committee::Committee;
use crate::digests::{
    ChainIdentifier, CheckpointContentsDigest, CheckpointDigest, TransactionDigest,
    TransactionEventsDigest,
};
use crate::dynamic_field::DynamicFieldType;
use crate::effects::{TransactionEffects, TransactionEvents};
use crate::full_checkpoint_content::CheckpointData;
use crate::messages_checkpoint::{
    CheckpointContents, CheckpointSequenceNumber, FullCheckpointContents, VerifiedCheckpoint,
};
use crate::transaction::VerifiedTransaction;
use move_core_types::language_storage::StructTag;
use move_core_types::language_storage::TypeTag;
use serde::Deserialize;
use serde::Serialize;
use std::sync::Arc;

pub trait ReadStore: ObjectStore {
    //
    // Committee Getters
    //

    fn get_committee(&self, epoch: EpochId) -> Option<Arc<Committee>>;

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

    /// Lowest available checkpoint for which transaction and checkpoint data can be requested.
    ///
    /// Specifically this is the lowest checkpoint for which the following data can be requested:
    ///  - checkpoints
    ///  - transactions
    ///  - effects
    ///  - events
    ///
    /// For object availability see `get_lowest_available_checkpoint_objects`.
    fn get_lowest_available_checkpoint(&self) -> Result<CheckpointSequenceNumber>;

    fn get_checkpoint_by_digest(&self, digest: &CheckpointDigest) -> Option<VerifiedCheckpoint>;

    fn get_checkpoint_by_sequence_number(
        &self,
        sequence_number: CheckpointSequenceNumber,
    ) -> Option<VerifiedCheckpoint>;

    fn get_checkpoint_contents_by_digest(
        &self,
        digest: &CheckpointContentsDigest,
    ) -> Option<CheckpointContents>;

    fn get_checkpoint_contents_by_sequence_number(
        &self,
        sequence_number: CheckpointSequenceNumber,
    ) -> Option<CheckpointContents>;

    //
    // Transaction Getters
    //

    fn get_transaction(&self, tx_digest: &TransactionDigest) -> Option<Arc<VerifiedTransaction>>;

    fn multi_get_transactions(
        &self,
        tx_digests: &[TransactionDigest],
    ) -> Vec<Option<Arc<VerifiedTransaction>>> {
        tx_digests
            .iter()
            .map(|digest| self.get_transaction(digest))
            .collect()
    }

    fn get_transaction_effects(&self, tx_digest: &TransactionDigest) -> Option<TransactionEffects>;

    fn multi_get_transaction_effects(
        &self,
        tx_digests: &[TransactionDigest],
    ) -> Vec<Option<TransactionEffects>> {
        tx_digests
            .iter()
            .map(|digest| self.get_transaction_effects(digest))
            .collect()
    }

    fn get_events(&self, event_digest: &TransactionEventsDigest) -> Option<TransactionEvents>;

    fn multi_get_events(
        &self,
        event_digests: &[TransactionEventsDigest],
    ) -> Vec<Option<TransactionEvents>> {
        event_digests
            .iter()
            .map(|digest| self.get_events(digest))
            .collect()
    }

    //
    // Extra Checkpoint fetching apis
    //

    /// Get a "full" checkpoint for purposes of state-sync
    /// "full" checkpoints include: header, contents, transactions, effects
    fn get_full_checkpoint_contents_by_sequence_number(
        &self,
        sequence_number: CheckpointSequenceNumber,
    ) -> Option<FullCheckpointContents>;

    /// Get a "full" checkpoint for purposes of state-sync
    /// "full" checkpoints include: header, contents, transactions, effects
    fn get_full_checkpoint_contents(
        &self,
        digest: &CheckpointContentsDigest,
    ) -> Option<FullCheckpointContents>;

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
        use std::collections::HashMap;

        let transaction_digests = checkpoint_contents
            .iter()
            .map(|execution_digests| execution_digests.transaction)
            .collect::<Vec<_>>();
        let transactions = self
            .multi_get_transactions(&transaction_digests)
            .into_iter()
            .map(|maybe_transaction| {
                maybe_transaction.ok_or_else(|| anyhow::anyhow!("missing transaction"))
            })
            .collect::<anyhow::Result<Vec<_>>>()?;

        let effects = self
            .multi_get_transaction_effects(&transaction_digests)
            .into_iter()
            .map(|maybe_effects| maybe_effects.ok_or_else(|| anyhow::anyhow!("missing effects")))
            .collect::<anyhow::Result<Vec<_>>>()?;

        let event_digests = effects
            .iter()
            .flat_map(|fx| fx.events_digest().copied())
            .collect::<Vec<_>>();

        let events = self
            .multi_get_events(&event_digests)
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

            let input_object_keys = fx
                .modified_at_versions()
                .into_iter()
                .map(|(object_id, version)| ObjectKey(object_id, version))
                .collect::<Vec<_>>();

            let input_objects = self
                .multi_get_objects_by_key(&input_object_keys)
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
                .multi_get_objects_by_key(&output_object_keys)
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
    fn get_committee(&self, epoch: EpochId) -> Option<Arc<Committee>> {
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

    fn get_checkpoint_by_digest(&self, digest: &CheckpointDigest) -> Option<VerifiedCheckpoint> {
        (*self).get_checkpoint_by_digest(digest)
    }

    fn get_checkpoint_by_sequence_number(
        &self,
        sequence_number: CheckpointSequenceNumber,
    ) -> Option<VerifiedCheckpoint> {
        (*self).get_checkpoint_by_sequence_number(sequence_number)
    }

    fn get_checkpoint_contents_by_digest(
        &self,
        digest: &CheckpointContentsDigest,
    ) -> Option<CheckpointContents> {
        (*self).get_checkpoint_contents_by_digest(digest)
    }

    fn get_checkpoint_contents_by_sequence_number(
        &self,
        sequence_number: CheckpointSequenceNumber,
    ) -> Option<CheckpointContents> {
        (*self).get_checkpoint_contents_by_sequence_number(sequence_number)
    }

    fn get_transaction(&self, tx_digest: &TransactionDigest) -> Option<Arc<VerifiedTransaction>> {
        (*self).get_transaction(tx_digest)
    }

    fn multi_get_transactions(
        &self,
        tx_digests: &[TransactionDigest],
    ) -> Vec<Option<Arc<VerifiedTransaction>>> {
        (*self).multi_get_transactions(tx_digests)
    }

    fn get_transaction_effects(&self, tx_digest: &TransactionDigest) -> Option<TransactionEffects> {
        (*self).get_transaction_effects(tx_digest)
    }

    fn multi_get_transaction_effects(
        &self,
        tx_digests: &[TransactionDigest],
    ) -> Vec<Option<TransactionEffects>> {
        (*self).multi_get_transaction_effects(tx_digests)
    }

    fn get_events(&self, event_digest: &TransactionEventsDigest) -> Option<TransactionEvents> {
        (*self).get_events(event_digest)
    }

    fn multi_get_events(
        &self,
        event_digests: &[TransactionEventsDigest],
    ) -> Vec<Option<TransactionEvents>> {
        (*self).multi_get_events(event_digests)
    }

    fn get_full_checkpoint_contents_by_sequence_number(
        &self,
        sequence_number: CheckpointSequenceNumber,
    ) -> Option<FullCheckpointContents> {
        (*self).get_full_checkpoint_contents_by_sequence_number(sequence_number)
    }

    fn get_full_checkpoint_contents(
        &self,
        digest: &CheckpointContentsDigest,
    ) -> Option<FullCheckpointContents> {
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
    fn get_committee(&self, epoch: EpochId) -> Option<Arc<Committee>> {
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

    fn get_checkpoint_by_digest(&self, digest: &CheckpointDigest) -> Option<VerifiedCheckpoint> {
        (**self).get_checkpoint_by_digest(digest)
    }

    fn get_checkpoint_by_sequence_number(
        &self,
        sequence_number: CheckpointSequenceNumber,
    ) -> Option<VerifiedCheckpoint> {
        (**self).get_checkpoint_by_sequence_number(sequence_number)
    }

    fn get_checkpoint_contents_by_digest(
        &self,
        digest: &CheckpointContentsDigest,
    ) -> Option<CheckpointContents> {
        (**self).get_checkpoint_contents_by_digest(digest)
    }

    fn get_checkpoint_contents_by_sequence_number(
        &self,
        sequence_number: CheckpointSequenceNumber,
    ) -> Option<CheckpointContents> {
        (**self).get_checkpoint_contents_by_sequence_number(sequence_number)
    }

    fn get_transaction(&self, tx_digest: &TransactionDigest) -> Option<Arc<VerifiedTransaction>> {
        (**self).get_transaction(tx_digest)
    }

    fn multi_get_transactions(
        &self,
        tx_digests: &[TransactionDigest],
    ) -> Vec<Option<Arc<VerifiedTransaction>>> {
        (**self).multi_get_transactions(tx_digests)
    }

    fn get_transaction_effects(&self, tx_digest: &TransactionDigest) -> Option<TransactionEffects> {
        (**self).get_transaction_effects(tx_digest)
    }

    fn multi_get_transaction_effects(
        &self,
        tx_digests: &[TransactionDigest],
    ) -> Vec<Option<TransactionEffects>> {
        (**self).multi_get_transaction_effects(tx_digests)
    }

    fn get_events(&self, event_digest: &TransactionEventsDigest) -> Option<TransactionEvents> {
        (**self).get_events(event_digest)
    }

    fn multi_get_events(
        &self,
        event_digests: &[TransactionEventsDigest],
    ) -> Vec<Option<TransactionEvents>> {
        (**self).multi_get_events(event_digests)
    }

    fn get_full_checkpoint_contents_by_sequence_number(
        &self,
        sequence_number: CheckpointSequenceNumber,
    ) -> Option<FullCheckpointContents> {
        (**self).get_full_checkpoint_contents_by_sequence_number(sequence_number)
    }

    fn get_full_checkpoint_contents(
        &self,
        digest: &CheckpointContentsDigest,
    ) -> Option<FullCheckpointContents> {
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
    fn get_committee(&self, epoch: EpochId) -> Option<Arc<Committee>> {
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

    fn get_checkpoint_by_digest(&self, digest: &CheckpointDigest) -> Option<VerifiedCheckpoint> {
        (**self).get_checkpoint_by_digest(digest)
    }

    fn get_checkpoint_by_sequence_number(
        &self,
        sequence_number: CheckpointSequenceNumber,
    ) -> Option<VerifiedCheckpoint> {
        (**self).get_checkpoint_by_sequence_number(sequence_number)
    }

    fn get_checkpoint_contents_by_digest(
        &self,
        digest: &CheckpointContentsDigest,
    ) -> Option<CheckpointContents> {
        (**self).get_checkpoint_contents_by_digest(digest)
    }

    fn get_checkpoint_contents_by_sequence_number(
        &self,
        sequence_number: CheckpointSequenceNumber,
    ) -> Option<CheckpointContents> {
        (**self).get_checkpoint_contents_by_sequence_number(sequence_number)
    }

    fn get_transaction(&self, tx_digest: &TransactionDigest) -> Option<Arc<VerifiedTransaction>> {
        (**self).get_transaction(tx_digest)
    }

    fn multi_get_transactions(
        &self,
        tx_digests: &[TransactionDigest],
    ) -> Vec<Option<Arc<VerifiedTransaction>>> {
        (**self).multi_get_transactions(tx_digests)
    }

    fn get_transaction_effects(&self, tx_digest: &TransactionDigest) -> Option<TransactionEffects> {
        (**self).get_transaction_effects(tx_digest)
    }

    fn multi_get_transaction_effects(
        &self,
        tx_digests: &[TransactionDigest],
    ) -> Vec<Option<TransactionEffects>> {
        (**self).multi_get_transaction_effects(tx_digests)
    }

    fn get_events(&self, event_digest: &TransactionEventsDigest) -> Option<TransactionEvents> {
        (**self).get_events(event_digest)
    }

    fn multi_get_events(
        &self,
        event_digests: &[TransactionEventsDigest],
    ) -> Vec<Option<TransactionEvents>> {
        (**self).multi_get_events(event_digests)
    }

    fn get_full_checkpoint_contents_by_sequence_number(
        &self,
        sequence_number: CheckpointSequenceNumber,
    ) -> Option<FullCheckpointContents> {
        (**self).get_full_checkpoint_contents_by_sequence_number(sequence_number)
    }

    fn get_full_checkpoint_contents(
        &self,
        digest: &CheckpointContentsDigest,
    ) -> Option<FullCheckpointContents> {
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

/// Trait used to provide functionality to the REST API service.
///
/// It extends both ObjectStore and ReadStore by adding functionality that may require more
/// detailed underlying databases or indexes to support.
pub trait RpcStateReader: ObjectStore + ReadStore + Send + Sync {
    /// Lowest available checkpoint for which object data can be requested.
    ///
    /// Specifically this is the lowest checkpoint for which input/output object data will be
    /// available.
    fn get_lowest_available_checkpoint_objects(&self) -> Result<CheckpointSequenceNumber>;

    fn get_chain_identifier(&self) -> Result<ChainIdentifier>;

    // Get a handle to an instance of the RpcIndexes
    fn indexes(&self) -> Option<&dyn RpcIndexes>;
}

pub trait RpcIndexes: Send + Sync {
    fn get_transaction_checkpoint(
        &self,
        digest: &TransactionDigest,
    ) -> Result<Option<CheckpointSequenceNumber>>;

    fn account_owned_objects_info_iter(
        &self,
        owner: SuiAddress,
        cursor: Option<ObjectID>,
    ) -> Result<Box<dyn Iterator<Item = AccountOwnedObjectInfo> + '_>>;

    fn dynamic_field_iter(
        &self,
        parent: ObjectID,
        cursor: Option<ObjectID>,
    ) -> Result<Box<dyn Iterator<Item = (DynamicFieldKey, DynamicFieldIndexInfo)> + '_>>;

    fn get_coin_info(&self, coin_type: &StructTag) -> Result<Option<CoinInfo>>;
}

pub struct AccountOwnedObjectInfo {
    pub owner: SuiAddress,
    pub object_id: ObjectID,
    pub version: SequenceNumber,
    pub type_: MoveObjectType,
}

#[derive(Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub struct DynamicFieldKey {
    pub parent: ObjectID,
    pub field_id: ObjectID,
}

impl DynamicFieldKey {
    pub fn new<P: Into<ObjectID>>(parent: P, field_id: ObjectID) -> Self {
        Self {
            parent: parent.into(),
            field_id,
        }
    }
}

#[derive(Clone, Serialize, Deserialize, PartialEq, Eq, Debug)]
pub struct DynamicFieldIndexInfo {
    // field_id of this dynamic field is a part of the Key
    pub dynamic_field_type: DynamicFieldType,
    pub name_type: TypeTag,
    pub name_value: Vec<u8>,
    // TODO do we want to also store the type of the value? We can get this for free for
    // DynamicFields, but for DynamicObjects it would require a lookup in the DB on init, or
    // scanning the transaction's output objects for the coorisponding Object to retrieve its type
    // information.
    //
    // pub value_type: TypeTag,
    /// ObjectId of the child object when `dynamic_field_type == DynamicFieldType::DynamicObject`
    pub dynamic_object_id: Option<ObjectID>,
}

#[derive(Clone, Serialize, Deserialize, PartialEq, Eq, Debug)]
pub struct CoinInfo {
    pub coin_metadata_object_id: Option<ObjectID>,
    pub treasury_object_id: Option<ObjectID>,
}
