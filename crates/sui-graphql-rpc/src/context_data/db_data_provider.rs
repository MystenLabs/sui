// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::{
    context_data::db_data_provider::diesel_macro::read_only_blocking,
    error::Error,
    types::{digest::Digest, epoch::Epoch},
};
use diesel::{
    r2d2::{self, ConnectionManager},
    sql_types::BigInt,
    ExpressionMethods, PgConnection, QueryDsl, RunQueryDsl,
};
use move_bytecode_utils::module_cache::SyncModuleCache;
use std::{env, str::FromStr, sync::Arc};
use sui_indexer::schema_v2::transactions;
use sui_indexer::{
    models_v2::{epoch::StoredEpochInfo, transactions::StoredTransaction},
    schema_v2::epochs,
};
use sui_json_rpc_types::SuiTransactionBlockResponse;
pub type PgConnectionPool = diesel::r2d2::Pool<ConnectionManager<PgConnection>>;
pub type PgPoolConnection = diesel::r2d2::PooledConnection<ConnectionManager<PgConnection>>;
use async_graphql::Result;

use super::module_resolver::PgModuleResolver;

pub(crate) mod diesel_macro {
    macro_rules! read_only_blocking {
        ($pool:expr, $query:expr) => {{
            let mut pg_pool_conn =
                crate::context_data::db_data_provider::get_pg_pool_connection($pool)?;
            let result = pg_pool_conn.build_transaction().read_only().run($query);

            match result {
                Ok(value) => Ok(Some(value)),
                Err(e) => match e {
                    diesel::result::Error::NotFound => Ok(None),
                    _ => Err(Error::Internal(e.to_string())),
                },
            }
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

    pub(crate) async fn fetch_epoch(
        &self,
        epoch_id: Option<u64>,
    ) -> Result<Option<StoredEpochInfo>> {
        let mut query = epochs::dsl::epochs.into_boxed();

        if let Some(e) = epoch_id {
            let bigint_e = i64::try_from(e)
                .map_err(|_| Error::Internal("Failed to convert epoch to i64".to_string()))?;
            query = query.filter(epochs::dsl::epoch.eq(bigint_e));
        } else {
            query = query.order(epochs::dsl::epoch.desc()).limit(1);
        }

        Ok(
            read_only_blocking!(&self.pool, |conn| { query.first::<StoredEpochInfo>(conn) })
                .map_err(Error::from)?,
        )
    }
}
