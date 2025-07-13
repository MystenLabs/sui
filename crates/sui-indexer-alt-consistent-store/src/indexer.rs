// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
#![allow(dead_code)]

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

/// An indexer specialised for writing to a RocksDB store via a schema, `S`.
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
        let store = Store::open(path, db_config, consistency_config.snapshots)
            .context("Failed to create store")?;

        let sync = Synchronizer::new(
            store.db().clone(),
            consistency_config.stride,
            consistency_config.buffer_size,
            indexer_args.first_checkpoint,
            cancel.clone(),
        );

        let indexer = framework::Indexer::new(
            store,
            indexer_args,
            client_args,
            ingestion_config,
            registry,
            cancel.clone(),
        )
        .await
        .context("Failed to create indexer")?;

        Ok(Self { indexer, sync })
    }

    /// Adds a new sequential pipeline to the indexer and starts it up. See
    /// [`framework::Indexer::sequential_pipeline`] for details.
    ///
    /// Unlike the generic indexer, this indexer only supports sequential pipelines, because the
    /// underlying store only works with transactional writes, and does not support pruning.
    pub(crate) async fn sequential_pipeline<H>(
        &mut self,
        handler: H,
        config: SequentialConfig,
    ) -> anyhow::Result<()>
    where
        H: sequential::Handler<Store = Store<S>> + Send + Sync + 'static,
    {
        self.sync
            .add_pipeline(H::NAME)
            .context("Failed to add pipeline to synchronizer")?;
        self.indexer.sequential_pipeline(handler, config).await
    }

    /// Start ingesting checkpoints. See [`framework::Indexer::run`] for details.
    pub(crate) async fn run(self) -> anyhow::Result<JoinHandle<()>> {
        let h_sync = self.indexer.store().sync(self.sync)?;
        let h_indexer = self.indexer.run();
        Ok(tokio::spawn(async move {
            let (_, _) = futures::join!(h_sync, h_indexer);
        }))
    }
}
