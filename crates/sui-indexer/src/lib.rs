// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
#![recursion_limit = "256"]

use std::time::Duration;

use anyhow::Result;
use config::JsonRpcConfig;
use jsonrpsee::http_client::{HeaderMap, HeaderValue, HttpClient, HttpClientBuilder};
use metrics::IndexerMetrics;
use mysten_metrics::spawn_monitored_task;
use prometheus::Registry;
use system_package_task::SystemPackageTask;
use tokio_util::sync::CancellationToken;
use tracing::warn;

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
pub mod backfill;
pub mod config;
pub mod database;
pub mod db;
pub mod errors;
pub mod handlers;
pub mod indexer;
pub mod indexer_reader;
pub mod metrics;
pub mod models;
pub mod restorer;
pub mod schema;
pub mod store;
pub mod system_package_task;
pub mod test_utils;
pub mod types;

pub async fn build_json_rpc_server(
    prometheus_registry: &Registry,
    reader: IndexerReader,
    config: &JsonRpcConfig,
    cancel: CancellationToken,
) -> Result<ServerHandle, IndexerError> {
    let mut builder =
        JsonRpcServerBuilder::new(env!("CARGO_PKG_VERSION"), prometheus_registry, None, None);
    let http_client = crate::get_http_client(&config.rpc_client_url)?;

    builder.register_module(WriteApi::new(http_client.clone()))?;
    builder.register_module(IndexerApi::new(
        reader.clone(),
        config.name_service_options.to_config(),
    ))?;
    builder.register_module(TransactionBuilderApi::new(reader.clone()))?;
    builder.register_module(MoveUtilsApi::new(reader.clone()))?;
    builder.register_module(GovernanceReadApi::new(reader.clone()))?;
    builder.register_module(ReadApi::new(reader.clone()))?;
    builder.register_module(CoinReadApi::new(reader.clone()))?;
    builder.register_module(ExtendedApi::new(reader.clone()))?;

    let system_package_task =
        SystemPackageTask::new(reader.clone(), cancel.clone(), Duration::from_secs(10));

    tracing::info!("Starting system package task");
    spawn_monitored_task!(async move { system_package_task.run().await });

    Ok(builder
        .start(config.rpc_address, None, ServerType::Http, Some(cancel))
        .await?)
}

fn get_http_client(rpc_client_url: &str) -> Result<HttpClient, IndexerError> {
    let mut headers = HeaderMap::new();
    headers.insert(CLIENT_SDK_TYPE_HEADER, HeaderValue::from_static("indexer"));

    HttpClientBuilder::default()
        .max_request_size(2 << 30)
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
