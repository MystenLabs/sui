// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use super::object_change::{IDOperation, ObjectIn, ObjectOut};
use super::EffectsObjectChange;
use crate::base_types::{
    EpochId, ObjectDigest, ObjectID, ObjectRef, SequenceNumber, SuiAddress, TransactionDigest,
    VersionDigest,
};
use crate::digests::{Digest, TransactionEventsDigest};
use crate::effects::{InputSharedObjectKind, TransactionEffectsAPI};
use crate::execution_status::ExecutionStatus;
use crate::gas::GasCostSummary;
use crate::message_envelope::Message;
use crate::object::{Owner, OBJECT_START_VERSION};
use crate::transaction::SenderSignedData;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
#[cfg(debug_assertions)]
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
    /// System transaction that don't require gas will leave this as None.
    gas_object_index: Option<u32>,
    /// The digest of the events emitted during execution,
    /// can be None if the transaction does not emit any event.
    events_digest: Option<TransactionEventsDigest>,
    /// The set of transaction digests this transaction depends on.
    dependencies: Vec<TransactionDigest>,

    /// The version number of all the written Move objects by this transaction.
    lamport_version: SequenceNumber,
    /// Objects whose state are changed in the object store.
    changed_objects: Vec<(ObjectID, EffectsObjectChange)>,
    /// Shared objects that are not mutated in this transaction. Unlike owned objects,
    /// read-only shared objects' version are not committed in the transaction,
    /// and in order for a node to catch up and execute it without consensus sequencing,
    /// the version needs to be committed in the effects.
    unchanged_shared_objects: Vec<(ObjectID, UnchangedSharedKind)>,
    /// Auxiliary data that are not protocol-critical, generated as part of the effects but are stored separately.
    /// Storing it separately allows us to avoid bloating the effects with data that are not critical.
    /// It also provides more flexibility on the format and type of the data.
    aux_data_digest: Option<Digest>,
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

    fn modified_at_versions(&self) -> Vec<(ObjectID, SequenceNumber)> {
        self.changed_objects
            .iter()
            .filter_map(|(id, change)| {
                if let ObjectIn::Exist(((version, _digest), _owner)) = &change.input_state {
                    Some((*id, *version))
                } else {
                    None
                }
            })
            .collect()
    }

    fn old_object_metadata(&self) -> Vec<(ObjectRef, Owner)> {
        self.changed_objects
            .iter()
            .filter_map(|(id, change)| {
                if let ObjectIn::Exist(((version, digest), owner)) = &change.input_state {
                    Some(((*id, *version, *digest), *owner))
                } else {
                    None
                }
            })
            .collect()
    }

    fn input_shared_objects(&self) -> Vec<(ObjectRef, InputSharedObjectKind)> {
        self.changed_objects
            .iter()
            .filter_map(|(id, change)| match &change.input_state {
                ObjectIn::Exist(((version, digest), Owner::Shared { .. })) => {
                    Some(((*id, *version, *digest), InputSharedObjectKind::Mutate))
                }
                _ => None,
            })
            .chain(self.unchanged_shared_objects.iter().map(
                |(id, change_kind)| match change_kind {
                    UnchangedSharedKind::ReadOnlyRoot((version, digest)) => {
                        ((*id, *version, *digest), InputSharedObjectKind::ReadOnly)
                    }
                },
            ))
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
                        ObjectOut::ObjectWrite((digest, owner)),
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
                    (ObjectIn::Exist(_), ObjectOut::ObjectWrite((digest, owner))) => {
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
                        ObjectOut::ObjectWrite((digest, owner)),
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
        if let Some(gas_object_index) = self.gas_object_index {
            let entry = &self.changed_objects[gas_object_index as usize];
            match entry.1.output_state {
                ObjectOut::ObjectWrite((digest, owner)) => {
                    ((entry.0, self.lamport_version, digest), owner)
                }
                _ => panic!("Gas object must be an ObjectWrite in changed_objects"),
            }
        } else {
            (
                (ObjectID::ZERO, SequenceNumber::default(), ObjectDigest::MIN),
                Owner::AddressOwner(SuiAddress::default()),
            )
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
                EffectsObjectChange {
                    input_state: ObjectIn::Exist((
                        (obj_ref.1, obj_ref.2),
                        Owner::Shared {
                            initial_shared_version: OBJECT_START_VERSION,
                        },
                    )),
                    output_state: ObjectOut::ObjectWrite((
                        obj_ref.2,
                        Owner::Shared {
                            initial_shared_version: obj_ref.1,
                        },
                    )),
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
            EffectsObjectChange {
                input_state: ObjectIn::Exist((
                    (obj_ref.1, obj_ref.2),
                    Owner::AddressOwner(SuiAddress::default()),
                )),
                output_state: ObjectOut::NotExist,
                id_operation: IDOperation::Deleted,
            },
        ))
    }
}

impl TransactionEffectsV2 {
    pub fn new(
        status: ExecutionStatus,
        executed_epoch: EpochId,
        gas_used: GasCostSummary,
        shared_objects: Vec<ObjectRef>,
        transaction_digest: TransactionDigest,
        lamport_version: SequenceNumber,
        changed_objects: BTreeMap<ObjectID, EffectsObjectChange>,
        gas_object: Option<ObjectID>,
        events_digest: Option<TransactionEventsDigest>,
        dependencies: Vec<TransactionDigest>,
    ) -> Self {
        let unchanged_shared_objects = shared_objects
            .into_iter()
            .filter_map(|(id, version, digest)| {
                if changed_objects.contains_key(&id) {
                    None
                } else {
                    Some((id, UnchangedSharedKind::ReadOnlyRoot((version, digest))))
                }
            })
            .collect();
        let changed_objects: Vec<_> = changed_objects.into_iter().collect();

        let gas_object_index = gas_object.map(|gas_id| {
            changed_objects
                .iter()
                .position(|(id, _)| id == &gas_id)
                .unwrap() as u32
        });

        let result = Self {
            status,
            executed_epoch,
            gas_used,
            transaction_digest,
            lamport_version,
            changed_objects,
            unchanged_shared_objects,
            gas_object_index,
            events_digest,
            dependencies,
            aux_data_digest: None,
        };
        #[cfg(debug_assertions)]
        result.check_invariant();

        result
    }

    pub fn new_with_tx_and_gas(tx: &SenderSignedData, gas_object: (ObjectRef, Owner)) -> Self {
        Self {
            transaction_digest: tx.digest(),
            lamport_version: gas_object.0 .1,
            changed_objects: vec![(
                gas_object.0 .0,
                EffectsObjectChange {
                    input_state: ObjectIn::Exist((
                        (SequenceNumber::default(), ObjectDigest::MIN),
                        gas_object.1,
                    )),
                    output_state: ObjectOut::ObjectWrite((gas_object.0 .2, gas_object.1)),
                    id_operation: IDOperation::None,
                },
            )],
            gas_object_index: Some(0),
            ..Default::default()
        }
    }

    pub fn new_with_tx_and_status(tx: &SenderSignedData, status: ExecutionStatus) -> Self {
        Self {
            transaction_digest: tx.digest(),
            status,
            ..Default::default()
        }
    }

    /// This function demonstrates what's the invariant of the effects.
    /// It also documents the semantics of different combinations in object changes.
    #[cfg(debug_assertions)]
    fn check_invariant(&self) {
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
                (ObjectIn::NotExist, ObjectOut::ObjectWrite((_, owner)), IDOperation::None) => {
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
                (ObjectIn::Exist((_, old_owner)), ObjectOut::NotExist, IDOperation::None) => {
                    // wrapped.
                    assert!(!old_owner.is_shared(), "Cannot wrap shared object");
                }
                (ObjectIn::Exist(_), ObjectOut::NotExist, IDOperation::Deleted) => {
                    // deleted.
                }
                (
                    ObjectIn::Exist(((old_version, old_digest), _)),
                    ObjectOut::ObjectWrite((new_digest, _)),
                    IDOperation::None,
                ) => {
                    // mutated.
                    assert!(old_version.value() < self.lamport_version.value());
                    assert_ne!(old_digest, new_digest);
                }
                (
                    ObjectIn::Exist(((old_version, _), _)),
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

impl Default for TransactionEffectsV2 {
    fn default() -> Self {
        Self {
            status: ExecutionStatus::Success,
            executed_epoch: 0,
            gas_used: GasCostSummary::default(),
            transaction_digest: TransactionDigest::default(),
            lamport_version: SequenceNumber::default(),
            changed_objects: vec![],
            unchanged_shared_objects: vec![],
            gas_object_index: None,
            events_digest: None,
            dependencies: vec![],
            aux_data_digest: None,
        }
    }
}

#[derive(Eq, PartialEq, Clone, Debug, Serialize, Deserialize)]
pub(crate) enum UnchangedSharedKind {
    /// Read-only shared objects from the input. We don't really need ObjectDigest
    /// for protocol correctness, but it will make it easier to verify untrusted read.
    ReadOnlyRoot(VersionDigest),
}
