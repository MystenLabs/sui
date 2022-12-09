// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::errors::IndexerError;
use crate::schema::object_logs;
use crate::schema::object_logs::dsl::*;
use diesel::prelude::*;

#[derive(Queryable, Debug, Identifiable)]
#[diesel(primary_key(last_processed_id))]
pub struct ObjectLog {
    pub last_processed_id: i64,
}

pub fn read_object_log(conn: &mut PgConnection) -> Result<ObjectLog, IndexerError> {
    object_logs.limit(1).first::<ObjectLog>(conn).map_err(|e| {
        IndexerError::PostgresReadError(format!("Failed reading object log with error {:?}", e))
    })
}

pub fn commit_object_log(conn: &mut PgConnection, id: i64) -> Result<usize, IndexerError> {
    diesel::update(object_logs::table)
        .set(last_processed_id.eq(id))
        .execute(conn)
        .map_err(|e| {
            IndexerError::PostgresWriteError(format!(
                "Failed to commit object log with id: {:?} and error: {:?}",
                id, e
            ))
        })
}
