// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::Result;
use backoff::retry;
use backoff::ExponentialBackoff;
use diesel::pg::PgConnection;
use diesel::prelude::*;
use diesel::r2d2::{ConnectionManager, Pool, PooledConnection};
use jsonrpsee::http_client::{HeaderMap, HeaderValue, HttpClientBuilder};
use prometheus::Registry;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::Arc;
use sui_json_rpc::{JsonRpcServerBuilder, ServerHandle, CLIENT_SDK_TYPE_HEADER};
use sui_sdk::{SuiClient, SuiClientBuilder};
use tracing::{info, warn};

pub mod apis;
pub mod errors;
pub mod metrics;
pub mod models;
pub mod schema;
pub mod utils;

pub type PgConnectionPool = Pool<ConnectionManager<PgConnection>>;
pub type PgPoolConnection = PooledConnection<ConnectionManager<PgConnection>>;

use crate::apis::{
    CoinReadApi, EventReadApi, GovernanceReadApi, ReadApi, ThresholdBlsApi, TransactionBuilderApi,
    WriteApi,
};
use errors::IndexerError;

// TODO: placeholder, read from env or config file.
pub const FAKE_PKG_VERSION: &str = "0.0.0";

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

pub async fn build_json_rpc_server(
    prometheus_registry: &Registry,
    pg_connection_pool: Arc<PgConnectionPool>,
    fullnode_url: &str,
) -> Result<ServerHandle, IndexerError> {
    let mut builder = JsonRpcServerBuilder::new(FAKE_PKG_VERSION, prometheus_registry);

    let mut headers = HeaderMap::new();
    headers.insert(CLIENT_SDK_TYPE_HEADER, HeaderValue::from_static("indexer"));

    let http_client = HttpClientBuilder::default()
        .max_request_body_size(2 << 30)
        .max_concurrent_requests(usize::MAX)
        .set_headers(headers.clone())
        .build(fullnode_url)
        .map_err(|e| IndexerError::RpcClientInitError(e.to_string()))?;

    builder.register_module(ReadApi::new(pg_connection_pool, http_client.clone()))?;
    builder.register_module(CoinReadApi::new(http_client.clone()))?;
    builder.register_module(ThresholdBlsApi::new(http_client.clone()))?;
    builder.register_module(TransactionBuilderApi::new(http_client.clone()))?;
    builder.register_module(GovernanceReadApi::new(http_client.clone()))?;
    builder.register_module(EventReadApi::new(http_client.clone()))?;
    builder.register_module(WriteApi::new(http_client))?;
    // TODO: placeholder, read from env or config file.
    let default_socket_addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 3030);
    Ok(builder.start(default_socket_addr).await?)
}
