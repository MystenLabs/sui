// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::fmt::{Display, Formatter};

use multiaddr::Multiaddr;
use serde::{Deserialize, Serialize};

use crate::{NetworkPublicKey, ProtocolPublicKey};

/// Committee of the consensus protocol is updated each epoch.
pub type Epoch = u64;

/// Voting power of an authority, roughly proportional to the actual amount of Sui staked
/// by the authority.
/// Total stake / voting power of all authorities should sum to 10,000.
pub type Stake = u64;

/// Committee is the set of authorities that participate in the consensus protocol for this epoch.
/// Its configuration is stored and computed on chain.
#[derive(Serialize, Deserialize)]
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

    /// Public accessors for Committee data.

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
        self.authorities[authority_index.value()].stake
    }

    pub fn authority(&self, authority_index: AuthorityIndex) -> &Authority {
        &self.authorities[authority_index.value()]
    }

    pub fn authorities(&self) -> impl Iterator<Item = (AuthorityIndex, &Authority)> {
        self.authorities
            .iter()
            .enumerate()
            .map(|(i, a)| (AuthorityIndex(i as u32), a))
    }

    pub fn size(&self) -> usize {
        self.authorities.len()
    }
}

/// Represents one authority in the committee.
///
/// NOTE: this is intentionally un-cloneable, to encourage only copying relevant fields.
/// AuthorityIndex should be used to reference an authority instead.
#[derive(Serialize, Deserialize)]
pub struct Authority {
    /// Voting power of the authority in the committee.
    pub stake: Stake,
    /// Network address for communicating with the authority.
    pub address: Multiaddr,
    /// The validator's hostname, for metrics and logging.
    pub hostname: String,
    /// The authority's ed25519 publicKey for signing network messages and blocks.
    pub network_key: NetworkPublicKey,
    /// The authority's bls public key for random beacon.
    pub protocol_key: ProtocolPublicKey,
}

/// Each authority is uniquely identified by its AuthorityIndex in the Committee.
/// AuthorityIndex is between 0 (inclusive) and the total number of authorities (exclusive).
///
/// NOTE: AuthorityIndex should not need to be created outside of this file or incremented.
#[derive(
    Eq, PartialEq, Ord, PartialOrd, Clone, Copy, Debug, Default, Hash, Serialize, Deserialize,
)]
pub struct AuthorityIndex(u32);

impl AuthorityIndex {
    pub fn value(&self) -> usize {
        self.0 as usize
    }
}

impl Display for AuthorityIndex {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.0.to_string().as_str())
    }
}

#[cfg(test)]
mod tests {
    use crate::{Authority, Committee, NetworkKeyPair, ProtocolKeyPair, Stake};
    use fastcrypto::traits::KeyPair as _;
    use multiaddr::Multiaddr;
    use rand::{rngs::StdRng, SeedableRng};

    #[test]
    fn committee_basic() {
        // GIVEN
        let epoch = 100;

        let mut authorities = vec![];
        let mut rng = StdRng::from_seed([9; 32]);
        let num_of_authorities = 9;
        for i in 1..=num_of_authorities {
            let network_keypair = NetworkKeyPair::generate(&mut rng);
            let protocol_keypair = ProtocolKeyPair::generate(&mut rng);
            authorities.push(Authority {
                stake: i as Stake,
                address: Multiaddr::empty(),
                hostname: "test_host".to_string(),
                network_key: network_keypair.public().clone(),
                protocol_key: protocol_keypair.public().clone(),
            });
        }

        let committee = Committee::new(epoch, authorities);

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
