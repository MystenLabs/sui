// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::context_data::db_data_provider::PgManager;
use crate::error::Error;

use super::big_int::BigInt;
use super::checkpoint::Checkpoint;
use super::date_time::DateTime;
use super::protocol_config::ProtocolConfigs;
use super::transaction_block::{TransactionBlock, TransactionBlockFilter};
use super::validator_set::ValidatorSet;
use async_graphql::connection::Connection;
use async_graphql::*;

const CHECKPOINT_RANGE_BOUNDING: i64 = 100;

#[derive(Clone, Debug, PartialEq, Eq, SimpleObject)]
#[graphql(complex)]
pub(crate) struct Epoch {
    pub epoch_id: u64,
    #[graphql(skip)]
    pub protocol_version: u64,
    pub reference_gas_price: Option<BigInt>,
    pub validator_set: Option<ValidatorSet>,
    pub start_timestamp: Option<DateTime>,
    pub end_timestamp: Option<DateTime>,
}

#[ComplexObject]
impl Epoch {
    async fn protocol_configs(&self, ctx: &Context<'_>) -> Result<Option<ProtocolConfigs>> {
        Ok(Some(
            ctx.data_unchecked::<PgManager>()
                .fetch_protocol_configs(Some(self.protocol_version))
                .await?,
        ))
    }

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
            ))?;

        let existing_filter = filter.unwrap_or_default();

        // Upper bound in the absence of after or before to avoid inefficiently large queries
        let (after_default, before_default) = if last.is_some() {
            (
                stored_epoch.last_checkpoint_id.map(|id| {
                    std::cmp::max(
                        id - CHECKPOINT_RANGE_BOUNDING,
                        stored_epoch.first_checkpoint_id,
                    ) as u64
                }),
                stored_epoch.last_checkpoint_id.map(|id| id as u64),
            )
        } else {
            (
                // Subtract and add 1 to include the first and last checkpoints
                Some((stored_epoch.first_checkpoint_id - 1) as u64),
                Some(
                    (std::cmp::min(
                        stored_epoch.first_checkpoint_id + CHECKPOINT_RANGE_BOUNDING,
                        stored_epoch.last_checkpoint_id.map(|id| id + 1).unwrap_or(
                            stored_epoch.first_checkpoint_id + CHECKPOINT_RANGE_BOUNDING,
                        ),
                    )) as u64,
                ),
            )
        };

        let nfilter = match (after.is_some(), before.is_some()) {
            (true, _) | (_, true) => TransactionBlockFilter {
                after_checkpoint: Some((stored_epoch.first_checkpoint_id - 1) as u64),
                before_checkpoint: stored_epoch.last_checkpoint_id.map(|id| (id + 1) as u64),
                ..existing_filter
            },
            _ => TransactionBlockFilter {
                after_checkpoint: after_default,
                before_checkpoint: before_default,
                ..existing_filter
            },
        };

        ctx.data_unchecked::<PgManager>()
            .fetch_txs(first, after, last, before, Some(nfilter))
            .await
            .extend()
    }
}
