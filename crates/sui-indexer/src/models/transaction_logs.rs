// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::errors::IndexerError;
use crate::schema::transaction_logs;
use crate::schema::transaction_logs::dsl::*;
use crate::PgPoolConnection;

use diesel::prelude::*;
use diesel::result::Error;

#[derive(Queryable, Debug, Identifiable)]
#[diesel(primary_key(id))]
pub struct TransactionLog {
    pub id: i32,
    pub next_cursor_tx_digest: Option<String>,
}

pub fn read_transaction_log(
    pg_pool_conn: &mut PgPoolConnection,
) -> Result<TransactionLog, IndexerError> {
    // NOTE: always read one row, as txn logs only have one row
    let txn_log_read_result: Result<TransactionLog, Error> = pg_pool_conn
        .build_transaction()
        .read_only()
        .run::<_, Error, _>(|conn| transaction_logs.limit(1).first::<TransactionLog>(conn));

    txn_log_read_result.map_err(|e| {
        IndexerError::PostgresReadError(format!(
            "Failed reading transaction log in PostgresDB with tx with error {:?}",
            e
        ))
    })
}

pub fn commit_transaction_log(
    pg_pool_conn: &mut PgPoolConnection,
    txn_digest: Option<String>,
) -> Result<usize, IndexerError> {
    let txn_log_commit_result: Result<usize, Error> = pg_pool_conn
        .build_transaction()
        .read_write()
        .run::<_, Error, _>(|conn| {
            diesel::update(transaction_logs::table)
                .set(next_cursor_tx_digest.eq(txn_digest.clone()))
                .execute(conn)
        });

    txn_log_commit_result.map_err(|e| {
        IndexerError::PostgresWriteError(format!(
            "Failed updating transaction log in PostgresDB with tx digest {:?} and error {:?}",
            txn_digest, e
        ))
    })
}
