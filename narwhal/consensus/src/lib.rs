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

use serde::{Deserialize, Serialize};
use types::{Certificate, SequenceNumber};

/// The default channel size used in the consensus and subscriber logic.
pub const DEFAULT_CHANNEL_SIZE: usize = 1_000;

/// The output format of the consensus.
#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct ConsensusOutput {
    /// The sequenced certificate.
    pub certificate: Certificate,
    /// The (global) index associated with this certificate.
    pub consensus_index: SequenceNumber,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct CommittedSubDag {
    /// The sequence of committed certificates.
    pub certificates: Vec<ConsensusOutput>,
    /// The leader certificate responsible of committing this sub-dag.
    pub leader: Certificate,
}

impl CommittedSubDag {
    pub fn len(&self) -> usize {
        self.certificates.len()
    }
}
