// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::base_types::{
    random_object_ref, EpochId, ObjectID, ObjectRef, SequenceNumber, SuiAddress, TransactionDigest,
};
use crate::digests::{ObjectDigest, TransactionEventsDigest};
use crate::effects::{InputSharedObject, TransactionEffectsAPI, UnchangedSharedKind};
use crate::execution_status::ExecutionStatus;
use crate::gas::GasCostSummary;
use crate::object::Owner;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashSet};
use std::fmt::{Display, Formatter, Write};

use super::{IDOperation, ObjectChange};

/// The response from processing a transaction or a certified transaction
#[derive(Eq, PartialEq, Clone, Debug, Serialize, Deserialize)]
pub struct TransactionEffectsV1 {
    /// The status of the execution
    status: ExecutionStatus,
    /// The epoch when this transaction was executed.
    executed_epoch: EpochId,
    gas_used: GasCostSummary,
    /// The version that every modified (mutated or deleted) object had before it was modified by
    /// this transaction.
    modified_at_versions: Vec<(ObjectID, SequenceNumber)>,
    /// The object references of the shared objects used in this transaction. Empty if no shared objects were used.
    shared_objects: Vec<ObjectRef>,
    /// The transaction digest
    transaction_digest: TransactionDigest,

    // TODO: All the SequenceNumbers in the ObjectRefs below equal the same value (the lamport
    // timestamp of the transaction).  Consider factoring this out into one place in the effects.
    /// ObjectRef and owner of new objects created.
    created: Vec<(ObjectRef, Owner)>,
    /// ObjectRef and owner of mutated objects, including gas object.
    mutated: Vec<(ObjectRef, Owner)>,
    /// ObjectRef and owner of objects that are unwrapped in this transaction.
    /// Unwrapped objects are objects that were wrapped into other objects in the past,
    /// and just got extracted out.
    unwrapped: Vec<(ObjectRef, Owner)>,
    /// Object Refs of objects now deleted (the new refs).
    deleted: Vec<ObjectRef>,
    /// Object refs of objects previously wrapped in other objects but now deleted.
    unwrapped_then_deleted: Vec<ObjectRef>,
    /// Object refs of objects now wrapped in other objects.
    wrapped: Vec<ObjectRef>,
    /// The updated gas object reference. Have a dedicated field for convenient access.
    /// It's also included in mutated.
    gas_object: (ObjectRef, Owner),
    /// The digest of the events emitted during execution,
    /// can be None if the transaction does not emit any event.
    events_digest: Option<TransactionEventsDigest>,
    /// The set of transaction digests this transaction depends on.
    dependencies: Vec<TransactionDigest>,
}

impl TransactionEffectsV1 {
    pub fn new(
        status: ExecutionStatus,
        executed_epoch: EpochId,
        gas_used: GasCostSummary,
        modified_at_versions: Vec<(ObjectID, SequenceNumber)>,
        shared_objects: Vec<ObjectRef>,
        transaction_digest: TransactionDigest,
        created: Vec<(ObjectRef, Owner)>,
        mutated: Vec<(ObjectRef, Owner)>,
        unwrapped: Vec<(ObjectRef, Owner)>,
        deleted: Vec<ObjectRef>,
        unwrapped_then_deleted: Vec<ObjectRef>,
        wrapped: Vec<ObjectRef>,
        gas_object: (ObjectRef, Owner),
        events_digest: Option<TransactionEventsDigest>,
        dependencies: Vec<TransactionDigest>,
    ) -> Self {
        Self {
            status,
            executed_epoch,
            gas_used,
            modified_at_versions,
            shared_objects,
            transaction_digest,
            created,
            mutated,
            unwrapped,
            deleted,
            unwrapped_then_deleted,
            wrapped,
            gas_object,
            events_digest,
            dependencies,
        }
    }

    pub fn modified_at_versions(&self) -> &[(ObjectID, SequenceNumber)] {
        &self.modified_at_versions
    }

    pub fn mutated(&self) -> &[(ObjectRef, Owner)] {
        &self.mutated
    }

    pub fn created(&self) -> &[(ObjectRef, Owner)] {
        &self.created
    }

    pub fn unwrapped(&self) -> &[(ObjectRef, Owner)] {
        &self.unwrapped
    }

    pub fn deleted(&self) -> &[ObjectRef] {
        &self.deleted
    }

    pub fn wrapped(&self) -> &[ObjectRef] {
        &self.wrapped
    }
}

impl TransactionEffectsAPI for TransactionEffectsV1 {
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
        self.modified_at_versions
            .iter()
            // V1 transaction effects "modified_at_versions" includes unwrapped_then_deleted
            // objects, so in order to have parity with the V2 transaction effects semantics of
            // "modified_at_versions", filter out any objects that are unwrapped_then_deleted'ed
            .filter(|(object_id, _)| {
                !self
                    .unwrapped_then_deleted
                    .iter()
                    .any(|(deleted_id, _, _)| deleted_id == object_id)
            })
            .cloned()
            .collect()
    }

    fn lamport_version(&self) -> SequenceNumber {
        SequenceNumber::lamport_increment(self.modified_at_versions.iter().map(|(_, v)| *v))
    }

    fn old_object_metadata(&self) -> Vec<(ObjectRef, Owner)> {
        unimplemented!("Only supposed by v2 and above");
    }

    fn input_shared_objects(&self) -> Vec<InputSharedObject> {
        let modified: HashSet<_> = self.modified_at_versions.iter().map(|(r, _)| r).collect();
        self.shared_objects
            .iter()
            .map(|r| {
                if modified.contains(&r.0) {
                    InputSharedObject::Mutate(*r)
                } else {
                    InputSharedObject::ReadOnly(*r)
                }
            })
            .collect()
    }

    fn created(&self) -> Vec<(ObjectRef, Owner)> {
        self.created.clone()
    }

    fn mutated(&self) -> Vec<(ObjectRef, Owner)> {
        self.mutated.clone()
    }

    fn unwrapped(&self) -> Vec<(ObjectRef, Owner)> {
        self.unwrapped.clone()
    }

    fn deleted(&self) -> Vec<ObjectRef> {
        self.deleted.clone()
    }

    fn unwrapped_then_deleted(&self) -> Vec<ObjectRef> {
        self.unwrapped_then_deleted.clone()
    }

    fn wrapped(&self) -> Vec<ObjectRef> {
        self.wrapped.clone()
    }

    fn object_changes(&self) -> Vec<ObjectChange> {
        let modified_at: BTreeMap<_, _> = self.modified_at_versions.iter().copied().collect();

        let created = self.created.iter().map(|((id, v, d), _)| ObjectChange {
            id: *id,
            input_version: None,
            input_digest: None,
            output_version: Some(*v),
            output_digest: Some(*d),
            id_operation: IDOperation::Created,
        });

        let mutated = self.mutated.iter().map(|((id, v, d), _)| ObjectChange {
            id: *id,
            input_version: modified_at.get(id).copied(),
            input_digest: None,
            output_version: Some(*v),
            output_digest: Some(*d),
            id_operation: IDOperation::None,
        });

        let unwrapped = self.unwrapped.iter().map(|((id, v, d), _)| ObjectChange {
            id: *id,
            input_version: None,
            input_digest: None,
            output_version: Some(*v),
            output_digest: Some(*d),
            id_operation: IDOperation::None,
        });

        let deleted = self.deleted.iter().map(|(id, _, _)| ObjectChange {
            id: *id,
            input_version: modified_at.get(id).copied(),
            input_digest: None,
            output_version: None,
            output_digest: None,
            id_operation: IDOperation::Deleted,
        });

        let unwrapped_then_deleted =
            self.unwrapped_then_deleted
                .iter()
                .map(|(id, _, _)| ObjectChange {
                    id: *id,
                    input_version: None,
                    input_digest: None,
                    output_version: None,
                    output_digest: None,
                    id_operation: IDOperation::Deleted,
                });

        let wrapped = self.wrapped.iter().map(|(id, _, _)| ObjectChange {
            id: *id,
            input_version: modified_at.get(id).copied(),
            input_digest: None,
            output_version: None,
            output_digest: None,
            id_operation: IDOperation::None,
        });

        created
            .chain(mutated)
            .chain(unwrapped)
            .chain(deleted)
            .chain(unwrapped_then_deleted)
            .chain(wrapped)
            .collect()
    }

    fn gas_object(&self) -> (ObjectRef, Owner) {
        self.gas_object.clone()
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

    fn unchanged_shared_objects(&self) -> Vec<(ObjectID, UnchangedSharedKind)> {
        self.input_shared_objects()
            .iter()
            .filter_map(|o| match o {
                // In effects v1, the only unchanged shared objects are read-only shared objects.
                InputSharedObject::ReadOnly(oref) => {
                    Some((oref.0, UnchangedSharedKind::ReadOnlyRoot((oref.1, oref.2))))
                }
                _ => None,
            })
            .collect()
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

    fn unsafe_add_input_shared_object_for_testing(&mut self, kind: InputSharedObject) {
        match kind {
            InputSharedObject::Mutate(obj_ref) => {
                self.shared_objects.push(obj_ref);
                self.modified_at_versions.push((obj_ref.0, obj_ref.1));
            }
            InputSharedObject::ReadOnly(obj_ref) => {
                self.shared_objects.push(obj_ref);
            }
            InputSharedObject::ReadDeleted(id, version)
            | InputSharedObject::MutateDeleted(id, version) => {
                self.shared_objects
                    .push((id, version, ObjectDigest::OBJECT_DIGEST_DELETED));
            }
            InputSharedObject::Cancelled(..) => {
                panic!("Transaction cancellation is not supported in effect v1");
            }
        }
    }

    fn unsafe_add_deleted_live_object_for_testing(&mut self, object: ObjectRef) {
        self.modified_at_versions.push((object.0, object.1));
    }

    fn unsafe_add_object_tombstone_for_testing(&mut self, object: ObjectRef) {
        self.deleted.push(object);
    }
}

impl Display for TransactionEffectsV1 {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let mut writer = String::new();
        writeln!(writer, "Status : {:?}", self.status)?;
        if !self.created.is_empty() {
            writeln!(writer, "Created Objects:")?;
            for ((id, _, _), owner) in &self.created {
                writeln!(writer, "  - ID: {} , Owner: {}", id, owner)?;
            }
        }
        if !self.mutated.is_empty() {
            writeln!(writer, "Mutated Objects:")?;
            for ((id, _, _), owner) in &self.mutated {
                writeln!(writer, "  - ID: {} , Owner: {}", id, owner)?;
            }
        }
        if !self.deleted.is_empty() {
            writeln!(writer, "Deleted Objects:")?;
            for (id, _, _) in &self.deleted {
                writeln!(writer, "  - ID: {}", id)?;
            }
        }
        if !self.wrapped.is_empty() {
            writeln!(writer, "Wrapped Objects:")?;
            for (id, _, _) in &self.wrapped {
                writeln!(writer, "  - ID: {}", id)?;
            }
        }
        if !self.unwrapped.is_empty() {
            writeln!(writer, "Unwrapped Objects:")?;
            for ((id, _, _), owner) in &self.unwrapped {
                writeln!(writer, "  - ID: {} , Owner: {}", id, owner)?;
            }
        }
        write!(f, "{}", writer)
    }
}

impl Default for TransactionEffectsV1 {
    fn default() -> Self {
        TransactionEffectsV1 {
            status: ExecutionStatus::Success,
            executed_epoch: 0,
            gas_used: GasCostSummary {
                computation_cost: 0,
                storage_cost: 0,
                storage_rebate: 0,
                non_refundable_storage_fee: 0,
            },
            modified_at_versions: Vec::new(),
            shared_objects: Vec::new(),
            transaction_digest: TransactionDigest::random(),
            created: Vec::new(),
            mutated: Vec::new(),
            unwrapped: Vec::new(),
            deleted: Vec::new(),
            unwrapped_then_deleted: Vec::new(),
            wrapped: Vec::new(),
            gas_object: (
                random_object_ref(),
                Owner::AddressOwner(SuiAddress::default()),
            ),
            events_digest: None,
            dependencies: Vec::new(),
        }
    }
}
