// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use super::{
    base64::Base64,
    date_time::DateTime,
    epoch::Epoch,
    gas::GasCostSummary,
    transaction_block::{TransactionBlock, TransactionBlockFilter},
};
use crate::{context_data::db_data_provider::PgManager, error::Error};
use async_graphql::{connection::Connection, *};
use fastcrypto::encoding::{Base58, Encoding};
use sui_indexer::models_v2::checkpoints::StoredCheckpoint;
use sui_types::messages_checkpoint::CheckpointCommitment;

/// Filter either by the digest, or the sequence number, or neither, to get the latest checkpoint.
#[derive(InputObject)]
pub(crate) struct CheckpointId {
    pub digest: Option<String>,
    pub sequence_number: Option<u64>,
}

#[derive(Clone)]
pub(crate) struct Checkpoint {
    /// Representation of transaction data in the Indexer's Store. The indexer stores the
    /// transaction data and its effects together, in one table.
    pub stored: StoredCheckpoint,
}

#[Object]
impl Checkpoint {
    /// A 32-byte hash that uniquely identifies the checkpoint contents, encoded in Base58. This
    /// hash can be used to verify checkpoint contents by checking signatures against the committee,
    /// Hashing contents to match digest, and checking that the previous checkpoint digest matches.
    async fn digest(&self) -> String {
        Base58::encode(&self.stored.checkpoint_digest)
    }

    /// This checkpoint's position in the total order of finalized checkpoints, agreed upon by
    /// consensus.
    async fn sequence_number(&self) -> u64 {
        self.sequence_number_impl()
    }

    /// The timestamp at which the checkpoint is agreed to have happened according to consensus.
    /// Transactions that access time in this checkpoint will observe this timestamp.
    async fn timestamp(&self) -> Result<DateTime> {
        DateTime::from_ms(self.stored.timestamp_ms).extend()
    }

    /// This is an aggregation of signatures from a quorum of validators for the checkpoint
    /// proposal.
    async fn validator_signatures(&self) -> Base64 {
        Base64::from(&self.stored.validator_signature)
    }

    /// The digest of the checkpoint at the previous sequence number.
    async fn previous_checkpoint_digest(&self) -> Option<String> {
        self.stored
            .previous_checkpoint_digest
            .as_ref()
            .map(Base58::encode)
    }

    /// A commitment by the committee at the end of epoch on the contents of the live object set at
    /// that time. This can be used to verify state snapshots.
    async fn live_object_set_digest(&self) -> Result<Option<String>> {
        use CheckpointCommitment as C;
        Ok(
            bcs::from_bytes::<Vec<C>>(&self.stored.checkpoint_commitments)
                .map_err(|e| Error::Internal(format!("Error deserializing commitments: {e}")))
                .extend()?
                .into_iter()
                .map(|commitment| {
                    let C::ECMHLiveObjectSetDigest(digest) = commitment;
                    Base58::encode(digest.digest.into_inner())
                })
                .next(),
        )
    }

    /// The total number of transaction blocks in the network by the end of this checkpoint.
    async fn network_total_transactions(&self) -> Option<u64> {
        Some(self.stored.network_total_transactions as u64)
    }

    /// The computation cost, storage cost, storage rebate, and non-refundable storage fee
    /// accumulated during this epoch, up to and including this checkpoint. These values increase
    /// monotonically across checkpoints in the same epoch, and reset on epoch boundaries.
    async fn rolling_gas_summary(&self) -> Option<GasCostSummary> {
        Some(GasCostSummary {
            computation_cost: self.stored.computation_cost as u64,
            storage_cost: self.stored.storage_cost as u64,
            storage_rebate: self.stored.storage_rebate as u64,
            non_refundable_storage_fee: self.stored.non_refundable_storage_fee as u64,
        })
    }

    /// The epoch this checkpoint is part of.
    async fn epoch(&self, ctx: &Context<'_>) -> Result<Option<Epoch>> {
        let epoch = ctx
            .data_unchecked::<PgManager>()
            .fetch_epoch_strict(self.stored.epoch as u64)
            .await
            .extend()?;

        Ok(Some(epoch))
    }

    /// Transactions in this checkpoint.
    async fn transaction_block_connection(
        &self,
        ctx: &Context<'_>,
        first: Option<u64>,
        after: Option<String>,
        last: Option<u64>,
        before: Option<String>,
        filter: Option<TransactionBlockFilter>,
    ) -> Result<Option<Connection<String, TransactionBlock>>> {
        let mut filter = filter.unwrap_or_default();
        filter.at_checkpoint = Some(self.stored.sequence_number as u64);

        ctx.data_unchecked::<PgManager>()
            .fetch_txs(first, after, last, before, Some(filter))
            .await
            .extend()
    }
}

impl Checkpoint {
    pub(crate) fn sequence_number_impl(&self) -> u64 {
        self.stored.sequence_number as u64
    }
}
