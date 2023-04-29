// Copyright (c) 2021, Facebook, Inc. and its affiliates
// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeMap;

use crate::base_types::{AuthorityName, ObjectRef, TransactionDigest};
use crate::committee::StakeUnit;
use crate::crypto::ConciseAuthorityPublicKeyBytes;
use crate::error::SuiError;
pub use crate::messages::QuorumDriverResponse;
use crate::messages::VerifiedTransaction;
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
