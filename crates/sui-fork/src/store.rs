// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::RwLock;

use move_core_types::language_storage::ModuleId;
use move_core_types::resolver::ModuleResolver;
use simulacrum::store::SimulatorStore;
use sui_data_store::stores::DataStore;
use sui_data_store::{ObjectKey, ObjectStore as RemoteObjectStore, VersionQuery};
use sui_types::base_types::{ObjectID, SequenceNumber, SuiAddress};
use sui_types::committee::{Committee, EpochId};
use sui_types::digests::{ObjectDigest, TransactionDigest};
use sui_types::effects::{TransactionEffects, TransactionEffectsAPI, TransactionEvents};
use sui_types::error::SuiErrorKind;
use sui_types::messages_checkpoint::{
    CheckpointContents, CheckpointContentsDigest, CheckpointDigest, CheckpointSequenceNumber,
    VerifiedCheckpoint,
};
use sui_types::object::{Data, Object, Owner};
use sui_types::storage::{
    BackingPackageStore, BackingStore, ChildObjectResolver, ObjectStore, PackageObject, ParentSync,
    load_package_object_from_object_store,
};
use sui_types::transaction::VerifiedTransaction;
use sui_types::{SUI_CLOCK_OBJECT_ID, sui_system_state::SuiSystemState};

/// Cloneable local state for snapshots.
#[derive(Clone, Default)]
pub struct LocalState {
    pub checkpoints: BTreeMap<CheckpointSequenceNumber, VerifiedCheckpoint>,
    pub checkpoint_digest_to_seq: HashMap<CheckpointDigest, CheckpointSequenceNumber>,
    pub checkpoint_contents: HashMap<CheckpointContentsDigest, CheckpointContents>,
    pub transactions: HashMap<TransactionDigest, VerifiedTransaction>,
    pub effects: HashMap<TransactionDigest, TransactionEffects>,
    pub events: HashMap<TransactionDigest, TransactionEvents>,
    pub epoch_to_committee: Vec<Committee>,
    pub live_objects: HashMap<ObjectID, SequenceNumber>,
    pub objects: HashMap<ObjectID, BTreeMap<SequenceNumber, Object>>,
    /// Tracks objects deleted locally so remote fallback doesn't resurrect them.
    pub deleted_objects: HashSet<ObjectID>,
}

/// A store that reads from local in-memory state first, then falls back to the remote
/// GraphQL-backed DataStore for objects not yet cached locally.
/// Writes go to local state only; the remote network is never modified.
pub struct ForkedStore {
    pub(crate) local: RwLock<LocalState>,
    pub(crate) remote: DataStore,
    pub(crate) fork_checkpoint: u64,
}

impl ForkedStore {
    pub fn new(remote: DataStore, fork_checkpoint: u64) -> Self {
        Self {
            local: RwLock::new(LocalState::default()),
            remote,
            fork_checkpoint,
        }
    }

    /// Insert an object directly into local state (used during bootstrap seeding).
    pub fn insert_object(&self, obj: Object) {
        let version = obj.version();
        let id = obj.id();
        let mut local = self.local.write().unwrap();
        // If this object was previously deleted, remove that tombstone — the object exists now.
        local.deleted_objects.remove(&id);
        local.live_objects.insert(id, version);
        local.objects.entry(id).or_default().insert(version, obj);
    }

    /// Clone the current local state for snapshotting.
    pub fn snapshot_local(&self) -> LocalState {
        self.local.read().unwrap().clone()
    }

    /// Restore local state from a snapshot.
    pub fn restore_local(&self, state: LocalState) {
        *self.local.write().unwrap() = state;
    }
}

impl ObjectStore for ForkedStore {
    fn get_object(&self, object_id: &ObjectID) -> Option<Object> {
        // Check local cache first.
        {
            let local = self.local.read().unwrap();
            // Return None for locally deleted objects without hitting remote.
            if local.deleted_objects.contains(object_id) {
                return None;
            }
            if let Some(&version) = local.live_objects.get(object_id)
                && let Some(obj) = local.objects.get(object_id).and_then(|v| v.get(&version))
            {
                return Some(obj.clone());
            }
        }
        // Fetch from remote at the fork checkpoint.
        let key = ObjectKey {
            object_id: *object_id,
            version_query: VersionQuery::AtCheckpoint(self.fork_checkpoint),
        };
        let results = self.remote.get_objects(&[key]).ok()?;
        let (obj, version) = results.into_iter().next().flatten()?;
        let seq = SequenceNumber::from(version);
        let mut local = self.local.write().unwrap();
        local.live_objects.insert(*object_id, seq);
        local
            .objects
            .entry(*object_id)
            .or_default()
            .insert(seq, obj.clone());
        Some(obj)
    }

    fn get_object_by_key(
        &self,
        object_id: &ObjectID,
        version: sui_types::base_types::VersionNumber,
    ) -> Option<Object> {
        // Check local cache first.
        {
            let local = self.local.read().unwrap();
            // Return None for locally deleted objects without hitting remote.
            if local.deleted_objects.contains(object_id) {
                return None;
            }
            if let Some(obj) = local
                .objects
                .get(object_id)
                .and_then(|v| v.get(&version))
            {
                return Some(obj.clone());
            }
        }
        // Fetch from remote at the specific version.
        let key = ObjectKey {
            object_id: *object_id,
            version_query: VersionQuery::Version(version.value()),
        };
        let results = self.remote.get_objects(&[key]).ok()?;
        let (obj, fetched_version) = results.into_iter().next().flatten()?;
        let seq = SequenceNumber::from(fetched_version);
        let mut local = self.local.write().unwrap();
        local
            .objects
            .entry(*object_id)
            .or_default()
            .insert(seq, obj.clone());
        // Only set live version if not already tracked.
        local.live_objects.entry(*object_id).or_insert(seq);
        Some(obj)
    }
}

impl BackingPackageStore for ForkedStore {
    fn get_package_object(
        &self,
        package_id: &ObjectID,
    ) -> sui_types::error::SuiResult<Option<PackageObject>> {
        load_package_object_from_object_store(self, package_id)
    }
}

impl ChildObjectResolver for ForkedStore {
    fn read_child_object(
        &self,
        parent: &ObjectID,
        child: &ObjectID,
        child_version_upper_bound: SequenceNumber,
    ) -> sui_types::error::SuiResult<Option<Object>> {
        let child_object = match ObjectStore::get_object(self, child) {
            None => return Ok(None),
            Some(obj) => obj,
        };
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
                error: "ForkedStore does not support bounded child object reads".to_owned(),
            }
            .into());
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
        let recv_object = match ObjectStore::get_object(self, receiving_object_id) {
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

impl ParentSync for ForkedStore {
    fn get_latest_parent_entry_ref_deprecated(
        &self,
        _object_id: ObjectID,
    ) -> Option<sui_types::base_types::ObjectRef> {
        None
    }
}

impl SimulatorStore for ForkedStore {
    fn get_checkpoint_by_sequence_number(
        &self,
        sequence_number: CheckpointSequenceNumber,
    ) -> Option<VerifiedCheckpoint> {
        self.local
            .read()
            .unwrap()
            .checkpoints
            .get(&sequence_number)
            .cloned()
    }

    fn get_checkpoint_by_digest(&self, digest: &CheckpointDigest) -> Option<VerifiedCheckpoint> {
        let local = self.local.read().unwrap();
        local
            .checkpoint_digest_to_seq
            .get(digest)
            .and_then(|seq| local.checkpoints.get(seq))
            .cloned()
    }

    fn get_highest_checkpint(&self) -> Option<VerifiedCheckpoint> {
        self.local
            .read()
            .unwrap()
            .checkpoints
            .last_key_value()
            .map(|(_, c)| c.clone())
    }

    fn get_checkpoint_contents(
        &self,
        digest: &CheckpointContentsDigest,
    ) -> Option<CheckpointContents> {
        self.local
            .read()
            .unwrap()
            .checkpoint_contents
            .get(digest)
            .cloned()
    }

    fn get_committee_by_epoch(&self, epoch: EpochId) -> Option<Committee> {
        self.local
            .read()
            .unwrap()
            .epoch_to_committee
            .get(epoch as usize)
            .cloned()
    }

    fn get_transaction(&self, digest: &TransactionDigest) -> Option<VerifiedTransaction> {
        self.local
            .read()
            .unwrap()
            .transactions
            .get(digest)
            .cloned()
    }

    fn get_transaction_effects(&self, digest: &TransactionDigest) -> Option<TransactionEffects> {
        self.local.read().unwrap().effects.get(digest).cloned()
    }

    fn get_transaction_events(&self, digest: &TransactionDigest) -> Option<TransactionEvents> {
        self.local.read().unwrap().events.get(digest).cloned()
    }

    fn get_object(&self, id: &ObjectID) -> Option<Object> {
        ObjectStore::get_object(self, id)
    }

    fn get_object_at_version(&self, id: &ObjectID, version: SequenceNumber) -> Option<Object> {
        self.get_object_by_key(id, version)
    }

    fn get_system_state(&self) -> SuiSystemState {
        sui_types::sui_system_state::get_sui_system_state(self)
            .expect("system state must exist")
    }

    fn get_clock(&self) -> sui_types::clock::Clock {
        ObjectStore::get_object(self, &SUI_CLOCK_OBJECT_ID)
            .expect("clock must exist")
            .to_rust()
            .expect("clock object should deserialize")
    }

    fn owned_objects(&self, owner: SuiAddress) -> Box<dyn Iterator<Item = Object> + '_> {
        let local = self.local.read().unwrap();
        let objects: Vec<Object> = local
            .live_objects
            .iter()
            .flat_map(|(id, version)| {
                local
                    .objects
                    .get(id)
                    .and_then(|v| v.get(version))
                    .cloned()
            })
            .filter(
                move |obj| matches!(&obj.owner, Owner::AddressOwner(addr) if *addr == owner),
            )
            .collect();
        Box::new(objects.into_iter())
    }

    fn insert_checkpoint(&mut self, checkpoint: VerifiedCheckpoint) {
        let local = self.local.get_mut().unwrap();
        if let Some(end_of_epoch_data) = &checkpoint.data().end_of_epoch_data {
            let next_committee = end_of_epoch_data
                .next_epoch_committee
                .iter()
                .cloned()
                .collect();
            let committee =
                Committee::new(checkpoint.epoch().checked_add(1).unwrap(), next_committee);
            let epoch = committee.epoch as usize;
            if local.epoch_to_committee.get(epoch).is_none()
                && local.epoch_to_committee.len() == epoch
            {
                local.epoch_to_committee.push(committee);
            }
        }
        local
            .checkpoint_digest_to_seq
            .insert(*checkpoint.digest(), *checkpoint.sequence_number());
        local
            .checkpoints
            .insert(*checkpoint.sequence_number(), checkpoint);
    }

    fn insert_checkpoint_contents(&mut self, contents: CheckpointContents) {
        self.local
            .get_mut()
            .unwrap()
            .checkpoint_contents
            .insert(*contents.digest(), contents);
    }

    fn insert_committee(&mut self, committee: Committee) {
        let local = self.local.get_mut().unwrap();
        let epoch = committee.epoch as usize;
        if local.epoch_to_committee.get(epoch).is_some() {
            return;
        }
        if local.epoch_to_committee.len() == epoch {
            local.epoch_to_committee.push(committee);
        }
    }

    fn insert_executed_transaction(
        &mut self,
        transaction: VerifiedTransaction,
        effects: TransactionEffects,
        events: TransactionEvents,
        written_objects: BTreeMap<ObjectID, Object>,
    ) {
        let deleted_objects = effects.deleted();
        let tx_digest = *effects.transaction_digest();
        let local = self.local.get_mut().unwrap();
        local.transactions.insert(*transaction.digest(), transaction);
        local
            .effects
            .insert(*effects.transaction_digest(), effects);
        local.events.insert(tx_digest, events);
        for (object_id, _, _) in deleted_objects {
            local.live_objects.remove(&object_id);
            local.deleted_objects.insert(object_id);
        }
        for (object_id, object) in written_objects {
            let version = object.version();
            // Writing an object clears any prior deletion tombstone for that ID.
            local.deleted_objects.remove(&object_id);
            local.live_objects.insert(object_id, version);
            local
                .objects
                .entry(object_id)
                .or_default()
                .insert(version, object);
        }
    }

    fn insert_transaction(&mut self, transaction: VerifiedTransaction) {
        self.local
            .get_mut()
            .unwrap()
            .transactions
            .insert(*transaction.digest(), transaction);
    }

    fn insert_transaction_effects(&mut self, effects: TransactionEffects) {
        self.local
            .get_mut()
            .unwrap()
            .effects
            .insert(*effects.transaction_digest(), effects);
    }

    fn insert_events(&mut self, tx_digest: &TransactionDigest, events: TransactionEvents) {
        self.local
            .get_mut()
            .unwrap()
            .events
            .insert(*tx_digest, events);
    }

    fn update_objects(
        &mut self,
        written_objects: BTreeMap<ObjectID, Object>,
        deleted_objects: Vec<(ObjectID, SequenceNumber, ObjectDigest)>,
    ) {
        let local = self.local.get_mut().unwrap();
        for (object_id, _, _) in deleted_objects {
            local.live_objects.remove(&object_id);
            local.deleted_objects.insert(object_id);
        }
        for (object_id, object) in written_objects {
            let version = object.version();
            local.deleted_objects.remove(&object_id);
            local.live_objects.insert(object_id, version);
            local
                .objects
                .entry(object_id)
                .or_default()
                .insert(version, object);
        }
    }

    fn backing_store(&self) -> &dyn BackingStore {
        self
    }
}

impl ModuleResolver for ForkedStore {
    type Error = anyhow::Error;

    fn get_module(&self, id: &ModuleId) -> Result<Option<Vec<u8>>, Self::Error> {
        let pkg_id = ObjectID::from(*id.address());
        let Some(obj) = ObjectStore::get_object(self, &pkg_id) else {
            return Ok(None);
        };
        let Data::Package(ref pkg) = obj.data else {
            return Ok(None);
        };
        Ok(pkg.serialized_module_map().get(id.name().as_str()).cloned())
    }
}
