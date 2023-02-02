// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
#![warn(
    future_incompatible,
    nonstandard_style,
    rust_2018_idioms,
    rust_2021_compatibility
)]

pub mod bullshark;
pub mod consensus;
#[cfg(test)]
#[path = "tests/consensus_utils.rs"]
pub mod consensus_utils;
pub mod dag;
pub mod metrics;
mod utils;

pub use crate::consensus::Consensus;
use store::StoreError;
use thiserror::Error;

use types::SequenceNumber;

/// The default channel size used in the consensus and subscriber logic.
pub const DEFAULT_CHANNEL_SIZE: usize = 1_000;

/// The number of shutdown receivers to create on startup. We need one per component loop.
pub const NUM_SHUTDOWN_RECEIVERS: u64 = 25;

#[derive(Clone, Debug, Error)]
enum ConsensusError {
    #[error("Storage failure: {0}")]
    StoreError(#[from] StoreError),

    #[error("System shutting down")]
    ShuttingDown,
}
