// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::{BTreeMap, HashMap};

use anemo::PeerId;
use multiaddr::Multiaddr;
use narwhal_config::{WorkerCache, WorkerIndex};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use sui_protocol_config::ProtocolVersion;

use crate::{
    base_types::{AuthorityName, SuiAddress},
    committee::{Committee, CommitteeWithNetAddresses, StakeUnit},
    sui_system_state::{SuiSystemState, SuiSystemStateTrait},
};

#[derive(Debug, Serialize, Deserialize, Clone, Eq, PartialEq, JsonSchema)]
pub struct SuiValidatorSummary {
    pub sui_address: SuiAddress,
    #[schemars(with = "&[u8]")]
    pub protocol_pubkey: narwhal_crypto::PublicKey,
    #[schemars(with = "&[u8]")]
    pub network_pubkey: narwhal_crypto::NetworkPublicKey,
    #[schemars(with = "&[u8]")]
    pub worker_pubkey: narwhal_crypto::NetworkPublicKey,
    pub proof_of_possession_bytes: Vec<u8>,
    pub name: String,
    pub description: String,
    pub image_url: String,
    pub project_url: String,
    #[schemars(with = "&[u8]")]
    pub net_address: Multiaddr,
    #[schemars(with = "&[u8]")]
    pub p2p_address: Multiaddr,
    #[schemars(with = "&[u8]")]
    pub primary_address: Multiaddr,
    #[schemars(with = "&[u8]")]
    pub worker_address: Multiaddr,
    pub voting_power: StakeUnit,
    // TODO: Add more fields.
}

impl SuiValidatorSummary {
    // Convenient method to get the public key of the validator as AuthorityName.
    pub fn pubkey_bytes(&self) -> AuthorityName {
        (&self.protocol_pubkey).into()
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, Eq, PartialEq, JsonSchema)]
pub struct SuiSystemStateSummary {
    epoch: u64,
    protocol_version: u64,
    active_validators: Vec<SuiValidatorSummary>,
    reference_gas_price: u64,
    safe_mode: bool,
    epoch_start_timestamp_ms: u64,
    // TODO: Add more fields
}

impl SuiSystemStateSummary {
    pub fn new_for_testing() -> Self {
        Self {
            epoch: 0,
            protocol_version: ProtocolVersion::MIN.as_u64(),
            active_validators: vec![],
            reference_gas_price: 0,
            safe_mode: false,
            epoch_start_timestamp_ms: 0,
        }
    }

    pub fn epoch(&self) -> u64 {
        self.epoch
    }

    pub fn protocol_version(&self) -> u64 {
        self.protocol_version
    }

    pub fn reference_gas_price(&self) -> u64 {
        self.reference_gas_price
    }

    pub fn safe_mode(&self) -> bool {
        self.safe_mode
    }

    pub fn epoch_start_timestamp_ms(&self) -> u64 {
        self.epoch_start_timestamp_ms
    }

    pub fn sui_committee(&self) -> Committee {
        let voting_rights = self
            .active_validators
            .iter()
            .map(|validator| (validator.pubkey_bytes(), validator.voting_power))
            .collect();
        Committee::new(self.epoch, voting_rights).expect("Committee information must be valid.")
    }

    pub fn sui_committee_with_net_addresses(&self) -> CommitteeWithNetAddresses {
        let net_addresses = self
            .active_validators
            .iter()
            // TODO: CommitteeWithNetAddresses should use Multiaddr instead of Vec<u8>.
            .map(|validator| {
                (
                    validator.pubkey_bytes(),
                    validator.net_address.clone().to_vec(),
                )
            })
            .collect();
        CommitteeWithNetAddresses {
            committee: self.sui_committee(),
            net_addresses,
        }
    }

    #[allow(clippy::mutable_key_type)]
    pub fn narwhal_committee(&self) -> narwhal_config::Committee {
        let narwhal_committee = self
            .active_validators
            .iter()
            .map(|validator| {
                let authority = narwhal_config::Authority {
                    stake: validator.voting_power as narwhal_config::Stake,
                    primary_address: validator.primary_address.clone(),
                    network_key: validator.network_pubkey.clone(),
                };
                (validator.protocol_pubkey.clone(), authority)
            })
            .collect();

        narwhal_config::Committee {
            authorities: narwhal_committee,
            epoch: self.epoch as narwhal_config::Epoch,
        }
    }

    #[allow(clippy::mutable_key_type)]
    pub fn narwhal_worker_cache(
        &self,
        transactions_address: &multiaddr::Multiaddr,
    ) -> narwhal_config::WorkerCache {
        let workers: BTreeMap<narwhal_crypto::PublicKey, WorkerIndex> = self
            .active_validators
            .iter()
            .map(|validator| {
                let workers = [(
                    0,
                    narwhal_config::WorkerInfo {
                        name: validator.worker_pubkey.clone(),
                        transactions: transactions_address.clone(),
                        worker_address: validator.worker_address.clone(),
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

    pub fn authority_names_to_peer_ids(&self) -> HashMap<AuthorityName, anemo::PeerId> {
        self.active_validators
            .iter()
            .map(|validator| {
                let name = validator.pubkey_bytes();
                let peer_id = PeerId(validator.network_pubkey.0.to_bytes());
                (name, peer_id)
            })
            .collect()
    }
}

impl From<SuiSystemState> for SuiSystemStateSummary {
    fn from(state: SuiSystemState) -> Self {
        Self {
            epoch: state.epoch(),
            protocol_version: state.protocol_version(),
            active_validators: state.get_validator_summary_vec(),
            reference_gas_price: state.reference_gas_price(),
            safe_mode: state.safe_mode(),
            epoch_start_timestamp_ms: state.epoch_start_timestamp_ms(),
        }
    }
}
