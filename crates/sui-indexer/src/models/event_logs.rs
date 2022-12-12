// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::errors::IndexerError;
use crate::schema::event_logs;
use crate::schema::event_logs::dsl::*;
use crate::PgPoolConnection;

use diesel::prelude::*;
use diesel::result::Error;

#[derive(Queryable, Debug, Identifiable)]
#[diesel(primary_key(id))]
pub struct EventLog {
    pub id: i32,
    pub next_cursor_tx_seq: Option<i64>,
    pub next_cursor_event_seq: Option<i64>,
}

pub fn read_event_log(pg_pool_conn: &mut PgPoolConnection) -> Result<EventLog, IndexerError> {
    // NOTE: always read one row, as event logs only have one row
    let event_log_read_result: Result<EventLog, Error> = pg_pool_conn
        .build_transaction()
        .read_only()
        .run::<_, Error, _>(|conn| event_logs.limit(1).first::<EventLog>(conn));

    event_log_read_result.map_err(|e| {
        IndexerError::PostgresReadError(format!(
            "Failed reading event log in PostgresDB with error {:?}",
            e
        ))
    })
}

pub fn commit_event_log(
    pg_pool_conn: &mut PgPoolConnection,
    tx_seq: Option<i64>,
    event_seq: Option<i64>,
) -> Result<usize, IndexerError> {
    let event_log_commit_result: Result<usize, Error> = pg_pool_conn
        .build_transaction()
        .read_write()
        .run::<_, Error, _>(|conn| {
            diesel::update(event_logs::table)
                .set((
                    next_cursor_tx_seq.eq(tx_seq),
                    next_cursor_event_seq.eq(event_seq),
                ))
                .execute(conn)
        });

    event_log_commit_result.map_err(|e|
        IndexerError::PostgresWriteError(format!(
            "Failed updating event log in PostgresDB with tx seq {:?}, event seq {:?} and error {:?}",
            tx_seq, event_seq, e
        ))
    )
}
