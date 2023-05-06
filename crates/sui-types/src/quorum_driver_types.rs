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
    Result<(VerifiedTransaction, QuorumDriverResponse), (TransactionDigest, QuorumDriverError)>;

/// Client facing errors regarding transaction submission via Quorum Driver.
/// Every invariant needs detailed documents to instruct client handling.
#[derive(Eq, PartialEq, Clone, Debug, Serialize, Deserialize, Error, Hash, AsRefStr)]
pub enum QuorumDriverError {
    #[error("QuorumDriver internal error: {0:?}.")]
    QuorumDriverInternalError(SuiError),
    #[error("Invalid user signature: {0:?}.")]
    InvalidUserSignature(SuiError),
    #[error(
        "Failed to sign transaction by a quorum of validators because of locked objects: {:?}, retried a conflicting transaction {:?}, success: {:?}",
        conflicting_txes,
        retried_tx,
        retried_tx_success
    )]
    ObjectsDoubleUsed {
        conflicting_txes: BTreeMap<TransactionDigest, (Vec<(AuthorityName, ObjectRef)>, StakeUnit)>,
        retried_tx: Option<TransactionDigest>,
        retried_tx_success: Option<bool>,
    },
    #[error("Transaction timed out before reaching finality")]
    TimeoutBeforeFinality,
    #[error("Transaction failed to reach finality with transient error after {total_attempts} attempts.")]
    FailedWithTransientErrorAfterMaximumAttempts { total_attempts: u8 },
    #[error("Transaction has non recoverable errors from at least 1/3 of validators: {errors:?}.")]
    NonRecoverableTransactionError { errors: GroupedErrors },
    #[error("Transaction is not processed because {overloaded_stake} of validators by stake are overloaded with certificates pending execution.")]
    SystemOverload {
        overloaded_stake: StakeUnit,
        errors: GroupedErrors,
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

#[derive(Serialize, Deserialize, Clone, Debug)]
pub enum EffectsFinalityInfo {
    Certified(AuthorityStrongQuorumSignInfo),
    Checkpointed(EpochId, CheckpointSequenceNumber),
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
    pub events: TransactionEvents,
    pub objects: Vec<Object>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ExecuteTransactionRequest {
    pub transaction: Transaction,
    pub request_type: ExecuteTransactionRequestType,
}

impl ExecuteTransactionRequest {
    pub fn transaction_type(&self) -> TransactionType {
        if self.transaction.contains_shared_object() {
            TransactionType::SharedObject
        } else {
            TransactionType::SingleWriter
        }
    }
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
        }
    }
}
