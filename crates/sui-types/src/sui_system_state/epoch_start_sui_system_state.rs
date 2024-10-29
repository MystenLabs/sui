// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use enum_dispatch::enum_dispatch;
use std::collections::HashMap;

use crate::base_types::{AuthorityName, EpochId, SuiAddress};
use crate::committee::{Committee, CommitteeWithNetworkMetadata, NetworkMetadata, StakeUnit};
use crate::crypto::{AuthorityPublicKey, NetworkPublicKey};
use crate::multiaddr::Multiaddr;
use anemo::types::{PeerAffinity, PeerInfo};
use anemo::PeerId;
use consensus_config::{Authority, Committee as ConsensusCommittee};
use serde::{Deserialize, Serialize};
use sui_protocol_config::ProtocolVersion;
use tracing::{error, warn};

#[enum_dispatch]
pub trait EpochStartSystemStateTrait {
    fn epoch(&self) -> EpochId;
    fn protocol_version(&self) -> ProtocolVersion;
    fn reference_gas_price(&self) -> u64;
    fn safe_mode(&self) -> bool;
    fn epoch_start_timestamp_ms(&self) -> u64;
    fn epoch_duration_ms(&self) -> u64;
    fn get_validator_addresses(&self) -> Vec<SuiAddress>;
    fn get_sui_committee(&self) -> Committee;
    fn get_sui_committee_with_network_metadata(&self) -> CommitteeWithNetworkMetadata;
    fn get_consensus_committee(&self) -> ConsensusCommittee;
    fn get_validator_as_p2p_peers(&self, excluding_self: AuthorityName) -> Vec<PeerInfo>;
    fn get_authority_names_to_peer_ids(&self) -> HashMap<AuthorityName, PeerId>;
    fn get_authority_names_to_hostnames(&self) -> HashMap<AuthorityName, String>;
}

/// This type captures the minimum amount of information from SuiSystemState needed by a validator
/// to run the protocol. This allows us to decouple from the actual SuiSystemState type, and hence
/// do not need to evolve it when we upgrade the SuiSystemState type.
/// Evolving EpochStartSystemState is also a lot easier in that we could add optional fields
/// and fill them with None for older versions. When we absolutely must delete fields, we could
/// also add new db tables to store the new version. This is OK because we only store one copy of
/// this as part of EpochStartConfiguration for the most recent epoch in the db.
#[derive(Serialize, Deserialize, Debug, Eq, PartialEq)]
#[enum_dispatch(EpochStartSystemStateTrait)]
pub enum EpochStartSystemState {
    V1(EpochStartSystemStateV1),
}

impl EpochStartSystemState {
    pub fn new_v1(
        epoch: EpochId,
        protocol_version: u64,
        reference_gas_price: u64,
        safe_mode: bool,
        epoch_start_timestamp_ms: u64,
        epoch_duration_ms: u64,
        active_validators: Vec<EpochStartValidatorInfoV1>,
    ) -> Self {
        Self::V1(EpochStartSystemStateV1 {
            epoch,
            protocol_version,
            reference_gas_price,
            safe_mode,
            epoch_start_timestamp_ms,
            epoch_duration_ms,
            active_validators,
        })
    }

    pub fn new_for_testing_with_epoch(epoch: EpochId) -> Self {
        Self::V1(EpochStartSystemStateV1::new_for_testing_with_epoch(epoch))
    }

    pub fn new_at_next_epoch_for_testing(&self) -> Self {
        // Only need to support the latest version for testing.
        match self {
            Self::V1(state) => Self::V1(EpochStartSystemStateV1 {
                epoch: state.epoch + 1,
                protocol_version: state.protocol_version,
                reference_gas_price: state.reference_gas_price,
                safe_mode: state.safe_mode,
                epoch_start_timestamp_ms: state.epoch_start_timestamp_ms,
                epoch_duration_ms: state.epoch_duration_ms,
                active_validators: state.active_validators.clone(),
            }),
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Eq, PartialEq)]
pub struct EpochStartSystemStateV1 {
    epoch: EpochId,
    protocol_version: u64,
    reference_gas_price: u64,
    safe_mode: bool,
    epoch_start_timestamp_ms: u64,
    epoch_duration_ms: u64,
    active_validators: Vec<EpochStartValidatorInfoV1>,
}

impl EpochStartSystemStateV1 {
    pub fn new_for_testing() -> Self {
        Self::new_for_testing_with_epoch(0)
    }

    pub fn new_for_testing_with_epoch(epoch: EpochId) -> Self {
        Self {
            epoch,
            protocol_version: ProtocolVersion::MAX.as_u64(),
            reference_gas_price: crate::transaction::DEFAULT_VALIDATOR_GAS_PRICE,
            safe_mode: false,
            epoch_start_timestamp_ms: 0,
            epoch_duration_ms: 1000,
            active_validators: vec![],
        }
    }
}

impl EpochStartSystemStateTrait for EpochStartSystemStateV1 {
    fn epoch(&self) -> EpochId {
        self.epoch
    }

    fn protocol_version(&self) -> ProtocolVersion {
        ProtocolVersion::new(self.protocol_version)
    }

    fn reference_gas_price(&self) -> u64 {
        self.reference_gas_price
    }

    fn safe_mode(&self) -> bool {
        self.safe_mode
    }

    fn epoch_start_timestamp_ms(&self) -> u64 {
        self.epoch_start_timestamp_ms
    }

    fn epoch_duration_ms(&self) -> u64 {
        self.epoch_duration_ms
    }

    fn get_validator_addresses(&self) -> Vec<SuiAddress> {
        self.active_validators
            .iter()
            .map(|validator| validator.sui_address)
            .collect()
    }

    fn get_sui_committee_with_network_metadata(&self) -> CommitteeWithNetworkMetadata {
        let validators = self
            .active_validators
            .iter()
            .map(|validator| {
                (
                    validator.authority_name(),
                    (
                        validator.voting_power,
                        NetworkMetadata {
                            network_address: validator.sui_net_address.clone(),
                            narwhal_primary_address: validator.narwhal_primary_address.clone(),
                            network_public_key: Some(validator.narwhal_network_pubkey.clone()),
                        },
                    ),
                )
            })
            .collect();

        CommitteeWithNetworkMetadata::new(self.epoch, validators)
    }

    fn get_sui_committee(&self) -> Committee {
        let voting_rights = self
            .active_validators
            .iter()
            .map(|validator| (validator.authority_name(), validator.voting_power))
            .collect();
        Committee::new(self.epoch, voting_rights)
    }

    fn get_consensus_committee(&self) -> ConsensusCommittee {
        let mut authorities = vec![];
        for validator in self.active_validators.iter() {
            authorities.push(Authority {
                stake: validator.voting_power as consensus_config::Stake,
                // TODO(mysticeti): Add EpochStartValidatorInfoV2 with new field for mysticeti address.
                address: validator.narwhal_primary_address.clone(),
                hostname: validator.hostname.clone(),
                authority_key: consensus_config::AuthorityPublicKey::new(
                    validator.protocol_pubkey.clone(),
                ),
                protocol_key: consensus_config::ProtocolPublicKey::new(
                    validator.narwhal_worker_pubkey.clone(),
                ),
                network_key: consensus_config::NetworkPublicKey::new(
                    validator.narwhal_network_pubkey.clone(),
                ),
            });
        }

        // Sort the authorities by their protocol (public) key in ascending order, same as the order
        // in the Sui committee returned from get_sui_committee().
        authorities.sort_by(|a1, a2| a1.authority_key.cmp(&a2.authority_key));

        for ((i, mysticeti_authority), sui_authority_name) in authorities
            .iter()
            .enumerate()
            .zip(self.get_sui_committee().names())
        {
            if sui_authority_name.0 != mysticeti_authority.authority_key.to_bytes() {
                error!(
                    "Mismatched authority order between Sui and Mysticeti! Index {}, Mysticeti authority {:?}\nSui authority name {}",
                    i, mysticeti_authority, sui_authority_name
                );
            }
        }

        ConsensusCommittee::new(self.epoch as consensus_config::Epoch, authorities)
    }

    fn get_validator_as_p2p_peers(&self, excluding_self: AuthorityName) -> Vec<PeerInfo> {
        self.active_validators
            .iter()
            .filter(|validator| validator.authority_name() != excluding_self)
            .map(|validator| {
                let address = validator
                    .p2p_address
                    .to_anemo_address()
                    .into_iter()
                    .collect::<Vec<_>>();
                let peer_id = PeerId(validator.narwhal_network_pubkey.0.to_bytes());
                if address.is_empty() {
                    warn!(
                        ?peer_id,
                        "Peer has invalid p2p address: {}", &validator.p2p_address
                    );
                }
                PeerInfo {
                    peer_id,
                    affinity: PeerAffinity::High,
                    address,
                }
            })
            .collect()
    }

    fn get_authority_names_to_peer_ids(&self) -> HashMap<AuthorityName, PeerId> {
        self.active_validators
            .iter()
            .map(|validator| {
                let name = validator.authority_name();
                let peer_id = PeerId(validator.narwhal_network_pubkey.0.to_bytes());

                (name, peer_id)
            })
            .collect()
    }

    fn get_authority_names_to_hostnames(&self) -> HashMap<AuthorityName, String> {
        self.active_validators
            .iter()
            .map(|validator| {
                let name = validator.authority_name();
                let hostname = validator.hostname.clone();

                (name, hostname)
            })
            .collect()
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, Eq, PartialEq)]
pub struct EpochStartValidatorInfoV1 {
    pub sui_address: SuiAddress,
    pub protocol_pubkey: AuthorityPublicKey,
    pub narwhal_network_pubkey: NetworkPublicKey,
    pub narwhal_worker_pubkey: NetworkPublicKey,
    pub sui_net_address: Multiaddr,
    pub p2p_address: Multiaddr,
    pub narwhal_primary_address: Multiaddr,
    pub narwhal_worker_address: Multiaddr,
    pub voting_power: StakeUnit,
    pub hostname: String,
}

impl EpochStartValidatorInfoV1 {
    pub fn authority_name(&self) -> AuthorityName {
        (&self.protocol_pubkey).into()
    }
}

#[cfg(test)]
mod test {
    use crate::base_types::SuiAddress;
    use crate::committee::CommitteeTrait;
    use crate::crypto::{get_key_pair, AuthorityKeyPair, NetworkKeyPair};
    use crate::sui_system_state::epoch_start_sui_system_state::{
        EpochStartSystemStateTrait, EpochStartSystemStateV1, EpochStartValidatorInfoV1,
    };
    use fastcrypto::traits::KeyPair;
    use mysten_network::Multiaddr;
    use rand::thread_rng;
    use sui_protocol_config::ProtocolVersion;

    #[test]
    fn test_sui_and_mysticeti_committee_are_same() {
        // GIVEN
        let mut active_validators = vec![];

        for i in 0..10 {
            let (sui_address, protocol_key): (SuiAddress, AuthorityKeyPair) = get_key_pair();
            let narwhal_network_key = NetworkKeyPair::generate(&mut thread_rng());

            active_validators.push(EpochStartValidatorInfoV1 {
                sui_address,
                protocol_pubkey: protocol_key.public().clone(),
                narwhal_network_pubkey: narwhal_network_key.public().clone(),
                narwhal_worker_pubkey: narwhal_network_key.public().clone(),
                sui_net_address: Multiaddr::empty(),
                p2p_address: Multiaddr::empty(),
                narwhal_primary_address: Multiaddr::empty(),
                narwhal_worker_address: Multiaddr::empty(),
                voting_power: 1_000,
                hostname: format!("host-{i}").to_string(),
            })
        }

        let state = EpochStartSystemStateV1 {
            epoch: 10,
            protocol_version: ProtocolVersion::MAX.as_u64(),
            reference_gas_price: 0,
            safe_mode: false,
            epoch_start_timestamp_ms: 0,
            epoch_duration_ms: 0,
            active_validators,
        };

        // WHEN
        let sui_committee = state.get_sui_committee();
        let consensus_committee = state.get_consensus_committee();

        // THEN
        // assert the validators details
        assert_eq!(sui_committee.num_members(), 10);
        assert_eq!(sui_committee.num_members(), consensus_committee.size());
        assert_eq!(
            sui_committee.validity_threshold(),
            consensus_committee.validity_threshold()
        );
        assert_eq!(
            sui_committee.quorum_threshold(),
            consensus_committee.quorum_threshold()
        );
        assert_eq!(state.epoch, consensus_committee.epoch());

        for (authority_index, consensus_authority) in consensus_committee.authorities() {
            let sui_authority_name = sui_committee
                .authority_by_index(authority_index.value() as u32)
                .unwrap();

            assert_eq!(
                consensus_authority.authority_key.to_bytes(),
                sui_authority_name.0,
                "Mysten & SUI committee member of same index correspond to different public key"
            );
            assert_eq!(
                consensus_authority.stake,
                sui_committee.weight(sui_authority_name),
                "Mysten & SUI committee member stake differs"
            );
        }
    }
}
