// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::base_types::{EpochId, ObjectDigest, ObjectRef, TransactionDigest};
use crate::digests::TransactionEventsDigest;
use crate::effects::{TransactionEffectsAPI, TransactionEffectsDebugSummary};
use crate::execution_status::ExecutionStatus;
use crate::gas::GasCostSummary;
use crate::object::Owner;
use crate::storage::{DeleteKind, WriteKind};
use crate::{ObjectID, SequenceNumber};
use serde::{Deserialize, Serialize};
use std::cell::OnceCell;
use std::collections::HashSet;

/// The response from processing a transaction or a certified transaction
#[derive(Eq, PartialEq, Clone, Debug, Serialize, Deserialize)]
pub struct TransactionEffectsV2 {
    /// The status of the execution
    status: ExecutionStatus,
    /// The epoch when this transaction was executed.
    executed_epoch: EpochId,
    gas_used: GasCostSummary,
    /// The transaction digest
    transaction_digest: TransactionDigest,
    /// The updated gas object reference, as an index into the `changed_objects` vector.
    /// Having a dedicated field for convenient access.
    gas_object_index: u16,
    /// The digest of the events emitted during execution,
    /// can be None if the transaction does not emit any event.
    events_digest: Option<TransactionEventsDigest>,
    /// The set of transaction digests this transaction depends on.
    dependencies: Vec<TransactionDigest>,

    /// The version number of all the written Move objects by this transaction.
    lamport_version: SequenceNumber,
    /// Objects whose state are changed in the object store.
    changed_objects: Vec<(ObjectID, ObjectChange)>,
    /// Shared objects that are not mutated in this transaction. Unlike owned objects,
    /// read-only shared objects' version are not committed in the transaction,
    /// and in order for a node to catch up and execute it without consensus sequencing,
    /// the version needs to be committed in the effects.
    unchanged_shared_objects: Vec<(ObjectID, UnchangedSharedKind)>,

    /// This is just to satisfy the current TransactionEffectsAPI.
    /// TODO: We should just change gas_object() to return value instead of reference.
    #[serde(skip)]
    _cached_gas_object: OnceCell<(ObjectRef, Owner)>,
}

impl TransactionEffectsAPI for TransactionEffectsV2 {
    fn status(&self) -> &ExecutionStatus {
        &self.status
    }

    fn into_status(self) -> ExecutionStatus {
        self.status
    }

    fn executed_epoch(&self) -> EpochId {
        self.executed_epoch
    }

    fn modified_at_versions(&self) -> &[(ObjectID, SequenceNumber)] {
        unimplemented!()
    }

    fn shared_objects(&self) -> &[ObjectRef] {
        unimplemented!()
    }

    fn created(&self) -> &[(ObjectRef, Owner)] {
        unimplemented!()
    }

    fn mutated(&self) -> &[(ObjectRef, Owner)] {
        unimplemented!()
    }

    fn unwrapped(&self) -> &[(ObjectRef, Owner)] {
        unimplemented!()
    }

    fn deleted(&self) -> &[ObjectRef] {
        unimplemented!()
    }

    fn unwrapped_then_deleted(&self) -> &[ObjectRef] {
        unimplemented!()
    }

    fn wrapped(&self) -> &[ObjectRef] {
        unimplemented!()
    }

    fn gas_object(&self) -> &(ObjectRef, Owner) {
        self._cached_gas_object.get_or_init(|| {
            let entry = &self.changed_objects[self.gas_object_index as usize];
            match entry.1.output_state {
                ObjectOut::ObjectWrite(digest, owner) => {
                    ((entry.0, self.lamport_version, digest), owner)
                }
                _ => panic!("Gas object must be an ObjectWrite in changed_objects"),
            }
        })
    }

    fn events_digest(&self) -> Option<&TransactionEventsDigest> {
        self.events_digest.as_ref()
    }

    fn dependencies(&self) -> &[TransactionDigest] {
        &self.dependencies
    }

    fn all_changed_objects(&self) -> Vec<(&ObjectRef, &Owner, WriteKind)> {
        unimplemented!()
    }

    fn all_deleted(&self) -> Vec<(&ObjectRef, DeleteKind)> {
        unimplemented!()
    }

    fn transaction_digest(&self) -> &TransactionDigest {
        &self.transaction_digest
    }

    fn mutated_excluding_gas(&self) -> Vec<&(ObjectRef, Owner)> {
        unimplemented!()
    }

    fn gas_cost_summary(&self) -> &GasCostSummary {
        &self.gas_used
    }

    fn summary_for_debug(&self) -> TransactionEffectsDebugSummary {
        unimplemented!()
    }

    fn status_mut_for_testing(&mut self) -> &mut ExecutionStatus {
        &mut self.status
    }

    fn gas_cost_summary_mut_for_testing(&mut self) -> &mut GasCostSummary {
        &mut self.gas_used
    }

    fn transaction_digest_mut_for_testing(&mut self) -> &mut TransactionDigest {
        &mut self.transaction_digest
    }

    fn dependencies_mut_for_testing(&mut self) -> &mut Vec<TransactionDigest> {
        &mut self.dependencies
    }

    fn shared_objects_mut_for_testing(&mut self) -> &mut Vec<ObjectRef> {
        unimplemented!()
    }

    fn modified_at_versions_mut_for_testing(&mut self) -> &mut Vec<(ObjectID, SequenceNumber)> {
        unimplemented!()
    }
}

impl TransactionEffectsV2 {
    #[cfg(debug_assertions)]
    /// This function demonstrates what's the invariant of the effects.
    /// It also documents the semantics of different combinations in object changes.
    /// TODO: It will be called in the constructor of `TransactionEffectsV2` in the future.
    fn _check_invariant(&self) {
        let mut unique_ids = HashSet::new();
        for (id, change) in &self.changed_objects {
            assert!(unique_ids.insert(*id));
            match (
                &change.input_state,
                &change.output_state,
                &change.id_operation,
            ) {
                (ObjectIn::NotExist, ObjectOut::NotExist, IDOperation::Created) => {
                    // created and then wrapped Move object.
                }
                (ObjectIn::NotExist, ObjectOut::NotExist, IDOperation::Deleted) => {
                    // unwrapped and then deleted Move object.
                }
                (ObjectIn::NotExist, ObjectOut::ObjectWrite(_, owner), IDOperation::None) => {
                    // unwrapped Move object.
                    // It's not allowed to make an object shared after unwrapping.
                    assert!(!owner.is_shared());
                }
                (ObjectIn::NotExist, ObjectOut::ObjectWrite(..), IDOperation::Created) => {
                    // created Move object.
                }
                (ObjectIn::NotExist, ObjectOut::PackageWrite(_), IDOperation::Created) => {
                    // created Move package or user Move package upgrade.
                }
                (ObjectIn::Exist(_), ObjectOut::NotExist, IDOperation::None) => {
                    // wrapped.
                }
                (ObjectIn::Exist(_), ObjectOut::NotExist, IDOperation::Deleted) => {
                    // deleted.
                }
                (
                    ObjectIn::Exist((old_version, old_digest)),
                    ObjectOut::ObjectWrite(new_digest, _),
                    IDOperation::None,
                ) => {
                    // mutated.
                    assert!(old_version.value() < self.lamport_version.value());
                    assert_ne!(old_digest, new_digest);
                }
                (
                    ObjectIn::Exist((old_version, _)),
                    ObjectOut::PackageWrite((new_version, _)),
                    IDOperation::None,
                ) => {
                    // system package upgrade.
                    assert_eq!(old_version.value() + 1, new_version.value());
                }
                _ => {
                    panic!("Impossible object change: {:?}, {:?}", id, change);
                }
            }
        }
        // Make sure that gas object exists in changed_objects.
        let (_, owner) = self.gas_object();
        assert!(matches!(owner, Owner::AddressOwner(_)));

        for (id, _) in &self.unchanged_shared_objects {
            assert!(unique_ids.insert(*id));
        }
    }
}

#[derive(Eq, PartialEq, Clone, Debug, Serialize, Deserialize)]
struct ObjectChange {
    // input_state and output_state are the core fields that's required by
    // the protocol as it tells how an object changes on-chain.
    /// State of the object in the store prior to this transaction.
    input_state: ObjectIn,
    /// State of the object in the store after this transaction.
    output_state: ObjectOut,

    /// Whether this object ID is created or deleted in this transaction.
    /// This information isn't required by the protocol but is useful for providing more detailed
    /// semantics on object changes.
    id_operation: IDOperation,
}

#[derive(Eq, PartialEq, Clone, Debug, Serialize, Deserialize)]
enum IDOperation {
    None,
    Created,
    Deleted,
}

type VersionDigest = (SequenceNumber, ObjectDigest);

/// If an object exists (at root-level) in the store prior to this transaction,
/// it should be Exist, otherwise it's NonExist, e.g. wrapped objects should be
/// NonExist.
#[derive(Eq, PartialEq, Clone, Debug, Serialize, Deserialize)]
enum ObjectIn {
    NotExist,
    Exist(VersionDigest),
}

#[derive(Eq, PartialEq, Clone, Debug, Serialize, Deserialize)]
enum ObjectOut {
    /// Same definition as in ObjectIn.
    NotExist,
    /// Any written object, including all of mutated, created, unwrapped today.
    ObjectWrite(ObjectDigest, Owner),
    /// Packages writes need to be tracked separately with version because
    /// we don't use lamport version for package publish and upgrades.
    PackageWrite(VersionDigest),
}

#[derive(Eq, PartialEq, Clone, Debug, Serialize, Deserialize)]
enum UnchangedSharedKind {
    /// Read-only shared objects from the input. We don't really need ObjectDigest
    /// for protocol correctness, but it will make it easier to verify untrusted read.
    ReadOnlyRoot(VersionDigest),
    /// Child objects of read-only shared objects. We don't need this for protocol correctness,
    /// but having it would make debugging a lot easier.
    ReadOnlyChild(VersionDigest),
    /// Already deleted shared objects.
    Deleted(SequenceNumber),
}
