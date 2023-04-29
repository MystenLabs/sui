// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::base_types::{
    random_object_ref, ExecutionDigests, ObjectID, ObjectRef, SequenceNumber, SuiAddress,
};
use crate::committee::EpochId;
use crate::crypto::{
    default_hash, AuthoritySignInfo, AuthorityStrongQuorumSignInfo, EmptySignInfo,
};
use crate::digests::{TransactionDigest, TransactionEffectsDigest, TransactionEventsDigest};
use crate::error::{SuiError, SuiResult};
use crate::event::Event;
use crate::execution_status::ExecutionStatus;
use crate::gas::GasCostSummary;
use crate::message_envelope::{Envelope, Message, TrustedEnvelope, VerifiedEnvelope};
use crate::messages::{Transaction, TransactionDataAPI, VersionedProtocolMessage};
use crate::object::Owner;
use crate::storage::{DeleteKind, WriteKind};
use core::fmt::{Display, Formatter};
use enum_dispatch::enum_dispatch;
use serde::{Deserialize, Serialize};
use shared_crypto::intent::IntentScope;
use std::fmt::Write;
use sui_protocol_config::{ProtocolConfig, ProtocolVersion, SupportedProtocolVersions};

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
pub enum TransactionEffects {
    V1(TransactionEffectsV1),
}

impl VersionedProtocolMessage for TransactionEffects {
    fn message_version(&self) -> Option<u64> {
        Some(match self {
            Self::V1(_) => 1,
        })
    }

    fn check_version_supported(&self, protocol_config: &ProtocolConfig) -> SuiResult {
        let (message_version, supported) = match self {
            Self::V1(_) => (1, SupportedProtocolVersions::new_for_message(1, u64::MAX)),
            // Suppose we add V2 at protocol version 7, then we must change this to:
            // Self::V1 => (1, SupportedProtocolVersions::new_for_message(1, u64::MAX)),
            // Self::V2 => (2, SupportedProtocolVersions::new_for_message(7, u64::MAX)),
        };

        if supported.is_version_supported(protocol_config.version) {
            Ok(())
        } else {
            Err(SuiError::WrongMessageVersion {
                error: format!(
                    "TransactionEffectsV{} is not supported at {:?}. (Supported range is {:?}",
                    message_version, protocol_config.version, supported
                ),
            })
        }
    }
}

impl Message for TransactionEffects {
    type DigestType = TransactionEffectsDigest;
    const SCOPE: IntentScope = IntentScope::TransactionEffects;

    fn digest(&self) -> Self::DigestType {
        TransactionEffectsDigest::new(default_hash(self))
    }

    fn verify(&self, _sig_epoch: Option<EpochId>) -> SuiResult {
        Ok(())
    }
}

impl Default for TransactionEffects {
    fn default() -> Self {
        TransactionEffects::V1(Default::default())
    }
}

impl TransactionEffects {
    /// Creates a TransactionEffects message from the results of execution, choosing the correct
    /// format for the current protocol version.
    pub fn new_from_execution(
        _protocol_version: ProtocolVersion,
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
        // TODO: when there are multiple versions, use protocol_version to construct the
        // appropriate one.

        Self::V1(TransactionEffectsV1 {
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
        })
    }

    pub fn execution_digests(&self) -> ExecutionDigests {
        ExecutionDigests {
            transaction: *self.transaction_digest(),
            effects: self.digest(),
        }
    }

    pub fn estimate_effects_size_upperbound(
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
}

// testing helpers.
impl TransactionEffects {
    pub fn new_with_tx(tx: &Transaction) -> TransactionEffects {
        Self::new_with_tx_and_gas(
            tx,
            (
                random_object_ref(),
                Owner::AddressOwner(tx.data().intent_message().value.sender()),
            ),
        )
    }

    pub fn new_with_tx_and_gas(tx: &Transaction, gas_object: (ObjectRef, Owner)) -> Self {
        TransactionEffects::V1(TransactionEffectsV1 {
            transaction_digest: *tx.digest(),
            gas_object,
            ..Default::default()
        })
    }
}

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
    fn modified_at_versions(&self) -> &[(ObjectID, SequenceNumber)] {
        &self.modified_at_versions
    }
    fn shared_objects(&self) -> &[ObjectRef] {
        &self.shared_objects
    }
    fn created(&self) -> &[(ObjectRef, Owner)] {
        &self.created
    }
    fn mutated(&self) -> &[(ObjectRef, Owner)] {
        &self.mutated
    }
    fn unwrapped(&self) -> &[(ObjectRef, Owner)] {
        &self.unwrapped
    }
    fn deleted(&self) -> &[ObjectRef] {
        &self.deleted
    }
    fn unwrapped_then_deleted(&self) -> &[ObjectRef] {
        &self.unwrapped_then_deleted
    }
    fn wrapped(&self) -> &[ObjectRef] {
        &self.wrapped
    }
    fn gas_object(&self) -> &(ObjectRef, Owner) {
        &self.gas_object
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
    fn all_changed_objects(&self) -> Vec<(&ObjectRef, &Owner, WriteKind)> {
        self.mutated
            .iter()
            .map(|(r, o)| (r, o, WriteKind::Mutate))
            .chain(self.created.iter().map(|(r, o)| (r, o, WriteKind::Create)))
            .chain(
                self.unwrapped
                    .iter()
                    .map(|(r, o)| (r, o, WriteKind::Unwrap)),
            )
            .collect()
    }

    /// Return an iterator that iterates through all deleted objects, including deleted,
    /// unwrapped_then_deleted, and wrapped objects. In other words, all objects that
    /// do not exist in the object state after this transaction.
    fn all_deleted(&self) -> Vec<(&ObjectRef, DeleteKind)> {
        self.deleted
            .iter()
            .map(|r| (r, DeleteKind::Normal))
            .chain(
                self.unwrapped_then_deleted
                    .iter()
                    .map(|r| (r, DeleteKind::UnwrapThenDelete)),
            )
            .chain(self.wrapped.iter().map(|r| (r, DeleteKind::Wrap)))
            .collect()
    }

    /// Return an iterator of mutated objects, but excluding the gas object.
    fn mutated_excluding_gas(&self) -> Vec<&(ObjectRef, Owner)> {
        self.mutated
            .iter()
            .filter(|o| *o != &self.gas_object)
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
    fn shared_objects_mut_for_testing(&mut self) -> &mut Vec<ObjectRef> {
        &mut self.shared_objects
    }
    fn modified_at_versions_mut_for_testing(&mut self) -> &mut Vec<(ObjectID, SequenceNumber)> {
        &mut self.modified_at_versions
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

#[enum_dispatch]
pub trait TransactionEffectsAPI {
    fn status(&self) -> &ExecutionStatus;
    fn into_status(self) -> ExecutionStatus;
    fn executed_epoch(&self) -> EpochId;
    fn modified_at_versions(&self) -> &[(ObjectID, SequenceNumber)];
    fn shared_objects(&self) -> &[ObjectRef];
    fn created(&self) -> &[(ObjectRef, Owner)];
    fn mutated(&self) -> &[(ObjectRef, Owner)];
    fn unwrapped(&self) -> &[(ObjectRef, Owner)];
    fn deleted(&self) -> &[ObjectRef];
    fn unwrapped_then_deleted(&self) -> &[ObjectRef];
    fn wrapped(&self) -> &[ObjectRef];
    fn gas_object(&self) -> &(ObjectRef, Owner);
    fn events_digest(&self) -> Option<&TransactionEventsDigest>;
    fn dependencies(&self) -> &[TransactionDigest];
    // All changed objects include created, mutated and unwrapped objects,
    // they do NOT include wrapped and deleted.
    fn all_changed_objects(&self) -> Vec<(&ObjectRef, &Owner, WriteKind)>;

    fn all_deleted(&self) -> Vec<(&ObjectRef, DeleteKind)>;

    fn transaction_digest(&self) -> &TransactionDigest;

    fn mutated_excluding_gas(&self) -> Vec<&(ObjectRef, Owner)>;

    fn gas_cost_summary(&self) -> &GasCostSummary;

    fn summary_for_debug(&self) -> TransactionEffectsDebugSummary;

    // All of these should be #[cfg(test)], but they are used by tests in other crates, and
    // dependencies don't get built with cfg(test) set as far as I can tell.
    fn status_mut_for_testing(&mut self) -> &mut ExecutionStatus;
    fn gas_cost_summary_mut_for_testing(&mut self) -> &mut GasCostSummary;
    fn transaction_digest_mut_for_testing(&mut self) -> &mut TransactionDigest;
    fn dependencies_mut_for_testing(&mut self) -> &mut Vec<TransactionDigest>;
    fn shared_objects_mut_for_testing(&mut self) -> &mut Vec<ObjectRef>;
    fn modified_at_versions_mut_for_testing(&mut self) -> &mut Vec<(ObjectID, SequenceNumber)>;
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
