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
use sui_indexer::models_v2::epoch::QueryableEpochInfo;

pub(crate) struct Epoch {
    pub stored: QueryableEpochInfo,
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
    async fn validator_set(&self, ctx: &Context<'_>) -> Result<Option<ValidatorSet>> {
        let system_state = ctx
            .data_unchecked::<PgManager>()
            .fetch_sui_system_state(Some(self.stored.epoch as u64))
            .await?;

        let active_validators = convert_to_validators(system_state.active_validators, None);
        let validator_set = ValidatorSet {
            total_stake: Some(BigInt::from(self.stored.total_stake)),
            active_validators: Some(active_validators),
            ..Default::default()
        };
        Ok(Some(validator_set))
    }

    /// The epoch's starting timestamp
    async fn start_timestamp(&self) -> Result<DateTime, Error> {
        DateTime::from_ms(self.stored.epoch_start_timestamp)
    }

    /// The epoch's ending timestamp
    async fn end_timestamp(&self) -> Result<Option<DateTime>, Error> {
        self.stored
            .epoch_end_timestamp
            .map(DateTime::from_ms)
            .transpose()
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

impl From<QueryableEpochInfo> for Epoch {
    fn from(e: QueryableEpochInfo) -> Self {
        Epoch { stored: e }
    }
}
