// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::{
    context_data::db_data_provider::diesel_marco::read_only_blocking, error::Error,
    types::digest::Digest,
};
use diesel::{
    r2d2::{self, ConnectionManager},
    sql_types::{BigInt, Bytea},
    PgConnection, QueryableByName, RunQueryDsl,
};
use fastcrypto::encoding::{Base58, Encoding};
use std::{env, str::FromStr};
pub type PgConnectionPool = diesel::r2d2::Pool<ConnectionManager<PgConnection>>;
pub type PgPoolConnection = diesel::r2d2::PooledConnection<ConnectionManager<PgConnection>>;
use async_graphql::{Error as GQLError, Result};
// TODO: use schema from indexerV2

mod diesel_marco {
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

    macro_rules! transactional_blocking {
        ($pool:expr, $query:expr) => {{
            let mut pg_pool_conn =
                crate::context_data::db_data_provider::get_pg_pool_connection($pool)?;
            pg_pool_conn
                .build_transaction()
                .serializable()
                .read_write()
                .run($query)
                .map_err(|e| Error::Internal(e.to_string()))
        }};
    }
    pub(crate) use read_only_blocking;
    pub(crate) use transactional_blocking;
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

const TRANSACTION_QUERY: &str = "SELECT * FROM transactions WHERE transaction_digest=$1";
#[derive(QueryableByName)]
pub(crate) struct PgTransaction {
    #[sql_type = "BigInt"]
    pub tx_sequence_number: i64,
    #[sql_type = "Bytea"]
    pub transaction_digest: Vec<u8>,
    #[sql_type = "BigInt"]
    pub checkpoint_sequence_number: i64,
}

pub(crate) struct PgManager {
    pool: PgConnectionPool,
}

impl PgManager {
    pub(crate) fn new() -> Self {
        Self {
            pool: establish_connection_pool(),
        }
    }

    pub(crate) async fn fetch_tx(&self, digest: String) -> Result<PgTransaction> {
        let digest =
            Digest::from_str(&digest).map_err(|e| Error::Internal("whatever".to_string()))?;
        let result = read_only_blocking!(&self.pool, |conn| {
            diesel::sql_query(TRANSACTION_QUERY)
                .bind::<Bytea, _>(digest.into_array().to_vec())
                .get_result::<PgTransaction>(conn)
        })?;
        Ok(result)
    }
}
