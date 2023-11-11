// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeMap;
use sui_config::genesis;
use sui_types::{
    base_types::{ObjectID, SequenceNumber, SuiAddress},
    committee::{Committee, EpochId},
    digests::{ObjectDigest, TransactionDigest, TransactionEventsDigest},
    effects::{TransactionEffects, TransactionEffectsAPI, TransactionEvents},
    messages_checkpoint::{
        CheckpointContents, CheckpointContentsDigest, CheckpointDigest, CheckpointSequenceNumber,
        VerifiedCheckpoint,
    },
    object::Object,
    transaction::VerifiedTransaction,
};

pub mod in_mem_store;

pub trait SimulatorStore:
    sui_types::storage::BackingPackageStore + sui_types::storage::ObjectStore
{
    fn init_with_genesis(&mut self, genesis: &genesis::Genesis) {
        self.insert_checkpoint(genesis.checkpoint());
        self.insert_checkpoint_contents(genesis.checkpoint_contents().clone());
        self.insert_committee(genesis.committee().unwrap());
        self.insert_transaction(VerifiedTransaction::new_unchecked(
            genesis.transaction().clone(),
        ));
        self.insert_transaction_effects(genesis.effects().clone());
        self.insert_events(
            genesis.effects().transaction_digest(),
            genesis.events().clone(),
        );

        self.insert_to_live_objects(genesis.objects());
    }

    fn insert_to_live_objects(&mut self, objects: &[Object]);

    fn get_checkpoint_by_sequence_number(
        &self,
        sequence_number: CheckpointSequenceNumber,
    ) -> Option<VerifiedCheckpoint>;

    fn get_checkpoint_by_digest(&self, digest: &CheckpointDigest) -> Option<VerifiedCheckpoint>;

    fn get_highest_checkpint(&self) -> Option<VerifiedCheckpoint>;

    fn get_checkpoint_contents(
        &self,
        digest: &CheckpointContentsDigest,
    ) -> Option<CheckpointContents>;

    fn get_committee_by_epoch(&self, epoch: EpochId) -> Option<Committee>;

    fn get_transaction(&self, digest: &TransactionDigest) -> Option<VerifiedTransaction>;

    fn get_transaction_effects(&self, digest: &TransactionDigest) -> Option<TransactionEffects>;

    fn get_transaction_events(&self, digest: &TransactionEventsDigest)
        -> Option<TransactionEvents>;

    fn get_transaction_events_by_tx_digest(
        &self,
        tx_digest: &TransactionDigest,
    ) -> Option<TransactionEvents>;

    fn get_object(&self, id: &ObjectID) -> Option<Object>;

    fn get_object_at_version(&self, id: &ObjectID, version: SequenceNumber) -> Option<Object>;

    fn get_system_state(&self) -> sui_types::sui_system_state::SuiSystemState;

    fn get_clock(&self) -> sui_types::clock::Clock;

    fn owned_objects(&self, owner: SuiAddress) -> Box<dyn Iterator<Item = Object> + '_>;

    fn insert_checkpoint(&mut self, checkpoint: VerifiedCheckpoint);

    fn insert_checkpoint_contents(&mut self, contents: CheckpointContents);

    fn insert_committee(&mut self, committee: Committee);

    fn insert_executed_transaction(
        &mut self,
        transaction: VerifiedTransaction,
        effects: TransactionEffects,
        events: TransactionEvents,
        written_objects: BTreeMap<ObjectID, Object>,
    );

    fn insert_transaction(&mut self, transaction: VerifiedTransaction);

    fn insert_transaction_effects(&mut self, effects: TransactionEffects);

    fn insert_events(&mut self, tx_digest: &TransactionDigest, events: TransactionEvents);

    fn update_objects(
        &mut self,
        written_objects: BTreeMap<ObjectID, Object>,
        deleted_objects: Vec<(ObjectID, SequenceNumber, ObjectDigest)>,
    );
}
