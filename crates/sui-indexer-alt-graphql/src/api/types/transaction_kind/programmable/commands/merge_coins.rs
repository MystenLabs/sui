// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use async_graphql::SimpleObject;

use crate::api::types::transaction_kind::programmable::commands::TransactionArgument;

/// Merges `coins` into the first `coin` (produces no results).
#[derive(SimpleObject, Clone)]
pub struct MergeCoinsCommand {
    /// The coin to merge into.
    pub coin: Option<TransactionArgument>,
    /// The coins to be merged.
    pub coins: Vec<TransactionArgument>,
}
