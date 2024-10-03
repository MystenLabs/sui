// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use config::JsonRpcConfig;
use deepbook_api::DeepBookApi;
use errors::IndexerError;
use prometheus::Registry;
use sui_deepbook_indexer::PgDeepbookPersistent;
use sui_json_rpc::ServerType;
use sui_json_rpc::{JsonRpcServerBuilder, ServerHandle};
use tokio_util::sync::CancellationToken;

pub mod config;
pub mod deepbook_api;
pub mod errors;
pub mod events;
pub mod metrics;
pub mod models;
pub mod postgres_manager;
pub mod schema;
pub mod types;

pub mod sui_datasource;
pub mod sui_deepbook_indexer;

pub async fn build_json_rpc_server(
    prometheus_registry: &Registry,
    pool: PgDeepbookPersistent,
    config: &JsonRpcConfig,
) -> Result<ServerHandle, IndexerError> {
    let mut builder =
        JsonRpcServerBuilder::new(env!("CARGO_PKG_VERSION"), prometheus_registry, None, None);

    builder.register_module(DeepBookApi::new(pool.clone()))?;
    let cancel = CancellationToken::new();

    Ok(builder
        .start(config.rpc_address, None, ServerType::Http, Some(cancel))
        .await?)
}
