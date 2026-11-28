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

    /// The FreezeList version this transaction was sequenced against, recording the read for
    /// effects. `None` when no version was assigned.
    fn freeze_list_version(&self) -> Option<SequenceNumber>;
}
