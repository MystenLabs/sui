// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::errors::IndexerError;
use crate::schema::checkpoint_logs;
use crate::schema::checkpoint_logs::dsl::*;
use crate::PgPoolConnection;

use diesel::prelude::*;
use diesel::result::Error;

#[derive(Queryable, Debug, Identifiable)]
#[diesel(primary_key(next_cursor_sequence_number))]
pub struct CheckpointLog {
    pub next_cursor_sequence_number: i64,
}

pub fn read_checkpoint_log(
    pg_pool_conn: &mut PgPoolConnection,
) -> Result<CheckpointLog, IndexerError> {
    let checkpoint_log_read_result: Result<CheckpointLog, Error> = pg_pool_conn
        .build_transaction()
        .read_only()
        .run::<_, Error, _>(|conn| checkpoint_logs.limit(1).first::<CheckpointLog>(conn));

    checkpoint_log_read_result.map_err(|e| {
        IndexerError::PostgresReadError(format!(
            "Failed reading checkpoint log in PostgresDB with error {:?}",
            e
        ))
    })
}

pub fn commit_checkpoint_log(
    pg_pool_conn: &mut PgPoolConnection,
    sequence_number: i64,
) -> Result<usize, IndexerError> {
    let checkpoint_log_commit_result: Result<usize, Error> = pg_pool_conn
        .build_transaction()
        .read_write()
        .run::<_, Error, _>(|conn| {
            diesel::update(checkpoint_logs::table)
                .set(next_cursor_sequence_number.eq(sequence_number))
                .execute(conn)
        });

    checkpoint_log_commit_result.map_err(|e| {
        IndexerError::PostgresWriteError(format!(
            "Failed updating checkpoint log in PostgresDB with sequence number {:?} and error {:?}",
            sequence_number, e
        ))
    })
}
