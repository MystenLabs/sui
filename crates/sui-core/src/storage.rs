// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

use sui_types::base_types::TransactionDigest;
use sui_types::base_types::TransactionEffectsDigest;
use sui_types::committee::Committee;
use sui_types::committee::EpochId;
use sui_types::message_envelope::Message;
use sui_types::messages::TransactionEffects;
use sui_types::messages::VerifiedCertificate;
use sui_types::messages_checkpoint::CheckpointContents;
use sui_types::messages_checkpoint::CheckpointContentsDigest;
use sui_types::messages_checkpoint::CheckpointDigest;
use sui_types::messages_checkpoint::CheckpointSequenceNumber;
use sui_types::messages_checkpoint::VerifiedCheckpoint;
use sui_types::storage::ReadStore;
use sui_types::storage::WriteStore;
use typed_store::Map;

use crate::authority::AuthorityStore;
use crate::checkpoints::CheckpointStoreTables;
use crate::epoch::committee_store::CommitteeStore;

#[derive(Clone)]
pub struct RocksDbStore {
    authority_store: Arc<AuthorityStore>,
    committee_store: Arc<CommitteeStore>,
    checkpoint_store: Arc<CheckpointStoreTables>,
}

impl RocksDbStore {
    pub fn new(
        authority_store: Arc<AuthorityStore>,
        committee_store: Arc<CommitteeStore>,
        checkpoint_store: Arc<CheckpointStoreTables>,
    ) -> Self {
        Self {
            authority_store,
            committee_store,
            checkpoint_store,
        }
    }
}

impl ReadStore for RocksDbStore {
    fn get_checkpoint_by_digest(&self, digest: &CheckpointDigest) -> Option<VerifiedCheckpoint> {
        self.checkpoint_store.get_checkpoint_by_digest(digest)
    }

    fn get_checkpoint_by_sequence_number(
        &self,
        sequence_number: CheckpointSequenceNumber,
    ) -> Option<VerifiedCheckpoint> {
        self.checkpoint_store
            .get_checkpoint_by_sequence_number(sequence_number)
    }

    fn get_highest_verified_checkpoint(&self) -> Option<VerifiedCheckpoint> {
        self.checkpoint_store.get_highest_verified_checkpoint()
    }

    fn get_highest_synced_checkpoint(&self) -> Option<VerifiedCheckpoint> {
        self.checkpoint_store.get_highest_synced_checkpoint()
    }

    fn get_checkpoint_contents(
        &self,
        digest: &CheckpointContentsDigest,
    ) -> Option<CheckpointContents> {
        self.checkpoint_store.get_checkpoint_contents(digest)
    }

    fn get_committee(&self, epoch: EpochId) -> Option<Committee> {
        self.committee_store.get_committee(&epoch).unwrap()
    }

    fn get_transaction(&self, digest: &TransactionDigest) -> Option<VerifiedCertificate> {
        if let Some(transaction) = self
            .authority_store
            .epoch_tables()
            .pending_certificates
            .get(digest)
            .ok()
            .flatten()
        {
            return Some(transaction.into());
        }

        if let Some(transaction) = self
            .authority_store
            .perpetual_tables
            .certificates
            .get(digest)
            .ok()
            .flatten()
        {
            return Some(transaction.into());
        }

        None
    }

    fn get_transaction_effects(
        &self,
        digest: &TransactionEffectsDigest,
    ) -> Option<TransactionEffects> {
        self.authority_store
            .perpetual_tables
            .effects2
            .get(digest)
            .ok()
            .flatten()
    }
}

impl WriteStore for RocksDbStore {
    fn insert_checkpoint(&self, checkpoint: VerifiedCheckpoint) {
        if let Some(next_committee) = checkpoint.next_epoch_committee() {
            let next_committee = next_committee.iter().cloned().collect();
            let committee = Committee::new(checkpoint.epoch().saturating_add(1), next_committee)
                .expect("new committee from consensus should be constructable");
            self.insert_committee(committee);
        }

        self.checkpoint_store.insert_verified_checkpoint(checkpoint)
    }

    fn update_highest_synced_checkpoint(&self, checkpoint: &VerifiedCheckpoint) {
        self.checkpoint_store
            .update_highest_synced_checkpoint(checkpoint)
    }

    fn insert_checkpoint_contents(&self, contents: CheckpointContents) {
        self.checkpoint_store.insert_checkpoint_contents(contents)
    }

    fn insert_committee(&self, new_committee: Committee) {
        self.committee_store
            .insert_new_committee(&new_committee)
            .unwrap()
    }

    fn insert_transaction(&self, transaction: VerifiedCertificate) {
        self.authority_store
            .epoch_tables()
            .pending_certificates
            .insert(transaction.digest(), transaction.serializable_ref())
            .expect("writing to rocksdb should not fail")
    }

    fn insert_transaction_effects(&self, transaction_effects: TransactionEffects) {
        self.authority_store
            .perpetual_tables
            .effects2
            .insert(&transaction_effects.digest(), &transaction_effects)
            .expect("writing to rocksdb should not fail")
    }
}
