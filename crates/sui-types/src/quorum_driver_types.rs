// Copyright (c) 2021, Facebook, Inc. and its affiliates
// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeMap;

use crate::base_types::{AuthorityName, EpochId, ObjectRef, TransactionDigest};
use crate::committee::StakeUnit;
use crate::crypto::{AuthorityStrongQuorumSignInfo, ConciseAuthorityPublicKeyBytes};
use crate::effects::{
    CertifiedTransactionEffects, TransactionEffects, TransactionEvents,
    VerifiedCertifiedTransactionEffects,
};
use crate::error::SuiError;
use crate::messages_checkpoint::CheckpointSequenceNumber;
use crate::object::Object;
use crate::transaction::{Transaction, VerifiedTransaction};
use serde::{Deserialize, Serialize};
use strum::AsRefStr;
use thiserror::Error;

pub type QuorumDriverResult = Result<QuorumDriverResponse, QuorumDriverError>;

pub type QuorumDriverEffectsQueueResult =
    Result<(Transaction, QuorumDriverResponse), (TransactionDigest, QuorumDriverError)>;

pub const NON_RECOVERABLE_ERROR_MSG: &str =
    "Transaction has non recoverable errors from at least 1/3 of validators";

/// Client facing errors regarding transaction submission via Quorum Driver.
/// Every invariant needs detailed documents to instruct client handling.
#[derive(Eq, PartialEq, Clone, Debug, Serialize, Deserialize, Error, Hash, AsRefStr)]
pub enum QuorumDriverError {
    #[error("QuorumDriver internal error: {0}.")]
    QuorumDriverInternalError(SuiError),
    #[error("Invalid user signature: {0}.")]
    InvalidUserSignature(SuiError),
    #[error(
        "Failed to sign transaction by a quorum of validators because of locked objects: {conflicting_txes:?}",
    )]
    ObjectsDoubleUsed {
        conflicting_txes: BTreeMap<TransactionDigest, (Vec<(AuthorityName, ObjectRef)>, StakeUnit)>,
    },
    #[error("Transaction timed out before reaching finality")]
    TimeoutBeforeFinality,
    #[error("Transaction failed to reach finality with transient error after {total_attempts} attempts.")]
    FailedWithTransientErrorAfterMaximumAttempts { total_attempts: u32 },
    #[error("{NON_RECOVERABLE_ERROR_MSG}: {errors:?}.")]
    NonRecoverableTransactionError { errors: GroupedErrors },
    #[error("Transaction is not processed because {overloaded_stake} of validators by stake are overloaded with certificates pending execution.")]
    SystemOverload {
        overloaded_stake: StakeUnit,
        errors: GroupedErrors,
    },
    #[error("Transaction is already finalized but with different user signatures")]
    TxAlreadyFinalizedWithDifferentUserSignatures,
    #[error("Transaction is not processed because {overload_stake} of validators are overloaded and asked client to retry after {retry_after_secs}.")]
    SystemOverloadRetryAfter {
        overload_stake: StakeUnit,
        errors: GroupedErrors,
        retry_after_secs: u64,
    },
}

pub type GroupedErrors = Vec<(SuiError, StakeUnit, Vec<ConciseAuthorityPublicKeyBytes>)>;

#[derive(Serialize, Deserialize, Clone, Debug, schemars::JsonSchema)]
pub enum ExecuteTransactionRequestType {
    WaitForEffectsCert,
    WaitForLocalExecution,
}

#[derive(Debug)]
pub enum TransactionType {
    SingleWriter, // Txes that only use owned objects and/or immutable objects
    SharedObject, // Txes that use at least one shared object
}

/// Proof of finality of transaction effects.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub enum EffectsFinalityInfo {
    /// Effects are certified by a quorum of validators.
    Certified(AuthorityStrongQuorumSignInfo),

    /// Effects are included in a checkpoint.
    Checkpointed(EpochId, CheckpointSequenceNumber),

    /// A quorum of validators have acknowledged effects.
    QuorumExecuted(EpochId),
}

/// When requested to execute a transaction with WaitForLocalExecution,
/// TransactionOrchestrator attempts to execute this transaction locally
/// after it is finalized. This value represents whether the transaction
/// is confirmed to be executed on this node before the response returns.
pub type IsTransactionExecutedLocally = bool;

#[derive(Serialize, Deserialize, Clone, Debug)]
pub enum ExecuteTransactionResponse {
    EffectsCert(
        Box<(
            FinalizedEffects,
            TransactionEvents,
            IsTransactionExecutedLocally,
        )>,
    ),
}

#[derive(Clone, Debug)]
pub struct QuorumDriverRequest {
    pub transaction: VerifiedTransaction,
}

#[derive(Debug, Clone)]
pub struct QuorumDriverResponse {
    pub effects_cert: VerifiedCertifiedTransactionEffects,
    // pub events: TransactionEvents,
    pub events: Option<TransactionEvents>,
    // Input objects will only be populated in the happy path
    pub input_objects: Option<Vec<Object>>,
    // Output objects will only be populated in the happy path
    pub output_objects: Option<Vec<Object>>,
    pub auxiliary_data: Option<Vec<u8>>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ExecuteTransactionRequestV3 {
    pub transaction: Transaction,

    pub include_events: bool,
    pub include_input_objects: bool,
    pub include_output_objects: bool,
    pub include_auxiliary_data: bool,
}

impl ExecuteTransactionRequestV3 {
    pub fn new_v2<T: Into<Transaction>>(transaction: T) -> Self {
        Self {
            transaction: transaction.into(),
            include_events: true,
            include_input_objects: false,
            include_output_objects: false,
            include_auxiliary_data: false,
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ExecuteTransactionResponseV3 {
    pub effects: FinalizedEffects,

    pub events: Option<TransactionEvents>,
    // Input objects will only be populated in the happy path
    pub input_objects: Option<Vec<Object>>,
    // Output objects will only be populated in the happy path
    pub output_objects: Option<Vec<Object>>,
    pub auxiliary_data: Option<Vec<u8>>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct FinalizedEffects {
    pub effects: TransactionEffects,
    pub finality_info: EffectsFinalityInfo,
}

impl FinalizedEffects {
    pub fn new_from_effects_cert(effects_cert: CertifiedTransactionEffects) -> Self {
        let (data, sig) = effects_cert.into_data_and_sig();
        Self {
            effects: data,
            finality_info: EffectsFinalityInfo::Certified(sig),
        }
    }

    pub fn epoch(&self) -> EpochId {
        match &self.finality_info {
            EffectsFinalityInfo::Certified(cert) => cert.epoch,
            EffectsFinalityInfo::Checkpointed(epoch, _) => *epoch,
            EffectsFinalityInfo::QuorumExecuted(epoch) => *epoch,
        }
    }

    pub fn data(&self) -> &TransactionEffects {
        &self.effects
    }
}
