// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::context_data::db_data_provider::{convert_to_validators, PgManager};
use crate::error::Error;

use super::big_int::BigInt;
use super::checkpoint::Checkpoint;
use super::date_time::DateTime;
use super::protocol_config::ProtocolConfigs;
use super::transaction_block::{TransactionBlock, TransactionBlockFilter};
use super::validator_set::ValidatorSet;
use async_graphql::connection::Connection;
use async_graphql::*;
use sui_indexer::models_v2::epoch::StoredEpochInfo;
use sui_types::sui_system_state::sui_system_state_summary::SuiValidatorSummary;

// #[derive(Clone, Debug, PartialEq, Eq, SimpleObject)]
// #[graphql(complex)]
#[derive(Clone, Debug)]
pub(crate) struct Epoch {
    pub stored_epoch_info: StoredEpochInfo,
    pub validators_summary: Vec<SuiValidatorSummary>,
}

#[Object]
impl Epoch {
    /// The epoch's protocol version
    #[graphql(skip)]
    pub fn protocol_version(&self) -> u64 {
        self.stored_epoch_info.protocol_version as u64
    }

    /// The epoch's id as a sequence number that starts at 0 and it is incremented by one at every epoch change
    async fn epoch_id(&self) -> u64 {
        self.stored_epoch_info.epoch as u64
    }

    /// The minimum gas price that a quorum of validators are guaranteed to sign a transaction for
    async fn reference_gas_price(&self) -> Option<BigInt> {
        Some(BigInt::from(
            self.stored_epoch_info.reference_gas_price as u64,
        ))
    }

    /// Validator related properties, including the active validators
    async fn validator_set(&self) -> Option<ValidatorSet> {
        let active_validators = convert_to_validators(self.validators_summary.clone(), None);
        let validator_set = ValidatorSet {
            total_stake: self
                .stored_epoch_info
                .new_total_stake
                .map(|s| BigInt::from(s as u64)),
            active_validators: Some(active_validators),
            ..Default::default()
        };
        Some(validator_set)
    }

    /// The epoch's starting timestamp
    async fn start_timestamp(&self) -> Option<DateTime> {
        DateTime::from_ms(self.stored_epoch_info.epoch_start_timestamp)
    }
    /// The epoch's ending timestamp
    async fn end_timestamp(&self) -> Option<DateTime> {
        self.stored_epoch_info
            .epoch_end_timestamp
            .and_then(DateTime::from_ms)
    }
    /// The total number of checkpoints in this epoch.
    async fn total_checkpoints(&self) -> Option<BigInt> {
        self.stored_epoch_info
            .last_checkpoint_id
            .map(|last_chckp_id| {
                BigInt::from(last_chckp_id - self.stored_epoch_info.first_checkpoint_id)
            })
    }
    /// The total amount of gas fees (in MIST) that were paid in this epoch.
    async fn total_gas_fees(&self) -> Option<BigInt> {
        self.stored_epoch_info.total_gas_fees.map(BigInt::from)
    }
    /// The total MIST rewarded as stake.
    async fn total_stake_rewards(&self) -> Option<BigInt> {
        self.stored_epoch_info
            .total_stake_rewards_distributed
            .map(BigInt::from)
    }
    /// The amount added to total gas fees to make up the total stake rewards.
    async fn total_stake_subsidies(&self) -> Option<BigInt> {
        self.stored_epoch_info
            .stake_subsidy_amount
            .map(BigInt::from)
    }
    /// The storage fund available in this epoch.
    /// This fund is used to redistribute storage fees from past transactions
    /// to future validators.
    async fn fund_size(&self) -> Option<BigInt> {
        self.stored_epoch_info
            .storage_fund_balance
            .map(BigInt::from)
        //             fund_outflow: e.storage_rebate.map(BigInt::from),
    }
    /// The difference between the fund inflow and outflow, representing
    /// the net amount of storage fees accumulated in this epoch.
    async fn net_inflow(&self) -> Option<BigInt> {
        if let (Some(fund_inflow), Some(fund_outflow)) = (
            self.stored_epoch_info.storage_charge,
            self.stored_epoch_info.storage_rebate,
        ) {
            Some(BigInt::from(fund_inflow - fund_outflow))
        } else {
            None
        }
    }
    /// The storage fees paid for transactions executed during the epoch.
    async fn fund_inflow(&self) -> Option<BigInt> {
        self.stored_epoch_info.storage_charge.map(BigInt::from)
    }
    /// The storage fee rebates paid to users
    /// who deleted the data associated with past transactions.
    async fn fund_outflow(&self) -> Option<BigInt> {
        self.stored_epoch_info.storage_rebate.map(BigInt::from)
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

    /// The epoch's corresponding checkpoints
    async fn checkpoint_connection(
        &self,
        ctx: &Context<'_>,
        first: Option<u64>,
        after: Option<String>,
        last: Option<u64>,
        before: Option<String>,
    ) -> Result<Option<Connection<String, Checkpoint>>> {
        let epoch = self.stored_epoch_info.epoch as u64;
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
        let stored_epoch = &self.stored_epoch_info;

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

impl TryFrom<StoredEpochInfo> for Epoch {
    type Error = Error;
    fn try_from(e: StoredEpochInfo) -> Result<Self, Error> {
        let stored_epoch_info = e.clone();
        let validators_summary: Vec<SuiValidatorSummary> = e
            .validators
            .into_iter()
            .flatten()
            .map(|v| {
                bcs::from_bytes(&v).map_err(|e| {
                    Error::Internal(format!(
                        "Can't convert validator into Validator. Error: {e}",
                    ))
                })
            })
            .collect::<Result<Vec<_>, Error>>()?;
        Ok(Epoch {
            stored_epoch_info,
            validators_summary,
        })
    }
}
