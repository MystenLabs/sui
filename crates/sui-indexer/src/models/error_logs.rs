// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::errors::IndexerError;
use crate::schema::error_logs;
use crate::PgPoolConnection;

use chrono::{NaiveDateTime, Utc};
use diesel::prelude::*;
use diesel::result::Error;

// NOTE: this is for the errors table in PG
#[derive(Queryable, Debug)]
pub struct ErrorLog {
    pub id: i64,
    pub error_type: String,
    pub error: String,
    pub error_time: NaiveDateTime,
}

#[derive(Debug, Insertable)]
#[diesel(table_name = error_logs)]
pub struct NewErrorLog {
    pub error_type: String,
    pub error: String,
    pub error_time: NaiveDateTime,
}

pub fn err_to_error_log(error: IndexerError) -> NewErrorLog {
    NewErrorLog {
        error_type: error.name(),
        error: error.to_string(),
        error_time: Utc::now().naive_utc(),
    }
}

pub fn commit_error_logs(
    pg_pool_conn: &mut PgPoolConnection,
    new_error_logs: Vec<NewErrorLog>,
) -> Result<usize, IndexerError> {
    let error_commit_result: Result<usize, Error> = pg_pool_conn
        .build_transaction()
        .read_write()
        .run::<_, Error, _>(|conn| {
        diesel::insert_into(error_logs::table)
            .values(&new_error_logs)
            .execute(conn)
    });
    error_commit_result.map_err(|e| {
        IndexerError::PostgresWriteError(format!(
            "Failed writing error logs to PostgresDB with error logs  {:?} and error: {:?}",
            new_error_logs, e
        ))
    })
}
