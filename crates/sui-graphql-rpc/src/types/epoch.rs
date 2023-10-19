// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::context_data::context_ext::DataProviderContextExt;
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
            ctx.data_provider()
                .fetch_protocol_config(Some(self.protocol_version))
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

        // TODO (wlmyng): The combination of ordering by tx_sequence_number and filtering by checkpoint_sequence_number is too slow
        let filter = if last.is_some() {
            TransactionBlockFilter {
                before_checkpoint: stored_epoch.last_checkpoint_id.map(|id| id as u64),
                after_checkpoint: stored_epoch.last_checkpoint_id.map(|id| (id - 10) as u64),
                ..existing_filter
            }
        } else {
            TransactionBlockFilter {
                after_checkpoint: Some((stored_epoch.first_checkpoint_id - 1) as u64),
                before_checkpoint: Some((stored_epoch.first_checkpoint_id + 10) as u64),
                ..existing_filter
            }
        };

        ctx.data_unchecked::<PgManager>()
            .fetch_txs(first, after, last, before, Some(filter))
            .await
            .extend()
    }
}
