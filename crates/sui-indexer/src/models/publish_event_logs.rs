// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::errors::IndexerError;
use crate::schema::publish_event_logs;
use crate::schema::publish_event_logs::dsl::*;
use crate::PgPoolConnection;

use diesel::prelude::*;
use diesel::result::Error;

#[derive(Queryable, Debug, Identifiable)]
#[diesel(primary_key(id))]
pub struct PublishEventLog {
    pub id: i32,
    pub next_cursor_tx_dig: Option<String>,
    pub next_cursor_event_seq: Option<i64>,
}

pub fn read_event_log(
    pg_pool_conn: &mut PgPoolConnection,
) -> Result<PublishEventLog, IndexerError> {
    let event_log_read_result: Result<PublishEventLog, Error> = pg_pool_conn
        .build_transaction()
        .read_only()
        .run::<_, Error, _>(|conn| publish_event_logs.limit(1).first::<PublishEventLog>(conn));

    event_log_read_result.map_err(|e| {
        IndexerError::PostgresReadError(format!(
            "Failed reading publish event log in PostgresDB with error {:?}",
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
            diesel::update(publish_event_logs::table)
                .set((
                    next_cursor_tx_dig.eq(tx_digest.clone()),
                    next_cursor_event_seq.eq(event_seq),
                ))
                .execute(conn)
        });

    event_log_commit_result.map_err(|e|
        IndexerError::PostgresWriteError(format!(
            "Failed updating publish event log in PostgresDB with tx seq {:?}, event seq {:?} and error {:?}",
            tx_digest.clone(), event_seq, e
        ))
    )
}
