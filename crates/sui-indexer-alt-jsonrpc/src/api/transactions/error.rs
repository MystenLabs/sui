// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use sui_types::{base_types::ObjectID, digests::TransactionDigest};

#[derive(thiserror::Error, Debug)]
pub(super) enum Error {
    #[error("Cannot filter by function name {function:?} without specifying a module")]
    MissingModule { function: String },

    #[error("Transaction {0} not found")]
    NotFound(TransactionDigest),

    #[error("Pagination issue: {0}")]
    Pagination(#[from] crate::paginate::Error),

    #[error("Balance changes for transaction {0} have been pruned")]
    PrunedBalanceChanges(TransactionDigest),

    #[error(
        "Transaction {0} affected object {} pruned at version {2}",
        .1.to_canonical_display(/* with_prefix */ true),
    )]
    PrunedObject(TransactionDigest, ObjectID, u64),
}
