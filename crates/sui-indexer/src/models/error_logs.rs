// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::errors::IndexerError;
use crate::schema::error_logs;
use crate::PgPoolConnection;

use chrono::{NaiveDateTime, Utc};
use diesel::prelude::*;

// NOTE: this is for the errors table in PG
#[derive(Queryable, Insertable, Debug)]
#[diesel(table_name = error_logs)]
pub struct ErrorLog {
    pub id: Option<i64>,
    pub error_type: String,
    pub error: String,
    pub error_time: NaiveDateTime,
}

impl From<IndexerError> for ErrorLog {
    fn from(error: IndexerError) -> Self {
        ErrorLog {
            id: None,
            error_type: error.name(),
            error: error.to_string(),
            error_time: Utc::now().naive_utc(),
        }
    }
}

pub fn commit_error_logs(
    pg_pool_conn: &mut PgPoolConnection,
    new_error_logs: Vec<ErrorLog>,
) -> Result<usize, IndexerError> {
    pg_pool_conn
        .build_transaction()
        .read_write()
        .run(|conn| {
            diesel::insert_into(error_logs::table)
                .values(&new_error_logs)
                .execute(conn)
        })
        .map_err(|e| {
            IndexerError::PostgresWriteError(format!(
                "Failed writing error logs to PostgresDB with error logs  {:?} and error: {:?}",
                new_error_logs, e
            ))
        })
}
