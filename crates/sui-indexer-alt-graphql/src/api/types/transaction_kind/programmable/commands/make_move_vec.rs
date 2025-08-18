// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use async_graphql::*;

use crate::api::types::transaction_kind::programmable::commands::TransactionArgument;

/// Create a vector (can be empty).
#[derive(SimpleObject, Clone)]
pub struct MakeMoveVecCommand {
    /// The values to pack into the vector, all of the same type.
    pub elements: Option<Vec<TransactionArgument>>,
    // TODO(DVX-1373): Support MoveType once available
}
