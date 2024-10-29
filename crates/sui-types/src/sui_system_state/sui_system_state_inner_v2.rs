// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use super::epoch_start_sui_system_state::EpochStartValidatorInfoV1;
use super::sui_system_state_inner_v1::ValidatorV1;
use super::sui_system_state_summary::{SuiSystemStateSummary, SuiValidatorSummary};
use super::{AdvanceEpochParams, SuiSystemStateTrait};
use crate::balance::Balance;
use crate::base_types::SuiAddress;
use crate::collection_types::{Bag, Table, TableVec, VecMap, VecSet};
use crate::committee::{CommitteeWithNetworkMetadata, NetworkMetadata};
use crate::error::SuiError;
use crate::storage::ObjectStore;
use crate::sui_system_state::epoch_start_sui_system_state::EpochStartSystemState;
use crate::sui_system_state::get_validators_from_table_vec;
use crate::sui_system_state::sui_system_state_inner_v1::{
    StakeSubsidyV1, StorageFundV1, ValidatorSetV1,
};
use serde::{Deserialize, Serialize};

/// Rust version of the Move sui::sui_system::SystemParametersV2 type
#[derive(Debug, Serialize, Deserialize, Clone, Eq, PartialEq)]
pub struct SystemParametersV2 {
    /// The duration of an epoch, in milliseconds.
    pub epoch_duration_ms: u64,

    /// The starting epoch in which stake subsidies start being paid out
    pub stake_subsidy_start_epoch: u64,

    /// Minimum number of active validators at any moment.
    pub min_validator_count: u64,

    /// Maximum number of active validators at any moment.
    /// We do not allow the number of validators in any epoch to go above this.
    pub max_validator_count: u64,

    /// Lower-bound on the amount of stake required to become a validator.
    pub min_validator_joining_stake: u64,

    /// Validators with stake amount below `validator_low_stake_threshold` are considered to
    /// have low stake and will be escorted out of the validator set after being below this
    /// threshold for more than `validator_low_stake_grace_period` number of epochs.
    pub validator_low_stake_threshold: u64,

    /// Validators with stake below `validator_very_low_stake_threshold` will be removed
    /// immediately at epoch change, no grace period.
    pub validator_very_low_stake_threshold: u64,

    /// A validator can have stake below `validator_low_stake_threshold`
    /// for this many epochs before being kicked out.
    pub validator_low_stake_grace_period: u64,

    pub extra_fields: Bag,
}

/// Rust version of the Move sui_system::sui_system::SuiSystemStateInnerV2 type
#[derive(Debug, Serialize, Deserialize, Clone, Eq, PartialEq)]
pub struct SuiSystemStateInnerV2 {
    pub epoch: u64,
    pub protocol_version: u64,
    pub system_state_version: u64,
    pub validators: ValidatorSetV1,
    pub storage_fund: StorageFundV1,
    pub parameters: SystemParametersV2,
    pub reference_gas_price: u64,
    pub validator_report_records: VecMap<SuiAddress, VecSet<SuiAddress>>,
    pub stake_subsidy: StakeSubsidyV1,
    pub safe_mode: bool,
    pub safe_mode_storage_rewards: Balance,
    pub safe_mode_computation_rewards: Balance,
    pub safe_mode_storage_rebates: u64,
    pub safe_mode_non_refundable_storage_fee: u64,
    pub epoch_start_timestamp_ms: u64,
    pub extra_fields: Bag,
    // TODO: Use getters instead of all pub.
}

impl SuiSystemStateTrait for SuiSystemStateInnerV2 {
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
        self.safe_mode_storage_rewards
            .deposit_for_safe_mode(params.storage_charge);
        self.safe_mode_storage_rebates += params.storage_rebate;
        self.safe_mode_computation_rewards
            .deposit_for_safe_mode(params.computation_charge);
        self.safe_mode_non_refundable_storage_fee += params.non_refundable_storage_fee;
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
        object_store: &S,
    ) -> Result<Vec<SuiValidatorSummary>, SuiError> {
        let table_id = self.validators.pending_active_validators.contents.id;
        let table_size = self.validators.pending_active_validators.contents.size;
        let validators: Vec<ValidatorV1> =
            get_validators_from_table_vec(&object_store, table_id, table_size)?;
        Ok(validators
            .into_iter()
            .map(|v| v.into_sui_validator_summary())
            .collect())
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
                        hostname: metadata.name.clone(),
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
                    extra_fields: _,
                },
            storage_fund,
            parameters:
                SystemParametersV2 {
                    stake_subsidy_start_epoch,
                    epoch_duration_ms,
                    min_validator_count: _, // TODO: Add it to RPC layer in the future.
                    max_validator_count,
                    min_validator_joining_stake,
                    validator_low_stake_threshold,
                    validator_very_low_stake_threshold,
                    validator_low_stake_grace_period,
                    extra_fields: _,
                },
            reference_gas_price,
            validator_report_records:
                VecMap {
                    contents: validator_report_records,
                },
            stake_subsidy:
                StakeSubsidyV1 {
                    balance: stake_subsidy_balance,
                    distribution_counter: stake_subsidy_distribution_counter,
                    current_distribution_amount: stake_subsidy_current_distribution_amount,
                    stake_subsidy_period_length,
                    stake_subsidy_decrease_rate,
                    extra_fields: _,
                },
            safe_mode,
            safe_mode_storage_rewards,
            safe_mode_computation_rewards,
            safe_mode_storage_rebates,
            safe_mode_non_refundable_storage_fee,
            epoch_start_timestamp_ms,
            extra_fields: _,
        } = self;
        SuiSystemStateSummary {
            epoch,
            protocol_version,
            system_state_version,
            storage_fund_total_object_storage_rebates: storage_fund
                .total_object_storage_rebates
                .value(),
            storage_fund_non_refundable_balance: storage_fund.non_refundable_balance.value(),
            reference_gas_price,
            safe_mode,
            safe_mode_storage_rewards: safe_mode_storage_rewards.value(),
            safe_mode_computation_rewards: safe_mode_computation_rewards.value(),
            safe_mode_storage_rebates,
            safe_mode_non_refundable_storage_fee,
            epoch_start_timestamp_ms,
            stake_subsidy_start_epoch,
            epoch_duration_ms,
            stake_subsidy_distribution_counter,
            stake_subsidy_balance: stake_subsidy_balance.value(),
            stake_subsidy_current_distribution_amount,
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
            max_validator_count,
            min_validator_joining_stake,
            validator_low_stake_threshold,
            validator_very_low_stake_threshold,
            validator_low_stake_grace_period,
            stake_subsidy_period_length,
            stake_subsidy_decrease_rate,
        }
    }
}
