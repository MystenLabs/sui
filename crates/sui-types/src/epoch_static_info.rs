// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::{
    base_types::SuiAddress,
    crypto::AuthorityPublicKeyBytes,
    sui_system_state::{SuiSystemState, ValidatorMetadata},
};
use fastcrypto::traits::ToFromBytes;
use multiaddr::Multiaddr;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Default, Serialize, Deserialize, Clone, Eq, PartialEq, JsonSchema)]
pub struct EpochStaticInfo<V = EpochValidator> {
    pub epoch: u64,
    pub protocol_version: u64,
    pub safe_mode: bool,

    pub reference_gas_price: u64,
    pub epoch_start_timestamp_ms: u64,

    pub storage_fund_balance: u64,

    pub stake_subsidy_epoch_counter: u64,
    pub stake_subsidy_balance: u64,
    pub stake_subsidy_current_epoch_amount: u64,

    pub total_validator_self_stake: u64,
    pub total_delegation_stake: u64,

    pub validators: Vec<V>,
}

#[derive(Debug, Serialize, Deserialize, Clone, Eq, PartialEq)]
pub struct EpochValidator {
    pub sui_address: SuiAddress,
    pub pubkey_bytes: AuthorityPublicKeyBytes,
    pub network_pubkey_bytes: narwhal_crypto::NetworkPublicKey,
    pub worker_pubkey_bytes: narwhal_crypto::NetworkPublicKey,
    pub proof_of_possession_bytes: Vec<u8>,
    pub name: String,
    pub description: String,
    pub image_url: String,
    pub project_url: String,
    pub net_address: Multiaddr,
    pub p2p_address: Multiaddr,
    pub consensus_address: Multiaddr,
    pub worker_address: Multiaddr,

    pub voting_power: u64,
    pub stake_amount: u64,

    pub gas_price: u64,
    pub commission_rate: u64,
}

impl EpochValidator {
    pub fn new(
        metadata: &ValidatorMetadata,
        voting_power: u64,
        stake_amount: u64,
        gas_price: u64,
        commission_rate: u64,
    ) -> Self {
        // TODO: All the unwraps will be replaced once we have proper metadata validation.
        Self {
            sui_address: metadata.sui_address,
            pubkey_bytes: AuthorityPublicKeyBytes::from_bytes(metadata.pubkey_bytes.as_ref())
                .unwrap(),
            network_pubkey_bytes: narwhal_crypto::NetworkPublicKey::from_bytes(
                &metadata.network_pubkey_bytes,
            )
            .unwrap(),
            worker_pubkey_bytes: narwhal_crypto::NetworkPublicKey::from_bytes(
                &metadata.worker_pubkey_bytes,
            )
            .unwrap(),
            proof_of_possession_bytes: metadata.proof_of_possession_bytes.clone(),
            name: metadata.name.clone(),
            description: metadata.description.clone(),
            image_url: metadata.image_url.clone(),
            project_url: metadata.project_url.clone(),
            net_address: Multiaddr::try_from(metadata.net_address.clone()).unwrap(),
            p2p_address: Multiaddr::try_from(metadata.p2p_address.clone()).unwrap(),
            consensus_address: Multiaddr::try_from(metadata.consensus_address.clone()).unwrap(),
            worker_address: Multiaddr::try_from(metadata.worker_address.clone()).unwrap(),
            voting_power,
            stake_amount,
            gas_price,
            commission_rate,
        }
    }
}

/// Corresponding type to EpochValidator that's json-serializable for RPC responses.
#[derive(Debug, Default, Serialize, Deserialize, Clone, Eq, PartialEq, JsonSchema)]
pub struct SuiEpochValidator {
    pub sui_address: SuiAddress,
    pub pubkey_bytes: Vec<u8>,
    pub network_pubkey_bytes: Vec<u8>,
    pub worker_pubkey_bytes: Vec<u8>,
    pub proof_of_possession_bytes: Vec<u8>,
    pub name: String,
    pub description: String,
    pub image_url: String,
    pub project_url: String,
    pub net_address: Vec<u8>,
    pub p2p_address: Vec<u8>,
    pub consensus_address: Vec<u8>,
    pub worker_address: Vec<u8>,

    pub voting_power: u64,
    pub stake_amount: u64,

    pub gas_price: u64,
    pub commission_rate: u64,
}

impl From<EpochValidator> for SuiEpochValidator {
    fn from(v: EpochValidator) -> Self {
        Self {
            sui_address: v.sui_address,
            pubkey_bytes: v.pubkey_bytes.as_bytes().to_vec(),
            network_pubkey_bytes: v.network_pubkey_bytes.as_bytes().to_vec(),
            worker_pubkey_bytes: v.worker_pubkey_bytes.as_bytes().to_vec(),
            proof_of_possession_bytes: v.proof_of_possession_bytes,
            name: v.name,
            description: v.description,
            image_url: v.image_url,
            project_url: v.project_url,
            net_address: v.net_address.to_vec(),
            p2p_address: v.p2p_address.to_vec(),
            consensus_address: v.consensus_address.to_vec(),
            worker_address: v.worker_address.to_vec(),
            voting_power: v.voting_power,
            stake_amount: v.stake_amount,
            gas_price: v.gas_price,
            commission_rate: v.commission_rate,
        }
    }
}

impl From<SuiEpochValidator> for EpochValidator {
    fn from(e: SuiEpochValidator) -> Self {
        Self::new(
            &ValidatorMetadata {
                sui_address: e.sui_address,
                pubkey_bytes: e.pubkey_bytes,
                network_pubkey_bytes: e.network_pubkey_bytes,
                worker_pubkey_bytes: e.worker_pubkey_bytes,
                proof_of_possession_bytes: e.proof_of_possession_bytes,
                name: e.name,
                description: e.description,
                image_url: e.image_url,
                project_url: e.project_url,
                net_address: e.net_address,
                p2p_address: e.p2p_address,
                consensus_address: e.consensus_address,
                worker_address: e.worker_address,
            },
            e.voting_power,
            e.stake_amount,
            e.gas_price,
            e.commission_rate,
        )
    }
}

/// Corresponding type to EpochStaticInfo that's json-serializable for RPC responses.
pub type SuiEpochStaticInfo = EpochStaticInfo<SuiEpochValidator>;

impl From<EpochStaticInfo> for SuiEpochStaticInfo {
    fn from(e: EpochStaticInfo) -> Self {
        Self {
            epoch: e.epoch,
            protocol_version: e.protocol_version,
            safe_mode: e.safe_mode,
            reference_gas_price: e.reference_gas_price,
            epoch_start_timestamp_ms: e.epoch_start_timestamp_ms,
            storage_fund_balance: e.storage_fund_balance,
            stake_subsidy_epoch_counter: e.stake_subsidy_epoch_counter,
            stake_subsidy_balance: e.stake_subsidy_balance,
            stake_subsidy_current_epoch_amount: e.stake_subsidy_current_epoch_amount,
            total_validator_self_stake: e.total_validator_self_stake,
            total_delegation_stake: e.total_delegation_stake,
            validators: e.validators.into_iter().map(|v| v.into()).collect(),
        }
    }
}

impl From<SuiEpochStaticInfo> for EpochStaticInfo {
    fn from(e: SuiEpochStaticInfo) -> Self {
        Self {
            epoch: e.epoch,
            protocol_version: e.protocol_version,
            safe_mode: e.safe_mode,
            reference_gas_price: e.reference_gas_price,
            epoch_start_timestamp_ms: e.epoch_start_timestamp_ms,
            storage_fund_balance: e.storage_fund_balance,
            stake_subsidy_epoch_counter: e.stake_subsidy_epoch_counter,
            stake_subsidy_balance: e.stake_subsidy_balance,
            stake_subsidy_current_epoch_amount: e.stake_subsidy_current_epoch_amount,
            total_validator_self_stake: e.total_validator_self_stake,
            total_delegation_stake: e.total_delegation_stake,
            validators: e.validators.into_iter().map(|v| v.into()).collect(),
        }
    }
}

impl From<&SuiSystemState> for EpochStaticInfo {
    fn from(state: &SuiSystemState) -> Self {
        Self {
            epoch: state.epoch,
            protocol_version: state.protocol_version,
            safe_mode: state.safe_mode,
            reference_gas_price: state.reference_gas_price,
            epoch_start_timestamp_ms: state.epoch_start_timestamp_ms,
            storage_fund_balance: state.storage_fund.value(),
            stake_subsidy_epoch_counter: state.stake_subsidy.epoch_counter,
            stake_subsidy_balance: state.stake_subsidy.balance.value(),
            stake_subsidy_current_epoch_amount: state.stake_subsidy.current_epoch_amount,
            total_validator_self_stake: state.validators.validator_stake,
            total_delegation_stake: state.validators.delegation_stake,
            validators: state
                .validators
                .active_validators
                .iter()
                .map(|v| {
                    EpochValidator::new(
                        &v.metadata,
                        v.voting_power,
                        v.stake_amount,
                        v.gas_price,
                        v.commission_rate,
                    )
                })
                .collect(),
        }
    }
}
