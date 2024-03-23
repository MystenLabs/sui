// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/// Load package from the DB.
/// A DB URL with proper privileges is required.
/// Follows the syntax of `sui-tool dump-packages`.
use crate::{errors::PackageAnalyzerError, DEFAULT_CAPACITY};
use diesel::{
    r2d2::{ConnectionManager, Pool},
    PgConnection, RunQueryDsl,
};
use std::time::Duration;
use sui_indexer::{models::packages::StoredPackage, schema::packages};
use sui_types::{base_types::ObjectID, move_package::MovePackage};

/// Query packages from the DB.
pub fn query_packages(db_url: &str) -> Result<Vec<MovePackage>, PackageAnalyzerError> {
    let conn = ConnectionManager::<PgConnection>::new(db_url);
    let pool = Pool::builder()
        .max_size(1)
        .connection_timeout(Duration::from_secs(30))
        .build(conn)
        .map_err(|e| {
            PackageAnalyzerError::DBReadError(format!("error connecting to DB {e}").to_string())
        })?;
    let mut conn = pool.get().map_err(|e| {
        PackageAnalyzerError::DBReadError(format!("Failed to get connection: {e}").to_string())
    })?;
    let stored_packages = packages::dsl::packages
        .load::<StoredPackage>(&mut conn)
        .map_err(|e| {
            PackageAnalyzerError::DBReadError(
                format!("error reading packages from DB {e}").to_string(),
            )
        })?;
    let mut packages = Vec::with_capacity(DEFAULT_CAPACITY);
    for stored_package in stored_packages {
        let package =
            bcs::from_bytes::<MovePackage>(&stored_package.move_package).map_err(|e| {
                PackageAnalyzerError::DBReadError(format!(
                    "Cannot deserialize package with id {:?}: {:?}",
                    ObjectID::from_bytes(&stored_package.package_id),
                    e,
                ))
            })?;
        packages.push(package);
    }
    Ok(packages)
}
