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

use std::path::Path;

use config::{PipelineLayer, ServiceConfig};
use db::config::DbConfig;
use handlers::object_by_owner::ObjectByOwner;
use indexer::Indexer;
use prometheus::Registry;
use schema::Schema;
use sui_indexer_alt_framework::{
    ingestion::ClientArgs, pipeline::sequential::SequentialConfig, pipeline::CommitterConfig,
    IndexerArgs,
};
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;

mod config;
mod db;
mod handlers;
mod indexer;
pub(crate) mod schema;
mod store;

pub async fn start_service(
    path: impl AsRef<Path>,
    indexer_args: IndexerArgs,
    client_args: ClientArgs,
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
        cancel,
    )
    .await?;

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

    indexer.run().await
}
