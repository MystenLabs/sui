// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::sync::OnceLock;
use std::{path::Path, sync::Arc, time::Duration};

use anyhow::{anyhow, bail, Context as _};
use scoped_futures::ScopedBoxFuture;
use sui_indexer_alt_framework::store::{self, CommitterWatermark, Store as _};
use synchronizer::Queue;
use tokio::task::JoinHandle;

use crate::db::config::DbConfig;
use crate::db::{Db, Watermark};

use self::synchronizer::Synchronizer;

pub(crate) mod synchronizer;

/// Defines the schema for the database.
pub(crate) trait Schema: Sized {
    /// Configuration for this schema's column families (names and options).
    fn cfs() -> Vec<(&'static str, rocksdb::Options)>;

    /// Construct the Rust value that represents the schema's tables, given access to the database.
    /// It is expected to be a struct containing various `DbMap`s as fields.
    fn open(db: &Arc<Db>) -> anyhow::Result<Self>;
}

/// A wrapper around a rocksdb [`Db`] that implements the indexer framework's [`store::Store`]
/// interface.
pub(crate) struct Store<S>(Arc<Inner<S>>);

/// A connection to the store that supports reads, writes and watermarking.
pub(crate) struct Connection<'s, S> {
    pub store: &'s Store<S>,
    pub batch: rocksdb::WriteBatch,
    watermark: Option<(&'static str, Watermark)>,
}

/// The contents of the store.
struct Inner<S> {
    db: Arc<Db>,

    /// A rust representation of the column families in the database that we want to access from
    /// this store.
    schema: S,

    /// Access to a synchronizer queue per-pipeline. This is initialized when the store is given a
    /// [`Synchronizer`] to run, and is used to send writes to the database, associated with a
    /// pipeline.
    queue: OnceLock<Queue>,
}

impl<S: Schema> Store<S> {
    /// Create a new store with the database at a given `path`, configured by `config`.
    ///
    /// `snapshots` is the maximum number of consistent snapshots to keep in the database at one
    /// time, and `schema` controls which tables are opened on the database.
    pub(crate) fn open(
        path: impl AsRef<Path>,
        config: DbConfig,
        snapshots: u64,
    ) -> anyhow::Result<Self> {
        let db = Arc::new(
            Db::open(path, config.into(), snapshots as usize, S::cfs())
                .context("Failed to open database")?,
        );

        let schema = S::open(&db).context("Failed to open schema")?;

        Ok(Self(Arc::new(Inner {
            db,
            schema,
            queue: OnceLock::new(),
        })))
    }

    /// Access to the store's database.
    pub(crate) fn db(&self) -> &Arc<Db> {
        &self.0.db
    }

    /// Access to the store's schema.
    pub(crate) fn schema(&self) -> &S {
        &self.0.schema
    }

    /// Run the provided synchronizer, and register its queue with the store. This will fail if the
    /// store already has a synchronizer running.
    pub(crate) fn sync(&self, s: Synchronizer) -> anyhow::Result<JoinHandle<()>> {
        let (handle, queue) = s.run()?;
        self.0
            .queue
            .set(queue)
            .map_err(|_| anyhow!("Store already has synchronizer"))?;
        Ok(handle)
    }
}

#[async_trait::async_trait]
impl<S: Send + Sync + 'static> store::Store for Store<S> {
    type Connection<'s> = Connection<'s, S>;

    async fn connect(&self) -> anyhow::Result<Connection<'_, S>> {
        Ok(Connection {
            store: self,
            batch: rocksdb::WriteBatch::default(),
            watermark: None,
        })
    }
}

#[async_trait::async_trait]
impl<S: Send + Sync + 'static> store::TransactionalStore for Store<S> {
    async fn transaction<'a, R, F>(&self, f: F) -> anyhow::Result<R>
    where
        R: Send + 'a,
        F: Send + 'a,
        F: for<'r> FnOnce(&'r mut Connection<'_, S>) -> ScopedBoxFuture<'a, 'r, anyhow::Result<R>>,
    {
        let mut conn = self.connect().await?;
        let r = f(&mut conn).await?;

        let Some((pipeline, watermark)) = conn.watermark else {
            bail!("No watermark set during transaction");
        };

        self.0
            .queue
            .get()
            .context("Synchronizer not running for store")?
            .get(pipeline)
            .with_context(|| format!("No {pipeline:?} synchronizer queue"))?
            .send((watermark, conn.batch))
            .await
            .map_err(|_| anyhow!("{pipeline:?} synchronizer queue closed"))?;

        Ok(r)
    }
}

#[async_trait::async_trait]
impl<S: Send + Sync> store::Connection for Connection<'_, S> {
    async fn committer_watermark(
        &mut self,
        pipeline: &'static str,
    ) -> anyhow::Result<Option<CommitterWatermark>> {
        Ok(self.store.0.db.watermark(pipeline)?.map(Into::into))
    }

    async fn set_committer_watermark(
        &mut self,
        pipeline: &'static str,
        watermark: CommitterWatermark,
    ) -> anyhow::Result<bool> {
        self.watermark = Some((pipeline, watermark.into()));
        Ok(true)
    }

    async fn reader_watermark(
        &mut self,
        _pipeline: &'static str,
    ) -> anyhow::Result<Option<store::ReaderWatermark>> {
        Ok(None)
    }

    async fn pruner_watermark(
        &mut self,
        _pipeline: &'static str,
        _delay: Duration,
    ) -> anyhow::Result<Option<store::PrunerWatermark>> {
        Ok(None)
    }

    async fn set_reader_watermark(
        &mut self,
        _pipeline: &'static str,
        _reader_lo: u64,
    ) -> anyhow::Result<bool> {
        bail!("Pruning not supported by this store");
    }

    async fn set_pruner_watermark(
        &mut self,
        _pipeline: &'static str,
        _pruner_hi: u64,
    ) -> anyhow::Result<bool> {
        bail!("Pruning not supported by this store");
    }
}

impl<S> Clone for Store<S> {
    fn clone(&self) -> Self {
        Self(self.0.clone())
    }
}

#[cfg(test)]
mod tests {
    use std::future::Future;

    use scoped_futures::ScopedFutureExt;
    use sui_indexer_alt_framework::store::{Connection as _, TransactionalStore};
    use tokio::time::{self, error::Elapsed};
    use tokio_util::sync::CancellationToken;

    use crate::db::map::DbMap;

    use super::*;

    struct TestSchema {
        a: DbMap<String, u64>,
        b: DbMap<u64, String>,
    }

    impl Schema for TestSchema {
        fn cfs() -> Vec<(&'static str, rocksdb::Options)> {
            vec![
                ("a", rocksdb::Options::default()),
                ("b", rocksdb::Options::default()),
            ]
        }

        fn open(db: &Arc<Db>) -> anyhow::Result<Self> {
            Ok(Self {
                a: DbMap::new(db.clone(), "a"),
                b: DbMap::new(db.clone(), "b"),
            })
        }
    }

    async fn wait_until<F, R>(f: F) -> Result<(), Elapsed>
    where
        F: Fn() -> R,
        R: Future<Output = bool>,
    {
        time::timeout(Duration::from_millis(500), async move {
            let mut interval = time::interval(Duration::from_millis(100));
            loop {
                interval.tick().await;
                if f().await {
                    return;
                }
            }
        })
        .await
    }

    async fn write<M>(
        store: &Store<TestSchema>,
        pipeline: &'static str,
        cp: u64,
        mutator: M,
    ) -> anyhow::Result<()>
    where
        M: Send + 'static + FnOnce(&TestSchema, &mut rocksdb::WriteBatch) -> anyhow::Result<()>,
    {
        store
            .transaction(move |c| {
                async move {
                    mutator(c.store.schema(), &mut c.batch)?;
                    c.set_committer_watermark(pipeline, CommitterWatermark::new_for_testing(cp))
                        .await?;
                    Ok(())
                }
                .scope_boxed()
            })
            .await
    }

    #[tokio::test]
    async fn test_open() {
        let d = tempfile::tempdir().unwrap();
        let _store: Store<TestSchema> =
            Store::open(d.path().join("db"), DbConfig::default(), 4).unwrap();
    }

    #[tokio::test]
    async fn test_no_queue() {
        let d = tempfile::tempdir().unwrap();
        let store: Store<TestSchema> =
            Store::open(d.path().join("db"), DbConfig::default(), 4).unwrap();

        // If the store is not associated with a synchronizer, all writes will fail.
        let err = write(&store, "test", 0, |s, b| {
            s.a.insert("x".to_owned(), 42, b)?;
            s.b.insert(42, "x".to_owned(), b)?;
            Ok(())
        })
        .await
        .unwrap_err()
        .to_string();

        assert!(err.contains("Synchronizer not running for store"), "{err}");
    }

    #[tokio::test]
    async fn test_single_pipeline() {
        let d = tempfile::tempdir().unwrap();
        let store: Store<TestSchema> =
            Store::open(d.path().join("db"), DbConfig::default(), 4).unwrap();

        let stride = 1;
        let buffer_size = 10;
        let first_checkpoint = None;
        let cancel = CancellationToken::new();
        let mut sync = Synchronizer::new(
            store.db().clone(),
            stride,
            buffer_size,
            first_checkpoint,
            cancel.clone(),
        );

        sync.register_pipeline("test").unwrap();
        let h_sync = store.sync(sync).unwrap();

        write(&store, "test", 0, |s, b| {
            s.a.insert("x".to_owned(), 42, b)?;
            s.b.insert(42, "x".to_owned(), b)?;
            Ok(())
        })
        .await
        .unwrap();

        wait_until(|| async { store.db().snapshot_range().is_some_and(|s| s.end() == &0) })
            .await
            .unwrap();

        let s = store.schema();
        assert_eq!(s.a.get(0, "x".to_owned()).unwrap(), Some(42));
        assert_eq!(s.b.get(0, 42).unwrap(), Some("x".to_owned()));

        cancel.cancel();
        h_sync.await.unwrap();
    }

    #[tokio::test]
    async fn test_multiple_pipelines() {
        let d = tempfile::tempdir().unwrap();
        let store: Store<TestSchema> =
            Store::open(d.path().join("db"), DbConfig::default(), 4).unwrap();

        let stride = 1;
        let buffer_size = 10;
        let first_checkpoint = None;
        let cancel = CancellationToken::new();
        let mut sync = Synchronizer::new(
            store.db().clone(),
            stride,
            buffer_size,
            first_checkpoint,
            cancel.clone(),
        );

        sync.register_pipeline("a").unwrap();
        sync.register_pipeline("b").unwrap();
        let h_sync = store.sync(sync).unwrap();

        write(&store, "a", 0, |s, b| {
            s.a.insert("x".to_owned(), 42, b)?;
            Ok(())
        })
        .await
        .unwrap();

        // There are two pipelines, so the synchronizer will not take a snapshot until both have
        // been written to.
        wait_until(|| async { store.db().snapshot_range().is_some() })
            .await
            .unwrap_err();

        write(&store, "b", 0, |s, b| {
            s.b.insert(42, "x".to_owned(), b)?;
            Ok(())
        })
        .await
        .unwrap();

        wait_until(|| async { store.db().snapshot_range().is_some_and(|s| s.end() == &0) })
            .await
            .unwrap();

        let s = store.schema();
        assert_eq!(s.a.get(0, "x".to_owned()).unwrap(), Some(42));
        assert_eq!(s.b.get(0, 42).unwrap(), Some("x".to_owned()));

        cancel.cancel();
        h_sync.await.unwrap();
    }

    #[tokio::test]
    async fn test_single_pipeline_existing() {
        let d = tempfile::tempdir().unwrap();
        let snapshots = 4;

        {
            // Initialize the database with some data for the pipeline
            let db = Arc::new(
                Db::open(
                    d.path().join("db"),
                    DbConfig::default().into(),
                    snapshots as usize,
                    TestSchema::cfs(),
                )
                .unwrap(),
            );

            let schema = TestSchema::open(&db).unwrap();

            let mut batch = rocksdb::WriteBatch::default();
            schema.b.insert(42, "x".to_owned(), &mut batch).unwrap();
            db.write("b", CommitterWatermark::new_for_testing(0).into(), batch)
                .unwrap();
        }

        let store: Store<TestSchema> =
            Store::open(d.path().join("db"), DbConfig::default(), snapshots).unwrap();

        let stride = 1;
        let buffer_size = 10;
        let first_checkpoint = None;
        let cancel = CancellationToken::new();
        let mut sync = Synchronizer::new(
            store.db().clone(),
            stride,
            buffer_size,
            first_checkpoint,
            cancel.clone(),
        );

        sync.register_pipeline("b").unwrap();
        let h_sync = store.sync(sync).unwrap();

        // When there is existing data, the synchronizer will take a snapshot to make it available
        // before the store sees any writes.
        wait_until(|| async { store.db().snapshot_range().is_some_and(|s| s.end() == &0) })
            .await
            .unwrap();

        cancel.cancel();
        h_sync.await.unwrap();
    }

    #[tokio::test]
    async fn test_multiple_pipeline_existing() {
        let d = tempfile::tempdir().unwrap();
        let snapshots: u64 = 4;

        {
            // Initialize the database with some data for both pipelines
            let db = Arc::new(
                Db::open(
                    d.path().join("db"),
                    DbConfig::default().into(),
                    snapshots as usize,
                    TestSchema::cfs(),
                )
                .unwrap(),
            );

            let schema = TestSchema::open(&db).unwrap();

            let mut batch = rocksdb::WriteBatch::default();
            schema.a.insert("x".to_owned(), 42, &mut batch).unwrap();
            db.write("a", CommitterWatermark::new_for_testing(0).into(), batch)
                .unwrap();

            let mut batch = rocksdb::WriteBatch::default();
            schema.b.insert(42, "x".to_owned(), &mut batch).unwrap();
            db.write("b", CommitterWatermark::new_for_testing(0).into(), batch)
                .unwrap();
        }

        let store: Store<TestSchema> =
            Store::open(d.path().join("db"), DbConfig::default(), snapshots).unwrap();

        let stride = 1;
        let buffer_size = 10;
        let first_checkpoint = None;
        let cancel = CancellationToken::new();
        let mut sync = Synchronizer::new(
            store.db().clone(),
            stride,
            buffer_size,
            first_checkpoint,
            cancel.clone(),
        );

        sync.register_pipeline("a").unwrap();
        sync.register_pipeline("b").unwrap();
        let h_sync = store.sync(sync).unwrap();

        // When there is existing data, the synchronizer will take a snapshot to make it available
        // before the store sees any writes.
        wait_until(|| async { store.db().snapshot_range().is_some_and(|s| s.end() == &0) })
            .await
            .unwrap();

        cancel.cancel();
        h_sync.await.unwrap();
    }

    #[tokio::test]
    async fn test_multiple_pipeline_catchup() {
        let d = tempfile::tempdir().unwrap();
        let snapshots = 4;

        {
            // Initialize the database with some data for one of the pipelines.
            let db = Arc::new(
                Db::open(
                    d.path().join("db"),
                    DbConfig::default().into(),
                    snapshots,
                    TestSchema::cfs(),
                )
                .unwrap(),
            );

            let schema = TestSchema::open(&db).unwrap();

            let mut batch = rocksdb::WriteBatch::default();
            schema.b.insert(42, "x".to_owned(), &mut batch).unwrap();
            db.write("b", CommitterWatermark::new_for_testing(0).into(), batch)
                .unwrap();
        }

        let store: Store<TestSchema> =
            Store::open(d.path().join("db"), DbConfig::default(), 4).unwrap();

        let stride = 1;
        let buffer_size = 10;
        let first_checkpoint = None;
        let cancel = CancellationToken::new();
        let mut sync = Synchronizer::new(
            store.db().clone(),
            stride,
            buffer_size,
            first_checkpoint,
            cancel.clone(),
        );

        sync.register_pipeline("a").unwrap();
        sync.register_pipeline("b").unwrap();
        let h_sync = store.sync(sync).unwrap();

        // The pipelines are not in sync to begin with, so the synchronizer is waiting for the
        // writes for the other pipeline in order to take a snapshot.
        wait_until(|| async { store.db().snapshot_range().is_some() })
            .await
            .unwrap_err();

        // Further writes to the pipeline that is ahead will be held back.
        write(&store, "b", 1, |s, b| {
            s.b.insert(42, "y".to_owned(), b)?;
            Ok(())
        })
        .await
        .unwrap();

        // Further writes to the pipeline that is ahead will be held back.
        wait_until(|| async { store.db().snapshot_range().is_some() })
            .await
            .unwrap_err();

        write(&store, "a", 0, |s, b| {
            s.a.insert("x".to_owned(), 42, b)?;
            Ok(())
        })
        .await
        .unwrap();

        // After the other pipeline was caught up, the synchronizer will take the snapshot, but it
        // will not yet make the subsequent write to the other pipeline available.
        wait_until(|| async { store.db().snapshot_range().is_some_and(|s| s.end() == &0) })
            .await
            .unwrap();

        let s = store.schema();
        assert_eq!(s.a.get(0, "x".to_owned()).unwrap(), Some(42));
        assert_eq!(s.b.get(0, 42).unwrap(), Some("x".to_owned()));

        // Catch up the first pipeline without writing any further data.
        write(&store, "a", 1, |_, _| Ok(())).await.unwrap();
        wait_until(|| async { store.db().snapshot_range().is_some_and(|s| s.end() == &1) })
            .await
            .unwrap();

        let s = store.schema();
        assert_eq!(s.a.get(1, "x".to_owned()).unwrap(), Some(42));
        assert_eq!(s.b.get(1, 42).unwrap(), Some("y".to_owned()));

        cancel.cancel();
        h_sync.await.unwrap();
    }

    #[tokio::test]
    async fn test_missing_pipeline() {
        let d = tempfile::tempdir().unwrap();

        let store: Store<TestSchema> =
            Store::open(d.path().join("db"), DbConfig::default(), 4).unwrap();

        let stride = 1;
        let buffer_size = 10;
        let first_checkpoint = None;
        let cancel = CancellationToken::new();
        let sync = Synchronizer::new(
            store.db().clone(),
            stride,
            buffer_size,
            first_checkpoint,
            cancel.clone(),
        );

        let h_sync = store.sync(sync).unwrap();

        let err = write(&store, "test", 0, |_, _| Ok(()))
            .await
            .unwrap_err()
            .to_string();

        // If pipelines are not registered with the synchronizer before it is associated with the
        // store, writes to them will fail.
        assert!(err.contains("No \"test\" synchronizer queue"), "{err}");

        cancel.cancel();
        h_sync.await.unwrap();
    }

    #[tokio::test]
    async fn test_first_checkpoint() {
        let d = tempfile::tempdir().unwrap();

        let store: Store<TestSchema> =
            Store::open(d.path().join("db"), DbConfig::default(), 4).unwrap();

        let stride = 1;
        let buffer_size = 10;
        let first_checkpoint = Some(100);
        let cancel = CancellationToken::new();
        let mut sync = Synchronizer::new(
            store.db().clone(),
            stride,
            buffer_size,
            first_checkpoint,
            cancel.clone(),
        );

        sync.register_pipeline("test").unwrap();
        let h_sync = store.sync(sync).unwrap();

        write(&store, "test", 100, |s, b| {
            s.a.insert("x".to_owned(), 42, b)?;
            s.b.insert(42, "x".to_owned(), b)?;
            Ok(())
        })
        .await
        .unwrap();

        // The synchronizer will take a snapshot for the checkpoint before the first, if the first
        // checkpoint is not 0.
        wait_until(|| async {
            store
                .db()
                .snapshot_range()
                .is_some_and(|s| s.start() == &99 && s.end() == &100)
        })
        .await
        .unwrap();

        let s = store.schema();
        assert_eq!(s.a.get(99, "x".to_owned()).unwrap(), None);
        assert_eq!(s.b.get(99, 42).unwrap(), None);
        assert_eq!(s.a.get(100, "x".to_owned()).unwrap(), Some(42));
        assert_eq!(s.b.get(100, 42).unwrap(), Some("x".to_owned()));

        cancel.cancel();
        h_sync.await.unwrap();
    }

    #[tokio::test]
    async fn test_stride() {
        let d = tempfile::tempdir().unwrap();

        let store: Store<TestSchema> =
            Store::open(d.path().join("db"), DbConfig::default(), 4).unwrap();

        let stride = 3;
        let buffer_size = 10;
        let first_checkpoint = None;
        let cancel = CancellationToken::new();
        let mut sync = Synchronizer::new(
            store.db().clone(),
            stride,
            buffer_size,
            first_checkpoint,
            cancel.clone(),
        );

        sync.register_pipeline("test").unwrap();
        let h_sync = store.sync(sync).unwrap();

        // Write a run of checkpoints.
        for cp in 0..=10 {
            write(&store, "test", cp, move |s, b| {
                s.a.insert("x".to_owned(), cp * 3, b)?;
                s.b.insert(cp * 3, "x".to_owned(), b)?;
                Ok(())
            })
            .await
            .unwrap();
        }

        // The synchronizer will take a snapshot before every `stride`-th checkpoint.
        wait_until(|| async {
            store
                .db()
                .snapshot_range()
                .is_some_and(|s| s.start() == &2 && s.end() == &8)
        })
        .await
        .unwrap();

        let s = store.schema();
        assert_eq!(store.db().snapshots(), 3);
        for cp in (2..10).step_by(stride as usize) {
            assert_eq!(s.a.get(cp, "x".to_owned()).unwrap(), Some(cp * 3));
            assert_eq!(s.b.get(cp, cp * 3).unwrap(), Some("x".to_owned()));
        }

        cancel.cancel();
        h_sync.await.unwrap();
    }

    #[tokio::test]
    async fn test_no_watermark() {
        let d = tempfile::tempdir().unwrap();

        let store: Store<TestSchema> =
            Store::open(d.path().join("db"), DbConfig::default(), 4).unwrap();

        let stride = 1;
        let buffer_size = 10;
        let first_checkpoint = Some(100);
        let cancel = CancellationToken::new();
        let mut sync = Synchronizer::new(
            store.db().clone(),
            stride,
            buffer_size,
            first_checkpoint,
            cancel.clone(),
        );

        sync.register_pipeline("test").unwrap();
        let h_sync = store.sync(sync).unwrap();

        let err = store
            .transaction(|c| {
                async move {
                    // The transaction does not set a watermark, so the write should fail.
                    c.store
                        .schema()
                        .a
                        .insert("x".to_owned(), 42, &mut c.batch)?;
                    Ok(())
                }
                .scope_boxed()
            })
            .await
            .unwrap_err()
            .to_string();

        assert!(err.contains("No watermark set during transaction"), "{err}");

        cancel.cancel();
        h_sync.await.unwrap();
    }

    #[tokio::test]
    async fn test_out_of_order_batch() {
        let d = tempfile::tempdir().unwrap();

        let store: Store<TestSchema> =
            Store::open(d.path().join("db"), DbConfig::default(), 4).unwrap();

        let stride = 1;
        let buffer_size = 10;
        let first_checkpoint = None;
        let cancel = CancellationToken::new();
        let mut sync = Synchronizer::new(
            store.db().clone(),
            stride,
            buffer_size,
            first_checkpoint,
            cancel.clone(),
        );

        sync.register_pipeline("test").unwrap();
        let h_sync = store.sync(sync).unwrap();

        write(&store, "test", 0, |s, b| {
            s.a.insert("x".to_owned(), 42, b)?;
            Ok(())
        })
        .await
        .unwrap();

        write(&store, "test", 10, |s, b| {
            s.a.insert("y".to_owned(), 43, b)?;
            Ok(())
        })
        .await
        .unwrap();

        // The out of order batch will appear to succeed, but the synchronizer will detect the
        // situation and stop gracefully.
        time::timeout(Duration::from_millis(500), h_sync)
            .await
            .unwrap()
            .unwrap();

        // The first write made it through, but the second one did not.
        let s = store.schema();
        let db = store.db();
        assert_eq!(db.snapshots(), 1);
        assert_eq!(db.snapshot_range().map(|s| *s.end()), Some(0));
        assert_eq!(s.a.get(0, "x".to_owned()).unwrap(), Some(42));
        assert_eq!(s.a.get(0, "y".to_owned()).unwrap(), None);
    }
}
