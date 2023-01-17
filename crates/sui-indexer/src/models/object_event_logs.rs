// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::errors::IndexerError;
use crate::schema::object_event_logs;
use crate::schema::object_event_logs::dsl::*;
use crate::PgPoolConnection;

use diesel::prelude::*;
use diesel::result::Error;

#[derive(Queryable, Debug, Identifiable)]
#[diesel(primary_key(id))]
pub struct ObjectEventLog {
    pub id: i32,
    pub next_cursor_tx_dig: Option<String>,
    pub next_cursor_event_seq: Option<i64>,
}

pub fn read_event_log(pg_pool_conn: &mut PgPoolConnection) -> Result<ObjectEventLog, IndexerError> {
    let event_log_read_result: Result<ObjectEventLog, Error> = pg_pool_conn
        .build_transaction()
        .read_only()
        .run::<_, Error, _>(|conn| object_event_logs.limit(1).first::<ObjectEventLog>(conn));

    event_log_read_result.map_err(|e| {
        IndexerError::PostgresReadError(format!(
            "Failed reading object event log in PostgresDB with error {:?}",
            e
        ))
    })
}

pub fn commit_event_log(
    pg_pool_conn: &mut PgPoolConnection,
    tx_digest: Option<String>,
    event_seq: Option<i64>,
) -> Result<usize, IndexerError> {
    let event_log_commit_result: Result<usize, Error> = pg_pool_conn
        .build_transaction()
        .read_write()
        .run::<_, Error, _>(|conn| {
            diesel::update(object_event_logs::table)
                .set((
                    next_cursor_tx_dig.eq(tx_digest.clone()),
                    next_cursor_event_seq.eq(event_seq),
                ))
                .execute(conn)
        });

    event_log_commit_result.map_err(|e|
        IndexerError::PostgresWriteError(format!(
            "Failed updating object event log in PostgresDB with tx seq {:?}, event seq {:?} and error {:?}",
            tx_digest.clone(), event_seq, e
        ))
    )
}
