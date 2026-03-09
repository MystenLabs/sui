// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use async_graphql::SimpleObject;

use crate::api::types::transaction_effects::TransactionEffects;

/// The execution result of a transaction, including the transaction effects.
#[derive(Clone, SimpleObject)]
pub struct ExecutionResult {
    /// The effects of the transaction execution.
    pub effects: Option<TransactionEffects>,
}
