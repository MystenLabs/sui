// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::fmt::Display;

use consensus_config::AuthorityIndex;
use serde::{Deserialize, Serialize};

use crate::block::{BlockAPI, BlockRef, Round, Slot, VerifiedBlock};

/// Default wave length for all committers. A longer wave length increases the
/// chance of committing the leader under asynchrony at the cost of latency in
/// the common case.
pub(crate) const DEFAULT_WAVE_LENGTH: Round = MINIMUM_WAVE_LENGTH;

/// We need at least one leader round, one voting round, and one decision round.
pub(crate) const MINIMUM_WAVE_LENGTH: Round = 3;

/// The consensus protocol operates in 'waves'. Each wave is composed of a leader
/// round, at least one voting round, and one decision round.
#[allow(unused)]
pub(crate) type WaveNumber = u32;

/// Index of the commit.
pub(crate) type CommitIndex = u64;

/// Specifies one consensus commit.
/// It is stored on disk, so it does not contain blocks which are stored individually.
#[allow(unused)]
#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq)]
pub(crate) struct Commit {
    /// Index of the commit.
    /// First commit after genesis has an index of 1, then every next commit has an index incremented by 1.
    pub index: CommitIndex,
    /// A reference to the the commit leader.
    pub leader: BlockRef,
    /// Refs to committed blocks, in the commit order.
    pub blocks: Vec<BlockRef>,
    /// Last committed round per authority.
    pub last_committed_rounds: Vec<Round>,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
#[allow(unused)]
pub(crate) enum Decision {
    Direct,
    Indirect,
}

/// The status of every leader output by the committers. While the core only cares
/// about committed leaders, providing a richer status allows for easier debugging,
/// testing, and composition with advanced commit strategies.
#[allow(unused)]
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum LeaderStatus {
    Commit(VerifiedBlock),
    Skip(Slot),
    Undecided(Slot),
}

#[allow(unused)]
impl LeaderStatus {
    pub fn round(&self) -> Round {
        match self {
            Self::Commit(block) => block.round(),
            Self::Skip(leader) => leader.round,
            Self::Undecided(leader) => leader.round,
        }
    }

    pub fn authority(&self) -> AuthorityIndex {
        match self {
            Self::Commit(block) => block.author(),
            Self::Skip(leader) => leader.authority,
            Self::Undecided(leader) => leader.authority,
        }
    }

    pub fn is_decided(&self) -> bool {
        match self {
            Self::Commit(_) => true,
            Self::Skip(_) => true,
            Self::Undecided(_) => false,
        }
    }
}

impl Display for LeaderStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Commit(block) => write!(f, "Commit({})", block.reference()),
            Self::Skip(leader) => write!(f, "Skip({leader})"),
            Self::Undecided(leader) => write!(f, "Undecided({leader})"),
        }
    }
}
