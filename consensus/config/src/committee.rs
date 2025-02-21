// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{
    fmt::{Display, Formatter},
    ops::{Index, IndexMut},
};

use mysten_network::Multiaddr;
use serde::{Deserialize, Serialize};

use crate::{AuthorityPublicKey, NetworkPublicKey, ProtocolPublicKey};

/// Committee of the consensus protocol is updated each epoch.
pub type Epoch = u64;

/// Voting power of an authority, roughly proportional to the actual amount of Sui staked
/// by the authority.
/// Total stake / voting power of all authorities should sum to 10,000.
pub type Stake = u64;

/// Committee is the set of authorities that participate in the consensus protocol for this epoch.
/// Its configuration is stored and computed on chain.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Committee {
    /// The epoch number of this committee
    epoch: Epoch,
    /// Total stake in the committee.
    total_stake: Stake,
    /// The quorum threshold (2f+1).
    quorum_threshold: Stake,
    /// The validity threshold (f+1).
    validity_threshold: Stake,
    /// Protocol and network info of each authority.
    authorities: Vec<Authority>,
}

impl Committee {
    pub fn new(epoch: Epoch, authorities: Vec<Authority>) -> Self {
        assert!(!authorities.is_empty(), "Committee cannot be empty!");
        assert!(
            authorities.len() < u32::MAX as usize,
            "Too many authorities ({})!",
            authorities.len()
        );

        let total_stake = authorities.iter().map(|a| a.stake).sum();
        assert_ne!(total_stake, 0, "Total stake cannot be zero!");
        let quorum_threshold = 2 * total_stake / 3 + 1;
        let validity_threshold = (total_stake + 2) / 3;
        Self {
            epoch,
            total_stake,
            quorum_threshold,
            validity_threshold,
            authorities,
        }
    }

    /// -----------------------------------------------------------------------
    /// Accessors to Committee fields.

    pub fn epoch(&self) -> Epoch {
        self.epoch
    }

    pub fn total_stake(&self) -> Stake {
        self.total_stake
    }

    pub fn quorum_threshold(&self) -> Stake {
        self.quorum_threshold
    }

    pub fn validity_threshold(&self) -> Stake {
        self.validity_threshold
    }

    pub fn stake(&self, authority_index: AuthorityIndex) -> Stake {
        self.authorities[authority_index].stake
    }

    pub fn authority(&self, authority_index: AuthorityIndex) -> &Authority {
        &self.authorities[authority_index]
    }

    pub fn authorities(&self) -> impl Iterator<Item = (AuthorityIndex, &Authority)> {
        self.authorities
            .iter()
            .enumerate()
            .map(|(i, a)| (AuthorityIndex(i as u32), a))
    }

    /// -----------------------------------------------------------------------
    /// Helpers for Committee properties.

    /// Returns true if the provided stake has reached quorum (2f+1).
    pub fn reached_quorum(&self, stake: Stake) -> bool {
        stake >= self.quorum_threshold()
    }

    /// Returns true if the provided stake has reached validity (f+1).
    pub fn reached_validity(&self, stake: Stake) -> bool {
        stake >= self.validity_threshold()
    }

    /// Coverts an index to an AuthorityIndex, if valid.
    /// Returns None if index is out of bound.
    pub fn to_authority_index(&self, index: usize) -> Option<AuthorityIndex> {
        if index < self.authorities.len() {
            Some(AuthorityIndex(index as u32))
        } else {
            None
        }
    }

    /// Returns true if the provided index is valid.
    pub fn is_valid_index(&self, index: AuthorityIndex) -> bool {
        index.value() < self.size()
    }

    /// Returns number of authorities in the committee.
    pub fn size(&self) -> usize {
        self.authorities.len()
    }
}

/// Represents one authority in the committee.
///
/// NOTE: this is intentionally un-cloneable, to encourage only copying relevant fields.
/// AuthorityIndex should be used to reference an authority instead.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Authority {
    /// Voting power of the authority in the committee.
    pub stake: Stake,
    /// Network address for communicating with the authority.
    pub address: Multiaddr,
    /// The authority's hostname, for metrics and logging.
    pub hostname: String,
    /// The authority's public key as Sui identity.
    pub authority_key: AuthorityPublicKey,
    /// The authority's public key for verifying blocks.
    pub protocol_key: ProtocolPublicKey,
    /// The authority's public key for TLS and as network identity.
    pub network_key: NetworkPublicKey,
}

/// Each authority is uniquely identified by its AuthorityIndex in the Committee.
/// AuthorityIndex is between 0 (inclusive) and the total number of authorities (exclusive).
///
/// NOTE: for safety, invalid AuthorityIndex should be impossible to create. So AuthorityIndex
/// should not be created or incremented outside of this file. AuthorityIndex received from peers
/// should be validated before use.
#[derive(
    Eq, PartialEq, Ord, PartialOrd, Clone, Copy, Debug, Default, Hash, Serialize, Deserialize,
)]
pub struct AuthorityIndex(u32);

impl AuthorityIndex {
    // Minimum committee size is 1, so 0 index is always valid.
    pub const ZERO: Self = Self(0);

    // Only for scanning rows in the database. Invalid elsewhere.
    pub const MIN: Self = Self::ZERO;
    pub const MAX: Self = Self(u32::MAX);

    pub fn value(&self) -> usize {
        self.0 as usize
    }
}

impl AuthorityIndex {
    pub fn new_for_test(index: u32) -> Self {
        Self(index)
    }
}

impl Display for AuthorityIndex {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "[{}]", self.value())
    }
}

impl<T, const N: usize> Index<AuthorityIndex> for [T; N] {
    type Output = T;

    fn index(&self, index: AuthorityIndex) -> &Self::Output {
        self.get(index.value()).unwrap()
    }
}

impl<T> Index<AuthorityIndex> for Vec<T> {
    type Output = T;

    fn index(&self, index: AuthorityIndex) -> &Self::Output {
        self.get(index.value()).unwrap()
    }
}

impl<T, const N: usize> IndexMut<AuthorityIndex> for [T; N] {
    fn index_mut(&mut self, index: AuthorityIndex) -> &mut Self::Output {
        self.get_mut(index.value()).unwrap()
    }
}

impl<T> IndexMut<AuthorityIndex> for Vec<T> {
    fn index_mut(&mut self, index: AuthorityIndex) -> &mut Self::Output {
        self.get_mut(index.value()).unwrap()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::local_committee_and_keys;

    #[test]
    fn committee_basic() {
        // GIVEN
        let epoch = 100;
        let num_of_authorities = 9;
        let authority_stakes = (1..=9).map(|s| s as Stake).collect();
        let (committee, _) = local_committee_and_keys(epoch, authority_stakes);

        // THEN make sure the output Committee fields are populated correctly.
        assert_eq!(committee.size(), num_of_authorities);
        for (i, authority) in committee.authorities() {
            assert_eq!((i.value() + 1) as Stake, authority.stake);
        }

        // AND ensure thresholds are calculated correctly.
        assert_eq!(committee.total_stake(), 45);
        assert_eq!(committee.quorum_threshold(), 31);
        assert_eq!(committee.validity_threshold(), 15);
    }
}
