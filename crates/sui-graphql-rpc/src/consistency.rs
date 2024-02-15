// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use async_graphql::connection::CursorType;
use serde::{Deserialize, Serialize};
use sui_indexer::models_v2::objects::StoredHistoryObject;

use crate::data::Conn;
use crate::raw_query::RawQuery;
use crate::types::checkpoint::Checkpoint;
use crate::types::cursor::{JsonCursor, Page};
use crate::types::object::Cursor;
use crate::{filter, query};

#[derive(Copy, Clone)]
pub(crate) enum View {
    /// Return objects that fulfill the filtering criteria, even if there are more recent versions
    /// of the object within the checkpoint range. This is used for lookups such as by `object_id`
    /// and `version`.
    Historical,
    /// Return objects that fulfill the filtering criteria and are the most recent version within
    /// the checkpoint range.
    Consistent,
}

/// The consistent cursor for an index into a `Vec` field is constructed from the index of the
/// element and the checkpoint the cursor was constructed at.
#[derive(Serialize, Deserialize, Clone, PartialEq, Eq, Debug)]
pub(crate) struct ConsistentIndexCursor {
    #[serde(rename = "i")]
    pub ix: usize,
    /// The checkpoint sequence number at which the entity corresponding to this cursor was viewed at.
    pub c: u64,
}

/// The consistent cursor for an index into a `Map` field is constructed from the name or key of the
/// element and the checkpoint the cursor was constructed at.
#[derive(Serialize, Deserialize, Clone, PartialEq, Eq, Debug)]
pub(crate) struct ConsistentNamedCursor {
    #[serde(rename = "n")]
    pub name: String,
    /// The checkpoint sequence number at which the entity corresponding to this cursor was viewed at.
    pub c: u64,
}

/// Trait for cursors that have a checkpoint sequence number associated with them.
pub(crate) trait Checkpointed: CursorType {
    fn checkpoint_viewed_at(&self) -> u64;
}

impl Checkpointed for JsonCursor<ConsistentIndexCursor> {
    fn checkpoint_viewed_at(&self) -> u64 {
        self.c
    }
}

impl Checkpointed for JsonCursor<ConsistentNamedCursor> {
    fn checkpoint_viewed_at(&self) -> u64 {
        self.c
    }
}

/// Constructs a `RawQuery` against the `objects_snapshot` and `objects_history` table to fetch
/// objects that satisfy some filtering criteria `filter_fn` within the provided checkpoint range
/// `lhs` and `rhs`. The `objects_snapshot` table contains the latest versions of objects up to the
/// latest checkpoint sequence number on the table, and `objects_history` captures changes after
/// that, so a query to both tables is necessary to capture objects in these scenarios:
/// 1) In snapshot, not in history - occurs when an object gets snapshotted and then has not been
///    modified since
/// 2) In history, not in snapshot - occurs when a new object is created
/// 3) In snapshot and in history - occurs when an object is snapshotted and further modified
///
/// One approach is to issue two queries and merge in the application, but this can also be achieved
/// directly in the database with `UNION ALL` and `SELECT DISTINCT ON (object_id)`.
///
/// The query also applies the `page`'s cursor and limit to each inner query to ensure that the page
/// of results are in step based on `object_id`.
///
/// Additionally, even among the objects that satisfy the filtering criteria, it is possible that
/// there is a yet more recent version of the object within the checkpoint range. The `view`
/// parameter controls whether to filter out such objects. If the `view` parameter is set to
/// `Consistent`, this filter is applied, otherwise if the `view` parameter is set to `Historical`,
/// this filter is not applied.
pub(crate) fn build_objects_query<F>(
    view: View,
    lhs: i64,
    rhs: i64,
    page: &Page<Cursor>,
    filter_fn: F,
) -> RawQuery
where
    F: Fn(RawQuery) -> RawQuery,
{
    let mut newer = query!("SELECT object_id, object_version FROM objects_history");
    newer = filter!(
        newer,
        format!(r#"checkpoint_sequence_number BETWEEN {} AND {}"#, lhs, rhs)
    );

    // Construct the filtered inner query - apply the same filtering criteria to both
    // objects_snapshot and objects_history tables.
    let mut snapshot_objs_inner = query!("SELECT * FROM objects_snapshot");
    snapshot_objs_inner = filter_fn(snapshot_objs_inner);

    let mut snapshot_objs = match view {
        View::Consistent => {
            // The LEFT JOIN filters out objects that have a more recent version within the
            // checkpoint range
            let mut snapshot_objs = query!(
                r#"SELECT candidates.* FROM ({}) candidates
                    LEFT JOIN ({}) newer
                    ON (candidates.object_id = newer.object_id AND candidates.object_version < newer.object_version)"#,
                snapshot_objs_inner,
                newer.clone()
            );
            snapshot_objs = filter!(snapshot_objs, "newer.object_version IS NULL");
            snapshot_objs
        }
        View::Historical => query!(
            "SELECT candidates.* FROM ({}) candidates",
            snapshot_objs_inner
        ),
    };

    // Always apply cursor pagination and limit so that each part of the query is in step, and to
    // avoid a scenario where a user provides more `objectKeys` than allowed by the maximum page
    // size.
    snapshot_objs = page.apply::<StoredHistoryObject>(snapshot_objs);

    // Similar to the snapshot query, construct the filtered inner query for the history table.
    let mut history_objs_inner = query!("SELECT * FROM objects_history");
    // filter_fn must go before filtering on checkpoint_sequence_number - namely for multi-gets
    // based on `object_id` and `object_version`
    history_objs_inner = filter_fn(history_objs_inner);

    let mut history_objs = match view {
        View::Consistent => {
            // Bound the inner `objects_history` query by the checkpoint range
            history_objs_inner = filter!(
                history_objs_inner,
                format!(r#"checkpoint_sequence_number BETWEEN {} AND {}"#, lhs, rhs)
            );

            let mut history_objs = query!(
                r#"SELECT candidates.* FROM ({}) candidates
                    LEFT JOIN ({}) newer
                    ON (candidates.object_id = newer.object_id AND candidates.object_version < newer.object_version)"#,
                history_objs_inner,
                newer
            );
            history_objs = filter!(history_objs, "newer.object_version IS NULL");
            history_objs
        }
        View::Historical => query!(
            "SELECT candidates.* FROM ({}) candidates",
            history_objs_inner
        ),
    };

    // Always apply cursor pagination and limit so that each part of the query is in step, and to
    // avoid a scenario where a user provides more `objectKeys` than allowed by the maximum page
    // size.
    history_objs = page.apply::<StoredHistoryObject>(history_objs);

    // Combine the two queries, and select the most recent version of each object. The result set is
    // the most recent version of objects from `objects_snapshot` and `objects_history` that match
    // the filter criteria.
    let query = query!(
        r#"SELECT DISTINCT ON (object_id) * FROM (({}) UNION ALL ({})) candidates"#,
        snapshot_objs,
        history_objs
    )
    .order_by("object_id")
    .order_by("object_version DESC");

    query!("SELECT * FROM ({}) candidates", query)
}

/// Given a `checkpoint_viewed_at` representing the checkpoint sequence number when the query was
/// made, check whether the value falls under the current available range of the database. Returns
/// `None` if the `checkpoint_viewed_at` lies outside the range, otherwise return a tuple consisting
/// of the available range's lower bound and the `checkpoint_viewed_at`, or the upper bound of the
/// database if `checkpoint_viewed_at` is `None`.
pub(crate) fn consistent_range(
    conn: &mut Conn,
    checkpoint_viewed_at: Option<u64>,
) -> Result<Option<(u64, u64)>, diesel::result::Error> {
    let (lhs, mut rhs) = Checkpoint::available_range(conn)?;

    if let Some(checkpoint_viewed_at) = checkpoint_viewed_at {
        if checkpoint_viewed_at < lhs || rhs < checkpoint_viewed_at {
            return Ok(None);
        }
        rhs = checkpoint_viewed_at;
    }

    Ok(Some((lhs, rhs)))
}
