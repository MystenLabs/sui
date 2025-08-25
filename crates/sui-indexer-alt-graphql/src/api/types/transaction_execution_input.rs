// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use async_graphql::InputObject;

/// Input for executing a transaction.
#[derive(InputObject)]
pub struct TransactionExecutionInput {
    /// BCS-encoded transaction bytes (Base64).
    ///
    /// This is a `TransactionData` struct that has been BCS-encoded and then Base64-encoded.
    pub transaction_data_bcs: String,
}
