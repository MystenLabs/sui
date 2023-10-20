// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeMap;
use sui_types::{
    base_types::{ObjectID, SequenceNumber, SuiAddress},
    committee::{Committee, EpochId},
    digests::{ObjectDigest, TransactionDigest, TransactionEventsDigest},
    effects::{TransactionEffects, TransactionEvents},
    messages_checkpoint::{
        CheckpointContents, CheckpointContentsDigest, CheckpointDigest, CheckpointSequenceNumber,
        VerifiedCheckpoint,
    },
    object::Object,
    transaction::VerifiedTransaction,
};

pub mod in_mem_store;
pub mod persisted_store;

#[async_trait::async_trait]
pub trait SimulatorStore:
    sui_types::storage::BackingPackageStore
    + sui_types::storage::ObjectStore
    + sui_types::storage::ReceivedMarkerQuery
{
    async fn get_checkpoint_by_sequence_number(
        &self,
        sequence_number: CheckpointSequenceNumber,
    ) -> Option<&VerifiedCheckpoint>;

    async fn get_checkpoint_by_digest(
        &self,
        digest: &CheckpointDigest,
    ) -> Option<&VerifiedCheckpoint>;

    async fn get_highest_checkpint(&self) -> Option<&VerifiedCheckpoint>;

    async fn get_checkpoint_contents(
        &self,
        digest: &CheckpointContentsDigest,
    ) -> Option<&CheckpointContents>;

    async fn get_committee_by_epoch(&self, epoch: EpochId) -> Option<&Committee>;

    async fn get_transaction(&self, digest: &TransactionDigest) -> Option<&VerifiedTransaction>;

    async fn get_transaction_effects(
        &self,
        digest: &TransactionDigest,
    ) -> Option<&TransactionEffects>;

    async fn get_transaction_events(
        &self,
        digest: &TransactionEventsDigest,
    ) -> Option<&TransactionEvents>;

    async fn get_object(&self, id: &ObjectID) -> Option<&Object>;

    async fn get_object_at_version(
        &self,
        id: &ObjectID,
        version: SequenceNumber,
    ) -> Option<&Object>;

    async fn get_system_state(&self) -> sui_types::sui_system_state::SuiSystemState;

    async fn get_clock(&self) -> sui_types::clock::Clock;

    async fn owned_objects(&self, owner: SuiAddress) -> Box<dyn Iterator<Item = &Object> + '_>;

    async fn insert_checkpoint(&mut self, checkpoint: VerifiedCheckpoint);

    async fn insert_checkpoint_contents(&mut self, contents: CheckpointContents);

    async fn insert_committee(&mut self, committee: Committee);

    async fn insert_executed_transaction(
        &mut self,
        transaction: VerifiedTransaction,
        effects: TransactionEffects,
        events: TransactionEvents,
        written_objects: BTreeMap<ObjectID, Object>,
    );

    async fn insert_transaction(&mut self, transaction: VerifiedTransaction);

    async fn insert_transaction_effects(&mut self, effects: TransactionEffects);

    async fn insert_events(&mut self, events: TransactionEvents);

    async fn update_objects(
        &mut self,
        written_objects: BTreeMap<ObjectID, Object>,
        deleted_objects: Vec<(ObjectID, SequenceNumber, ObjectDigest)>,
    );
}
