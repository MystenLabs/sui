// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{
    collections::BTreeMap,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
};

use sui_futures::service::Service;
use tokio::{
    sync::{SetOnce, mpsc},
    time::{MissedTickBehavior, interval},
};
use tracing::{debug, info};

use crate::{
    metrics::{CheckpointLagMetricReporter, IndexerMetrics},
    pipeline::{CommitterConfig, IndexedCheckpoint, WatermarkPart},
};

use super::{BatchStatus, BatchedRows, Handler};

/// Processed values that are waiting to be written to the database. This is an internal type used
/// by the concurrent collector to hold data it is waiting to send to the committer.
struct PendingCheckpoint<H: Handler> {
    /// Iterator over values to be inserted into the database from this checkpoint
    values: std::vec::IntoIter<H::Value>,
    /// The watermark associated with this checkpoint and the part of it that is left to commit
    watermark: WatermarkPart,
}

impl<H: Handler> PendingCheckpoint<H> {
    /// Whether there are values left to commit from this indexed checkpoint.
    fn is_empty(&self) -> bool {
        let empty = self.values.len() == 0;
        debug_assert!(!empty || self.watermark.batch_rows == 0);
        empty
    }
}

impl<H: Handler> From<IndexedCheckpoint<H>> for PendingCheckpoint<H> {
    fn from(indexed: IndexedCheckpoint<H>) -> Self {
        let total_rows = indexed.values.len();
        Self {
            watermark: WatermarkPart {
                watermark: indexed.watermark,
                batch_rows: total_rows,
                total_rows,
            },
            values: indexed.values.into_iter(),
        }
    }
}

/// The collector task is responsible for gathering rows into batches which it then sends to a
/// committer task to write to the database. The task publishes batches in the following
/// circumstances:
///
/// - If `H::BATCH_SIZE` rows are pending, it will immediately schedule a batch to be gathered.
///
/// - If after sending one batch there is more data to be sent, it will immediately schedule the
///   next batch to be gathered (Each batch will contain at most `H::CHUNK_SIZE` rows).
///
/// - Otherwise, it will check for any data to write out at a regular interval (controlled by
///   `config.collect_interval()`).
///
/// The `main_reader_lo` tracks the lowest checkpoint that can be committed by this pipeline.
///
/// This task will shutdown if any of its channels are closed.
pub(super) fn collector<H: Handler + 'static>(
    handler: Arc<H>,
    config: CommitterConfig,
    mut rx: mpsc::Receiver<IndexedCheckpoint<H>>,
    tx: mpsc::Sender<BatchedRows<H>>,
    main_reader_lo: Arc<SetOnce<AtomicU64>>,
    metrics: Arc<IndexerMetrics>,
) -> Service {
    Service::new().spawn_aborting(async move {
        // The `poll` interval controls the maximum time to wait between collecting batches,
        // regardless of number of rows pending.
        let mut poll = interval(config.collect_interval());
        poll.set_missed_tick_behavior(MissedTickBehavior::Delay);

        let checkpoint_lag_reporter = CheckpointLagMetricReporter::new_for_pipeline::<H>(
            &metrics.collected_checkpoint_timestamp_lag,
            &metrics.latest_collected_checkpoint_timestamp_lag_ms,
            &metrics.latest_collected_checkpoint,
        );

        // Data for checkpoints that are ready to be sent but haven't been written yet.
        let mut pending: BTreeMap<u64, PendingCheckpoint<H>> = BTreeMap::new();
        let mut pending_rows = 0;

        info!(pipeline = H::NAME, "Starting collector");

        loop {
            tokio::select! {
                // Time to create another batch and push it to the committer.
                _ = poll.tick() => {
                    let guard = metrics
                        .collector_gather_latency
                        .with_label_values(&[H::NAME])
                        .start_timer();

                    let mut batch = H::Batch::default();
                    let mut watermark = Vec::new();
                    let mut batch_len = 0;

                    loop {
                        let Some(mut entry) = pending.first_entry() else {
                            break;
                        };

                        if watermark.len() >= H::MAX_WATERMARK_UPDATES {
                            break;
                        }

                        let indexed = entry.get_mut();
                        let before = indexed.values.len();
                        let status = handler.batch(&mut batch, &mut indexed.values);
                        let taken = before - indexed.values.len();

                        batch_len += taken;
                        watermark.push(indexed.watermark.take(taken));
                        if indexed.is_empty() {
                            checkpoint_lag_reporter.report_lag(
                                indexed.watermark.checkpoint(),
                                indexed.watermark.timestamp_ms(),
                            );
                            entry.remove();
                        }

                        if status == BatchStatus::Ready {
                            // Batch is full, send it
                            break;
                        }
                    }
                    pending_rows -= batch_len;
                    let elapsed = guard.stop_and_record();
                    debug!(
                        pipeline = H::NAME,
                        elapsed_ms = elapsed * 1000.0,
                        rows = batch_len,
                        pending_rows = pending_rows,
                        "Gathered batch",
                    );

                    metrics
                        .total_collector_batches_created
                        .with_label_values(&[H::NAME])
                        .inc();

                    metrics
                        .collector_batch_size
                        .with_label_values(&[H::NAME])
                        .observe(batch_len as f64);

                    let batched_rows = BatchedRows {
                        batch,
                        batch_len,
                        watermark,
                    };

                    if tx.send(batched_rows).await.is_err() {
                        info!(pipeline = H::NAME, "Committer closed channel, stopping collector");
                        break;
                    }

                    if pending_rows > 0 {
                        poll.reset_immediately();
                    } else if rx.is_closed() && rx.is_empty() {
                        info!(
                            pipeline = H::NAME,
                            "Processor closed channel, pending rows empty, stopping collector",
                        );
                        break;
                    }
                }

                // docs::#collector (see docs/content/guides/developer/advanced/custom-indexer.mdx)
                Some(mut indexed) = rx.recv(), if pending_rows < H::MAX_PENDING_ROWS => {
                    // Clear the values of outdated checkpoints, so that we don't commit data to the
                    // store, but can still advance watermarks.
                    if indexed.checkpoint() < main_reader_lo.wait().await.load(Ordering::Relaxed) {
                        indexed.values.clear();
                        metrics.total_collector_skipped_checkpoints
                            .with_label_values(&[H::NAME])
                            .inc();
                    }

                    metrics
                        .total_collector_rows_received
                        .with_label_values(&[H::NAME])
                        .inc_by(indexed.len() as u64);
                    metrics
                        .total_collector_checkpoints_received
                        .with_label_values(&[H::NAME])
                        .inc();

                    pending_rows += indexed.len();
                    pending.insert(indexed.checkpoint(), indexed.into());

                    if pending_rows >= H::MIN_EAGER_ROWS {
                        poll.reset_immediately()
                    }
                }
                // docs::/#collector
            }
        }

        Ok(())
    })
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use async_trait::async_trait;
    use sui_pg_db::{Connection, Db};
    use tokio::sync::mpsc;

    use crate::{
        metrics::tests::test_metrics,
        pipeline::{Processor, concurrent::BatchStatus},
        types::full_checkpoint_content::Checkpoint,
    };

    use super::*;

    #[derive(Clone)]
    struct Entry;

    struct TestHandler;

    // Max chunk rows for testing - simulates postgres bind parameter limit
    const TEST_MAX_CHUNK_ROWS: usize = 1024;

    #[async_trait]
    impl Processor for TestHandler {
        type Value = Entry;
        const NAME: &'static str = "test_handler";
        const FANOUT: usize = 1;

        async fn process(&self, _checkpoint: &Arc<Checkpoint>) -> anyhow::Result<Vec<Self::Value>> {
            Ok(vec![])
        }
    }

    #[async_trait]
    impl Handler for TestHandler {
        type Store = Db;
        type Batch = Vec<Entry>;

        const MIN_EAGER_ROWS: usize = 10;
        const MAX_PENDING_ROWS: usize = 10000;

        fn batch(
            &self,
            batch: &mut Self::Batch,
            values: &mut std::vec::IntoIter<Self::Value>,
        ) -> BatchStatus {
            // Simulate batch size limit
            let remaining_capacity = TEST_MAX_CHUNK_ROWS.saturating_sub(batch.len());
            let to_take = remaining_capacity.min(values.len());
            batch.extend(values.take(to_take));

            if batch.len() >= TEST_MAX_CHUNK_ROWS {
                BatchStatus::Ready
            } else {
                BatchStatus::Pending
            }
        }

        async fn commit<'a>(
            &self,
            _batch: &Self::Batch,
            _conn: &mut Connection<'a>,
        ) -> anyhow::Result<usize> {
            tokio::time::sleep(Duration::from_millis(1000)).await;
            Ok(0)
        }
    }

    /// Wait for a timeout on the channel, expecting this operation to timeout.
    async fn expect_timeout<H: Handler + 'static>(
        rx: &mut mpsc::Receiver<BatchedRows<H>>,
        duration: Duration,
    ) {
        match tokio::time::timeout(duration, rx.recv()).await {
            Err(_) => (), // Expected timeout - test passes
            Ok(_) => panic!("Expected timeout but received data instead"),
        }
    }

    /// Receive from the channel with a given timeout, panicking if the timeout is reached or the
    /// channel is closed.
    async fn recv_with_timeout<H: Handler + 'static>(
        rx: &mut mpsc::Receiver<BatchedRows<H>>,
        timeout: Duration,
    ) -> BatchedRows<H> {
        match tokio::time::timeout(timeout, rx.recv()).await {
            Ok(Some(batch)) => batch,
            Ok(None) => panic!("Collector channel was closed unexpectedly"),
            Err(_) => panic!("Test timed out waiting for batch from collector"),
        }
    }

    #[tokio::test]
    async fn test_collector_batches_data() {
        let (processor_tx, processor_rx) = mpsc::channel(10);
        let (collector_tx, mut collector_rx) = mpsc::channel(10);
        let main_reader_lo = Arc::new(SetOnce::new_with(Some(AtomicU64::new(0))));

        let handler = Arc::new(TestHandler);
        let _collector = collector::<TestHandler>(
            handler,
            CommitterConfig::default(),
            processor_rx,
            collector_tx,
            main_reader_lo.clone(),
            test_metrics(),
        );

        let part1_length = TEST_MAX_CHUNK_ROWS / 2;
        let part2_length = TEST_MAX_CHUNK_ROWS - part1_length - 1;

        // Send test data
        let test_data = vec![
            IndexedCheckpoint::new(0, 1, 10, 1000, vec![Entry; part1_length]),
            IndexedCheckpoint::new(0, 2, 20, 2000, vec![Entry; part2_length]),
            IndexedCheckpoint::new(0, 3, 30, 3000, vec![Entry, Entry]),
        ];

        for data in test_data {
            processor_tx.send(data).await.unwrap();
        }

        let batch1 = recv_with_timeout(&mut collector_rx, Duration::from_secs(1)).await;
        assert_eq!(batch1.batch_len, TEST_MAX_CHUNK_ROWS);

        let batch2 = recv_with_timeout(&mut collector_rx, Duration::from_secs(1)).await;
        assert_eq!(batch2.batch_len, 1);

        let batch3 = recv_with_timeout(&mut collector_rx, Duration::from_secs(1)).await;
        assert_eq!(batch3.batch_len, 0);
    }

    #[tokio::test]
    async fn test_collector_shutdown() {
        let (processor_tx, processor_rx) = mpsc::channel(10);
        let (collector_tx, mut collector_rx) = mpsc::channel(10);
        let main_reader_lo = Arc::new(SetOnce::new_with(Some(AtomicU64::new(0))));

        let handler = Arc::new(TestHandler);
        let mut collector = collector::<TestHandler>(
            handler,
            CommitterConfig::default(),
            processor_rx,
            collector_tx,
            main_reader_lo,
            test_metrics(),
        );

        processor_tx
            .send(IndexedCheckpoint::new(0, 1, 10, 1000, vec![Entry, Entry]))
            .await
            .unwrap();

        tokio::time::sleep(Duration::from_millis(200)).await;

        let batch = recv_with_timeout(&mut collector_rx, Duration::from_secs(1)).await;
        assert_eq!(batch.batch_len, 2);

        // Drop processor sender to simulate shutdown
        drop(processor_tx);

        // After a short delay, collector should shut down
        tokio::time::timeout(Duration::from_millis(500), collector.join())
            .await
            .expect("collector shutdown timeout")
            .expect("collector shutdown failed");
    }

    #[tokio::test]
    async fn test_collector_respects_max_pending() {
        let processor_channel_size = 5; // unit is checkpoint
        let collector_channel_size = 2; // unit is batch, aka rows / MAX_CHUNK_ROWS
        let (processor_tx, processor_rx) = mpsc::channel(processor_channel_size);
        let (collector_tx, _collector_rx) = mpsc::channel(collector_channel_size);
        let main_reader_lo = Arc::new(SetOnce::new_with(Some(AtomicU64::new(0))));

        let metrics = test_metrics();

        let handler = Arc::new(TestHandler);
        let _collector = collector::<TestHandler>(
            handler,
            CommitterConfig::default(),
            processor_rx,
            collector_tx,
            main_reader_lo.clone(),
            metrics.clone(),
        );

        // Send more data than MAX_PENDING_ROWS plus collector channel buffer
        let data = IndexedCheckpoint::new(
            0,
            1,
            10,
            1000,
            vec![
                Entry;
                // Decreasing this number by even 1 would make the test fail.
                TestHandler::MAX_PENDING_ROWS
                    + TEST_MAX_CHUNK_ROWS * collector_channel_size
            ],
        );
        processor_tx.send(data).await.unwrap();

        tokio::time::sleep(Duration::from_millis(200)).await;

        // Now fill up the processor channel with minimum data to trigger send blocking
        for _ in 0..processor_channel_size {
            let more_data = IndexedCheckpoint::new(0, 2, 11, 1000, vec![Entry]);
            processor_tx.send(more_data).await.unwrap();
        }

        // Now sending even more data should block because of MAX_PENDING_ROWS limit.
        let even_more_data = IndexedCheckpoint::new(0, 3, 12, 1000, vec![Entry]);

        let send_result = processor_tx.try_send(even_more_data);
        assert!(matches!(
            send_result,
            Err(mpsc::error::TrySendError::Full(_))
        ));
    }

    #[tokio::test]
    async fn test_collector_accumulates_across_checkpoints_until_eager_threshold() {
        let (processor_tx, processor_rx) = mpsc::channel(10);
        let (collector_tx, mut collector_rx) = mpsc::channel(10);
        let main_reader_lo = Arc::new(SetOnce::new_with(Some(AtomicU64::new(0))));

        // Set a very long collect interval (60 seconds) to ensure timing doesn't trigger batching
        let config = CommitterConfig {
            collect_interval_ms: 60_000,
            ..CommitterConfig::default()
        };
        let handler = Arc::new(TestHandler);
        let _collector = collector::<TestHandler>(
            handler,
            config,
            processor_rx,
            collector_tx,
            main_reader_lo.clone(),
            test_metrics(),
        );

        let start_time = std::time::Instant::now();

        // The collector starts with an immediate poll tick, creating an empty batch
        let initial_batch = recv_with_timeout(&mut collector_rx, Duration::from_secs(1)).await;
        assert_eq!(initial_batch.batch_len, 0);

        // Send data that's just below MIN_EAGER_ROWS threshold.
        let below_threshold =
            IndexedCheckpoint::new(0, 1, 10, 1000, vec![Entry; TestHandler::MIN_EAGER_ROWS - 1]);
        processor_tx.send(below_threshold).await.unwrap();

        // Try to receive with timeout - should timeout since we're below threshold
        expect_timeout(&mut collector_rx, Duration::from_secs(1)).await;

        // Now send one more entry to cross the MIN_EAGER_ROWS threshold
        let threshold_trigger = IndexedCheckpoint::new(
            0,
            2,
            20,
            2000,
            vec![Entry; 1], // Just 1 more entry to reach 10 total
        );
        processor_tx.send(threshold_trigger).await.unwrap();

        // Should immediately get a batch without waiting for the long interval
        let eager_batch = recv_with_timeout(&mut collector_rx, Duration::from_secs(1)).await;
        assert_eq!(eager_batch.batch_len, TestHandler::MIN_EAGER_ROWS);

        // Verify batch was created quickly (much less than 60 seconds)
        let elapsed = start_time.elapsed();
        assert!(elapsed < Duration::from_secs(10));
    }

    #[tokio::test]
    async fn test_immediate_batch_on_min_eager_rows() {
        let (processor_tx, processor_rx) = mpsc::channel(10);
        let (collector_tx, mut collector_rx) = mpsc::channel(10);
        let main_reader_lo = Arc::new(SetOnce::new_with(Some(AtomicU64::new(0))));

        // Set a very long collect interval (60 seconds) to ensure timing doesn't trigger batching
        let config = CommitterConfig {
            collect_interval_ms: 60_000,
            ..CommitterConfig::default()
        };
        let handler = Arc::new(TestHandler);
        let _collector = collector::<TestHandler>(
            handler,
            config,
            processor_rx,
            collector_tx,
            main_reader_lo.clone(),
            test_metrics(),
        );

        // The collector starts with an immediate poll tick, creating an empty batch
        let initial_batch = recv_with_timeout(&mut collector_rx, Duration::from_secs(1)).await;
        assert_eq!(initial_batch.batch_len, 0);
        // The collector will then just wait for the next poll as there is no new data yet.
        expect_timeout(&mut collector_rx, Duration::from_secs(1)).await;

        let start_time = std::time::Instant::now();

        // Send exactly MIN_EAGER_ROWS in one checkpoint
        let exact_threshold =
            IndexedCheckpoint::new(0, 1, 10, 1000, vec![Entry; TestHandler::MIN_EAGER_ROWS]);
        processor_tx.send(exact_threshold).await.unwrap();

        // Should trigger immediately since pending_rows >= MIN_EAGER_ROWS.
        let batch = recv_with_timeout(&mut collector_rx, Duration::from_secs(1)).await;
        assert_eq!(batch.batch_len, TestHandler::MIN_EAGER_ROWS);

        // Verify batch was created quickly (much less than 60 seconds)
        let elapsed = start_time.elapsed();
        assert!(elapsed < Duration::from_secs(10));
    }

    #[tokio::test]
    async fn test_collector_waits_for_timer_when_below_eager_threshold() {
        let (processor_tx, processor_rx) = mpsc::channel(10);
        let (collector_tx, mut collector_rx) = mpsc::channel(10);
        let main_reader_lo = Arc::new(SetOnce::new_with(Some(AtomicU64::new(0))));

        // Set a reasonable collect interval for this test (3 seconds).
        let config = CommitterConfig {
            collect_interval_ms: 3000,
            ..CommitterConfig::default()
        };
        let handler = Arc::new(TestHandler);
        let _collector = collector::<TestHandler>(
            handler,
            config,
            processor_rx,
            collector_tx,
            main_reader_lo.clone(),
            test_metrics(),
        );

        // Consume initial empty batch
        let initial_batch = recv_with_timeout(&mut collector_rx, Duration::from_secs(1)).await;
        assert_eq!(initial_batch.batch_len, 0);

        // Send MIN_EAGER_ROWS - 1 entries (below threshold)
        let below_threshold =
            IndexedCheckpoint::new(0, 1, 10, 1000, vec![Entry; TestHandler::MIN_EAGER_ROWS - 1]);
        processor_tx.send(below_threshold).await.unwrap();

        // Try to receive with timeout - should timeout since we're below threshold
        expect_timeout(&mut collector_rx, Duration::from_secs(1)).await;

        // Should eventually get batch when timer triggers
        let timer_batch = recv_with_timeout(&mut collector_rx, Duration::from_secs(4)).await;
        assert_eq!(timer_batch.batch_len, TestHandler::MIN_EAGER_ROWS - 1);
    }

    /// The collector must wait for `main_reader_lo` to be initialized before attempting to prepare
    /// checkpoints for commit.
    #[tokio::test(start_paused = true)]
    async fn test_collector_waits_for_main_reader_lo_init() {
        let (processor_tx, processor_rx) = mpsc::channel(10);
        let (collector_tx, mut collector_rx) = mpsc::channel(10);
        let main_reader_lo = Arc::new(SetOnce::new());

        let handler = Arc::new(TestHandler);
        let collector = collector(
            handler,
            CommitterConfig {
                // Collect interval longer than time to advance to ensure timing doesn't trigger
                // batching.
                collect_interval_ms: 200_000,
                ..CommitterConfig::default()
            },
            processor_rx,
            collector_tx,
            main_reader_lo.clone(),
            test_metrics(),
        );

        // Send enough data to trigger batching.
        let test_data =
            IndexedCheckpoint::new(0, 1, 10, 1000, vec![Entry; TestHandler::MIN_EAGER_ROWS + 1]);
        processor_tx.send(test_data).await.unwrap();

        // Advance time significantly - collector should still be blocked waiting for
        // main_reader_lo.
        tokio::time::advance(Duration::from_secs(100)).await;

        assert!(collector_rx.try_recv().is_err());

        // Now initialize the main reader lo to 0, unblocking the collector.
        main_reader_lo.set(AtomicU64::new(0)).ok();

        tokio::time::advance(Duration::from_secs(1)).await;

        let batch = recv_with_timeout(&mut collector_rx, Duration::from_secs(2)).await;

        assert_eq!(batch.batch_len, TestHandler::MIN_EAGER_ROWS + 1);

        collector.shutdown().await.unwrap();
    }

    /// When receiving checkpoints, if they are below the main reader lo, they should be dropped
    /// immediately.
    #[tokio::test]
    async fn test_collector_drops_checkpoints_immediately_if_le_main_reader_lo() {
        let (processor_tx, processor_rx) = mpsc::channel(10);
        let (collector_tx, mut collector_rx) = mpsc::channel(10);
        let main_reader_lo = Arc::new(SetOnce::new_with(Some(AtomicU64::new(5))));
        let metrics = test_metrics();

        let collector = collector(
            Arc::new(TestHandler),
            CommitterConfig {
                // Collect interval longer than time to advance to ensure timing doesn't trigger
                // batching.
                collect_interval_ms: 200_000,
                ..CommitterConfig::default()
            },
            processor_rx,
            collector_tx,
            main_reader_lo.clone(),
            metrics.clone(),
        );

        let eager_rows_plus_one = TestHandler::MIN_EAGER_ROWS + 1;

        let test_data: Vec<_> = [1, 5, 2, 6, 4, 3]
            .into_iter()
            .map(|cp| IndexedCheckpoint::new(0, cp, 10, 1000, vec![Entry; eager_rows_plus_one]))
            .collect();
        for data in test_data {
            processor_tx.send(data).await.unwrap();
        }
        let batch = recv_with_timeout(&mut collector_rx, Duration::from_secs(2)).await;

        // Make sure that we are advancing watermarks.
        assert_eq!(batch.watermark.len(), 6);
        // And reporting the checkpoints as received.
        assert_eq!(
            metrics
                .total_collector_checkpoints_received
                .with_label_values(&[TestHandler::NAME])
                .get(),
            6
        );
        // But the collector should filter out four checkpoints: (1, 2, 3, 4)
        assert_eq!(
            metrics
                .total_collector_skipped_checkpoints
                .with_label_values(&[TestHandler::NAME])
                .get(),
            4
        );
        // And that we only have values from two checkpoints (5, 6)
        assert_eq!(batch.batch_len, eager_rows_plus_one * 2);

        collector.shutdown().await.unwrap();
    }

    /// Because a checkpoint may be partially batched before the main reader lo advances past it,
    /// the collector must ensure that it fully writes out the checkpoint. Otherwise, this will
    /// essentially stall the commit_watermark task indefinitely as the latter waits for the
    /// remaining checkpoint parts.
    #[tokio::test(start_paused = true)]
    async fn test_collector_only_filters_whole_checkpoints() {
        let (processor_tx, processor_rx) = mpsc::channel(10);
        let (collector_tx, mut collector_rx) = mpsc::channel(10);
        let main_reader_lo = Arc::new(SetOnce::new_with(Some(AtomicU64::new(0))));

        let metrics = test_metrics();

        let collector = collector(
            Arc::new(TestHandler),
            CommitterConfig::default(),
            processor_rx,
            collector_tx,
            main_reader_lo.clone(),
            metrics.clone(),
        );

        let more_than_max_chunk_rows = TEST_MAX_CHUNK_ROWS + 10;

        let test_data =
            IndexedCheckpoint::new(0, 1, 10, 1000, vec![Entry; more_than_max_chunk_rows]);
        processor_tx.send(test_data).await.unwrap();
        tokio::time::advance(Duration::from_secs(1)).await;
        let batch = recv_with_timeout(&mut collector_rx, Duration::from_secs(2)).await;

        // There are still 10 rows left to be sent in the next batch.
        assert_eq!(batch.batch_len, TEST_MAX_CHUNK_ROWS);

        // Send indexed checkpoints 2 through 5 inclusive, but also bump the main reader lo to 4.
        let test_data: Vec<_> = (2..=5)
            .map(|cp| {
                IndexedCheckpoint::new(
                    0,
                    cp,
                    10,
                    1000,
                    vec![Entry; TestHandler::MIN_EAGER_ROWS + 1],
                )
            })
            .collect();
        for data in test_data {
            processor_tx.send(data).await.unwrap();
        }
        let atomic = main_reader_lo.get().unwrap();
        atomic.store(4, Ordering::Relaxed);
        tokio::time::advance(Duration::from_secs(10)).await;

        let batch = recv_with_timeout(&mut collector_rx, Duration::from_secs(2)).await;

        // The next batch should still be the remaining 10 rows from checkpoint 1.
        assert_eq!(batch.batch_len, 10);
        assert_eq!(batch.watermark[0].watermark.checkpoint_hi_inclusive, 1);

        recv_with_timeout(&mut collector_rx, Duration::from_secs(2)).await;

        assert_eq!(
            metrics
                .total_collector_skipped_checkpoints
                .with_label_values(&[TestHandler::NAME])
                .get(),
            2
        );
        assert_eq!(
            metrics
                .total_collector_checkpoints_received
                .with_label_values(&[TestHandler::NAME])
                .get(),
            5
        );

        collector.shutdown().await.unwrap();
    }
}
