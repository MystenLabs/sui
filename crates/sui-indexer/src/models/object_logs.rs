// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::errors::IndexerError;
use crate::schema::object_logs;
use crate::schema::object_logs::dsl::*;
use crate::PgPoolConnection;

use diesel::prelude::*;
use diesel::result::Error;

#[derive(Queryable, Debug, Identifiable)]
#[diesel(primary_key(last_processed_id))]
pub struct ObjectLog {
    pub last_processed_id: i64,
}

pub fn read_object_log(pg_pool_conn: &mut PgPoolConnection) -> Result<ObjectLog, IndexerError> {
    // NOTE: always read one row, as object logs only have one row
    let obj_log_read_result: Result<ObjectLog, Error> = pg_pool_conn
        .build_transaction()
        .read_only()
        .run::<_, Error, _>(|conn| object_logs.limit(1).first::<ObjectLog>(conn));

    obj_log_read_result.map_err(|e| {
        IndexerError::PostgresReadError(format!("Failed reading object log with error {:?}", e))
    })
}

pub fn commit_object_log(
    pg_pool_conn: &mut PgPoolConnection,
    id: i64,
) -> Result<usize, IndexerError> {
    let event_log_commit_result: Result<usize, Error> = pg_pool_conn
        .build_transaction()
        .read_write()
        .run::<_, Error, _>(|conn| {
            diesel::update(object_logs::table)
                .set(last_processed_id.eq(id))
                .execute(conn)
        });

    event_log_commit_result.map_err(|e| {
        IndexerError::PostgresWriteError(format!(
            "Failed to commit object log with id: {:?} and error: {:?}",
            id, e
        ))
    })
}
