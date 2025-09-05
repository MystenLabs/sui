// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::balance::Balance;
use crate::base_types::{MoveObjectType, TransactionDigest};
use crate::collection_types::VecMap;
use crate::dynamic_field::{derive_dynamic_field_id, serialize_dynamic_field};
use crate::id::UID;
use crate::object::{MoveObject, Object, Owner};
use crate::sui_system_state::sui_system_state_inner_v1::{
    StakeSubsidyV1, StorageFundV1, SuiSystemStateInnerV1, SystemParametersV1, ValidatorSetV1,
};
use crate::sui_system_state::sui_system_state_inner_v2::{
    SuiSystemStateInnerV2, SystemParametersV2,
};
use crate::sui_system_state::{SuiSystemState, SuiSystemStateWrapper};
use crate::{MoveTypeTagTrait, SUI_SYSTEM_STATE_OBJECT_ID};
use sui_protocol_config::ProtocolConfig;

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

pub fn system_parameters_v2() -> SystemParametersV2 {
    SystemParametersV2 {
        epoch_duration_ms: 0,
        stake_subsidy_start_epoch: 0,
        min_validator_count: 0,
        max_validator_count: 0,
        min_validator_joining_stake: 0,
        validator_low_stake_threshold: 0,
        validator_very_low_stake_threshold: 0,
        validator_low_stake_grace_period: 0,
        extra_fields: Default::default(),
    }
}

pub fn sui_system_state_inner_v2() -> SuiSystemStateInnerV2 {
    SuiSystemStateInnerV2 {
        epoch: 0,
        protocol_version: 0,
        // must be 2
        system_state_version: 2,
        validators: validator_set_v1(),
        storage_fund: storage_fund_v1(),
        parameters: system_parameters_v2(),
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

pub fn system_state_output_objects(sui_system_state: SuiSystemState) -> Vec<Object> {
    let version = sui_system_state.version();
    let system_state_wrapper_object = Object::new_move(
        unsafe {
            MoveObject::new_from_execution(
                MoveObjectType::gas_coin(),
                // must be true to pass validation
                true,
                0.into(),
                bcs::to_bytes(&SuiSystemStateWrapper {
                    // must be SUI_SYSTEM_STATE_OBJECT_ID
                    id: UID::new(SUI_SYSTEM_STATE_OBJECT_ID),
                    version,
                })
                .unwrap(),
                &ProtocolConfig::get_for_max_version_UNSAFE(),
                false,
            )
            .unwrap()
        },
        Owner::Immutable,
        TransactionDigest::genesis_marker(),
    );
    let dynamic_field_key = 0u64; // value does not matter

    // hash based on system_state_wrapper_object
    let system_state_object_id = derive_dynamic_field_id(
        SUI_SYSTEM_STATE_OBJECT_ID,
        &dynamic_field_key.get_instance_type_tag(),
        &bcs::to_bytes(&version).unwrap(),
    )
    .unwrap();

    let field_id = UID::new(system_state_object_id);
    let object_bytes = match sui_system_state {
        SuiSystemState::V1(inner) => serialize_dynamic_field(&field_id, &dynamic_field_key, inner),
        SuiSystemState::V2(inner) => serialize_dynamic_field(&field_id, &dynamic_field_key, inner),
        #[cfg(msim)]
        SuiSystemState::SimTestV1(_)
        | SuiSystemState::SimTestShallowV2(_)
        | SuiSystemState::SimTestDeepV2(_) => unimplemented!(),
    }
    .unwrap();
    let system_state_inner_object = Object::new_move(
        unsafe {
            MoveObject::new_from_execution(
                MoveObjectType::gas_coin(),
                true,
                0.into(),
                object_bytes,
                &ProtocolConfig::get_for_max_version_UNSAFE(),
                false,
            )
            .unwrap()
        },
        Owner::Immutable,
        TransactionDigest::genesis_marker(),
    );
    vec![system_state_wrapper_object, system_state_inner_object]
}
