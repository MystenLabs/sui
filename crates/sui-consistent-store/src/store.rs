// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! [`Store`] and [`Connection`] — the indexer-alt-framework
//! handles that satisfy
//! [`sui_indexer_alt_framework_store_traits`]'s `Store` /
//! `Connection` / `SequentialStore` / `SequentialConnection`.
//!
//! The store wraps a triple — a [`Db`] handle, an owned
//! [`FrameworkSchema`] holding the bookkeeping watermark and
//! chain-id CFs, and an `Arc<S>` holding the consumer's own
//! schema. The connection type owns a pending [`Batch`] plus the
//! watermark write deferred until
//! [`transaction`](sui_indexer_alt_framework_store_traits::SequentialStore::transaction)
//! commits.
//!
//! # Atomicity model
//!
//! `transaction(closure)` runs the closure with `&mut Connection`,
//! lets it stage typed writes on `conn.batch`, observes
//! `set_committer_watermark` calls (which are deferred, not
//! committed eagerly), then writes the watermark to the framework
//! schema and commits the batch in one atomic
//! [`Batch::commit`](crate::Batch::commit). The
//! consumer pipeline's data writes and the watermark advance
//! become visible together or not at all.
//!
//! `accepts_chain_id` is special: it runs outside any transaction
//! (the framework calls it on a fresh `Connection` during
//! processor init) and so it cannot defer through the pending
//! batch. On a fresh pipeline it writes a one-shot commit to
//! record the chain id; on a returning pipeline it just reads and
//! compares.
//!
//! # Synchronizer integration
//!
//! When [`install_sync`](Store::install_sync) has been called,
//! [`transaction`](SequentialStore::transaction) routes each
//! pipeline's `(Watermark, Batch)` pair through the corresponding
//! per-pipeline queue owned by the [`Synchronizer`](crate::synchronizer);
//! the synchronizer commits the batch and coordinates with peer
//! pipelines at stride boundaries to take cross-pipeline
//! snapshots. With no synchronizer installed, transactions commit
//! inline.
//!
//! With a synchronizer installed, the pipeline's
//! `sequential::Handler` impl **must** set
//! `MAX_BATCH_CHECKPOINTS = 1`: the synchronizer requires each
//! batch shipped through its queue to correspond to exactly one
//! checkpoint. Folding multiple checkpoints into one batch will
//! trip the synchronizer's out-of-order check on the first
//! multi-checkpoint batch and shut the task down.

use std::sync::Arc;
use std::sync::OnceLock;

use anyhow::Context as _;
use anyhow::anyhow;
use anyhow::bail;
use async_trait::async_trait;
use scoped_futures::ScopedBoxFuture;
use sui_indexer_alt_framework_store_traits::CommitterWatermark;
use sui_indexer_alt_framework_store_traits::InitWatermark;
use sui_indexer_alt_framework_store_traits::SequentialConnection;
use sui_indexer_alt_framework_store_traits::SequentialStore;
use sui_indexer_alt_framework_store_traits::Store as _;
use sui_indexer_alt_framework_store_traits::{self as store_traits};
use tokio::task::JoinSet;

use crate::Batch;
use crate::ChainId;
use crate::Db;
use crate::FrameworkSchema;
use crate::PipelineTaskKey;
use crate::Watermark;
use crate::committer_watermark;
use crate::synchronizer::Queue;
use crate::synchronizer::Synchronizer;

/// Framework-side wrapper around a [`Db`] handle plus a
/// consumer-supplied user schema `S`.
///
/// `Store<S>` is `Clone` (cheap, [`Arc`]-backed) so the framework
/// can hand it out to every pipeline's processor task. The
/// auto-registered [`FrameworkSchema`] (watermarks, chain ids,
/// restore state) is cached internally so writes through
/// [`SequentialStore::transaction`] do not pay a re-construction
/// cost; reads are exposed via [`Db::framework`] on the
/// underlying handle.
pub struct Store<S> {
    inner: Arc<Inner<S>>,
}

struct Inner<S> {
    db: Db,
    /// Owned, cached framework schema. Holds the typed
    /// [`DbMap`](crate::DbMap)s the connection
    /// uses for watermark and chain-id writes. Identical to
    /// `db.framework()` but owned (no `Arc` bumps per access).
    framework: FrameworkSchema,
    user: Arc<S>,
    /// Per-pipeline write queues, set once when
    /// [`Store::install_sync`] runs a [`Synchronizer`]. If
    /// `Some`, [`SequentialStore::transaction`] routes through
    /// the queue instead of committing inline; if `None`, the
    /// transaction commits inline (single-pipeline mode).
    queue: OnceLock<Queue>,
}

impl<S> Store<S> {
    /// Build a store from a [`Db`] and a consumer-supplied schema.
    ///
    /// The framework's own bookkeeping CFs (watermarks, chain
    /// ids, restore state) are auto-registered by
    /// [`Db::open`](crate::Db::open), so the
    /// caller does not pass them in — the store constructs an
    /// owned [`FrameworkSchema`] from `db` internally.
    pub fn new(db: Db, user: Arc<S>) -> Self {
        let framework = FrameworkSchema::new(db.clone());
        Self {
            inner: Arc::new(Inner {
                db,
                framework,
                user,
                queue: OnceLock::new(),
            }),
        }
    }

    /// Run the supplied [`Synchronizer`] and register its
    /// per-pipeline write queues with this store.
    ///
    /// After this call returns,
    /// [`SequentialStore::transaction`] no longer commits inline:
    /// each transaction ships its `(Watermark, Batch)` pair through
    /// the appropriate pipeline's queue, where the synchronizer
    /// task commits it and coordinates with peer pipelines at
    /// stride boundaries to take cross-pipeline snapshots.
    ///
    /// Returns the [`JoinSet`] driving the synchronizer's tasks.
    /// Dropping the returned [`JoinSet`] does **not** stop the
    /// tasks immediately — the framework's standard shutdown
    /// path is to close every pipeline's mpsc sender (by dropping
    /// the queue inside the store, which happens on the store's
    /// last [`Arc`] drop), at which point each task observes a
    /// closed receiver and exits.
    ///
    /// Errors:
    /// - If a synchronizer is already installed on this store.
    pub fn install_sync(&self, sync: Synchronizer) -> anyhow::Result<JoinSet<anyhow::Result<()>>> {
        let (join_set, queue) = sync.run()?;
        self.inner
            .queue
            .set(queue)
            .map_err(|_| anyhow!("synchronizer already installed on this store"))?;
        Ok(join_set)
    }

    /// Borrow the underlying [`Db`] handle. Use
    /// [`Db::framework`](crate::Db::framework) on
    /// the returned handle for borrowed access to the framework
    /// schema.
    pub fn db(&self) -> &Db {
        &self.inner.db
    }

    /// Borrow the consumer pipeline's schema.
    pub fn schema(&self) -> &S {
        &self.inner.user
    }
}

impl<S> Clone for Store<S> {
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
        }
    }
}

/// A connection to the store that satisfies the framework's
/// `Connection` / `SequentialConnection` traits.
///
/// Holds a pending [`Batch`] that pipelines stage typed writes on,
/// plus an optional deferred watermark write that
/// [`SequentialStore::transaction`] applies at commit time.
///
/// `store` and `batch` are both public so closures running inside
/// [`SequentialStore::transaction`] can borrow them disjointly —
/// `c.store.schema()` is an immutable borrow of `c.store` while
/// `c.batch` is a mutable borrow of `c.batch`. They are different
/// fields so the two borrows do not conflict.
pub struct Connection<'s, S> {
    /// The store this connection was opened from. Borrowed
    /// immutably; callers reach through it to access the user
    /// schema and the framework schema.
    pub store: &'s Store<S>,
    /// Pending typed writes. Pipelines stage operations here
    /// during [`SequentialStore::transaction`]'s closure body;
    /// [`transaction`](SequentialStore::transaction) commits this
    /// batch atomically with the watermark write.
    pub batch: Batch,
    /// Deferred watermark update set by
    /// [`set_committer_watermark`](store_traits::Connection::set_committer_watermark).
    /// Applied by [`transaction`](SequentialStore::transaction).
    watermark: Option<(String, Watermark)>,
}

#[async_trait]
impl<S: Send + Sync + 'static> store_traits::Store for Store<S> {
    type Connection<'s> = Connection<'s, S>;

    async fn connect(&self) -> anyhow::Result<Connection<'_, S>> {
        Ok(Connection {
            store: self,
            batch: self.inner.db.batch(),
            watermark: None,
        })
    }
}

#[async_trait]
impl<S: Send + Sync + 'static> SequentialStore for Store<S> {
    type SequentialConnection<'c> = Connection<'c, S>;

    async fn transaction<'a, R, F>(&self, f: F) -> anyhow::Result<R>
    where
        R: Send + 'a,
        F: Send + 'a,
        F: for<'r> FnOnce(&'r mut Connection<'_, S>) -> ScopedBoxFuture<'a, 'r, anyhow::Result<R>>,
    {
        let mut conn = self.connect().await?;
        let r = f(&mut conn).await?;

        let Some((pipeline_task, watermark)) = conn.watermark.take() else {
            bail!("No watermark set during transaction");
        };

        // Stage the watermark into the same batch as the user's
        // data writes so the data and the watermark advance commit
        // atomically — either both visible or neither.
        let key = PipelineTaskKey::new(pipeline_task.clone());
        conn.batch
            .put(&self.inner.framework.watermarks, &key, &watermark)
            .context("staging framework watermark")?;

        if let Some(queue) = self.inner.queue.get() {
            // Synchronizer mode: route the batch through the
            // pipeline's per-task queue. The synchronizer commits
            // it and coordinates the cross-pipeline snapshot
            // cadence. The queue is keyed by `&'static str`; the
            // lookup resolves via `Borrow<str>` so passing the
            // String's slice (rather than the static name we
            // registered with) Just Works.
            let sender = queue.get(pipeline_task.as_str()).with_context(|| {
                format!("pipeline {pipeline_task} not registered with the synchronizer")
            })?;
            sender
                .send((watermark, conn.batch))
                .await
                .map_err(|_| anyhow!("{pipeline_task} synchronizer queue closed"))?;
        } else {
            // No synchronizer installed: commit inline. This is
            // the single-pipeline / no-cross-pipeline-snapshot
            // mode.
            conn.batch
                .commit()
                .context("committing framework transaction")?;
        }

        Ok(r)
    }
}

#[async_trait]
impl<S: Send + Sync> store_traits::Connection for Connection<'_, S> {
    async fn init_watermark(
        &mut self,
        pipeline_task: &str,
        checkpoint_hi_inclusive: Option<u64>,
    ) -> anyhow::Result<Option<InitWatermark>> {
        // Sequential pipelines: delegate to the committer
        // watermark, matching alt's behavior.
        self.delegate_to_committer_watermark(pipeline_task, checkpoint_hi_inclusive)
            .await
    }

    async fn accepts_chain_id(
        &mut self,
        pipeline_task: &str,
        chain_id: [u8; 32],
    ) -> anyhow::Result<bool> {
        let key = PipelineTaskKey::new(pipeline_task);
        let stored = self.store.inner.framework.chain_ids.get(&key)?;
        match stored {
            Some(ChainId(stored_id)) => Ok(stored_id == chain_id),
            None => {
                // First call for this pipeline: persist the chain
                // id immediately so a process crash before any
                // checkpoint commits still records the pinning.
                // The framework's call site uses a freshly created
                // `Connection` that is not committed via
                // `transaction`, so we cannot stage on
                // `self.batch`.
                let mut wb = self.store.inner.db.batch();
                wb.put(
                    &self.store.inner.framework.chain_ids,
                    &key,
                    &ChainId(chain_id),
                )?;
                wb.commit()?;
                Ok(true)
            }
        }
    }

    async fn committer_watermark(
        &mut self,
        pipeline_task: &str,
    ) -> anyhow::Result<Option<CommitterWatermark>> {
        let key = PipelineTaskKey::new(pipeline_task);
        Ok(self
            .store
            .inner
            .framework
            .watermarks
            .get(&key)?
            .map(committer_watermark::to_committer))
    }

    async fn set_committer_watermark(
        &mut self,
        pipeline_task: &str,
        watermark: CommitterWatermark,
    ) -> anyhow::Result<bool> {
        // Defer the write until `transaction` commits, so the
        // watermark advance and the user's data writes land
        // atomically.
        self.watermark = Some((
            pipeline_task.to_string(),
            committer_watermark::from_committer(watermark),
        ));
        Ok(true)
    }
}

#[async_trait]
impl<S: Send + Sync> SequentialConnection for Connection<'_, S> {}

#[cfg(test)]
mod tests {
    use scoped_futures::ScopedFutureExt;
    use sui_indexer_alt_framework_store_traits::Connection as _;
    use sui_indexer_alt_framework_store_traits::Store as _;
    use tempfile::TempDir;

    use super::*;
    use crate::CfDescriptor;
    use crate::DbMap;
    use crate::DbOptions;
    use crate::Decode;
    use crate::Encode;
    use crate::Schema;
    use crate::error::DecodeError;
    use crate::error::EncodeError;
    use crate::error::OpenError;

    /// Minimal consumer schema with one CF used by the transaction
    /// tests below.
    #[derive(Debug)]
    struct UserSchema {
        items: DbMap<U64Be, U64Be>,
    }

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    struct U64Be(u64);

    impl Encode for U64Be {
        fn encode_into<B: bytes::BufMut>(&self, buf: &mut B) -> Result<(), EncodeError> {
            buf.put_slice(&self.0.to_be_bytes());
            Ok(())
        }
    }

    impl Decode for U64Be {
        fn decode<B: bytes::Buf>(buf: &mut B) -> Result<Self, DecodeError> {
            if buf.remaining() != 8 {
                return Err(DecodeError::msg("expected 8 bytes"));
            }
            Ok(Self(buf.get_u64()))
        }
    }

    impl Schema for UserSchema {
        fn cfs(base_options: &rocksdb::Options) -> Vec<CfDescriptor> {
            vec![CfDescriptor::new("items", base_options.clone())]
        }

        fn open(db: &Db) -> Result<Self, OpenError> {
            Ok(Self {
                items: DbMap::new(db.clone(), "items")?,
            })
        }
    }

    fn setup() -> (TempDir, Store<UserSchema>) {
        let dir = TempDir::new().unwrap();
        // The framework's bookkeeping CFs are auto-registered by
        // `Db::open`, so the user schema declares only its own CFs.
        let (db, schema) = Db::open::<UserSchema>(dir.path(), DbOptions::default()).unwrap();
        let store = Store::new(db, Arc::new(schema));
        (dir, store)
    }

    #[tokio::test]
    async fn connect_returns_fresh_connection() {
        let (_dir, store) = setup();
        let conn = store.connect().await.unwrap();
        assert!(conn.batch.is_empty());
    }

    #[tokio::test]
    async fn committer_watermark_is_none_initially() {
        let (_dir, store) = setup();
        let mut conn = store.connect().await.unwrap();
        let w = conn.committer_watermark("balances").await.unwrap();
        assert!(w.is_none());
    }

    #[tokio::test]
    async fn transaction_writes_data_and_watermark_atomically() {
        let (_dir, store) = setup();

        store
            .transaction(|c| {
                async move {
                    c.batch
                        .put(&c.store.schema().items, &U64Be(1), &U64Be(10))?;
                    c.set_committer_watermark("items", CommitterWatermark::new_for_testing(7))
                        .await?;
                    Ok::<(), anyhow::Error>(())
                }
                .scope_boxed()
            })
            .await
            .unwrap();

        // Data row landed.
        assert_eq!(
            store.schema().items.get(&U64Be(1)).unwrap(),
            Some(U64Be(10)),
        );
        // Watermark landed.
        let mut conn = store.connect().await.unwrap();
        let w = conn.committer_watermark("items").await.unwrap();
        assert_eq!(w.unwrap().checkpoint_hi_inclusive, 7);
    }

    #[tokio::test]
    async fn transaction_without_watermark_fails() {
        let (_dir, store) = setup();
        let err = store
            .transaction(|c| {
                async move {
                    c.batch
                        .put(&c.store.schema().items, &U64Be(1), &U64Be(10))?;
                    // Note: no set_committer_watermark call.
                    Ok::<(), anyhow::Error>(())
                }
                .scope_boxed()
            })
            .await
            .unwrap_err();
        assert!(format!("{err:#}").contains("No watermark set"));
        // The data write did not land — `transaction` aborted
        // before committing the batch.
        assert!(store.schema().items.get(&U64Be(1)).unwrap().is_none());
    }

    #[tokio::test]
    async fn accepts_chain_id_records_on_first_call() {
        let (_dir, store) = setup();
        let mut conn = store.connect().await.unwrap();
        let id = [3u8; 32];
        assert!(conn.accepts_chain_id("pipeline", id).await.unwrap());

        // The chain id is persisted immediately (not deferred to
        // transaction commit), so a fresh connection sees it.
        let mut conn2 = store.connect().await.unwrap();
        assert!(conn2.accepts_chain_id("pipeline", id).await.unwrap());
    }

    #[tokio::test]
    async fn accepts_chain_id_rejects_mismatch() {
        let (_dir, store) = setup();
        let mut conn = store.connect().await.unwrap();
        assert!(conn.accepts_chain_id("pipeline", [3u8; 32]).await.unwrap());

        let mut conn2 = store.connect().await.unwrap();
        let accepted = conn2.accepts_chain_id("pipeline", [4u8; 32]).await.unwrap();
        assert!(!accepted, "different chain id must be rejected");
    }

    #[tokio::test]
    async fn accepts_chain_id_is_per_pipeline() {
        let (_dir, store) = setup();
        let mut conn = store.connect().await.unwrap();
        assert!(conn.accepts_chain_id("a", [1u8; 32]).await.unwrap());
        // A different pipeline gets to record its own id.
        assert!(conn.accepts_chain_id("b", [2u8; 32]).await.unwrap());
    }

    #[tokio::test]
    async fn init_watermark_returns_existing_after_commit() {
        let (_dir, store) = setup();
        store
            .transaction(|c| {
                async move {
                    c.set_committer_watermark("p", CommitterWatermark::new_for_testing(42))
                        .await?;
                    Ok::<(), anyhow::Error>(())
                }
                .scope_boxed()
            })
            .await
            .unwrap();

        let mut conn = store.connect().await.unwrap();
        let init = conn.init_watermark("p", None).await.unwrap().unwrap();
        assert_eq!(init.checkpoint_hi_inclusive, Some(42));
        assert!(init.reader_lo.is_none());
    }

    #[tokio::test]
    async fn init_watermark_returns_none_for_unknown_pipeline() {
        let (_dir, store) = setup();
        let mut conn = store.connect().await.unwrap();
        let init = conn.init_watermark("unknown", None).await.unwrap();
        assert!(init.is_none());
    }

    #[tokio::test]
    async fn multiple_transactions_advance_the_watermark() {
        let (_dir, store) = setup();
        for cp in [1u64, 2, 3] {
            store
                .transaction(move |c| {
                    async move {
                        c.batch
                            .put(&c.store.schema().items, &U64Be(cp), &U64Be(cp * 10))?;
                        c.set_committer_watermark("p", CommitterWatermark::new_for_testing(cp))
                            .await?;
                        Ok::<(), anyhow::Error>(())
                    }
                    .scope_boxed()
                })
                .await
                .unwrap();
        }
        let mut conn = store.connect().await.unwrap();
        let w = conn.committer_watermark("p").await.unwrap().unwrap();
        assert_eq!(w.checkpoint_hi_inclusive, 3);
        // All three data rows landed.
        for cp in [1u64, 2, 3] {
            assert_eq!(
                store.schema().items.get(&U64Be(cp)).unwrap(),
                Some(U64Be(cp * 10)),
            );
        }
    }

    #[tokio::test]
    async fn store_clone_shares_the_same_db() {
        let (_dir, store) = setup();
        let store2 = store.clone();
        store
            .transaction(|c| {
                async move {
                    c.batch
                        .put(&c.store.schema().items, &U64Be(1), &U64Be(10))?;
                    c.set_committer_watermark("p", CommitterWatermark::new_for_testing(1))
                        .await?;
                    Ok::<(), anyhow::Error>(())
                }
                .scope_boxed()
            })
            .await
            .unwrap();
        // The clone observes the same write.
        assert_eq!(
            store2.schema().items.get(&U64Be(1)).unwrap(),
            Some(U64Be(10)),
        );
    }
}
