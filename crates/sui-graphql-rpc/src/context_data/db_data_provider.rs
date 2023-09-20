// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::{
    context_data::db_data_provider::diesel_marco::read_only_blocking, error::Error,
    types::digest::Digest,
};
use diesel::{
    r2d2::{self, ConnectionManager},
    ExpressionMethods, PgConnection, QueryDsl, RunQueryDsl,
};
use move_bytecode_utils::module_cache::SyncModuleCache;
use std::{env, str::FromStr, sync::Arc};
use sui_indexer::models_v2::transactions::StoredTransaction;
use sui_indexer::schema_v2::transactions;
use sui_json_rpc_types::SuiTransactionBlockResponse;
pub type PgConnectionPool = diesel::r2d2::Pool<ConnectionManager<PgConnection>>;
pub type PgPoolConnection = diesel::r2d2::PooledConnection<ConnectionManager<PgConnection>>;
use async_graphql::Result;

use super::module_resolver::PgModuleResolver;

pub(crate) mod diesel_marco {
    macro_rules! read_only_blocking {
        ($pool:expr, $query:expr) => {{
            let mut pg_pool_conn =
                crate::context_data::db_data_provider::get_pg_pool_connection($pool)?;
            pg_pool_conn
                .build_transaction()
                .read_only()
                .run($query)
                .map_err(|e| Error::Internal(e.to_string()))
        }};
    }
    pub(crate) use read_only_blocking;
}

pub fn get_pg_pool_connection(pool: &PgConnectionPool) -> Result<PgPoolConnection, Error> {
    pool.get().map_err(|e| {
        Error::Internal(format!(
            "Failed to get connection from PG connection pool with error: {:?}",
            e
        ))
    })
}

pub fn establish_connection_pool() -> PgConnectionPool {
    let database_url = env::var("PG_DB_URL").expect("PG_DB_URL must be set");
    let manager = ConnectionManager::<PgConnection>::new(database_url);
    r2d2::Pool::builder()
        .build(manager)
        .expect("Failed to create pool.")
}

pub(crate) struct PgManager {
    pub pool: PgConnectionPool,
    pub module_cache: Arc<SyncModuleCache<PgModuleResolver>>,
}

impl PgManager {
    pub(crate) fn new() -> Self {
        let pool = establish_connection_pool();
        Self {
            pool: pool.clone(),
            module_cache: Arc::new(SyncModuleCache::new(PgModuleResolver::new(pool))),
        }
    }

    pub(crate) async fn fetch_tx(&self, digest: String) -> Result<SuiTransactionBlockResponse> {
        let digest = Digest::from_str(&digest)?;
        let result: StoredTransaction = read_only_blocking!(&self.pool, |conn| {
            transactions::dsl::transactions
                .filter(transactions::dsl::transaction_digest.eq(digest.into_array().to_vec()))
                .first::<StoredTransaction>(conn)
        })?;
        Ok(result
            .try_into_sui_transaction_block_response(&self.module_cache)
            .map_err(Error::from)?)
    }
}
