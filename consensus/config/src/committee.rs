// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use multiaddr::Multiaddr;
use serde::{Deserialize, Serialize};

use crate::{NetworkPublicKey, ProtocolPublicKey};

/// Committee of the consensus protocol is updated each epoch.
pub type Epoch = u64;

/// Each authority is uniquely identified by its AuthorityIndex in the Committee.
/// AuthorityIndex is between 0 (inclusive) and the total number of authorities (exclusive).
pub type AuthorityIndex = u32;

/// Voting power of an authority, roughly proportional to the actual amount of Sui staked
/// by the authority.
/// Total stake / voting power of all authorities should sum to 10,000.
pub type Stake = u64;

/// Network information of one authority in the committee.
#[derive(Serialize, Deserialize)]
pub struct NetworkInfo {
    /// Network address for communicating with the authority.
    pub address: Multiaddr,
    /// The validator's hostname, for metrics and logging.
    pub hostname: String,
    /// The authority's ed25519 publicKey for signing network messages and blocks.
    pub network_key: NetworkPublicKey,
    /// The authority's bls public key for random beacon.
    pub protocol_key: ProtocolPublicKey,
}

/// Committee is the set of authorities that participate in the consensus protocol for this epoch.
/// Its configuration is computed and stored on chain, and passed from Sui.
#[derive(Serialize, Deserialize)]
pub struct Committee {
    /// The epoch number of this committee
    epoch: Epoch,
    /// Stakes of each authority.
    stakes: Vec<Stake>,
    /// Total stake in the committee.
    total_stake: Stake,
    /// The quorum threshold (2f+1).
    quorum_threshold: Stake,
    /// The validity threshold (f+1).
    validity_threshold: Stake,
    /// Network information of each authority.
    network_info: Vec<NetworkInfo>,
}

impl Committee {
    /// Committee should be created via the CommitteeBuilder - this is intentionally be marked as
    /// private method.
    fn new(epoch: Epoch, stakes: Vec<Stake>, network_info: Vec<NetworkInfo>) -> Self {
        assert_eq!(stakes.len(), network_info.len());
        let total_stake = stakes.iter().sum();
        assert_ne!(total_stake, 0, "Total stake cannot be zero!");
        let quorum_threshold = 2 * total_stake / 3 + 1;
        let validity_threshold = (total_stake + 2) / 3;
        Self {
            epoch,
            stakes,
            total_stake,
            quorum_threshold,
            validity_threshold,
            network_info,
        }
    }

    /// Returns the current epoch.
    pub fn epoch(&self) -> Epoch {
        self.epoch
    }

    pub fn stakes(&self) -> &[Stake] {
        &self.stakes
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

    pub fn network_info(&self) -> &[NetworkInfo] {
        &self.network_info
    }

    /// Returns the number of authorities.
    pub fn size(&self) -> usize {
        self.stakes.len()
    }
}

/// Use builder to construct a Committee.
pub struct CommitteeBuilder {
    epoch: Epoch,
    stakes: Vec<Stake>,
    network_info: Vec<NetworkInfo>,
}

impl CommitteeBuilder {
    /// Epoch is constant and cannot be updated later.
    pub fn new(epoch: Epoch) -> Self {
        Self {
            epoch,
            stakes: Vec::new(),
            network_info: Vec::new(),
        }
    }

    /// All authorities added to the CommitteeBuilder will be part of the Committee.
    pub fn add_authority(
        &mut self,
        stake: Stake,
        address: Multiaddr,
        hostname: String,
        network_key: NetworkPublicKey,
        protocol_key: ProtocolPublicKey,
    ) -> &mut Self {
        self.stakes.push(stake);
        self.network_info.push(NetworkInfo {
            address,
            hostname,
            network_key,
            protocol_key: protocol_key.clone(),
        });
        self
    }

    /// Consumes self and creates a Committee.
    pub fn build(self) -> Committee {
        Committee::new(self.epoch, self.stakes, self.network_info)
    }
}

#[cfg(test)]
mod tests {
    use crate::{CommitteeBuilder, NetworkKeyPair, ProtocolKeyPair, Stake};
    use fastcrypto::traits::KeyPair as _;
    use multiaddr::Multiaddr;
    use rand::{rngs::StdRng, SeedableRng};

    #[test]
    fn committee_builder() {
        // GIVEN
        let mut rng = StdRng::from_seed([9; 32]);
        let num_of_authorities = 9;

        let mut committee_builder = CommitteeBuilder::new(100);

        for i in 1..=num_of_authorities {
            let network_keypair = NetworkKeyPair::generate(&mut rng);
            let protocol_keypair = ProtocolKeyPair::generate(&mut rng);
            committee_builder.add_authority(
                i as Stake,
                Multiaddr::empty(),
                "test_host".to_string(),
                network_keypair.public().clone(),
                protocol_keypair.public().clone(),
            );
        }

        let committee = committee_builder.build();

        // THEN make sure the output Committee fields are populated correctly.
        assert_eq!(committee.size(), num_of_authorities);
        assert_eq!(committee.stakes().len(), committee.network_info().len());
        for (i, stake) in committee.stakes().iter().enumerate() {
            assert_eq!((i + 1) as Stake, *stake);
        }

        // AND ensure thresholds are calculated correctly.
        assert_eq!(committee.total_stake(), 45);
        assert_eq!(committee.quorum_threshold(), 31);
        assert_eq!(committee.validity_threshold(), 15);
    }
}
