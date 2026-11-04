// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

use async_trait::async_trait;
use serde::Deserialize;
use serde::Serialize;
use sui_futures::service::Service;
use tokio::sync::mpsc;
use tracing::info;

use crate::config::ConcurrencyConfig;
use crate::ingestion::ingestion_client::CheckpointEnvelope;
use crate::metrics::IndexerMetrics;
use crate::pipeline::CommitterConfig;
use crate::pipeline::IngestionConfig;
use crate::pipeline::Processor;
use crate::pipeline::processor::processor;
use crate::pipeline::sequential::collector::BatchedRows;
use crate::pipeline::sequential::collector::collector;
use crate::pipeline::sequential::committer::committer;
use crate::store::SequentialStore;
use crate::store::Store;

mod collector;
mod committer;

/// Handlers implement the logic for a given indexing pipeline: How to process checkpoint data (by
/// implementing [Processor]) into rows for their table, how to combine multiple rows into a single
/// DB operation, and then how to write those rows atomically to the database.
///
/// The handler is also responsible for tuning the various parameters of the pipeline (provided as
/// associated values).
///
/// Sequential handlers can only be used in sequential pipelines, where checkpoint data is
/// processed out-of-order, but then gathered and written in order. If multiple checkpoints are
/// available, the pipeline will attempt to combine their writes taking advantage of batching to
/// avoid emitting redundant writes.
///
/// Back-pressure is handled by the bounded subscriber channel from the ingestion service, the
/// same as concurrent pipelines: the channel blocks broadcaster sends when full, and the adaptive
/// ingestion controller cuts fetch concurrency as the channel fills up.
#[async_trait]
pub trait Handler: Processor {
    type Store: SequentialStore;

    /// Soft cap: once this many rows are pending, the collector stops eagerly draining
    /// its input channel and yields so that a batch can be cut. Receive is never hard-gated —
    /// unlike concurrent pipelines, a missing predecessor may be buried in the input channel, and
    /// blocking receive would risk deadlock. The cap only bounds receive-to-write latency in the
    /// happy path.
    const MAX_PENDING_ROWS: usize = 5000;

    /// Maximum number of checkpoints to try and write in a single batch. The larger this number
    /// is, the more chances the pipeline has to merge redundant writes, but the longer each write
    /// transaction is likely to be.
    const MAX_BATCH_CHECKPOINTS: usize = 5 * 60;

    /// A type to combine multiple `Self::Value`-s into. This can be used to avoid redundant writes
    /// by combining multiple rows into one (e.g. if one row supersedes another, the latter can be
    /// omitted).
    type Batch: Default + Send + Sync + 'static;

    /// Add `values` from processing a checkpoint to the current `batch`. Checkpoints are
    /// guaranteed to be presented to the batch in checkpoint order. The handler takes ownership
    /// of the iterator and consumes all values.
    ///
    /// Returns `BatchStatus::Ready` if the batch is full and should be committed,
    /// or `BatchStatus::Pending` if the batch can accept more values.
    ///
    /// Note: The handler can signal batch readiness via `BatchStatus::Ready`, but the framework
    /// may also decide to commit a batch based on the trait parameters above.
    fn batch(&self, batch: &mut Self::Batch, values: std::vec::IntoIter<Self::Value>);

    /// Take a batch of values and commit them to the database, returning the number of rows
    /// affected.
    async fn commit<'a>(
        &self,
        batch: &Self::Batch,
        conn: &mut <Self::Store as Store>::Connection<'a>,
    ) -> anyhow::Result<usize>;
}

/// Configuration for a sequential pipeline
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct SequentialConfig {
    /// Configuration for the writer, that makes forward progress.
    pub committer: CommitterConfig,

    /// Per-pipeline ingestion overrides.
    pub ingestion: IngestionConfig,

    /// Processor concurrency. Defaults to adaptive scaling up to the number of CPUs.
    pub fanout: Option<ConcurrencyConfig>,

    /// Override for `Handler::MAX_PENDING_ROWS` (soft cap).
    pub max_pending_rows: Option<usize>,

    /// Override for `Handler::MAX_BATCH_CHECKPOINTS` (checkpoints per write batch).
    pub max_batch_checkpoints: Option<usize>,

    /// Size of the channel between the processor and committer.
    pub processor_channel_size: Option<usize>,

    /// Depth of the channel between the collector and committer tasks. Allows the collector
    /// to build the next batch while the previous batch is being flushed to the DB.
    pub pipeline_depth: Option<usize>,
}

/// Start a new sequential (in-order) indexing pipeline, served by the handler, `H`. Starting
/// strictly after the `watermark` (or from the beginning if no watermark was provided).
///
/// Each pipeline consists of a processor which takes checkpoint data and breaks it down into rows,
/// ready for insertion, and a committer which orders the rows and combines them into batches to
/// write to the database.
///
/// Commits are performed in checkpoint order, potentially involving multiple checkpoints at a
/// time. The call to [Handler::commit] and the associated watermark update are performed in a
/// transaction to ensure atomicity. Unlike in the case of concurrent pipelines, the data passed to
/// [Handler::commit] is not chunked up, so the handler must perform this step itself, if
/// necessary.
///
/// Checkpoint data is fed into the pipeline through the `checkpoint_rx` channel, and internal
/// channels are created to communicate between its various components. The pipeline will shutdown
/// if any of its input or output channels close, any of its independent tasks fail, or if it is
/// signalled to shutdown through the returned service handle.
pub(crate) fn pipeline<H: Handler>(
    handler: H,
    next_checkpoint: u64,
    config: SequentialConfig,
    store: H::Store,
    checkpoint_rx: mpsc::Receiver<Arc<CheckpointEnvelope>>,
    metrics: Arc<IndexerMetrics>,
) -> Service {
    info!(
        pipeline = H::NAME,
        "Starting pipeline with config: {config:#?}",
    );

    let concurrency = config
        .fanout
        .clone()
        .unwrap_or(ConcurrencyConfig::Adaptive {
            initial: 1,
            min: 1,
            max: num_cpus::get().max(1),
            dead_band: None,
        });
    let max_pending_rows = config.max_pending_rows.unwrap_or(H::MAX_PENDING_ROWS);
    let max_batch_checkpoints = config
        .max_batch_checkpoints
        .unwrap_or(H::MAX_BATCH_CHECKPOINTS)
        .max(1);

    let processor_channel_size = config
        .processor_channel_size
        .unwrap_or_else(|| num_cpus::get() / 2)
        .max(1);
    let (processor_tx, collector_rx) = mpsc::channel(processor_channel_size);

    let pipeline_depth = config
        .pipeline_depth
        .unwrap_or_else(|| num_cpus::get() / 2)
        .max(4);
    let (collector_tx, committer_rx) = mpsc::channel::<BatchedRows<H>>(pipeline_depth);

    let handler = Arc::new(handler);

    let s_processor = processor(
        handler.clone(),
        checkpoint_rx,
        processor_tx,
        metrics.clone(),
        concurrency,
        store.clone(),
    );

    let s_collector = collector::<H>(
        handler.clone(),
        config,
        next_checkpoint,
        collector_rx,
        metrics.clone(),
        max_pending_rows,
        max_batch_checkpoints,
        collector_tx,
    );

    let s_committer = committer::<H>(handler, store, metrics.clone(), committer_rx);

    s_processor.merge(s_collector).merge(s_committer)
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use prometheus::Registry;
    use sui_types::full_checkpoint_content::Checkpoint;

    use crate::mocks::store::FallibleMockConnection;
    use crate::mocks::store::FallibleMockStore;
    use crate::pipeline::IndexedCheckpoint;

    use super::*;

    // Test implementation of Handler
    #[derive(Default)]
    struct TestHandler;

    #[async_trait]
    impl Processor for TestHandler {
        const NAME: &'static str = "test";
        type Value = u64;

        async fn process(&self, _checkpoint: &Arc<Checkpoint>) -> anyhow::Result<Vec<Self::Value>> {
            Ok(vec![])
        }
    }

    #[async_trait]
    impl Handler for TestHandler {
        type Store = FallibleMockStore;
        type Batch = Vec<u64>;
        const MAX_BATCH_CHECKPOINTS: usize = 3; // Using small max value for testing.

        fn batch(&self, batch: &mut Self::Batch, values: std::vec::IntoIter<Self::Value>) {
            batch.extend(values);
        }

        async fn commit<'a>(
            &self,
            batch: &Self::Batch,
            conn: &mut FallibleMockConnection<'a>,
        ) -> anyhow::Result<usize> {
            if !batch.is_empty() {
                let mut sequential_data = conn.0.sequential_checkpoint_data.lock().unwrap();
                sequential_data.extend(batch.iter().cloned());
            }
            Ok(batch.len())
        }
    }

    struct TestSetup {
        store: FallibleMockStore,
        checkpoint_tx: mpsc::Sender<IndexedCheckpoint<TestHandler>>,
        #[allow(unused)]
        service: Service,
    }

    /// Emulates adding a sequential pipeline to the indexer. Bypasses the processor stage and
    /// feeds [IndexedCheckpoint]s directly to the collector. `next_checkpoint` is the starting
    /// checkpoint for the indexer.
    fn setup_test(
        next_checkpoint: u64,
        config: SequentialConfig,
        store: FallibleMockStore,
    ) -> TestSetup {
        let metrics = IndexerMetrics::new(None, &Registry::default());

        let max_pending_rows = config
            .max_pending_rows
            .unwrap_or(TestHandler::MAX_PENDING_ROWS);
        let max_batch_checkpoints = config
            .max_batch_checkpoints
            .unwrap_or(TestHandler::MAX_BATCH_CHECKPOINTS);
        let pipeline_depth = config
            .pipeline_depth
            .unwrap_or_else(|| num_cpus::get() / 2)
            .max(4);

        let (checkpoint_tx, checkpoint_rx) = mpsc::channel(10);
        let (collector_tx, committer_rx) =
            mpsc::channel::<BatchedRows<TestHandler>>(pipeline_depth);

        let store_clone = store.clone();
        let handler = Arc::new(TestHandler);

        let s_collector = collector(
            handler.clone(),
            config,
            next_checkpoint,
            checkpoint_rx,
            metrics.clone(),
            max_pending_rows,
            max_batch_checkpoints,
            collector_tx,
        );
        let s_committer = committer(handler, store_clone, metrics, committer_rx);

        TestSetup {
            store,
            checkpoint_tx,
            service: s_collector.merge(s_committer),
        }
    }

    async fn send_checkpoint(setup: &mut TestSetup, checkpoint: u64) {
        setup
            .checkpoint_tx
            .send(create_checkpoint(checkpoint))
            .await
            .unwrap();
    }

    fn create_checkpoint(checkpoint: u64) -> IndexedCheckpoint<TestHandler> {
        IndexedCheckpoint::new(
            checkpoint,        // epoch
            checkpoint,        // checkpoint number
            checkpoint,        // tx_hi
            checkpoint * 1000, // timestamp
            vec![checkpoint],  // values
        )
    }

    /// Starts only the collector, handing batches to a channel with room for `channel_size` of
    /// them, so tests can inspect each batch.
    fn spawn_collector(
        config: SequentialConfig,
        channel_size: usize,
    ) -> (
        mpsc::Sender<IndexedCheckpoint<TestHandler>>,
        mpsc::Receiver<BatchedRows<TestHandler>>,
        Service,
    ) {
        let metrics = IndexerMetrics::new(None, &Registry::default());
        let (checkpoint_tx, checkpoint_rx) = mpsc::channel(10);
        let (collector_tx, collector_rx) = mpsc::channel(channel_size);

        let service = collector(
            Arc::new(TestHandler),
            config,
            0,
            checkpoint_rx,
            metrics,
            TestHandler::MAX_PENDING_ROWS,
            TestHandler::MAX_BATCH_CHECKPOINTS,
            collector_tx,
        );

        (checkpoint_tx, collector_rx, service)
    }

    /// Configuration that holds rows until `min_batch_rows` are pending, or for at most
    /// `max_batch_delay_ms`.
    fn min_batch_config(min_batch_rows: usize, max_batch_delay_ms: u64) -> SequentialConfig {
        SequentialConfig {
            committer: CommitterConfig {
                min_batch_rows,
                max_batch_delay_ms,
                ..Default::default()
            },
            ..Default::default()
        }
    }

    async fn recv_batch(
        rx: &mut mpsc::Receiver<BatchedRows<TestHandler>>,
    ) -> BatchedRows<TestHandler> {
        tokio::time::timeout(Duration::from_secs(1), rx.recv())
            .await
            .expect("timed out waiting for a batch")
            .expect("collector closed its channel")
    }

    async fn expect_no_batch(rx: &mut mpsc::Receiver<BatchedRows<TestHandler>>) {
        let result = tokio::time::timeout(Duration::from_secs(1), rx.recv()).await;
        assert!(result.is_err(), "expected no batch");
    }

    #[tokio::test]
    async fn test_committer_processes_sequential_checkpoints() {
        let config = SequentialConfig::default();
        let mut setup = setup_test(0, config, FallibleMockStore::default());

        // Send checkpoints in order
        for i in 0..3 {
            send_checkpoint(&mut setup, i).await;
        }

        // Wait for processing
        tokio::time::sleep(Duration::from_millis(200)).await;

        // Verify data was written in order
        assert_eq!(setup.store.get_sequential_data(), vec![0, 1, 2]);

        // Verify watermark was updated
        let watermark = setup.store.watermark(TestHandler::NAME).unwrap();
        assert_eq!(watermark.checkpoint_hi_inclusive, Some(2));
        assert_eq!(watermark.tx_hi, 2);
    }

    /// Configure `FallibleMockStore` with no watermark, and emulate `first_checkpoint` by passing
    /// the `initial_watermark` into the setup.
    #[tokio::test]
    async fn test_committer_processes_sequential_checkpoints_with_initial_watermark() {
        let config = SequentialConfig::default();
        let mut setup = setup_test(5, config, FallibleMockStore::default());

        // Verify watermark hasn't progressed
        let watermark = setup.store.watermark(TestHandler::NAME);
        assert!(watermark.is_none());

        // Send checkpoints in order
        for i in 0..5 {
            send_checkpoint(&mut setup, i).await;
        }

        // Wait for processing
        tokio::time::sleep(Duration::from_millis(1000)).await;

        // Verify watermark hasn't progressed
        let watermark = setup.store.watermark(TestHandler::NAME);
        assert!(watermark.is_none());

        for i in 5..8 {
            send_checkpoint(&mut setup, i).await;
        }

        // Wait for processing
        tokio::time::sleep(Duration::from_millis(1000)).await;

        // Verify data was written in order
        assert_eq!(setup.store.get_sequential_data(), vec![5, 6, 7]);

        // Verify watermark was updated
        let watermark = setup.store.watermark(TestHandler::NAME).unwrap();
        assert_eq!(watermark.checkpoint_hi_inclusive, Some(7));
        assert_eq!(watermark.tx_hi, 7);
    }

    #[tokio::test]
    async fn test_committer_processes_out_of_order_checkpoints() {
        let config = SequentialConfig::default();
        let mut setup = setup_test(0, config, FallibleMockStore::default());

        // Send checkpoints out of order
        for i in [1, 0, 2] {
            send_checkpoint(&mut setup, i).await;
        }

        // Wait for processing
        tokio::time::sleep(Duration::from_millis(200)).await;

        // Verify data was written in order despite receiving out of order
        assert_eq!(setup.store.get_sequential_data(), vec![0, 1, 2]);

        // Verify watermark was updated
        let watermark = setup.store.watermark(TestHandler::NAME).unwrap();
        assert_eq!(watermark.checkpoint_hi_inclusive, Some(2));
        assert_eq!(watermark.tx_hi, 2);
    }

    #[tokio::test]
    async fn test_committer_commit_up_to_max_batch_checkpoints() {
        let config = SequentialConfig::default();
        let mut setup = setup_test(0, config, FallibleMockStore::default());

        // Send checkpoints up to MAX_BATCH_CHECKPOINTS
        for i in 0..4 {
            send_checkpoint(&mut setup, i).await;
        }

        // Wait for processing
        tokio::time::sleep(Duration::from_millis(200)).await;

        // Verify data is written in order across batches
        assert_eq!(setup.store.get_sequential_data(), vec![0, 1, 2, 3]);
    }

    /// The next checkpoint is cut into a batch as soon as the committer has room, and checkpoints
    /// that arrive while it has none are gathered into the next batch together.
    #[tokio::test(start_paused = true)]
    async fn collector_writes_as_soon_as_the_committer_has_room() {
        let (checkpoint_tx, mut batch_rx, _collector) =
            spawn_collector(SequentialConfig::default(), 1);

        checkpoint_tx.send(create_checkpoint(0)).await.unwrap();
        tokio::time::sleep(Duration::from_millis(10)).await;

        // The first batch now fills the committer's channel, so these have to wait.
        for i in 1..=3 {
            checkpoint_tx.send(create_checkpoint(i)).await.unwrap();
        }
        tokio::time::sleep(Duration::from_millis(10)).await;

        let batch = recv_batch(&mut batch_rx).await;
        assert_eq!(batch.batch, vec![0]);

        let batch = recv_batch(&mut batch_rx).await;
        assert_eq!(batch.batch, vec![1, 2, 3]);
        assert_eq!(batch.watermark.checkpoint_hi_inclusive, 3);
    }

    /// Checkpoints that arrive ahead of their predecessor are held until it arrives.
    #[tokio::test(start_paused = true)]
    async fn collector_waits_for_missing_predecessor() {
        let (checkpoint_tx, mut batch_rx, _collector) =
            spawn_collector(SequentialConfig::default(), 10);

        for i in [1, 2] {
            checkpoint_tx.send(create_checkpoint(i)).await.unwrap();
        }
        expect_no_batch(&mut batch_rx).await;

        checkpoint_tx.send(create_checkpoint(0)).await.unwrap();

        let batch = recv_batch(&mut batch_rx).await;
        assert_eq!(batch.batch, vec![0, 1, 2]);
    }

    #[tokio::test(start_paused = true)]
    async fn collector_holds_checkpoints_until_min_batch_rows() {
        let (checkpoint_tx, mut batch_rx, _collector) =
            spawn_collector(min_batch_config(3, 60_000), 10);

        for i in 0..2 {
            checkpoint_tx.send(create_checkpoint(i)).await.unwrap();
        }
        expect_no_batch(&mut batch_rx).await;

        checkpoint_tx.send(create_checkpoint(2)).await.unwrap();

        let batch = recv_batch(&mut batch_rx).await;
        assert_eq!(batch.batch, vec![0, 1, 2]);
    }

    #[tokio::test(start_paused = true)]
    async fn collector_writes_below_min_batch_rows_after_max_batch_delay() {
        let (checkpoint_tx, mut batch_rx, _collector) =
            spawn_collector(min_batch_config(3, 3_000), 10);

        for i in 0..2 {
            checkpoint_tx.send(create_checkpoint(i)).await.unwrap();
        }
        expect_no_batch(&mut batch_rx).await;

        tokio::time::sleep(Duration::from_secs(2)).await;
        let batch = recv_batch(&mut batch_rx).await;
        assert_eq!(batch.batch, vec![0, 1]);
    }

    #[tokio::test(start_paused = true)]
    async fn collector_writes_below_min_batch_rows_on_shutdown() {
        let (checkpoint_tx, mut batch_rx, mut collector) =
            spawn_collector(min_batch_config(100, 60_000), 10);

        for i in 0..2 {
            checkpoint_tx.send(create_checkpoint(i)).await.unwrap();
        }
        drop(checkpoint_tx);

        let batch = recv_batch(&mut batch_rx).await;
        assert_eq!(batch.batch, vec![0, 1]);

        tokio::time::timeout(Duration::from_secs(1), collector.join())
            .await
            .expect("collector shutdown timeout")
            .expect("collector shutdown failed");
    }

    #[tokio::test]
    async fn test_committer_retries_on_transaction_failure() {
        let config = SequentialConfig::default();

        // Create store with transaction failure configuration
        let store = FallibleMockStore::default().with_transaction_failures(1); // Will fail once before succeeding

        let mut setup = setup_test(10, config, store);

        // Send a checkpoint
        send_checkpoint(&mut setup, 10).await;

        // Wait long enough for the committer to complete one failed attempt and one successful
        // retry under exponential backoff (100ms initial).
        tokio::time::sleep(Duration::from_millis(1_500)).await;

        assert_eq!(setup.store.get_sequential_data(), vec![10]);
    }

    /// Smoke test for pipelined operation under a slow-commit store: with
    /// `pipeline_depth = 1` and two full batches to process, both batches must land and
    /// the watermark must reach the last checkpoint.
    #[tokio::test]
    async fn pipelined_commit_runs_under_slow_commit() {
        let config = SequentialConfig {
            committer: CommitterConfig::default(),
            max_batch_checkpoints: Some(3),
            pipeline_depth: Some(1),
            ..Default::default()
        };

        let store = FallibleMockStore::default().with_commit_delay(700);
        let mut setup = setup_test(0, config, store);

        for i in 0..6 {
            send_checkpoint(&mut setup, i).await;
        }

        // Two batches × 700ms commit each + slack.
        tokio::time::sleep(Duration::from_millis(2_000)).await;

        assert_eq!(setup.store.get_sequential_data(), vec![0, 1, 2, 3, 4, 5]);
        let watermark = setup.store.watermark(TestHandler::NAME).unwrap();
        assert_eq!(watermark.checkpoint_hi_inclusive, Some(5));
    }

    /// Watermarks must advance strictly in batch order, even under pipelining.
    #[tokio::test]
    async fn pipelined_commit_preserves_watermark_ordering() {
        let config = SequentialConfig {
            committer: CommitterConfig::default(),
            max_batch_checkpoints: Some(2),
            pipeline_depth: Some(2),
            ..Default::default()
        };

        let store = FallibleMockStore::default().with_commit_delay(100);
        let mut setup = setup_test(0, config, store);

        for i in 0..6 {
            send_checkpoint(&mut setup, i).await;
        }

        tokio::time::sleep(Duration::from_millis(1_500)).await;

        assert_eq!(setup.store.get_sequential_data(), vec![0, 1, 2, 3, 4, 5]);
        let watermark = setup.store.watermark(TestHandler::NAME).unwrap();
        assert_eq!(watermark.checkpoint_hi_inclusive, Some(5));
        assert_eq!(watermark.tx_hi, 5);
    }
}
