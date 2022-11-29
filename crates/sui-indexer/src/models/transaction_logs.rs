// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::errors::IndexerError;
use crate::schema::transaction_logs;
use crate::schema::transaction_logs::dsl::*;
use diesel::prelude::*;

#[derive(Queryable, Debug, Identifiable)]
#[diesel(primary_key(id))]
pub struct TransactionLog {
    pub id: i32,
    pub next_cursor_tx_digest: Option<String>,
}

pub fn commit_transction_log(
    conn: &mut PgConnection,
    txn_digest: Option<String>,
) -> Result<usize, IndexerError> {
    diesel::update(transaction_logs::table)
        .set(next_cursor_tx_digest.eq(txn_digest.clone()))
        .execute(conn)
        .map_err(|e| {
            IndexerError::PostgresWriteError(format!(
                "Failed updating transaction log in PostgresDB with tx digest {:?} and error {:?}",
                txn_digest, e
            ))
        })
}

pub fn read_transaction_log(conn: &mut PgConnection) -> Result<TransactionLog, IndexerError> {
    transaction_logs
        .limit(1)
        .first::<TransactionLog>(conn)
        .map_err(|e| {
            IndexerError::PostgresReadError(format!(
                "Failed reading transaction log in PostgresDB with tx with error {:?}",
                e
            ))
        })
}
