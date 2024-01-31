// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use async_graphql::connection::CursorType;
use serde::{Deserialize, Serialize};

use crate::data::Conn;
use crate::raw_query::RawQuery;
use crate::types::checkpoint::Checkpoint;
use crate::types::cursor::JsonCursor;
use crate::{filter, query};

pub(crate) enum View {
    /// Return objects that fulfill the filtering criteria, even if there are more recent versions
    /// of the object within the checkpoint range
    Historical,
    /// Return objects that fulfill the filtering criteria and are the most recent version within
    /// the checkpoint range
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
/// `lhs` and `rhs`. If the `view` parameter is set to `Consistent`, the query additionally filters
/// out objects that satisfy the provided filters, but are not the most recent version of the object
/// within the checkpoint range. If the view parameter is set to `Historical`, this final filter is
/// not applied.
pub(crate) fn build_objects_query<F>(view: View, lhs: i64, rhs: i64, filter_fn: F) -> RawQuery
where
    F: Fn(RawQuery) -> RawQuery,
{
    // Construct the filtered inner query - apply the same filtering criteria to both
    // objects_snapshot and objects_history tables.
    let mut snapshot_objs = query!(r#"SELECT * FROM objects_snapshot"#);
    snapshot_objs = filter_fn(snapshot_objs);

    // Additionally filter objects_history table for results between the available range, or
    // checkpoint_viewed_at, if provided.
    let mut history_objs = query!(r#"SELECT * FROM objects_history"#);
    history_objs = filter_fn(history_objs);
    history_objs = filter!(
        history_objs,
        format!(r#"checkpoint_sequence_number BETWEEN {} AND {}"#, lhs, rhs)
    );

    // Combine the two queries, and select the most recent version of each object. The result set is
    // the most recent version of objects from `objects_snapshot` and `objects_history` that match
    // the filter criteria.
    let candidates = query!(
        r#"SELECT DISTINCT ON (object_id) * FROM (({}) UNION ({})) o"#,
        snapshot_objs,
        history_objs
    )
    .order_by("object_id")
    .order_by("object_version DESC");

    // The following conditions ensure that the version of object matching our filters is the latest
    // version at the checkpoint we are viewing at. If the filter includes version constraints (an
    // `object_keys` field), then this extra check is not required (it will filter out correct
    // results).
    match view {
        View::Consistent => {
            let mut newer = query!("SELECT object_id, object_version FROM objects_history");
            newer = filter!(
                newer,
                format!(r#"checkpoint_sequence_number BETWEEN {} AND {}"#, lhs, rhs)
            );
            let query = query!(
                r#"SELECT candidates.*
                FROM ({}) candidates
                LEFT JOIN ({}) newer
                ON (
                    candidates.object_id = newer.object_id
                    AND candidates.object_version < newer.object_version
                )"#,
                candidates,
                newer
            );
            filter!(query, "newer.object_version IS NULL")
        }
        View::Historical => {
            query!("SELECT * FROM ({}) candidates", candidates)
        }
    }
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
