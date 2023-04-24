// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use enum_dispatch::enum_dispatch;
use std::collections::{BTreeMap, HashMap};

use crate::base_types::{AuthorityName, EpochId, SuiAddress};
use crate::committee::{Committee, StakeUnit};
use crate::multiaddr::Multiaddr;
use anemo::types::{PeerAffinity, PeerInfo};
use anemo::PeerId;
use narwhal_config::{Committee as NarwhalCommittee, CommitteeBuilder, WorkerCache, WorkerIndex};
use serde::{Deserialize, Serialize};
use sui_protocol_config::ProtocolVersion;
use tracing::warn;

#[enum_dispatch]
pub trait EpochStartSystemStateTrait {
    fn epoch(&self) -> EpochId;
    fn protocol_version(&self) -> ProtocolVersion;
    fn reference_gas_price(&self) -> u64;
    fn safe_mode(&self) -> bool;
    fn epoch_start_timestamp_ms(&self) -> u64;
    fn epoch_duration_ms(&self) -> u64;
    fn get_sui_committee(&self) -> Committee;
    fn get_narwhal_committee(&self) -> NarwhalCommittee;
    fn get_validator_as_p2p_peers(&self, excluding_self: AuthorityName) -> Vec<PeerInfo>;
    fn get_authority_names_to_peer_ids(&self) -> HashMap<AuthorityName, PeerId>;
    fn get_authority_names_to_hostnames(&self) -> HashMap<AuthorityName, String>;
    fn get_narwhal_worker_cache(&self, transactions_address: &Multiaddr) -> WorkerCache;
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
            reference_gas_price: 1,
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

    fn get_sui_committee(&self) -> Committee {
        let voting_rights = self
            .active_validators
            .iter()
            .map(|validator| (validator.authority_name(), validator.voting_power))
            .collect();
        Committee::new(self.epoch, voting_rights)
    }

    #[allow(clippy::mutable_key_type)]
    fn get_narwhal_committee(&self) -> NarwhalCommittee {
        let mut committee_builder = CommitteeBuilder::new(self.epoch as narwhal_config::Epoch);

        for validator in self.active_validators.iter() {
            committee_builder = committee_builder.add_authority(
                validator.protocol_pubkey.clone(),
                validator.voting_power as narwhal_config::Stake,
                validator.narwhal_primary_address.clone(),
                validator.narwhal_network_pubkey.clone(),
            );
        }

        committee_builder.build()
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

    #[allow(clippy::mutable_key_type)]
    fn get_narwhal_worker_cache(&self, transactions_address: &Multiaddr) -> WorkerCache {
        let workers: BTreeMap<narwhal_crypto::PublicKey, WorkerIndex> = self
            .active_validators
            .iter()
            .map(|validator| {
                let workers = [(
                    0,
                    narwhal_config::WorkerInfo {
                        name: validator.narwhal_worker_pubkey.clone(),
                        transactions: transactions_address.clone(),
                        worker_address: validator.narwhal_worker_address.clone(),
                    },
                )]
                .into_iter()
                .collect();
                let worker_index = WorkerIndex(workers);

                (validator.protocol_pubkey.clone(), worker_index)
            })
            .collect();
        WorkerCache {
            workers,
            epoch: self.epoch,
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Eq, PartialEq)]
pub struct EpochStartValidatorInfoV1 {
    pub sui_address: SuiAddress,
    pub protocol_pubkey: narwhal_crypto::PublicKey,
    pub narwhal_network_pubkey: narwhal_crypto::NetworkPublicKey,
    pub narwhal_worker_pubkey: narwhal_crypto::NetworkPublicKey,
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
