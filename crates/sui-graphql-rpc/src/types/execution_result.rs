// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::types::transaction_block_effects::TransactionBlockEffects;
use async_graphql::*;

#[derive(SimpleObject, Clone)]
pub(crate) struct ExecutionResult {
    /// The errors field captures any errors that occurred during execution
    pub errors: Option<Vec<String>>,

    /// The digest field captures the digest of the transaction block
    pub digest: String,

    /// The effects field captures the effects of the transaction block
    pub effects: TransactionBlockEffects,
}
