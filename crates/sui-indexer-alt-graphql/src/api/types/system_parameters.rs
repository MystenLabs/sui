// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::{api::scalars::big_int::BigInt, api::scalars::uint53::UInt53};
use async_graphql::SimpleObject;
use sui_types::sui_system_state::sui_system_state_inner_v1::SystemParametersV1;
use sui_types::sui_system_state::sui_system_state_inner_v2::SystemParametersV2;

/// Details of the system that are decided during genesis.
#[derive(Clone, Debug, PartialEq, Eq, SimpleObject)]
pub(crate) struct SystemParameters {
    /// Target duration of an epoch, in milliseconds.
    pub duration_ms: Option<BigInt>,

    /// The epoch at which stake subsidies start being paid out.
    pub stake_subsidy_start_epoch: Option<UInt53>,

    /// The minimum number of active validators that the system supports.
    pub min_validator_count: Option<u64>,

    /// The maximum number of active validators that the system supports.
    pub max_validator_count: Option<u64>,

    /// Minimum stake needed to become a new validator.
    pub min_validator_joining_stake: Option<BigInt>,

    /// Validators with stake below this threshold will enter the grace period (see `validatorLowStakeGracePeriod`), after which they are removed from the active validator set.
    pub validator_low_stake_threshold: Option<BigInt>,

    /// Validators with stake below this threshold will be removed from the active validator set at the next epoch boundary, without a grace period.
    pub validator_very_low_stake_threshold: Option<BigInt>,

    /// The number of epochs that a validator has to recover from having less than `validatorLowStakeThreshold` stake.
    pub validator_low_stake_grace_period: Option<BigInt>,
}

pub(crate) fn from_system_parameters_v1(value: SystemParametersV1) -> SystemParameters {
    SystemParameters {
        duration_ms: Some(value.epoch_duration_ms.into()),
        stake_subsidy_start_epoch: Some(value.stake_subsidy_start_epoch.into()),
        // TODO min validator count can be extracted, but it requires some JSON RPC changes,
        // so we decided to wait on it for now.
        min_validator_count: None,
        max_validator_count: Some(value.max_validator_count),
        min_validator_joining_stake: Some(value.min_validator_joining_stake.into()),
        validator_low_stake_threshold: Some(value.validator_low_stake_threshold.into()),
        validator_very_low_stake_threshold: Some(value.validator_very_low_stake_threshold.into()),
        validator_low_stake_grace_period: Some(value.validator_low_stake_grace_period.into()),
    }
}

pub(crate) fn from_system_parameters_v2(value: SystemParametersV2) -> SystemParameters {
    SystemParameters {
        duration_ms: Some(value.epoch_duration_ms.into()),
        stake_subsidy_start_epoch: Some(value.stake_subsidy_start_epoch.into()),
        // TODO min validator count can be extracted, but it requires some JSON RPC changes,
        // so we decided to wait on it for now.
        min_validator_count: None,
        max_validator_count: Some(value.max_validator_count),
        min_validator_joining_stake: Some(value.min_validator_joining_stake.into()),
        validator_low_stake_threshold: Some(value.validator_low_stake_threshold.into()),
        validator_very_low_stake_threshold: Some(value.validator_very_low_stake_threshold.into()),
        validator_low_stake_grace_period: Some(value.validator_low_stake_grace_period.into()),
    }
}
