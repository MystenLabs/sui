// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use async_graphql::*;

use crate::api::types::{
    move_type::MoveType, transaction_kind::programmable::commands::TransactionArgument,
};

/// Create a vector (can be empty).
#[derive(SimpleObject, Clone)]
pub struct MakeMoveVecCommand {
    /// If the elements are not objects, or the vector is empty, a type must be supplied.
    pub type_: Option<MoveType>,
    /// The values to pack into the vector, all of the same type.
    pub elements: Option<Vec<TransactionArgument>>,
}
