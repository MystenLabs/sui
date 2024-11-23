// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use super::error::Result;
use super::ObjectStore;
use crate::base_types::{EpochId, TransactionDigest};
use crate::committee::Committee;
use crate::digests::{CheckpointContentsDigest, CheckpointDigest, TransactionEventsDigest};
use crate::effects::{TransactionEffects, TransactionEvents};
use crate::messages_checkpoint::{
    CheckpointContents, CheckpointSequenceNumber, FullCheckpointContents, VerifiedCheckpoint,
    VerifiedCheckpointContents,
};
use crate::storage::{ReadStore, WriteStore};
use crate::transaction::VerifiedTransaction;
use std::collections::HashMap;
use std::sync::Arc;
use tap::Pipe;
use tracing::error;

#[derive(Clone, Debug, Default)]
pub struct SharedInMemoryStore(Arc<std::sync::RwLock<InMemoryStore>>);

impl SharedInMemoryStore {
    pub fn inner(&self) -> std::sync::RwLockReadGuard<'_, InMemoryStore> {
        self.0.read().unwrap()
    }

    pub fn inner_mut(&self) -> std::sync::RwLockWriteGuard<'_, InMemoryStore> {
        self.0.write().unwrap()
    }
}

impl ReadStore for SharedInMemoryStore {
    fn get_checkpoint_by_digest(&self, digest: &CheckpointDigest) -> Option<VerifiedCheckpoint> {
        self.inner().get_checkpoint_by_digest(digest).cloned()
    }

    fn get_checkpoint_by_sequence_number(
        &self,
        sequence_number: CheckpointSequenceNumber,
    ) -> Option<VerifiedCheckpoint> {
        self.inner()
            .get_checkpoint_by_sequence_number(sequence_number)
            .cloned()
    }

    fn get_highest_verified_checkpoint(&self) -> Result<VerifiedCheckpoint> {
        self.inner()
            .get_highest_verified_checkpoint()
            .cloned()
            .expect("storage should have been initialized with genesis checkpoint")
            .pipe(Ok)
    }

    fn get_highest_synced_checkpoint(&self) -> Result<VerifiedCheckpoint> {
        self.inner()
            .get_highest_synced_checkpoint()
            .cloned()
            .expect("storage should have been initialized with genesis checkpoint")
            .pipe(Ok)
    }

    fn get_lowest_available_checkpoint(&self) -> Result<CheckpointSequenceNumber> {
        Ok(self.inner().get_lowest_available_checkpoint())
    }

    fn get_full_checkpoint_contents_by_sequence_number(
        &self,
        sequence_number: CheckpointSequenceNumber,
    ) -> Option<FullCheckpointContents> {
        self.inner()
            .full_checkpoint_contents
            .get(&sequence_number)
            .cloned()
    }

    fn get_full_checkpoint_contents(
        &self,
        digest: &CheckpointContentsDigest,
    ) -> Option<FullCheckpointContents> {
        // First look to see if we saved the complete contents already.
        let inner = self.inner();
        let contents = inner
            .get_sequence_number_by_contents_digest(digest)
            .and_then(|seq_num| inner.full_checkpoint_contents.get(&seq_num).cloned());
        if contents.is_some() {
            return contents;
        }

        // Otherwise gather it from the individual components.
        inner.get_checkpoint_contents(digest).and_then(|contents| {
            FullCheckpointContents::from_checkpoint_contents(self, contents.to_owned())
        })
    }

    fn get_committee(&self, epoch: EpochId) -> Option<Arc<Committee>> {
        self.inner()
            .get_committee_by_epoch(epoch)
            .cloned()
            .map(Arc::new)
    }

    fn get_transaction(&self, digest: &TransactionDigest) -> Option<Arc<VerifiedTransaction>> {
        self.inner()
            .get_transaction_block(digest)
            .map(|tx| Arc::new(tx.clone()))
    }

    fn get_transaction_effects(&self, digest: &TransactionDigest) -> Option<TransactionEffects> {
        self.inner().get_transaction_effects(digest).cloned()
    }

    fn get_events(&self, digest: &TransactionEventsDigest) -> Option<TransactionEvents> {
        self.inner().get_transaction_events(digest).cloned()
    }

    fn get_latest_checkpoint(&self) -> Result<VerifiedCheckpoint> {
        todo!()
    }

    fn get_checkpoint_contents_by_digest(
        &self,
        _digest: &CheckpointContentsDigest,
    ) -> Option<CheckpointContents> {
        todo!()
    }

    fn get_checkpoint_contents_by_sequence_number(
        &self,
        _sequence_number: CheckpointSequenceNumber,
    ) -> Option<CheckpointContents> {
        todo!()
    }
}

impl ObjectStore for SharedInMemoryStore {
    fn get_object(
        &self,
        _object_id: &crate::base_types::ObjectID,
    ) -> Option<crate::object::Object> {
        todo!()
    }

    fn get_object_by_key(
        &self,
        _object_id: &crate::base_types::ObjectID,
        _version: crate::base_types::VersionNumber,
    ) -> Option<crate::object::Object> {
        todo!()
    }
}

impl WriteStore for SharedInMemoryStore {
    fn insert_checkpoint(&self, checkpoint: &VerifiedCheckpoint) -> Result<()> {
        self.inner_mut().insert_checkpoint(checkpoint);
        Ok(())
    }

    fn update_highest_synced_checkpoint(&self, checkpoint: &VerifiedCheckpoint) -> Result<()> {
        self.inner_mut()
            .update_highest_synced_checkpoint(checkpoint);
        Ok(())
    }

    fn update_highest_verified_checkpoint(&self, checkpoint: &VerifiedCheckpoint) -> Result<()> {
        self.inner_mut()
            .update_highest_verified_checkpoint(checkpoint);
        Ok(())
    }

    fn insert_checkpoint_contents(
        &self,
        checkpoint: &VerifiedCheckpoint,
        contents: VerifiedCheckpointContents,
    ) -> Result<()> {
        self.inner_mut()
            .insert_checkpoint_contents(checkpoint, contents);
        Ok(())
    }

    fn insert_committee(&self, new_committee: Committee) -> Result<()> {
        self.inner_mut().insert_committee(new_committee);
        Ok(())
    }
}

impl SharedInMemoryStore {
    pub fn insert_certified_checkpoint(&self, checkpoint: &VerifiedCheckpoint) {
        self.inner_mut().insert_certified_checkpoint(checkpoint);
    }
}

#[derive(Debug, Default)]
pub struct InMemoryStore {
    highest_verified_checkpoint: Option<(CheckpointSequenceNumber, CheckpointDigest)>,
    highest_synced_checkpoint: Option<(CheckpointSequenceNumber, CheckpointDigest)>,
    checkpoints: HashMap<CheckpointDigest, VerifiedCheckpoint>,
    full_checkpoint_contents: HashMap<CheckpointSequenceNumber, FullCheckpointContents>,
    contents_digest_to_sequence_number: HashMap<CheckpointContentsDigest, CheckpointSequenceNumber>,
    sequence_number_to_digest: HashMap<CheckpointSequenceNumber, CheckpointDigest>,
    checkpoint_contents: HashMap<CheckpointContentsDigest, CheckpointContents>,
    transactions: HashMap<TransactionDigest, VerifiedTransaction>,
    effects: HashMap<TransactionDigest, TransactionEffects>,
    events: HashMap<TransactionEventsDigest, TransactionEvents>,

    epoch_to_committee: Vec<Committee>,

    lowest_checkpoint_number: CheckpointSequenceNumber,
}

impl InMemoryStore {
    pub fn insert_genesis_state(
        &mut self,
        checkpoint: VerifiedCheckpoint,
        contents: VerifiedCheckpointContents,
        committee: Committee,
    ) {
        self.insert_committee(committee);
        self.insert_checkpoint(&checkpoint);
        self.insert_checkpoint_contents(&checkpoint, contents);
        self.update_highest_synced_checkpoint(&checkpoint);
    }

    pub fn get_checkpoint_by_digest(
        &self,
        digest: &CheckpointDigest,
    ) -> Option<&VerifiedCheckpoint> {
        self.checkpoints.get(digest)
    }

    pub fn get_checkpoint_by_sequence_number(
        &self,
        sequence_number: CheckpointSequenceNumber,
    ) -> Option<&VerifiedCheckpoint> {
        self.sequence_number_to_digest
            .get(&sequence_number)
            .and_then(|digest| self.get_checkpoint_by_digest(digest))
    }

    pub fn get_sequence_number_by_contents_digest(
        &self,
        digest: &CheckpointContentsDigest,
    ) -> Option<CheckpointSequenceNumber> {
        self.contents_digest_to_sequence_number.get(digest).copied()
    }

    pub fn get_highest_verified_checkpoint(&self) -> Option<&VerifiedCheckpoint> {
        self.highest_verified_checkpoint
            .as_ref()
            .and_then(|(_, digest)| self.get_checkpoint_by_digest(digest))
    }

    pub fn get_highest_synced_checkpoint(&self) -> Option<&VerifiedCheckpoint> {
        self.highest_synced_checkpoint
            .as_ref()
            .and_then(|(_, digest)| self.get_checkpoint_by_digest(digest))
    }

    pub fn get_lowest_available_checkpoint(&self) -> CheckpointSequenceNumber {
        self.lowest_checkpoint_number
    }

    pub fn set_lowest_available_checkpoint(
        &mut self,
        checkpoint_seq_num: CheckpointSequenceNumber,
    ) {
        self.lowest_checkpoint_number = checkpoint_seq_num;
    }

    pub fn get_checkpoint_contents(
        &self,
        digest: &CheckpointContentsDigest,
    ) -> Option<&CheckpointContents> {
        self.checkpoint_contents.get(digest)
    }

    pub fn insert_checkpoint_contents(
        &mut self,
        checkpoint: &VerifiedCheckpoint,
        contents: VerifiedCheckpointContents,
    ) {
        for tx in contents.iter() {
            self.transactions
                .insert(*tx.transaction.digest(), tx.transaction.to_owned());
            self.effects
                .insert(*tx.transaction.digest(), tx.effects.to_owned());
        }
        self.contents_digest_to_sequence_number
            .insert(checkpoint.content_digest, *checkpoint.sequence_number());
        let contents = contents.into_inner();
        self.full_checkpoint_contents
            .insert(*checkpoint.sequence_number(), contents.clone());
        let contents = contents.into_checkpoint_contents();
        self.checkpoint_contents
            .insert(*contents.digest(), contents);
    }

    pub fn insert_checkpoint(&mut self, checkpoint: &VerifiedCheckpoint) {
        self.insert_certified_checkpoint(checkpoint);
        let digest = *checkpoint.digest();
        let sequence_number = *checkpoint.sequence_number();

        if Some(sequence_number) > self.highest_verified_checkpoint.map(|x| x.0) {
            self.highest_verified_checkpoint = Some((sequence_number, digest));
        }
    }

    // This function simulates Consensus inserts certified checkpoint into the checkpoint store
    // without bumping the highest_verified_checkpoint watermark.
    pub fn insert_certified_checkpoint(&mut self, checkpoint: &VerifiedCheckpoint) {
        let digest = *checkpoint.digest();
        let sequence_number = *checkpoint.sequence_number();

        if let Some(end_of_epoch_data) = &checkpoint.data().end_of_epoch_data {
            let next_committee = end_of_epoch_data
                .next_epoch_committee
                .iter()
                .cloned()
                .collect();
            let committee =
                Committee::new(checkpoint.epoch().checked_add(1).unwrap(), next_committee);
            self.insert_committee(committee);
        }

        self.checkpoints.insert(digest, checkpoint.clone());
        self.sequence_number_to_digest
            .insert(sequence_number, digest);
    }

    pub fn delete_checkpoint_content_test_only(
        &mut self,
        sequence_number: u64,
    ) -> anyhow::Result<()> {
        let contents = self
            .full_checkpoint_contents
            .get(&sequence_number)
            .unwrap()
            .clone();
        let contents_digest = *contents.checkpoint_contents().digest();
        for content in contents.iter() {
            let tx_digest = content.transaction.digest();
            self.effects.remove(tx_digest);
            self.transactions.remove(tx_digest);
        }
        self.checkpoint_contents.remove(&contents_digest);
        self.full_checkpoint_contents.remove(&sequence_number);
        self.contents_digest_to_sequence_number
            .remove(&contents_digest);
        self.lowest_checkpoint_number = sequence_number + 1;
        Ok(())
    }

    pub fn update_highest_synced_checkpoint(&mut self, checkpoint: &VerifiedCheckpoint) {
        if !self.checkpoints.contains_key(checkpoint.digest()) {
            panic!("store should already contain checkpoint");
        }
        if let Some(highest_synced_checkpoint) = self.highest_synced_checkpoint {
            if highest_synced_checkpoint.0 >= checkpoint.sequence_number {
                return;
            }
        }
        self.highest_synced_checkpoint =
            Some((*checkpoint.sequence_number(), *checkpoint.digest()));
    }

    pub fn update_highest_verified_checkpoint(&mut self, checkpoint: &VerifiedCheckpoint) {
        if !self.checkpoints.contains_key(checkpoint.digest()) {
            panic!("store should already contain checkpoint");
        }
        if let Some(highest_verified_checkpoint) = self.highest_verified_checkpoint {
            if highest_verified_checkpoint.0 >= checkpoint.sequence_number {
                return;
            }
        }
        self.highest_verified_checkpoint =
            Some((*checkpoint.sequence_number(), *checkpoint.digest()));
    }

    pub fn checkpoints(&self) -> &HashMap<CheckpointDigest, VerifiedCheckpoint> {
        &self.checkpoints
    }

    pub fn checkpoint_sequence_number_to_digest(
        &self,
    ) -> &HashMap<CheckpointSequenceNumber, CheckpointDigest> {
        &self.sequence_number_to_digest
    }

    pub fn get_committee_by_epoch(&self, epoch: EpochId) -> Option<&Committee> {
        self.epoch_to_committee.get(epoch as usize)
    }

    pub fn insert_committee(&mut self, committee: Committee) {
        let epoch = committee.epoch as usize;

        if self.epoch_to_committee.get(epoch).is_some() {
            return;
        }

        self.epoch_to_committee.push(committee);

        if self.epoch_to_committee.len() != epoch + 1 {
            error!("committee was inserted into EpochCommitteeMap out of order");
        }
    }

    pub fn get_transaction_block(
        &self,
        digest: &TransactionDigest,
    ) -> Option<&VerifiedTransaction> {
        self.transactions.get(digest)
    }

    pub fn get_transaction_effects(
        &self,
        digest: &TransactionDigest,
    ) -> Option<&TransactionEffects> {
        self.effects.get(digest)
    }

    pub fn get_transaction_events(
        &self,
        digest: &TransactionEventsDigest,
    ) -> Option<&TransactionEvents> {
        self.events.get(digest)
    }
}

// This store only keeps last checkpoint in memory which is all we need
// for archive verification.
#[derive(Clone, Debug, Default)]
pub struct SingleCheckpointSharedInMemoryStore(SharedInMemoryStore);

impl SingleCheckpointSharedInMemoryStore {
    pub fn insert_genesis_state(
        &mut self,
        checkpoint: VerifiedCheckpoint,
        contents: VerifiedCheckpointContents,
        committee: Committee,
    ) {
        let mut locked = self.0 .0.write().unwrap();
        locked.insert_genesis_state(checkpoint, contents, committee);
    }
}

impl ObjectStore for SingleCheckpointSharedInMemoryStore {
    fn get_object(
        &self,
        _object_id: &crate::base_types::ObjectID,
    ) -> Option<crate::object::Object> {
        todo!()
    }

    fn get_object_by_key(
        &self,
        _object_id: &crate::base_types::ObjectID,
        _version: crate::base_types::VersionNumber,
    ) -> Option<crate::object::Object> {
        todo!()
    }
}

impl ReadStore for SingleCheckpointSharedInMemoryStore {
    fn get_checkpoint_by_digest(&self, digest: &CheckpointDigest) -> Option<VerifiedCheckpoint> {
        self.0.get_checkpoint_by_digest(digest)
    }

    fn get_checkpoint_by_sequence_number(
        &self,
        sequence_number: CheckpointSequenceNumber,
    ) -> Option<VerifiedCheckpoint> {
        self.0.get_checkpoint_by_sequence_number(sequence_number)
    }

    fn get_highest_verified_checkpoint(&self) -> Result<VerifiedCheckpoint> {
        self.0.get_highest_verified_checkpoint()
    }

    fn get_highest_synced_checkpoint(&self) -> Result<VerifiedCheckpoint> {
        self.0.get_highest_synced_checkpoint()
    }

    fn get_lowest_available_checkpoint(&self) -> Result<CheckpointSequenceNumber> {
        self.0.get_lowest_available_checkpoint()
    }

    fn get_full_checkpoint_contents_by_sequence_number(
        &self,
        sequence_number: CheckpointSequenceNumber,
    ) -> Option<FullCheckpointContents> {
        self.0
            .get_full_checkpoint_contents_by_sequence_number(sequence_number)
    }

    fn get_full_checkpoint_contents(
        &self,
        digest: &CheckpointContentsDigest,
    ) -> Option<FullCheckpointContents> {
        self.0.get_full_checkpoint_contents(digest)
    }

    fn get_committee(&self, epoch: EpochId) -> Option<Arc<Committee>> {
        self.0.get_committee(epoch)
    }

    fn get_transaction(&self, digest: &TransactionDigest) -> Option<Arc<VerifiedTransaction>> {
        self.0.get_transaction(digest)
    }

    fn get_transaction_effects(&self, digest: &TransactionDigest) -> Option<TransactionEffects> {
        self.0.get_transaction_effects(digest)
    }

    fn get_events(&self, digest: &TransactionEventsDigest) -> Option<TransactionEvents> {
        self.0.get_events(digest)
    }

    fn get_latest_checkpoint(&self) -> Result<VerifiedCheckpoint> {
        todo!()
    }

    fn get_checkpoint_contents_by_digest(
        &self,
        _digest: &CheckpointContentsDigest,
    ) -> Option<CheckpointContents> {
        todo!()
    }

    fn get_checkpoint_contents_by_sequence_number(
        &self,
        _sequence_number: CheckpointSequenceNumber,
    ) -> Option<CheckpointContents> {
        todo!()
    }
}

impl WriteStore for SingleCheckpointSharedInMemoryStore {
    fn insert_checkpoint(&self, checkpoint: &VerifiedCheckpoint) -> Result<()> {
        {
            let mut locked = self.0 .0.write().unwrap();
            locked.checkpoints.clear();
            locked.sequence_number_to_digest.clear();
        }
        self.0.insert_checkpoint(checkpoint)?;
        Ok(())
    }

    fn update_highest_synced_checkpoint(&self, checkpoint: &VerifiedCheckpoint) -> Result<()> {
        self.0.update_highest_synced_checkpoint(checkpoint)?;
        Ok(())
    }

    fn update_highest_verified_checkpoint(&self, checkpoint: &VerifiedCheckpoint) -> Result<()> {
        self.0.update_highest_verified_checkpoint(checkpoint)?;
        Ok(())
    }

    fn insert_checkpoint_contents(
        &self,
        checkpoint: &VerifiedCheckpoint,
        contents: VerifiedCheckpointContents,
    ) -> Result<()> {
        {
            let mut locked = self.0 .0.write().unwrap();
            locked.transactions.clear();
            locked.effects.clear();
            locked.contents_digest_to_sequence_number.clear();
            locked.full_checkpoint_contents.clear();
            locked.checkpoint_contents.clear();
        }
        self.0.insert_checkpoint_contents(checkpoint, contents)?;
        Ok(())
    }

    fn insert_committee(&self, new_committee: Committee) -> Result<()> {
        self.0.insert_committee(new_committee)
    }
}
