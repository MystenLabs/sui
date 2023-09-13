// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use diesel::pg::sql_types::Bytea;
use diesel::sql_types::Text;
use diesel::QueryableByName;
use diesel::RunQueryDsl;
use move_core_types::language_storage::ModuleId;
use move_core_types::resolver::ModuleResolver;

use sui_types::base_types::ObjectID;

use crate::errors::{Context, IndexerError};
use crate::store::diesel_marco::read_only_blocking;
use crate::PgConnectionPool;

pub struct IndexerModuleResolverV2 {
    cp: PgConnectionPool,
}

impl IndexerModuleResolverV2 {
    // TODO remove this after integration is done
    #[allow(dead_code)]
    pub fn new(cp: PgConnectionPool) -> Self {
        Self { cp }
    }
}

const LATEST_MODULE_QUERY: &str = "SELECT (t2.module).data
FROM (SELECT UNNEST(modules) AS module
      FROM (SELECT modules FROM packages WHERE package_id = $1 LIMIT 1) t1) t2
WHERE (module).name = $2;";

impl ModuleResolver for IndexerModuleResolverV2 {
    type Error = IndexerError;

    fn get_module(&self, id: &ModuleId) -> Result<Option<Vec<u8>>, Self::Error> {
        #[derive(Debug, QueryableByName)]
        struct ModuleBytes {
            #[diesel(sql_type = Bytea)]
            data: Vec<u8>,
        }
        let package_id = ObjectID::from(*id.address()).to_vec();
        let module_name = id.name().to_string();

        let module_bytes: ModuleBytes = read_only_blocking!(&self.cp, |conn| {
            diesel::sql_query(LATEST_MODULE_QUERY)
                .bind::<Bytea, _>(package_id)
                .bind::<Text, _>(module_name)
                .get_result::<ModuleBytes>(conn)
        })
        .context("Error reading module.")?;

        Ok(Some(module_bytes.data))
    }
}
