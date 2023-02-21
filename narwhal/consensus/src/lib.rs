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
pub mod tusk;
mod utils;

pub use crate::consensus::Consensus;
use store::StoreError;
use thiserror::Error;

use types::{Certificate, Round, SequenceNumber};

/// The default channel size used in the consensus and subscriber logic.
pub const DEFAULT_CHANNEL_SIZE: usize = 1_000;

/// The number of shutdown receivers to create on startup. We need one per component loop.
pub const NUM_SHUTDOWN_RECEIVERS: u64 = 25;

#[derive(Clone, Debug, Error, PartialEq)]
pub enum ConsensusError {
    #[error("Storage failure: {0}")]
    StoreError(#[from] StoreError),

    #[error("Certificate {0:?} not inserted, is passed commit round {1}")]
    CertificatePassedCommit(Certificate, Round),

    #[error("Earlier certificate {0:?} already exists for round {1} when trying to insert {2:?}")]
    CertificateAlreadyExistsForRound(Certificate, Round, Certificate),

    #[error("System shutting down")]
    ShuttingDown,
}
