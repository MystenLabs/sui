// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
#![recursion_limit = "256"]

use std::env;
use std::net::SocketAddr;
use std::sync::Arc;
use std::{collections::HashMap, time::Duration};

use anyhow::{anyhow, Result};
use axum::{extract::Extension, http::StatusCode, routing::get, Router};
use backoff::ExponentialBackoff;
use clap::Parser;
use diesel::pg::PgConnection;
use diesel::r2d2::ConnectionManager;
use jsonrpsee::http_client::{HeaderMap, HeaderValue, HttpClient, HttpClientBuilder};
use metrics::IndexerMetrics;
use prometheus::{Registry, TextEncoder};
use regex::Regex;
use tokio::runtime::Handle;
use tracing::{info, warn};
use url::Url;

use apis::{
    CoinReadApi, ExtendedApi, GovernanceReadApi, IndexerApi, ReadApi, TransactionBuilderApi,
    WriteApi,
};
use errors::IndexerError;
use handlers::checkpoint_handler::CheckpointHandler;
use mysten_metrics::{spawn_monitored_task, RegistryService};
use processors::processor_orchestrator::ProcessorOrchestrator;
use store::IndexerStore;
use sui_core::subscription_handler::SubscriptionHandler;
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

pub type PgConnectionPool = diesel::r2d2::Pool<ConnectionManager<PgConnection>>;
pub type PgPoolConnection = diesel::r2d2::PooledConnection<ConnectionManager<PgConnection>>;

const METRICS_ROUTE: &str = "/metrics";
/// Returns all endpoints for which we have implemented on the indexer,
/// some of them are not validated yet.
/// NOTE: we only use this for integration testing
const IMPLEMENTED_METHODS: [&str; 9] = [
    // read apis
    "get_checkpoint",
    "get_latest_checkpoint_sequence_number",
    "get_object",
    "get_owned_objects",
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
    pub db_url: Option<String>,
    #[clap(long)]
    pub db_user_name: Option<String>,
    #[clap(long)]
    pub db_password: Option<String>,
    #[clap(long)]
    pub db_host: Option<String>,
    #[clap(long)]
    pub db_port: Option<u16>,
    #[clap(long)]
    pub db_name: Option<String>,
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
    #[clap(long)]
    pub fullnode_sync_worker: bool,
    #[clap(long)]
    pub rpc_server_worker: bool,
    // NOTE: experimental only, do not use in production.
    #[clap(long)]
    pub skip_db_commit: bool,
}

impl IndexerConfig {
    /// returns connection url without the db name
    pub fn base_connection_url(&self) -> Result<String, anyhow::Error> {
        let url_str = self.get_db_url()?;
        let url = Url::parse(&url_str).expect("Failed to parse URL");
        Ok(format!(
            "{}://{}:{}@{}:{}/",
            url.scheme(),
            url.username(),
            url.password().unwrap_or_default(),
            url.host_str().unwrap_or_default(),
            url.port().unwrap_or_default()
        ))
    }

    pub fn all_implemented_methods() -> Vec<String> {
        IMPLEMENTED_METHODS.iter().map(|&s| s.to_string()).collect()
    }

    pub fn get_db_url(&self) -> Result<String, anyhow::Error> {
        match (&self.db_url, &self.db_user_name, &self.db_password, &self.db_host, &self.db_port, &self.db_name) {
            (Some(db_url), _, _, _, _, _) => Ok(db_url.clone()),
            (None, Some(db_user_name), Some(db_password), Some(db_host), Some(db_port), Some(db_name)) => {
                Ok(format!(
                    "postgres://{}:{}@{}:{}/{}",
                    db_user_name, db_password, db_host, db_port, db_name
                ))
            }
            _ => Err(anyhow!("Invalid db connection config, either db_url or (db_user_name, db_password, db_host, db_port, db_name) must be provided")),
        }
    }
}

impl Default for IndexerConfig {
    fn default() -> Self {
        Self {
            db_url: Some("postgres://postgres:postgres@localhost:5432/sui_indexer".to_string()),
            db_user_name: None,
            db_password: None,
            db_host: None,
            db_port: None,
            db_name: None,
            rpc_client_url: "http://127.0.0.1:9000".to_string(),
            client_metric_host: "0.0.0.0".to_string(),
            client_metric_port: 9184,
            rpc_server_url: "0.0.0.0".to_string(),
            rpc_server_port: 9000,
            migrated_methods: vec![],
            reset_db: false,
            fullnode_sync_worker: true,
            rpc_server_worker: true,
            skip_db_commit: false,
        }
    }
}

pub struct Indexer;

impl Indexer {
    pub async fn start<S: IndexerStore + Sync + Send + Clone + 'static>(
        config: &IndexerConfig,
        registry: &Registry,
        store: S,
        metrics: IndexerMetrics,
        custom_runtime: Option<Handle>,
    ) -> Result<(), IndexerError> {
        info!(
            "Sui indexer of version {:?} started...",
            env!("CARGO_PKG_VERSION")
        );
        let subscription_handler = Arc::new(SubscriptionHandler::new(registry));

        if config.rpc_server_worker && config.fullnode_sync_worker {
            info!("Starting indexer with both fullnode sync and RPC server");
            // let JSON RPC server run forever.
            let handle = build_json_rpc_server(
                registry,
                store.clone(),
                subscription_handler.clone(),
                config,
                custom_runtime,
            )
            .await
            .expect("Json rpc server should not run into errors upon start.");
            spawn_monitored_task!(handle.stopped());

            // let async processor run forever.
            let mut processor_orchestrator = ProcessorOrchestrator::new(store.clone(), registry);
            spawn_monitored_task!(processor_orchestrator.run_forever());

            backoff::future::retry(ExponentialBackoff::default(), || async {
                let subscription_handler_clone = subscription_handler.clone();
                let metrics_clone = metrics.clone();
                let http_client = get_http_client(config.rpc_client_url.as_str())?;
                let cp = CheckpointHandler::new(
                    store.clone(),
                    http_client,
                    subscription_handler_clone,
                    metrics_clone,
                    config,
                );
                cp.spawn()
                    .await
                    .expect("Indexer main should not run into errors.");
                Ok(())
            })
            .await
        } else if config.rpc_server_worker {
            info!("Starting indexer with only RPC server");
            let handle = build_json_rpc_server(
                registry,
                store.clone(),
                subscription_handler.clone(),
                config,
                custom_runtime,
            )
            .await
            .expect("Json rpc server should not run into errors upon start.");
            handle.stopped().await;
            Ok(())
        } else if config.fullnode_sync_worker {
            info!("Starting indexer with only fullnode sync");
            let mut processor_orchestrator = ProcessorOrchestrator::new(store.clone(), registry);
            spawn_monitored_task!(processor_orchestrator.run_forever());

            backoff::future::retry(ExponentialBackoff::default(), || async {
                let subscription_handler_clone = subscription_handler.clone();
                let metrics_clone = metrics.clone();
                let http_client = get_http_client(config.rpc_client_url.as_str())?;
                let cp = CheckpointHandler::new(
                    store.clone(),
                    http_client,
                    subscription_handler_clone,
                    metrics_clone,
                    config,
                );
                cp.spawn()
                    .await
                    .expect("Indexer main should not run into errors.");
                Ok(())
            })
            .await
        } else {
            Ok(())
        }
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

pub fn new_pg_connection_pool(db_url: &str) -> Result<PgConnectionPool, IndexerError> {
    let pool_config = PgConectionPoolConfig::default();
    let manager = ConnectionManager::<PgConnection>::new(db_url);

    diesel::r2d2::Pool::builder()
        .max_size(pool_config.pool_size)
        .connection_timeout(pool_config.connection_timeout)
        .connection_customizer(Box::new(pool_config.connection_config()))
        .build(manager)
        .map_err(|e| {
            IndexerError::PgConnectionPoolInitError(format!(
                "Failed to initialize connection pool with error: {:?}",
                e
            ))
        })
}

#[derive(Debug, Clone, Copy)]
struct PgConectionPoolConfig {
    pool_size: u32,
    connection_timeout: Duration,
    statement_timeout: Duration,
}

impl PgConectionPoolConfig {
    const DEFAULT_POOL_SIZE: u32 = 10;
    const DEFAULT_CONNECTION_TIMEOUT: Duration = Duration::from_secs(30);
    const DEFAULT_STATEMENT_TIMEOUT: Duration = Duration::from_secs(5);

    fn connection_config(&self) -> PgConnectionConfig {
        PgConnectionConfig {
            statement_timeout: self.statement_timeout,
        }
    }
}

impl Default for PgConectionPoolConfig {
    fn default() -> Self {
        Self {
            pool_size: Self::DEFAULT_POOL_SIZE,
            connection_timeout: Self::DEFAULT_CONNECTION_TIMEOUT,
            statement_timeout: Self::DEFAULT_STATEMENT_TIMEOUT,
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct PgConnectionConfig {
    statement_timeout: Duration,
    // read_only: bool,
}

impl diesel::r2d2::CustomizeConnection<PgConnection, diesel::r2d2::Error> for PgConnectionConfig {
    fn on_acquire(&self, conn: &mut PgConnection) -> std::result::Result<(), diesel::r2d2::Error> {
        use diesel::{sql_query, RunQueryDsl};

        sql_query(format!(
            "SET statement_timeout = {}",
            self.statement_timeout.as_millis(),
        ))
        .execute(conn)
        .map_err(diesel::r2d2::Error::QueryError)?;

        // if self.read_only {
        //     sql_query("SET default_transaction_read_only = 't'")
        //         .execute(conn)
        //         .map_err(r2d2::Error::QueryError)?;
        // }

        Ok(())
    }
}

pub fn get_pg_pool_connection(pool: &PgConnectionPool) -> Result<PgPoolConnection, IndexerError> {
    pool.get().map_err(|e| {
        IndexerError::PgPoolConnectionError(format!(
            "Failed to get connection from PG connection pool with error: {:?}",
            e
        ))
    })
}

pub async fn build_json_rpc_server<S: IndexerStore + Sync + Send + 'static + Clone>(
    prometheus_registry: &Registry,
    state: S,
    subscription_handler: Arc<SubscriptionHandler>,
    config: &IndexerConfig,
    custom_runtime: Option<Handle>,
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
        subscription_handler,
        config.migrated_methods.clone(),
    ))?;
    builder.register_module(WriteApi::new(state.clone(), http_client.clone()))?;
    builder.register_module(ExtendedApi::new(state.clone()))?;
    builder.register_module(MoveUtilsApi::new(http_client))?;
    let default_socket_addr = SocketAddr::new(
        // unwrap() here is safe b/c the address is a static config.
        config.rpc_server_url.as_str().parse().unwrap(),
        config.rpc_server_port,
    );
    Ok(builder.start(default_socket_addr, custom_runtime).await?)
}

fn convert_url(url_str: &str) -> Option<String> {
    // NOTE: unwrap here is safe because the regex is a constant.
    let re = Regex::new(r"https?://([a-z0-9-]+\.[a-z0-9-]+\.[a-z]+)").unwrap();
    let captures = re.captures(url_str)?;

    captures.get(1).map(|m| m.as_str().to_string())
}

pub fn start_prometheus_server(
    addr: SocketAddr,
    fn_url: &str,
) -> Result<(RegistryService, Registry), anyhow::Error> {
    let converted_fn_url = convert_url(fn_url);
    if converted_fn_url.is_none() {
        warn!(
            "Failed to convert full node url {} to a shorter version",
            fn_url
        );
    }
    let fn_url_str = converted_fn_url.unwrap_or_else(|| "unknown_url".to_string());

    let labels = HashMap::from([("indexer_fullnode".to_string(), fn_url_str)]);
    info!("Starting prometheus server with labels: {:?}", labels);
    let registry = Registry::new_custom(Some("indexer".to_string()), Some(labels))?;
    let registry_service = RegistryService::new(registry.clone());

    let app = Router::new()
        .route(METRICS_ROUTE, get(metrics))
        .layer(Extension(registry_service.clone()));

    tokio::spawn(async move {
        axum::Server::bind(&addr)
            .serve(app.into_make_service())
            .await
            .unwrap();
    });
    Ok((registry_service, registry))
}

async fn metrics(Extension(registry_service): Extension<RegistryService>) -> (StatusCode, String) {
    let metrics_families = registry_service.gather_all();
    match TextEncoder.encode_to_string(&metrics_families) {
        Ok(metrics) => (StatusCode::OK, metrics),
        Err(error) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("unable to encode metrics: {error}"),
        ),
    }
}
