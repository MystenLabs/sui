// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;

use anyhow::{Context as _, anyhow};
use tracing::{error, warn};

use move_binary_format::CompiledModule;
use move_bytecode_utils::module_cache::GetModule;
use move_core_types::{language_storage::ModuleId, resolver::ModuleResolver};

use simulacrum::SimulatorStore;
use sui_data_store::stores::{
    DataStore, FileSystemStore, InMemoryStore, ReadThroughStore, WriteThroughStore,
};
use sui_data_store::{ObjectKey, ObjectStore as _, ObjectStoreWriter as _, VersionQuery};
use sui_types::SUI_CLOCK_OBJECT_ID;
use sui_types::clock::Clock;
use sui_types::error::SuiErrorKind;
use sui_types::full_checkpoint_content::{Checkpoint as FullCheckpointData, ExecutedTransaction};
use sui_types::message_envelope::Envelope;
use sui_types::storage::ReadStore;
use sui_types::sui_system_state::sui_system_state_inner_v1::ValidatorSetV1;
use sui_types::sui_system_state::{SuiSystemState, get_sui_system_state};
use sui_types::transaction::{SenderSignedData, Transaction};
use sui_types::{
    base_types::{ObjectID, SequenceNumber, SuiAddress},
    committee::{Committee, EpochId},
    digests::{ObjectDigest, TransactionDigest},
    effects::{TransactionEffects, TransactionEffectsAPI, TransactionEvents},
    messages_checkpoint::{
        CheckpointContents, CheckpointContentsDigest, CheckpointDigest, CheckpointSequenceNumber,
        VerifiedCheckpoint,
    },
    object::{Object, Owner},
    storage::{
        BackingPackageStore, ChildObjectResolver, ObjectStore, PackageObject, ParentSync,
        get_module, load_package_object_from_object_store,
    },
    transaction::VerifiedTransaction,
};

pub(crate) type DiskThenGraphqlObjects = ReadThroughStore<Arc<FileSystemStore>, Arc<DataStore>>;
pub(crate) type HotObjects = WriteThroughStore<Arc<InMemoryStore>, Arc<DiskThenGraphqlObjects>>;

pub struct ForkingStore {
    checkpoints: BTreeMap<CheckpointSequenceNumber, VerifiedCheckpoint>,
    checkpoint_contents: BTreeMap<CheckpointSequenceNumber, CheckpointContents>,
    checkpoint_sequences_by_digest: HashMap<CheckpointDigest, CheckpointSequenceNumber>,
    checkpoint_sequences_by_contents_digest:
        HashMap<CheckpointContentsDigest, CheckpointSequenceNumber>,
    epoch_to_committee: BTreeMap<EpochId, Committee>,
    events: HashMap<TransactionDigest, TransactionEvents>,
    transactions: HashMap<TransactionDigest, VerifiedTransaction>,
    effects: HashMap<TransactionDigest, TransactionEffects>,
    filesystem: Arc<FileSystemStore>,
    objects: Arc<HotObjects>,
    forked_at_checkpoint: u64,
    validator_set_override: Option<ValidatorSetV1>,
    pending_checkpoint: Option<VerifiedCheckpoint>,
}

impl ForkingStore {
    pub fn new(
        forked_at_checkpoint: u64,
        filesystem: Arc<FileSystemStore>,
        objects: Arc<HotObjects>,
    ) -> Self {
        Self {
            checkpoints: BTreeMap::new(),
            checkpoint_contents: BTreeMap::new(),
            checkpoint_sequences_by_digest: HashMap::new(),
            checkpoint_sequences_by_contents_digest: HashMap::new(),
            epoch_to_committee: BTreeMap::new(),
            events: HashMap::new(),
            transactions: HashMap::new(),
            effects: HashMap::new(),
            filesystem,
            objects,
            forked_at_checkpoint,
            validator_set_override: None,
            pending_checkpoint: None,
        }
    }

    fn verified_checkpoint_from_full_checkpoint_data(
        checkpoint: &FullCheckpointData,
    ) -> VerifiedCheckpoint {
        let certified_summary = checkpoint.summary.clone();
        let envelope = Envelope::new_from_data_and_sig(
            certified_summary.data().clone(),
            certified_summary.auth_sig().clone(),
        );
        VerifiedCheckpoint::new_unchecked(envelope)
    }

    fn insert_checkpoint_transaction(&mut self, transaction: &ExecutedTransaction) {
        let sender_signed_data = SenderSignedData::new(
            transaction.transaction.clone(),
            transaction.signatures.clone(),
        );
        let verified_transaction =
            VerifiedTransaction::new_unchecked(Transaction::new(sender_signed_data));
        let digest = *transaction.effects.transaction_digest();

        self.transactions.insert(digest, verified_transaction);
        self.effects.insert(digest, transaction.effects.clone());
        if let Some(events) = transaction.events.clone() {
            self.events.insert(digest, events);
        }
    }

    pub fn insert_startup_checkpoint_data(
        &mut self,
        checkpoint: &FullCheckpointData,
    ) -> Result<(), anyhow::Error> {
        let verified_checkpoint = Self::verified_checkpoint_from_full_checkpoint_data(checkpoint);
        self.insert_checkpoint(verified_checkpoint);
        for transaction in &checkpoint.transactions {
            self.insert_checkpoint_transaction(transaction);
        }
        self.insert_checkpoint_contents(checkpoint.contents.clone());
        Ok(())
    }

    pub fn get_checkpoint_by_sequence_number(
        &self,
        sequence_number: CheckpointSequenceNumber,
    ) -> Option<VerifiedCheckpoint> {
        self.checkpoints.get(&sequence_number).cloned()
    }

    pub fn get_checkpoint_by_digest(
        &self,
        digest: &CheckpointDigest,
    ) -> Option<VerifiedCheckpoint> {
        let sequence_number = *self.checkpoint_sequences_by_digest.get(digest)?;
        self.get_checkpoint_by_sequence_number(sequence_number)
    }

    pub fn get_checkpoint_contents_by_sequence_number(
        &self,
        sequence_number: CheckpointSequenceNumber,
    ) -> Option<CheckpointContents> {
        self.checkpoint_contents.get(&sequence_number).cloned()
    }

    pub fn get_checkpoint_contents_by_digest(
        &self,
        digest: &CheckpointContentsDigest,
    ) -> Option<CheckpointContents> {
        let sequence_number = *self.checkpoint_sequences_by_contents_digest.get(digest)?;
        self.get_checkpoint_contents_by_sequence_number(sequence_number)
    }

    pub fn get_highest_checkpint(&self) -> Option<VerifiedCheckpoint> {
        self.checkpoints
            .last_key_value()
            .map(|(_, checkpoint)| checkpoint.clone())
    }

    pub fn get_committee_by_epoch(&self, epoch: EpochId) -> Option<&Committee> {
        self.epoch_to_committee.get(&epoch)
    }

    pub fn get_transaction(&self, digest: &TransactionDigest) -> Option<VerifiedTransaction> {
        self.transactions.get(digest).cloned()
    }

    pub fn get_transaction_effects(
        &self,
        digest: &TransactionDigest,
    ) -> Option<TransactionEffects> {
        self.effects.get(digest).cloned()
    }

    pub fn get_transaction_events(&self, digest: &TransactionDigest) -> Option<&TransactionEvents> {
        self.events.get(digest)
    }

    pub fn get_object(&self, id: &ObjectID) -> Option<Object> {
        let local_latest = match self.filesystem.get_object_latest(id) {
            Ok(object) => object,
            Err(err) => {
                error!(
                    object_id = %id,
                    "failed to read latest object from local filesystem store: {err}"
                );
                None
            }
        };

        if let Some((object, _version)) = local_latest {
            return Some(object);
        }

        let objects = match self.objects.get_objects(&[ObjectKey {
            object_id: *id,
            version_query: VersionQuery::AtCheckpoint(self.forked_at_checkpoint),
        }]) {
            Ok(objects) => objects,
            Err(err) => {
                error!(
                    object_id = %id,
                    checkpoint = self.forked_at_checkpoint,
                    "failed to fetch object from object chain: {err}"
                );
                return None;
            }
        };

        objects
            .first()
            .and_then(|entry| entry.as_ref())
            .map(|(object, _version)| object.clone())
    }

    pub fn get_object_at_version(&self, id: &ObjectID, version: SequenceNumber) -> Option<Object> {
        let objects = match self.objects.get_objects(&[ObjectKey {
            object_id: *id,
            version_query: VersionQuery::Version(version.value()),
        }]) {
            Ok(objects) => objects,
            Err(err) => {
                error!(
                    object_id = %id,
                    object_version = version.value(),
                    "failed to fetch object version from object chain: {err}"
                );
                return None;
            }
        };

        objects
            .first()
            .and_then(|entry| entry.as_ref())
            .map(|(object, _version)| object.clone())
    }

    pub fn get_objects(
        &self,
        keys: &[ObjectKey],
    ) -> Result<Vec<Option<(Object, u64)>>, anyhow::Error> {
        self.objects.get_objects(keys).map_err(Into::into)
    }

    pub fn get_system_state(&self) -> SuiSystemState {
        let system_state = get_sui_system_state(self).expect("system state should exist");
        let Some(validators) = &self.validator_set_override else {
            return system_state;
        };

        match system_state {
            SuiSystemState::V1(mut inner) => {
                inner.validators = validators.clone();
                SuiSystemState::V1(inner)
            }
            SuiSystemState::V2(mut inner) => {
                inner.validators = validators.clone();
                SuiSystemState::V2(inner)
            }
            #[cfg(msim)]
            state @ (SuiSystemState::SimTestV1(_)
            | SuiSystemState::SimTestShallowV2(_)
            | SuiSystemState::SimTestDeepV2(_)) => state,
        }
    }

    pub fn get_clock(&self) -> Clock {
        self.get_object(&SUI_CLOCK_OBJECT_ID)
            .expect("clock should exist")
            .to_rust()
            .expect("clock object should deserialize")
    }

    pub fn owned_objects(&self, owner: SuiAddress) -> Vec<Object> {
        self.filesystem
            .get_objects_by_owner(owner)
            .unwrap_or_default()
    }

    pub fn checkpoint_count(&self) -> usize {
        self.checkpoints.len()
    }

    pub fn transaction_count(&self) -> usize {
        self.transactions.len()
    }

    pub fn effect_count(&self) -> usize {
        self.effects.len()
    }

    pub fn event_count(&self) -> usize {
        self.events.len()
    }

    pub fn set_system_state_validator_set_override(&mut self, validators: ValidatorSetV1) {
        self.validator_set_override = Some(validators);
    }

    pub fn insert_checkpoint(&mut self, checkpoint: VerifiedCheckpoint) {
        if let Some(end_of_epoch_data) = &checkpoint.data().end_of_epoch_data {
            let next_committee = end_of_epoch_data
                .next_epoch_committee
                .iter()
                .cloned()
                .collect();
            if let Some(next_epoch) = checkpoint.epoch().checked_add(1) {
                self.insert_committee(Committee::new(next_epoch, next_committee));
            } else {
                warn!(
                    sequence_number = *checkpoint.sequence_number(),
                    current_epoch = checkpoint.epoch(),
                    "skipping committee insertion due to epoch overflow"
                );
            }
        }

        if let Some(previous_pending) = &self.pending_checkpoint {
            warn!(
                previous_sequence = *previous_pending.sequence_number(),
                next_sequence = *checkpoint.sequence_number(),
                "overwriting pending checkpoint before matching contents were inserted"
            );
        }
        self.pending_checkpoint = Some(checkpoint);
    }

    pub fn insert_checkpoint_contents(&mut self, contents: CheckpointContents) {
        let Some(checkpoint) = self.pending_checkpoint.take() else {
            warn!("checkpoint contents inserted without a pending checkpoint summary");
            return;
        };

        if checkpoint.content_digest != *contents.digest() {
            warn!(
                sequence_number = *checkpoint.sequence_number(),
                "checkpoint content digest mismatch between summary and inserted contents"
            );
            return;
        }

        let sequence_number = *checkpoint.sequence_number();
        self.checkpoint_sequences_by_digest
            .insert(*checkpoint.digest(), sequence_number);
        self.checkpoint_sequences_by_contents_digest
            .insert(*contents.digest(), sequence_number);
        self.checkpoint_contents.insert(sequence_number, contents);
        self.checkpoints.insert(sequence_number, checkpoint);
    }

    pub fn insert_committee(&mut self, committee: Committee) {
        self.epoch_to_committee
            .entry(committee.epoch)
            .or_insert(committee);
    }

    pub fn insert_executed_transaction(
        &mut self,
        transaction: VerifiedTransaction,
        effects: TransactionEffects,
        events: TransactionEvents,
        written_objects: BTreeMap<ObjectID, Object>,
    ) {
        let transaction_digest = *effects.transaction_digest();
        self.transactions.insert(transaction_digest, transaction);
        self.effects.insert(transaction_digest, effects);
        self.events.insert(transaction_digest, events);

        if let Err(err) = self.persist_objects(written_objects) {
            error!(
                transaction_digest = %transaction_digest,
                "failed to persist written objects for executed transaction: {err}"
            );
        }
    }

    pub fn insert_transaction(&mut self, transaction: VerifiedTransaction) {
        self.transactions.insert(*transaction.digest(), transaction);
    }

    pub fn insert_transaction_effects(&mut self, effects: TransactionEffects) {
        self.effects.insert(*effects.transaction_digest(), effects);
    }

    pub fn insert_events(&mut self, tx_digest: &TransactionDigest, events: TransactionEvents) {
        self.events.insert(*tx_digest, events);
    }

    pub fn update_objects(
        &mut self,
        written_objects: BTreeMap<ObjectID, Object>,
        _deleted_objects: Vec<(ObjectID, SequenceNumber, ObjectDigest)>,
    ) {
        if let Err(err) = self.persist_objects(written_objects) {
            error!("failed to persist updated objects to local object chain: {err}");
        }
    }

    fn persist_objects(
        &self,
        written_objects: BTreeMap<ObjectID, Object>,
    ) -> Result<(), anyhow::Error> {
        for (object_id, object) in written_objects {
            let version = object.version().value();
            let key = ObjectKey {
                object_id,
                version_query: VersionQuery::Version(version),
            };
            self.objects
                .write_object(&key, object, version)
                .with_context(|| {
                    format!("failed to persist object {object_id} at version {version}")
                })?;
        }
        Ok(())
    }
}

impl BackingPackageStore for ForkingStore {
    fn get_package_object(
        &self,
        package_id: &ObjectID,
    ) -> sui_types::error::SuiResult<Option<PackageObject>> {
        load_package_object_from_object_store(self, package_id)
    }
}

impl ChildObjectResolver for ForkingStore {
    fn read_child_object(
        &self,
        parent: &ObjectID,
        child: &ObjectID,
        child_version_upper_bound: SequenceNumber,
    ) -> sui_types::error::SuiResult<Option<Object>> {
        let validate = |child_object: Object| -> sui_types::error::SuiResult<Option<Object>> {
            let parent = *parent;
            if child_object.owner != Owner::ObjectOwner(parent.into()) {
                return Err(SuiErrorKind::InvalidChildObjectAccess {
                    object: *child,
                    given_parent: parent,
                    actual_owner: child_object.owner.clone(),
                }
                .into());
            }

            if child_object.version() > child_version_upper_bound {
                return Err(SuiErrorKind::UnsupportedFeatureError {
                    error:
                        "TODO ForkingStore::read_child_object does not yet support bounded reads"
                            .to_owned(),
                }
                .into());
            }

            Ok(Some(child_object))
        };

        let local_latest = self
            .filesystem
            .get_object_latest(child)
            .map_err(|err| SuiErrorKind::Storage(err.to_string()))?;
        if let Some((child_object, _version)) = &local_latest
            && child_object.version() <= child_version_upper_bound
        {
            return validate(child_object.clone());
        }

        let object_key = ObjectKey {
            object_id: *child,
            version_query: VersionQuery::RootVersion(child_version_upper_bound.value()),
        };
        let mut object = self
            .get_objects(&[object_key])
            .map_err(|err| SuiErrorKind::Storage(err.to_string()))?;
        debug_assert_eq!(object.len(), 1, "expected a single child object lookup");

        match object.pop().unwrap().map(|(object, _version)| object) {
            Some(object) => validate(object),
            None => {
                if local_latest.is_some() {
                    Err(SuiErrorKind::UnsupportedFeatureError {
                        error:
                            "TODO ForkingStore::read_child_object does not yet support bounded reads"
                                .to_owned(),
                    }
                    .into())
                } else {
                    Ok(None)
                }
            }
        }
    }

    fn get_object_received_at_version(
        &self,
        owner: &ObjectID,
        receiving_object_id: &ObjectID,
        receive_object_at_version: SequenceNumber,
        _epoch_id: EpochId,
    ) -> sui_types::error::SuiResult<Option<Object>> {
        let Some(object) = self.get_object(receiving_object_id) else {
            return Ok(None);
        };
        if object.owner != Owner::AddressOwner((*owner).into()) {
            return Ok(None);
        }
        if object.version() != receive_object_at_version {
            return Ok(None);
        }
        Ok(Some(object))
    }
}

impl GetModule for ForkingStore {
    type Error = anyhow::Error;
    type Item = Arc<CompiledModule>;

    fn get_module_by_id(&self, id: &ModuleId) -> Result<Option<Self::Item>, Self::Error> {
        let module = self
            .get_module(id)?
            .map(|bytes| {
                CompiledModule::deserialize_with_defaults(&bytes)
                    .map_err(|err| anyhow!("failed to deserialize compiled module {id:?}: {err}"))
            })
            .transpose()?;

        Ok(module.map(Arc::new))
    }
}

impl ModuleResolver for ForkingStore {
    type Error = anyhow::Error;

    fn get_module(&self, module_id: &ModuleId) -> Result<Option<Vec<u8>>, Self::Error> {
        get_module(self, module_id).map_err(|err| anyhow!(err.to_string()))
    }

    fn get_packages_static<const N: usize>(
        &self,
        _ids: [move_core_types::account_address::AccountAddress; N],
    ) -> Result<[Option<move_core_types::resolver::SerializedPackage>; N], Self::Error> {
        todo!()
    }

    fn get_packages<'a>(
        &self,
        _ids: impl ExactSizeIterator<Item = &'a move_core_types::account_address::AccountAddress>,
    ) -> Result<Vec<Option<move_core_types::resolver::SerializedPackage>>, Self::Error> {
        todo!()
    }
}

impl ObjectStore for ForkingStore {
    fn get_object(&self, object_id: &ObjectID) -> Option<Object> {
        ForkingStore::get_object(self, object_id)
    }

    fn get_object_by_key(
        &self,
        object_id: &ObjectID,
        version: sui_types::base_types::VersionNumber,
    ) -> Option<Object> {
        ForkingStore::get_object_at_version(self, object_id, version)
    }
}

impl ParentSync for ForkingStore {
    fn get_latest_parent_entry_ref_deprecated(
        &self,
        _object_id: ObjectID,
    ) -> Option<sui_types::base_types::ObjectRef> {
        panic!("Never called in newer protocol versions")
    }
}

impl SimulatorStore for ForkingStore {
    fn get_checkpoint_by_sequence_number(
        &self,
        sequence_number: CheckpointSequenceNumber,
    ) -> Option<VerifiedCheckpoint> {
        ForkingStore::get_checkpoint_by_sequence_number(self, sequence_number)
    }

    fn get_checkpoint_by_digest(&self, digest: &CheckpointDigest) -> Option<VerifiedCheckpoint> {
        ForkingStore::get_checkpoint_by_digest(self, digest)
    }

    fn get_highest_checkpint(&self) -> Option<VerifiedCheckpoint> {
        ForkingStore::get_highest_checkpint(self)
    }

    fn get_checkpoint_contents(
        &self,
        digest: &CheckpointContentsDigest,
    ) -> Option<CheckpointContents> {
        ForkingStore::get_checkpoint_contents_by_digest(self, digest)
    }

    fn get_committee_by_epoch(&self, epoch: EpochId) -> Option<Committee> {
        self.get_committee_by_epoch(epoch).cloned()
    }

    fn get_transaction(&self, digest: &TransactionDigest) -> Option<VerifiedTransaction> {
        ForkingStore::get_transaction(self, digest)
    }

    fn get_transaction_effects(&self, digest: &TransactionDigest) -> Option<TransactionEffects> {
        ForkingStore::get_transaction_effects(self, digest)
    }

    fn get_transaction_events(&self, digest: &TransactionDigest) -> Option<TransactionEvents> {
        self.get_transaction_events(digest).cloned()
    }

    fn get_object(&self, id: &ObjectID) -> Option<Object> {
        ForkingStore::get_object(self, id)
    }

    fn get_object_at_version(&self, id: &ObjectID, version: SequenceNumber) -> Option<Object> {
        ForkingStore::get_object_at_version(self, id, version)
    }

    fn get_system_state(&self) -> SuiSystemState {
        ForkingStore::get_system_state(self)
    }

    fn get_clock(&self) -> Clock {
        ForkingStore::get_clock(self)
    }

    fn owned_objects(&self, owner: SuiAddress) -> Box<dyn Iterator<Item = Object> + '_> {
        Box::new(self.owned_objects(owner).into_iter())
    }

    fn insert_checkpoint(&mut self, checkpoint: VerifiedCheckpoint) {
        ForkingStore::insert_checkpoint(self, checkpoint)
    }

    fn insert_checkpoint_contents(&mut self, contents: CheckpointContents) {
        ForkingStore::insert_checkpoint_contents(self, contents)
    }

    fn insert_committee(&mut self, committee: Committee) {
        ForkingStore::insert_committee(self, committee)
    }

    fn insert_executed_transaction(
        &mut self,
        transaction: VerifiedTransaction,
        effects: TransactionEffects,
        events: TransactionEvents,
        written_objects: BTreeMap<ObjectID, Object>,
    ) {
        ForkingStore::insert_executed_transaction(
            self,
            transaction,
            effects,
            events,
            written_objects,
        )
    }

    fn insert_transaction(&mut self, transaction: VerifiedTransaction) {
        ForkingStore::insert_transaction(self, transaction)
    }

    fn insert_transaction_effects(&mut self, effects: TransactionEffects) {
        ForkingStore::insert_transaction_effects(self, effects)
    }

    fn insert_events(&mut self, tx_digest: &TransactionDigest, events: TransactionEvents) {
        ForkingStore::insert_events(self, tx_digest, events)
    }

    fn update_objects(
        &mut self,
        written_objects: BTreeMap<ObjectID, Object>,
        deleted_objects: Vec<(ObjectID, SequenceNumber, ObjectDigest)>,
    ) {
        ForkingStore::update_objects(self, written_objects, deleted_objects)
    }

    fn backing_store(&self) -> &dyn sui_types::storage::BackingStore {
        self
    }
}

impl ReadStore for ForkingStore {
    fn get_committee(&self, epoch: EpochId) -> Option<Arc<Committee>> {
        self.get_committee_by_epoch(epoch).cloned().map(Arc::new)
    }

    fn get_latest_checkpoint(&self) -> sui_types::storage::error::Result<VerifiedCheckpoint> {
        self.get_highest_checkpint()
            .ok_or_else(|| sui_types::storage::error::Error::custom("No checkpoint available"))
    }

    fn get_highest_verified_checkpoint(
        &self,
    ) -> sui_types::storage::error::Result<VerifiedCheckpoint> {
        self.get_latest_checkpoint()
    }

    fn get_highest_synced_checkpoint(
        &self,
    ) -> sui_types::storage::error::Result<VerifiedCheckpoint> {
        self.get_latest_checkpoint()
    }

    fn get_lowest_available_checkpoint(
        &self,
    ) -> sui_types::storage::error::Result<CheckpointSequenceNumber> {
        Ok(self
            .checkpoints
            .first_key_value()
            .map(|(sequence, _)| *sequence)
            .unwrap_or(0))
    }

    fn get_checkpoint_by_digest(&self, digest: &CheckpointDigest) -> Option<VerifiedCheckpoint> {
        ForkingStore::get_checkpoint_by_digest(self, digest)
    }

    fn get_checkpoint_by_sequence_number(
        &self,
        sequence_number: CheckpointSequenceNumber,
    ) -> Option<VerifiedCheckpoint> {
        ForkingStore::get_checkpoint_by_sequence_number(self, sequence_number)
    }

    fn get_checkpoint_contents_by_digest(
        &self,
        digest: &CheckpointContentsDigest,
    ) -> Option<CheckpointContents> {
        ForkingStore::get_checkpoint_contents_by_digest(self, digest)
    }

    fn get_checkpoint_contents_by_sequence_number(
        &self,
        sequence_number: CheckpointSequenceNumber,
    ) -> Option<CheckpointContents> {
        ForkingStore::get_checkpoint_contents_by_sequence_number(self, sequence_number)
    }

    fn get_transaction(&self, tx_digest: &TransactionDigest) -> Option<Arc<VerifiedTransaction>> {
        ForkingStore::get_transaction(self, tx_digest).map(Arc::new)
    }

    fn get_transaction_effects(&self, tx_digest: &TransactionDigest) -> Option<TransactionEffects> {
        ForkingStore::get_transaction_effects(self, tx_digest)
    }

    fn get_events(&self, tx_digest: &TransactionDigest) -> Option<TransactionEvents> {
        self.get_transaction_events(tx_digest).cloned()
    }

    fn get_unchanged_loaded_runtime_objects(
        &self,
        _digest: &TransactionDigest,
    ) -> Option<Vec<sui_types::storage::ObjectKey>> {
        None
    }

    fn get_transaction_checkpoint(
        &self,
        _digest: &TransactionDigest,
    ) -> Option<CheckpointSequenceNumber> {
        None
    }

    fn get_full_checkpoint_contents(
        &self,
        _sequence_number: Option<CheckpointSequenceNumber>,
        _digest: &CheckpointContentsDigest,
    ) -> Option<sui_types::messages_checkpoint::VersionedFullCheckpointContents> {
        None
    }
}

#[cfg(test)]
mod tests {
    use anyhow::Result;
    use sui_types::{
        base_types::{ObjectID, SequenceNumber, SuiAddress},
        object::{Object, Owner},
        test_checkpoint_data_builder::TestCheckpointBuilder,
    };
    use tempfile::TempDir;

    use super::*;
    use sui_data_store::{Node, SetupStore as _};

    const CHAIN_ID: &str = "test_chain";

    fn sample_checkpoint(sequence: u64) -> FullCheckpointData {
        TestCheckpointBuilder::new(sequence)
            .start_transaction(1)
            .create_owned_object(42)
            .finish_transaction()
            .build_checkpoint()
    }

    fn sample_object(object_id: ObjectID, owner: SuiAddress, version: u64) -> Object {
        Object::with_id_owner_version_for_testing(
            object_id,
            SequenceNumber::from_u64(version),
            Owner::AddressOwner(owner),
        )
    }

    fn make_forking_store(
        forked_at_checkpoint: u64,
    ) -> Result<(TempDir, ForkingStore), anyhow::Error> {
        let tempdir = tempfile::tempdir()?;
        let filesystem = Arc::new(FileSystemStore::new_with_path(
            Node::Testnet,
            tempdir.path().to_path_buf(),
        )?);
        filesystem.setup(Some(CHAIN_ID.to_string()))?;

        let memory = Arc::new(InMemoryStore::new(Node::Testnet));
        let graphql = Arc::new(DataStore::new(Node::Testnet, "test-version")?);
        let disk_then_graphql_objects: Arc<DiskThenGraphqlObjects> =
            Arc::new(ReadThroughStore::new(filesystem.clone(), graphql));
        let hot_objects: Arc<HotObjects> =
            Arc::new(WriteThroughStore::new(memory, disk_then_graphql_objects));

        Ok((
            tempdir,
            ForkingStore::new(forked_at_checkpoint, filesystem, hot_objects),
        ))
    }

    #[test]
    fn startup_checkpoint_data_stays_in_memory() -> Result<()> {
        let (_tempdir, mut store) = make_forking_store(11)?;
        let checkpoint = sample_checkpoint(11);

        store.insert_startup_checkpoint_data(&checkpoint)?;

        assert!(store.get_checkpoint_by_sequence_number(11).is_some());
        assert!(
            store
                .get_checkpoint_by_digest(checkpoint.summary.digest())
                .is_some()
        );
        assert!(
            store
                .get_checkpoint_contents_by_digest(checkpoint.contents.digest())
                .is_some()
        );
        assert!(
            store
                .get_checkpoint_contents_by_sequence_number(11)
                .is_some()
        );
        assert_eq!(store.transaction_count(), checkpoint.transactions.len());
        assert_eq!(store.effect_count(), checkpoint.transactions.len());

        Ok(())
    }

    #[test]
    fn get_object_prefers_local_latest_and_owned_objects_stay_filesystem_backed() -> Result<()> {
        let (_tempdir, store) = make_forking_store(50)?;
        let owner = SuiAddress::random_for_testing_only();
        let object_id = ObjectID::random();
        let checkpoint_object = sample_object(object_id, owner, 1);
        let latest_object = sample_object(object_id, owner, 3);

        store.objects.write_object(
            &ObjectKey {
                object_id,
                version_query: VersionQuery::AtCheckpoint(50),
            },
            checkpoint_object,
            1,
        )?;
        store.objects.write_object(
            &ObjectKey {
                object_id,
                version_query: VersionQuery::Version(3),
            },
            latest_object.clone(),
            3,
        )?;

        let returned = store.get_object(&object_id).expect("latest object");
        assert_eq!(returned.version().value(), 3);

        let owned = store.owned_objects(owner);
        assert_eq!(owned.len(), 1);
        assert_eq!(owned[0].id(), object_id);

        Ok(())
    }

    #[test]
    fn update_objects_persists_to_local_object_chain() -> Result<()> {
        let (_tempdir, mut store) = make_forking_store(9)?;
        let owner = SuiAddress::random_for_testing_only();
        let object_id = ObjectID::random();
        let object = sample_object(object_id, owner, 7);

        store.update_objects(BTreeMap::from([(object_id, object.clone())]), Vec::new());

        let local_latest = store
            .filesystem
            .get_object_latest(&object_id)?
            .expect("object should be in filesystem");
        assert_eq!(local_latest.0.version().value(), 7);
        assert_eq!(
            store.get_object(&object_id).expect("object lookup").id(),
            object_id
        );

        Ok(())
    }
}
