// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::base_types::{TransactionDigest, VersionNumber};
use crate::committee::{Committee, EpochId};
use crate::digests::{
    CheckpointContentsDigest, CheckpointDigest, TransactionEffectsDigest, TransactionEventsDigest,
};
use crate::effects::{TransactionEffects, TransactionEvents};
use crate::error::SuiError;
use crate::message_envelope::Message;
use crate::messages::{SenderSignedData, TransactionDataAPI, VerifiedTransaction};
use crate::messages_checkpoint::{
    CheckpointContents, CheckpointSequenceNumber, FullCheckpointContents, VerifiedCheckpoint,
    VerifiedCheckpointContents,
};
use crate::move_package::MovePackage;
use crate::{
    base_types::{ObjectID, ObjectRef, SequenceNumber},
    error::SuiResult,
    event::Event,
    object::Object,
};
use itertools::Itertools;
use move_binary_format::CompiledModule;
use move_core_types::language_storage::ModuleId;
use serde::{Deserialize, Serialize};
use serde_with::serde_as;
use std::collections::{BTreeMap, HashMap};
use std::convert::Infallible;
use std::fmt::{Display, Formatter};
use std::sync::Arc;
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

#[derive(Debug)]
pub enum ObjectChange {
    Write(Object, WriteKind),
    Delete(SequenceNumber, DeleteKind),
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

    fn save_loaded_child_objects(
        &mut self,
        loaded_child_objects: BTreeMap<ObjectID, SequenceNumber>,
    );
}

pub type PackageFetchResults<Package> = Result<Vec<Package>, Vec<ObjectID>>;

pub trait BackingPackageStore {
    fn get_package_object(&self, package_id: &ObjectID) -> SuiResult<Option<Object>>;
    fn get_package(&self, package_id: &ObjectID) -> SuiResult<Option<MovePackage>> {
        self.get_package_object(package_id)
            .map(|opt_obj| opt_obj.and_then(|obj| obj.data.try_into_package()))
    }
    /// Returns Ok(<object for each package id in `package_ids`>) if all package IDs in
    /// `package_id` were found. If any package in `package_ids` was not found it returns a list
    /// of any package ids that are unable to be found>).
    fn get_package_objects<'a>(
        &self,
        package_ids: impl IntoIterator<Item = &'a ObjectID>,
    ) -> SuiResult<PackageFetchResults<Object>> {
        let package_objects: Vec<Result<Object, ObjectID>> = package_ids
            .into_iter()
            .map(|id| match self.get_package_object(id) {
                Ok(None) => Ok(Err(*id)),
                Ok(Some(o)) => Ok(Ok(o)),
                Err(x) => Err(x),
            })
            .collect::<SuiResult<_>>()?;

        let (fetched, failed_to_fetch): (Vec<_>, Vec<_>) =
            package_objects.into_iter().partition_result();
        if !failed_to_fetch.is_empty() {
            Ok(Err(failed_to_fetch))
        } else {
            Ok(Ok(fetched))
        }
    }
    fn get_packages<'a>(
        &self,
        package_ids: impl IntoIterator<Item = &'a ObjectID>,
    ) -> SuiResult<PackageFetchResults<MovePackage>> {
        let objects = self.get_package_objects(package_ids)?;
        Ok(objects.and_then(|objects| {
            let (packages, failed): (Vec<_>, Vec<_>) = objects
                .into_iter()
                .map(|obj| {
                    let obj_id = obj.id();
                    obj.data.try_into_package().ok_or(obj_id)
                })
                .partition_result();
            if !failed.is_empty() {
                Err(failed)
            } else {
                Ok(packages)
            }
        }))
    }
}

impl<S: BackingPackageStore> BackingPackageStore for std::sync::Arc<S> {
    fn get_package_object(&self, package_id: &ObjectID) -> SuiResult<Option<Object>> {
        BackingPackageStore::get_package_object(self.as_ref(), package_id)
    }
}

impl<S: BackingPackageStore> BackingPackageStore for &S {
    fn get_package_object(&self, package_id: &ObjectID) -> SuiResult<Option<Object>> {
        BackingPackageStore::get_package_object(*self, package_id)
    }
}

impl<S: BackingPackageStore> BackingPackageStore for &mut S {
    fn get_package_object(&self, package_id: &ObjectID) -> SuiResult<Option<Object>> {
        BackingPackageStore::get_package_object(*self, package_id)
    }
}

pub fn get_module<S: BackingPackageStore>(
    store: S,
    module_id: &ModuleId,
) -> Result<Option<Vec<u8>>, SuiError> {
    Ok(store
        .get_package(&ObjectID::from(*module_id.address()))?
        .and_then(|package| {
            package
                .serialized_module_map()
                .get(module_id.name().as_str())
                .cloned()
        }))
}

pub fn get_module_by_id<S: BackingPackageStore>(
    store: S,
    id: &ModuleId,
) -> anyhow::Result<Option<CompiledModule>, SuiError> {
    Ok(get_module(store, id)?
        .map(|bytes| CompiledModule::deserialize_with_defaults(&bytes).unwrap()))
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

    fn get_full_checkpoint_contents_by_sequence_number(
        &self,
        sequence_number: CheckpointSequenceNumber,
    ) -> Result<Option<FullCheckpointContents>, Self::Error>;

    fn get_full_checkpoint_contents(
        &self,
        digest: &CheckpointContentsDigest,
    ) -> Result<Option<FullCheckpointContents>, Self::Error>;

    fn get_committee(&self, epoch: EpochId) -> Result<Option<Arc<Committee>>, Self::Error>;

    fn get_transaction_block(
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

    fn get_full_checkpoint_contents_by_sequence_number(
        &self,
        sequence_number: CheckpointSequenceNumber,
    ) -> Result<Option<FullCheckpointContents>, Self::Error> {
        ReadStore::get_full_checkpoint_contents_by_sequence_number(*self, sequence_number)
    }

    fn get_full_checkpoint_contents(
        &self,
        digest: &CheckpointContentsDigest,
    ) -> Result<Option<FullCheckpointContents>, Self::Error> {
        ReadStore::get_full_checkpoint_contents(*self, digest)
    }

    fn get_committee(&self, epoch: EpochId) -> Result<Option<Arc<Committee>>, Self::Error> {
        ReadStore::get_committee(*self, epoch)
    }

    fn get_transaction_block(
        &self,
        digest: &TransactionDigest,
    ) -> Result<Option<VerifiedTransaction>, Self::Error> {
        ReadStore::get_transaction_block(*self, digest)
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
    fn insert_checkpoint_contents(
        &self,
        checkpoint: &VerifiedCheckpoint,
        contents: VerifiedCheckpointContents,
    ) -> Result<(), Self::Error>;

    fn insert_committee(&self, new_committee: Committee) -> Result<(), Self::Error>;
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

    fn insert_checkpoint_contents(
        &self,
        checkpoint: &VerifiedCheckpoint,
        contents: VerifiedCheckpointContents,
    ) -> Result<(), Self::Error> {
        WriteStore::insert_checkpoint_contents(*self, checkpoint, contents)
    }

    fn insert_committee(&self, new_committee: Committee) -> Result<(), Self::Error> {
        WriteStore::insert_committee(*self, new_committee)
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
    effects: HashMap<TransactionEffectsDigest, TransactionEffects>,
    events: HashMap<TransactionEventsDigest, TransactionEvents>,

    epoch_to_committee: Vec<Committee>,
}

impl InMemoryStore {
    pub fn insert_genesis_state(
        &mut self,
        checkpoint: VerifiedCheckpoint,
        contents: VerifiedCheckpointContents,
        committee: Committee,
    ) {
        self.insert_committee(committee);
        self.insert_checkpoint(checkpoint.clone());
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
                .insert(tx.effects.digest(), tx.effects.to_owned());
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

    pub fn insert_checkpoint(&mut self, checkpoint: VerifiedCheckpoint) {
        let digest = *checkpoint.digest();
        let sequence_number = *checkpoint.sequence_number();

        if let Some(end_of_epoch_data) = &checkpoint.data().end_of_epoch_data {
            let next_committee = end_of_epoch_data
                .next_epoch_committee
                .iter()
                .cloned()
                .collect();
            let committee = Committee::new(checkpoint.epoch().saturating_add(1), next_committee);
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
        if !self.checkpoints.contains_key(checkpoint.digest()) {
            panic!("store should already contain checkpoint");
        }

        self.highest_synced_checkpoint =
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

        if self.epoch_to_committee.len() == epoch {
            self.epoch_to_committee.push(committee);
        } else {
            panic!("committee was inserted into EpochCommitteeMap out of order");
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

    fn get_full_checkpoint_contents_by_sequence_number(
        &self,
        sequence_number: CheckpointSequenceNumber,
    ) -> Result<Option<FullCheckpointContents>, Self::Error> {
        Ok(self
            .inner()
            .full_checkpoint_contents
            .get(&sequence_number)
            .cloned())
    }

    fn get_full_checkpoint_contents(
        &self,
        digest: &CheckpointContentsDigest,
    ) -> Result<Option<FullCheckpointContents>, Self::Error> {
        // First look to see if we saved the complete contents already.
        let inner = self.inner();
        let contents = inner
            .get_sequence_number_by_contents_digest(digest)
            .and_then(|seq_num| inner.full_checkpoint_contents.get(&seq_num).cloned());
        if contents.is_some() {
            return Ok(contents);
        }

        // Otherwise gather it from the individual components.
        inner
            .get_checkpoint_contents(digest)
            .map(|contents| {
                FullCheckpointContents::from_checkpoint_contents(&self, contents.to_owned())
            })
            .transpose()
            .map(|contents| contents.flatten())
    }

    fn get_committee(&self, epoch: EpochId) -> Result<Option<Arc<Committee>>, Self::Error> {
        self.inner()
            .get_committee_by_epoch(epoch)
            .cloned()
            .map(Arc::new)
            .pipe(Ok)
    }

    fn get_transaction_block(
        &self,
        digest: &TransactionDigest,
    ) -> Result<Option<VerifiedTransaction>, Self::Error> {
        self.inner().get_transaction_block(digest).cloned().pipe(Ok)
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

    fn insert_checkpoint_contents(
        &self,
        checkpoint: &VerifiedCheckpoint,
        contents: VerifiedCheckpointContents,
    ) -> Result<(), Self::Error> {
        self.inner_mut()
            .insert_checkpoint_contents(checkpoint, contents);
        Ok(())
    }

    fn insert_committee(&self, new_committee: Committee) -> Result<(), Self::Error> {
        self.inner_mut().insert_committee(new_committee);
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
    use crate::messages::InputObjectKind as I;
    Ok(tx
        .intent_message()
        .value
        .input_objects()?
        .into_iter()
        .filter_map(|object| match object {
            I::MovePackage(_) | I::SharedMoveObject { .. } => None,
            I::ImmOrOwnedMoveObject(obj) => Some(obj.into()),
        })
        .collect())
}

pub trait ObjectStore {
    fn get_object(&self, object_id: &ObjectID) -> Result<Option<Object>, SuiError>;
    fn get_object_by_key(
        &self,
        object_id: &ObjectID,
        version: VersionNumber,
    ) -> Result<Option<Object>, SuiError>;
}

impl ObjectStore for &[Object] {
    fn get_object(&self, object_id: &ObjectID) -> Result<Option<Object>, SuiError> {
        Ok(self.iter().find(|o| o.id() == *object_id).cloned())
    }

    fn get_object_by_key(
        &self,
        object_id: &ObjectID,
        version: VersionNumber,
    ) -> Result<Option<Object>, SuiError> {
        Ok(self
            .iter()
            .find(|o| o.id() == *object_id && o.version() == version)
            .cloned())
    }
}

impl ObjectStore for BTreeMap<ObjectID, (ObjectRef, Object, WriteKind)> {
    fn get_object(&self, object_id: &ObjectID) -> Result<Option<Object>, SuiError> {
        Ok(self.get(object_id).map(|(_, obj, _)| obj).cloned())
    }

    fn get_object_by_key(
        &self,
        object_id: &ObjectID,
        version: VersionNumber,
    ) -> Result<Option<Object>, SuiError> {
        Ok(self
            .get(object_id)
            .and_then(|(_, obj, _)| {
                if obj.version() == version {
                    Some(obj)
                } else {
                    None
                }
            })
            .cloned())
    }
}

impl ObjectStore for BTreeMap<ObjectID, Object> {
    fn get_object(&self, object_id: &ObjectID) -> Result<Option<Object>, SuiError> {
        Ok(self.get(object_id).cloned())
    }

    fn get_object_by_key(
        &self,
        object_id: &ObjectID,
        version: VersionNumber,
    ) -> Result<Option<Object>, SuiError> {
        Ok(self.get(object_id).and_then(|o| {
            if o.version() == version {
                Some(o.clone())
            } else {
                None
            }
        }))
    }
}

impl<T: ObjectStore> ObjectStore for Arc<T> {
    fn get_object(&self, object_id: &ObjectID) -> Result<Option<Object>, SuiError> {
        self.as_ref().get_object(object_id)
    }

    fn get_object_by_key(
        &self,
        object_id: &ObjectID,
        version: VersionNumber,
    ) -> Result<Option<Object>, SuiError> {
        self.as_ref().get_object_by_key(object_id, version)
    }
}

impl Display for DeleteKind {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            DeleteKind::Wrap => write!(f, "Wrap"),
            DeleteKind::Normal => write!(f, "Normal"),
            DeleteKind::UnwrapThenDelete => write!(f, "UnwrapThenDelete"),
        }
    }
}
