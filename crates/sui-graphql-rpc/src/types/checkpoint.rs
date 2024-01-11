// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use super::{
    base64::Base64,
    cursor::{self, Page, Target},
    date_time::DateTime,
    digest::Digest,
    epoch::Epoch,
    gas::GasCostSummary,
    transaction_block::{self, TransactionBlock, TransactionBlockFilter},
};
use crate::{
    data::{BoxedQuery, Db, QueryExecutor},
    error::Error,
};
use async_graphql::{
    connection::{Connection, CursorType, Edge},
    *,
};
use diesel::{ExpressionMethods, QueryDsl};
use fastcrypto::encoding::{Base58, Encoding};
use sui_indexer::{models_v2::checkpoints::StoredCheckpoint, schema_v2::checkpoints};
use sui_types::messages_checkpoint::{CheckpointCommitment, CheckpointDigest};

/// Filter either by the digest, or the sequence number, or neither, to get the latest checkpoint.
#[derive(Default, InputObject)]
pub(crate) struct CheckpointId {
    pub digest: Option<Digest>,
    pub sequence_number: Option<u64>,
}

#[derive(Clone)]
pub(crate) struct Checkpoint {
    /// Representation of transaction data in the Indexer's Store. The indexer stores the
    /// transaction data and its effects together, in one table.
    pub stored: StoredCheckpoint,
}

pub(crate) type Cursor = cursor::Cursor<u64>;
type Query<ST, GB> = BoxedQuery<ST, checkpoints::table, Db, GB>;

#[Object]
impl Checkpoint {
    /// A 32-byte hash that uniquely identifies the checkpoint contents, encoded in Base58. This
    /// hash can be used to verify checkpoint contents by checking signatures against the committee,
    /// Hashing contents to match digest, and checking that the previous checkpoint digest matches.
    async fn digest(&self) -> Result<String> {
        Ok(self.digest_impl().extend()?.base58_encode())
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
        Epoch::query(ctx.data_unchecked(), Some(self.stored.epoch as u64))
            .await
            .extend()
    }

    /// Transactions in this checkpoint.
    async fn transaction_blocks(
        &self,
        ctx: &Context<'_>,
        first: Option<u64>,
        after: Option<transaction_block::Cursor>,
        last: Option<u64>,
        before: Option<transaction_block::Cursor>,
        filter: Option<TransactionBlockFilter>,
    ) -> Result<Connection<String, TransactionBlock>> {
        let page = Page::from_params(ctx.data_unchecked(), first, after, last, before)?;

        let Some(filter) = filter
            .unwrap_or_default()
            .intersect(TransactionBlockFilter {
                at_checkpoint: Some(self.stored.sequence_number as u64),
                ..Default::default()
            })
        else {
            return Ok(Connection::new(false, false));
        };

        TransactionBlock::paginate(ctx.data_unchecked(), page, filter)
            .await
            .extend()
    }
}

impl CheckpointId {
    pub(crate) fn by_seq_num(seq_num: u64) -> Self {
        CheckpointId {
            sequence_number: Some(seq_num),
            digest: None,
        }
    }
}

impl Checkpoint {
    pub(crate) fn sequence_number_impl(&self) -> u64 {
        self.stored.sequence_number as u64
    }

    pub(crate) fn digest_impl(&self) -> Result<CheckpointDigest, Error> {
        CheckpointDigest::try_from(self.stored.checkpoint_digest.clone())
            .map_err(|e| Error::Internal(format!("Failed to deserialize checkpoint digest: {e}")))
    }

    /// Look up a `Checkpoint` in the database, filtered by either sequence number or digest. If
    /// both filters are supplied they will both be applied. If none are supplied, the latest
    /// checkpoint is fetched.
    pub(crate) async fn query(db: &Db, filter: CheckpointId) -> Result<Option<Self>, Error> {
        use checkpoints::dsl;

        let digest = filter.digest.map(|d| d.to_vec());
        let seq_num = filter.sequence_number.map(|n| n as i64);

        let stored = db
            .optional(move || {
                let mut query = dsl::checkpoints
                    .order_by(dsl::sequence_number.desc())
                    .limit(1)
                    .into_boxed();

                if let Some(digest) = digest.clone() {
                    query = query.filter(dsl::checkpoint_digest.eq(digest));
                }

                if let Some(seq_num) = seq_num {
                    query = query.filter(dsl::sequence_number.eq(seq_num));
                }

                query
            })
            .await
            .map_err(|e| Error::Internal(format!("Failed to fetch checkpoint: {e}")))?;

        Ok(stored.map(|stored| Checkpoint { stored }))
    }

    /// Query the database for a `page` of checkpoints. The Page uses checkpoint sequence numbers as
    /// the cursor, and can optionally be further `filter`-ed by an epoch number (to only return
    /// checkpoints within that epoch).
    pub(crate) async fn paginate(
        db: &Db,
        page: Page<u64>,
        filter: Option<u64>,
    ) -> Result<Connection<String, Checkpoint>, Error> {
        use checkpoints::dsl;

        let (prev, next, results) = page
            .paginate_query::<StoredCheckpoint, _, _, _>(db, move || {
                let mut query = dsl::checkpoints.into_boxed();
                if let Some(epoch) = filter {
                    query = query.filter(dsl::epoch.eq(epoch as i64));
                }

                query
            })
            .await?;

        let mut conn = Connection::new(prev, next);
        for stored in results {
            let cursor = Cursor::new(stored.cursor()).encode_cursor();
            conn.edges.push(Edge::new(cursor, Checkpoint { stored }));
        }

        Ok(conn)
    }
}

impl Target<u64> for StoredCheckpoint {
    type Source = checkpoints::table;

    fn filter_ge<ST, GB>(cursor: &u64, query: Query<ST, GB>) -> Query<ST, GB> {
        query.filter(checkpoints::dsl::sequence_number.ge(*cursor as i64))
    }

    fn filter_le<ST, GB>(cursor: &u64, query: Query<ST, GB>) -> Query<ST, GB> {
        query.filter(checkpoints::dsl::sequence_number.le(*cursor as i64))
    }

    fn order<ST, GB>(asc: bool, query: Query<ST, GB>) -> Query<ST, GB> {
        use checkpoints::dsl;
        if asc {
            query.order(dsl::sequence_number)
        } else {
            query.order(dsl::sequence_number.desc())
        }
    }

    fn cursor(&self) -> u64 {
        self.sequence_number as u64
    }
}
