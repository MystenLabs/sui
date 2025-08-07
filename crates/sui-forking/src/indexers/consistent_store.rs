// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::path::PathBuf;

use anyhow::Context;
use prometheus::Registry;

use sui_futures::service::Service;
use sui_indexer_alt_consistent_store::{args::RpcArgs, config::ServiceConfig, start_service};
use sui_indexer_alt_framework::{IndexerArgs, ingestion::ClientArgs};

pub(crate) struct ConsistentStoreConfig {
    rocksdb_path: PathBuf,
    indexer_args: IndexerArgs,
    client_args: ClientArgs,
}

impl ConsistentStoreConfig {
    pub fn new(rocksdb_path: PathBuf, indexer_args: IndexerArgs, client_args: ClientArgs) -> Self {
        Self {
            rocksdb_path,
            indexer_args,
            client_args,
        }
    }
}

pub(crate) async fn start_consistent_store(
    config: ConsistentStoreConfig,
    registry: &Registry,
    version: &'static str,
) -> anyhow::Result<Service> {
    let ConsistentStoreConfig {
        rocksdb_path,
        indexer_args,
        client_args,
    } = config;
    let service_config = ServiceConfig::for_test();
    let rpc_args = RpcArgs::default();
    let service = start_service(
        rocksdb_path,
        indexer_args,
        client_args,
        rpc_args,
        version,
        service_config,
        registry,
    )
    .await
    .context("Failed to start consistent store service")?;

    Ok(service)
}
