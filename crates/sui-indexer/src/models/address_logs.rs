// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::errors::IndexerError;
use crate::schema::address_logs;
use crate::schema::address_logs::dsl::*;
use diesel::prelude::*;

#[derive(Queryable, Debug, Identifiable)]
#[diesel(primary_key(last_processed_id))]
pub struct AddressLog {
    pub last_processed_id: i64,
}

pub fn read_address_log(conn: &mut PgConnection) -> Result<AddressLog, IndexerError> {
    address_logs
        .limit(1)
        .first::<AddressLog>(conn)
        .map_err(|e| {
            IndexerError::PostgresReadError(format!(
                "Failed reading address log with error: {:?}",
                e
            ))
        })
}

pub fn commit_address_log(conn: &mut PgConnection, id: i64) -> Result<usize, IndexerError> {
    diesel::update(address_logs::table)
        .set(last_processed_id.eq(id))
        .execute(conn)
        .map_err(|e| {
            IndexerError::PostgresWriteError(format!(
                "Failed to commit address log with id: {:?} and error: {:?}",
                id, e
            ))
        })
}
