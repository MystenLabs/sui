// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::context_data::db_data_provider::{convert_to_validators, PgManager};
use crate::error::Error;

use super::big_int::BigInt;
use super::checkpoint::Checkpoint;
use super::date_time::DateTime;
use super::gas::GasCostSummary;
use super::protocol_config::ProtocolConfigs;
use super::safe_mode::SafeMode;
use super::stake_subsidy::StakeSubsidy;
use super::storage_fund::StorageFund;
use super::system_parameters::SystemParameters;
use super::transaction_block::{TransactionBlock, TransactionBlockFilter};
use super::validator_set::ValidatorSet;
use async_graphql::connection::Connection;
use async_graphql::*;
use sui_indexer::models_v2::epoch::StoredEpochInfo;
use sui_types::sui_system_state::sui_system_state_summary::SuiSystemStateSummary as NativeSuiSystemStateSummary;

#[derive(Clone, Debug)]
pub(crate) struct Epoch {
    pub stored: StoredEpochInfo,
    pub system_state: NativeSuiSystemStateSummary,
}

#[Object]
impl Epoch {
    /// The epoch's id as a sequence number that starts at 0 and is incremented by one at every epoch change
    async fn epoch_id(&self) -> u64 {
        self.stored.epoch as u64
    }

    /// The minimum gas price that a quorum of validators are guaranteed to sign a transaction for
    async fn reference_gas_price(&self) -> Option<BigInt> {
        Some(BigInt::from(self.stored.reference_gas_price as u64))
    }

    /// Validator related properties, including the active validators
    async fn validator_set(&self) -> Result<Option<ValidatorSet>> {
        let system_state: NativeSuiSystemStateSummary = bcs::from_bytes(&self.stored.system_state)
            .map_err(|e| {
                Error::Internal(format!(
                    "Can't convert system_state into SystemState. Error: {e}",
                ))
            })?;

        let active_validators = convert_to_validators(system_state.active_validators, None);
        let validator_set = ValidatorSet {
            total_stake: Some(BigInt::from(self.stored.total_stake)),
            active_validators: Some(active_validators),
            ..Default::default()
        };
        Ok(Some(validator_set))
    }

    /// The epoch's starting timestamp
    async fn start_timestamp(&self) -> Option<DateTime> {
        DateTime::from_ms(self.stored.epoch_start_timestamp)
    }

    /// The epoch's ending timestamp
    async fn end_timestamp(&self) -> Option<DateTime> {
        DateTime::from_ms(self.stored.epoch_end_timestamp?)
    }

    /// The total number of checkpoints in this epoch.
    async fn total_checkpoints(&self, ctx: &Context<'_>) -> Result<Option<BigInt>, Error> {
        let last = match self.stored.last_checkpoint_id {
            Some(last) => last as u64,
            None => {
                ctx.data_unchecked::<PgManager>()
                    .fetch_latest_checkpoint()
                    .await?
                    .sequence_number
            }
        };
        Ok(Some(BigInt::from(
            last - self.stored.first_checkpoint_id as u64,
        )))
    }

    /// The total amount of gas fees (in MIST) that were paid in this epoch.
    async fn total_gas_fees(&self) -> Option<BigInt> {
        self.stored.total_gas_fees.map(BigInt::from)
    }

    /// The total MIST rewarded as stake.
    async fn total_stake_rewards(&self) -> Option<BigInt> {
        self.stored
            .total_stake_rewards_distributed
            .map(BigInt::from)
    }

    /// The amount added to total gas fees to make up the total stake rewards.
    async fn total_stake_subsidies(&self) -> Option<BigInt> {
        self.stored.stake_subsidy_amount.map(BigInt::from)
    }

    /// SUI set aside to account for objects stored on-chain, at the start of the epoch.

    async fn storage_fund(&self) -> Option<StorageFund> {
        Some(StorageFund {
            total_object_storage_rebates: Some(BigInt::from(
                self.system_state.storage_fund_total_object_storage_rebates,
            )),
            non_refundable_balance: Some(BigInt::from(
                self.system_state.storage_fund_non_refundable_balance,
            )),
        })
    }

    /// The storage fund available in this epoch.
    /// This fund is used to redistribute storage fees from past transactions
    /// to future validators.
    async fn fund_size(&self) -> Option<BigInt> {
        Some(BigInt::from(self.stored.storage_fund_balance))
    }

    /// The difference between the fund inflow and outflow, representing
    /// the net amount of storage fees accumulated in this epoch.
    async fn net_inflow(&self) -> Option<BigInt> {
        if let (Some(fund_inflow), Some(fund_outflow)) =
            (self.stored.storage_charge, self.stored.storage_rebate)
        {
            Some(BigInt::from(fund_inflow - fund_outflow))
        } else {
            None
        }
    }

    /// The storage fees paid for transactions executed during the epoch.
    async fn fund_inflow(&self) -> Option<BigInt> {
        self.stored.storage_charge.map(BigInt::from)
    }

    /// The storage fee rebates paid to users
    /// who deleted the data associated with past transactions.
    async fn fund_outflow(&self) -> Option<BigInt> {
        self.stored.storage_rebate.map(BigInt::from)
    }

    /// The epoch's corresponding protocol configuration, including the feature flags and the configuration options
    async fn protocol_configs(&self, ctx: &Context<'_>) -> Result<Option<ProtocolConfigs>> {
        Ok(Some(
            ctx.data_unchecked::<PgManager>()
                .fetch_protocol_configs(Some(self.protocol_version()))
                .await
                .extend()?,
        ))
    }

    /// Information about whether last epoch change used safe mode, which happens if the full epoch
    /// change logic fails for some reason.
    async fn safe_mode(&self) -> Option<SafeMode> {
        Some(SafeMode {
            enabled: Some(self.system_state.safe_mode),
            gas_summary: Some(GasCostSummary {
                computation_cost: self.system_state.safe_mode_computation_rewards,
                storage_cost: self.system_state.safe_mode_storage_rewards,
                storage_rebate: self.system_state.safe_mode_storage_rebates,
                non_refundable_storage_fee: self.system_state.safe_mode_non_refundable_storage_fee,
            }),
        })
    }

    /// The value of the `version` field of `0x5`, the `0x3::sui::SuiSystemState` object.  This
    /// version changes whenever the fields contained in the system state object (held in a dynamic
    /// field attached to `0x5`) change.
    async fn system_state_version(&self) -> Option<BigInt> {
        Some(BigInt::from(self.system_state.system_state_version))
    }

    /// Details of the system that are decided during genesis.
    async fn system_parameters(&self) -> Option<SystemParameters> {
        Some(SystemParameters {
            duration_ms: Some(BigInt::from(self.system_state.epoch_duration_ms)),
            stake_subsidy_start_epoch: Some(self.system_state.stake_subsidy_start_epoch),
            min_validator_count: Some(self.system_state.max_validator_count),
            max_validator_count: Some(self.system_state.max_validator_count),
            min_validator_joining_stake: Some(BigInt::from(
                self.system_state.min_validator_joining_stake,
            )),
            validator_low_stake_threshold: Some(BigInt::from(
                self.system_state.validator_low_stake_threshold,
            )),
            validator_very_low_stake_threshold: Some(BigInt::from(
                self.system_state.validator_very_low_stake_threshold,
            )),
            validator_low_stake_grace_period: Some(BigInt::from(
                self.system_state.validator_low_stake_grace_period,
            )),
        })
    }

    /// Parameters related to subsiding staking rewards
    async fn system_stake_subsidy(&self) -> Option<StakeSubsidy> {
        Some(StakeSubsidy {
            balance: Some(BigInt::from(self.system_state.stake_subsidy_balance)),
            distribution_counter: Some(self.system_state.stake_subsidy_distribution_counter),
            current_distribution_amount: Some(BigInt::from(
                self.system_state.stake_subsidy_current_distribution_amount,
            )),
            period_length: Some(self.system_state.stake_subsidy_period_length),
            decrease_rate: Some(self.system_state.stake_subsidy_decrease_rate as u64),
        })
    }

    /// The epoch's corresponding checkpoints
    async fn checkpoint_connection(
        &self,
        ctx: &Context<'_>,
        first: Option<u64>,
        after: Option<String>,
        last: Option<u64>,
        before: Option<String>,
    ) -> Result<Option<Connection<String, Checkpoint>>> {
        let epoch = self.stored.epoch as u64;
        ctx.data_unchecked::<PgManager>()
            .fetch_checkpoints(first, after, last, before, Some(epoch))
            .await
            .extend()
    }

    /// The epoch's corresponding transaction blocks
    async fn transaction_block_connection(
        &self,
        ctx: &Context<'_>,
        first: Option<u64>,
        after: Option<String>,
        last: Option<u64>,
        before: Option<String>,
        filter: Option<TransactionBlockFilter>,
    ) -> Result<Option<Connection<String, TransactionBlock>>> {
        let stored_epoch = &self.stored;

        let new_filter = TransactionBlockFilter {
            after_checkpoint: if stored_epoch.first_checkpoint_id > 0 {
                Some((stored_epoch.first_checkpoint_id - 1) as u64)
            } else {
                None
            },
            before_checkpoint: stored_epoch.last_checkpoint_id.map(|id| (id + 1) as u64),
            ..filter.unwrap_or_default()
        };

        ctx.data_unchecked::<PgManager>()
            .fetch_txs(first, after, last, before, Some(new_filter))
            .await
            .extend()
    }
}

impl Epoch {
    /// The epoch's protocol version
    pub fn protocol_version(&self) -> u64 {
        self.stored.protocol_version as u64
    }
}

pub fn from_epoch_and_system_state(
    stored: StoredEpochInfo,
    system_state: NativeSuiSystemStateSummary,
) -> Epoch {
    Epoch {
        stored,
        system_state,
    }
}
