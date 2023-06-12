// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::base_types::{random_object_ref, ExecutionDigests, ObjectID, ObjectRef, SequenceNumber};
use crate::committee::EpochId;
use crate::crypto::{
    default_hash, AuthoritySignInfo, AuthorityStrongQuorumSignInfo, EmptySignInfo,
};
use crate::digests::{TransactionDigest, TransactionEffectsDigest, TransactionEventsDigest};
use crate::error::{SuiError, SuiResult};
use crate::event::Event;
use crate::execution_status::ExecutionStatus;
use crate::gas::GasCostSummary;
use crate::message_envelope::{
    Envelope, Message, TrustedEnvelope, UnauthenticatedMessage, VerifiedEnvelope,
};
use crate::object::Owner;
use crate::storage::{DeleteKind, WriteKind};
use crate::transaction::{Transaction, TransactionDataAPI, VersionedProtocolMessage};
pub use effects_v1::TransactionEffectsV1;
use enum_dispatch::enum_dispatch;
use serde::{Deserialize, Serialize};
use shared_crypto::intent::IntentScope;
use sui_protocol_config::{ProtocolConfig, ProtocolVersion, SupportedProtocolVersions};

mod effects_v1;

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

    fn verify_epoch(&self, _: EpochId) -> SuiResult {
        // Authorities are allowed to re-sign effects from prior epochs, so we do not verify the
        // epoch here.
        Ok(())
    }
}

impl UnauthenticatedMessage for TransactionEffects {}

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

pub enum InputSharedObjectKind {
    Mutate,
    ReadOnly,
}

#[enum_dispatch]
pub trait TransactionEffectsAPI {
    fn status(&self) -> &ExecutionStatus;
    fn into_status(self) -> ExecutionStatus;
    fn executed_epoch(&self) -> EpochId;
    fn modified_at_versions(&self) -> Vec<(ObjectID, SequenceNumber)>;
    /// Returns the list of shared objects used in the input, with full object reference
    /// and use kind. This is needed in effects because in transaction we only have object ID
    /// for shared objects. Their version and digest can only be figured out after sequencing.
    /// Also provides the use kind to indicate whether the object was mutated or read-only.
    /// Down the road it could also indicate use-of-deleted.
    fn input_shared_objects(&self) -> Vec<(ObjectRef, InputSharedObjectKind)>;
    fn created(&self) -> Vec<(ObjectRef, Owner)>;
    fn mutated(&self) -> Vec<(ObjectRef, Owner)>;
    fn unwrapped(&self) -> Vec<(ObjectRef, Owner)>;
    fn deleted(&self) -> Vec<ObjectRef>;
    fn unwrapped_then_deleted(&self) -> Vec<ObjectRef>;
    fn wrapped(&self) -> Vec<ObjectRef>;
    fn gas_object(&self) -> (ObjectRef, Owner);
    fn events_digest(&self) -> Option<&TransactionEventsDigest>;
    fn dependencies(&self) -> &[TransactionDigest];
    // All changed objects include created, mutated and unwrapped objects,
    // they do NOT include wrapped and deleted.
    fn all_changed_objects(&self) -> Vec<(ObjectRef, Owner, WriteKind)>;

    fn all_deleted(&self) -> Vec<(ObjectRef, DeleteKind)>;

    fn transaction_digest(&self) -> &TransactionDigest;

    fn mutated_excluding_gas(&self) -> Vec<(ObjectRef, Owner)>;

    fn gas_cost_summary(&self) -> &GasCostSummary;

    fn summary_for_debug(&self) -> TransactionEffectsDebugSummary;

    // All of these should be #[cfg(test)], but they are used by tests in other crates, and
    // dependencies don't get built with cfg(test) set as far as I can tell.
    fn status_mut_for_testing(&mut self) -> &mut ExecutionStatus;
    fn gas_cost_summary_mut_for_testing(&mut self) -> &mut GasCostSummary;
    fn transaction_digest_mut_for_testing(&mut self) -> &mut TransactionDigest;
    fn dependencies_mut_for_testing(&mut self) -> &mut Vec<TransactionDigest>;
    fn unsafe_add_input_shared_object_for_testing(
        &mut self,
        obj_ref: ObjectRef,
        kind: InputSharedObjectKind,
    );
    fn unsafe_add_deleted_object_for_testing(&mut self, object: ObjectRef);
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
