// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::base_types::{EpochId, ObjectDigest, ObjectRef, TransactionDigest};
use crate::digests::TransactionEventsDigest;
use crate::effects::{InputSharedObjectKind, TransactionEffectsAPI};
use crate::execution_status::ExecutionStatus;
use crate::gas::GasCostSummary;
use crate::object::Owner;
use crate::{ObjectID, SequenceNumber};
use serde::{Deserialize, Serialize};
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

    // TODO: Add a new API to return modified object refs.
    fn modified_at_versions(&self) -> Vec<(ObjectID, SequenceNumber)> {
        self.changed_objects
            .iter()
            .filter_map(|(id, change)| {
                if let ObjectIn::Exist((version, _digest)) = &change.input_state {
                    Some((*id, *version))
                } else {
                    None
                }
            })
            .collect()
    }

    fn input_shared_objects(&self) -> Vec<(ObjectRef, InputSharedObjectKind)> {
        self.changed_objects
            .iter()
            .filter_map(
                |(id, change)| match (&change.input_state, &change.output_state) {
                    (
                        ObjectIn::Exist((version, digest)),
                        ObjectOut::ObjectWrite(_, Owner::Shared { .. }),
                    ) => Some(((*id, *version, *digest), InputSharedObjectKind::Mutate)),
                    _ => None,
                },
            )
            .chain(
                self.unchanged_shared_objects
                    .iter()
                    .filter_map(|(id, change_kind)| match change_kind {
                        UnchangedSharedKind::ReadOnlyRoot((version, digest)) => {
                            Some(((*id, *version, *digest), InputSharedObjectKind::ReadOnly))
                        }
                        UnchangedSharedKind::ReadOnlyChild(_) => None,
                    }),
            )
            .collect()
    }

    fn created(&self) -> Vec<(ObjectRef, Owner)> {
        self.changed_objects
            .iter()
            .filter_map(|(id, change)| {
                match (
                    &change.input_state,
                    &change.output_state,
                    &change.id_operation,
                ) {
                    (
                        ObjectIn::NotExist,
                        ObjectOut::ObjectWrite(digest, owner),
                        IDOperation::Created,
                    ) => Some(((*id, self.lamport_version, *digest), *owner)),
                    (
                        ObjectIn::NotExist,
                        ObjectOut::PackageWrite((version, digest)),
                        IDOperation::Created,
                    ) => Some(((*id, *version, *digest), Owner::Immutable)),
                    _ => None,
                }
            })
            .collect()
    }

    fn mutated(&self) -> Vec<(ObjectRef, Owner)> {
        self.changed_objects
            .iter()
            .filter_map(
                |(id, change)| match (&change.input_state, &change.output_state) {
                    (ObjectIn::Exist(_), ObjectOut::ObjectWrite(digest, owner)) => {
                        Some(((*id, self.lamport_version, *digest), *owner))
                    }
                    (ObjectIn::Exist(_), ObjectOut::PackageWrite((version, digest))) => {
                        Some(((*id, *version, *digest), Owner::Immutable))
                    }
                    _ => None,
                },
            )
            .collect()
    }

    fn unwrapped(&self) -> Vec<(ObjectRef, Owner)> {
        self.changed_objects
            .iter()
            .filter_map(|(id, change)| {
                match (
                    &change.input_state,
                    &change.output_state,
                    &change.id_operation,
                ) {
                    (
                        ObjectIn::NotExist,
                        ObjectOut::ObjectWrite(digest, owner),
                        IDOperation::None,
                    ) => Some(((*id, self.lamport_version, *digest), *owner)),
                    _ => None,
                }
            })
            .collect()
    }

    fn deleted(&self) -> Vec<ObjectRef> {
        self.changed_objects
            .iter()
            .filter_map(|(id, change)| {
                match (
                    &change.input_state,
                    &change.output_state,
                    &change.id_operation,
                ) {
                    (ObjectIn::Exist(_), ObjectOut::NotExist, IDOperation::Deleted) => Some((
                        *id,
                        self.lamport_version,
                        ObjectDigest::OBJECT_DIGEST_DELETED,
                    )),
                    _ => None,
                }
            })
            .collect()
    }

    fn unwrapped_then_deleted(&self) -> Vec<ObjectRef> {
        self.changed_objects
            .iter()
            .filter_map(|(id, change)| {
                match (
                    &change.input_state,
                    &change.output_state,
                    &change.id_operation,
                ) {
                    (ObjectIn::NotExist, ObjectOut::NotExist, IDOperation::Deleted) => Some((
                        *id,
                        self.lamport_version,
                        ObjectDigest::OBJECT_DIGEST_DELETED,
                    )),
                    _ => None,
                }
            })
            .collect()
    }

    fn wrapped(&self) -> Vec<ObjectRef> {
        self.changed_objects
            .iter()
            .filter_map(|(id, change)| {
                match (
                    &change.input_state,
                    &change.output_state,
                    &change.id_operation,
                ) {
                    (ObjectIn::Exist(_), ObjectOut::NotExist, IDOperation::None) => Some((
                        *id,
                        self.lamport_version,
                        ObjectDigest::OBJECT_DIGEST_WRAPPED,
                    )),
                    _ => None,
                }
            })
            .collect()
    }

    fn gas_object(&self) -> (ObjectRef, Owner) {
        let entry = &self.changed_objects[self.gas_object_index as usize];
        match entry.1.output_state {
            ObjectOut::ObjectWrite(digest, owner) => {
                ((entry.0, self.lamport_version, digest), owner)
            }
            _ => panic!("Gas object must be an ObjectWrite in changed_objects"),
        }
    }

    fn events_digest(&self) -> Option<&TransactionEventsDigest> {
        self.events_digest.as_ref()
    }

    fn dependencies(&self) -> &[TransactionDigest] {
        &self.dependencies
    }

    fn transaction_digest(&self) -> &TransactionDigest {
        &self.transaction_digest
    }

    fn gas_cost_summary(&self) -> &GasCostSummary {
        &self.gas_used
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

    fn unsafe_add_input_shared_object_for_testing(
        &mut self,
        obj_ref: ObjectRef,
        kind: InputSharedObjectKind,
    ) {
        match kind {
            InputSharedObjectKind::Mutate => self.changed_objects.push((
                obj_ref.0,
                ObjectChange {
                    input_state: ObjectIn::Exist((obj_ref.1, obj_ref.2)),
                    output_state: ObjectOut::ObjectWrite(
                        obj_ref.2,
                        Owner::Shared {
                            initial_shared_version: obj_ref.1,
                        },
                    ),
                    id_operation: IDOperation::None,
                },
            )),
            InputSharedObjectKind::ReadOnly => self.unchanged_shared_objects.push((
                obj_ref.0,
                UnchangedSharedKind::ReadOnlyRoot((obj_ref.1, obj_ref.2)),
            )),
        }
    }

    fn unsafe_add_deleted_object_for_testing(&mut self, obj_ref: ObjectRef) {
        self.changed_objects.push((
            obj_ref.0,
            ObjectChange {
                input_state: ObjectIn::Exist((obj_ref.1, obj_ref.2)),
                output_state: ObjectOut::NotExist,
                id_operation: IDOperation::Deleted,
            },
        ))
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
}
