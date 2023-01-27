// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::errors::IndexerError;
use crate::schema::transaction_stats;
use crate::schema::transaction_stats::dsl::*;
use crate::PgPoolConnection;

use chrono::NaiveDateTime;
use diesel::prelude::*;
use diesel::result::Error;

#[derive(Queryable, Debug)]
#[diesel(primary_key(id))]
pub struct TransactionStats {
    pub id: i64,
    pub computation_time: NaiveDateTime,
    pub start_txn_time: NaiveDateTime,
    pub end_txn_time: NaiveDateTime,
    pub tps: f32,
}

#[derive(Insertable, Debug)]
#[diesel(table_name = transaction_stats)]
pub struct NewTransactionStats {
    pub computation_time: NaiveDateTime,
    pub start_txn_time: NaiveDateTime,
    pub end_txn_time: NaiveDateTime,
    pub tps: f32,
}

pub fn commit_transaction_stats(
    pg_pool_conn: &mut PgPoolConnection,
    new_tx_stats_vec: Vec<NewTransactionStats>,
) -> Result<usize, IndexerError> {
    let tx_stats_commit_result: Result<usize, Error> = pg_pool_conn
        .build_transaction()
        .read_write()
        .run::<_, Error, _>(|conn| {
            diesel::insert_into(transaction_stats::table)
                .values(&new_tx_stats_vec)
                .on_conflict(id)
                .do_nothing()
                .execute(conn)
        });

    tx_stats_commit_result.map_err(|e| {
        IndexerError::PostgresWriteError(format!(
            "Failed writing transaction stats to Postgres DB with transaction stats {:?} and error: {:?}",
            new_tx_stats_vec, e
        ))
    })
}
