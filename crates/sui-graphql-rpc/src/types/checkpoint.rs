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
    // id: ID1
    /// The digest of the checkpoint
    pub digest: String,
    /// Sequence number of the checkpoint
    pub sequence_number: u64,
    /// Timestamp of the checkpoint
    pub timestamp: Option<DateTime>,
    /// The aggregate authority signature
    pub validator_signature: Option<Base64>,
    /// The digest of the previous checkpoint
    pub previous_checkpoint_digest: Option<String>,
    /// A single commitment of ECMHLiveObjectSetDigest
    pub live_object_set_digest: Option<String>,
    /// Tracks the total number of transaction blocks in the network at the time of the checkpoint
    pub network_total_transactions: Option<u64>,
    /// The computation and storage cost, storage rebate, and nonrefundable storage fee of the checkpoint
    /// These values should increase throughout the epoch
    pub rolling_gas_summary: Option<GasCostSummary>,
    #[graphql(skip)]
    pub epoch_id: u64,
    /// End of epoch data is only available on the final checkpoint of an epoch
    pub end_of_epoch: Option<EndOfEpochData>,
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
