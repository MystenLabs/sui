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
use sui_json_rpc::{JsonRpcServerBuilder, ServerHandle, CLIENT_SDK_TYPE_HEADER};
use sui_sdk::{SuiClient, SuiClientBuilder};
use tracing::{info, warn};

pub mod apis;
pub mod errors;
mod handlers;
pub mod metrics;
pub mod models;
pub mod processors;
pub mod schema;
pub mod store;
pub mod utils;

pub type PgConnectionPool = Pool<ConnectionManager<PgConnection>>;
pub type PgPoolConnection = PooledConnection<ConnectionManager<PgConnection>>;

use crate::apis::{
    CoinReadApi, EventReadApi, GovernanceReadApi, ReadApi, TransactionBuilderApi, WriteApi,
};
use crate::handlers::checkpoint_handler::CheckpointHandler;
use crate::store::IndexerStore;
use errors::IndexerError;
use mysten_metrics::spawn_monitored_task;

// TODO: placeholder, read from env or config file.
pub const FAKE_PKG_VERSION: &str = "0.0.0";

pub struct Indexer;

impl Indexer {
    pub async fn start<S: IndexerStore + Sync + Send + Clone + 'static>(
        fullnode_url: &str,
        registry: &Registry,
        store: S,
    ) -> Result<(), IndexerError> {
        let handle = build_json_rpc_server(registry, store.clone(), fullnode_url)
            .await
            .expect("Json rpc server should not run into errors upon start.");
        // let JSON RPC server run forever.
        spawn_monitored_task!(handle.stopped());
        info!("Sui indexer started...");

        backoff::future::retry(ExponentialBackoff::default(), || async {
            let rpc_client = new_rpc_client(fullnode_url).await?;
            // NOTE: Each handler is responsible for one type of data from nodes,like transactions and events;
            // Handler orchestrator runs these handlers in parallel and manage them upon errors etc.
            let cp = CheckpointHandler::new(store.clone(), rpc_client.clone(), registry);
            cp.spawn()
                .await
                .expect("Indexer main should not run into errors.");
            Ok(())
        })
        .await
    }
}

pub async fn new_rpc_client(http_url: &str) -> Result<SuiClient, IndexerError> {
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

pub async fn new_pg_connection_pool(db_url: &str) -> Result<PgConnectionPool, IndexerError> {
    let manager = ConnectionManager::<PgConnection>::new(db_url);
    // default connection pool max size is 10
    Pool::builder().build(manager).map_err(|e| {
        IndexerError::PgConnectionPoolInitError(format!(
            "Failed to initialize connection pool with error: {:?}",
            e
        ))
    })
}

pub fn get_pg_pool_connection(pool: &PgConnectionPool) -> Result<PgPoolConnection, IndexerError> {
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

pub async fn build_json_rpc_server<S: IndexerStore + Sync + Send + 'static>(
    prometheus_registry: &Registry,
    state: S,
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

    builder.register_module(ReadApi::new(state, http_client.clone()))?;
    builder.register_module(CoinReadApi::new(http_client.clone()))?;
    builder.register_module(TransactionBuilderApi::new(http_client.clone()))?;
    builder.register_module(GovernanceReadApi::new(http_client.clone()))?;
    builder.register_module(EventReadApi::new(http_client.clone()))?;
    builder.register_module(WriteApi::new(http_client))?;
    // TODO: placeholder, read from env or config file.
    let default_socket_addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 3030);
    Ok(builder.start(default_socket_addr).await?)
}
