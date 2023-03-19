// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::str::FromStr;
use std::sync::Arc;

use anyhow::Result;
use backoff::retry;
use backoff::ExponentialBackoff;
use clap::Parser;
use diesel::pg::PgConnection;
use diesel::prelude::*;
use diesel::r2d2::{ConnectionManager, Pool, PooledConnection};
use jsonrpsee::http_client::{HeaderMap, HeaderValue, HttpClientBuilder};
use prometheus::Registry;
use tracing::{info, warn};

use errors::IndexerError;
use mysten_metrics::spawn_monitored_task;
use sui_core::event_handler::EventHandler;
use sui_json_rpc::{JsonRpcServerBuilder, ServerHandle, CLIENT_SDK_TYPE_HEADER};
use sui_json_rpc_types::SuiTransactionResponseOptions;
use sui_sdk::apis::ReadApi as SuiReadApi;
use sui_sdk::{SuiClient, SuiClientBuilder};
use sui_types::base_types::TransactionDigest;

use crate::apis::{
    CoinReadApi, EventReadApi, GovernanceReadApi, ReadApi, TransactionBuilderApi, WriteApi,
};
use crate::handlers::checkpoint_handler::CheckpointHandler;
use crate::store::IndexerStore;
use crate::types::SuiTransactionFullResponse;

pub mod apis;
pub mod errors;
mod handlers;
pub mod metrics;
pub mod models;
pub mod processors;
pub mod schema;
pub mod store;
pub mod types;
pub mod utils;

pub type PgConnectionPool = Pool<ConnectionManager<PgConnection>>;
pub type PgPoolConnection = PooledConnection<ConnectionManager<PgConnection>>;

// TODO: placeholder, read from env or config file.
pub const FAKE_PKG_VERSION: &str = "0.0.0";

#[derive(Parser, Clone, Debug)]
#[clap(
    name = "Sui indexer",
    about = "An off-fullnode service serving data from Sui protocol",
    rename_all = "kebab-case"
)]
pub struct IndexerConfig {
    #[clap(long)]
    pub db_url: String,
    #[clap(long)]
    pub rpc_client_url: String,
    #[clap(long, default_value = "0.0.0.0", global = true)]
    pub client_metric_host: String,
    #[clap(long, default_value = "9184", global = true)]
    pub client_metric_port: u16,
    #[clap(long, default_value = "0.0.0.0", global = true)]
    pub rpc_server_url: String,
    #[clap(long, default_value = "9000", global = true)]
    pub rpc_server_port: u16,
}

impl IndexerConfig {
    pub fn default() -> Self {
        Self {
            db_url: "postgres://postgres:postgres@localhost:5432/sui_indexer".to_string(),
            rpc_client_url: "http://127.0.0.1:9000".to_string(),
            client_metric_host: "0.0.0.0".to_string(),
            client_metric_port: 9184,
            rpc_server_url: "0.0.0.0".to_string(),
            rpc_server_port: 9000,
        }
    }
}

pub struct Indexer;

impl Indexer {
    pub async fn start<S: IndexerStore + Sync + Send + Clone + 'static>(
        config: &IndexerConfig,
        registry: &Registry,
        store: S,
    ) -> Result<(), IndexerError> {
        let event_handler = Arc::new(EventHandler::default());
        let handle = build_json_rpc_server(registry, store.clone(), event_handler.clone(), config)
            .await
            .expect("Json rpc server should not run into errors upon start.");
        // let JSON RPC server run forever.
        spawn_monitored_task!(handle.stopped());
        info!("Sui indexer started...");

        backoff::future::retry(ExponentialBackoff::default(), || async {
            let event_handler_clone = event_handler.clone();
            let rpc_client = new_rpc_client(config.rpc_client_url.as_str()).await?;
            // NOTE: Each handler is responsible for one type of data from nodes,like transactions and events;
            // Handler orchestrator runs these handlers in parallel and manage them upon errors etc.
            let cp = CheckpointHandler::new(
                store.clone(),
                rpc_client.clone(),
                event_handler_clone,
                registry,
            );
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

pub async fn build_json_rpc_server<S: IndexerStore + Sync + Send + 'static + Clone>(
    prometheus_registry: &Registry,
    state: S,
    event_handler: Arc<EventHandler>,
    config: &IndexerConfig,
) -> Result<ServerHandle, IndexerError> {
    let mut builder = JsonRpcServerBuilder::new(FAKE_PKG_VERSION, prometheus_registry);

    let mut headers = HeaderMap::new();
    headers.insert(CLIENT_SDK_TYPE_HEADER, HeaderValue::from_static("indexer"));

    let http_client = HttpClientBuilder::default()
        .max_request_body_size(2 << 30)
        .max_concurrent_requests(usize::MAX)
        .set_headers(headers.clone())
        .build(config.rpc_client_url.as_str())
        .map_err(|e| IndexerError::RpcClientInitError(e.to_string()))?;

    builder.register_module(ReadApi::new(state.clone(), http_client.clone()))?;
    builder.register_module(CoinReadApi::new(http_client.clone()))?;
    builder.register_module(TransactionBuilderApi::new(http_client.clone()))?;
    builder.register_module(GovernanceReadApi::new(http_client.clone()))?;
    builder.register_module(EventReadApi::new(state, http_client.clone(), event_handler))?;
    builder.register_module(WriteApi::new(http_client))?;
    let default_socket_addr = SocketAddr::new(
        // unwrap() here is safe b/c the address is a static config.
        IpAddr::V4(Ipv4Addr::from_str(config.rpc_server_url.as_str()).unwrap()),
        config.rpc_server_port,
    );
    Ok(builder.start(default_socket_addr).await?)
}

pub async fn multi_get_full_transactions(
    read_api: &SuiReadApi,
    digests: Vec<TransactionDigest>,
) -> Result<Vec<SuiTransactionFullResponse>, IndexerError> {
    let sui_transactions = read_api
        .multi_get_transactions_with_options(
            digests.clone(),
            // MUSTFIX(gegaowp): avoid double fetching both input and raw_input
            SuiTransactionResponseOptions::new()
                .with_input()
                .with_effects()
                .with_events()
                .with_raw_input(),
        )
        .await
        .map_err(|e| {
            IndexerError::FullNodeReadingError(format!(
                "Failed to get transactions {:?} with error: {:?}",
                digests.clone(),
                e
            ))
        })?;
    let sui_full_transactions: Vec<SuiTransactionFullResponse> = sui_transactions
        .into_iter()
        .map(SuiTransactionFullResponse::try_from)
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| {
            IndexerError::FullNodeReadingError(format!(
                "Unexpected None value in SuiTransactionFullResponse of digests {:?} with error {:?}",
                digests, e
            ))
        })?;
    Ok(sui_full_transactions)
}
