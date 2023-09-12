// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::context_data::context_ext::DataProviderContextExt;

use super::big_int::BigInt;
use super::date_time::DateTime;
use super::gas::GasCostSummary;
use super::protocol_config::ProtocolConfigs;
use super::safe_mode::SafeMode;
use super::stake_subsidy::StakeSubsidy;
use super::storage_fund::StorageFund;
use super::system_parameters::SystemParameters;
use super::validator::Validator;
use super::validator_set::ValidatorSet;
use crate::error::Error;
use async_graphql::*;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Epoch {
    pub epoch_id: u64,
    pub gas_cost_summary: Option<GasCostSummary>,
}

#[derive(Clone, Debug, PartialEq, Eq, SimpleObject)]
pub(crate) struct SystemStateSummary {
    system_state_version: Option<BigInt>,
    reference_gas_price: Option<BigInt>,
    system_parameters: Option<SystemParameters>,
    stake_subsidy: Option<StakeSubsidy>,
    validator_set: Option<ValidatorSet>,
    storage_fund: Option<StorageFund>,
    safe_mode: Option<SafeMode>,
    start_timestamp: Option<DateTime>,
    // pub end_timestamp: Option<DateTime>, //TODO decide if we want this data exposed or not
}

#[Object]
impl Epoch {
    async fn epoch_id(&self) -> u64 {
        self.epoch_id
    }

    async fn system_state_summary(&self, ctx: &Context<'_>) -> Result<Option<SystemStateSummary>> {
        let system_state = ctx.data_provider().get_latest_sui_system_state().await?;
        let active_validators = system_state
            .active_validators
            .clone()
            .into_iter()
            .map(Validator::from)
            .collect();
        let start_timestamp = i64::try_from(system_state.epoch_start_timestamp_ms).map_err(|_| {
            Error::Internal(format!(
                "Cannot convert start timestamp u64 ({}) of epoch ({}) into i64 required by DateTime",
                system_state.epoch_start_timestamp_ms, self.epoch_id
            ))
        })?;

        let start_timestamp = DateTime::from_ms(start_timestamp).ok_or_else(|| {
            Error::Internal(format!(
                "Cannot convert start timestamp ({}) of epoch ({}) into a DateTime",
                start_timestamp, self.epoch_id
            ))
        })?;
        Ok(Some(SystemStateSummary {
            system_state_version: Some(BigInt::from(system_state.system_state_version)),
            reference_gas_price: Some(BigInt::from(system_state.reference_gas_price)),
            system_parameters: Some(SystemParameters {
                duration_ms: Some(BigInt::from(system_state.epoch_duration_ms)),
                stake_subsidy_start_epoch: Some(system_state.stake_subsidy_start_epoch),
                min_validator_count: Some(system_state.max_validator_count),
                max_validator_count: Some(system_state.max_validator_count),
                min_validator_joining_stake: Some(BigInt::from(
                    system_state.min_validator_joining_stake,
                )),
                validator_low_stake_threshold: Some(BigInt::from(
                    system_state.validator_low_stake_threshold,
                )),
                validator_very_low_stake_threshold: Some(BigInt::from(
                    system_state.validator_very_low_stake_threshold,
                )),
                validator_low_stake_grace_period: Some(BigInt::from(
                    system_state.validator_low_stake_grace_period,
                )),
            }),
            stake_subsidy: Some(StakeSubsidy {
                balance: Some(BigInt::from(system_state.stake_subsidy_balance)),
                distribution_counter: Some(system_state.stake_subsidy_distribution_counter),
                current_distribution_amount: Some(BigInt::from(
                    system_state.stake_subsidy_current_distribution_amount,
                )),
                period_length: Some(system_state.stake_subsidy_period_length),
                decrease_rate: Some(system_state.stake_subsidy_decrease_rate as u64),
            }),
            validator_set: Some(ValidatorSet {
                total_stake: Some(BigInt::from(system_state.total_stake)),
                active_validators: Some(active_validators),
                pending_removals: Some(system_state.pending_removals.clone()),
                pending_active_validators_size: Some(system_state.pending_active_validators_size),
                stake_pool_mappings_size: Some(system_state.staking_pool_mappings_size),
                inactive_pools_size: Some(system_state.inactive_pools_size),
                validator_candidates_size: Some(system_state.validator_candidates_size),
            }),
            storage_fund: Some(StorageFund {
                total_object_storage_rebates: Some(BigInt::from(
                    system_state.storage_fund_total_object_storage_rebates,
                )),
                non_refundable_balance: Some(BigInt::from(
                    system_state.storage_fund_non_refundable_balance,
                )),
            }),
            safe_mode: Some(SafeMode {
                enabled: Some(system_state.safe_mode),
                gas_summary: self.gas_cost_summary,
            }),
            start_timestamp: Some(start_timestamp),
        }))
    }

    async fn protocol_configs(&self, ctx: &Context<'_>) -> Result<Option<ProtocolConfigs>> {
        Ok(Some(
            ctx.data_provider()
                .fetch_protocol_config(Some(self.epoch_id))
                .await?,
        ))
    }
}
