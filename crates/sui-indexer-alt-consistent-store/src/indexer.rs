// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::path::Path;

use anyhow::{Context as _, ensure};
use prometheus::Registry;
use sui_indexer_alt_framework::{
    self as framework, IndexerArgs,
    ingestion::{ClientArgs, IngestionConfig},
    pipeline::sequential::{self, SequentialConfig},
    service::Service,
};

use crate::{
    config::ConsistencyConfig,
    db::config::DbConfig,
    store::{Schema, Store, synchronizer::Synchronizer},
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
        );

        let metrics_prefix = Some("consistent_indexer");
        let indexer = framework::Indexer::new(
            store,
            indexer_args,
            client_args,
            ingestion_config,
            metrics_prefix,
            registry,
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
        let is_restoring = self
            .store()
            .db()
            .restore_watermark(H::NAME)
            .with_context(|| format!("Bad restore watermark for pipeline {:?}", H::NAME))?
            .is_some();

        ensure!(
            !is_restoring,
            "Restoration in progress for pipeline {:?}",
            H::NAME
        );

        // TODO: Refactor consistent store indexer to use `init_watermark` instead of wrapping `sequential_pipeline`.
        self.sync
            .register_pipeline(H::NAME)
            .with_context(|| format!("Failed to add pipeline {:?} to synchronizer", H::NAME))?;

        self.indexer
            .sequential_pipeline(handler, config)
            .await
            .with_context(|| format!("Failed to add pipeline {:?} to indexer", H::NAME))?;

        Ok(())
    }

    /// Start ingesting checkpoints, consuming the indexer in the process.
    ///
    /// See [`framework::Indexer::run`] for details.
    pub(crate) async fn run(self) -> anyhow::Result<Service> {
        // Associate the indexer's store with the synchronizer. This spins up a separate task for
        // each pipeline that was registered, and installs the write queues that talk to those
        // tasks into the store, so that when a write arrives to the store for a particular
        // pipeline, it can make its way to the right task.
        let s_sync = self.indexer.store().sync(self.sync)?;
        let s_indexer = self.indexer.run().await?;

        Ok(s_indexer.attach(s_sync))
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use sui_indexer_alt_framework::{
        ingestion::ingestion_client::IngestionClientArgs,
        pipeline::Processor,
        types::{full_checkpoint_content::Checkpoint, object::Object},
    };

    use crate::{
        db::{Db, tests::wm},
        restore::Restore,
        store::Connection,
    };

    use super::*;

    /// A handler that never indexes or restores any data.
    struct TestHandler;
    struct TestSchema;

    #[async_trait::async_trait]
    impl Processor for TestHandler {
        const NAME: &'static str = "test";
        type Value = ();

        async fn process(&self, _: &Arc<Checkpoint>) -> anyhow::Result<Vec<Self::Value>> {
            Ok(vec![])
        }
    }

    impl Restore<TestSchema> for TestHandler {
        fn restore(_: &TestSchema, _: &Object, _: &mut rocksdb::WriteBatch) -> anyhow::Result<()> {
            Ok(())
        }
    }

    #[async_trait::async_trait]
    impl sequential::Handler for TestHandler {
        type Store = Store<TestSchema>;
        type Batch = ();

        fn batch(&self, _: &mut (), _: std::vec::IntoIter<()>) {}

        async fn commit<'a>(
            &self,
            _: &(),
            _: &mut Connection<'a, TestSchema>,
        ) -> anyhow::Result<usize> {
            Ok(0)
        }
    }

    impl Schema for TestSchema {
        fn cfs(_: &rocksdb::Options) -> Vec<(&'static str, rocksdb::Options)> {
            vec![("test", rocksdb::Options::default())]
        }

        fn open(_: &Arc<Db>) -> anyhow::Result<Self> {
            Ok(Self)
        }
    }

    #[tokio::test]
    async fn test_restore_protection() {
        let d = tempfile::tempdir().unwrap();
        let mut opts = rocksdb::Options::default();
        opts.create_if_missing(true);
        let cfs = TestSchema::cfs(&opts);

        {
            // Start restoring a pipeline.
            let db = Db::open(d.path().join("db"), opts.clone(), 0, cfs.clone()).unwrap();
            db.restore_at("test", wm(10)).unwrap();
        }

        {
            // If the pipeline is being restored, then the indexer will not allow it to be added.
            let mut indexer = Indexer::<TestSchema>::new(
                d.path().join("db"),
                IndexerArgs::default(),
                ClientArgs {
                    ingestion: IngestionClientArgs {
                        local_ingestion_path: Some(d.path().join("checkpoints")),
                        ..Default::default()
                    },
                    ..Default::default()
                },
                ConsistencyConfig::default(),
                IngestionConfig::default(),
                DbConfig::default(),
                &prometheus::Registry::new(),
            )
            .await
            .unwrap();

            indexer
                .sequential_pipeline(TestHandler, SequentialConfig::default())
                .await
                .unwrap_err();
        }

        {
            // Indicate that the restoration has completed.
            let db = Db::open(d.path().join("db"), opts.clone(), 0, cfs.clone()).unwrap();
            db.complete_restore("test").unwrap();
        }

        {
            // Now the indexer will allow the pipeline to be added.
            let mut indexer = Indexer::<TestSchema>::new(
                d.path().join("db"),
                IndexerArgs::default(),
                ClientArgs {
                    ingestion: IngestionClientArgs {
                        local_ingestion_path: Some(d.path().join("checkpoints")),
                        ..Default::default()
                    },
                    ..Default::default()
                },
                ConsistencyConfig::default(),
                IngestionConfig::default(),
                DbConfig::default(),
                &prometheus::Registry::new(),
            )
            .await
            .unwrap();

            indexer
                .sequential_pipeline(TestHandler, SequentialConfig::default())
                .await
                .unwrap();
        }
    }
}
