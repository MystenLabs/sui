// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{sync::Arc, time::Duration};

use serde::{Deserialize, Serialize};
use tokio::{sync::mpsc, task::JoinHandle};
use tokio_util::sync::CancellationToken;
use tracing::info;

use crate::{
    metrics::IndexerMetrics,
    store::{CommitterWatermark, Store},
    types::full_checkpoint_content::CheckpointData,
    FieldCount,
};

use super::{processor::processor, CommitterConfig, Processor, WatermarkPart, PIPELINE_BUFFER};

use self::{
    collector::collector, commit_watermark::commit_watermark, committer::committer, pruner::pruner,
    reader_watermark::reader_watermark,
};

mod collector;
mod commit_watermark;
mod committer;
mod pruner;
mod reader_watermark;

/// Handlers implement the logic for a given indexing pipeline: How to process checkpoint data (by
/// implementing [Processor]) into rows for their table, and how to write those rows to the database.
///
/// The handler is also responsible for tuning the various parameters of the pipeline (provided as
/// associated values). Reasonable defaults have been chosen to balance concurrency with memory
/// usage, but each handle may choose to override these defaults, e.g.
///
/// - Handlers that produce many small rows may wish to increase their batch/chunk/max-pending
///   sizes).
/// - Handlers that do more work during processing may wish to increase their fanout so more of it
///   can be done concurrently, to preserve throughput.
///
/// Concurrent handlers can only be used in concurrent pipelines, where checkpoint data is
/// processed and committed out-of-order and a watermark table is kept up-to-date with the latest
/// checkpoint below which all data has been committed.
///
/// Back-pressure is handled through the `MAX_PENDING_SIZE` constant -- if more than this many rows
/// build up, the collector will stop accepting new checkpoints, which will eventually propagate
/// back to the ingestion service.
#[async_trait::async_trait]
pub trait Handler: Processor<Value: FieldCount> {
    type Store: Store;

    /// If at least this many rows are pending, the committer will commit them eagerly.
    const MIN_EAGER_ROWS: usize = 50;

    /// If there are more than this many rows pending, the committer applies backpressure.
    const MAX_PENDING_ROWS: usize = 5000;

    /// The maximum number of watermarks that can show up in a single batch.
    /// This limit exists to deal with pipelines that produce no data for a majority of
    /// checkpoints -- the size of these pipeline's batches will be dominated by watermark updates.
    const MAX_WATERMARK_UPDATES: usize = 10_000;

    /// Take a chunk of values and commit them to the database, returning the number of rows
    /// affected.
    async fn commit<'a>(
        values: &[Self::Value],
        conn: &mut <Self::Store as Store>::Connection<'a>,
    ) -> anyhow::Result<usize>;

    /// Clean up data between checkpoints `_from` and `_to_exclusive` (exclusive) in the database, returning
    /// the number of rows affected. This function is optional, and defaults to not pruning at all.
    async fn prune<'a>(
        &self,
        _from: u64,
        _to_exclusive: u64,
        _conn: &mut <Self::Store as Store>::Connection<'a>,
    ) -> anyhow::Result<usize> {
        Ok(0)
    }
}

/// Configuration for a concurrent pipeline
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct ConcurrentConfig {
    /// Configuration for the writer, that makes forward progress.
    pub committer: CommitterConfig,

    /// Configuration for the pruner, that deletes old data.
    pub pruner: Option<PrunerConfig>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct PrunerConfig {
    /// How often the pruner should check whether there is any data to prune, in milliseconds.
    pub interval_ms: u64,

    /// How long to wait after the reader low watermark was set, until it is safe to prune up until
    /// this new watermark, in milliseconds.
    pub delay_ms: u64,

    /// How much data to keep, this is measured in checkpoints.
    pub retention: u64,

    /// The maximum range to try and prune in one request, measured in checkpoints.
    pub max_chunk_size: u64,

    /// The max number of tasks to run in parallel for pruning.
    pub prune_concurrency: u64,
}

/// Values ready to be written to the database. This is an internal type used to communicate
/// between the collector and the committer parts of the pipeline.
///
/// Values inside each batch may or may not be from the same checkpoint. Values in the same
/// checkpoint can also be split across multiple batches.
struct BatchedRows<H: Handler> {
    /// The rows to write
    values: Vec<H::Value>,
    /// Proportions of all the watermarks that are represented in this chunk
    watermark: Vec<WatermarkPart>,
}

impl PrunerConfig {
    pub fn interval(&self) -> Duration {
        Duration::from_millis(self.interval_ms)
    }

    pub fn delay(&self) -> Duration {
        Duration::from_millis(self.delay_ms)
    }
}

impl<H: Handler> BatchedRows<H> {
    fn new() -> Self {
        Self {
            values: vec![],
            watermark: vec![],
        }
    }

    /// Number of rows in this batch.
    fn len(&self) -> usize {
        self.values.len()
    }

    /// The batch is full if it has more than enough values to write to the database, or more than
    /// enough watermarks to update.
    fn is_full(&self) -> bool {
        self.values.len() >= max_chunk_rows::<H>()
            || self.watermark.len() >= H::MAX_WATERMARK_UPDATES
    }
}

impl Default for PrunerConfig {
    fn default() -> Self {
        Self {
            interval_ms: 300_000,
            delay_ms: 120_000,
            retention: 4_000_000,
            max_chunk_size: 2_000,
            prune_concurrency: 1,
        }
    }
}

/// Start a new concurrent (out-of-order) indexing pipeline served by the handler, `H`. Starting
/// strictly after the `watermark` (or from the beginning if no watermark was provided).
///
/// Each pipeline consists of a processor task which takes checkpoint data and breaks it down into
/// rows, ready for insertion, a collector which batches those rows into an appropriate size for
/// the database, a committer which writes the rows out concurrently, and a watermark task to
/// update the high watermark.
///
/// Committing is performed out-of-order: the pipeline may write out checkpoints out-of-order,
/// either because it received the checkpoints out-of-order or because of variance in processing
/// time.
///
/// The pipeline also maintains a row in the `watermarks` table for the pipeline which tracks the
/// watermark below which all data has been committed (modulo pruning), as long as `skip_watermark`
/// is not true.
///
/// Checkpoint data is fed into the pipeline through the `checkpoint_rx` channel, and internal
/// channels are created to communicate between its various components. The pipeline can be
/// shutdown using its `cancel` token, and will also shutdown if any of its independent tasks
/// reports an issue.
pub(crate) fn pipeline<H: Handler + Send + Sync + 'static>(
    handler: H,
    initial_commit_watermark: Option<CommitterWatermark>,
    config: ConcurrentConfig,
    skip_watermark: bool,
    store: H::Store,
    checkpoint_rx: mpsc::Receiver<Arc<CheckpointData>>,
    metrics: Arc<IndexerMetrics>,
    cancel: CancellationToken,
) -> JoinHandle<()> {
    info!(
        pipeline = H::NAME,
        "Starting pipeline with config: {:?}", config
    );
    let ConcurrentConfig {
        committer: committer_config,
        pruner: pruner_config,
    } = config;

    let (processor_tx, collector_rx) = mpsc::channel(H::FANOUT + PIPELINE_BUFFER);
    let (collector_tx, committer_rx) =
        mpsc::channel(committer_config.write_concurrency + PIPELINE_BUFFER);
    let (committer_tx, watermark_rx) =
        mpsc::channel(committer_config.write_concurrency + PIPELINE_BUFFER);

    // The pruner is not connected to the rest of the tasks by channels, so it needs to be
    // explicitly signalled to shutdown when the other tasks shutdown, in addition to listening to
    // the global cancel signal. We achieve this by creating a child cancel token that we call
    // cancel on once the committer tasks have shutdown.
    let pruner_cancel = cancel.child_token();
    let handler = Arc::new(handler);

    let processor = processor(
        handler.clone(),
        checkpoint_rx,
        processor_tx,
        metrics.clone(),
        cancel.clone(),
    );

    let collector = collector::<H>(
        committer_config.clone(),
        collector_rx,
        collector_tx,
        metrics.clone(),
        cancel.clone(),
    );

    let committer = committer::<H>(
        committer_config.clone(),
        skip_watermark,
        committer_rx,
        committer_tx,
        store.clone(),
        metrics.clone(),
        cancel.clone(),
    );

    let commit_watermark = commit_watermark::<H>(
        initial_commit_watermark,
        committer_config,
        skip_watermark,
        watermark_rx,
        store.clone(),
        metrics.clone(),
        cancel,
    );

    let reader_watermark = reader_watermark::<H>(
        pruner_config.clone(),
        store.clone(),
        metrics.clone(),
        pruner_cancel.clone(),
    );

    let pruner = pruner(
        handler,
        pruner_config,
        store,
        metrics,
        pruner_cancel.clone(),
    );

    tokio::spawn(async move {
        let (_, _, _, _) = futures::join!(processor, collector, committer, commit_watermark);

        pruner_cancel.cancel();
        let _ = futures::join!(reader_watermark, pruner);
    })
}

const fn max_chunk_rows<H: Handler>() -> usize {
    if H::Value::FIELD_COUNT == 0 {
        i16::MAX as usize
    } else {
        i16::MAX as usize / H::Value::FIELD_COUNT
    }
}

#[cfg(test)]
mod tests {
    use std::{sync::Arc, time::Duration};

    use async_trait::async_trait;
    use prometheus::Registry;
    use tokio::{sync::mpsc, time::timeout};
    use tokio_util::sync::CancellationToken;

    use crate::{
        metrics::IndexerMetrics,
        pipeline::Processor,
        store::CommitterWatermark,
        testing::mock_store::MockStore,
        types::{
            full_checkpoint_content::CheckpointData,
            test_checkpoint_data_builder::TestCheckpointDataBuilder,
        },
        FieldCount,
    };

    use super::*;

    const TEST_TIMEOUT: Duration = Duration::from_secs(60);
    const TEST_CHECKPOINT_BUFFER_SIZE: usize = 3; // Critical for back-pressure testing calculations

    #[derive(Clone, Debug, FieldCount)]
    struct TestValue {
        checkpoint: u64,
        data: u64,
    }

    struct DataPipeline;

    impl Processor for DataPipeline {
        const NAME: &'static str = "test_handler";
        const FANOUT: usize = 2;
        type Value = TestValue;

        fn process(&self, checkpoint: &Arc<CheckpointData>) -> anyhow::Result<Vec<Self::Value>> {
            let cp_num = checkpoint.checkpoint_summary.sequence_number;

            // Every checkpoint will come with 2 processed values
            Ok(vec![
                TestValue {
                    checkpoint: cp_num,
                    data: cp_num * 10 + 1,
                },
                TestValue {
                    checkpoint: cp_num,
                    data: cp_num * 10 + 2,
                },
            ])
        }
    }

    #[async_trait]
    impl Handler for DataPipeline {
        type Store = MockStore;
        const MIN_EAGER_ROWS: usize = 1000; // High value to disable eager batching
        const MAX_PENDING_ROWS: usize = 4; // Small value to trigger back pressure quickly
        const MAX_WATERMARK_UPDATES: usize = 1; // Each batch will have 1 checkpoint for an ease of testing.

        async fn commit<'a>(
            values: &[Self::Value],
            conn: &mut crate::testing::mock_store::MockConnection<'a>,
        ) -> anyhow::Result<usize> {
            // Group values by checkpoint
            let mut grouped: std::collections::HashMap<u64, Vec<u64>> =
                std::collections::HashMap::new();
            for value in values {
                grouped
                    .entry(value.checkpoint)
                    .or_default()
                    .push(value.data);
            }

            // Commit all data at once
            conn.0.commit_data(grouped).await
        }

        async fn prune<'a>(
            &self,
            from: u64,
            to_exclusive: u64,
            conn: &mut crate::testing::mock_store::MockConnection<'a>,
        ) -> anyhow::Result<usize> {
            conn.0.prune_data(from, to_exclusive)
        }
    }

    struct TestSetup {
        store: MockStore,
        checkpoint_tx: mpsc::Sender<Arc<CheckpointData>>,
        pipeline_handle: JoinHandle<()>,
        cancel: CancellationToken,
    }

    impl TestSetup {
        async fn new(
            config: ConcurrentConfig,
            store: MockStore,
            initial_watermark: Option<CommitterWatermark>,
        ) -> Self {
            let (checkpoint_tx, checkpoint_rx) = mpsc::channel(TEST_CHECKPOINT_BUFFER_SIZE);
            let metrics = IndexerMetrics::new(&Registry::default());
            let cancel = CancellationToken::new();

            let skip_watermark = false;
            let pipeline_handle = pipeline(
                DataPipeline,
                initial_watermark,
                config,
                skip_watermark,
                store.clone(),
                checkpoint_rx,
                metrics,
                cancel.clone(),
            );

            Self {
                store,
                checkpoint_tx,
                pipeline_handle,
                cancel,
            }
        }

        async fn send_checkpoint(&self, checkpoint: u64) -> anyhow::Result<()> {
            let checkpoint = Arc::new(
                TestCheckpointDataBuilder::new(checkpoint)
                    .with_epoch(1)
                    .with_network_total_transactions(checkpoint * 2)
                    .with_timestamp_ms(1000000000 + checkpoint * 1000)
                    .build_checkpoint(),
            );
            self.checkpoint_tx.send(checkpoint).await?;
            Ok(())
        }

        async fn shutdown(self) {
            drop(self.checkpoint_tx);
            self.cancel.cancel();
            let _ = self.pipeline_handle.await;
        }

        async fn send_checkpoint_with_timeout(
            &self,
            checkpoint: u64,
            timeout_duration: Duration,
        ) -> anyhow::Result<()> {
            timeout(timeout_duration, self.send_checkpoint(checkpoint)).await?
        }

        async fn send_checkpoint_expect_timeout(
            &self,
            checkpoint: u64,
            timeout_duration: Duration,
        ) {
            timeout(timeout_duration, self.send_checkpoint(checkpoint))
                .await
                .unwrap_err(); // Panics if send succeeds
        }
    }

    #[tokio::test]
    async fn test_e2e_pipeline() {
        let config = ConcurrentConfig {
            pruner: Some(PrunerConfig {
                interval_ms: 5_000, // Long interval to test states before pruning
                delay_ms: 100,      // Short delay for faster tests
                retention: 3,       // Keep only 3 checkpoints
                ..Default::default()
            }),
            ..Default::default()
        };
        let store = MockStore::default();
        let setup = TestSetup::new(config, store, None).await;

        // Send initial checkpoints
        for i in 0..3 {
            setup
                .send_checkpoint_with_timeout(i, Duration::from_millis(200))
                .await
                .unwrap();
        }

        // Verify all initial data is available (before any pruning)
        for i in 0..3 {
            let data = setup.store.wait_for_data(i, TEST_TIMEOUT).await;
            assert_eq!(data, vec![i * 10 + 1, i * 10 + 2]);
        }

        // Add more checkpoints to trigger pruning
        for i in 3..6 {
            setup
                .send_checkpoint_with_timeout(i, Duration::from_millis(200))
                .await
                .unwrap();
        }

        // Verify data is still available BEFORE pruning kicks in
        // With long pruning interval (5s), we can safely verify data without race conditions
        for i in 0..6 {
            let data = setup.store.wait_for_data(i, Duration::from_secs(1)).await;
            assert_eq!(data, vec![i * 10 + 1, i * 10 + 2]);
        }

        // Wait for pruning to occur (5s + delay + processing time)
        tokio::time::sleep(Duration::from_millis(5_200)).await;

        // Verify pruning has occurred
        {
            let data = setup.store.data.lock().unwrap();

            // Verify recent checkpoints are still available
            assert!(data.contains_key(&3));
            assert!(data.contains_key(&4));
            assert!(data.contains_key(&5));

            // Verify old checkpoints are pruned
            assert!(!data.contains_key(&0));
            assert!(!data.contains_key(&1));
            assert!(!data.contains_key(&2));
        };

        setup.shutdown().await;
    }

    #[tokio::test]
    async fn test_e2e_pipeline_without_pruning() {
        let config = ConcurrentConfig {
            pruner: None,
            ..Default::default()
        };
        let store = MockStore::default();
        let setup = TestSetup::new(config, store, None).await;

        // Send several checkpoints
        for i in 0..10 {
            setup
                .send_checkpoint_with_timeout(i, Duration::from_millis(200))
                .await
                .unwrap();
        }

        // Wait for all data to be processed and committed
        let watermark = setup.store.wait_for_watermark(9, TEST_TIMEOUT).await;

        // Verify ALL data was processed correctly (no pruning)
        for i in 0..10 {
            let data = setup.store.wait_for_data(i, Duration::from_secs(1)).await;
            assert_eq!(data, vec![i * 10 + 1, i * 10 + 2]);
        }

        // Verify watermark progression
        assert_eq!(watermark.checkpoint_hi_inclusive, 9);
        assert_eq!(watermark.tx_hi, 18); // 9 * 2
        assert_eq!(watermark.timestamp_ms_hi_inclusive, 1000009000); // 1000000000 + 9 * 1000

        // Verify no data was pruned - all 10 checkpoints should still exist
        let total_checkpoints = {
            let data = setup.store.data.lock().unwrap();
            data.len()
        };
        assert_eq!(total_checkpoints, 10);

        setup.shutdown().await;
    }

    #[tokio::test]
    async fn test_out_of_order_processing() {
        let config = ConcurrentConfig::default();
        let store = MockStore::default();
        let setup = TestSetup::new(config, store, None).await;

        // Send checkpoints out of order
        let checkpoints = vec![2, 0, 4, 1, 3];
        for cp in checkpoints {
            setup
                .send_checkpoint_with_timeout(cp, Duration::from_millis(200))
                .await
                .unwrap();
        }

        // Wait for all data to be processed
        let _watermark = setup
            .store
            .wait_for_watermark(4, Duration::from_secs(5))
            .await;

        // Verify all checkpoints were processed correctly despite out-of-order arrival
        for i in 0..5 {
            let data = setup.store.wait_for_data(i, Duration::from_secs(1)).await;
            assert_eq!(data, vec![i * 10 + 1, i * 10 + 2]);
        }

        setup.shutdown().await;
    }

    #[tokio::test]
    async fn test_watermark_progression_with_gaps() {
        let config = ConcurrentConfig::default();
        let store = MockStore::default();
        let setup = TestSetup::new(config, store, None).await;

        // Send checkpoints with a gap (0, 1, 3, 4) - missing checkpoint 2
        for cp in [0, 1, 3, 4] {
            setup
                .send_checkpoint_with_timeout(cp, Duration::from_millis(200))
                .await
                .unwrap();
        }

        // Wait for processing
        tokio::time::sleep(Duration::from_secs(1)).await;

        // Watermark should only progress to 1 (can't progress past the gap)
        let watermark = setup.store.get_watermark();
        assert_eq!(watermark.checkpoint_hi_inclusive, 1);

        // Now send the missing checkpoint 2
        setup
            .send_checkpoint_with_timeout(2, Duration::from_millis(200))
            .await
            .unwrap();

        // Now watermark should progress to 4
        let watermark = setup.store.wait_for_watermark(4, TEST_TIMEOUT).await;
        assert_eq!(watermark.checkpoint_hi_inclusive, 4);

        setup.shutdown().await;
    }

    // ==================== BACK-PRESSURE TESTING ====================

    #[tokio::test]
    async fn test_back_pressure_collector_max_pending_rows() {
        // Pipeline Diagram - Collector Back Pressure via MAX_PENDING_ROWS:
        //
        // ┌────────────┐    ┌────────────┐    ┌────────────┐    ┌────────────┐
        // │ Checkpoint │ ─► │ Processor  │ ─► │ Collector  │ ─► │ Committer  │
        // │   Input    │    │ (FANOUT=2) │    │            │    │            │
        // └────────────┘    └────────────┘    └[BOTTLENECK]┘    └────────────┘
        //                │                 │                 │
        //              [●●●]           [●●●●●●●]         [●●●●●●]
        //            buffer: 3         buffer: 7         buffer: 6
        //
        // BOTTLENECK: Collector stops accepting when pending rows ≥ MAX_PENDING_ROWS (4)

        let config = ConcurrentConfig {
            committer: CommitterConfig {
                collect_interval_ms: 5_000, // Long interval to prevent timer-driven collection
                write_concurrency: 1,
                ..Default::default()
            },
            ..Default::default()
        };
        let store = MockStore::default();
        let setup = TestSetup::new(config, store, None).await;

        // Wait for initial setup
        tokio::time::sleep(Duration::from_millis(200)).await;

        // Pipeline capacity analysis with collector back pressure:
        // Configuration: MAX_PENDING_ROWS=4, FANOUT=2, PIPELINE_BUFFER=5
        //
        // Channel and task breakdown:
        // - Checkpoint->Processor channel: 3 slots (TEST_CHECKPOINT_BUFFER_SIZE)
        // - Processor tasks: 2 tasks (FANOUT=2)
        // - Processor->Collector channel: 7 slots (FANOUT=2 + PIPELINE_BUFFER=5)
        // - Collector pending: 2 checkpoints × 2 values = 4 values (hits MAX_PENDING_ROWS=4)
        //
        // Total capacity: 3 + 2 + 7 + 2 = 14 checkpoints

        // Fill pipeline to capacity - these should all succeed
        for i in 0..14 {
            setup
                .send_checkpoint_with_timeout(i, Duration::from_millis(200))
                .await
                .unwrap();
        }

        // Checkpoint 14 should block due to MAX_PENDING_ROWS back pressure
        setup
            .send_checkpoint_expect_timeout(14, Duration::from_millis(200))
            .await;

        // Allow pipeline to drain by sending the blocked checkpoint with longer timeout
        setup
            .send_checkpoint_with_timeout(14, TEST_TIMEOUT)
            .await
            .unwrap();

        // Verify data was processed correctly
        let data = setup.store.wait_for_data(0, TEST_TIMEOUT).await;
        assert_eq!(data, vec![1, 2]);

        setup.shutdown().await;
    }

    #[tokio::test]
    async fn test_back_pressure_committer_slow_commits() {
        // Pipeline Diagram - Committer Back Pressure via Slow Database Commits:
        //
        // ┌────────────┐    ┌────────────┐    ┌────────────┐    ┌────────────┐
        // │ Checkpoint │ ─► │ Processor  │ ─► │ Collector  │ ─► │ Committer  │
        // │   Input    │    │ (FANOUT=2) │    │            │    │🐌 10s Delay│
        // └────────────┘    └────────────┘    └────────────┘    └[BOTTLENECK]┘
        //                │                 │                 │
        //              [●●●]           [●●●●●●●]         [●●●●●●]
        //            buffer: 3         buffer: 7         buffer: 6
        //
        // BOTTLENECK: Committer with 10s delay blocks entire pipeline

        let config = ConcurrentConfig {
            committer: CommitterConfig {
                write_concurrency: 1, // Single committer for deterministic blocking
                ..Default::default()
            },
            ..Default::default()
        };
        let store = MockStore::default().with_commit_delay(10_000); // 10 seconds delay
        let setup = TestSetup::new(config, store, None).await;

        // Pipeline capacity analysis with slow commits:
        // Configuration: FANOUT=2, write_concurrency=1, PIPELINE_BUFFER=5
        //
        // Channel and task breakdown:
        // - Checkpoint->Processor channel: 3 slots (TEST_CHECKPOINT_BUFFER_SIZE)
        // - Processor tasks: 2 tasks (FANOUT=2)
        // - Processor->Collector channel: 7 slots (FANOUT=2 + PIPELINE_BUFFER=5)
        // - Collector->Committer channel: 6 slots (write_concurrency=1 + PIPELINE_BUFFER=5)
        // - Committer task: 1 task (blocked by slow commit)
        //
        // Total theoretical capacity: 3 + 2 + 7 + 6 + 1 = 19 checkpoints

        // Fill pipeline to theoretical capacity - these should all succeed
        for i in 0..19 {
            setup
                .send_checkpoint_with_timeout(i, Duration::from_millis(100))
                .await
                .unwrap();
        }

        // Find the actual back pressure point
        // Due to non-determinism in collector's tokio::select!, the collector may consume
        // up to 2 checkpoints (filling MAX_PENDING_ROWS=4) before applying back pressure.
        // This means back pressure occurs somewhere in range 19-21.
        let mut back_pressure_checkpoint = None;
        for checkpoint in 19..22 {
            if setup
                .send_checkpoint_with_timeout(checkpoint, Duration::from_millis(100))
                .await
                .is_err()
            {
                back_pressure_checkpoint = Some(checkpoint);
                break;
            }
        }
        assert!(
            back_pressure_checkpoint.is_some(),
            "Back pressure should occur between checkpoints 19-21"
        );

        // Verify that some data has been processed (pipeline is working)
        setup.store.wait_for_any_data(TEST_TIMEOUT).await;

        // Allow pipeline to drain by sending the blocked checkpoint with longer timeout
        setup
            .send_checkpoint_with_timeout(back_pressure_checkpoint.unwrap(), TEST_TIMEOUT)
            .await
            .unwrap();

        setup.shutdown().await;
    }

    // ==================== FAILURE TESTING ====================

    #[tokio::test]
    async fn test_commit_failure_retry() {
        let config = ConcurrentConfig::default();
        let store = MockStore::default().with_commit_failures(2); // Fail 2 times, then succeed
        let setup = TestSetup::new(config, store, None).await;

        // Send a checkpoint
        setup
            .send_checkpoint_with_timeout(0, Duration::from_millis(200))
            .await
            .unwrap();

        // Should eventually succeed despite initial commit failures
        let _watermark = setup.store.wait_for_watermark(0, TEST_TIMEOUT).await;

        // Verify data was eventually committed
        let data = setup.store.wait_for_data(0, Duration::from_secs(1)).await;
        assert_eq!(data, vec![1, 2]);

        setup.shutdown().await;
    }

    #[tokio::test]
    async fn test_prune_failure_retry() {
        let config = ConcurrentConfig {
            pruner: Some(PrunerConfig {
                interval_ms: 2000, // 2 seconds interval for testing
                delay_ms: 100,     // Short delay
                retention: 2,      // Keep only 2 checkpoints
                ..Default::default()
            }),
            ..Default::default()
        };

        // Configure prune failures for range [0, 2) - fail twice then succeed
        let store = MockStore::default().with_prune_failures(0, 2, 1);
        let setup = TestSetup::new(config, store, None).await;

        // Send enough checkpoints to trigger pruning
        for i in 0..4 {
            setup
                .send_checkpoint_with_timeout(i, Duration::from_millis(200))
                .await
                .unwrap();
        }

        // Verify data is still available BEFORE pruning kicks in
        // With long pruning interval (5s), we can safely verify data without race conditions
        for i in 0..4 {
            let data = setup.store.wait_for_data(i, Duration::from_secs(1)).await;
            assert_eq!(data, vec![i * 10 + 1, i * 10 + 2]);
        }

        // Wait for first pruning attempt (should fail) and verify no data has been pruned
        setup
            .store
            .wait_for_prune_attempts(0, 2, 1, TEST_TIMEOUT)
            .await;
        {
            let data = setup.store.data.lock().unwrap();
            for i in 0..4 {
                assert!(data.contains_key(&i));
            }
        };

        // Wait for second pruning attempt (should succeed)
        setup
            .store
            .wait_for_prune_attempts(0, 2, 2, TEST_TIMEOUT)
            .await;
        {
            let data = setup.store.data.lock().unwrap();
            // Verify recent checkpoints are still available
            assert!(data.contains_key(&2));
            assert!(data.contains_key(&3));

            // Verify old checkpoints are pruned
            assert!(!data.contains_key(&0));
            assert!(!data.contains_key(&1));
        };

        setup.shutdown().await;
    }

    #[tokio::test]
    async fn test_reader_watermark_failure_retry() {
        let config = ConcurrentConfig {
            pruner: Some(PrunerConfig {
                interval_ms: 100, // Fast interval for testing
                delay_ms: 100,    // Short delay
                retention: 3,     // Keep 3 checkpoints
                ..Default::default()
            }),
            ..Default::default()
        };

        // Configure reader watermark failures - fail 2 times then succeed
        let store = MockStore::default().with_reader_watermark_failures(2);
        let setup = TestSetup::new(config, store, None).await;

        // Send checkpoints to trigger reader watermark updates
        for i in 0..6 {
            setup
                .send_checkpoint_with_timeout(i, Duration::from_millis(200))
                .await
                .unwrap();
        }

        // Wait for processing to complete
        let _watermark = setup.store.wait_for_watermark(5, TEST_TIMEOUT).await;

        // Wait for reader watermark task to attempt updates (with failures and retries)
        tokio::time::sleep(Duration::from_secs(2)).await;

        // Verify that reader watermark was eventually updated despite failures
        let watermark = setup.store.get_watermark();
        assert_eq!(watermark.reader_lo, 3);

        setup.shutdown().await;
    }

    #[tokio::test]
    async fn test_database_connection_failure_retry() {
        let config = ConcurrentConfig::default();
        let store = MockStore::default().with_connection_failures(2); // Fail 2 times, then succeed
        let setup = TestSetup::new(config, store, None).await;

        // Send a checkpoint
        setup
            .send_checkpoint_with_timeout(0, Duration::from_millis(200))
            .await
            .unwrap();

        // Should eventually succeed despite initial failures
        let _watermark = setup.store.wait_for_watermark(0, TEST_TIMEOUT).await;

        // Verify data was eventually committed
        let data = setup.store.wait_for_data(0, TEST_TIMEOUT).await;
        assert_eq!(data, vec![1, 2]);

        setup.shutdown().await;
    }
}
