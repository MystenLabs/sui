// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use diesel::ExpressionMethods;
use diesel::QueryDsl;
use diesel::RunQueryDsl;
use move_core_types::language_storage::ModuleId;
use move_core_types::resolver::ModuleResolver;
use sui_indexer::schema_v2::packages;

use sui_indexer::models_v2::packages::StoredPackage;
use sui_sdk::types::base_types::ObjectID;
use sui_sdk::types::move_package::MovePackage;

use super::db_data_provider::diesel_macro::read_only_blocking;
use super::db_data_provider::PgConnectionPool;

use crate::error::Error;

pub(crate) struct PgModuleResolver {
    pub pool: PgConnectionPool,
}

impl PgModuleResolver {
    pub fn new(pool: PgConnectionPool) -> Self {
        Self { pool }
    }
}

// Basically lifted from sui-indexer/src/store/module_resolver_v2.rs
impl ModuleResolver for PgModuleResolver {
    type Error = Error;

    fn get_module(&self, id: &ModuleId) -> Result<Option<Vec<u8>>, Self::Error> {
        let package_id = ObjectID::from(*id.address()).to_vec();
        let module_name = id.name().to_string();

        // Note: this implementation is potentially vulnerable to package upgrade race conditions
        // for framework packages because they reuse the same package IDs.
        let stored_package: StoredPackage = read_only_blocking!(&self.pool, |conn| {
            packages::dsl::packages
                .filter(packages::dsl::package_id.eq(package_id))
                .first::<StoredPackage>(conn)
        })?;

        let move_package =
            bcs::from_bytes::<MovePackage>(&stored_package.move_package).map_err(|e| {
                Error::Internal(format!("Error deserializing move package. Error: {}", e))
            })?;

        Ok(move_package
            .serialized_module_map()
            .get(&module_name)
            .cloned())
    }
}
