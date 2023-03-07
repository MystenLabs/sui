// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;
use sui_sdk::{SuiClient, SuiClientBuilder};

use backoff::retry;
use backoff::ExponentialBackoff;
use diesel::pg::PgConnection;
use diesel::prelude::*;
use diesel::r2d2::{ConnectionManager, Pool, PooledConnection};
use tracing::{info, warn};

pub mod errors;
pub mod metrics;
pub mod models;
pub mod schema;
pub mod utils;

pub type PgConnectionPool = Pool<ConnectionManager<PgConnection>>;
pub type PgPoolConnection = PooledConnection<ConnectionManager<PgConnection>>;

use errors::IndexerError;

pub async fn new_rpc_client(http_url: String) -> Result<SuiClient, IndexerError> {
    info!("Getting new RPC client...");
    SuiClientBuilder::default()
        .build(http_url)
        .await
        .map_err(|e| {
            warn!("Failed to get new RPC client with error: {:?}", e);
            IndexerError::RpcClientInitError(format!(
                "Failed to initialize fullnode RPC client with error: {:?}",
                e
            ))
        })
}

pub fn establish_connection(db_url: String) -> PgConnection {
    PgConnection::establish(&db_url).unwrap_or_else(|_| panic!("Error connecting to {}", db_url))
}

pub async fn new_pg_connection_pool(db_url: String) -> Result<Arc<PgConnectionPool>, IndexerError> {
    let manager = ConnectionManager::<PgConnection>::new(db_url);
    // default connection pool max size is 10
    let pool = Pool::builder().build(manager).map_err(|e| {
        IndexerError::PgConnectionPoolInitError(format!(
            "Failed to initialize connection pool with error: {:?}",
            e
        ))
    })?;
    Ok(Arc::new(pool))
}

pub fn get_pg_pool_connection(
    pool: Arc<PgConnectionPool>,
) -> Result<PgPoolConnection, IndexerError> {
    retry(ExponentialBackoff::default(), || {
        let pool_conn = pool.get()?;
        Ok(pool_conn)
    })
    .map_err(|e| {
        IndexerError::PgPoolConnectionError(format!(
            "Failed to get pool connection from PG connection pool with error: {:?}",
            e
        ))
    })
}
