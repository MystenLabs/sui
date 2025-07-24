// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! # Consistent Store Indexer and RPC
//!
//! This crate defines a service that combines a [`sui_indexer_alt_framework`] indexer, with a gRPC
//! service serving queries about live data, consistent with some recent (measured in minutes or
//! hours) checkpoint.
//!
//! Supported queries include fetching objects by owner or by type, and fetching an address'
//! balance (across coin-like objects it owns).
//!
//! The service's indexer writes to a RocksDB database which it interacts through a `db`
//! abstraction, which exposes a type-safe abstraction over the underlying bytes-to-bytes ordered
//! map offered by [`rocksdb`].
//!
//! The database abstraction is also responsible for taking and exposing snapshots of the database,
//! which is what allows the RPC to serve a query at some checkpoint in the recent past. Snapshots
//! preserve access to the state of the database at a point in time, they are ephemeral (stored in
//! memory), and database-wide (not per-column-family).
//!
//! It is the `Indexer`'s responsibility to coordinate writes across pipelines, to arrange for the
//! database to contain a consistent view of the data at checkpoints it should take a snapshot of.
//! To this end, the indexer only supports sequential pipelines (pipelines also update keys
//! in-place, which precludes out-of-order writes), but writes are buffered, post-commit to allow
//! pipelines to make progress on later while checkpoints while waiting for lagging pipelines to
//! reach the snapshot checkpoint.
//!
//! The indexer and RPC agree on a `Schema` which describes the key types, value types and options
//! for all column families to be set-up in the database.

use std::{path::Path, sync::Arc};

use config::{PipelineLayer, ServiceConfig};
use db::config::DbConfig;
use handlers::{object_by_owner::ObjectByOwner, object_by_type::ObjectByType};
use indexer::Indexer;
use prometheus::Registry;
use rpc::{state::State, RpcArgs, RpcService};
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
        pipeline: PipelineLayer {
            object_by_owner,
            object_by_type,
        },
        rpc,
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

    let state = State {
        store: indexer.store().clone(),
        config: Arc::new(rpc),
    };

    let rpc = RpcService::new(rpc_args, version, registry, cancel.child_token())
        .register_encoded_file_descriptor_set(proto::rpc::consistent::v1alpha::FILE_DESCRIPTOR_SET)
        .add_service(ConsistentServiceServer::new(state.clone()));

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
    add_sequential!(ObjectByType, object_by_type);

    let h_rpc = rpc.run().await?;
    let h_indexer = indexer.run().await?;

    Ok(tokio::spawn(async move {
        let (_, _) = futures::join!(h_rpc, h_indexer);
    }))
}
