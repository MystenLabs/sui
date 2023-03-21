// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::errors::IndexerError;
use crate::{get_pg_pool_connection, PgConnectionPool};
use diesel::pg::sql_types::Bytea;
use diesel::sql_types::Text;
use diesel::QueryableByName;
use diesel::RunQueryDsl;
use move_core_types::language_storage::ModuleId;
use move_core_types::resolver::ModuleResolver;
use sui_types::base_types::ObjectID;

pub struct IndexerModuleResolver {
    cp: PgConnectionPool,
}

impl IndexerModuleResolver {
    pub fn new(cp: PgConnectionPool) -> Self {
        Self { cp }
    }
}

const LATEST_MODULE_QUERY: &str = "SELECT (t2.module).data
FROM (SELECT UNNEST(data) AS module
      FROM (SELECT data FROM packages WHERE package_id = $1 ORDER BY version DESC FETCH FIRST 1 ROW ONLY) t1) t2
WHERE (module).name = $2;";

impl ModuleResolver for IndexerModuleResolver {
    type Error = IndexerError;

    fn get_module(&self, id: &ModuleId) -> Result<Option<Vec<u8>>, Self::Error> {
        #[derive(QueryableByName)]
        struct ModuleBytes {
            #[diesel(sql_type = Bytea)]
            data: Vec<u8>,
        }

        let package_id = ObjectID::from(*id.address()).to_string();
        let module_name = id.name().to_string();

        let mut pg_pool_conn = get_pg_pool_connection(&self.cp)?;
        let module_bytes = pg_pool_conn
            .build_transaction()
            .read_only()
            .run(|conn| {
                diesel::sql_query(LATEST_MODULE_QUERY)
                    .bind::<Text, _>(package_id)
                    .bind::<Text, _>(module_name)
                    .get_result::<ModuleBytes>(conn)
            })
            .map_err(|e| {
                println!("{e}");
                IndexerError::PostgresReadError(e.to_string())
            })?;

        Ok(Some(module_bytes.data))
    }
}
