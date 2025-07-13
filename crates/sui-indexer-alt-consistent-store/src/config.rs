// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use sui_default_config::DefaultConfig;

#[DefaultConfig]
pub(crate) struct ConsistencyConfig {
    /// The number of snapshots to keep in the buffer.
    pub snapshots: u64,

    /// The stride between checkpoints.
    pub stride: u64,

    /// The size of the buffer for storing checkpoints.
    pub buffer_size: usize,
}

impl Default for ConsistencyConfig {
    fn default() -> Self {
        Self {
            snapshots: 15000,
            stride: 1,
            buffer_size: 5000,
        }
    }
}
