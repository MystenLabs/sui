// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::{
    block::{Block, BlockAPI},
    utils::{format_authority_index, format_authority_round},
};

use serde::{Deserialize, Serialize};
use std::fmt::{self, Display};

use consensus_config::AuthorityIndex;

/// The consensus protocol operates in 'waves'. Each wave is composed of a leader
/// round, at least one voting round, and one decision round.
#[allow(unused)]
pub type WaveNumber = u32;

/// Round number of a block.
pub type Round = u32;

/// The status of every leader output by the committers. While the core only cares
/// about committed leaders, providing a richer status allows for easier debugging,
/// testing, and composition with advanced commit strategies.
#[allow(unused)]
#[derive(Debug, Clone, Eq, PartialEq)]
pub enum LeaderStatus {
    Commit(Block),
    Skip(AuthorityRound),
    Undecided(AuthorityRound),
}

#[allow(unused)]
impl LeaderStatus {
    pub fn round(&self) -> Round {
        match self {
            Self::Commit(block) => block.round(),
            Self::Skip(leader) => leader.round(),
            Self::Undecided(leader) => leader.round(),
        }
    }
}

impl Display for LeaderStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Commit(block) => write!(f, "Commit({})", block.reference()),
            Self::Skip(leader) => write!(
                f,
                "Skip({})",
                format_authority_round(leader.authority, leader.round)
            ),
            Self::Undecided(leader) => write!(
                f,
                "Undecided({})",
                format_authority_round(leader.authority, leader.round)
            ),
        }
    }
}

#[derive(Clone, Copy, Eq, PartialEq, Serialize, Deserialize, Default, Hash)]
pub struct AuthorityRound {
    authority: AuthorityIndex,
    round: Round,
}

#[allow(unused)]
impl AuthorityRound {
    pub fn new(authority: AuthorityIndex, round: Round) -> Self {
        Self { authority, round }
    }

    pub fn round(&self) -> Round {
        self.round
    }

    pub fn authority(&self) -> AuthorityIndex {
        self.authority
    }
}

impl fmt::Debug for AuthorityRound {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self)
    }
}

impl fmt::Display for AuthorityRound {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.authority.value() < 26 {
            write!(
                f,
                "{}{}",
                format_authority_index(self.authority),
                self.round
            )
        } else {
            write!(f, "[{:02}]{}", self.authority, self.round)
        }
    }
}
