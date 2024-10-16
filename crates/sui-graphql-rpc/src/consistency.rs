// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use async_graphql::connection::CursorType;
use serde::{Deserialize, Serialize};
use sui_indexer::models::objects::StoredHistoryObject;

use crate::raw_query::RawQuery;
use crate::types::available_range::AvailableRange;
use crate::types::cursor::{JsonCursor, Page, ScanLimited};
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

impl ScanLimited for JsonCursor<ConsistentIndexCursor> {}

impl ScanLimited for JsonCursor<ConsistentNamedCursor> {}

/// Constructs a `RawQuery` against the `objects_snapshot` and `objects_history` table to fetch
/// objects that satisfy some filtering criteria `filter_fn` within the provided checkpoint `range`.
/// The `objects_snapshot` table contains the latest versions of objects up to a checkpoint sequence
/// number, and `objects_history` captures changes after that, so a query to both tables is
/// necessary to handle these object states:
/// 1) In snapshot, not in history - occurs when a live object gets snapshotted and then has not been
///    modified since
/// 2) Not in snapshot, in history - occurs when a new object is created or a wrapped object is unwrapped
/// 3) In snapshot and in history - occurs when an object is snapshotted and further modified, the modification
///    can be wrapping or deleting.
///
/// Additionally, even among objects that satisfy the filtering criteria, it is possible that there
/// is a yet more recent version of the object within the checkpoint range, such as when the owner
/// of an object changes. The `LEFT JOIN` against the `objects_history` table handles this and
/// scenario 3. Note that the implementation applies the `LEFT JOIN` to each inner query in
/// conjunction with the `page`'s cursor and limit. If this was instead done once at the end, the
/// query would be drastically inefficient as we would be dealing with a large number of rows from
/// `objects_snapshot`, and potentially `objects_history` as the checkpoint range grows. Instead,
/// the `LEFT JOIN` and limit applied on the inner queries work in conjunction to make the final
/// query noticeably more efficient. The former serves as a filter, and the latter reduces the
/// number of rows that the database needs to work with.
///
/// However, not all queries require this `LEFT JOIN`, such as when no filtering criteria is
/// specified, or if the filter is a lookup at a specific `object_id` and `object_version`. This is
/// controlled by the `view` parameter. If the `view` parameter is set to `Consistent`, this filter
/// is applied, otherwise if the `view` parameter is set to `Historical`, this filter is not
/// applied.
///
/// Finally, the two queries are merged together with `UNION ALL`. We use `UNION ALL` instead of
/// `UNION`; the latter incurs significant overhead as it additionally de-duplicates records from
/// both sources. This dedupe is unnecessary, since we have the fragment `SELECT DISTINCT ON
/// (object_id) ... ORDER BY object_id, object_version DESC`. This is also redundant for the most
/// part, due to the invariant that the `objects_history` captures changes that occur after
/// `objects_snapshot`, but it's a safeguard to handle any possible overlap during snapshot
/// creation.
pub(crate) fn build_objects_query(
    view: View,
    range: AvailableRange,
    page: &Page<Cursor>,
    filter_fn: impl Fn(RawQuery) -> RawQuery,
    newer_criteria: impl Fn(RawQuery) -> RawQuery,
) -> RawQuery {
    // Subquery to be used in `LEFT JOIN` against the inner queries for more recent object versions
    let newer = newer_criteria(filter!(
        query!("SELECT object_id, object_version FROM objects_history"),
        format!(
            r#"checkpoint_sequence_number BETWEEN {} AND {}"#,
            range.first, range.last
        )
    ));

    let mut snapshot_objs_inner = query!("SELECT * FROM objects_snapshot");
    snapshot_objs_inner = filter_fn(snapshot_objs_inner);

    let mut snapshot_objs = match view {
        View::Consistent => {
            // The `LEFT JOIN` serves as a filter to remove objects that have a more recent version
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
        View::Historical => {
            // The cursor pagination logic refers to the table with the `candidates` alias
            query!(
                "SELECT candidates.* FROM ({}) candidates",
                snapshot_objs_inner
            )
        }
    };

    // Always apply cursor pagination and limit to constrain the number of rows returned, ensure
    // that the inner queries are in step, and to handle the scenario where a user provides more
    // `objectKeys` than allowed by the maximum page size.
    snapshot_objs = page.apply::<StoredHistoryObject>(snapshot_objs);

    // Similar to the snapshot query, construct the filtered inner query for the history table.
    let mut history_objs_inner = query!("SELECT * FROM objects_history");
    history_objs_inner = filter_fn(history_objs_inner);
    history_objs_inner = filter!(history_objs_inner, "object_status = 0");

    let mut history_objs = match view {
        View::Consistent => {
            // Additionally bound the inner `objects_history` query by the checkpoint range
            history_objs_inner = filter!(
                history_objs_inner,
                format!(
                    r#"checkpoint_sequence_number BETWEEN {} AND {}"#,
                    range.first, range.last
                )
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
        View::Historical => {
            // The cursor pagination logic refers to the table with the `candidates` alias
            query!(
                "SELECT candidates.* FROM ({}) candidates",
                history_objs_inner
            )
        }
    };

    // Always apply cursor pagination and limit to constrain the number of rows returned, ensure
    // that the inner queries are in step, and to handle the scenario where a user provides more
    // `objectKeys` than allowed by the maximum page size.
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
