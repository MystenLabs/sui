// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::errors::IndexerError;
use crate::schema::package_logs;
use crate::schema::package_logs::dsl::*;
use diesel::prelude::*;

#[derive(Queryable, Debug, Identifiable)]
#[diesel(primary_key(last_processed_id))]
pub struct PackageLog {
    pub last_processed_id: i64,
}

pub fn read_package_log(conn: &mut PgConnection) -> Result<PackageLog, IndexerError> {
    package_logs
        .limit(1)
        .first::<PackageLog>(conn)
        .map_err(|e| {
            IndexerError::PostgresReadError(format!(
                "Failed reading package log with error {:?}",
                e
            ))
        })
}

pub fn commit_package_log(conn: &mut PgConnection, id: i64) -> Result<usize, IndexerError> {
    diesel::update(package_logs::table)
        .set(last_processed_id.eq(id))
        .execute(conn)
        .map_err(|e| {
            IndexerError::PostgresWriteError(format!(
                "Failed to commit package log with id {:?} and error {:?}",
                id, e
            ))
        })
}
