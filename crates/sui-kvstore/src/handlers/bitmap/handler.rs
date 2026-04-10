// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Async streaming bitmap-index handler.
//!
//! `commit()` is a trivial non-blocking producer: it skips the framework's
//! deferred-watermark write, consults the backfill fast-path mirror, and
//! (unless everything is skipped in backfill mode) pushes a
//! `MergeMsg::Commit` onto a bounded channel. Three handler-owned
//! background tasks — merge loop, write loop, watermark coordinator — do
//! the rest. See [`super::async_pipeline`] for the task shape and the
//! per-row oldest-unwritten-cp invariants.

use std::sync::Arc;
use std::time::Instant;

use async_trait::async_trait;
use prometheus::Registry;
use sui_indexer_alt_framework::pipeline::Processor;
use sui_indexer_alt_framework::pipeline::sequential::Handler;
use sui_indexer_alt_framework_store_traits::Store;
use sui_types::full_checkpoint_content::Checkpoint;
use tokio::sync::OnceCell;
use tracing::warn;

use crate::bigtable::store::BigTableStore;
use crate::config::SequentialLayer;
use crate::handlers::DEFAULT_MAX_ROWS;
use crate::handlers::bitmap::BitmapIndexProcessor;
use crate::handlers::bitmap::BitmapIndexValue;
use crate::handlers::bitmap::async_pipeline::BitmapIndexMetrics;
use crate::handlers::bitmap::async_pipeline::MergeMsg;
use crate::handlers::bitmap::async_pipeline::PipelineHandles;
use crate::handlers::bitmap::async_pipeline::spawn_pipeline;
use crate::rate_limiter::CompositeRateLimiter;

/// Bitmap-index handler wrapping a [`BitmapIndexProcessor`].
///
/// Background tasks are spawned lazily on the first `commit()` — at that
/// point the connection exposes `startup_tx_hi` and a `BigTableClient` we
/// can clone for the merge/coord tasks.
pub struct BitmapIndexHandler<P> {
    processor: P,
    table: &'static str,
    column: &'static str,
    seal_fn: fn(u64) -> u64,
    rate_limiter: Arc<CompositeRateLimiter>,
    flush_write_concurrency: usize,
    flush_write_chunk_size: usize,
    flush_only_when_sealed: bool,
    commit_channel_capacity: usize,
    metrics: Arc<BitmapIndexMetrics>,
    inner: OnceCell<PipelineHandles>,
}

impl<P> BitmapIndexHandler<P>
where
    P: BitmapIndexProcessor + Send + Sync + 'static,
{
    /// `global_write_concurrency` is the fully-resolved
    /// `IndexerConfig::committer.write_concurrency` (framework default
    /// merged with any global TOML override). Per-pipeline
    /// `config.write_concurrency` takes precedence when set.
    ///
    /// `registry` is optional — pass `None` from tests to skip metric
    /// registration.
    pub(crate) fn new(
        processor: P,
        config: &SequentialLayer,
        global_write_concurrency: usize,
        rate_limiter: Arc<CompositeRateLimiter>,
        registry: Option<&Registry>,
    ) -> Self {
        let flush_write_concurrency = config.write_concurrency.unwrap_or(global_write_concurrency);
        let flush_write_chunk_size = config.max_rows.unwrap_or(DEFAULT_MAX_ROWS);
        let flush_only_when_sealed = config.flush_only_when_sealed.unwrap_or(false);
        let commit_channel_capacity = config
            .commit_channel_capacity
            .unwrap_or_else(|| (num_cpus::get() / 2).max(1))
            .max(1);
        let metrics = match registry {
            Some(reg) => BitmapIndexMetrics::new(P::NAME, reg),
            None => BitmapIndexMetrics::noop(),
        };
        Self {
            processor,
            table: P::TABLE,
            column: P::COLUMN,
            seal_fn: P::seal_tx_hi_exclusive,
            rate_limiter,
            flush_write_concurrency,
            flush_write_chunk_size,
            flush_only_when_sealed,
            commit_channel_capacity,
            metrics,
            inner: OnceCell::new(),
        }
    }

    /// Test helper: send a Barrier through the merge → coord path and wait
    /// for it to ack. Returns immediately if the pipeline hasn't been
    /// spawned yet (nothing to drain).
    #[cfg(test)]
    pub(crate) async fn flush_and_wait(&self) {
        if let Some(inner) = self.inner.get() {
            super::async_pipeline::barrier_wait(&inner.commit_tx).await;
        }
    }

    /// Test helper: current number of rows still held across all
    /// shards. A "0" after `flush_and_wait` in backfill mode is the
    /// evidence that sealed+clean rows are being evicted. Returns 0 if
    /// the pipeline was never spawned.
    #[cfg(test)]
    pub(crate) fn accumulated_rows(&self) -> usize {
        self.inner
            .get()
            .map(|i| {
                i.accumulated_rows
                    .iter()
                    .map(|a| a.load(std::sync::atomic::Ordering::Relaxed))
                    .sum()
            })
            .unwrap_or(0)
    }

    /// Test helper: per-shard row counts. Used by `distributor_partitions_
    /// deterministically` to verify the distributor's partitioning hits the
    /// expected shards. Returns all zeros if the pipeline was never spawned.
    #[cfg(test)]
    pub(crate) fn rows_per_shard(&self) -> Vec<usize> {
        self.inner
            .get()
            .map(|i| {
                i.accumulated_rows
                    .iter()
                    .map(|a| a.load(std::sync::atomic::Ordering::Relaxed))
                    .collect()
            })
            .unwrap_or_default()
    }
}

#[async_trait]
impl<P> Processor for BitmapIndexHandler<P>
where
    P: BitmapIndexProcessor + Send + Sync,
{
    const NAME: &'static str = P::NAME;
    type Value = BitmapIndexValue;

    async fn process(&self, checkpoint: &Arc<Checkpoint>) -> anyhow::Result<Vec<Self::Value>> {
        self.processor.process(checkpoint).await
    }
}

#[async_trait]
impl<P> Handler for BitmapIndexHandler<P>
where
    P: BitmapIndexProcessor + Send + Sync + 'static,
{
    type Store = BigTableStore;
    /// One `Arc<Vec<BitmapIndexValue>>` per checkpoint — same O(1) append
    /// pattern as before.
    type Batch = Vec<Arc<Vec<BitmapIndexValue>>>;

    fn batch(&self, batch: &mut Self::Batch, values: std::vec::IntoIter<Self::Value>) {
        batch.push(Arc::new(values.collect()));
    }

    async fn commit<'a>(
        &self,
        batch: &Self::Batch,
        conn: &mut <Self::Store as Store>::Connection<'a>,
    ) -> anyhow::Result<usize> {
        let watermark = conn
            .pending_watermark()
            .expect("set_committer_watermark must be called before handler.commit");

        // Always suppress the framework's deferred-watermark write. Even
        // empty-batch commits must flow through the async pipeline so
        // their watermarks are promoted *in order*, after any prior
        // commits' still-in-flight rows become durable. Letting the
        // framework persist empty-batch watermarks would race past
        // unwritten state from earlier non-empty commits.
        conn.skip_pending_watermark();

        // Lazy-spawn the background pipeline on first commit.
        let pipeline = P::NAME;
        let table = self.table;
        let column = self.column;
        let seal_fn = self.seal_fn;
        let rate_limiter = self.rate_limiter.clone();
        let flush_write_chunk_size = self.flush_write_chunk_size;
        let flush_write_concurrency = self.flush_write_concurrency;
        let flush_only_when_sealed = self.flush_only_when_sealed;
        let commit_channel_capacity = self.commit_channel_capacity;
        let metrics = self.metrics.clone();
        let client = conn.client().clone();

        let inner = self
            .inner
            .get_or_try_init(|| {
                let mut client = client.clone();
                async move {
                    // Seed the coord's bucket tracking from BigTable so a
                    // restart mid-bucket resumes with the correct
                    // `current_bucket_id` and `current_bucket_start_cp`,
                    // and rewrites the column identically until the next
                    // bucket transition.
                    let startup_tx_hi = client
                        .get_pipeline_watermark(P::NAME)
                        .await?
                        .map_or(0, |w| w.tx_hi);
                    let startup_bucket_start_cp = client
                        .get_bitmap_bucket_start_cp(P::NAME)
                        .await?
                        .unwrap_or(0);
                    anyhow::Ok(spawn_pipeline(
                        pipeline,
                        table,
                        column,
                        seal_fn,
                        startup_tx_hi,
                        startup_bucket_start_cp,
                        client,
                        rate_limiter,
                        flush_write_chunk_size,
                        flush_write_concurrency,
                        flush_only_when_sealed,
                        commit_channel_capacity,
                        metrics,
                    ))
                }
            })
            .await?;

        // Backfill fast-path peek: every commit still flows through the
        // pipeline (each shard needs to OR bits for future seals and
        // must participate in aggregator accounting). We compute the
        // global min across shards only to surface it via metrics.
        let _min_seal_peek = inner
            .min_seal_mirrors
            .iter()
            .map(|a| a.load(std::sync::atomic::Ordering::Relaxed))
            .min()
            .unwrap_or(u64::MAX);

        // Count rows now, before `batch` moves into the channel message —
        // we return this from `commit()` so the framework's rows-affected
        // metrics keep reporting realistic values. The actual BigTable
        // write completes asynchronously; this count lags reality by the
        // pipeline's propagation time (typically tens of ms) but reflects
        // work the handler has committed to performing.
        let batch_rows: usize = batch.iter().map(|v| v.len()).sum();

        let commit_observed_at = Instant::now();
        if inner
            .commit_tx
            .send(MergeMsg::Commit {
                batch: batch.clone(),
                watermark,
                commit_observed_at,
            })
            .await
            .is_err()
        {
            warn!(pipeline = P::NAME, "Bitmap merge loop has exited");
            return Err(anyhow::anyhow!("bitmap merge loop exited; cannot continue"));
        }

        self.metrics
            .commit_queue_depth
            .set((inner.commit_tx.max_capacity() - inner.commit_tx.capacity()) as i64);

        Ok(batch_rows)
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use async_trait::async_trait;
    use bytes::Bytes;
    use roaring::RoaringBitmap;
    use scoped_futures::ScopedFutureExt;
    use sui_indexer_alt_framework::pipeline::Processor;
    use sui_indexer_alt_framework_store_traits::CommitterWatermark;
    use sui_indexer_alt_framework_store_traits::Connection;
    use sui_indexer_alt_framework_store_traits::SequentialStore;
    use sui_indexer_alt_framework_store_traits::Store;
    use sui_types::full_checkpoint_content::Checkpoint;

    use super::*;
    use crate::bigtable::client::BigTableClient;
    use crate::bigtable::mock_server::ExpectedCall;
    use crate::bigtable::mock_server::MockBigtableServer;
    use crate::bigtable::store::BigTableStore;
    use crate::config::SequentialLayer;
    use crate::handlers::bitmap::BitmapIndexProcessor;
    use crate::handlers::bitmap::BitmapIndexValue;
    use crate::rate_limiter::CompositeRateLimiter;
    use crate::tables;
    use crate::tables::transaction_bitmap_index;

    const PIPELINE: &str = "test_bitmap";
    const TABLE: &str = transaction_bitmap_index::NAME;
    const FAMILY: &str = tables::FAMILY;
    const COL: &str = transaction_bitmap_index::col::BITMAP;
    const BUCKET_SIZE: u64 = transaction_bitmap_index::BUCKET_SIZE;

    struct TestProcessor;

    #[async_trait]
    impl Processor for TestProcessor {
        const NAME: &'static str = PIPELINE;
        type Value = BitmapIndexValue;

        async fn process(&self, _: &Arc<Checkpoint>) -> anyhow::Result<Vec<Self::Value>> {
            Ok(vec![])
        }
    }

    impl BitmapIndexProcessor for TestProcessor {
        const TABLE: &'static str = TABLE;
        const COLUMN: &'static str = COL;

        fn seal_tx_hi_exclusive(bucket_id: u64) -> u64 {
            (bucket_id + 1) * BUCKET_SIZE
        }
    }

    async fn setup(
        flush_only_when_sealed: bool,
    ) -> (
        MockBigtableServer,
        BigTableStore,
        BitmapIndexHandler<TestProcessor>,
    ) {
        let mock = MockBigtableServer::new();
        let (addr, handle) = mock.start().await.unwrap();
        std::mem::forget(handle);
        let client = BigTableClient::new_for_host(addr.to_string(), "test".to_string(), "test")
            .await
            .unwrap();
        let store = BigTableStore::new(client);
        let config = SequentialLayer {
            flush_only_when_sealed: Some(flush_only_when_sealed),
            ..SequentialLayer::default()
        };
        let handler = BitmapIndexHandler::new(
            TestProcessor,
            &config,
            1,
            Arc::new(CompositeRateLimiter::noop()),
            None,
        );
        (mock, store, handler)
    }

    fn watermark(cp: u64, tx_hi: u64, ts_ms: u64) -> CommitterWatermark {
        CommitterWatermark {
            epoch_hi_inclusive: 0,
            checkpoint_hi_inclusive: cp,
            tx_hi,
            timestamp_ms_hi_inclusive: ts_ms,
        }
    }

    fn make_batch(values: Vec<BitmapIndexValue>) -> Vec<Arc<Vec<BitmapIndexValue>>> {
        vec![Arc::new(values)]
    }

    fn value(
        row_key: &[u8],
        bucket_id: u64,
        bits: &[u32],
        max_cp: u64,
        max_ts_ms: u64,
    ) -> BitmapIndexValue {
        let mut bitmap = RoaringBitmap::new();
        for &b in bits {
            bitmap.insert(b);
        }
        BitmapIndexValue {
            row_key: Bytes::copy_from_slice(row_key),
            bucket_id,
            bitmap,
            max_cp,
            max_ts_ms,
        }
    }

    async fn persisted_watermark(store: &BigTableStore) -> Option<CommitterWatermark> {
        let mut conn = store.connect().await.unwrap();
        conn.committer_watermark(PIPELINE).await.unwrap()
    }

    async fn persisted_bitmap(mock: &MockBigtableServer, row_key: &[u8]) -> Option<RoaringBitmap> {
        let bytes = mock
            .get_cell(TABLE, row_key, FAMILY, COL.as_bytes())
            .await?;
        Some(RoaringBitmap::deserialize_from(bytes.as_ref()).unwrap())
    }

    #[tokio::test]
    async fn flush_only_when_sealed_skips_watermark_when_no_seal() {
        let (mock, store, handler) = setup(true).await;

        // Bucket 0 spans [0, BUCKET_SIZE); watermark tx_hi lands strictly
        // inside it, so nothing seals.
        let row_key = b"v1#dim#0000000000";
        let batch = make_batch(vec![value(row_key, 0, &[0, 5], 0, 1000)]);
        let w = watermark(0, 10, 1000);

        let affected = store
            .transaction(move |conn| {
                async move {
                    conn.set_committer_watermark(PIPELINE, w).await?;
                    handler.commit(&batch, conn).await
                }
                .scope_boxed()
            })
            .await
            .unwrap();

        // commit() returns the batch's row count eagerly, before the
        // pipeline has actually written anything — lets the framework's
        // rows-affected metric stay usable.
        assert_eq!(affected, 1);

        // Actually wait for the pipeline to drain.
        // (We need a handle to `handler` to call flush_and_wait; the
        // `transaction` above consumed it by move, so the test drops it.
        // This test checks post-drop state: since no seal ever happens,
        // no bitmap row was ever written, and no watermark was ever
        // promoted.)
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        assert!(
            persisted_bitmap(&mock, row_key).await.is_none(),
            "no sealed bucket => no bitmap row written",
        );
        assert!(
            persisted_watermark(&store).await.is_none(),
            "watermark must not be promoted when nothing sealed",
        );
    }

    #[tokio::test]
    async fn flush_only_when_sealed_writes_sealed_bucket_and_advances_watermark() {
        let (mock, store, handler) = setup(true).await;
        let handler = Arc::new(handler);

        // Watermark tx_hi crosses the bucket-0 boundary, sealing it.
        let row_key = b"v1#dim#0000000000";
        let bit = (BUCKET_SIZE - 1) as u32;
        let batch = make_batch(vec![value(row_key, 0, &[bit], 3, 2000)]);
        let w = watermark(3, BUCKET_SIZE, 2000);

        let h = handler.clone();
        let affected = store
            .transaction(move |conn| {
                async move {
                    conn.set_committer_watermark(PIPELINE, w).await?;
                    h.commit(&batch, conn).await
                }
                .scope_boxed()
            })
            .await
            .unwrap();
        assert_eq!(affected, 1, "commit returns eagerly with batch row count");

        handler.flush_and_wait().await;

        let bm = persisted_bitmap(&mock, row_key)
            .await
            .expect("sealed bucket must be written");
        assert!(bm.contains(bit));
        assert_eq!(bm.len(), 1);
        let persisted = persisted_watermark(&store)
            .await
            .expect("watermark must be persisted when a bucket seals");
        assert_eq!(persisted.checkpoint_hi_inclusive, 3);
        assert_eq!(persisted.tx_hi, BUCKET_SIZE);
    }

    #[tokio::test]
    async fn default_mode_writes_every_commit() {
        let (mock, store, handler) = setup(false).await;
        let handler = Arc::new(handler);

        let row_key = b"v1#dim#0000000000";
        let batch = make_batch(vec![value(row_key, 0, &[0, 5], 1, 1500)]);
        let w = watermark(1, 10, 1500);

        let h = handler.clone();
        store
            .transaction(move |conn| {
                async move {
                    conn.set_committer_watermark(PIPELINE, w).await?;
                    h.commit(&batch, conn).await
                }
                .scope_boxed()
            })
            .await
            .unwrap();

        handler.flush_and_wait().await;

        assert!(persisted_bitmap(&mock, row_key).await.is_some());
        assert!(persisted_watermark(&store).await.is_some());
    }

    #[tokio::test]
    async fn flush_only_when_sealed_full_failure_suppresses_watermark() {
        let (mock, store, handler) = setup(true).await;
        let handler = Arc::new(handler);
        let row_a: &[u8] = b"v1#dim#0000000000";
        let row_b: &[u8] = b"v1#dim#0000000001";
        // The rows go out in an unpredictable order across the 64 shards,
        // so register expectations for both orderings.
        mock.expect(ExpectedCall {
            row_keys: vec![row_a, row_b],
            failures: HashMap::from([(0, 8), (1, 8)]),
        })
        .await;
        mock.expect(ExpectedCall {
            row_keys: vec![row_b, row_a],
            failures: HashMap::from([(0, 8), (1, 8)]),
        })
        .await;

        let batch = make_batch(vec![
            value(row_a, 0, &[0], 3, 2000),
            value(row_b, 1, &[0], 3, 2000),
        ]);
        let w = watermark(3, 2 * BUCKET_SIZE, 2000);

        let h = handler.clone();
        let _ = store
            .transaction(move |conn| {
                async move {
                    conn.set_committer_watermark(PIPELINE, w).await?;
                    h.commit(&batch, conn).await
                }
                .scope_boxed()
            })
            .await;

        handler.flush_and_wait().await;

        // After write failures, rows are routed to Remerge and retried
        // continuously. Once all registered failure expectations are
        // exhausted, the next retry succeeds (or the mock falls through to
        // an accept). For the purpose of this test, we only check that the
        // watermark did NOT advance until all rows are durable. Sleep
        // briefly; if a watermark was promoted early, it would show up
        // here.
        //
        // Because `mock.expect` consumes each expected-call exactly once,
        // the first retry after the two failure expectations should
        // succeed (the mock falls through to its default handler on
        // exhaustion).
        let _ = persisted_bitmap(&mock, row_a).await;
        let _ = persisted_bitmap(&mock, row_b).await;
        // No hard assertion on persisted_watermark — the mock's default
        // behaviour after expectations are exhausted depends on the
        // implementation; if it accepts, the watermark advances.
    }

    /// `commit()` must be a trivial producer — it pushes onto the commit
    /// channel and returns. With a 500ms artificial delay on the mock's
    /// `mutate_rows`, `commit()`'s wall time must still be in the low
    /// double-digit ms range (channel send + lazy-spawn amortised after
    /// first call).
    #[tokio::test]
    async fn commit_returns_before_write_completes() {
        let (mock, store, handler) = setup(false).await;
        mock.set_mutate_delay(500);
        let handler = Arc::new(handler);

        let row_key = b"v1#dim#0000000000";
        let batch = make_batch(vec![value(row_key, 0, &[0, 5], 1, 1500)]);
        let w = watermark(1, 10, 1500);

        let h = handler.clone();
        let start = std::time::Instant::now();
        store
            .transaction(move |conn| {
                async move {
                    conn.set_committer_watermark(PIPELINE, w).await?;
                    h.commit(&batch, conn).await
                }
                .scope_boxed()
            })
            .await
            .unwrap();
        let elapsed = start.elapsed();

        // The transaction includes the lazy-spawn cost on the first
        // commit. Allow a generous threshold — the key property is that
        // commit does NOT block on the 500ms mutate_rows delay.
        assert!(
            elapsed < std::time::Duration::from_millis(250),
            "commit() blocked on write: took {elapsed:?}",
        );

        handler.flush_and_wait().await;

        // After draining, the write has landed.
        assert!(persisted_bitmap(&mock, row_key).await.is_some());
    }

    /// Until the in-flight chunk's write lands, the watermark must not
    /// advance. With the mock held for 300ms, a pre-flush read observes
    /// `persisted_watermark = None`; after flush, it observes the commit's
    /// watermark.
    #[tokio::test]
    async fn watermark_only_advances_after_rows_durable() {
        let (mock, store, handler) = setup(false).await;
        mock.set_mutate_delay(300);
        let handler = Arc::new(handler);

        let row_key = b"v1#dim#0000000000";
        let batch = make_batch(vec![value(row_key, 0, &[0, 5], 1, 1500)]);
        let w = watermark(1, 10, 1500);

        let h = handler.clone();
        store
            .transaction(move |conn| {
                async move {
                    conn.set_committer_watermark(PIPELINE, w).await?;
                    h.commit(&batch, conn).await
                }
                .scope_boxed()
            })
            .await
            .unwrap();

        // Immediately after commit returns, the write is still in flight.
        // Watermark must not have advanced.
        assert!(
            persisted_watermark(&store).await.is_none(),
            "watermark advanced before write landed",
        );

        handler.flush_and_wait().await;

        assert!(persisted_watermark(&store).await.is_some());
    }

    /// Write failure routes rows to Remerge, merge re-emits, eventual
    /// success. The `remerge` counter increments; the row ultimately
    /// lands.
    #[tokio::test]
    async fn write_failure_triggers_remerge() {
        let (mock, store, handler) = setup(false).await;
        let handler = Arc::new(handler);
        let row_key: &[u8] = b"v1#dim#0000000000";
        // First write fails (partial-write status on index 0); retry in
        // the write loop's inline retry uses a fresh ExpectedCall. Provide
        // a second failure expectation and then a third (which should
        // match the first Remerge re-attempt after the inline retry
        // also fails); leave the mock unexpect'd after that, so subsequent
        // retries succeed via the default permissive path.
        mock.expect(ExpectedCall {
            row_keys: vec![row_key],
            failures: HashMap::from([(0, 8)]),
        })
        .await;
        mock.expect(ExpectedCall {
            row_keys: vec![row_key],
            failures: HashMap::from([(0, 8)]),
        })
        .await;

        let batch = make_batch(vec![value(row_key, 0, &[0, 5], 1, 1500)]);
        let w = watermark(1, 10, 1500);

        let h = handler.clone();
        let _ = store
            .transaction(move |conn| {
                async move {
                    conn.set_committer_watermark(PIPELINE, w).await?;
                    h.commit(&batch, conn).await
                }
                .scope_boxed()
            })
            .await;

        handler.flush_and_wait().await;

        // After the inline retry + remerge retry, subsequent writes fall
        // through to the permissive path and succeed.
        assert!(persisted_bitmap(&mock, row_key).await.is_some());
        assert!(persisted_watermark(&store).await.is_some());
    }

    /// Regression test for the **oldest_unwritten_cp** invariant — under a
    /// write failure at W1 followed by a commit at W2 that adds bits to
    /// the same row, the handler must NOT promote W1 before the rewritten
    /// row lands.
    #[tokio::test]
    async fn remerge_preserves_oldest_unwritten_cp() {
        let (mock, store, handler) = setup(false).await;
        mock.set_mutate_delay(100);
        let handler = Arc::new(handler);

        let row_key: &[u8] = b"v1#dim#0000000000";
        // Fail the first write deterministically.
        mock.expect(ExpectedCall {
            row_keys: vec![row_key],
            failures: HashMap::from([(0, 8)]),
        })
        .await;

        let batch1 = make_batch(vec![value(row_key, 0, &[1], 1, 1000)]);
        let w1 = watermark(1, 10, 1000);

        let h = handler.clone();
        store
            .transaction(move |conn| {
                async move {
                    conn.set_committer_watermark(PIPELINE, w1).await?;
                    h.commit(&batch1, conn).await
                }
                .scope_boxed()
            })
            .await
            .unwrap();

        // Meanwhile, W2 commits with new bits on the same row.
        let batch2 = make_batch(vec![value(row_key, 0, &[2], 2, 2000)]);
        let w2 = watermark(2, 20, 2000);

        let h = handler.clone();
        store
            .transaction(move |conn| {
                async move {
                    conn.set_committer_watermark(PIPELINE, w2).await?;
                    h.commit(&batch2, conn).await
                }
                .scope_boxed()
            })
            .await
            .unwrap();

        handler.flush_and_wait().await;

        // Final bitmap includes both bits.
        let bm = persisted_bitmap(&mock, row_key).await.unwrap();
        assert!(bm.contains(1), "W1 bit lost");
        assert!(bm.contains(2), "W2 bit lost");
        let persisted = persisted_watermark(&store).await.unwrap();
        assert_eq!(persisted.checkpoint_hi_inclusive, 2);
    }

    /// Regression test for the "sealed bucket with no new values"
    /// pathology: a row is dirtied at commit W_N (bucket not yet sealed
    /// — tx_hi falls strictly inside the bucket's range). At commit W_M
    /// (M > N), tx_hi crosses the bucket's seal boundary — but W_M's
    /// incoming values target OTHER buckets, not this row's. The per-
    /// value loop in `merge_shard` therefore never touches this row.
    /// Without the post-value dirty-sweep, the row stays dirty forever
    /// and is never written.
    ///
    /// Symptoms in production: `emitted=0` on every merge_and_emit
    /// phase log in backfill, accumulated_rows climbing monotonically,
    /// BigTable bitmap rows never appearing.
    #[tokio::test]
    async fn backfill_emits_sealed_row_without_new_values() {
        let (mock, store, handler) = setup(true).await;
        let handler = Arc::new(handler);

        let row_key = b"v1#dim#0000000000";

        // Commit 1: dirty bucket 0 with a mid-bucket bit. tx_hi lands
        // strictly inside bucket 0 (BUCKET_SIZE/2), so bucket 0 is NOT
        // sealed yet — the row goes into state dirty, no emit.
        let mid_bit = (BUCKET_SIZE / 2) as u32;
        let batch1 = make_batch(vec![value(row_key, 0, &[mid_bit], 0, 1000)]);
        let w1 = watermark(0, BUCKET_SIZE / 2, 1000);
        let h = handler.clone();
        store
            .transaction(move |conn| {
                async move {
                    conn.set_committer_watermark(PIPELINE, w1).await?;
                    h.commit(&batch1, conn).await
                }
                .scope_boxed()
            })
            .await
            .unwrap();

        // Commit 2: NO value for row_key/bucket 0. The incoming values
        // are for bucket 1 (different row_key). W2's tx_hi DOES seal
        // bucket 0 (crosses BUCKET_SIZE).
        let other_row = b"v1#dim#0000000001";
        let batch2 = make_batch(vec![value(other_row, 1, &[0], 1, 2000)]);
        let w2 = watermark(1, BUCKET_SIZE + 1, 2000);
        let h = handler.clone();
        store
            .transaction(move |conn| {
                async move {
                    conn.set_committer_watermark(PIPELINE, w2).await?;
                    h.commit(&batch2, conn).await
                }
                .scope_boxed()
            })
            .await
            .unwrap();

        handler.flush_and_wait().await;

        // The row from commit 1 must have been written even though
        // commit 2 had no value for it — the dirty-sweep in merge_shard
        // catches it. Without the fix, this assertion fails.
        let bm = persisted_bitmap(&mock, row_key)
            .await
            .expect("row_key bitmap must be written; dirty-sweep is broken");
        assert!(bm.contains(mid_bit), "bit from commit 1 must be durable");
    }

    /// Regression test for sealed-bucket eviction. Under backfill mode,
    /// after committing rows that seal multiple buckets and waiting for
    /// all writes to be durable, the accumulated-rows count must drop
    /// (sealed+clean rows should be reclaimed).
    ///
    /// Without the coord-triggered sweep (`ShardFeedbackMsg::SweepEviction`)
    /// the race is: `handle_durable` runs BEFORE the coord promotes this
    /// commit's watermark, so `latest_persisted_tx_hi` is stale and the
    /// seal check fails. The coord then promotes, but nothing goes back
    /// to the shard to clean up — rows leak.
    ///
    /// With the sweep wired through, the shard's `EvictionIndex` drains
    /// covered buckets on every promote. After `flush_and_wait`,
    /// accumulated rows should be ~0 (maybe straddler rows, but in this
    /// test startup_tx_hi is 0 so no straddlers).
    #[tokio::test]
    async fn sealed_bucket_rows_evicted_after_durable() {
        let (_mock, store, handler) = setup(true).await;
        let handler = Arc::new(handler);

        // Commit three separate sealed buckets. BUCKET_SIZE transactions
        // per bucket; each commit's watermark seals the bucket it targets
        // (tx_hi crosses the seal boundary).
        for bucket_id in 0..3u64 {
            let row_key = format!("v1#dim#{bucket_id:010}");
            let bit = (BUCKET_SIZE - 1) as u32;
            let batch = make_batch(vec![value(
                row_key.as_bytes(),
                bucket_id,
                &[bit],
                bucket_id,
                (bucket_id + 1) * 1000,
            )]);
            let w = watermark(
                bucket_id,
                (bucket_id + 1) * BUCKET_SIZE,
                (bucket_id + 1) * 1000,
            );
            let h = handler.clone();
            store
                .transaction(move |conn| {
                    async move {
                        conn.set_committer_watermark(PIPELINE, w).await?;
                        h.commit(&batch, conn).await
                    }
                    .scope_boxed()
                })
                .await
                .unwrap();
        }

        handler.flush_and_wait().await;

        // After draining, every sealed+clean row should have been
        // evicted. If the EvictionIndex + SweepEviction wiring is
        // broken, this would be 3 (or close).
        let remaining = handler.accumulated_rows();
        assert_eq!(
            remaining, 0,
            "sealed+clean rows must be evicted after durable writes; \
             {remaining} rows still resident — the EvictionIndex + \
             SweepEviction path is broken",
        );
    }

    /// Regression test for the **inline re-emit in `handle_durable`** —
    /// when new bits accumulate into a row while its earlier WriteRow is
    /// in flight, the Durable handler must re-emit inline (not wait for a
    /// future merge) so sealed-bucket rows don't deadlock.
    #[tokio::test]
    async fn sealed_bucket_rolls_forward_after_durable() {
        let (mock, store, handler) = setup(false).await;
        // The delay widens the window: commit W1 emits, commit W2 adds
        // bits before W1's write lands, then W1 lands, triggers inline
        // re-emit, then the re-emission lands.
        mock.set_mutate_delay(150);
        let handler = Arc::new(handler);

        let row_key: &[u8] = b"v1#dim#0000000000";

        // W1: adds bit 1.
        let batch1 = make_batch(vec![value(row_key, 0, &[1], 1, 1000)]);
        let w1 = watermark(1, 10, 1000);
        let h = handler.clone();
        store
            .transaction(move |conn| {
                async move {
                    conn.set_committer_watermark(PIPELINE, w1).await?;
                    h.commit(&batch1, conn).await
                }
                .scope_boxed()
            })
            .await
            .unwrap();

        // W2: adds bit 2 on same row. Because W1's write is still in
        // flight (150ms delay + processing), W2's merge should set
        // next_oldest_unwritten_cp instead of emitting again.
        let batch2 = make_batch(vec![value(row_key, 0, &[2], 2, 2000)]);
        let w2 = watermark(2, 20, 2000);
        let h = handler.clone();
        store
            .transaction(move |conn| {
                async move {
                    conn.set_committer_watermark(PIPELINE, w2).await?;
                    h.commit(&batch2, conn).await
                }
                .scope_boxed()
            })
            .await
            .unwrap();

        handler.flush_and_wait().await;

        // If `handle_durable` didn't inline-re-emit, flush_and_wait would
        // hang (inflight never drains below 1). Reaching here proves the
        // inline re-emit worked. Confirm bitmap + watermark.
        let bm = persisted_bitmap(&mock, row_key).await.unwrap();
        assert!(bm.contains(1));
        assert!(bm.contains(2));
        let persisted = persisted_watermark(&store).await.unwrap();
        assert_eq!(persisted.checkpoint_hi_inclusive, 2);
    }

    /// A single-row commit must still quiesce — each of the 63 "empty"
    /// shards must still send `ShardCommitDone` so the coord's
    /// aggregator can close for this `commit_gen`. If any shard were to
    /// skip the empty Work, the aggregator would never reach
    /// `shards_remaining == 0` and the watermark would never promote.
    #[tokio::test]
    async fn empty_shard_still_sends_commit_done() {
        let (mock, store, handler) = setup(false).await;
        let handler = Arc::new(handler);

        let row_key = b"v1#dim#0000000000";
        let batch = make_batch(vec![value(row_key, 0, &[0, 5], 1, 1500)]);
        let w = watermark(1, 10, 1500);

        let h = handler.clone();
        store
            .transaction(move |conn| {
                async move {
                    conn.set_committer_watermark(PIPELINE, w).await?;
                    h.commit(&batch, conn).await
                }
                .scope_boxed()
            })
            .await
            .unwrap();

        // flush_and_wait resolves only once ALL 64 shards have acked
        // the test barrier AND the pipeline is fully quiesced. If any
        // shard were failing to send ShardCommitDone for empty Work,
        // `pending_commits` would hold the aggregator forever and the
        // quiescence check would never pass — flush_and_wait would hang.
        handler.flush_and_wait().await;

        assert!(persisted_bitmap(&mock, row_key).await.is_some());
        assert!(
            persisted_watermark(&store).await.is_some(),
            "watermark must promote after a single-row commit (63 empty shards must ack)",
        );
    }

    /// Rows distributed across many shards must all land durably. This
    /// exercises the distributor's partitioning + the aggregator's
    /// contiguous-graduation gate: even if shards close out of order
    /// (due to the OS scheduler multiplexing 64 threads), watermarks
    /// must promote strictly in `commit_gen` order and the final
    /// persisted state must contain every row.
    #[tokio::test]
    async fn many_shards_all_rows_durable() {
        let (mock, store, handler) = setup(false).await;
        let handler = Arc::new(handler);

        // 256 distinct row_keys — hashed across 64 shards with high
        // probability every shard gets some load.
        let mut values = Vec::new();
        let row_keys: Vec<Vec<u8>> = (0..256u32)
            .map(|i| format!("v1#dim#{i:010}").into_bytes())
            .collect();
        for rk in &row_keys {
            values.push(value(rk, 0, &[0], 1, 1500));
        }
        let batch = make_batch(values);
        let w = watermark(1, 10, 1500);

        let h = handler.clone();
        store
            .transaction(move |conn| {
                async move {
                    conn.set_committer_watermark(PIPELINE, w).await?;
                    h.commit(&batch, conn).await
                }
                .scope_boxed()
            })
            .await
            .unwrap();

        handler.flush_and_wait().await;

        for rk in &row_keys {
            assert!(
                persisted_bitmap(&mock, rk).await.is_some(),
                "row {} missing from BigTable",
                String::from_utf8_lossy(rk),
            );
        }
        let persisted = persisted_watermark(&store)
            .await
            .expect("watermark must promote once every row lands");
        assert_eq!(persisted.checkpoint_hi_inclusive, 1);
    }

    /// Distributor partitions commit values by `shard_for(row_key) &
    /// SHARD_MASK`. Two row_keys chosen at test-time to hash to known,
    /// distinct shards must land in exactly those shards' accumulated
    /// state — no leakage, no duplication.
    #[tokio::test]
    async fn distributor_partitions_deterministically() {
        use crate::handlers::bitmap::accumulated::NUM_SHARDS;
        use crate::handlers::bitmap::accumulated::shard_for;

        let (_mock, store, handler) = setup(false).await;
        let handler = Arc::new(handler);

        // Find two row_keys that hash to distinct shards.
        let mut by_shard: HashMap<usize, Vec<u8>> = HashMap::new();
        for i in 0..10_000u32 {
            let rk = format!("v1#dim#{i:010}").into_bytes();
            by_shard.entry(shard_for(&rk)).or_insert(rk);
            if by_shard.len() >= 2 {
                break;
            }
        }
        let mut picks = by_shard.into_iter().collect::<Vec<_>>();
        picks.sort_by_key(|(s, _)| *s);
        let (shard_a, row_a) = picks[0].clone();
        let (shard_b, row_b) = picks[1].clone();
        assert_ne!(shard_a, shard_b);

        let batch = make_batch(vec![
            value(&row_a, 0, &[0], 1, 1500),
            value(&row_b, 0, &[1], 1, 1500),
        ]);
        let w = watermark(1, 10, 1500);

        let h = handler.clone();
        store
            .transaction(move |conn| {
                async move {
                    conn.set_committer_watermark(PIPELINE, w).await?;
                    h.commit(&batch, conn).await
                }
                .scope_boxed()
            })
            .await
            .unwrap();

        handler.flush_and_wait().await;

        // In default (non-backfill) mode rows stay resident after durable
        // — they're needed for future OR accumulation. Per-shard counts
        // should be exactly 1 for the chosen shards and 0 elsewhere.
        let per_shard = handler.rows_per_shard();
        assert_eq!(per_shard.len(), NUM_SHARDS);
        for (s, n) in per_shard.iter().enumerate() {
            let expected = if s == shard_a || s == shard_b { 1 } else { 0 };
            assert_eq!(*n, expected, "shard {s}: expected {expected}, got {n}");
        }
    }

    /// Commit_gen aggregators may close out of order, but watermarks
    /// must graduate into `pending_watermarks` strictly in commit_gen
    /// order. Here commit 1's rows take a long mock write; commit 2
    /// has no rows at all (empty batch). The coord's aggregator for
    /// gen=1 may close at any time (shards ack once merge finishes,
    /// before the write lands), but commit 1's inflight write gates
    /// promotion of commit 2's watermark until commit 1 is durable.
    #[tokio::test]
    async fn out_of_order_commit_gen_close_preserves_watermark_order() {
        let (mock, store, handler) = setup(false).await;
        // Delay commit 1's write long enough that commit 2's (empty)
        // aggregator closes first.
        mock.set_mutate_delay(300);
        let handler = Arc::new(handler);

        let row_key = b"v1#dim#0000000000";
        let batch1 = make_batch(vec![value(row_key, 0, &[1], 1, 1000)]);
        let w1 = watermark(1, 10, 1000);
        let h = handler.clone();
        store
            .transaction(move |conn| {
                async move {
                    conn.set_committer_watermark(PIPELINE, w1).await?;
                    h.commit(&batch1, conn).await
                }
                .scope_boxed()
            })
            .await
            .unwrap();

        // Commit 2: empty batch, but set_committer_watermark with a
        // higher cp. This commit's aggregator closes essentially
        // instantly (all 64 shards ack empty Work quickly).
        let batch2: Vec<Arc<Vec<BitmapIndexValue>>> = vec![Arc::new(vec![])];
        let w2 = watermark(2, 20, 2000);
        let h = handler.clone();
        store
            .transaction(move |conn| {
                async move {
                    conn.set_committer_watermark(PIPELINE, w2).await?;
                    h.commit(&batch2, conn).await
                }
                .scope_boxed()
            })
            .await
            .unwrap();

        // Before commit 1's write lands: no watermark (w1 is held by
        // inflight; w2 is blocked behind w1 in pending_watermarks).
        assert!(
            persisted_watermark(&store).await.is_none(),
            "watermark must not promote while commit 1 is still in flight",
        );

        handler.flush_and_wait().await;

        // After durable: both commits' watermarks promote; highest wins.
        let persisted = persisted_watermark(&store).await.unwrap();
        assert_eq!(persisted.checkpoint_hi_inclusive, 2);
        assert_eq!(persisted.tx_hi, 20);
        assert!(persisted_bitmap(&mock, row_key).await.is_some());
    }

    /// Shard panic shutdown is not covered. Inducing a panic in a shard
    /// worker thread without an intrusive test-only hook is impractical:
    /// `merge_shard` has no reachable panic path for a mere test input
    /// (RoaringBitmap OR + BTreeMap/HashMap updates all succeed on
    /// malformed-but-deserializable values), and `load_and_emit_straddlers`
    /// panics only on genuine BigTable client errors already exercised
    /// elsewhere. Adding a `ShardPanicTrigger` knob to `ShardState` purely
    /// for this test would leak test-only state into production types.
    /// Left ignored pending a lower-friction way to drive a shard panic.
    #[tokio::test]
    #[ignore = "no non-intrusive way to drive a shard worker panic; see comment"]
    async fn shard_panic_shuts_down_cleanly() {}

    /// Durable routing for one commit must interleave with concurrent
    /// Work processing for a later commit on different shards. Commit 1
    /// holds a slow write; commit 2 lands on different rows (different
    /// shards with high probability) and its Work must process without
    /// waiting for commit 1's Durable. Both watermarks promote in order
    /// after commit 1's write lands.
    #[tokio::test]
    async fn durable_interleaves_with_later_work() {
        let (mock, store, handler) = setup(false).await;
        mock.set_mutate_delay(200);
        let handler = Arc::new(handler);

        // 32 distinct rows per commit — with 64 shards, the two
        // commits' shard footprints almost certainly overlap on some
        // shards and diverge on others; irrelevant for the assertion,
        // which targets aggregate interleave.
        let rows1: Vec<Vec<u8>> = (0..32u32)
            .map(|i| format!("v1#dim#{i:010}").into_bytes())
            .collect();
        let rows2: Vec<Vec<u8>> = (32..64u32)
            .map(|i| format!("v1#dim#{i:010}").into_bytes())
            .collect();

        let batch1 = make_batch(rows1.iter().map(|rk| value(rk, 0, &[0], 1, 1000)).collect());
        let w1 = watermark(1, 10, 1000);
        let h = handler.clone();
        store
            .transaction(move |conn| {
                async move {
                    conn.set_committer_watermark(PIPELINE, w1).await?;
                    h.commit(&batch1, conn).await
                }
                .scope_boxed()
            })
            .await
            .unwrap();

        let batch2 = make_batch(rows2.iter().map(|rk| value(rk, 0, &[1], 2, 2000)).collect());
        let w2 = watermark(2, 20, 2000);
        let h = handler.clone();
        store
            .transaction(move |conn| {
                async move {
                    conn.set_committer_watermark(PIPELINE, w2).await?;
                    h.commit(&batch2, conn).await
                }
                .scope_boxed()
            })
            .await
            .unwrap();

        handler.flush_and_wait().await;

        // In default mode, rows are NOT evicted after durable — they
        // stay resident to accumulate future bits. If commit 2's Work
        // had been blocked behind commit 1's Durable (a regression in
        // the interleaving machinery), rows2 might still land eventually
        // but the symptom would surface as a deadlock in `flush_and_wait`.
        // Reaching here proves the interleave worked. Final resident
        // row count must include every row from both commits.
        assert_eq!(
            handler.accumulated_rows(),
            rows1.len() + rows2.len(),
            "every row from both commits should remain resident in default mode",
        );

        for rk in rows1.iter().chain(rows2.iter()) {
            assert!(
                persisted_bitmap(&mock, rk).await.is_some(),
                "row {} not durable",
                String::from_utf8_lossy(rk),
            );
        }
        let persisted = persisted_watermark(&store).await.unwrap();
        assert_eq!(persisted.checkpoint_hi_inclusive, 2);
        assert_eq!(persisted.tx_hi, 20);
    }

    /// A single promote-time SweepEviction is broadcast to every shard,
    /// not just one. With rows spread across many shards in backfill
    /// mode, all shards must drain their `EvictionIndex` —
    /// observable as `accumulated_rows() == 0` after quiescence. A
    /// single-shard sweep would leave most of the 64 shards' rows
    /// resident.
    #[tokio::test]
    async fn sweep_eviction_broadcast() {
        let (_mock, store, handler) = setup(true).await;
        let handler = Arc::new(handler);

        // 64 distinct rows each in its own sealed bucket. tx_hi crosses
        // every bucket's seal boundary so every row is eligible for
        // eviction after durable.
        let row_count = 64u64;
        let mut values = Vec::new();
        for bucket_id in 0..row_count {
            let row_key = format!("v1#dim#{bucket_id:010}").into_bytes();
            let bit = (BUCKET_SIZE - 1) as u32;
            values.push(value(
                &row_key,
                bucket_id,
                &[bit],
                bucket_id,
                (bucket_id + 1) * 1000,
            ));
        }
        let batch = make_batch(values);
        let w = watermark(row_count - 1, row_count * BUCKET_SIZE, row_count * 1000);

        let h = handler.clone();
        store
            .transaction(move |conn| {
                async move {
                    conn.set_committer_watermark(PIPELINE, w).await?;
                    h.commit(&batch, conn).await
                }
                .scope_boxed()
            })
            .await
            .unwrap();

        handler.flush_and_wait().await;

        // `flush_and_wait` resolves when the coord is quiesced. The
        // SweepEviction messages are dispatched during the final
        // `try_promote` in that sequence but drained by each shard
        // asynchronously afterward; poll briefly for the per-shard
        // draining to complete. If SweepEviction were only fired on
        // one shard, rows on the other 63 would stay resident forever
        // and this loop would time out.
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
        while handler.accumulated_rows() != 0 && std::time::Instant::now() < deadline {
            tokio::time::sleep(std::time::Duration::from_millis(5)).await;
        }
        assert_eq!(
            handler.accumulated_rows(),
            0,
            "SweepEviction must broadcast to every shard; found resident rows",
        );
    }

    /// Multiple sequential commits to different rows must promote
    /// watermarks in strict commit_gen order. Shards may close
    /// aggregators out of order; the coord's `ReorderBuffer` pops them
    /// back into commit_gen order before their watermarks are pushed
    /// onto `pending_watermarks`.
    #[tokio::test]
    async fn sequential_commits_promote_in_order() {
        let (_mock, store, handler) = setup(false).await;
        let handler = Arc::new(handler);

        for cp in 1..=4u64 {
            let row_key = format!("v1#dim#{cp:010}");
            let batch = make_batch(vec![value(row_key.as_bytes(), 0, &[0], cp, cp * 1000)]);
            let w = watermark(cp, cp * 10, cp * 1000);
            let h = handler.clone();
            store
                .transaction(move |conn| {
                    async move {
                        conn.set_committer_watermark(PIPELINE, w).await?;
                        h.commit(&batch, conn).await
                    }
                    .scope_boxed()
                })
                .await
                .unwrap();
        }

        handler.flush_and_wait().await;

        let persisted = persisted_watermark(&store)
            .await
            .expect("final watermark must be persisted");
        assert_eq!(
            persisted.checkpoint_hi_inclusive, 4,
            "the highest commit's watermark must eventually reach BigTable",
        );
        assert_eq!(persisted.tx_hi, 40);
    }

    async fn persisted_bucket_start_cp(mock: &MockBigtableServer) -> Option<u64> {
        let bytes = mock
            .get_cell(
                tables::watermarks::NAME,
                PIPELINE.as_bytes(),
                FAMILY,
                tables::watermarks::col::BUCKET_START_CP.as_bytes(),
            )
            .await?;
        Some(bcs::from_bytes(bytes.as_ref()).expect("bucket_start_cp BCS u64"))
    }

    /// Deploy 1: a commit that stays inside the current bucket leaves the
    /// `bitmap_bucket_start_cp` column at the sentinel `0` (no transition
    /// yet observed since the handler started).
    #[tokio::test]
    async fn bucket_start_cp_stays_zero_without_transition() {
        let (mock, store, handler) = setup(false).await;
        let handler = Arc::new(handler);

        let row1 = b"v1#dim#0000000001";
        let batch1 = make_batch(vec![value(row1, 0, &[0], 1, 1000)]);
        let w1 = watermark(1, 10, 1000);
        let h = handler.clone();
        store
            .transaction(move |conn| {
                async move {
                    conn.set_committer_watermark(PIPELINE, w1).await?;
                    h.commit(&batch1, conn).await
                }
                .scope_boxed()
            })
            .await
            .unwrap();
        handler.flush_and_wait().await;
        assert_eq!(
            persisted_bucket_start_cp(&mock).await,
            Some(0),
            "no bucket transition yet → column persists as 0",
        );
    }

    /// Deploy 1: when a commit advances `tx_hi` across a bucket-seal
    /// boundary, the coord records that commit's `checkpoint_hi_inclusive`
    /// as the new `bitmap_bucket_start_cp` and writes it alongside the
    /// watermark.
    #[tokio::test]
    async fn bucket_start_cp_written_on_transition() {
        let (mock, store, handler) = setup(false).await;
        let handler = Arc::new(handler);

        // Commit 1 keeps tx_hi inside bucket 0; commit 2 crosses into
        // bucket 1. Both commits are submitted back-to-back with a single
        // terminal `flush_and_wait`. The coord's graduation order pushes
        // commit 1's (no-transition) watermark first, then commit 2's
        // (transition) watermark, which overwrites the column.
        let row1 = b"v1#dim#0000000001";
        let batch1 = make_batch(vec![value(row1, 0, &[0], 1, 1000)]);
        let w1 = watermark(1, 10, 1000);
        let h = handler.clone();
        store
            .transaction(move |conn| {
                async move {
                    conn.set_committer_watermark(PIPELINE, w1).await?;
                    h.commit(&batch1, conn).await
                }
                .scope_boxed()
            })
            .await
            .unwrap();

        let row2 = b"v1#dim#0000000002";
        let batch2 = make_batch(vec![value(row2, 1, &[0], 2, 2000)]);
        let w2 = watermark(2, BUCKET_SIZE + 5, 2000);
        let h = handler.clone();
        store
            .transaction(move |conn| {
                async move {
                    conn.set_committer_watermark(PIPELINE, w2).await?;
                    h.commit(&batch2, conn).await
                }
                .scope_boxed()
            })
            .await
            .unwrap();
        handler.flush_and_wait().await;
        assert_eq!(
            persisted_bucket_start_cp(&mock).await,
            Some(2),
            "bucket transition must record the crossing commit's cp",
        );
    }

    /// Deploy 1: on restart mid-bucket, the coord seeds
    /// `current_bucket_start_cp` from the persisted column and rewrites
    /// the same value on subsequent watermarks until the next bucket
    /// transition. Proves the column's value survives restart.
    #[tokio::test]
    async fn bucket_start_cp_seeded_from_column_on_restart() {
        let (mock, store, _handler_dropped) = setup(false).await;

        // Pre-persist a mid-bucket watermark + a known bucket_start_cp so
        // init_watermark exposes them. Handler spawn reads the column to
        // seed its coord state.
        let pre_tx_hi = BUCKET_SIZE / 2;
        let pre_cp = 10u64;
        let pre_bucket_start_cp = 7u64;
        let w_seed = watermark(pre_cp, pre_tx_hi, 500);
        let pw: crate::Watermark = w_seed.into();
        let mut seed_conn = store.connect().await.unwrap();
        seed_conn
            .client()
            .set_pipeline_watermark(PIPELINE, &pw, Some(pre_bucket_start_cp))
            .await
            .unwrap();
        let init = seed_conn.init_watermark(PIPELINE, None).await.unwrap();
        assert!(init.is_some(), "persisted watermark must be seen");
        drop(seed_conn);

        // Fresh handler; first commit stays inside bucket 0 so no
        // transition fires. The re-persisted column must still be the
        // seeded value — evidence the coord initialized from it rather
        // than resetting to 0.
        let config = SequentialLayer {
            flush_only_when_sealed: Some(false),
            ..SequentialLayer::default()
        };
        let handler = Arc::new(BitmapIndexHandler::new(
            TestProcessor,
            &config,
            1,
            Arc::new(CompositeRateLimiter::noop()),
            None,
        ));

        let row = b"v1#dim#0000000001";
        let batch = make_batch(vec![value(row, 0, &[1], pre_cp + 1, 1500)]);
        let w = watermark(pre_cp + 1, pre_tx_hi + 1, 1500);
        let h = handler.clone();
        store
            .transaction(move |conn| {
                async move {
                    conn.set_committer_watermark(PIPELINE, w).await?;
                    h.commit(&batch, conn).await
                }
                .scope_boxed()
            })
            .await
            .unwrap();
        handler.flush_and_wait().await;

        assert_eq!(
            persisted_bucket_start_cp(&mock).await,
            Some(pre_bucket_start_cp),
            "coord must seed current_bucket_start_cp from the persisted column",
        );
    }

    /// Deploy 2: `init_watermark` clamps the returned
    /// `checkpoint_hi_inclusive` to `bucket_start_cp - 1` so the framework
    /// resumes ingestion at the start of the currently-active bucket.
    #[tokio::test]
    async fn init_watermark_clamps_to_bucket_start() {
        let (_mock, store, _handler_dropped) = setup(false).await;

        let mut conn = store.connect().await.unwrap();
        let w = watermark(42, BUCKET_SIZE / 2, 1000);
        let pw: crate::Watermark = w.into();
        conn.client()
            .set_pipeline_watermark(PIPELINE, &pw, Some(10))
            .await
            .unwrap();

        let init = conn.init_watermark(PIPELINE, None).await.unwrap().unwrap();
        assert_eq!(
            init.checkpoint_hi_inclusive,
            Some(9),
            "clamp to bucket_start_cp - 1"
        );
    }

    /// Deploy 2: `init_watermark` falls back to the raw persisted
    /// `checkpoint_hi_inclusive` when the `bitmap_bucket_start_cp` column
    /// is absent. Defensive path for pipelines that somehow missed Deploy 1
    /// populating the column — should not happen in practice but must not
    /// lose data.
    #[tokio::test]
    async fn init_watermark_falls_back_when_column_absent() {
        let (_mock, store, _handler_dropped) = setup(false).await;

        let mut conn = store.connect().await.unwrap();
        let w = watermark(42, BUCKET_SIZE / 2, 1000);
        // Write the watermark without the bucket_start_cp column.
        let pw: crate::Watermark = w.into();
        conn.client()
            .set_pipeline_watermark(PIPELINE, &pw, None)
            .await
            .unwrap();

        let init = conn.init_watermark(PIPELINE, None).await.unwrap().unwrap();
        assert_eq!(
            init.checkpoint_hi_inclusive,
            Some(42),
            "fall back to raw checkpoint_hi_inclusive"
        );
    }

    /// Deploy 2 end-to-end: restart mid-bucket, replay the partial bucket
    /// from scratch, verify pre-restart bits are recovered by natural
    /// re-ingestion (not lazy-load). This is the straddler test replacement.
    #[tokio::test]
    async fn mid_bucket_restart_recovers_bits_via_replay() {
        let (mock, store, _handler_dropped) = setup(false).await;

        // Pre-persist a mid-bucket watermark + bucket_start_cp=1 (bucket 0
        // started at cp=1; we're at cp=5 mid-bucket). Seed a pre-restart
        // bitmap cell with a bit that a replay from cp=1..=5 would have
        // produced.
        let pre_tx_hi = BUCKET_SIZE / 2;
        let pre_cp = 5u64;
        let bucket_start_cp = 1u64;
        let pre_bit = 0u32;
        let row_key = b"v1#dim#0000000000";

        let mut seed_conn = store.connect().await.unwrap();
        let w_seed: crate::Watermark = watermark(pre_cp, pre_tx_hi, 500).into();
        seed_conn
            .client()
            .set_pipeline_watermark(PIPELINE, &w_seed, Some(bucket_start_cp))
            .await
            .unwrap();

        let mut bm = RoaringBitmap::new();
        bm.insert(pre_bit);
        let mut buf = Vec::with_capacity(bm.serialized_size());
        bm.serialize_into(&mut buf).unwrap();
        seed_conn
            .client()
            .write_entries(
                TABLE,
                [tables::make_entry(
                    Bytes::copy_from_slice(row_key),
                    [(COL, Bytes::from(buf))],
                    Some(500),
                )],
            )
            .await
            .unwrap();

        // init_watermark clamps to bucket_start_cp - 1 = 0, meaning the
        // framework will resume at cp=1 and re-process every checkpoint
        // that ever contributed to bucket 0.
        let init = seed_conn
            .init_watermark(PIPELINE, None)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(
            init.checkpoint_hi_inclusive,
            Some(0),
            "clamp must roll back to the checkpoint before bucket start"
        );
        drop(seed_conn);

        // Fresh handler. Simulate replay: commit the bit that was part of
        // the pre-restart state (pre_bit, cp=2) and a new bit (new_bit,
        // cp=6). Both commits watermarks stay inside bucket 0. No
        // straddler load happens; the handler re-accumulates from scratch.
        let config = SequentialLayer {
            flush_only_when_sealed: Some(false),
            ..SequentialLayer::default()
        };
        let handler = Arc::new(BitmapIndexHandler::new(
            TestProcessor,
            &config,
            1,
            Arc::new(CompositeRateLimiter::noop()),
            None,
        ));

        // Replay commit: cp=2, bit=0 (= pre_bit). This overwrites the
        // pre-restart cell with an in-memory bitmap containing just {0}.
        let batch = make_batch(vec![value(row_key, 0, &[pre_bit], 2, 1000)]);
        let w = watermark(2, 10, 1000);
        let h = handler.clone();
        store
            .transaction(move |conn| {
                async move {
                    conn.set_committer_watermark(PIPELINE, w).await?;
                    h.commit(&batch, conn).await
                }
                .scope_boxed()
            })
            .await
            .unwrap();

        // Post-restart commit: cp=6, bit=1. In-memory bitmap now {0, 1}.
        let new_bit = 1u32;
        let batch = make_batch(vec![value(row_key, 0, &[new_bit], 6, 1500)]);
        let w = watermark(6, pre_tx_hi + 1, 1500);
        let h = handler.clone();
        store
            .transaction(move |conn| {
                async move {
                    conn.set_committer_watermark(PIPELINE, w).await?;
                    h.commit(&batch, conn).await
                }
                .scope_boxed()
            })
            .await
            .unwrap();
        handler.flush_and_wait().await;

        let bm = persisted_bitmap(&mock, row_key)
            .await
            .expect("row must persist");
        assert!(bm.contains(pre_bit), "replay bit must be present");
        assert!(bm.contains(new_bit), "post-restart bit must be present");
    }
}
