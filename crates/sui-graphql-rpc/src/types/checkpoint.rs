// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use super::{
    base64::Base64,
    cursor::{self, Page, Paginated, Target},
    date_time::DateTime,
    digest::Digest,
    epoch::Epoch,
    gas::GasCostSummary,
    transaction_block::{self, TransactionBlock, TransactionBlockFilter},
};
use crate::consistency::Checkpointed;
use crate::{
    data::{self, Conn, Db, DbConnection, QueryExecutor},
    error::Error,
};
use async_graphql::{
    connection::{Connection, CursorType, Edge},
    *,
};
use diesel::{CombineDsl, ExpressionMethods, OptionalExtension, QueryDsl};
use fastcrypto::encoding::{Base58, Encoding};
use serde::{Deserialize, Serialize};
use sui_indexer::{
    models::checkpoints::StoredCheckpoint,
    schema::{checkpoints, objects_snapshot},
};
use sui_types::messages_checkpoint::CheckpointDigest;

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
    // The checkpoint_sequence_number at which this was viewed at, or `None` if the data was
    // requested at the latest checkpoint.
    pub checkpoint_viewed_at: Option<u64>,
}

pub(crate) type Cursor = cursor::JsonCursor<CheckpointCursor>;
type Query<ST, GB> = data::Query<ST, checkpoints::table, GB>;

/// The cursor returned for each `Checkpoint` in a connection's page of results. The
/// `checkpoint_viewed_at` will set the consistent upper bound for subsequent queries made on this
/// cursor.
#[derive(Serialize, Deserialize, Clone, PartialEq, Eq)]
pub(crate) struct CheckpointCursor {
    /// The checkpoint sequence number this was viewed at.
    #[serde(rename = "c")]
    pub checkpoint_viewed_at: u64,
    #[serde(rename = "s")]
    pub sequence_number: u64,
}

/// Checkpoints contain finalized transactions and are used for node synchronization
/// and global transaction ordering.
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

    /// The total number of transaction blocks in the network by the end of this checkpoint.
    async fn network_total_transactions(&self) -> Option<u64> {
        Some(self.network_total_transactions_impl())
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
        Epoch::query(
            ctx.data_unchecked(),
            Some(self.stored.epoch as u64),
            self.checkpoint_viewed_at,
        )
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

        TransactionBlock::paginate(
            ctx.data_unchecked(),
            page,
            filter,
            self.checkpoint_viewed_at,
        )
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

    pub(crate) fn network_total_transactions_impl(&self) -> u64 {
        self.stored.network_total_transactions as u64
    }

    pub(crate) fn digest_impl(&self) -> Result<CheckpointDigest, Error> {
        CheckpointDigest::try_from(self.stored.checkpoint_digest.clone())
            .map_err(|e| Error::Internal(format!("Failed to deserialize checkpoint digest: {e}")))
    }

    /// Look up a `Checkpoint` in the database, filtered by either sequence number or digest. If
    /// both filters are supplied they will both be applied. If none are supplied, the latest
    /// checkpoint is fetched.
    pub(crate) async fn query(
        db: &Db,
        filter: CheckpointId,
        checkpoint_viewed_at: Option<u64>,
    ) -> Result<Option<Self>, Error> {
        use checkpoints::dsl;

        let digest = filter.digest.map(|d| d.to_vec());
        let seq_num = filter.sequence_number.map(|n| n as i64);

        let (stored, checkpoint_viewed_at): (Option<StoredCheckpoint>, u64) = db
            .execute_repeatable(move |conn| {
                let checkpoint_viewed_at = match checkpoint_viewed_at {
                    Some(value) => Ok(value),
                    None => Checkpoint::available_range(conn).map(|(_, rhs)| rhs),
                }?;

                let stored = conn
                    .first(move || {
                        let mut query = dsl::checkpoints
                            .order_by(dsl::sequence_number.desc())
                            .into_boxed();

                        if let Some(digest) = digest.clone() {
                            query = query.filter(dsl::checkpoint_digest.eq(digest));
                        }

                        if let Some(seq_num) = seq_num {
                            query = query.filter(dsl::sequence_number.eq(seq_num));
                        }

                        query
                    })
                    .optional()?;

                Ok::<_, diesel::result::Error>((stored, checkpoint_viewed_at))
            })
            .await
            .map_err(|e| Error::Internal(format!("Failed to fetch checkpoint: {e}")))?;

        Ok(stored.map(|stored| Checkpoint {
            stored,
            checkpoint_viewed_at: Some(checkpoint_viewed_at),
        }))
    }

    pub(crate) async fn query_latest_checkpoint_sequence_number(db: &Db) -> Result<u64, Error> {
        db.execute(move |conn| Checkpoint::latest_checkpoint_sequence_number(conn))
            .await
            .map_err(|e| Error::Internal(format!("Failed to fetch checkpoint: {e}")))
    }

    /// Queries the database for the upper bound of the available range supported by the graphql
    /// server. This method takes a connection, so that it can be used in an execute_repeatable
    /// transaction.
    pub(crate) fn latest_checkpoint_sequence_number(
        conn: &mut Conn,
    ) -> Result<u64, diesel::result::Error> {
        use checkpoints::dsl;

        let result: i64 = conn.first(move || {
            dsl::checkpoints
                .select(dsl::sequence_number)
                .order_by(dsl::sequence_number.desc())
        })?;

        Ok(result as u64)
    }

    /// Query the database for a `page` of checkpoints. The Page uses the checkpoint sequence number
    /// of the stored checkpoint and the checkpoint at which this was viewed at as the cursor, and
    /// can optionally be further `filter`-ed by an epoch number (to only return checkpoints within
    /// that epoch).
    ///
    /// The `checkpoint_viewed_at` parameter is an Option<u64> representing the
    /// checkpoint_sequence_number at which this page was queried for, or `None` if the data was
    /// requested at the latest checkpoint. Each entity returned in the connection will inherit this
    /// checkpoint, so that when viewing that entity's state, it will be from the reference of this
    /// checkpoint_viewed_at parameter.
    ///
    /// If the `Page<Cursor>` is set, then this function will defer to the `checkpoint_viewed_at` in
    /// the cursor if they are consistent.
    pub(crate) async fn paginate(
        db: &Db,
        page: Page<Cursor>,
        filter: Option<u64>,
        checkpoint_viewed_at: Option<u64>,
    ) -> Result<Connection<String, Checkpoint>, Error> {
        use checkpoints::dsl;
        let cursor_viewed_at = page.validate_cursor_consistency()?;
        let checkpoint_viewed_at: Option<u64> = cursor_viewed_at.or(checkpoint_viewed_at);

        let ((prev, next, results), rhs) = db
            .execute_repeatable(move |conn| {
                let checkpoint_viewed_at = match checkpoint_viewed_at {
                    Some(value) => Ok(value),
                    None => Checkpoint::available_range(conn).map(|(_, rhs)| rhs),
                }?;

                let result = page.paginate_query::<StoredCheckpoint, _, _, _>(
                    conn,
                    checkpoint_viewed_at,
                    move || {
                        let mut query = dsl::checkpoints.into_boxed();
                        query = query.filter(dsl::sequence_number.le(checkpoint_viewed_at as i64));
                        if let Some(epoch) = filter {
                            query = query.filter(dsl::epoch.eq(epoch as i64));
                        }
                        query
                    },
                )?;

                Ok::<_, diesel::result::Error>((result, checkpoint_viewed_at))
            })
            .await?;

        // Defer to the provided checkpoint_viewed_at, but if it is not provided, use the
        // current available range. This sets a consistent upper bound for the nested queries.
        let mut conn = Connection::new(prev, next);
        let checkpoint_viewed_at = checkpoint_viewed_at.unwrap_or(rhs);
        for stored in results {
            let cursor = stored.cursor(checkpoint_viewed_at).encode_cursor();
            conn.edges.push(Edge::new(
                cursor,
                Checkpoint {
                    stored,
                    checkpoint_viewed_at: Some(checkpoint_viewed_at),
                },
            ));
        }

        Ok(conn)
    }

    /// Queries the database for the available range supported by the graphql server. This method
    /// takes a connection, so that it can be used in an execute_repeatable transaction.
    pub(crate) fn available_range(conn: &mut Conn) -> Result<(u64, u64), diesel::result::Error> {
        use checkpoints::dsl as checkpoints;
        use objects_snapshot::dsl as snapshots;

        let checkpoint_range: Vec<i64> = conn.results(move || {
            let rhs = checkpoints::checkpoints
                .select(checkpoints::sequence_number)
                .order(checkpoints::sequence_number.desc())
                .limit(1);

            let lhs = snapshots::objects_snapshot
                .select(snapshots::checkpoint_sequence_number)
                .order(snapshots::checkpoint_sequence_number.desc())
                .limit(1);

            lhs.union(rhs)
        })?;

        Ok(match checkpoint_range.as_slice() {
            [] => (0, 0),
            [single_value] => (0, *single_value as u64),
            values => {
                let min_value = *values.iter().min().unwrap();
                let max_value = *values.iter().max().unwrap();
                (min_value as u64, max_value as u64)
            }
        })
    }
}

impl Paginated<Cursor> for StoredCheckpoint {
    type Source = checkpoints::table;

    fn filter_ge<ST, GB>(cursor: &Cursor, query: Query<ST, GB>) -> Query<ST, GB> {
        query.filter(checkpoints::dsl::sequence_number.ge(cursor.sequence_number as i64))
    }

    fn filter_le<ST, GB>(cursor: &Cursor, query: Query<ST, GB>) -> Query<ST, GB> {
        query.filter(checkpoints::dsl::sequence_number.le(cursor.sequence_number as i64))
    }

    fn order<ST, GB>(asc: bool, query: Query<ST, GB>) -> Query<ST, GB> {
        use checkpoints::dsl;
        if asc {
            query.order(dsl::sequence_number)
        } else {
            query.order(dsl::sequence_number.desc())
        }
    }
}

impl Target<Cursor> for StoredCheckpoint {
    fn cursor(&self, checkpoint_viewed_at: u64) -> Cursor {
        Cursor::new(CheckpointCursor {
            checkpoint_viewed_at,
            sequence_number: self.sequence_number as u64,
        })
    }
}

impl Checkpointed for Cursor {
    fn checkpoint_viewed_at(&self) -> u64 {
        self.checkpoint_viewed_at
    }
}
