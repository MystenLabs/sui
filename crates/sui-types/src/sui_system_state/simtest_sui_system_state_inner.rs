// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::balance::Balance;
use crate::base_types::SuiAddress;
use crate::collection_types::{Bag, Table};
use crate::committee::{CommitteeWithNetworkMetadata, NetworkMetadata};
use crate::crypto::{AuthorityPublicKey, AuthorityPublicKeyBytes, NetworkPublicKey};
use crate::error::SuiError;
use crate::storage::ObjectStore;
use crate::sui_system_state::epoch_start_sui_system_state::{
    EpochStartSystemState, EpochStartValidatorInfoV1,
};
use crate::sui_system_state::sui_system_state_summary::{
    SuiSystemStateSummary, SuiValidatorSummary,
};
use crate::sui_system_state::{AdvanceEpochParams, SuiSystemStateTrait};
use fastcrypto::traits::ToFromBytes;
use mysten_network::Multiaddr;
use once_cell::sync::OnceCell;
use serde::{Deserialize, Serialize};

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
        SuiValidatorSummary::default()
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
    pub protocol_pubkey: AuthorityPublicKey,
    pub network_pubkey: NetworkPublicKey,
    pub worker_pubkey: NetworkPublicKey,
    pub net_address: Multiaddr,
    pub p2p_address: Multiaddr,
    pub primary_address: Multiaddr,
    pub worker_address: Multiaddr,
}

impl SimTestValidatorMetadataV1 {
    pub fn verify(&self) -> VerifiedSimTestValidatorMetadataV1 {
        let protocol_pubkey =
            AuthorityPublicKey::from_bytes(self.protocol_pubkey_bytes.as_ref()).unwrap();
        let network_pubkey =
            NetworkPublicKey::from_bytes(self.network_pubkey_bytes.as_ref()).unwrap();
        let worker_pubkey =
            NetworkPublicKey::from_bytes(self.worker_pubkey_bytes.as_ref()).unwrap();
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

    fn advance_epoch_safe_mode(&mut self, params: &AdvanceEpochParams) {
        self.epoch = params.epoch;
        self.safe_mode = true;
        self.epoch_start_timestamp_ms = params.epoch_start_timestamp_ms;
        self.protocol_version = params.next_protocol_version.as_u64();
    }

    fn get_current_epoch_committee(&self) -> CommitteeWithNetworkMetadata {
        let validators = self
            .validators
            .active_validators
            .iter()
            .map(|validator| {
                let verified_metadata = validator.verified_metadata();
                let name = verified_metadata.sui_pubkey_bytes();
                (
                    name,
                    (
                        validator.voting_power,
                        NetworkMetadata {
                            network_address: verified_metadata.net_address.clone(),
                            narwhal_primary_address: verified_metadata.primary_address.clone(),
                            network_public_key: Some(verified_metadata.network_pubkey.clone()),
                        },
                    ),
                )
            })
            .collect();
        CommitteeWithNetworkMetadata::new(self.epoch, validators)
    }

    fn get_pending_active_validators<S: ObjectStore + ?Sized>(
        &self,
        _object_store: &S,
    ) -> Result<Vec<SuiValidatorSummary>, SuiError> {
        Ok(vec![])
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
                        hostname: "".to_string(),
                    }
                })
                .collect(),
        )
    }

    fn into_sui_system_state_summary(self) -> SuiSystemStateSummary {
        SuiSystemStateSummary::default()
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, Eq, PartialEq)]
pub struct SimTestSuiSystemStateInnerShallowV2 {
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

impl SuiSystemStateTrait for SimTestSuiSystemStateInnerShallowV2 {
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

    fn advance_epoch_safe_mode(&mut self, params: &AdvanceEpochParams) {
        self.epoch = params.epoch;
        self.safe_mode = true;
        self.epoch_start_timestamp_ms = params.epoch_start_timestamp_ms;
        self.protocol_version = params.next_protocol_version.as_u64();
    }

    fn get_current_epoch_committee(&self) -> CommitteeWithNetworkMetadata {
        let validators = self
            .validators
            .active_validators
            .iter()
            .map(|validator| {
                let verified_metadata = validator.verified_metadata();
                let name = verified_metadata.sui_pubkey_bytes();
                (
                    name,
                    (
                        validator.voting_power,
                        NetworkMetadata {
                            network_address: verified_metadata.net_address.clone(),
                            narwhal_primary_address: verified_metadata.primary_address.clone(),
                            network_public_key: Some(verified_metadata.network_pubkey.clone()),
                        },
                    ),
                )
            })
            .collect();
        CommitteeWithNetworkMetadata::new(self.epoch, validators)
    }

    fn get_pending_active_validators<S: ObjectStore + ?Sized>(
        &self,
        _object_store: &S,
    ) -> Result<Vec<SuiValidatorSummary>, SuiError> {
        Ok(vec![])
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
                        hostname: "".to_string(),
                    }
                })
                .collect(),
        )
    }

    fn into_sui_system_state_summary(self) -> SuiSystemStateSummary {
        SuiSystemStateSummary::default()
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, Eq, PartialEq)]
pub struct SimTestValidatorSetDeepV2 {
    pub active_validators: Vec<SimTestValidatorDeepV2>,
    pub inactive_validators: Table,
    pub extra_fields: Bag,
}

#[derive(Debug, Serialize, Deserialize, Clone, Eq, PartialEq)]
pub struct SimTestValidatorDeepV2 {
    pub new_dummy_field: u64,
    metadata: SimTestValidatorMetadataV1,
    #[serde(skip)]
    verified_metadata: OnceCell<VerifiedSimTestValidatorMetadataV1>,
    pub voting_power: u64,
    pub stake: Balance,
    pub extra_fields: Bag,
}

impl SimTestValidatorDeepV2 {
    pub fn verified_metadata(&self) -> &VerifiedSimTestValidatorMetadataV1 {
        self.verified_metadata
            .get_or_init(|| self.metadata.verify())
    }

    pub fn into_sui_validator_summary(self) -> SuiValidatorSummary {
        SuiValidatorSummary::default()
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, Eq, PartialEq)]
pub struct SimTestSuiSystemStateInnerDeepV2 {
    pub new_dummy_field: u64,
    pub epoch: u64,
    pub protocol_version: u64,
    pub system_state_version: u64,
    pub validators: SimTestValidatorSetDeepV2,
    pub storage_fund: Balance,
    pub parameters: SimTestSystemParametersV1,
    pub reference_gas_price: u64,
    pub safe_mode: bool,
    pub epoch_start_timestamp_ms: u64,
    pub extra_fields: Bag,
}

impl SuiSystemStateTrait for SimTestSuiSystemStateInnerDeepV2 {
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

    fn advance_epoch_safe_mode(&mut self, params: &AdvanceEpochParams) {
        self.epoch = params.epoch;
        self.safe_mode = true;
        self.epoch_start_timestamp_ms = params.epoch_start_timestamp_ms;
        self.protocol_version = params.next_protocol_version.as_u64();
    }

    fn get_current_epoch_committee(&self) -> CommitteeWithNetworkMetadata {
        let validators = self
            .validators
            .active_validators
            .iter()
            .map(|validator| {
                let verified_metadata = validator.verified_metadata();
                let name = verified_metadata.sui_pubkey_bytes();
                (
                    name,
                    (
                        validator.voting_power,
                        NetworkMetadata {
                            network_address: verified_metadata.net_address.clone(),
                            narwhal_primary_address: verified_metadata.primary_address.clone(),
                            network_public_key: Some(verified_metadata.network_pubkey.clone()),
                        },
                    ),
                )
            })
            .collect();
        CommitteeWithNetworkMetadata::new(self.epoch, validators)
    }

    fn get_pending_active_validators<S: ObjectStore + ?Sized>(
        &self,
        _object_store: &S,
    ) -> Result<Vec<SuiValidatorSummary>, SuiError> {
        Ok(vec![])
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
                        hostname: "".to_string(),
                    }
                })
                .collect(),
        )
    }

    fn into_sui_system_state_summary(self) -> SuiSystemStateSummary {
        SuiSystemStateSummary::default()
    }
}
