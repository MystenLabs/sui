// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use sui_types::{
    base_types::SequenceNumber,
    storage::{ImplicitSystemObjectResolver, RuntimeObjectResolver, Storage},
};

/// Interface with the store necessary to execute a programmable transaction
pub trait ExecutionState: Storage + RuntimeObjectResolver + ImplicitSystemObjectResolver {
    /// The version given to every object the transaction writes.
    fn lamport_timestamp(&self) -> SequenceNumber;
}
