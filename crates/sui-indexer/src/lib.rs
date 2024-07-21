// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
#![recursion_limit = "256"]

use std::net::SocketAddr;
use std::time::Duration;

use anyhow::{anyhow, Result};
use clap::Parser;
use diesel::r2d2::R2D2Connection;
use jsonrpsee::http_client::{HeaderMap, HeaderValue, HttpClient, HttpClientBuilder};
use metrics::IndexerMetrics;
use mysten_metrics::spawn_monitored_task;
use prometheus::Registry;
use secrecy::{ExposeSecret, Secret};
use std::path::PathBuf;
use sui_types::base_types::{ObjectID, SuiAddress};
use system_package_task::SystemPackageTask;
use tokio::runtime::Handle;
use tokio_util::sync::CancellationToken;
use tracing::warn;
use url::Url;

use sui_json_rpc::ServerType;
use sui_json_rpc::{JsonRpcServerBuilder, ServerHandle};
use sui_json_rpc_api::CLIENT_SDK_TYPE_HEADER;

use crate::apis::{
    CoinReadApi, ExtendedApi, GovernanceReadApi, IndexerApi, MoveUtilsApi, ReadApi,
    TransactionBuilderApi, WriteApi,
};
use crate::indexer_reader::IndexerReader;
use errors::IndexerError;

pub mod apis;
pub mod db;
pub mod errors;
pub mod handlers;
pub mod indexer;
pub mod indexer_reader;
pub mod metrics;
pub mod models;
pub mod schema;
pub mod store;
pub mod system_package_task;
pub mod test_utils;
pub mod types;

#[derive(Parser, Clone, Debug)]
#[clap(
    name = "Sui indexer",
    about = "An off-fullnode service serving data from Sui protocol",
    rename_all = "kebab-case"
)]
pub struct IndexerConfig {
    #[clap(long)]
    pub db_url: Option<Secret<String>>,
    #[clap(long)]
    pub db_user_name: Option<String>,
    #[clap(long)]
    pub db_password: Option<Secret<String>>,
    #[clap(long)]
    pub db_host: Option<String>,
    #[clap(long)]
    pub db_port: Option<u16>,
    #[clap(long)]
    pub db_name: Option<String>,
    #[clap(long, default_value = "http://0.0.0.0:9000", global = true)]
    pub rpc_client_url: String,
    #[clap(long, default_value = Some("https://checkpoints.mainnet.sui.io"), global = true)]
    pub remote_store_url: Option<String>,
    #[clap(long, default_value = "0.0.0.0", global = true)]
    pub client_metric_host: String,
    #[clap(long, default_value = "9184", global = true)]
    pub client_metric_port: u16,
    #[clap(long, default_value = "0.0.0.0", global = true)]
    pub rpc_server_url: String,
    #[clap(long, default_value = "9000", global = true)]
    pub rpc_server_port: u16,
    #[clap(long)]
    pub reset_db: bool,
    #[clap(long)]
    pub fullnode_sync_worker: bool,
    #[clap(long)]
    pub rpc_server_worker: bool,
    #[clap(long)]
    pub data_ingestion_path: Option<PathBuf>,
    #[clap(long)]
    pub name_service_package_address: Option<SuiAddress>,
    #[clap(long)]
    pub name_service_registry_id: Option<ObjectID>,
    #[clap(long)]
    pub name_service_reverse_registry_id: Option<ObjectID>,
}

impl IndexerConfig {
    /// returns connection url without the db name
    pub fn base_connection_url(&self) -> Result<String, anyhow::Error> {
        let url_secret = self.get_db_url()?;
        let url_str = url_secret.expose_secret();
        let url = Url::parse(url_str).expect("Failed to parse URL");
        Ok(format!(
            "{}://{}:{}@{}:{}/",
            url.scheme(),
            url.username(),
            url.password().unwrap_or_default(),
            url.host_str().unwrap_or_default(),
            url.port().unwrap_or_default()
        ))
    }

    pub fn get_db_url(&self) -> Result<Secret<String>, anyhow::Error> {
        match (&self.db_url, &self.db_user_name, &self.db_password, &self.db_host, &self.db_port, &self.db_name) {
            (Some(db_url), _, _, _, _, _) => Ok(db_url.clone()),
            (None, Some(db_user_name), Some(db_password), Some(db_host), Some(db_port), Some(db_name)) => {
                Ok(secrecy::Secret::new(format!(
                    "postgres://{}:{}@{}:{}/{}",
                    db_user_name, db_password.expose_secret(), db_host, db_port, db_name
                )))
            }
            _ => Err(anyhow!("Invalid db connection config, either db_url or (db_user_name, db_password, db_host, db_port, db_name) must be provided")),
        }
    }
}

impl Default for IndexerConfig {
    fn default() -> Self {
        Self {
            db_url: Some(secrecy::Secret::new(
                "postgres://postgres:postgres@localhost:5432/sui_indexer".to_string(),
            )),
            db_user_name: None,
            db_password: None,
            db_host: None,
            db_port: None,
            db_name: None,
            rpc_client_url: "http://127.0.0.1:9000".to_string(),
            remote_store_url: Some("https://checkpoints.mainnet.sui.io".to_string()),
            client_metric_host: "0.0.0.0".to_string(),
            client_metric_port: 9184,
            rpc_server_url: "0.0.0.0".to_string(),
            rpc_server_port: 9000,
            reset_db: false,
            fullnode_sync_worker: true,
            rpc_server_worker: true,
            data_ingestion_path: None,
            name_service_package_address: None,
            name_service_registry_id: None,
            name_service_reverse_registry_id: None,
        }
    }
}

pub async fn build_json_rpc_server<T: R2D2Connection>(
    prometheus_registry: &Registry,
    reader: IndexerReader<T>,
    config: &IndexerConfig,
    custom_runtime: Option<Handle>,
) -> Result<ServerHandle, IndexerError> {
    let mut builder =
        JsonRpcServerBuilder::new(env!("CARGO_PKG_VERSION"), prometheus_registry, None, None);
    let http_client = crate::get_http_client(config.rpc_client_url.as_str())?;

    let name_service_config =
        if let (Some(package_address), Some(registry_id), Some(reverse_registry_id)) = (
            config.name_service_package_address,
            config.name_service_registry_id,
            config.name_service_reverse_registry_id,
        ) {
            sui_json_rpc::name_service::NameServiceConfig::new(
                package_address,
                registry_id,
                reverse_registry_id,
            )
        } else {
            sui_json_rpc::name_service::NameServiceConfig::default()
        };

    builder.register_module(WriteApi::new(http_client.clone()))?;
    builder.register_module(IndexerApi::new(reader.clone(), name_service_config))?;
    builder.register_module(TransactionBuilderApi::new(reader.clone()))?;
    builder.register_module(MoveUtilsApi::new(reader.clone()))?;
    builder.register_module(GovernanceReadApi::new(reader.clone()))?;
    builder.register_module(ReadApi::new(reader.clone()))?;
    builder.register_module(CoinReadApi::new(reader.clone()))?;
    builder.register_module(ExtendedApi::new(reader.clone()))?;

    let default_socket_addr: SocketAddr = SocketAddr::new(
        // unwrap() here is safe b/c the address is a static config.
        config.rpc_server_url.as_str().parse().unwrap(),
        config.rpc_server_port,
    );

    let cancel = CancellationToken::new();
    let system_package_task =
        SystemPackageTask::new(reader.clone(), cancel.clone(), Duration::from_secs(10));

    tracing::info!("Starting system package task");
    spawn_monitored_task!(async move { system_package_task.run().await });

    Ok(builder
        .start(
            default_socket_addr,
            custom_runtime,
            ServerType::Http,
            Some(cancel),
        )
        .await?)
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
