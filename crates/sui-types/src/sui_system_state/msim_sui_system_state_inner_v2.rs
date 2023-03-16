// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::balance::Balance;
use crate::base_types::SuiAddress;
use crate::collection_types::{Table, TableVec, VecMap, VecSet};
use crate::committee::{Committee, CommitteeWithNetworkMetadata, NetworkMetadata};
use crate::sui_system_state::epoch_start_sui_system_state::{
    EpochStartSystemState, EpochStartValidatorInfoV1,
};
use crate::sui_system_state::sui_system_state_inner_v1::{
    StakeSubsidyV1, SystemParametersV1, ValidatorSetV1,
};
use crate::sui_system_state::sui_system_state_summary::SuiSystemStateSummary;
use crate::sui_system_state::SuiSystemStateTrait;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Serialize, Deserialize, Clone, Eq, PartialEq)]
pub struct SimTestSuiSystemStateInnerV2 {
    pub new_dummy_field: u64,
    pub epoch: u64,
    pub protocol_version: u64,
    pub system_state_version: u64,
    pub validators: ValidatorSetV1,
    pub storage_fund: Balance,
    pub parameters: SystemParametersV1,
    pub reference_gas_price: u64,
    pub validator_report_records: VecMap<SuiAddress, VecSet<SuiAddress>>,
    pub stake_subsidy: StakeSubsidyV1,
    pub safe_mode: bool,
    pub epoch_start_timestamp_ms: u64,
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
                    inactive_validators:
                        Table {
                            id: inactive_pools_id,
                            size: inactive_pools_size,
                        },
                    validator_candidates:
                        Table {
                            id: validator_candidates_id,
                            size: validator_candidates_size,
                        },
                    at_risk_validators:
                        VecMap {
                            contents: at_risk_validators,
                        },
                },
            storage_fund,
            parameters:
                SystemParametersV1 {
                    governance_start_epoch,
                    epoch_duration_ms,
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
            system_state_version,
            storage_fund: storage_fund.value(),
            reference_gas_price,
            safe_mode,
            epoch_start_timestamp_ms,
            governance_start_epoch,
            epoch_duration_ms,
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
            at_risk_validators: at_risk_validators
                .into_iter()
                .map(|e| (e.key, e.value))
                .collect(),
            validator_report_records: validator_report_records
                .into_iter()
                .map(|e| (e.key, e.value.contents))
                .collect(),
        }
    }
}
