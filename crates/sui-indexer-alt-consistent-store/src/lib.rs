// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::path::Path;

use config::{PipelineLayer, ServiceConfig};
use db::config::DbConfig;
use handlers::object_by_owner::ObjectByOwner;
use indexer::Indexer;
use prometheus::Registry;
use rpc::{RpcArgs, RpcService};
use schema::Schema;
use sui_indexer_alt_consistent_api::proto::{
    self, rpc::consistent::v1alpha::consistent_service_server::ConsistentServiceServer,
};
use sui_indexer_alt_framework::{
    ingestion::ClientArgs, pipeline::sequential::SequentialConfig, pipeline::CommitterConfig,
    IndexerArgs,
};
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;

pub mod args;
pub mod config;
mod db;
mod handlers;
mod indexer;
mod rpc;
pub(crate) mod schema;
mod store;

/// Set-up and run the Indexer and RPC service, using the provided arguments (expected to be
/// extracted from the command-line). The service will continue to run until the cancellation token
/// is triggered, and will signal cancellation on the token when it is shutting down.
///
/// `path` is the path to the RocksDB database,which will be created if it does not exist.
/// `indexer_args` and `client_args` control the behavior of the Indexer, while `rpc_args` controls
/// the behavior of the RPC service.
///
/// The service spins up auxiliary services (to expose metrics, run the indexer, and the RPC), and
/// will clean these up on shutdown as well.
pub async fn start_service(
    path: impl AsRef<Path>,
    indexer_args: IndexerArgs,
    client_args: ClientArgs,
    rpc_args: RpcArgs,
    version: &'static str,
    config: ServiceConfig,
    registry: &Registry,
    cancel: CancellationToken,
) -> anyhow::Result<JoinHandle<()>> {
    let ServiceConfig {
        ingestion,
        consistency,
        rocksdb,
        committer,
        pipeline: PipelineLayer { object_by_owner },
    } = config;

    let committer = committer.finish(CommitterConfig::default());

    let mut indexer: Indexer<Schema> = Indexer::new(
        path,
        indexer_args,
        client_args,
        consistency,
        ingestion.into(),
        rocksdb,
        registry,
        cancel.child_token(),
    )
    .await?;

    let rpc = RpcService::new(rpc_args, version, registry, cancel.child_token())
        .register_encoded_file_descriptor_set(proto::rpc::consistent::v1alpha::FILE_DESCRIPTOR_SET)
        .add_service(ConsistentServiceServer::new(indexer.store().clone()));

    macro_rules! add_sequential {
        ($handler:expr, $config:expr) => {
            if let Some(layer) = $config {
                indexer
                    .sequential_pipeline(
                        $handler,
                        SequentialConfig {
                            committer: layer.finish(committer.clone()),
                            checkpoint_lag: 0,
                        },
                    )
                    .await?
            }
        };
    }

    add_sequential!(ObjectByOwner, object_by_owner);

    let h_rpc = rpc.run().await?;
    let h_indexer = indexer.run().await?;

    Ok(tokio::spawn(async move {
        let (_, _) = futures::join!(h_rpc, h_indexer);
    }))
}
