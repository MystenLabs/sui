// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

mod bullshark;
#[path = "tests/consensus_utils.rs"]
mod consensus_utils;
mod leader_schedule;
mod metrics;
mod state;
mod utils;

pub use crate::consensus::bullshark::Bullshark;
#[cfg(test)]
pub use crate::consensus::consensus_utils::{make_certificate_store, NUM_SUB_DAGS_PER_SCHEDULE};
pub use crate::consensus::leader_schedule::{LeaderSchedule, LeaderSwapTable};
pub use crate::consensus::metrics::{ChannelMetrics, ConsensusMetrics};
pub use crate::consensus::state::{Consensus, ConsensusRound, ConsensusState, Dag};
pub use crate::consensus::utils::gc_round;
pub use consensus_utils::make_consensus_store;

use store::StoreError;
use thiserror::Error;

use types::Certificate;

/// The default channel size used in the consensus and subscriber logic.
pub const DEFAULT_CHANNEL_SIZE: usize = 1_000;

/// The number of shutdown receivers to create on startup. We need one per component loop.
pub const NUM_SHUTDOWN_RECEIVERS: u64 = 25;

#[derive(Clone, Debug, Error, PartialEq)]
pub enum ConsensusError {
    #[error("Storage failure: {0}")]
    StoreError(#[from] StoreError),

    #[error("Certificate {0:?} equivocates with earlier certificate {1:?}")]
    CertificateEquivocation(Certificate, Certificate),

    #[error("System shutting down")]
    ShuttingDown,
}

#[derive(Debug, PartialEq, Eq)]
pub enum Outcome {
    // Certificate is not processed, since it's below the latest committed round for its origin.
    CertificateBelowCommitRound,

    // Certificate processed is of an even round, so the previous one is an odd round and
    // no leader election takes process.
    NoLeaderElectedForOddRound,

    // Leader has been elected, but it's below the latest commit round, so commit happens.
    LeaderBelowCommitRound,

    // Tried to do a leader election, but leader was not found for the round, not commit will
    // take place.
    LeaderNotFound,

    // Leader has been found,  but there was no enough support from the children nodes, so leader
    // can't be used to commit.
    NotEnoughSupportForLeader,

    // Processed Certificate triggered a commit.
    Commit,

    // When the schedule has changed during a commit, then this is return with everything that has
    // been committed so far.
    ScheduleChanged,
}
