// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::errors::IndexerError;
use crate::schema::event_logs;
use crate::schema::event_logs::dsl::*;
use diesel::prelude::*;

#[derive(Queryable, Debug, Identifiable)]
#[diesel(primary_key(id))]
pub struct EventLog {
    pub id: i32,
    pub next_cursor_tx_seq: Option<i64>,
    pub next_cursor_event_seq: Option<i64>,
}

pub fn commit_event_log(
    conn: &mut PgConnection,
    tx_seq: Option<i64>,
    event_seq: Option<i64>,
) -> Result<usize, IndexerError> {
    diesel::update(event_logs::table).set((next_cursor_tx_seq.eq(tx_seq), next_cursor_event_seq.eq(event_seq))).execute(conn).map_err(|e|
        IndexerError::PostgresWriteError(format!(
            "Failed updating event log in PostgresDB with tx seq {:?}, event seq {:?} and error {:?}",
            tx_seq, event_seq, e
        ))
    )
}

pub fn read_event_log(conn: &mut PgConnection) -> Result<EventLog, IndexerError> {
    // NOTE: always read one row, as event logs only have one row
    event_logs.limit(1).first::<EventLog>(conn).map_err(|e| {
        IndexerError::PostgresReadError(format!(
            "Failed reading event log in PostgresDB with error {:?}",
            e
        ))
    })
}
