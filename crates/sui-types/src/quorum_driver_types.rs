// Copyright (c) 2021, Facebook, Inc. and its affiliates
// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeMap;

use crate::base_types::{AuthorityName, ObjectRef, TransactionDigest};
use crate::committee::StakeUnit;
use crate::error::SuiError;
use crate::messages::QuorumDriverResponse;
use serde::{Deserialize, Serialize};
use strum::AsRefStr;
use thiserror::Error;

pub type QuorumDriverResult = Result<QuorumDriverResponse, QuorumDriverError>;

/// Client facing errors regarding transaction submission via Quorum Driver.
/// Every invariant needs detailed documents to instruct client handling.
#[derive(Eq, PartialEq, Clone, Debug, Serialize, Deserialize, Error, Hash, AsRefStr)]
pub enum QuorumDriverError {
    #[error(
        "Failed to process transaction on a quorum of validators to form a transaction certificate because of locked objects: {:?}, retried a conflicting transaction {:?}, success: {:?}",
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
    TimeoutBeforeReachFinality,
    #[error("Transaction failed to reach finality after {total_attempts} attempts.")]
    FailedAfterMaximumAttempts { total_attempts: u8 },
    // We expect this occrus very rarely. For any common error types,
    // we should represent as a QuorumDriverError variant instead.
    #[error("Transaction encountered uncategorized SuiError: {:?}", error)]
    UncategorizedSuiError { error: SuiError },
}

impl From<SuiError> for QuorumDriverError {
    fn from(error: SuiError) -> Self {
        QuorumDriverError::UncategorizedSuiError { error }
    }
}
