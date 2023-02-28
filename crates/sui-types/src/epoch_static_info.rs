// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::sui_system_state::{SuiSystemState, ValidatorMetadata};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Default, Serialize, Deserialize, Clone, Eq, PartialEq, JsonSchema)]
pub struct EpochStaticInfo {
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

    pub validators: Vec<EpochValidator>,
}

#[derive(Debug, Serialize, Deserialize, Clone, Eq, PartialEq, JsonSchema)]
pub struct EpochValidator {
    pub metadata: ValidatorMetadata,

    pub voting_power: u64,
    pub stake_amount: u64,

    pub gas_price: u64,
    pub commission_rate: u64,
}

impl From<SuiSystemState> for EpochStaticInfo {
    fn from(state: SuiSystemState) -> Self {
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
                .map(|v| EpochValidator {
                    metadata: v.metadata.clone(),
                    voting_power: v.voting_power,
                    stake_amount: v.stake_amount,
                    gas_price: v.gas_price,
                    commission_rate: v.commission_rate,
                })
                .collect(),
        }
    }
}
