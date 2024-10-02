// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use self::effects_v2::TransactionEffectsV2;
use crate::base_types::{ExecutionDigests, ObjectID, ObjectRef, SequenceNumber};
use crate::committee::{Committee, EpochId};
use crate::crypto::{
    default_hash, AuthoritySignInfo, AuthoritySignInfoTrait, AuthorityStrongQuorumSignInfo,
    EmptySignInfo,
};
use crate::digests::{
    ObjectDigest, TransactionDigest, TransactionEffectsDigest, TransactionEventsDigest,
};
use crate::error::SuiResult;
use crate::event::Event;
use crate::execution::SharedInput;
use crate::execution_status::ExecutionStatus;
use crate::gas::GasCostSummary;
use crate::message_envelope::{Envelope, Message, TrustedEnvelope, VerifiedEnvelope};
use crate::object::Owner;
use crate::storage::WriteKind;
use effects_v1::TransactionEffectsV1;
pub use effects_v2::UnchangedSharedKind;
use enum_dispatch::enum_dispatch;
pub use object_change::{EffectsObjectChange, ObjectIn, ObjectOut};
use serde::{Deserialize, Serialize};
use shared_crypto::intent::{Intent, IntentScope};
use std::collections::{BTreeMap, BTreeSet};
pub use test_effects_builder::TestEffectsBuilder;

mod effects_v1;
mod effects_v2;
mod object_change;
mod test_effects_builder;

// Since `std::mem::size_of` may not be stable across platforms, we use rough constants
// We need these for estimating effects sizes
// Approximate size of `ObjectRef` type in bytes
pub const APPROX_SIZE_OF_OBJECT_REF: usize = 80;
// Approximate size of `ExecutionStatus` type in bytes
pub const APPROX_SIZE_OF_EXECUTION_STATUS: usize = 120;
// Approximate size of `EpochId` type in bytes
pub const APPROX_SIZE_OF_EPOCH_ID: usize = 10;
// Approximate size of `GasCostSummary` type in bytes
pub const APPROX_SIZE_OF_GAS_COST_SUMMARY: usize = 40;
// Approximate size of `Option<TransactionEventsDigest>` type in bytes
pub const APPROX_SIZE_OF_OPT_TX_EVENTS_DIGEST: usize = 40;
// Approximate size of `TransactionDigest` type in bytes
pub const APPROX_SIZE_OF_TX_DIGEST: usize = 40;
// Approximate size of `Owner` type in bytes
pub const APPROX_SIZE_OF_OWNER: usize = 48;

/// The response from processing a transaction or a certified transaction
#[enum_dispatch(TransactionEffectsAPI)]
#[derive(Eq, PartialEq, Clone, Debug, Serialize, Deserialize)]
#[allow(clippy::large_enum_variant)]
pub enum TransactionEffects {
    V1(TransactionEffectsV1),
    V2(TransactionEffectsV2),
}

impl Message for TransactionEffects {
    type DigestType = TransactionEffectsDigest;
    const SCOPE: IntentScope = IntentScope::TransactionEffects;

    fn digest(&self) -> Self::DigestType {
        TransactionEffectsDigest::new(default_hash(self))
    }
}

// TODO: Get rid of this and use TestEffectsBuilder instead.
impl Default for TransactionEffects {
    fn default() -> Self {
        TransactionEffects::V2(Default::default())
    }
}

pub enum ObjectRemoveKind {
    Delete,
    Wrap,
}

impl TransactionEffects {
    /// Creates a TransactionEffects message from the results of execution, choosing the correct
    /// format for the current protocol version.
    pub fn new_from_execution_v1(
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
        Self::V1(TransactionEffectsV1::new(
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
        ))
    }

    /// Creates a TransactionEffects message from the results of execution, choosing the correct
    /// format for the current protocol version.
    pub fn new_from_execution_v2(
        status: ExecutionStatus,
        executed_epoch: EpochId,
        gas_used: GasCostSummary,
        shared_objects: Vec<SharedInput>,
        loaded_per_epoch_config_objects: BTreeSet<ObjectID>,
        transaction_digest: TransactionDigest,
        lamport_version: SequenceNumber,
        changed_objects: BTreeMap<ObjectID, EffectsObjectChange>,
        gas_object: Option<ObjectID>,
        events_digest: Option<TransactionEventsDigest>,
        dependencies: Vec<TransactionDigest>,
    ) -> Self {
        Self::V2(TransactionEffectsV2::new(
            status,
            executed_epoch,
            gas_used,
            shared_objects,
            loaded_per_epoch_config_objects,
            transaction_digest,
            lamport_version,
            changed_objects,
            gas_object,
            events_digest,
            dependencies,
        ))
    }

    pub fn execution_digests(&self) -> ExecutionDigests {
        ExecutionDigests {
            transaction: *self.transaction_digest(),
            effects: self.digest(),
        }
    }

    pub fn estimate_effects_size_upperbound_v1(
        num_writes: usize,
        num_mutables: usize,
        num_deletes: usize,
        num_deps: usize,
    ) -> usize {
        let fixed_sizes = APPROX_SIZE_OF_EXECUTION_STATUS
            + APPROX_SIZE_OF_EPOCH_ID
            + APPROX_SIZE_OF_GAS_COST_SUMMARY
            + APPROX_SIZE_OF_OPT_TX_EVENTS_DIGEST;

        // Each write or delete contributes at roughly this amount because:
        // Each write can be a mutation which can show up in `mutated` and `modified_at_versions`
        // `num_delete` is added for padding
        let approx_change_entry_size = 1_000
            + (APPROX_SIZE_OF_OWNER + APPROX_SIZE_OF_OBJECT_REF) * num_writes
            + (APPROX_SIZE_OF_OBJECT_REF * num_mutables)
            + (APPROX_SIZE_OF_OBJECT_REF * num_deletes);

        let deps_size = 1_000 + APPROX_SIZE_OF_TX_DIGEST * num_deps;

        fixed_sizes + approx_change_entry_size + deps_size
    }

    pub fn estimate_effects_size_upperbound_v2(
        num_writes: usize,
        num_modifies: usize,
        num_deps: usize,
    ) -> usize {
        let fixed_sizes = APPROX_SIZE_OF_EXECUTION_STATUS
            + APPROX_SIZE_OF_EPOCH_ID
            + APPROX_SIZE_OF_GAS_COST_SUMMARY
            + APPROX_SIZE_OF_OPT_TX_EVENTS_DIGEST;

        // We store object ref and owner for both old objects and new objects.
        let approx_change_entry_size = 1_000
            + (APPROX_SIZE_OF_OWNER + APPROX_SIZE_OF_OBJECT_REF) * num_writes
            + (APPROX_SIZE_OF_OWNER + APPROX_SIZE_OF_OBJECT_REF) * num_modifies;

        let deps_size = 1_000 + APPROX_SIZE_OF_TX_DIGEST * num_deps;

        fixed_sizes + approx_change_entry_size + deps_size
    }

    /// Return an iterator that iterates through all changed objects, including mutated,
    /// created and unwrapped objects. In other words, all objects that still exist
    /// in the object state after this transaction.
    /// It doesn't include deleted/wrapped objects.
    pub fn all_changed_objects(&self) -> Vec<(ObjectRef, Owner, WriteKind)> {
        self.mutated()
            .into_iter()
            .map(|(r, o)| (r, o, WriteKind::Mutate))
            .chain(
                self.created()
                    .into_iter()
                    .map(|(r, o)| (r, o, WriteKind::Create)),
            )
            .chain(
                self.unwrapped()
                    .into_iter()
                    .map(|(r, o)| (r, o, WriteKind::Unwrap)),
            )
            .collect()
    }

    /// Return all objects that existed in the state prior to the transaction
    /// but no longer exist in the state after the transaction.
    /// It includes deleted and wrapped objects, but does not include unwrapped_then_deleted objects.
    pub fn all_removed_objects(&self) -> Vec<(ObjectRef, ObjectRemoveKind)> {
        self.deleted()
            .iter()
            .map(|obj_ref| (*obj_ref, ObjectRemoveKind::Delete))
            .chain(
                self.wrapped()
                    .iter()
                    .map(|obj_ref| (*obj_ref, ObjectRemoveKind::Wrap)),
            )
            .collect()
    }

    /// Returns all objects that will become a tombstone after this transaction.
    /// This includes deleted, unwrapped_then_deleted and wrapped objects.
    pub fn all_tombstones(&self) -> Vec<(ObjectID, SequenceNumber)> {
        self.deleted()
            .into_iter()
            .chain(self.unwrapped_then_deleted())
            .chain(self.wrapped())
            .map(|obj_ref| (obj_ref.0, obj_ref.1))
            .collect()
    }

    /// Return an iterator of mutated objects, but excluding the gas object.
    pub fn mutated_excluding_gas(&self) -> Vec<(ObjectRef, Owner)> {
        self.mutated()
            .into_iter()
            .filter(|o| o != &self.gas_object())
            .collect()
    }

    pub fn summary_for_debug(&self) -> TransactionEffectsDebugSummary {
        TransactionEffectsDebugSummary {
            bcs_size: bcs::serialized_size(self).unwrap(),
            status: self.status().clone(),
            gas_used: self.gas_cost_summary().clone(),
            transaction_digest: *self.transaction_digest(),
            created_object_count: self.created().len(),
            mutated_object_count: self.mutated().len(),
            unwrapped_object_count: self.unwrapped().len(),
            deleted_object_count: self.deleted().len(),
            wrapped_object_count: self.wrapped().len(),
            dependency_count: self.dependencies().len(),
        }
    }
}

#[derive(Eq, PartialEq, Clone, Debug)]
pub enum InputSharedObject {
    Mutate(ObjectRef),
    ReadOnly(ObjectRef),
    ReadDeleted(ObjectID, SequenceNumber),
    MutateDeleted(ObjectID, SequenceNumber),
    Cancelled(ObjectID, SequenceNumber),
}

impl InputSharedObject {
    pub fn id_and_version(&self) -> (ObjectID, SequenceNumber) {
        let oref = self.object_ref();
        (oref.0, oref.1)
    }

    pub fn object_ref(&self) -> ObjectRef {
        match self {
            InputSharedObject::Mutate(oref) | InputSharedObject::ReadOnly(oref) => *oref,
            InputSharedObject::ReadDeleted(id, version)
            | InputSharedObject::MutateDeleted(id, version) => {
                (*id, *version, ObjectDigest::OBJECT_DIGEST_DELETED)
            }
            InputSharedObject::Cancelled(id, version) => {
                (*id, *version, ObjectDigest::OBJECT_DIGEST_CANCELLED)
            }
        }
    }
}

#[enum_dispatch]
pub trait TransactionEffectsAPI {
    fn status(&self) -> &ExecutionStatus;
    fn into_status(self) -> ExecutionStatus;
    fn executed_epoch(&self) -> EpochId;
    fn modified_at_versions(&self) -> Vec<(ObjectID, SequenceNumber)>;

    /// The version assigned to all output objects (apart from packages).
    fn lamport_version(&self) -> SequenceNumber;

    /// Metadata of objects prior to modification. This includes any object that exists in the
    /// store prior to this transaction and is modified in this transaction.
    /// It includes objects that are mutated, wrapped and deleted.
    /// This API is only available on effects v2 and above.
    fn old_object_metadata(&self) -> Vec<(ObjectRef, Owner)>;
    /// Returns the list of sequenced shared objects used in the input.
    /// This is needed in effects because in transaction we only have object ID
    /// for shared objects. Their version and digest can only be figured out after sequencing.
    /// Also provides the use kind to indicate whether the object was mutated or read-only.
    /// It does not include per epoch config objects since they do not require sequencing.
    /// TODO: Rename this function to indicate sequencing requirement.
    fn input_shared_objects(&self) -> Vec<InputSharedObject>;
    fn created(&self) -> Vec<(ObjectRef, Owner)>;
    fn mutated(&self) -> Vec<(ObjectRef, Owner)>;
    fn unwrapped(&self) -> Vec<(ObjectRef, Owner)>;
    fn deleted(&self) -> Vec<ObjectRef>;
    fn unwrapped_then_deleted(&self) -> Vec<ObjectRef>;
    fn wrapped(&self) -> Vec<ObjectRef>;

    fn object_changes(&self) -> Vec<ObjectChange>;

    // TODO: We should consider having this function to return Option.
    // When the gas object is not available (i.e. system transaction), we currently return
    // dummy object ref and owner. This is not ideal.
    fn gas_object(&self) -> (ObjectRef, Owner);

    fn events_digest(&self) -> Option<&TransactionEventsDigest>;
    fn dependencies(&self) -> &[TransactionDigest];

    fn transaction_digest(&self) -> &TransactionDigest;

    fn gas_cost_summary(&self) -> &GasCostSummary;

    fn deleted_mutably_accessed_shared_objects(&self) -> Vec<ObjectID> {
        self.input_shared_objects()
            .into_iter()
            .filter_map(|kind| match kind {
                InputSharedObject::MutateDeleted(id, _) => Some(id),
                InputSharedObject::Mutate(..)
                | InputSharedObject::ReadOnly(..)
                | InputSharedObject::ReadDeleted(..)
                | InputSharedObject::Cancelled(..) => None,
            })
            .collect()
    }

    /// Returns all root shared objects (i.e. not child object) that are read-only in the transaction.
    fn unchanged_shared_objects(&self) -> Vec<(ObjectID, UnchangedSharedKind)>;

    // All of these should be #[cfg(test)], but they are used by tests in other crates, and
    // dependencies don't get built with cfg(test) set as far as I can tell.
    fn status_mut_for_testing(&mut self) -> &mut ExecutionStatus;
    fn gas_cost_summary_mut_for_testing(&mut self) -> &mut GasCostSummary;
    fn transaction_digest_mut_for_testing(&mut self) -> &mut TransactionDigest;
    fn dependencies_mut_for_testing(&mut self) -> &mut Vec<TransactionDigest>;
    fn unsafe_add_input_shared_object_for_testing(&mut self, kind: InputSharedObject);

    // Adding an old version of a live object.
    fn unsafe_add_deleted_live_object_for_testing(&mut self, obj_ref: ObjectRef);

    // Adding a tombstone for a deleted object.
    fn unsafe_add_object_tombstone_for_testing(&mut self, obj_ref: ObjectRef);
}

#[derive(Clone)]
pub struct ObjectChange {
    pub id: ObjectID,
    pub input_version: Option<SequenceNumber>,
    pub input_digest: Option<ObjectDigest>,
    pub output_version: Option<SequenceNumber>,
    pub output_digest: Option<ObjectDigest>,
    pub id_operation: IDOperation,
}

#[derive(Eq, PartialEq, Copy, Clone, Debug, Serialize, Deserialize)]
pub enum IDOperation {
    None,
    Created,
    Deleted,
}

#[derive(Eq, PartialEq, Clone, Debug, Serialize, Deserialize, Default)]
pub struct TransactionEvents {
    pub data: Vec<Event>,
}

impl TransactionEvents {
    pub fn digest(&self) -> TransactionEventsDigest {
        TransactionEventsDigest::new(default_hash(self))
    }
}

#[derive(Debug)]
pub struct TransactionEffectsDebugSummary {
    /// Size of bcs serialized byets of the effects.
    pub bcs_size: usize,
    pub status: ExecutionStatus,
    pub gas_used: GasCostSummary,
    pub transaction_digest: TransactionDigest,
    pub created_object_count: usize,
    pub mutated_object_count: usize,
    pub unwrapped_object_count: usize,
    pub deleted_object_count: usize,
    pub wrapped_object_count: usize,
    pub dependency_count: usize,
    // TODO: Add deleted_and_unwrapped_object_count and event digest.
}

pub type TransactionEffectsEnvelope<S> = Envelope<TransactionEffects, S>;
pub type UnsignedTransactionEffects = TransactionEffectsEnvelope<EmptySignInfo>;
pub type SignedTransactionEffects = TransactionEffectsEnvelope<AuthoritySignInfo>;
pub type CertifiedTransactionEffects = TransactionEffectsEnvelope<AuthorityStrongQuorumSignInfo>;

pub type TrustedSignedTransactionEffects = TrustedEnvelope<TransactionEffects, AuthoritySignInfo>;
pub type VerifiedTransactionEffectsEnvelope<S> = VerifiedEnvelope<TransactionEffects, S>;
pub type VerifiedSignedTransactionEffects = VerifiedTransactionEffectsEnvelope<AuthoritySignInfo>;
pub type VerifiedCertifiedTransactionEffects =
    VerifiedTransactionEffectsEnvelope<AuthorityStrongQuorumSignInfo>;

impl CertifiedTransactionEffects {
    pub fn verify_authority_signatures(&self, committee: &Committee) -> SuiResult {
        self.auth_sig().verify_secure(
            self.data(),
            Intent::sui_app(IntentScope::TransactionEffects),
            committee,
        )
    }

    pub fn verify(self, committee: &Committee) -> SuiResult<VerifiedCertifiedTransactionEffects> {
        self.verify_authority_signatures(committee)?;
        Ok(VerifiedCertifiedTransactionEffects::new_from_verified(self))
    }
}
