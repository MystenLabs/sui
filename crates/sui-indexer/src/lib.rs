// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
#![recursion_limit = "256"]

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
use jsonrpsee::http_client::{HeaderMap, HeaderValue, HttpClient, HttpClientBuilder};
use prometheus::Registry;
use tracing::{info, warn};
use url::Url;

use apis::{
    CoinReadApi, ExtendedApi, GovernanceReadApi, IndexerApi, ReadApi, TransactionBuilderApi,
    WriteApi,
};
use errors::IndexerError;
use handlers::checkpoint_handler::CheckpointHandler;
use mysten_metrics::spawn_monitored_task;
use store::IndexerStore;
use sui_core::event_handler::EventHandler;
use sui_json_rpc::{JsonRpcServerBuilder, ServerHandle, CLIENT_SDK_TYPE_HEADER};
use sui_sdk::{SuiClient, SuiClientBuilder};

use crate::apis::MoveUtilsApi;

pub mod apis;
pub mod errors;
mod handlers;
pub mod metrics;
pub mod models;
pub mod processors;
pub mod schema;
pub mod store;
pub mod test_utils;
pub mod types;
pub mod utils;

pub type PgConnectionPool = Pool<ConnectionManager<PgConnection>>;
pub type PgPoolConnection = PooledConnection<ConnectionManager<PgConnection>>;

/// Returns all endpoints for which we have implemented on the indexer,
/// some of them are not validated yet.
/// NOTE: we only use this for integration testing
const IMPLEMENTED_METHODS: [&str; 8] = [
    // read apis
    "get_checkpoint",
    "get_latest_checkpoint_sequence_number",
    "get_object",
    "get_total_transaction_blocks",
    "get_transaction_block",
    "multi_get_transaction_blocks",
    // indexer apis
    "query_events",
    "query_transaction_blocks",
];

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
    #[clap(long, multiple_occurrences = false, multiple_values = true)]
    pub migrated_methods: Vec<String>,
    #[clap(long)]
    pub reset_db: bool,
}

impl IndexerConfig {
    /// returns connection url without the db name
    pub fn base_connection_url(&self) -> String {
        let url = Url::parse(&self.db_url).expect("Failed to parse URL");
        format!(
            "{}://{}:{}@{}:{}/",
            url.scheme(),
            url.username(),
            url.password().unwrap_or_default(),
            url.host_str().unwrap_or_default(),
            url.port().unwrap_or_default()
        )
    }

    pub fn all_implemented_methods() -> Vec<String> {
        IMPLEMENTED_METHODS.iter().map(|&s| s.to_string()).collect()
    }
}

impl Default for IndexerConfig {
    fn default() -> Self {
        Self {
            db_url: "postgres://postgres:postgres@localhost:5432/sui_indexer".to_string(),
            rpc_client_url: "http://127.0.0.1:9000".to_string(),
            client_metric_host: "0.0.0.0".to_string(),
            client_metric_port: 9184,
            rpc_server_url: "0.0.0.0".to_string(),
            rpc_server_port: 9000,
            migrated_methods: vec![],
            reset_db: false,
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
        info!(
            "Sui indexer of version {:?} started...",
            env!("CARGO_PKG_VERSION")
        );

        backoff::future::retry(ExponentialBackoff::default(), || async {
            let event_handler_clone = event_handler.clone();
            let http_client = get_http_client(config.rpc_client_url.as_str())?;
            let cp =
                CheckpointHandler::new(store.clone(), http_client, event_handler_clone, registry);
            cp.spawn()
                .await
                .expect("Indexer main should not run into errors.");
            Ok(())
        })
        .await
    }
}

// TODO(gegaowp): this is only used in validation now, will remove in a separate PR
// together with the validation codes.
pub async fn new_rpc_client(http_url: &str) -> Result<SuiClient, IndexerError> {
    info!("Getting new RPC client...");
    SuiClientBuilder::default()
        .build(http_url)
        .await
        .map_err(|e| {
            warn!("Failed to get new RPC client with error: {:?}", e);
            IndexerError::HttpClientInitError(format!(
                "Failed to initialize fullnode RPC client with error: {:?}",
                e
            ))
        })
}

fn get_http_client(rpc_client_url: &str) -> Result<HttpClient, IndexerError> {
    let mut headers = HeaderMap::new();
    headers.insert(CLIENT_SDK_TYPE_HEADER, HeaderValue::from_static("indexer"));

    HttpClientBuilder::default()
        .max_request_body_size(2 << 30)
        .max_concurrent_requests(usize::MAX)
        .set_headers(headers.clone())
        .build(rpc_client_url)
        .map_err(|e| {
            warn!("Failed to get new Http client with error: {:?}", e);
            IndexerError::HttpClientInitError(format!(
                "Failed to initialize fullnode RPC client with error: {:?}",
                e
            ))
        })
}

pub fn establish_connection(db_url: String) -> PgConnection {
    PgConnection::establish(&db_url).unwrap_or_else(|_| panic!("Error connecting to {}", db_url))
}

pub fn new_pg_connection_pool(db_url: &str) -> Result<PgConnectionPool, IndexerError> {
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
    let mut builder = JsonRpcServerBuilder::new(env!("CARGO_PKG_VERSION"), prometheus_registry);
    let http_client = get_http_client(config.rpc_client_url.as_str())?;

    builder.register_module(ReadApi::new(
        state.clone(),
        http_client.clone(),
        config.migrated_methods.clone(),
    ))?;
    builder.register_module(CoinReadApi::new(http_client.clone()))?;
    builder.register_module(TransactionBuilderApi::new(http_client.clone()))?;
    builder.register_module(GovernanceReadApi::new(http_client.clone()))?;
    builder.register_module(IndexerApi::new(
        state.clone(),
        http_client.clone(),
        event_handler,
        config.migrated_methods.clone(),
    ))?;
    builder.register_module(WriteApi::new(state.clone(), http_client.clone()))?;
    builder.register_module(ExtendedApi::new(state.clone()))?;
    builder.register_module(MoveUtilsApi::new(http_client))?;
    let default_socket_addr = SocketAddr::new(
        // unwrap() here is safe b/c the address is a static config.
        IpAddr::V4(Ipv4Addr::from_str(config.rpc_server_url.as_str()).unwrap()),
        config.rpc_server_port,
    );
    Ok(builder.start(default_socket_addr).await?)
}
