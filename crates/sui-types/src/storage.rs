// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::base_types::{SuiAddress, TransactionDigest, TransactionEffectsDigest};
use crate::committee::{Committee, EpochId};
use crate::message_envelope::Message;
use crate::messages::{Transaction, TransactionEffects};
use crate::messages_checkpoint::{
    CheckpointContents, CheckpointContentsDigest, CheckpointDigest, CheckpointSequenceNumber,
    VerifiedCheckpoint,
};
use crate::{
    base_types::{ObjectID, ObjectRef, SequenceNumber},
    error::SuiResult,
    event::Event,
    object::Object,
    SUI_FRAMEWORK_OBJECT_ID,
};
use move_core_types::ident_str;
use move_core_types::identifier::{IdentStr, Identifier};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};

#[derive(Debug, PartialEq, Eq, Clone, Copy, Serialize, Deserialize)]
pub enum WriteKind {
    /// The object was in storage already but has been modified
    Mutate,
    /// The object was created in this transaction
    Create,
    /// The object was previously wrapped in another object, but has been restored to storage
    Unwrap,
}

#[derive(Debug, PartialEq, Eq, Clone, Copy, Serialize, Deserialize)]
pub enum DeleteKind {
    /// An object is provided in the call input, and gets deleted.
    Normal,
    /// An object is not provided in the call input, but gets unwrapped
    /// from another object, and then gets deleted.
    UnwrapThenDelete,
    /// An object is provided in the call input, and gets wrapped into another object.
    Wrap,
}

pub enum ObjectChange {
    Write(SingleTxContext, Object, WriteKind),
    Delete(SingleTxContext, SequenceNumber, DeleteKind),
}

#[derive(Clone)]
pub struct SingleTxContext {
    pub package_id: ObjectID,
    pub transaction_module: Identifier,
    pub sender: SuiAddress,
}

impl SingleTxContext {
    pub fn transfer_sui(sender: SuiAddress) -> Self {
        Self::sui_transaction(ident_str!("transfer_sui"), sender)
    }
    pub fn transfer_object(sender: SuiAddress) -> Self {
        Self::sui_transaction(ident_str!("transfer_object"), sender)
    }
    pub fn gateway(sender: SuiAddress) -> Self {
        Self::sui_transaction(ident_str!("native"), sender)
    }
    pub fn pay(sender: SuiAddress) -> Self {
        Self::sui_transaction(ident_str!("pay"), sender)
    }
    pub fn pay_sui(sender: SuiAddress) -> Self {
        Self::sui_transaction(ident_str!("pay_sui"), sender)
    }
    pub fn pay_all_sui(sender: SuiAddress) -> Self {
        Self::sui_transaction(ident_str!("pay_all_sui"), sender)
    }
    pub fn unused_input(sender: SuiAddress) -> Self {
        Self::sui_transaction(ident_str!("unused_input_object"), sender)
    }
    pub fn publish(sender: SuiAddress) -> Self {
        Self::sui_transaction(ident_str!("publish"), sender)
    }
    pub fn gas(sender: SuiAddress) -> Self {
        Self::sui_transaction(ident_str!("gas"), sender)
    }
    fn sui_transaction(ident: &IdentStr, sender: SuiAddress) -> Self {
        Self {
            package_id: SUI_FRAMEWORK_OBJECT_ID,
            transaction_module: Identifier::from(ident),
            sender,
        }
    }
}

/// An abstraction of the (possibly distributed) store for objects. This
/// API only allows for the retrieval of objects, not any state changes
pub trait ChildObjectResolver {
    fn read_child_object(&self, parent: &ObjectID, child: &ObjectID) -> SuiResult<Option<Object>>;
}

/// An abstraction of the (possibly distributed) store for objects, and (soon) events and transactions
pub trait Storage {
    fn reset(&mut self);

    /// Record an event that happened during execution
    fn log_event(&mut self, event: Event);

    fn read_object(&self, id: &ObjectID) -> Option<&Object>;

    fn apply_object_changes(&mut self, changes: BTreeMap<ObjectID, ObjectChange>);
}

pub trait BackingPackageStore {
    fn get_package(&self, package_id: &ObjectID) -> SuiResult<Option<Object>>;
}

impl<S: BackingPackageStore> BackingPackageStore for std::sync::Arc<S> {
    fn get_package(&self, package_id: &ObjectID) -> SuiResult<Option<Object>> {
        BackingPackageStore::get_package(self.as_ref(), package_id)
    }
}

impl<S: BackingPackageStore> BackingPackageStore for &S {
    fn get_package(&self, package_id: &ObjectID) -> SuiResult<Option<Object>> {
        BackingPackageStore::get_package(*self, package_id)
    }
}

impl<S: BackingPackageStore> BackingPackageStore for &mut S {
    fn get_package(&self, package_id: &ObjectID) -> SuiResult<Option<Object>> {
        BackingPackageStore::get_package(*self, package_id)
    }
}

pub trait ParentSync {
    fn get_latest_parent_entry_ref(&self, object_id: ObjectID) -> SuiResult<Option<ObjectRef>>;
}

impl<S: ParentSync> ParentSync for std::sync::Arc<S> {
    fn get_latest_parent_entry_ref(&self, object_id: ObjectID) -> SuiResult<Option<ObjectRef>> {
        ParentSync::get_latest_parent_entry_ref(self.as_ref(), object_id)
    }
}

impl<S: ParentSync> ParentSync for &S {
    fn get_latest_parent_entry_ref(&self, object_id: ObjectID) -> SuiResult<Option<ObjectRef>> {
        ParentSync::get_latest_parent_entry_ref(*self, object_id)
    }
}

impl<S: ParentSync> ParentSync for &mut S {
    fn get_latest_parent_entry_ref(&self, object_id: ObjectID) -> SuiResult<Option<ObjectRef>> {
        ParentSync::get_latest_parent_entry_ref(*self, object_id)
    }
}

impl<S: ChildObjectResolver> ChildObjectResolver for std::sync::Arc<S> {
    fn read_child_object(&self, parent: &ObjectID, child: &ObjectID) -> SuiResult<Option<Object>> {
        ChildObjectResolver::read_child_object(self.as_ref(), parent, child)
    }
}

impl<S: ChildObjectResolver> ChildObjectResolver for &S {
    fn read_child_object(&self, parent: &ObjectID, child: &ObjectID) -> SuiResult<Option<Object>> {
        ChildObjectResolver::read_child_object(*self, parent, child)
    }
}

pub trait ReadStore {
    fn get_checkpoint_by_digest(&self, digest: &CheckpointDigest) -> Option<VerifiedCheckpoint>;

    fn get_checkpoint_by_sequence_number(
        &self,
        sequence_number: CheckpointSequenceNumber,
    ) -> Option<VerifiedCheckpoint>;

    fn get_highest_verified_checkpoint(&self) -> Option<VerifiedCheckpoint>;

    fn get_highest_synced_checkpoint(&self) -> Option<VerifiedCheckpoint>;

    fn get_checkpoint_contents(
        &self,
        digest: &CheckpointContentsDigest,
    ) -> Option<CheckpointContents>;

    fn get_committee(&self, epoch: EpochId) -> Option<Committee>;

    fn get_transaction(&self, digest: &TransactionDigest) -> Option<Transaction>;

    fn get_transaction_effects(
        &self,
        digest: &TransactionEffectsDigest,
    ) -> Option<TransactionEffects>;
}

impl<T: ReadStore> ReadStore for &T {
    fn get_checkpoint_by_digest(&self, digest: &CheckpointDigest) -> Option<VerifiedCheckpoint> {
        ReadStore::get_checkpoint_by_digest(*self, digest)
    }

    fn get_checkpoint_by_sequence_number(
        &self,
        sequence_number: CheckpointSequenceNumber,
    ) -> Option<VerifiedCheckpoint> {
        ReadStore::get_checkpoint_by_sequence_number(*self, sequence_number)
    }

    fn get_highest_verified_checkpoint(&self) -> Option<VerifiedCheckpoint> {
        ReadStore::get_highest_verified_checkpoint(*self)
    }

    fn get_highest_synced_checkpoint(&self) -> Option<VerifiedCheckpoint> {
        ReadStore::get_highest_synced_checkpoint(*self)
    }

    fn get_checkpoint_contents(
        &self,
        digest: &CheckpointContentsDigest,
    ) -> Option<CheckpointContents> {
        ReadStore::get_checkpoint_contents(*self, digest)
    }

    fn get_committee(&self, epoch: EpochId) -> Option<Committee> {
        ReadStore::get_committee(*self, epoch)
    }

    fn get_transaction(&self, digest: &TransactionDigest) -> Option<Transaction> {
        ReadStore::get_transaction(*self, digest)
    }

    fn get_transaction_effects(
        &self,
        digest: &TransactionEffectsDigest,
    ) -> Option<TransactionEffects> {
        ReadStore::get_transaction_effects(*self, digest)
    }
}

pub trait WriteStore: ReadStore {
    fn insert_checkpoint(&self, checkpoint: VerifiedCheckpoint);
    fn update_highest_synced_checkpoint(&self, checkpoint: &VerifiedCheckpoint);
    fn insert_checkpoint_contents(&self, contents: CheckpointContents);

    fn insert_committee(&self, new_committee: Committee);

    fn insert_transaction(&self, transaction: Transaction);
    fn insert_transaction_effects(&self, transaction_effects: TransactionEffects);
}

impl<T: WriteStore> WriteStore for &T {
    fn insert_checkpoint(&self, checkpoint: VerifiedCheckpoint) {
        WriteStore::insert_checkpoint(*self, checkpoint)
    }

    fn update_highest_synced_checkpoint(&self, checkpoint: &VerifiedCheckpoint) {
        WriteStore::update_highest_synced_checkpoint(*self, checkpoint)
    }

    fn insert_checkpoint_contents(&self, contents: CheckpointContents) {
        WriteStore::insert_checkpoint_contents(*self, contents)
    }

    fn insert_committee(&self, new_committee: Committee) {
        WriteStore::insert_committee(*self, new_committee)
    }

    fn insert_transaction(&self, transaction: Transaction) {
        WriteStore::insert_transaction(*self, transaction)
    }

    fn insert_transaction_effects(&self, transaction_effects: TransactionEffects) {
        WriteStore::insert_transaction_effects(*self, transaction_effects)
    }
}

#[derive(Debug, Default)]
pub struct InMemoryStore {
    highest_verified_checkpoint: Option<(CheckpointSequenceNumber, CheckpointDigest)>,
    highest_synced_checkpoint: Option<(CheckpointSequenceNumber, CheckpointDigest)>,
    checkpoints: HashMap<CheckpointDigest, VerifiedCheckpoint>,
    sequence_number_to_digest: HashMap<CheckpointSequenceNumber, CheckpointDigest>,
    checkpoint_contents: HashMap<CheckpointContentsDigest, CheckpointContents>,
    transactions: HashMap<TransactionDigest, Transaction>,
    effects: HashMap<TransactionEffectsDigest, TransactionEffects>,

    epoch_to_committee: Vec<Committee>,
}

impl InMemoryStore {
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

    pub fn get_checkpoint_contents(
        &self,
        digest: &CheckpointContentsDigest,
    ) -> Option<&CheckpointContents> {
        self.checkpoint_contents.get(digest)
    }

    pub fn insert_checkpoint_contents(&mut self, contents: CheckpointContents) {
        self.checkpoint_contents.insert(contents.digest(), contents);
    }

    pub fn insert_checkpoint(&mut self, checkpoint: VerifiedCheckpoint) {
        let digest = checkpoint.digest();
        let sequence_number = checkpoint.sequence_number();

        if let Some(next_committee) = checkpoint.next_epoch_committee() {
            let next_committee = next_committee.iter().cloned().collect();
            let committee = Committee::new(checkpoint.epoch().saturating_add(1), next_committee)
                .expect("new committee from consensus should be constructable");
            self.insert_committee(committee);
        }

        // Update latest
        if Some(sequence_number) > self.highest_verified_checkpoint.map(|x| x.0) {
            self.highest_verified_checkpoint = Some((sequence_number, digest));
        }

        self.checkpoints.insert(digest, checkpoint);
        self.sequence_number_to_digest
            .insert(sequence_number, digest);
    }

    pub fn update_highest_synced_checkpoint(&mut self, checkpoint: &VerifiedCheckpoint) {
        if !self.checkpoints.contains_key(&checkpoint.digest()) {
            panic!("store should already contain checkpoint");
        }

        self.highest_synced_checkpoint = Some((checkpoint.sequence_number(), checkpoint.digest()));
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

        if self.epoch_to_committee.len() == epoch {
            self.epoch_to_committee.push(committee);
        } else {
            panic!("committe was inserted into EpochCommitteeMap out of order");
        }
    }

    pub fn get_transaction(&self, digest: &TransactionDigest) -> Option<&Transaction> {
        self.transactions.get(digest)
    }

    pub fn get_transaction_effects(
        &self,
        digest: &TransactionEffectsDigest,
    ) -> Option<&TransactionEffects> {
        self.effects.get(digest)
    }

    pub fn insert_transaction(&mut self, transaction: Transaction) {
        self.transactions.insert(*transaction.digest(), transaction);
    }

    pub fn insert_transaction_effects(&mut self, effects: TransactionEffects) {
        self.effects.insert(effects.digest(), effects);
    }
}

#[derive(Clone, Debug, Default)]
pub struct SharedInMemoryStore(std::sync::Arc<std::sync::RwLock<InMemoryStore>>);

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

    fn get_highest_verified_checkpoint(&self) -> Option<VerifiedCheckpoint> {
        self.inner().get_highest_verified_checkpoint().cloned()
    }

    fn get_highest_synced_checkpoint(&self) -> Option<VerifiedCheckpoint> {
        self.inner().get_highest_synced_checkpoint().cloned()
    }

    fn get_checkpoint_contents(
        &self,
        digest: &CheckpointContentsDigest,
    ) -> Option<CheckpointContents> {
        self.inner().get_checkpoint_contents(digest).cloned()
    }

    fn get_committee(&self, epoch: EpochId) -> Option<Committee> {
        self.inner().get_committee_by_epoch(epoch).cloned()
    }

    fn get_transaction(&self, digest: &TransactionDigest) -> Option<Transaction> {
        self.inner().get_transaction(digest).cloned()
    }

    fn get_transaction_effects(
        &self,
        digest: &TransactionEffectsDigest,
    ) -> Option<TransactionEffects> {
        self.inner().get_transaction_effects(digest).cloned()
    }
}

impl WriteStore for SharedInMemoryStore {
    fn insert_checkpoint(&self, checkpoint: VerifiedCheckpoint) {
        self.inner_mut().insert_checkpoint(checkpoint)
    }

    fn update_highest_synced_checkpoint(&self, checkpoint: &VerifiedCheckpoint) {
        self.inner_mut()
            .update_highest_synced_checkpoint(checkpoint)
    }

    fn insert_checkpoint_contents(&self, contents: CheckpointContents) {
        self.inner_mut().insert_checkpoint_contents(contents)
    }

    fn insert_committee(&self, new_committee: Committee) {
        self.inner_mut().insert_committee(new_committee)
    }

    fn insert_transaction(&self, transaction: Transaction) {
        self.inner_mut().insert_transaction(transaction)
    }

    fn insert_transaction_effects(&self, transaction_effects: TransactionEffects) {
        self.inner_mut()
            .insert_transaction_effects(transaction_effects)
    }
}
