// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::base_types::{AuthorityName, ObjectID, SuiAddress};
use crate::collection_types::{Table, TableVec, VecMap, VecSet};
use crate::committee::{
    Committee, CommitteeWithNetworkMetadata, NetworkMetadata, ProtocolVersion, StakeUnit,
};
use crate::crypto::AuthorityPublicKeyBytes;
use crate::dynamic_field::{derive_dynamic_field_id, Field};
use crate::error::SuiError;
use crate::storage::ObjectStore;
use crate::sui_serde::AsMultiaddr;
use crate::sui_serde::Readable;
use crate::id::ID;
use crate::sui_system_state::epoch_start_sui_system_state::{
    EpochStartSystemState, EpochStartValidatorInfo,
};
use crate::{balance::Balance, id::UID, SUI_FRAMEWORK_ADDRESS, SUI_SYSTEM_STATE_OBJECT_ID};
use anyhow::Result;
use enum_dispatch::enum_dispatch;
use fastcrypto::encoding::Base58;
use fastcrypto::traits::ToFromBytes;
use multiaddr::Multiaddr;
use serde::{Deserialize, Serialize};
use serde_with::serde_as;
use std::collections::BTreeMap;

use super::sui_system_state_summary::{SuiSystemStateSummary, SuiValidatorSummary};
use super::SuiSystemStateTrait;

const E_METADATA_INVALID_PUBKEY: u64 = 1;
const E_METADATA_INVALID_NET_PUBKEY: u64 = 2;
const E_METADATA_INVALID_WORKER_PUBKEY: u64 = 3;
const E_METADATA_INVALID_NET_ADDR: u64 = 4;
const E_METADATA_INVALID_P2P_ADDR: u64 = 5;
const E_METADATA_INVALID_PRIMARY_ADDR: u64 = 6;
const E_METADATA_INVALID_WORKER_ADDR: u64 = 7;

/// Rust version of the Move sui::sui_system::SystemParameters type
#[derive(Debug, Serialize, Deserialize, Clone, Eq, PartialEq)]
// TODO: Get rid of json schema once we deprecate getSuiSystemState RPC API.
#[serde(rename_all = "camelCase", rename = "SystemParameters")]
pub struct SystemParametersV1 {
    pub min_validator_stake: u64,
    pub max_validator_count: u64,
    pub governance_start_epoch: u64,
}

#[serde_as]
#[derive(Debug, Serialize, Deserialize, Clone, Eq, PartialEq)]
// TODO: Get rid of json schema once we deprecate getSuiSystemState RPC API.
#[serde(rename_all = "camelCase", rename = "ValidatorMetadata")]
pub struct ValidatorMetadataV1 {
    pub sui_address: SuiAddress,
    #[schemars(with = "Base58")]
    #[serde_as(as = "Readable<Base58, _>")]
    pub protocol_pubkey_bytes: Vec<u8>,
    #[schemars(with = "Base58")]
    #[serde_as(as = "Readable<Base58, _>")]
    pub network_pubkey_bytes: Vec<u8>,
    #[schemars(with = "Base58")]
    #[serde_as(as = "Readable<Base58, _>")]
    pub worker_pubkey_bytes: Vec<u8>,
    #[schemars(with = "Base58")]
    #[serde_as(as = "Readable<Base58, _>")]
    pub proof_of_possession_bytes: Vec<u8>,
    pub name: String,
    pub description: String,
    pub image_url: String,
    pub project_url: String,
    #[schemars(with = "String")]
    #[serde_as(as = "Readable<AsMultiaddr, _>")]
    pub net_address: Vec<u8>,
    #[schemars(with = "String")]
    #[serde_as(as = "Readable<AsMultiaddr, _>")]
    pub p2p_address: Vec<u8>,
    #[schemars(with = "String")]
    #[serde_as(as = "Readable<AsMultiaddr, _>")]
    pub primary_address: Vec<u8>,
    #[schemars(with = "String")]
    #[serde_as(as = "Readable<AsMultiaddr, _>")]
    pub worker_address: Vec<u8>,
    #[schemars(with = "Option<Base58>")]
    #[serde_as(as = "Readable<Option<Base58>, _>")]
    pub next_epoch_protocol_pubkey_bytes: Option<Vec<u8>>,
    #[schemars(with = "Option<Base58>")]
    #[serde_as(as = "Readable<Option<Base58>, _>")]
    pub next_epoch_proof_of_possession: Option<Vec<u8>>,
    #[schemars(with = "Option<Base58>")]
    #[serde_as(as = "Readable<Option<Base58>, _>")]
    pub next_epoch_network_pubkey_bytes: Option<Vec<u8>>,
    #[schemars(with = "Option<Base58>")]
    #[serde_as(as = "Readable<Option<Base58>, _>")]
    pub next_epoch_worker_pubkey_bytes: Option<Vec<u8>>,
    #[schemars(with = "Option<String>")]
    #[serde_as(as = "Readable<Option<AsMultiaddr>, _>")]
    pub next_epoch_net_address: Option<Vec<u8>>,
    #[schemars(with = "Option<String>")]
    #[serde_as(as = "Readable<Option<AsMultiaddr>, _>")]
    pub next_epoch_p2p_address: Option<Vec<u8>>,
    #[schemars(with = "Option<String>")]
    #[serde_as(as = "Readable<Option<AsMultiaddr>, _>")]
    pub next_epoch_primary_address: Option<Vec<u8>>,
    #[schemars(with = "Option<String>")]
    #[serde_as(as = "Readable<Option<AsMultiaddr>, _>")]
    pub next_epoch_worker_address: Option<Vec<u8>>,
}

#[derive(Debug, Clone)]
pub struct VerifiedValidatorMetadataV1 {
    pub sui_address: SuiAddress,
    pub protocol_pubkey: narwhal_crypto::PublicKey,
    pub network_pubkey: narwhal_crypto::NetworkPublicKey,
    pub worker_pubkey: narwhal_crypto::NetworkPublicKey,
    pub proof_of_possession_bytes: Vec<u8>,
    pub name: String,
    pub description: String,
    pub image_url: String,
    pub project_url: String,
    pub net_address: Multiaddr,
    pub p2p_address: Multiaddr,
    pub primary_address: Multiaddr,
    pub worker_address: Multiaddr,
    pub next_epoch_protocol_pubkey: Option<narwhal_crypto::PublicKey>,
    pub next_epoch_proof_of_possession: Option<Vec<u8>>,
    pub next_epoch_network_pubkey: Option<narwhal_crypto::NetworkPublicKey>,
    pub next_epoch_worker_pubkey: Option<narwhal_crypto::NetworkPublicKey>,
    pub next_epoch_net_address: Option<Multiaddr>,
    pub next_epoch_p2p_address: Option<Multiaddr>,
    pub next_epoch_primary_address: Option<Multiaddr>,
    pub next_epoch_worker_address: Option<Multiaddr>,
}

impl ValidatorMetadataV1 {
    /// Verify validator metadata and return a verified version (on success) or error code (on failure)
    pub fn verify(&self) -> Result<VerifiedValidatorMetadataV1, u64> {
        // TODO: move the proof of possession verification here

        let protocol_pubkey =
            narwhal_crypto::PublicKey::from_bytes(self.protocol_pubkey_bytes.as_ref())
                .map_err(|_| E_METADATA_INVALID_PUBKEY)?;
        let network_pubkey =
            narwhal_crypto::NetworkPublicKey::from_bytes(self.network_pubkey_bytes.as_ref())
                .map_err(|_| E_METADATA_INVALID_NET_PUBKEY)?;
        let worker_pubkey =
            narwhal_crypto::NetworkPublicKey::from_bytes(self.worker_pubkey_bytes.as_ref())
                .map_err(|_| E_METADATA_INVALID_WORKER_PUBKEY)?;
        let net_address = Multiaddr::try_from(self.net_address.clone())
            .map_err(|_| E_METADATA_INVALID_NET_ADDR)?;
        let p2p_address = Multiaddr::try_from(self.p2p_address.clone())
            .map_err(|_| E_METADATA_INVALID_P2P_ADDR)?;
        // Also make sure that the p2p address is a valid anemo address.
        // TODO: This will trigger a bunch of Move test failures today since we did not give proper
        // value for p2p address.
        // multiaddr_to_anemo_address(&p2p_address).ok_or(E_METADATA_INVALID_P2P_ADDR)?;
        let primary_address = Multiaddr::try_from(self.primary_address.clone())
            .map_err(|_| E_METADATA_INVALID_PRIMARY_ADDR)?;
        let worker_address = Multiaddr::try_from(self.worker_address.clone())
            .map_err(|_| E_METADATA_INVALID_WORKER_ADDR)?;

        let next_epoch_protocol_pubkey = match self.next_epoch_protocol_pubkey_bytes.clone() {
            None => Ok::<Option<narwhal_crypto::PublicKey>, u64>(None),
            Some(bytes) => Ok(Some(
                narwhal_crypto::PublicKey::from_bytes(bytes.as_ref())
                    .map_err(|_| E_METADATA_INVALID_PUBKEY)?,
            )),
        }?;

        let next_epoch_network_pubkey = match self.next_epoch_network_pubkey_bytes.clone() {
            None => Ok::<Option<narwhal_crypto::NetworkPublicKey>, u64>(None),
            Some(bytes) => Ok(Some(
                narwhal_crypto::NetworkPublicKey::from_bytes(bytes.as_ref())
                    .map_err(|_| E_METADATA_INVALID_NET_PUBKEY)?,
            )),
        }?;

        let next_epoch_worker_pubkey: Option<narwhal_crypto::NetworkPublicKey> =
            match self.next_epoch_worker_pubkey_bytes.clone() {
                None => Ok::<Option<narwhal_crypto::NetworkPublicKey>, u64>(None),
                Some(bytes) => Ok(Some(
                    narwhal_crypto::NetworkPublicKey::from_bytes(bytes.as_ref())
                        .map_err(|_| E_METADATA_INVALID_WORKER_PUBKEY)?,
                )),
            }?;

        let next_epoch_net_address = match self.next_epoch_net_address.clone() {
            None => Ok::<Option<Multiaddr>, u64>(None),
            Some(address) => Ok(Some(
                Multiaddr::try_from(address).map_err(|_| E_METADATA_INVALID_NET_ADDR)?,
            )),
        }?;

        let next_epoch_p2p_address = match self.next_epoch_p2p_address.clone() {
            None => Ok::<Option<Multiaddr>, u64>(None),
            Some(address) => Ok(Some(
                Multiaddr::try_from(address).map_err(|_| E_METADATA_INVALID_P2P_ADDR)?,
            )),
        }?;

        let next_epoch_primary_address = match self.next_epoch_primary_address.clone() {
            None => Ok::<Option<Multiaddr>, u64>(None),
            Some(address) => Ok(Some(
                Multiaddr::try_from(address).map_err(|_| E_METADATA_INVALID_PRIMARY_ADDR)?,
            )),
        }?;

        let next_epoch_worker_address = match self.next_epoch_worker_address.clone() {
            None => Ok::<Option<Multiaddr>, u64>(None),
            Some(address) => Ok(Some(
                Multiaddr::try_from(address).map_err(|_| E_METADATA_INVALID_WORKER_ADDR)?,
            )),
        }?;

        Ok(VerifiedValidatorMetadataV1 {
            sui_address: self.sui_address,
            protocol_pubkey,
            network_pubkey,
            worker_pubkey,
            proof_of_possession_bytes: self.proof_of_possession_bytes.clone(),
            name: self.name.clone(),
            description: self.description.clone(),
            image_url: self.image_url.clone(),
            project_url: self.project_url.clone(),
            net_address,
            p2p_address,
            primary_address,
            worker_address,
            next_epoch_protocol_pubkey,
            next_epoch_proof_of_possession: self.next_epoch_proof_of_possession.clone(),
            next_epoch_network_pubkey,
            next_epoch_worker_pubkey,
            next_epoch_net_address,
            next_epoch_p2p_address,
            next_epoch_primary_address,
            next_epoch_worker_address,
        })
    }
}

impl ValidatorMetadataV1 {
    pub fn network_address(&self) -> Result<Multiaddr> {
        Multiaddr::try_from(self.net_address.clone()).map_err(Into::into)
    }

    pub fn p2p_address(&self) -> Result<Multiaddr> {
        Multiaddr::try_from(self.p2p_address.clone()).map_err(Into::into)
    }
}

/// Rust version of the Move sui::validator::Validator type
#[derive(Debug, Serialize, Deserialize, Clone, Eq, PartialEq)]
// TODO: Get rid of json schema once we deprecate getSuiSystemState RPC API.
#[serde(rename_all = "camelCase", rename = "Validator")]
pub struct ValidatorV1 {
    pub metadata: ValidatorMetadataV1,
    pub voting_power: u64,
    pub operation_cap_id: ID,
    pub gas_price: u64,
    pub staking_pool: StakingPoolV1,
    pub commission_rate: u64,
    pub next_epoch_stake: u64,
    pub next_epoch_gas_price: u64,
    pub next_epoch_commission_rate: u64,
}

impl ValidatorV1 {
    pub fn to_stake_and_network_metadata(&self) -> (AuthorityName, StakeUnit, NetworkMetadata) {
        (
            // TODO: Make sure we are actually verifying this on-chain.
            AuthorityPublicKeyBytes::from_bytes(self.metadata.protocol_pubkey_bytes.as_ref())
                .expect("Validity of public key bytes should be verified on-chain"),
            self.voting_power,
            NetworkMetadata {
                network_address: self
                    .metadata
                    .network_address()
                    .expect("Validity of network address should be verified on-chain"),
            },
        )
    }

    pub fn authority_name(&self) -> AuthorityName {
        AuthorityPublicKeyBytes::from_bytes(self.metadata.protocol_pubkey_bytes.as_ref())
            .expect("Validity of public key bytes should be verified on-chain")
    }

    pub fn into_sui_validator_summary(self) -> SuiValidatorSummary {
        let Self {
            metadata:
                ValidatorMetadataV1 {
                    sui_address,
                    protocol_pubkey_bytes,
                    network_pubkey_bytes,
                    worker_pubkey_bytes,
                    proof_of_possession_bytes,
                    name,
                    description,
                    image_url,
                    project_url,
                    net_address,
                    p2p_address,
                    primary_address,
                    worker_address,
                    next_epoch_protocol_pubkey_bytes,
                    next_epoch_proof_of_possession,
                    next_epoch_network_pubkey_bytes,
                    next_epoch_worker_pubkey_bytes,
                    next_epoch_net_address,
                    next_epoch_p2p_address,
                    next_epoch_primary_address,
                    next_epoch_worker_address,
                },
            voting_power,
            operation_cap_id,
            gas_price,
            staking_pool:
                StakingPoolV1 {
                    id: staking_pool_id,
                    activation_epoch: staking_pool_activation_epoch,
                    deactivation_epoch: staking_pool_deactivation_epoch,
                    sui_balance: staking_pool_sui_balance,
                    rewards_pool,
                    pool_token_balance,
                    exchange_rates:
                        Table {
                            id: exchange_rates_id,
                            size: exchange_rates_size,
                        },
                    pending_delegation,
                    pending_total_sui_withdraw,
                    pending_pool_token_withdraw,
                },
            commission_rate,
            next_epoch_stake,
            next_epoch_gas_price,
            next_epoch_commission_rate,
        } = self;
        SuiValidatorSummary {
            sui_address,
            protocol_pubkey_bytes,
            network_pubkey_bytes,
            worker_pubkey_bytes,
            proof_of_possession_bytes,
            name,
            description,
            image_url,
            project_url,
            net_address,
            p2p_address,
            primary_address,
            worker_address,
            next_epoch_protocol_pubkey_bytes,
            next_epoch_proof_of_possession,
            next_epoch_network_pubkey_bytes,
            next_epoch_worker_pubkey_bytes,
            next_epoch_net_address,
            next_epoch_p2p_address,
            next_epoch_primary_address,
            next_epoch_worker_address,
            voting_power,
            operation_cap_id,
            gas_price,
            staking_pool_id,
            staking_pool_activation_epoch,
            staking_pool_deactivation_epoch,
            staking_pool_sui_balance,
            rewards_pool: rewards_pool.value(),
            pool_token_balance,
            exchange_rates_id,
            exchange_rates_size,
            pending_delegation,
            pending_total_sui_withdraw,
            pending_pool_token_withdraw,
            commission_rate,
            next_epoch_stake,
            next_epoch_gas_price,
            next_epoch_commission_rate,
        }
    }
}

/// Rust version of the Move sui::staking_pool::StakingPool type
#[derive(Debug, Serialize, Deserialize, Clone, Eq, PartialEq)]
// TODO: Get rid of json schema once we deprecate getSuiSystemState RPC API.
#[serde(rename_all = "camelCase", rename = "StakingPool")]
pub struct StakingPoolV1 {
    pub id: ObjectID,
    pub activation_epoch: Option<u64>,
    pub deactivation_epoch: Option<u64>,
    pub sui_balance: u64,
    pub rewards_pool: Balance,
    pub pool_token_balance: u64,
    pub exchange_rates: Table,
    pub pending_delegation: u64,
    pub pending_total_sui_withdraw: u64,
    pub pending_pool_token_withdraw: u64,
}

#[derive(Debug, Serialize, Deserialize, Clone, Eq, PartialEq)]
pub struct PoolTokenExchangeRate {
    sui_amount: u64,
    pool_token_amount: u64,
}

impl PoolTokenExchangeRate {
    pub fn rate(&self) -> f64 {
        self.pool_token_amount as f64 / self.sui_amount as f64
    }
}

/// Rust version of the Move sui::validator_set::ValidatorSet type
#[derive(Debug, Serialize, Deserialize, Clone, Eq, PartialEq)]
// TODO: Get rid of json schema once we deprecate getSuiSystemState RPC API.
#[serde(rename_all = "camelCase", rename = "ValidatorSet")]
pub struct ValidatorSetV1 {
    pub total_stake: u64,
    pub active_validators: Vec<ValidatorV1>,
    pub pending_active_validators: TableVec,
    pub pending_removals: Vec<u64>,
    pub staking_pool_mappings: Table,
    pub inactive_pools: Table,
    pub validator_candidates: Table,
}

/// Rust version of the Move sui::sui_system::SuiSystemStateInner type
/// We want to keep it named as SuiSystemState in Rust since this is the primary interface type.
#[derive(Debug, Serialize, Deserialize, Clone, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SuiSystemStateInnerV1 {
    pub epoch: u64,
    pub protocol_version: u64,
    pub validators: ValidatorSetV1,
    pub storage_fund: Balance,
    pub parameters: SystemParametersV1,
    pub reference_gas_price: u64,
    pub validator_report_records: VecMap<SuiAddress, VecSet<SuiAddress>>,
    pub stake_subsidy: StakeSubsidyV1,
    pub safe_mode: bool,
    pub epoch_start_timestamp_ms: u64,
    // TODO: Use getters instead of all pub.
}

#[derive(Debug, Serialize, Deserialize, Clone, Eq, PartialEq)]
// TODO: Get rid of json schema once we deprecate getSuiSystemState RPC API.
#[serde(rename_all = "camelCase", rename = "StakeSubsidy")]
pub struct StakeSubsidyV1 {
    pub epoch_counter: u64,
    pub balance: Balance,
    pub current_epoch_amount: u64,
}

impl SuiSystemStateTrait for SuiSystemStateInnerV1 {
    fn epoch(&self) -> u64 {
        self.epoch
    }

    fn reference_gas_price(&self) -> u64 {
        self.reference_gas_price
    }

    fn protocol_version(&self) -> u64 {
        self.protocol_version
    }

    fn epoch_start_timestamp_ms(&self) -> u64 {
        self.epoch_start_timestamp_ms
    }

    fn safe_mode(&self) -> bool {
        self.safe_mode
    }

    fn get_current_epoch_committee(&self) -> CommitteeWithNetworkMetadata {
        let mut voting_rights = BTreeMap::new();
        let mut network_metadata = BTreeMap::new();
        for validator in &self.validators.active_validators {
            let (name, voting_stake, metadata) = validator.to_stake_and_network_metadata();
            voting_rights.insert(name, voting_stake);
            network_metadata.insert(name, metadata);
        }
        CommitteeWithNetworkMetadata {
            committee: Committee::new(self.epoch, voting_rights)
                // unwrap is safe because we should have verified the committee on-chain.
                // TODO: Make sure we actually verify it.
                .unwrap(),
            network_metadata,
        }
    }

    fn get_validator_metadata_vec(&self) -> Vec<ValidatorMetadataV1> {
        self.validators
            .active_validators
            .iter()
            .map(|v| v.metadata.clone())
            .collect()
    }

    fn into_epoch_start_state(self) -> EpochStartSystemState {
        EpochStartSystemState {
            epoch: self.epoch,
            protocol_version: self.protocol_version,
            reference_gas_price: self.reference_gas_price,
            safe_mode: self.safe_mode,
            epoch_start_timestamp_ms: self.epoch_start_timestamp_ms,
            active_validators: self
                .validators
                .active_validators
                .iter()
                .map(|validator| {
                    let metadata = validator
                        .metadata
                        .verify()
                        .expect("Validator metadata must have been verified on-chain");
                    EpochStartValidatorInfo {
                        sui_address: metadata.sui_address,
                        protocol_pubkey: metadata.protocol_pubkey,
                        narwhal_network_pubkey: metadata.network_pubkey,
                        narwhal_worker_pubkey: metadata.worker_pubkey,
                        sui_net_address: metadata.net_address,
                        p2p_address: metadata.p2p_address,
                        narwhal_primary_address: metadata.primary_address,
                        narwhal_worker_address: metadata.worker_address,
                        voting_power: validator.voting_power,
                    }
                })
                .collect(),
        }
    }

    fn into_sui_system_state_summary(self) -> SuiSystemStateSummary {
        // If you are making any changes to SuiSystemStateV1 or any of its dependent types before
        // mainnet, please also update SuiSystemStateSummary and its corresponding TS type.
        // Post-mainnet, we will need to introduce a new version.
        let Self {
            epoch,
            protocol_version,
            validators:
                ValidatorSetV1 {
                    total_stake,
                    active_validators,
                    pending_active_validators:
                        TableVec {
                            contents:
                                Table {
                                    id: pending_active_validators_id,
                                    size: pending_active_validators_size,
                                },
                        },
                    pending_removals,
                    staking_pool_mappings:
                        Table {
                            id: staking_pool_mappings_id,
                            size: staking_pool_mappings_size,
                        },
                    inactive_pools:
                        Table {
                            id: inactive_pools_id,
                            size: inactive_pools_size,
                        },
                    validator_candidates:
                        Table {
                            id: validator_candidates_id,
                            size: validator_candidates_size,
                        },
                },
            storage_fund,
            parameters:
                SystemParametersV1 {
                    min_validator_stake,
                    max_validator_count,
                    governance_start_epoch,
                },
            reference_gas_price,
            validator_report_records:
                VecMap {
                    contents: validator_report_records,
                },
            stake_subsidy:
                StakeSubsidyV1 {
                    epoch_counter: stake_subsidy_epoch_counter,
                    balance: stake_subsidy_balance,
                    current_epoch_amount: stake_subsidy_current_epoch_amount,
                },
            safe_mode,
            epoch_start_timestamp_ms,
        } = self;
        SuiSystemStateSummary {
            epoch,
            protocol_version,
            storage_fund: storage_fund.value(),
            reference_gas_price,
            safe_mode,
            epoch_start_timestamp_ms,
            min_validator_stake,
            max_validator_count,
            governance_start_epoch,
            stake_subsidy_epoch_counter,
            stake_subsidy_balance: stake_subsidy_balance.value(),
            stake_subsidy_current_epoch_amount,
            total_stake,
            active_validators: active_validators
                .into_iter()
                .map(|v| v.into_sui_validator_summary())
                .collect(),
            pending_active_validators_id,
            pending_active_validators_size,
            pending_removals,
            staking_pool_mappings_id,
            staking_pool_mappings_size,
            inactive_pools_id,
            inactive_pools_size,
            validator_candidates_id,
            validator_candidates_size,
            validator_report_records: validator_report_records
                .into_iter()
                .map(|e| (e.key, e.value.contents))
                .collect(),
        }
    }

    fn staking_pool_mappings(&self) -> &Table {
        &self.validators.staking_pool_mappings
    }

    fn get_staking_pool(&self, pool_id: &ObjectID) -> Option<&StakingPool> {
        // TODO: search deleted pool when it's available
        self.validators.active_validators.iter().find_map(|v| {
            if &v.staking_pool.id == pool_id {
                Some(&v.staking_pool)
            } else {
                None
            }
        })
    }
}

// The default implementation for tests
impl Default for SuiSystemStateInnerV1 {
    fn default() -> Self {
        let validator_set = ValidatorSetV1 {
            total_stake: 2,
            active_validators: vec![],
            pending_active_validators: TableVec::default(),
            pending_removals: vec![],
            staking_pool_mappings: Table::default(),
            inactive_pools: Table::default(),
            validator_candidates: Table::default(),
        };
        Self {
            epoch: 0,
            protocol_version: ProtocolVersion::MIN.as_u64(),
            validators: validator_set,
            storage_fund: Balance::new(0),
            parameters: SystemParametersV1 {
                min_validator_stake: 1,
                max_validator_count: 100,
                governance_start_epoch: 0,
            },
            reference_gas_price: 1,
            validator_report_records: VecMap { contents: vec![] },
            stake_subsidy: StakeSubsidyV1 {
                epoch_counter: 0,
                balance: Balance::new(0),
                current_epoch_amount: 0,
            },
            safe_mode: false,
            epoch_start_timestamp_ms: 0,
        }
    }
}
