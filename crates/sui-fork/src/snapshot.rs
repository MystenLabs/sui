// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::store::LocalState;

/// A point-in-time snapshot of the forked store's local state.
pub struct ForkedStoreSnapshot {
    pub state: LocalState,
    /// The consensus round counter at snapshot time, so revert can avoid duplicate digests.
    pub next_consensus_round: u64,
}
