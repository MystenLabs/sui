// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::balance::Balance;
use crate::base_types::SuiAddress;
use crate::collection_types::{Bag, Table};
use crate::committee::{Committee, CommitteeWithNetworkMetadata, NetworkMetadata};
use crate::crypto::AuthorityPublicKeyBytes;
use crate::sui_system_state::epoch_start_sui_system_state::{
    EpochStartSystemState, EpochStartValidatorInfoV1,
};
use crate::sui_system_state::sui_system_state_summary::{
    SuiSystemStateSummary, SuiValidatorSummary,
};
use crate::sui_system_state::SuiSystemStateTrait;
use fastcrypto::traits::ToFromBytes;
use mysten_network::Multiaddr;
use once_cell::sync::OnceCell;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Serialize, Deserialize, Clone, Eq, PartialEq)]
pub struct SimTestSuiSystemStateInnerV1 {
    pub epoch: u64,
    pub protocol_version: u64,
    pub system_state_version: u64,
    pub validators: SimTestValidatorSetV1,
    pub storage_fund: Balance,
    pub parameters: SimTestSystemParametersV1,
    pub reference_gas_price: u64,
    pub safe_mode: bool,
    pub epoch_start_timestamp_ms: u64,
    pub extra_fields: Bag,
}

#[derive(Debug, Serialize, Deserialize, Clone, Eq, PartialEq)]
pub struct SimTestSystemParametersV1 {
    pub epoch_duration_ms: u64,
    pub extra_fields: Bag,
}

#[derive(Debug, Serialize, Deserialize, Clone, Eq, PartialEq)]
pub struct SimTestValidatorSetV1 {
    pub active_validators: Vec<SimTestValidatorV1>,
    pub inactive_validators: Table,
    pub extra_fields: Bag,
}

#[derive(Debug, Serialize, Deserialize, Clone, Eq, PartialEq)]
pub struct SimTestValidatorV1 {
    metadata: SimTestValidatorMetadataV1,
    #[serde(skip)]
    verified_metadata: OnceCell<VerifiedSimTestValidatorMetadataV1>,
    pub voting_power: u64,
    pub stake: Balance,
    pub extra_fields: Bag,
}

impl SimTestValidatorV1 {
    pub fn verified_metadata(&self) -> &VerifiedSimTestValidatorMetadataV1 {
        self.verified_metadata
            .get_or_init(|| self.metadata.verify())
    }

    pub fn into_sui_validator_summary(self) -> SuiValidatorSummary {
        let Self {
            metadata:
                SimTestValidatorMetadataV1 {
                    sui_address,
                    protocol_pubkey_bytes,
                    network_pubkey_bytes,
                    worker_pubkey_bytes,
                    net_address,
                    p2p_address,
                    primary_address,
                    worker_address,
                    extra_fields: _,
                },
            verified_metadata: _,
            stake: _,
            voting_power,
            extra_fields: _,
        } = self;
        SuiValidatorSummary {
            sui_address,
            protocol_pubkey_bytes,
            network_pubkey_bytes,
            worker_pubkey_bytes,
            net_address,
            p2p_address,
            primary_address,
            worker_address,
            voting_power,
            ..Default::default()
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, Eq, PartialEq)]
pub struct SimTestValidatorMetadataV1 {
    pub sui_address: SuiAddress,
    pub protocol_pubkey_bytes: Vec<u8>,
    pub network_pubkey_bytes: Vec<u8>,
    pub worker_pubkey_bytes: Vec<u8>,
    pub net_address: String,
    pub p2p_address: String,
    pub primary_address: String,
    pub worker_address: String,
    pub extra_fields: Bag,
}

#[derive(Debug, Serialize, Deserialize, Clone, Eq, PartialEq)]
pub struct VerifiedSimTestValidatorMetadataV1 {
    pub sui_address: SuiAddress,
    pub protocol_pubkey: narwhal_crypto::PublicKey,
    pub network_pubkey: narwhal_crypto::NetworkPublicKey,
    pub worker_pubkey: narwhal_crypto::NetworkPublicKey,
    pub net_address: Multiaddr,
    pub p2p_address: Multiaddr,
    pub primary_address: Multiaddr,
    pub worker_address: Multiaddr,
}

impl SimTestValidatorMetadataV1 {
    pub fn verify(&self) -> VerifiedSimTestValidatorMetadataV1 {
        let protocol_pubkey =
            narwhal_crypto::PublicKey::from_bytes(self.protocol_pubkey_bytes.as_ref()).unwrap();
        let network_pubkey =
            narwhal_crypto::NetworkPublicKey::from_bytes(self.network_pubkey_bytes.as_ref())
                .unwrap();
        let worker_pubkey =
            narwhal_crypto::NetworkPublicKey::from_bytes(self.worker_pubkey_bytes.as_ref())
                .unwrap();
        let net_address = Multiaddr::try_from(self.net_address.clone()).unwrap();
        let p2p_address = Multiaddr::try_from(self.p2p_address.clone()).unwrap();
        let primary_address = Multiaddr::try_from(self.primary_address.clone()).unwrap();
        let worker_address = Multiaddr::try_from(self.worker_address.clone()).unwrap();
        VerifiedSimTestValidatorMetadataV1 {
            sui_address: self.sui_address,
            protocol_pubkey,
            network_pubkey,
            worker_pubkey,
            net_address,
            p2p_address,
            primary_address,
            worker_address,
        }
    }
}

impl VerifiedSimTestValidatorMetadataV1 {
    pub fn sui_pubkey_bytes(&self) -> AuthorityPublicKeyBytes {
        (&self.protocol_pubkey).into()
    }
}

impl SuiSystemStateTrait for SimTestSuiSystemStateInnerV1 {
    fn epoch(&self) -> u64 {
        self.epoch
    }

    fn reference_gas_price(&self) -> u64 {
        self.reference_gas_price
    }

    fn protocol_version(&self) -> u64 {
        self.protocol_version
    }

    fn system_state_version(&self) -> u64 {
        self.system_state_version
    }

    fn epoch_start_timestamp_ms(&self) -> u64 {
        self.epoch_start_timestamp_ms
    }

    fn epoch_duration_ms(&self) -> u64 {
        self.parameters.epoch_duration_ms
    }

    fn safe_mode(&self) -> bool {
        self.safe_mode
    }

    fn get_current_epoch_committee(&self) -> CommitteeWithNetworkMetadata {
        let mut voting_rights = BTreeMap::new();
        let mut network_metadata = BTreeMap::new();
        for validator in &self.validators.active_validators {
            let verified_metadata = validator.verified_metadata();
            let name = verified_metadata.sui_pubkey_bytes();
            voting_rights.insert(name, validator.voting_power);
            network_metadata.insert(
                name,
                NetworkMetadata {
                    network_address: verified_metadata.net_address.clone(),
                    narwhal_primary_address: verified_metadata.primary_address.clone(),
                },
            );
        }
        CommitteeWithNetworkMetadata {
            committee: Committee::new(self.epoch, voting_rights),
            network_metadata,
        }
    }

    fn into_epoch_start_state(self) -> EpochStartSystemState {
        EpochStartSystemState::new_v1(
            self.epoch,
            self.protocol_version,
            self.reference_gas_price,
            self.safe_mode,
            self.epoch_start_timestamp_ms,
            self.parameters.epoch_duration_ms,
            self.validators
                .active_validators
                .iter()
                .map(|validator| {
                    let metadata = validator.verified_metadata();
                    EpochStartValidatorInfoV1 {
                        sui_address: metadata.sui_address,
                        protocol_pubkey: metadata.protocol_pubkey.clone(),
                        narwhal_network_pubkey: metadata.network_pubkey.clone(),
                        narwhal_worker_pubkey: metadata.worker_pubkey.clone(),
                        sui_net_address: metadata.net_address.clone(),
                        p2p_address: metadata.p2p_address.clone(),
                        narwhal_primary_address: metadata.primary_address.clone(),
                        narwhal_worker_address: metadata.worker_address.clone(),
                        voting_power: validator.voting_power,
                    }
                })
                .collect(),
        )
    }

    fn into_sui_system_state_summary(self) -> SuiSystemStateSummary {
        let Self {
            epoch,
            protocol_version,
            system_state_version,
            validators:
                SimTestValidatorSetV1 {
                    active_validators,
                    inactive_validators:
                        Table {
                            id: inactive_pools_id,
                            size: inactive_pools_size,
                        },
                    extra_fields: _,
                },
            storage_fund,
            parameters:
                SimTestSystemParametersV1 {
                    epoch_duration_ms,
                    extra_fields: _,
                },
            reference_gas_price,
            safe_mode,
            epoch_start_timestamp_ms,
            extra_fields: _,
        } = self;
        SuiSystemStateSummary {
            epoch,
            protocol_version,
            system_state_version,
            storage_fund: storage_fund.value(),
            reference_gas_price,
            safe_mode,
            epoch_start_timestamp_ms,
            epoch_duration_ms,
            active_validators: active_validators
                .into_iter()
                .map(|v| v.into_sui_validator_summary())
                .collect(),
            inactive_pools_id,
            inactive_pools_size,
            ..Default::default()
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, Eq, PartialEq)]
pub struct SimTestSuiSystemStateInnerV2 {
    pub new_dummy_field: u64,
    pub epoch: u64,
    pub protocol_version: u64,
    pub system_state_version: u64,
    pub validators: SimTestValidatorSetV1,
    pub storage_fund: Balance,
    pub parameters: SimTestSystemParametersV1,
    pub reference_gas_price: u64,
    pub safe_mode: bool,
    pub epoch_start_timestamp_ms: u64,
    pub extra_fields: Bag,
}

impl SuiSystemStateTrait for SimTestSuiSystemStateInnerV2 {
    fn epoch(&self) -> u64 {
        self.epoch
    }

    fn reference_gas_price(&self) -> u64 {
        self.reference_gas_price
    }

    fn protocol_version(&self) -> u64 {
        self.protocol_version
    }

    fn system_state_version(&self) -> u64 {
        self.system_state_version
    }

    fn epoch_start_timestamp_ms(&self) -> u64 {
        self.epoch_start_timestamp_ms
    }

    fn epoch_duration_ms(&self) -> u64 {
        self.parameters.epoch_duration_ms
    }

    fn safe_mode(&self) -> bool {
        self.safe_mode
    }

    fn get_current_epoch_committee(&self) -> CommitteeWithNetworkMetadata {
        let mut voting_rights = BTreeMap::new();
        let mut network_metadata = BTreeMap::new();
        for validator in &self.validators.active_validators {
            let verified_metadata = validator.verified_metadata();
            let name = verified_metadata.sui_pubkey_bytes();
            voting_rights.insert(name, validator.voting_power);
            network_metadata.insert(
                name,
                NetworkMetadata {
                    network_address: verified_metadata.net_address.clone(),
                    narwhal_primary_address: verified_metadata.primary_address.clone(),
                },
            );
        }
        CommitteeWithNetworkMetadata {
            committee: Committee::new(self.epoch, voting_rights),
            network_metadata,
        }
    }

    fn into_epoch_start_state(self) -> EpochStartSystemState {
        EpochStartSystemState::new_v1(
            self.epoch,
            self.protocol_version,
            self.reference_gas_price,
            self.safe_mode,
            self.epoch_start_timestamp_ms,
            self.parameters.epoch_duration_ms,
            self.validators
                .active_validators
                .iter()
                .map(|validator| {
                    let metadata = validator.verified_metadata();
                    EpochStartValidatorInfoV1 {
                        sui_address: metadata.sui_address,
                        protocol_pubkey: metadata.protocol_pubkey.clone(),
                        narwhal_network_pubkey: metadata.network_pubkey.clone(),
                        narwhal_worker_pubkey: metadata.worker_pubkey.clone(),
                        sui_net_address: metadata.net_address.clone(),
                        p2p_address: metadata.p2p_address.clone(),
                        narwhal_primary_address: metadata.primary_address.clone(),
                        narwhal_worker_address: metadata.worker_address.clone(),
                        voting_power: validator.voting_power,
                    }
                })
                .collect(),
        )
    }

    fn into_sui_system_state_summary(self) -> SuiSystemStateSummary {
        // If you are making any changes to SuiSystemStateV1 or any of its dependent types before
        // mainnet, please also update SuiSystemStateSummary and its corresponding TS type.
        // Post-mainnet, we will need to introduce a new version.
        let Self {
            new_dummy_field: _,
            epoch,
            protocol_version,
            system_state_version,
            validators:
                SimTestValidatorSetV1 {
                    active_validators,
                    inactive_validators:
                        Table {
                            id: inactive_pools_id,
                            size: inactive_pools_size,
                        },
                    extra_fields: _,
                },
            storage_fund,
            parameters:
                SimTestSystemParametersV1 {
                    epoch_duration_ms,
                    extra_fields: _,
                },
            reference_gas_price,
            safe_mode,
            epoch_start_timestamp_ms,
            extra_fields: _,
        } = self;
        SuiSystemStateSummary {
            epoch,
            protocol_version,
            system_state_version,
            storage_fund: storage_fund.value(),
            reference_gas_price,
            safe_mode,
            epoch_start_timestamp_ms,
            epoch_duration_ms,
            active_validators: active_validators
                .into_iter()
                .map(|v| v.into_sui_validator_summary())
                .collect(),
            inactive_pools_id,
            inactive_pools_size,
            ..Default::default()
        }
    }
}
