// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::context_data::db_data_provider::PgManager;
use crate::error::Error;

use super::big_int::BigInt;
use super::checkpoint::Checkpoint;
use super::date_time::DateTime;
use super::epoch_metrics::EpochMetrics;
use super::protocol_config::ProtocolConfigs;
use super::transaction_block::{TransactionBlock, TransactionBlockFilter};
use super::validator_set::ValidatorSet;
use async_graphql::connection::Connection;
use async_graphql::*;

#[derive(Clone, Debug, PartialEq, Eq, SimpleObject)]
#[graphql(complex)]
pub(crate) struct Epoch {
    /// The epoch's id as a sequence number that starts at 0 and it is incremented by one at every epoch change
    pub epoch_id: u64,
    /// The epoch's protocol version
    #[graphql(skip)]
    pub protocol_version: u64,
    /// The minimum gas price that a quorum of validators are guaranteed to sign a transaction for
    pub reference_gas_price: Option<BigInt>,
    /// Validator related properties, including the active validators
    pub validator_set: Option<ValidatorSet>,
    /// The epoch's starting timestamp
    pub start_timestamp: Option<DateTime>,
    /// The epoch's ending timestamp
    pub end_timestamp: Option<DateTime>,
    /// Epoch's metrics (fees, storage, stakes)
    pub epoch_metrics: Option<EpochMetrics>,
}

#[ComplexObject]
impl Epoch {
    /// The epoch's corresponding protocol configuration, including the feature flags and the configuration options
    async fn protocol_configs(&self, ctx: &Context<'_>) -> Result<Option<ProtocolConfigs>> {
        Ok(Some(
            ctx.data_unchecked::<PgManager>()
                .fetch_protocol_configs(Some(self.protocol_version))
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
        ctx.data_unchecked::<PgManager>()
            .fetch_checkpoints(first, after, last, before, Some(self.epoch_id))
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
        let stored_epoch = ctx
            .data_unchecked::<PgManager>()
            .get_epoch(Some(self.epoch_id as i64))
            .await
            .extend()?
            .ok_or(Error::Internal(
                "Epoch should be able to find itself".to_string(),
            ))
            .extend()?;

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
