// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use async_graphql::SimpleObject;

use super::transaction_effects::TransactionEffects;

/// The execution result of a transaction, including the transaction effects and any potential errors due to signing or quorum-driving.
#[derive(Clone, SimpleObject)]
pub struct ExecutionResult {
    /// The effects of the transaction execution, if successful.
    pub effects: Option<TransactionEffects>,

    /// Errors that occurred during execution (e.g., network errors, validation failures).
    /// These are distinct from execution failures within the transaction itself.
    pub errors: Option<Vec<String>>,
}
