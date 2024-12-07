// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::data::{Conn, Db, DbConnection, QueryExecutor};
use crate::error::Error;

use super::checkpoint::{Checkpoint, CheckpointId};
use async_graphql::*;
use diesel::{CombineDsl, ExpressionMethods, QueryDsl, QueryResult};
use diesel_async::scoped_futures::ScopedFutureExt;
use sui_indexer::schema::{checkpoints, objects_snapshot};

#[derive(Clone, Debug, PartialEq, Eq, Copy)]
pub(crate) struct AvailableRange {
    pub first: u64,
    pub last: u64,
}

/// Range of checkpoints that the RPC is guaranteed to produce a consistent response for.
#[Object]
impl AvailableRange {
    async fn first(&self, ctx: &Context<'_>) -> Result<Option<Checkpoint>> {
        Checkpoint::query(ctx, CheckpointId::by_seq_num(self.first), self.last)
            .await
            .extend()
    }

    async fn last(&self, ctx: &Context<'_>) -> Result<Option<Checkpoint>> {
        Checkpoint::query(ctx, CheckpointId::by_seq_num(self.last), self.last)
            .await
            .extend()
    }
}

impl AvailableRange {
    /// Look up the available range when viewing the data consistently at `checkpoint_viewed_at`.
    pub(crate) async fn query(db: &Db, checkpoint_viewed_at: u64) -> Result<Self, Error> {
        let Some(range): Option<Self> = db
            .execute(move |conn| {
                async move { Self::result(conn, checkpoint_viewed_at).await }.scope_boxed()
            })
            .await
            .map_err(|e| Error::Internal(format!("Failed to fetch available range: {e}")))?
        else {
            return Err(Error::Client(format!(
                "Requesting data at checkpoint {checkpoint_viewed_at}, outside the available \
                 range.",
            )));
        };

        Ok(range)
    }

    /// Look up the available range when viewing the data consistently at `checkpoint_viewed_at`.
    /// Made available on the `Conn` type to make it easier to call as part of other queries.
    ///
    /// Returns an error if there was an issue querying the database, Ok(None) if the checkpoint
    /// being viewed is not in the database's available range, or Ok(Some(AvailableRange))
    /// otherwise.
    pub(crate) async fn result(
        conn: &mut Conn<'_>,
        checkpoint_viewed_at: u64,
    ) -> QueryResult<Option<Self>> {
        use checkpoints::dsl as checkpoints;
        use objects_snapshot::dsl as snapshots;

        let checkpoint_range: Vec<i64> = conn
            .results(move || {
                let rhs = checkpoints::checkpoints
                    .select(checkpoints::sequence_number)
                    .order(checkpoints::sequence_number.desc())
                    .limit(1);

                let lhs = snapshots::objects_snapshot
                    .select(snapshots::checkpoint_sequence_number)
                    .order(snapshots::checkpoint_sequence_number.desc())
                    .limit(1);

                // We need to use `union_all` in case `lhs` and `rhs` have the same value.
                lhs.union_all(rhs)
            })
            .await?;

        let (first, mut last) = match checkpoint_range.as_slice() {
            [] => (0, 0),
            [single_value] => (0, *single_value as u64),
            values => {
                let min_value = *values.iter().min().unwrap();
                let max_value = *values.iter().max().unwrap();
                (min_value as u64, max_value as u64)
            }
        };

        if checkpoint_viewed_at < first || last < checkpoint_viewed_at {
            return Ok(None);
        }

        last = checkpoint_viewed_at;
        Ok(Some(Self { first, last }))
    }
}
