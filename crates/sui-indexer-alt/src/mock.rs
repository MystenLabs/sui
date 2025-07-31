// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use sui_indexer_alt_schema::checkpoints::StoredGenesis;
use sui_types::balance::Balance;
use sui_types::collection_types::VecMap;
use sui_types::sui_system_state::sui_system_state_inner_v1::{
    StakeSubsidyV1, StorageFundV1, SuiSystemStateInnerV1, SystemParametersV1, ValidatorSetV1,
};

pub fn stored_genesis() -> StoredGenesis {
    StoredGenesis {
        genesis_digest: [1u8; 32].to_vec(),
        initial_protocol_version: 0,
    }
}

pub fn validator_set_v1() -> ValidatorSetV1 {
    ValidatorSetV1 {
        total_stake: 0,
        active_validators: vec![],
        pending_active_validators: Default::default(),
        pending_removals: vec![],
        staking_pool_mappings: Default::default(),
        inactive_validators: Default::default(),
        validator_candidates: Default::default(),
        at_risk_validators: VecMap { contents: vec![] },
        extra_fields: Default::default(),
    }
}

pub fn storage_fund_v1() -> StorageFundV1 {
    StorageFundV1 {
        total_object_storage_rebates: Balance::new(0),
        non_refundable_balance: Balance::new(0),
    }
}

pub fn system_parameters_v1() -> SystemParametersV1 {
    SystemParametersV1 {
        epoch_duration_ms: 0,
        stake_subsidy_start_epoch: 0,
        max_validator_count: 0,
        min_validator_joining_stake: 0,
        validator_low_stake_threshold: 0,
        validator_very_low_stake_threshold: 0,
        validator_low_stake_grace_period: 0,
        extra_fields: Default::default(),
    }
}

pub fn stake_subsidy_v1() -> StakeSubsidyV1 {
    StakeSubsidyV1 {
        balance: Balance::new(0),
        distribution_counter: 0,
        current_distribution_amount: 0,
        stake_subsidy_period_length: 0,
        stake_subsidy_decrease_rate: 0,
        extra_fields: Default::default(),
    }
}

pub fn sui_system_state_inner_v1() -> SuiSystemStateInnerV1 {
    SuiSystemStateInnerV1 {
        epoch: 0,
        protocol_version: 0,
        // must be 1
        system_state_version: 1,
        validators: validator_set_v1(),
        storage_fund: storage_fund_v1(),
        parameters: system_parameters_v1(),
        reference_gas_price: 0,
        validator_report_records: VecMap { contents: vec![] },
        stake_subsidy: stake_subsidy_v1(),
        safe_mode: false,
        safe_mode_storage_rewards: Balance::new(0),
        safe_mode_computation_rewards: Balance::new(0),
        safe_mode_storage_rebates: 0,
        safe_mode_non_refundable_storage_fee: 0,
        epoch_start_timestamp_ms: 0,
        extra_fields: Default::default(),
    }
}
