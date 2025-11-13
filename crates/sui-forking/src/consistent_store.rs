use anyhow::Context;
use prometheus::Registry;
use reqwest::Url;
use std::path::{Path, PathBuf};
use sui_indexer_alt::{config::IndexerConfig as IndexerAltConfig, setup_indexer};
use sui_indexer_alt_consistent_store::{args::RpcArgs, config::ServiceConfig, start_service};
use sui_indexer_alt_framework::{
    IndexerArgs,
    ingestion::{self, ClientArgs},
};
use sui_pg_db::DbArgs;
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;

pub(crate) struct ConsistentStoreConfig {
    rocksdb_path: PathBuf,
    indexer_args: IndexerArgs,
    client_args: ClientArgs,
    version: &'static str,
}

impl ConsistentStoreConfig {
    pub fn new(
        rocksdb_path: PathBuf,
        indexer_args: IndexerArgs,
        client_args: ClientArgs,
        version: &'static str,
    ) -> Self {
        Self {
            rocksdb_path,
            indexer_args,
            client_args,
            version,
        }
    }
}

pub(crate) async fn start_consistent_store(
    config: ConsistentStoreConfig,
    registry: &Registry,
    cancel: CancellationToken,
) -> anyhow::Result<JoinHandle<()>> {
    let ConsistentStoreConfig {
        rocksdb_path,
        indexer_args,
        client_args,
        version,
    } = config;
    let service_config = ServiceConfig::for_test();
    let rpc_args = RpcArgs::default();
    let service = start_service(
        rocksdb_path,
        indexer_args,
        client_args,
        rpc_args,
        config.version,
        service_config,
        registry,
        cancel.child_token(),
    )
    .await
    .context("Failed to start consistent store service")?;

    Ok(service)
}
