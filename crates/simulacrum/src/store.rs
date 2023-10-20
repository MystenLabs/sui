// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use move_binary_format::CompiledModule;
use move_bytecode_utils::module_cache::GetModule;
use move_core_types::{language_storage::ModuleId, resolver::ModuleResolver};
use std::collections::{BTreeMap, HashMap};
use sui_config::genesis;
use sui_types::storage::{get_module, load_package_object_from_object_store, PackageObjectArc};
use sui_types::{
    base_types::{AuthorityName, ObjectID, ObjectRef, SequenceNumber, SuiAddress},
    committee::{Committee, EpochId},
    crypto::{AccountKeyPair, AuthorityKeyPair},
    digests::{ObjectDigest, TransactionDigest, TransactionEventsDigest},
    effects::{TransactionEffects, TransactionEffectsAPI, TransactionEvents},
    error::{SuiError, SuiResult, UserInputError},
    messages_checkpoint::{
        CheckpointContents, CheckpointContentsDigest, CheckpointDigest, CheckpointSequenceNumber,
        VerifiedCheckpoint,
    },
    object::{Object, Owner},
    storage::{BackingPackageStore, ChildObjectResolver, ObjectStore, ParentSync},
    transaction::{
        InputObjectKind, InputObjects, ObjectReadResult, ReceivingObjectReadResult,
        ReceivingObjects, VerifiedTransaction,
    },
};

#[derive(Debug, Default)]
pub struct InMemoryStore {
    // Checkpoint data
    checkpoints: BTreeMap<CheckpointSequenceNumber, VerifiedCheckpoint>,
    checkpoint_digest_to_sequence_number: HashMap<CheckpointDigest, CheckpointSequenceNumber>,
    checkpoint_contents: HashMap<CheckpointContentsDigest, CheckpointContents>,

    // Transaction data
    transactions: HashMap<TransactionDigest, VerifiedTransaction>,
    effects: HashMap<TransactionDigest, TransactionEffects>,
    events: HashMap<TransactionEventsDigest, TransactionEvents>,

    // Committee data
    epoch_to_committee: Vec<Committee>,

    // Object data
    live_objects: HashMap<ObjectID, SequenceNumber>,
    objects: HashMap<ObjectID, BTreeMap<SequenceNumber, Object>>,
}

impl InMemoryStore {
    pub fn new(genesis: &genesis::Genesis) -> Self {
        let mut store = Self::default();

        store.insert_checkpoint(genesis.checkpoint());
        store.insert_checkpoint_contents(genesis.checkpoint_contents().clone());
        store.insert_committee(genesis.committee().unwrap());
        store.insert_transaction(VerifiedTransaction::new_unchecked(
            genesis.transaction().clone(),
        ));
        store.insert_transaction_effects(genesis.effects().clone());
        store.insert_events(genesis.events().clone());

        for object in genesis.objects() {
            let object_id = object.id();
            let version = object.version();
            store.live_objects.insert(object_id, version);
            store
                .objects
                .entry(object_id)
                .or_default()
                .insert(version, object.clone());
        }

        store
    }

    pub fn get_checkpoint_by_sequence_number(
        &self,
        sequence_number: CheckpointSequenceNumber,
    ) -> Option<&VerifiedCheckpoint> {
        self.checkpoints.get(&sequence_number)
    }

    pub fn get_checkpoint_by_digest(
        &self,
        digest: &CheckpointDigest,
    ) -> Option<&VerifiedCheckpoint> {
        self.checkpoint_digest_to_sequence_number
            .get(digest)
            .and_then(|sequence_number| self.get_checkpoint_by_sequence_number(*sequence_number))
    }

    pub fn get_highest_checkpint(&self) -> Option<&VerifiedCheckpoint> {
        self.checkpoints
            .last_key_value()
            .map(|(_, checkpoint)| checkpoint)
    }

    pub fn get_checkpoint_contents(
        &self,
        digest: &CheckpointContentsDigest,
    ) -> Option<&CheckpointContents> {
        self.checkpoint_contents.get(digest)
    }

    pub fn get_committee_by_epoch(&self, epoch: EpochId) -> Option<&Committee> {
        self.epoch_to_committee.get(epoch as usize)
    }

    pub fn get_transaction(&self, digest: &TransactionDigest) -> Option<&VerifiedTransaction> {
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

    pub fn get_object(&self, id: &ObjectID) -> Option<&Object> {
        let version = self.live_objects.get(id)?;
        self.get_object_at_version(id, *version)
    }

    pub fn get_object_at_version(&self, id: &ObjectID, version: SequenceNumber) -> Option<&Object> {
        self.objects
            .get(id)
            .and_then(|versions| versions.get(&version))
    }

    pub fn get_system_state(&self) -> sui_types::sui_system_state::SuiSystemState {
        sui_types::sui_system_state::get_sui_system_state(self).expect("system state must exist")
    }

    pub fn get_clock(&self) -> sui_types::clock::Clock {
        self.get_object(&sui_types::SUI_CLOCK_OBJECT_ID)
            .expect("clock should exist")
            .to_rust()
            .expect("clock object should deserialize")
    }

    pub fn owned_objects(&self, owner: SuiAddress) -> impl Iterator<Item = &Object> {
        self.live_objects
            .iter()
            .flat_map(|(id, version)| self.get_object_at_version(id, *version))
            .filter(
                move |object| matches!(object.owner, Owner::AddressOwner(addr) if addr == owner),
            )
    }
}

impl InMemoryStore {
    pub fn insert_checkpoint(&mut self, checkpoint: VerifiedCheckpoint) {
        if let Some(end_of_epoch_data) = &checkpoint.data().end_of_epoch_data {
            let next_committee = end_of_epoch_data
                .next_epoch_committee
                .iter()
                .cloned()
                .collect();
            let committee = Committee::new(checkpoint.epoch().saturating_add(1), next_committee);
            self.insert_committee(committee);
        }

        self.checkpoint_digest_to_sequence_number
            .insert(*checkpoint.digest(), *checkpoint.sequence_number());
        self.checkpoints
            .insert(*checkpoint.sequence_number(), checkpoint);
    }

    pub fn insert_checkpoint_contents(&mut self, contents: CheckpointContents) {
        self.checkpoint_contents
            .insert(*contents.digest(), contents);
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

    pub fn insert_executed_transaction(
        &mut self,
        transaction: VerifiedTransaction,
        effects: TransactionEffects,
        events: TransactionEvents,
        written_objects: BTreeMap<ObjectID, Object>,
    ) {
        let deleted_objects = effects.deleted();
        self.insert_transaction(transaction);
        self.insert_transaction_effects(effects);
        self.insert_events(events);
        self.update_objects(written_objects, deleted_objects);
    }

    pub fn insert_transaction(&mut self, transaction: VerifiedTransaction) {
        self.transactions.insert(*transaction.digest(), transaction);
    }

    pub fn insert_transaction_effects(&mut self, effects: TransactionEffects) {
        self.effects.insert(*effects.transaction_digest(), effects);
    }

    pub fn insert_events(&mut self, events: TransactionEvents) {
        self.events.insert(events.digest(), events);
    }

    pub fn update_objects(
        &mut self,
        written_objects: BTreeMap<ObjectID, Object>,
        deleted_objects: Vec<(ObjectID, SequenceNumber, ObjectDigest)>,
    ) {
        for (object_id, _, _) in deleted_objects {
            self.live_objects.remove(&object_id);
        }

        for (object_id, object) in written_objects {
            let version = object.version();
            self.live_objects.insert(object_id, version);
            self.objects
                .entry(object_id)
                .or_default()
                .insert(version, object);
        }
    }
}

impl BackingPackageStore for InMemoryStore {
    fn get_package_object(
        &self,
        package_id: &ObjectID,
    ) -> sui_types::error::SuiResult<Option<PackageObjectArc>> {
        load_package_object_from_object_store(self, package_id)
    }
}

impl ChildObjectResolver for InMemoryStore {
    fn read_child_object(
        &self,
        parent: &ObjectID,
        child: &ObjectID,
        child_version_upper_bound: SequenceNumber,
    ) -> sui_types::error::SuiResult<Option<Object>> {
        let child_object = match self.get_object(child).cloned() {
            None => return Ok(None),
            Some(obj) => obj,
        };

        let parent = *parent;
        if child_object.owner != Owner::ObjectOwner(parent.into()) {
            return Err(SuiError::InvalidChildObjectAccess {
                object: *child,
                given_parent: parent,
                actual_owner: child_object.owner,
            });
        }

        if child_object.version() > child_version_upper_bound {
            return Err(SuiError::UnsupportedFeatureError {
                error: "TODO InMemoryStorage::read_child_object does not yet support bounded reads"
                    .to_owned(),
            });
        }

        Ok(Some(child_object))
    }

    fn get_object_received_at_version(
        &self,
        owner: &ObjectID,
        receiving_object_id: &ObjectID,
        receive_object_at_version: SequenceNumber,
        _epoch_id: EpochId,
    ) -> sui_types::error::SuiResult<Option<Object>> {
        let recv_object = match self.get_object(receiving_object_id).cloned() {
            None => return Ok(None),
            Some(obj) => obj,
        };
        if recv_object.owner != Owner::AddressOwner((*owner).into()) {
            return Ok(None);
        }

        if recv_object.version() != receive_object_at_version {
            return Ok(None);
        }
        Ok(Some(recv_object))
    }
}

impl GetModule for InMemoryStore {
    type Error = SuiError;
    type Item = CompiledModule;

    fn get_module_by_id(&self, id: &ModuleId) -> Result<Option<Self::Item>, Self::Error> {
        Ok(self
            .get_module(id)?
            .map(|bytes| CompiledModule::deserialize_with_defaults(&bytes).unwrap()))
    }
}

impl ModuleResolver for InMemoryStore {
    type Error = SuiError;

    fn get_module(&self, module_id: &ModuleId) -> Result<Option<Vec<u8>>, Self::Error> {
        get_module(self, module_id)
    }
}

impl ObjectStore for InMemoryStore {
    fn get_object(
        &self,
        object_id: &ObjectID,
    ) -> Result<Option<Object>, sui_types::error::SuiError> {
        Ok(self.get_object(object_id).cloned())
    }

    fn get_object_by_key(
        &self,
        object_id: &ObjectID,
        version: sui_types::base_types::VersionNumber,
    ) -> Result<Option<Object>, sui_types::error::SuiError> {
        Ok(self.get_object_at_version(object_id, version).cloned())
    }
}

impl ParentSync for InMemoryStore {
    fn get_latest_parent_entry_ref_deprecated(
        &self,
        _object_id: ObjectID,
    ) -> sui_types::error::SuiResult<Option<sui_types::base_types::ObjectRef>> {
        panic!("Never called in newer protocol versions")
    }
}

#[derive(Debug)]
pub struct KeyStore {
    validator_keys: BTreeMap<AuthorityName, AuthorityKeyPair>,
    #[allow(unused)]
    account_keys: BTreeMap<SuiAddress, AccountKeyPair>,
}

impl KeyStore {
    pub fn from_network_config(
        network_config: &sui_swarm_config::network_config::NetworkConfig,
    ) -> Self {
        use fastcrypto::traits::KeyPair;

        let validator_keys = network_config
            .validator_configs()
            .iter()
            .map(|config| {
                (
                    config.protocol_public_key(),
                    config.protocol_key_pair().copy(),
                )
            })
            .collect();

        let account_keys = network_config
            .account_keys
            .iter()
            .map(|key| (key.public().into(), key.copy()))
            .collect();
        Self {
            validator_keys,
            account_keys,
        }
    }

    pub fn validator(&self, name: &AuthorityName) -> Option<&AuthorityKeyPair> {
        self.validator_keys.get(name)
    }

    pub fn accounts(&self) -> impl Iterator<Item = (&SuiAddress, &AccountKeyPair)> {
        self.account_keys.iter()
    }
}

// TODO: After we abstract object storage into the ExecutionCache trait, we can replace this with
// sui_core::TransactionInputLoad using an appropriate cache implementation.
impl InMemoryStore {
    pub fn read_objects_for_synchronous_execution(
        &self,
        _tx_digest: &TransactionDigest,
        input_object_kinds: &[InputObjectKind],
        receiving_object_refs: &[ObjectRef],
    ) -> SuiResult<(InputObjects, ReceivingObjects)> {
        let mut input_objects = Vec::new();
        for kind in input_object_kinds {
            let obj = match kind {
                InputObjectKind::MovePackage(id) => self.get_object(id).cloned(),
                InputObjectKind::ImmOrOwnedMoveObject(objref) => {
                    self.get_object_by_key(&objref.0, objref.1)?
                }

                InputObjectKind::SharedMoveObject { id, .. } => self.get_object(id).cloned(),
            };

            input_objects.push(ObjectReadResult::new(
                *kind,
                obj.ok_or_else(|| kind.object_not_found_error())?.into(),
            ));
        }

        let mut receiving_objects = Vec::new();
        for objref in receiving_object_refs {
            // no need for marker table check in simulacrum
            let Some(obj) = self.get_object(&objref.0).cloned() else {
                return Err(UserInputError::ObjectNotFound {
                    object_id: objref.0,
                    version: Some(objref.1),
                }
                .into());
            };
            receiving_objects.push(ReceivingObjectReadResult::new(*objref, obj.into()));
        }

        Ok((input_objects.into(), receiving_objects.into()))
    }
}

pub trait SimulatorStore:
    sui_types::storage::BackingPackageStore + sui_types::storage::ObjectStore
{
    fn get_checkpoint_by_sequence_number(
        &self,
        sequence_number: CheckpointSequenceNumber,
    ) -> Option<&VerifiedCheckpoint>;

    fn get_checkpoint_by_digest(&self, digest: &CheckpointDigest) -> Option<&VerifiedCheckpoint>;

    fn get_highest_checkpint(&self) -> Option<&VerifiedCheckpoint>;
    fn get_checkpoint_contents(
        &self,
        digest: &CheckpointContentsDigest,
    ) -> Option<&CheckpointContents>;

    fn get_committee_by_epoch(&self, epoch: EpochId) -> Option<&Committee>;

    fn get_transaction(&self, digest: &TransactionDigest) -> Option<&VerifiedTransaction>;

    fn get_transaction_effects(&self, digest: &TransactionDigest) -> Option<&TransactionEffects>;
    fn get_transaction_events(
        &self,
        digest: &TransactionEventsDigest,
    ) -> Option<&TransactionEvents>;

    fn get_object(&self, id: &ObjectID) -> Option<&Object>;
    fn get_object_at_version(&self, id: &ObjectID, version: SequenceNumber) -> Option<&Object>;

    fn get_system_state(&self) -> sui_types::sui_system_state::SuiSystemState;

    fn get_clock(&self) -> sui_types::clock::Clock;

    fn owned_objects(&self, owner: SuiAddress) -> Box<dyn Iterator<Item = &Object> + '_>;

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

    fn insert_events(&mut self, events: TransactionEvents);

    fn update_objects(
        &mut self,
        written_objects: BTreeMap<ObjectID, Object>,
        deleted_objects: Vec<(ObjectID, SequenceNumber, ObjectDigest)>,
    );
}

impl SimulatorStore for InMemoryStore {
    fn get_checkpoint_by_sequence_number(
        &self,
        sequence_number: CheckpointSequenceNumber,
    ) -> Option<&VerifiedCheckpoint> {
        self.get_checkpoint_by_sequence_number(sequence_number)
    }

    fn get_checkpoint_by_digest(&self, digest: &CheckpointDigest) -> Option<&VerifiedCheckpoint> {
        self.get_checkpoint_by_digest(digest)
    }

    fn get_highest_checkpint(&self) -> Option<&VerifiedCheckpoint> {
        self.get_highest_checkpint()
    }

    fn get_checkpoint_contents(
        &self,
        digest: &CheckpointContentsDigest,
    ) -> Option<&CheckpointContents> {
        self.get_checkpoint_contents(digest)
    }

    fn get_committee_by_epoch(&self, epoch: EpochId) -> Option<&Committee> {
        self.get_committee_by_epoch(epoch)
    }

    fn get_transaction(&self, digest: &TransactionDigest) -> Option<&VerifiedTransaction> {
        self.get_transaction(digest)
    }

    fn get_transaction_effects(&self, digest: &TransactionDigest) -> Option<&TransactionEffects> {
        self.get_transaction_effects(digest)
    }

    fn get_transaction_events(
        &self,
        digest: &TransactionEventsDigest,
    ) -> Option<&TransactionEvents> {
        self.get_transaction_events(digest)
    }

    fn get_object(&self, id: &ObjectID) -> Option<&Object> {
        self.get_object(id)
    }

    fn get_object_at_version(&self, id: &ObjectID, version: SequenceNumber) -> Option<&Object> {
        self.get_object_at_version(id, version)
    }

    fn get_system_state(&self) -> sui_types::sui_system_state::SuiSystemState {
        self.get_system_state()
    }

    fn get_clock(&self) -> sui_types::clock::Clock {
        self.get_clock()
    }

    fn owned_objects(&self, owner: SuiAddress) -> Box<dyn Iterator<Item = &Object> + '_> {
        Box::new(self.owned_objects(owner))
    }

    fn insert_checkpoint(&mut self, checkpoint: VerifiedCheckpoint) {
        self.insert_checkpoint(checkpoint)
    }

    fn insert_checkpoint_contents(&mut self, contents: CheckpointContents) {
        self.insert_checkpoint_contents(contents)
    }

    fn insert_committee(&mut self, committee: Committee) {
        self.insert_committee(committee)
    }

    fn insert_executed_transaction(
        &mut self,
        transaction: VerifiedTransaction,
        effects: TransactionEffects,
        events: TransactionEvents,
        written_objects: BTreeMap<ObjectID, Object>,
    ) {
        self.insert_executed_transaction(transaction, effects, events, written_objects)
    }

    fn insert_transaction(&mut self, transaction: VerifiedTransaction) {
        self.insert_transaction(transaction)
    }

    fn insert_transaction_effects(&mut self, effects: TransactionEffects) {
        self.insert_transaction_effects(effects)
    }

    fn insert_events(&mut self, events: TransactionEvents) {
        self.insert_events(events)
    }

    fn update_objects(
        &mut self,
        written_objects: BTreeMap<ObjectID, Object>,
        deleted_objects: Vec<(ObjectID, SequenceNumber, ObjectDigest)>,
    ) {
        self.update_objects(written_objects, deleted_objects)
    }
}
