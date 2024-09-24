// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::backfill::backfill_task::{BackfillTask, ProcessedResult};
use crate::database::ConnectionPool;
use async_trait::async_trait;
use diesel_async::RunQueryDsl;
use std::ops::RangeInclusive;

pub struct FullObjectsHistoryBackfill {}

#[async_trait]
impl BackfillTask<()> for FullObjectsHistoryBackfill {
    async fn query_db(_pool: ConnectionPool, _range: &RangeInclusive<usize>) {}

    async fn commit_db(pool: ConnectionPool, results: ProcessedResult<()>) {
        let mut conn = pool.get().await.unwrap();

        let query = format!(
            "INSERT INTO full_objects_history (object_id, object_version, serialized_object) \
             SELECT object_id, object_version, serialized_object FROM objects_history \
             WHERE checkpoint_sequence_number BETWEEN {} AND {} ON CONFLICT DO NOTHING",
            *results.range.start(),
            *results.range.end(),
        );

        // Execute the SQL query using Diesel's async connection
        diesel::sql_query(query).execute(&mut conn).await.unwrap();
    }
}
