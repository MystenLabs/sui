// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::base_types::{SuiAddress, TransactionDigest, VersionNumber};
use crate::committee::{Committee, EpochId};
use crate::digests::{
    CheckpointContentsDigest, CheckpointDigest, TransactionEffectsDigest, TransactionEventsDigest,
};
use crate::error::SuiError;
use crate::message_envelope::Message;
use crate::messages::InputObjectKind::{ImmOrOwnedMoveObject, MovePackage, SharedMoveObject};
use crate::messages::{
    SenderSignedData, TransactionDataAPI, TransactionEffects, TransactionEvents,
    VerifiedTransaction,
};
use crate::messages_checkpoint::{
    CheckpointContents, CheckpointSequenceNumber, VerifiedCheckpoint,
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
use serde_with::serde_as;
use std::collections::{BTreeMap, HashMap};
use std::convert::Infallible;
use tap::Pipe;

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
    // legacy
    pub fn transfer_sui(sender: SuiAddress) -> Self {
        Self::sui_transaction(ident_str!("transfer_sui"), sender)
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
    // programmable transactions
    pub fn split_coin(sender: SuiAddress) -> Self {
        Self::sui_transaction(ident_str!("split_coin"), sender)
    }
    // common to legacy and programmable transactions
    pub fn transfer_object(sender: SuiAddress) -> Self {
        Self::sui_transaction(ident_str!("transfer_object"), sender)
    }
    pub fn unused_input(sender: SuiAddress) -> Self {
        Self::sui_transaction(ident_str!("unused_input_object"), sender)
    }
    pub fn publish(sender: SuiAddress) -> Self {
        Self::sui_transaction(ident_str!("publish"), sender)
    }
    // system
    pub fn gas(sender: SuiAddress) -> Self {
        Self::sui_transaction(ident_str!("gas"), sender)
    }
    pub fn genesis() -> Self {
        Self::sui_transaction(ident_str!("genesis"), SuiAddress::ZERO)
    }
    pub fn sui_system() -> Self {
        Self::sui_transaction(ident_str!("sui_system"), SuiAddress::ZERO)
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

impl<S: ChildObjectResolver> ChildObjectResolver for &mut S {
    fn read_child_object(&self, parent: &ObjectID, child: &ObjectID) -> SuiResult<Option<Object>> {
        ChildObjectResolver::read_child_object(*self, parent, child)
    }
}

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

    fn get_checkpoint_contents(
        &self,
        digest: &CheckpointContentsDigest,
    ) -> Result<Option<CheckpointContents>, Self::Error>;

    fn get_committee(&self, epoch: EpochId) -> Result<Option<Committee>, Self::Error>;

    fn get_transaction(
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

    fn get_checkpoint_contents(
        &self,
        digest: &CheckpointContentsDigest,
    ) -> Result<Option<CheckpointContents>, Self::Error> {
        ReadStore::get_checkpoint_contents(*self, digest)
    }

    fn get_committee(&self, epoch: EpochId) -> Result<Option<Committee>, Self::Error> {
        ReadStore::get_committee(*self, epoch)
    }

    fn get_transaction(
        &self,
        digest: &TransactionDigest,
    ) -> Result<Option<VerifiedTransaction>, Self::Error> {
        ReadStore::get_transaction(*self, digest)
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

pub trait WriteStore: ReadStore {
    fn insert_checkpoint(&self, checkpoint: VerifiedCheckpoint) -> Result<(), Self::Error>;
    fn update_highest_synced_checkpoint(
        &self,
        checkpoint: &VerifiedCheckpoint,
    ) -> Result<(), Self::Error>;
    fn insert_checkpoint_contents(&self, contents: CheckpointContents) -> Result<(), Self::Error>;

    fn insert_committee(&self, new_committee: Committee) -> Result<(), Self::Error>;

    fn insert_transaction_and_effects(
        &self,
        transaction: VerifiedTransaction,
        transaction_effects: TransactionEffects,
    ) -> Result<(), Self::Error>;
}

impl<T: WriteStore> WriteStore for &T {
    fn insert_checkpoint(&self, checkpoint: VerifiedCheckpoint) -> Result<(), Self::Error> {
        WriteStore::insert_checkpoint(*self, checkpoint)
    }

    fn update_highest_synced_checkpoint(
        &self,
        checkpoint: &VerifiedCheckpoint,
    ) -> Result<(), Self::Error> {
        WriteStore::update_highest_synced_checkpoint(*self, checkpoint)
    }

    fn insert_checkpoint_contents(&self, contents: CheckpointContents) -> Result<(), Self::Error> {
        WriteStore::insert_checkpoint_contents(*self, contents)
    }

    fn insert_committee(&self, new_committee: Committee) -> Result<(), Self::Error> {
        WriteStore::insert_committee(*self, new_committee)
    }

    fn insert_transaction_and_effects(
        &self,
        transaction: VerifiedTransaction,
        transaction_effects: TransactionEffects,
    ) -> Result<(), Self::Error> {
        WriteStore::insert_transaction_and_effects(*self, transaction, transaction_effects)
    }
}

#[derive(Debug, Default)]
pub struct InMemoryStore {
    highest_verified_checkpoint: Option<(CheckpointSequenceNumber, CheckpointDigest)>,
    highest_synced_checkpoint: Option<(CheckpointSequenceNumber, CheckpointDigest)>,
    checkpoints: HashMap<CheckpointDigest, VerifiedCheckpoint>,
    sequence_number_to_digest: HashMap<CheckpointSequenceNumber, CheckpointDigest>,
    checkpoint_contents: HashMap<CheckpointContentsDigest, CheckpointContents>,
    transactions: HashMap<TransactionDigest, VerifiedTransaction>,
    effects: HashMap<TransactionEffectsDigest, TransactionEffects>,
    events: HashMap<TransactionEventsDigest, TransactionEvents>,

    epoch_to_committee: Vec<Committee>,
}

impl InMemoryStore {
    pub fn insert_genesis_state(
        &mut self,
        checkpoint: VerifiedCheckpoint,
        contents: CheckpointContents,
        transactions: Vec<VerifiedTransaction>,
        effects: Vec<TransactionEffects>,
        committee: Committee,
    ) {
        self.insert_committee(committee);
        self.insert_checkpoint(checkpoint.clone());
        self.insert_checkpoint_contents(contents);
        self.update_highest_synced_checkpoint(&checkpoint);

        for (transaction, effect) in transactions.into_iter().zip(effects) {
            self.insert_transaction_and_effects(transaction, effect);
        }
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

        if let Some(end_of_epoch_data) = &checkpoint.summary.end_of_epoch_data {
            let next_committee = end_of_epoch_data
                .next_epoch_committee
                .iter()
                .cloned()
                .collect();
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
            panic!("committee was inserted into EpochCommitteeMap out of order");
        }
    }

    pub fn get_transaction(&self, digest: &TransactionDigest) -> Option<&VerifiedTransaction> {
        self.transactions.get(digest)
    }

    pub fn get_transaction_effects(
        &self,
        digest: &TransactionEffectsDigest,
    ) -> Option<&TransactionEffects> {
        self.effects.get(digest)
    }

    pub fn get_transaction_events(
        &self,
        digest: &TransactionEventsDigest,
    ) -> Option<&TransactionEvents> {
        self.events.get(digest)
    }

    pub fn insert_transaction_and_effects(
        &mut self,
        transaction: VerifiedTransaction,
        transaction_effects: TransactionEffects,
    ) {
        self.transactions.insert(*transaction.digest(), transaction);
        self.effects
            .insert(transaction_effects.digest(), transaction_effects);
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
    type Error = Infallible;

    fn get_checkpoint_by_digest(
        &self,
        digest: &CheckpointDigest,
    ) -> Result<Option<VerifiedCheckpoint>, Self::Error> {
        self.inner()
            .get_checkpoint_by_digest(digest)
            .cloned()
            .pipe(Ok)
    }

    fn get_checkpoint_by_sequence_number(
        &self,
        sequence_number: CheckpointSequenceNumber,
    ) -> Result<Option<VerifiedCheckpoint>, Self::Error> {
        self.inner()
            .get_checkpoint_by_sequence_number(sequence_number)
            .cloned()
            .pipe(Ok)
    }

    fn get_highest_verified_checkpoint(&self) -> Result<VerifiedCheckpoint, Self::Error> {
        self.inner()
            .get_highest_verified_checkpoint()
            .cloned()
            .expect("storage should have been initialized with genesis checkpoint")
            .pipe(Ok)
    }

    fn get_highest_synced_checkpoint(&self) -> Result<VerifiedCheckpoint, Self::Error> {
        self.inner()
            .get_highest_synced_checkpoint()
            .cloned()
            .expect("storage should have been initialized with genesis checkpoint")
            .pipe(Ok)
    }

    fn get_checkpoint_contents(
        &self,
        digest: &CheckpointContentsDigest,
    ) -> Result<Option<CheckpointContents>, Self::Error> {
        self.inner()
            .get_checkpoint_contents(digest)
            .cloned()
            .pipe(Ok)
    }

    fn get_committee(&self, epoch: EpochId) -> Result<Option<Committee>, Self::Error> {
        self.inner().get_committee_by_epoch(epoch).cloned().pipe(Ok)
    }

    fn get_transaction(
        &self,
        digest: &TransactionDigest,
    ) -> Result<Option<VerifiedTransaction>, Self::Error> {
        self.inner().get_transaction(digest).cloned().pipe(Ok)
    }

    fn get_transaction_effects(
        &self,
        digest: &TransactionEffectsDigest,
    ) -> Result<Option<TransactionEffects>, Self::Error> {
        self.inner()
            .get_transaction_effects(digest)
            .cloned()
            .pipe(Ok)
    }

    fn get_transaction_events(
        &self,
        digest: &TransactionEventsDigest,
    ) -> Result<Option<TransactionEvents>, Self::Error> {
        self.inner()
            .get_transaction_events(digest)
            .cloned()
            .pipe(Ok)
    }
}

impl WriteStore for SharedInMemoryStore {
    fn insert_checkpoint(&self, checkpoint: VerifiedCheckpoint) -> Result<(), Self::Error> {
        self.inner_mut().insert_checkpoint(checkpoint);
        Ok(())
    }

    fn update_highest_synced_checkpoint(
        &self,
        checkpoint: &VerifiedCheckpoint,
    ) -> Result<(), Self::Error> {
        self.inner_mut()
            .update_highest_synced_checkpoint(checkpoint);
        Ok(())
    }

    fn insert_checkpoint_contents(&self, contents: CheckpointContents) -> Result<(), Self::Error> {
        self.inner_mut().insert_checkpoint_contents(contents);
        Ok(())
    }

    fn insert_committee(&self, new_committee: Committee) -> Result<(), Self::Error> {
        self.inner_mut().insert_committee(new_committee);
        Ok(())
    }

    fn insert_transaction_and_effects(
        &self,
        transaction: VerifiedTransaction,
        transaction_effects: TransactionEffects,
    ) -> Result<(), Self::Error> {
        self.inner_mut()
            .insert_transaction_and_effects(transaction, transaction_effects);
        Ok(())
    }
}

// The primary key type for object storage.
#[serde_as]
#[derive(Eq, PartialEq, Clone, Copy, PartialOrd, Ord, Hash, Serialize, Deserialize, Debug)]
pub struct ObjectKey(pub ObjectID, pub VersionNumber);

impl ObjectKey {
    pub const ZERO: ObjectKey = ObjectKey(ObjectID::ZERO, VersionNumber::MIN);

    pub fn max_for_id(id: &ObjectID) -> Self {
        Self(*id, VersionNumber::MAX)
    }
}

impl From<ObjectRef> for ObjectKey {
    fn from(object_ref: ObjectRef) -> Self {
        ObjectKey::from(&object_ref)
    }
}

impl From<&ObjectRef> for ObjectKey {
    fn from(object_ref: &ObjectRef) -> Self {
        Self(object_ref.0, object_ref.1)
    }
}

/// Fetch the `ObjectKey`s (IDs and versions) for non-shared input objects.  Includes owned,
/// and immutable objects as well as the gas objects, but not move packages or shared objects.
pub fn transaction_input_object_keys(tx: &SenderSignedData) -> SuiResult<Vec<ObjectKey>> {
    Ok(tx
        .intent_message
        .value
        .input_objects()?
        .into_iter()
        .filter_map(|object| match object {
            MovePackage(_) | SharedMoveObject { .. } => None,
            ImmOrOwnedMoveObject(obj) => Some(obj.into()),
        })
        .collect())
}

pub trait ObjectStore {
    fn get_object(&self, object_id: &ObjectID) -> Result<Option<Object>, SuiError>;
}

impl ObjectStore for &[Object] {
    fn get_object(&self, object_id: &ObjectID) -> Result<Option<Object>, SuiError> {
        Ok(self.iter().find(|o| o.id() == *object_id).cloned())
    }
}

impl ObjectStore for BTreeMap<ObjectID, (ObjectRef, Object, WriteKind)> {
    fn get_object(&self, object_id: &ObjectID) -> Result<Option<Object>, SuiError> {
        Ok(self.get(object_id).map(|(_, obj, _)| obj).cloned())
    }
}
