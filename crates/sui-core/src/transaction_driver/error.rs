// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use sui_types::error::SuiError;
use thiserror::Error;

use sui_types::base_types::{AuthorityName, ObjectRef, TransactionDigest};
use sui_types::committee::StakeUnit;

/// Client facing errors regarding transaction submission via Transaction Driver.
/// Every invariant needs detailed documents to instruct client handling.
#[derive(Eq, PartialEq, Clone, Debug, Serialize, Deserialize, Error, Hash)]
pub enum TransactionDriverError {
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
    #[error("Failed to call validator {0}: {1}")]
    RpcFailure(String, String),
}
