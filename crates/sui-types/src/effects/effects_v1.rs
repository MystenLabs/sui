// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::base_types::{
    random_object_ref, EpochId, ObjectID, ObjectRef, SequenceNumber, SuiAddress, TransactionDigest,
};
use crate::digests::TransactionEventsDigest;
use crate::effects::{
    InputSharedObjectKind, TransactionEffectsAPI, TransactionEffectsDebugSummary,
};
use crate::execution_status::ExecutionStatus;
use crate::gas::GasCostSummary;
use crate::object::Owner;
use crate::storage::{DeleteKind, WriteKind};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::fmt::{Display, Formatter, Write};

/// The response from processing a transaction or a certified transaction
#[derive(Eq, PartialEq, Clone, Debug, Serialize, Deserialize)]
pub struct TransactionEffectsV1 {
    /// The status of the execution
    pub status: ExecutionStatus,
    /// The epoch when this transaction was executed.
    pub executed_epoch: EpochId,
    pub gas_used: GasCostSummary,
    /// The version that every modified (mutated or deleted) object had before it was modified by
    /// this transaction.
    pub modified_at_versions: Vec<(ObjectID, SequenceNumber)>,
    /// The object references of the shared objects used in this transaction. Empty if no shared objects were used.
    pub shared_objects: Vec<ObjectRef>,
    /// The transaction digest
    pub transaction_digest: TransactionDigest,

    // TODO: All the SequenceNumbers in the ObjectRefs below equal the same value (the lamport
    // timestamp of the transaction).  Consider factoring this out into one place in the effects.
    /// ObjectRef and owner of new objects created.
    pub created: Vec<(ObjectRef, Owner)>,
    /// ObjectRef and owner of mutated objects, including gas object.
    pub mutated: Vec<(ObjectRef, Owner)>,
    /// ObjectRef and owner of objects that are unwrapped in this transaction.
    /// Unwrapped objects are objects that were wrapped into other objects in the past,
    /// and just got extracted out.
    pub unwrapped: Vec<(ObjectRef, Owner)>,
    /// Object Refs of objects now deleted (the old refs).
    pub deleted: Vec<ObjectRef>,
    /// Object refs of objects previously wrapped in other objects but now deleted.
    pub unwrapped_then_deleted: Vec<ObjectRef>,
    /// Object refs of objects now wrapped in other objects.
    pub wrapped: Vec<ObjectRef>,
    /// The updated gas object reference. Have a dedicated field for convenient access.
    /// It's also included in mutated.
    pub gas_object: (ObjectRef, Owner),
    /// The digest of the events emitted during execution,
    /// can be None if the transaction does not emit any event.
    pub events_digest: Option<TransactionEventsDigest>,
    /// The set of transaction digests this transaction depends on.
    pub dependencies: Vec<TransactionDigest>,
}

impl TransactionEffectsAPI for TransactionEffectsV1 {
    fn status(&self) -> &ExecutionStatus {
        &self.status
    }
    fn into_status(self) -> ExecutionStatus {
        self.status
    }
    fn modified_at_versions(&self) -> Vec<(ObjectID, SequenceNumber)> {
        self.modified_at_versions.clone()
    }

    fn input_shared_objects(&self) -> Vec<(ObjectRef, InputSharedObjectKind)> {
        let modified: HashSet<_> = self.modified_at_versions.iter().map(|(r, _)| r).collect();
        self.shared_objects
            .iter()
            .map(|r| {
                let kind = if modified.contains(&r.0) {
                    InputSharedObjectKind::Mutate
                } else {
                    InputSharedObjectKind::ReadOnly
                };
                (*r, kind)
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
    fn gas_object(&self) -> (ObjectRef, Owner) {
        self.gas_object
    }
    fn events_digest(&self) -> Option<&TransactionEventsDigest> {
        self.events_digest.as_ref()
    }
    fn dependencies(&self) -> &[TransactionDigest] {
        &self.dependencies
    }

    fn executed_epoch(&self) -> EpochId {
        self.executed_epoch
    }

    /// Return an iterator that iterates through all changed objects, including mutated,
    /// created and unwrapped objects. In other words, all objects that still exist
    /// in the object state after this transaction.
    /// It doesn't include deleted/wrapped objects.
    fn all_changed_objects(&self) -> Vec<(ObjectRef, Owner, WriteKind)> {
        self.mutated
            .iter()
            .map(|(r, o)| (*r, *o, WriteKind::Mutate))
            .chain(
                self.created
                    .iter()
                    .map(|(r, o)| (*r, *o, WriteKind::Create)),
            )
            .chain(
                self.unwrapped
                    .iter()
                    .map(|(r, o)| (*r, *o, WriteKind::Unwrap)),
            )
            .collect()
    }

    /// Return an iterator that iterates through all deleted objects, including deleted,
    /// unwrapped_then_deleted, and wrapped objects. In other words, all objects that
    /// do not exist in the object state after this transaction.
    fn all_deleted(&self) -> Vec<(ObjectRef, DeleteKind)> {
        self.deleted
            .iter()
            .map(|r| (*r, DeleteKind::Normal))
            .chain(
                self.unwrapped_then_deleted
                    .iter()
                    .map(|r| (*r, DeleteKind::UnwrapThenDelete)),
            )
            .chain(self.wrapped.iter().map(|r| (*r, DeleteKind::Wrap)))
            .collect()
    }

    /// Return an iterator of mutated objects, but excluding the gas object.
    fn mutated_excluding_gas(&self) -> Vec<(ObjectRef, Owner)> {
        self.mutated
            .clone()
            .into_iter()
            .filter(|o| o != &self.gas_object)
            .collect()
    }

    fn transaction_digest(&self) -> &TransactionDigest {
        &self.transaction_digest
    }

    fn gas_cost_summary(&self) -> &GasCostSummary {
        &self.gas_used
    }

    fn summary_for_debug(&self) -> TransactionEffectsDebugSummary {
        TransactionEffectsDebugSummary {
            bcs_size: bcs::serialized_size(self).unwrap(),
            status: self.status.clone(),
            gas_used: self.gas_used.clone(),
            transaction_digest: self.transaction_digest,
            created_object_count: self.created.len(),
            mutated_object_count: self.mutated.len(),
            unwrapped_object_count: self.unwrapped.len(),
            deleted_object_count: self.deleted.len(),
            wrapped_object_count: self.wrapped.len(),
            dependency_count: self.dependencies.len(),
        }
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
        self.shared_objects.push(obj_ref);
        match kind {
            InputSharedObjectKind::Mutate => {
                self.modified_at_versions.push((obj_ref.0, obj_ref.1));
            }
            InputSharedObjectKind::ReadOnly => (),
        }
    }

    fn unsafe_add_deleted_object_for_testing(&mut self, object: ObjectRef) {
        self.modified_at_versions.push((object.0, object.1));
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
