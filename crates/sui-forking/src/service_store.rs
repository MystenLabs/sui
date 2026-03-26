// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#![allow(unused)]
use std::collections::BTreeMap;

use simulacrum::SimulatorStore;
use sui_types::base_types::ObjectID;
use sui_types::base_types::SequenceNumber;
use sui_types::base_types::SuiAddress;
use sui_types::clock::Clock;
use sui_types::committee::Committee;
use sui_types::committee::EpochId;
use sui_types::digests::ObjectDigest;
use sui_types::digests::TransactionDigest;
use sui_types::effects::TransactionEffects;
use sui_types::effects::TransactionEvents;
use sui_types::messages_checkpoint::CheckpointContents;
use sui_types::messages_checkpoint::CheckpointContentsDigest;
use sui_types::messages_checkpoint::CheckpointDigest;
use sui_types::messages_checkpoint::CheckpointSequenceNumber;
use sui_types::messages_checkpoint::VerifiedCheckpoint;
use sui_types::object::Object;
use sui_types::object::Owner;
use sui_types::storage::BackingPackageStore;
use sui_types::storage::ChildObjectResolver;
use sui_types::storage::ObjectStore;
use sui_types::storage::PackageObject;
use sui_types::storage::ParentSync;
use sui_types::storage::load_package_object_from_object_store;
use sui_types::sui_system_state::SuiSystemState;
use sui_types::sui_system_state::sui_system_state_inner_v1::ValidatorSetV1;
use sui_types::transaction::VerifiedTransaction;

/// Persistent store backing the forked network.
///
/// `ServiceStore` implements [`SimulatorStore`] so it can be used as the storage layer for a
/// [`Simulacrum`](simulacrum::Simulacrum) instance. It records the checkpoint at which the
/// fork was created and will serve objects and transactions from a combination of local state
/// (for post-fork data) and remote RPC reads (for pre-fork data fetched on demand).
pub struct ServiceStore {
    // The checkpoint at which this forked network was forked
    forked_at_checkpoint: u64,
}

impl ServiceStore {
    /// Creates a forking store with local cache/store chains already composed.
    pub fn new(forked_at_checkpoint: u64) -> Self {
        Self {
            forked_at_checkpoint,
        }
    }

    /// Returns checkpoint summary by sequence from the local memory/filesystem path.
    pub fn get_checkpoint_by_sequence_number(
        &self,
        sequence_number: CheckpointSequenceNumber,
    ) -> Option<VerifiedCheckpoint> {
        todo!()
    }

    /// Returns checkpoint summary by digest via local digest index and sequence read.
    pub fn get_checkpoint_by_digest(
        &self,
        digest: &CheckpointDigest,
    ) -> Option<VerifiedCheckpoint> {
        todo!()
    }

    /// Returns checkpoint contents by sequence from the local memory/filesystem path.
    pub fn get_checkpoint_contents_by_sequence_number(
        &self,
        sequence_number: CheckpointSequenceNumber,
    ) -> Option<CheckpointContents> {
        todo!()
    }

    /// Returns checkpoint contents by digest via local digest index and sequence read.
    pub fn get_checkpoint_contents_by_digest(
        &self,
        digest: &CheckpointContentsDigest,
    ) -> Option<CheckpointContents> {
        todo!()
    }

    /// Returns the latest locally available checkpoint summary from the local checkpoint path.
    pub fn get_highest_checkpint(&self) -> Option<VerifiedCheckpoint> {
        todo!()
    }

    /// Returns committee metadata for an epoch, if known in-memory.
    pub fn get_committee_by_epoch(&self, epoch: EpochId) -> Option<&Committee> {
        todo!()
    }

    /// Returns the transaction by digest from the local transaction path.
    pub fn get_transaction(&self, digest: &TransactionDigest) -> Option<VerifiedTransaction> {
        todo!()
    }

    /// Returns transaction effects by digest from the local transaction path.
    pub fn get_transaction_effects(
        &self,
        digest: &TransactionDigest,
    ) -> Option<TransactionEffects> {
        todo!()
    }

    /// Returns in-memory transaction events by transaction digest.
    pub fn get_transaction_events(&self, digest: &TransactionDigest) -> Option<&TransactionEvents> {
        todo!()
    }

    /// Tries to fetch the object at the latest version, and if not found, it will fetch it from
    /// RPC at the forked checkpoint.
    pub fn get_object(&self, id: &ObjectID) -> Option<Object> {
        todo!()
    }

    /// Returns an object at an exact version using read-through object fetch.
    pub fn get_object_at_version(&self, id: &ObjectID, version: SequenceNumber) -> Option<Object> {
        todo!()
    }

    /// Returns the current system state view derived from this store.
    /// Importantly, if `validator_set_override` is set, it will be used in place of the on-chain
    /// validator set for epoch state construction. This allows the forking store to keep the
    /// system state up-to-date with the locally available validator set.
    pub fn get_system_state(&self) -> SuiSystemState {
        todo!()
    }

    /// Gets the clock object, which should always be present in the store since it's a system
    /// object. Panics if not found or fails to deserialize.
    pub fn get_clock(&self) -> Clock {
        todo!()
    }

    /// Returns all locally cached objects currently owned by an address.
    pub fn owned_objects(&self, owner: SuiAddress) -> Vec<Object> {
        todo!()
    }

    /// Installs a validator-set override used by `get_system_state` for epoch-state derivation.
    pub fn set_system_state_validator_set_override(&mut self, validators: ValidatorSetV1) {
        todo!()
    }
}

impl ServiceStore {
    /// Records checkpoint summary state and updates committee map on epoch transitions.
    /// The matching contents are expected in a later `insert_checkpoint_contents` call.
    pub fn insert_checkpoint(&mut self, checkpoint: VerifiedCheckpoint) {
        todo!()
    }

    /// Completes a pending checkpoint and persists full checkpoint payload for post-fork sequences.
    pub fn insert_checkpoint_contents(&mut self, contents: CheckpointContents) {
        todo!()
    }

    /// Inserts committee info for an epoch if not already present.
    pub fn insert_committee(&mut self, committee: Committee) {
        todo!()
    }

    /// Inserts the transaction, its effects, events, and the written objects into the store.
    pub fn insert_executed_transaction(
        &mut self,
        transaction: VerifiedTransaction,
        effects: TransactionEffects,
        events: TransactionEvents,
        written_objects: BTreeMap<ObjectID, Object>,
    ) {
        todo!()
    }

    /// Placeholder for direct transaction insertion; currently unused in forking mode.
    pub fn insert_transaction(&mut self, transaction: VerifiedTransaction) {
        todo!()
    }

    /// Placeholder for direct effects insertion; currently unused in forking mode.
    pub fn insert_transaction_effects(&mut self, effects: TransactionEffects) {
        todo!()
    }

    /// Stores transaction events in-memory.
    pub fn insert_events(&mut self, tx_digest: &TransactionDigest, events: TransactionEvents) {
        todo!()
    }

    /// Placeholder for object update path; currently unused.
    pub fn update_objects(
        &mut self,
        written_objects: BTreeMap<ObjectID, Object>,
        _deleted_objects: Vec<(ObjectID, SequenceNumber, ObjectDigest)>,
    ) {
        todo!()
    }
}

impl BackingPackageStore for ServiceStore {
    fn get_package_object(
        &self,
        package_id: &ObjectID,
    ) -> sui_types::error::SuiResult<Option<PackageObject>> {
        load_package_object_from_object_store(self, package_id)
    }
}

impl ChildObjectResolver for ServiceStore {
    fn read_child_object(
        &self,
        parent: &ObjectID,
        child: &ObjectID,
        child_version_upper_bound: SequenceNumber,
    ) -> sui_types::error::SuiResult<Option<Object>> {
        todo!()
    }

    fn get_object_received_at_version(
        &self,
        owner: &ObjectID,
        receiving_object_id: &ObjectID,
        receive_object_at_version: SequenceNumber,
        _epoch_id: EpochId,
    ) -> sui_types::error::SuiResult<Option<Object>> {
        let recv_object = match self.get_object(receiving_object_id) {
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

impl ObjectStore for ServiceStore {
    fn get_object(&self, object_id: &ObjectID) -> Option<Object> {
        self.get_object(object_id)
    }

    fn get_object_by_key(
        &self,
        object_id: &ObjectID,
        version: sui_types::base_types::VersionNumber,
    ) -> Option<Object> {
        self.get_object_at_version(object_id, version)
    }
}

impl ParentSync for ServiceStore {
    fn get_latest_parent_entry_ref_deprecated(
        &self,
        _object_id: ObjectID,
    ) -> Option<sui_types::base_types::ObjectRef> {
        panic!("Never called in newer protocol versions")
    }
}

impl SimulatorStore for ServiceStore {
    fn get_checkpoint_by_sequence_number(
        &self,
        sequence_number: CheckpointSequenceNumber,
    ) -> Option<VerifiedCheckpoint> {
        ServiceStore::get_checkpoint_by_sequence_number(self, sequence_number)
    }

    fn get_checkpoint_by_digest(&self, digest: &CheckpointDigest) -> Option<VerifiedCheckpoint> {
        ServiceStore::get_checkpoint_by_digest(self, digest)
    }

    fn get_highest_checkpint(&self) -> Option<VerifiedCheckpoint> {
        ServiceStore::get_highest_checkpint(self)
    }

    fn get_checkpoint_contents(
        &self,
        digest: &CheckpointContentsDigest,
    ) -> Option<CheckpointContents> {
        ServiceStore::get_checkpoint_contents_by_digest(self, digest)
    }

    fn get_committee_by_epoch(&self, epoch: EpochId) -> Option<Committee> {
        self.get_committee_by_epoch(epoch).cloned()
    }

    fn get_transaction(&self, digest: &TransactionDigest) -> Option<VerifiedTransaction> {
        self.get_transaction(digest)
    }

    fn get_transaction_effects(&self, digest: &TransactionDigest) -> Option<TransactionEffects> {
        self.get_transaction_effects(digest)
    }

    fn get_transaction_events(&self, digest: &TransactionDigest) -> Option<TransactionEvents> {
        self.get_transaction_events(digest).cloned()
    }

    fn get_object(&self, id: &ObjectID) -> Option<Object> {
        self.get_object(id)
    }

    fn get_object_at_version(&self, id: &ObjectID, version: SequenceNumber) -> Option<Object> {
        self.get_object_at_version(id, version)
    }

    fn get_system_state(&self) -> SuiSystemState {
        self.get_system_state()
    }

    fn get_clock(&self) -> Clock {
        self.get_clock()
    }

    fn owned_objects(&self, owner: SuiAddress) -> Box<dyn Iterator<Item = Object> + '_> {
        Box::new(self.owned_objects(owner).into_iter())
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

    fn insert_events(&mut self, tx_digest: &TransactionDigest, events: TransactionEvents) {
        self.insert_events(tx_digest, events)
    }

    fn update_objects(
        &mut self,
        written_objects: BTreeMap<ObjectID, Object>,
        deleted_objects: Vec<(ObjectID, SequenceNumber, ObjectDigest)>,
    ) {
        self.update_objects(written_objects, deleted_objects)
    }

    fn backing_store(&self) -> &dyn sui_types::storage::BackingStore {
        self
    }
}
