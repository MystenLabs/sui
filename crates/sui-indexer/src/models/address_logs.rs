// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::errors::IndexerError;
use crate::schema::address_logs;
use crate::schema::address_logs::dsl::*;
use crate::PgPoolConnection;
use diesel::prelude::*;
use diesel::result::Error;

#[derive(Queryable, Debug, Identifiable)]
#[diesel(primary_key(last_processed_id))]
pub struct AddressLog {
    pub last_processed_id: i64,
}

pub fn read_address_log(pg_pool_conn: &mut PgPoolConnection) -> Result<AddressLog, IndexerError> {
    let addr_log_read_result: Result<AddressLog, Error> = pg_pool_conn
        .build_transaction()
        .read_only()
        .run::<_, Error, _>(|conn| address_logs.limit(1).first::<AddressLog>(conn));

    addr_log_read_result.map_err(|e| {
        IndexerError::PostgresReadError(format!("Failed reading address log with error: {:?}", e))
    })
}

pub fn commit_address_log(
    pg_pool_conn: &mut PgPoolConnection,
    id: i64,
) -> Result<usize, IndexerError> {
    let addr_log_commit_result: Result<usize, Error> = pg_pool_conn
        .build_transaction()
        .read_write()
        .run::<_, Error, _>(|conn| {
            diesel::update(address_logs::table)
                .set(last_processed_id.eq(id))
                .execute(conn)
        });

    addr_log_commit_result.map_err(|e| {
        IndexerError::PostgresWriteError(format!(
            "Failed to commit address log with id: {:?} and error: {:?}",
            id, e
        ))
    })
}
