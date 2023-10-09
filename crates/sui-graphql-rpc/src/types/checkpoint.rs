// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::context_data::db_data_provider::PgManager;

use super::{
    base64::Base64,
    date_time::DateTime,
    end_of_epoch_data::EndOfEpochData,
    epoch::Epoch,
    gas::GasCostSummary,
    transaction_block::{TransactionBlock, TransactionBlockFilter},
};
use async_graphql::{connection::Connection, *};

#[derive(InputObject)]
pub(crate) struct CheckpointId {
    pub digest: Option<String>,
    pub sequence_number: Option<u64>,
}

#[derive(Clone, Debug, PartialEq, Eq, SimpleObject)]
#[graphql(complex)]
pub(crate) struct Checkpoint {
    // id: ID1,
    pub digest: String,
    pub sequence_number: u64,
    pub timestamp: Option<DateTime>,
    pub validator_signature: Option<Base64>,
    pub previous_checkpoint_digest: Option<String>,
    pub live_object_set_digest: Option<String>,
    pub network_total_transactions: Option<u64>,
    pub rolling_gas_summary: Option<GasCostSummary>,
    #[graphql(skip)]
    pub epoch_id: u64,
    pub end_of_epoch: Option<EndOfEpochData>,
    // transactionConnection(first: Int, after: String, last: Int, before: String): TransactionBlockConnection
    // address_metrics: AddressMetrics,
}

#[ComplexObject]
impl Checkpoint {
    async fn epoch(&self, ctx: &Context<'_>) -> Result<Option<Epoch>> {
        let epoch = ctx
            .data_unchecked::<PgManager>()
            .fetch_epoch_strict(self.epoch_id)
            .await
            .extend()?;

        Ok(Some(epoch))
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
        let mut filter = filter;
        filter.get_or_insert_with(Default::default).at_checkpoint = Some(self.sequence_number);

        ctx.data_unchecked::<PgManager>()
            .fetch_txs(first, after, last, before, filter)
            .await
            .extend()
    }
}
