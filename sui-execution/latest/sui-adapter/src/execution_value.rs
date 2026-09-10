// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use sui_types::storage::{
    ObjectFundsResolver, RuntimeObjectResolver, RuntimeSystemObjectResolver, Storage,
};

/// Interface with the store necessary to execute a programmable transaction
pub trait ExecutionState:
    Storage + RuntimeObjectResolver + RuntimeSystemObjectResolver + ObjectFundsResolver
{
}

impl<T> ExecutionState for T where
    T: Storage + RuntimeObjectResolver + RuntimeSystemObjectResolver + ObjectFundsResolver
{
}
