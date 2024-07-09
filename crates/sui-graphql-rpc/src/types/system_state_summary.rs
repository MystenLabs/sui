// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use super::{
    big_int::BigInt, gas::GasCostSummary, safe_mode::SafeMode, stake_subsidy::StakeSubsidy,
    storage_fund::StorageFund, system_parameters::SystemParameters, uint53::UInt53,
};
use async_graphql::*;
use sui_types::sui_system_state::sui_system_state_summary::SuiSystemStateSummary as NativeSystemStateSummary;

#[derive(Clone, Debug)]
pub(crate) struct SystemStateSummary {
    pub native: NativeSystemStateSummary,
}

/// Aspects that affect the running of the system that are managed by the validators either
/// directly, or through system transactions.
#[Object]
impl SystemStateSummary {
    /// SUI set aside to account for objects stored on-chain, at the start of the epoch.
    /// This is also used for storage rebates.
    async fn storage_fund(&self) -> Option<StorageFund> {
        Some(StorageFund {
            total_object_storage_rebates: Some(BigInt::from(
                self.native.storage_fund_total_object_storage_rebates,
            )),
            non_refundable_balance: Some(BigInt::from(
                self.native.storage_fund_non_refundable_balance,
            )),
        })
    }

    /// Information about whether this epoch was started in safe mode, which happens if the full epoch
    /// change logic fails for some reason.
    async fn safe_mode(&self) -> Option<SafeMode> {
        Some(SafeMode {
            enabled: Some(self.native.safe_mode),
            gas_summary: Some(GasCostSummary {
                computation_cost: self.native.safe_mode_computation_rewards,
                storage_cost: self.native.safe_mode_storage_rewards,
                storage_rebate: self.native.safe_mode_storage_rebates,
                non_refundable_storage_fee: self.native.safe_mode_non_refundable_storage_fee,
            }),
        })
    }

    /// The value of the `version` field of `0x5`, the `0x3::sui::SuiSystemState` object.  This
    /// version changes whenever the fields contained in the system state object (held in a dynamic
    /// field attached to `0x5`) change.
    async fn system_state_version(&self) -> Option<UInt53> {
        Some(self.native.system_state_version.into())
    }

    /// Details of the system that are decided during genesis.
    async fn system_parameters(&self) -> Option<SystemParameters> {
        Some(SystemParameters {
            duration_ms: Some(BigInt::from(self.native.epoch_duration_ms)),
            stake_subsidy_start_epoch: Some(self.native.stake_subsidy_start_epoch.into()),
            // TODO min validator count can be extracted, but it requires some JSON RPC changes,
            // so we decided to wait on it for now.
            min_validator_count: None,
            max_validator_count: Some(self.native.max_validator_count),
            min_validator_joining_stake: Some(BigInt::from(
                self.native.min_validator_joining_stake,
            )),
            validator_low_stake_threshold: Some(BigInt::from(
                self.native.validator_low_stake_threshold,
            )),
            validator_very_low_stake_threshold: Some(BigInt::from(
                self.native.validator_very_low_stake_threshold,
            )),
            validator_low_stake_grace_period: Some(BigInt::from(
                self.native.validator_low_stake_grace_period,
            )),
        })
    }

    /// Parameters related to the subsidy that supplements staking rewards
    async fn system_stake_subsidy(&self) -> Option<StakeSubsidy> {
        Some(StakeSubsidy {
            balance: Some(BigInt::from(self.native.stake_subsidy_balance)),
            distribution_counter: Some(self.native.stake_subsidy_distribution_counter),
            current_distribution_amount: Some(BigInt::from(
                self.native.stake_subsidy_current_distribution_amount,
            )),
            period_length: Some(self.native.stake_subsidy_period_length),
            decrease_rate: Some(self.native.stake_subsidy_decrease_rate as u64),
        })
    }
}
