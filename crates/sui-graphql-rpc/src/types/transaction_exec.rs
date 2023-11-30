// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use super::transaction_block_effects::TransactionBlockEffects;
use async_graphql::*;

#[derive(SimpleObject, Clone)]
pub(crate) struct ExecutionResult {
    /// The effects field captures the results to the chain of executing this transaction
    pub effects: Option<TransactionBlockEffects>,

    /// The errors field captures any errors that occurred during execution
    pub errors: Vec<String>,
}
