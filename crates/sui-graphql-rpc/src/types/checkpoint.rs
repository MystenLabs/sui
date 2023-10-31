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
    /// The digest of the checkpoint is a hash of the checkpoint contents and can be used to verify the blockchain.
    pub digest: String,
    /// This number is set by validators and is 1 greater than the previous checkpoint's sequence number.
    pub sequence_number: u64,
    /// Timestamp of transactions and effects in the checkpoint, taken from the consensus commit which the checkpoint is based on.
    pub timestamp: Option<DateTime>,
    /// This is an aggregation of signatures from 2f+1 validators for the checkpoint proposal.
    pub validator_signature: Option<Base64>,
    /// The digest of the checkpoint at the previous sequence number.
    pub previous_checkpoint_digest: Option<String>,
    /// This is a commitment by the committee at the end of epoch
    /// on the contents of the live object set at that time.
    /// This can be used to verify state snapshots.
    pub live_object_set_digest: Option<String>,
    /// Tracks the total number of transaction blocks in the network at the time of the checkpoint.
    pub network_total_transactions: Option<u64>,
    /// The computation and storage cost, storage rebate, and nonrefundable storage fee of the checkpoint.
    /// These values should increase throughout the checkpoints.
    pub rolling_gas_summary: Option<GasCostSummary>,
    #[graphql(skip)]
    pub epoch_id: u64,
    /// End of epoch data is only available on the final checkpoint of an epoch.
    /// This field provides information on the new committee and protocol version for the next epoch.
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
