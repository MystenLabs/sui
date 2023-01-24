// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::errors::IndexerError;
use crate::models::transactions::Transaction;
use crate::schema::addresses;
use crate::schema::addresses::dsl::*;
use crate::PgPoolConnection;

use chrono::NaiveDateTime;
use diesel::prelude::*;
use diesel::result::Error;

#[derive(Queryable, Debug)]
#[diesel(primary_key(account_address))]
pub struct Addresses {
    pub id: i64,
    pub account_address: String,
    pub first_appearance_tx: String,
    pub first_appearance_time: Option<NaiveDateTime>,
}

#[derive(Debug, Insertable)]
#[diesel(table_name = addresses)]
pub struct NewAddress {
    pub account_address: String,
    pub first_appearance_tx: String,
    pub first_appearance_time: Option<NaiveDateTime>,
}

pub fn commit_addresses(
    pg_pool_conn: &mut PgPoolConnection,
    new_addr_vec: Vec<NewAddress>,
) -> Result<usize, IndexerError> {
    let addr_commit_result: Result<usize, Error> = pg_pool_conn
        .build_transaction()
        .read_write()
        .run::<_, Error, _>(|conn| {
        diesel::insert_into(addresses::table)
            .values(&new_addr_vec)
            .on_conflict(account_address)
            .do_nothing()
            .execute(conn)
    });

    addr_commit_result.map_err(|e| {
        IndexerError::PostgresWriteError(format!(
            "Failed writing addresses to Postgres DB with addresses {:?} and error: {:?}",
            new_addr_vec, e
        ))
    })
}

pub fn transaction_to_address(txn: Transaction) -> NewAddress {
    NewAddress {
        account_address: txn.sender,
        first_appearance_tx: txn.transaction_digest,
        first_appearance_time: txn.transaction_time,
    }
}
