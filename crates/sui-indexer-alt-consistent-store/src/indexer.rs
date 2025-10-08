// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::path::Path;

use anyhow::Context as _;
use prometheus::Registry;
use sui_indexer_alt_framework::{
    self as framework,
    ingestion::{ClientArgs, IngestionConfig},
    pipeline::sequential::{self, SequentialConfig},
    IndexerArgs,
};
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;

use crate::{
    config::ConsistencyConfig,
    db::config::DbConfig,
    store::{synchronizer::Synchronizer, Schema, Store},
};

/// An indexer specialised for writing to a RocksDB store via a schema, `S`, composed of three main
/// components:
///
/// - A [`framework::Indexer`], from the indexing framework. Only sequential pipelines are exposed
///   because the synchronizer requires writes to come in checkpoint order, and to be associated
///   with their checkpoint, so that it can line up all pipelines at the same checkpoint before
///   taking a snapshot.
///
/// - Access to RocksDB via a [`Store<S>`]. Its type parameter, `S`, describes the type-safe schema
///   of the database (the types of keys and values in each column family). Pipelines use maps in
///   the schema described by `S` to serialize data into writes for the database.
///
/// - A [`Synchronizer`], which coordinates taking database-wide snapshots with writes coming in
///   from the various pipelines.
///
/// When a pipeline performs a write for a checkpoint, the data for that checkpoint is bundled with
/// a watermark update, into an atomic write for the database. This write is sent down a channel to
/// a synchronizer task which decides whether to perform the write immediately, or wait because it
/// belongs in the next snapshot.
pub(crate) struct Indexer<S: Schema + Send + Sync + 'static> {
    indexer: framework::Indexer<Store<S>>,

    /// The synchronizer coordinates writes between pipelines to the same underlying database, and
    /// snapshots of that database.
    sync: Synchronizer,
}

impl<S: Schema + Send + Sync + 'static> Indexer<S> {
    /// Creates a new instance of the indexer, writing to a store whose database is at `path`, and
    /// is configured by `db_config`.
    ///
    /// See [`framework::Indexer::new`] for details on the other arguments.
    pub(crate) async fn new(
        path: impl AsRef<Path>,
        indexer_args: IndexerArgs,
        client_args: ClientArgs,
        consistency_config: ConsistencyConfig,
        ingestion_config: IngestionConfig,
        db_config: DbConfig,
        registry: &Registry,
        cancel: CancellationToken,
    ) -> anyhow::Result<Self> {
        let store = Store::open(
            path,
            db_config,
            consistency_config.snapshots,
            Some(registry),
        )
        .context("Failed to create store")?;

        let sync = Synchronizer::new(
            store.db().clone(),
            consistency_config.stride,
            consistency_config.buffer_size,
            indexer_args.first_checkpoint,
            cancel.child_token(),
        );

        let metrics_prefix = Some("consistent_indexer");
        let indexer = framework::Indexer::new(
            store,
            indexer_args,
            client_args,
            ingestion_config,
            metrics_prefix,
            registry,
            cancel.child_token(),
        )
        .await
        .context("Failed to create indexer")?;

        Ok(Self { indexer, sync })
    }

    pub(crate) fn store(&self) -> &Store<S> {
        self.indexer.store()
    }

    /// Adds a new sequential pipeline to the indexer and starts it up. See
    /// [`framework::Indexer::sequential_pipeline`] for details.
    pub(crate) async fn sequential_pipeline<H>(
        &mut self,
        handler: H,
        config: SequentialConfig,
    ) -> anyhow::Result<()>
    where
        H: sequential::Handler<Store = Store<S>> + Send + Sync + 'static,
    {
        self.sync
            .register_pipeline(H::NAME)
            .context("Failed to add pipeline to synchronizer")?;
        self.indexer.sequential_pipeline(handler, config).await
    }

    /// Start ingesting checkpoints, consuming the indexer in the process.
    ///
    /// See [`framework::Indexer::run`] for details.
    pub(crate) async fn run(self) -> anyhow::Result<JoinHandle<()>> {
        // Associate the indexer's store with the synchronizer. This spins up a separate task for
        // each pipeline that was registered, and installs the write queues that talk to those
        // tasks into the store, so that when a write arrives to the store for a particular
        // pipeline, it can make its way to the right task.
        let h_sync = self.indexer.store().sync(self.sync)?;
        let h_indexer = self.indexer.run();
        Ok(tokio::spawn(async move {
            let (_, _) = futures::join!(h_sync, h_indexer);
        }))
    }
}
