// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeMap;
use std::sync::Arc;
use std::sync::atomic::AtomicU64;
use std::sync::atomic::Ordering;

use sui_futures::service::Service;
use tokio::sync::SetOnce;
use tokio::sync::mpsc;
use tracing::debug;
use tracing::info;

use crate::metrics::CheckpointLagMetricReporter;
use crate::metrics::IndexerMetrics;
use crate::pipeline::CommitterConfig;
use crate::pipeline::IndexedCheckpoint;
use crate::pipeline::WatermarkPart;
use crate::pipeline::batch_floor::BatchFloor;
use crate::pipeline::concurrent::BatchStatus;
use crate::pipeline::concurrent::BatchedRows;
use crate::pipeline::concurrent::Handler;

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
/// committer task to write to the database:
///
/// - Whenever rows are pending and the committer has room for another batch (its input channel
///   has a free slot), the collector cuts the pending rows into a batch and sends it. While the
///   committer is busy, rows keep accumulating, so batches grow under back-pressure. A batch also
///   ends when the handler reports it as ready, or when it holds `max_watermark_updates`
///   checkpoints; whatever is left goes into the next batch.
///
/// - If `config.min_batch_rows` is set, rows are only cut into a batch once that many are pending,
///   or once rows have been pending for `config.max_batch_delay()`.
///
/// - When the processor closes its channel, everything still pending is written out.
///
/// The `main_reader_lo` tracks the lowest checkpoint that can be committed by this pipeline.
///
/// This task will shutdown if any of its channels are closed.
pub(super) fn collector<H: Handler>(
    handler: Arc<H>,
    config: CommitterConfig,
    mut rx: mpsc::Receiver<IndexedCheckpoint<H>>,
    tx: mpsc::Sender<BatchedRows<H>>,
    main_reader_lo: Arc<SetOnce<AtomicU64>>,
    metrics: Arc<IndexerMetrics>,
    max_pending_rows: usize,
    max_watermark_updates: usize,
) -> Service {
    Service::new().spawn_aborting(async move {
        let checkpoint_lag_reporter = CheckpointLagMetricReporter::new_for_pipeline::<H>(
            &metrics.collected_checkpoint_timestamp_lag,
            &metrics.latest_collected_checkpoint_timestamp_lag_ms,
            &metrics.latest_collected_checkpoint,
        );

        // Data for checkpoints that are ready to be sent but haven't been written yet.
        let mut pending: BTreeMap<u64, PendingCheckpoint<H>> = BTreeMap::new();
        let mut pending_rows = 0;
        let mut floor = BatchFloor::new(&config);

        info!(pipeline = H::NAME, "Starting collector");

        // Wait for main_reader_lo to be initialized before processing any checkpoints.
        let reader_lo_atomic = main_reader_lo.wait().await;

        loop {
            let batch_due = floor.is_reached(pending_rows);

            tokio::select! {
                biased;

                permit = tx.reserve(), if batch_due => {
                    let Ok(permit) = permit else {
                        info!(
                            pipeline = H::NAME,
                            "Committer closed channel, stopping collector"
                        );
                        return Ok(());
                    };

                    let batched_rows = gather_batch(
                        &*handler,
                        &mut pending,
                        &mut pending_rows,
                        max_watermark_updates,
                        &checkpoint_lag_reporter,
                        &metrics,
                    );
                    permit.send(batched_rows);
                    floor.set_pending(!pending.is_empty());
                }

                // docs::#collector (see https://docs.sui.io/develop/accessing-data/custom-indexer/custom-indexers)
                received = rx.recv(), if pending_rows < max_pending_rows => {
                    // The processor has stopped, so write out everything that is left.
                    let Some(mut indexed) = received else {
                        break;
                    };

                    let reader_lo = reader_lo_atomic.load(Ordering::Relaxed);

                    metrics
                        .collector_reader_lo
                        .with_label_values(&[H::NAME])
                        .set(reader_lo as i64);

                    let mut recv_cps = 0usize;
                    let mut recv_rows = 0usize;
                    loop {
                        if indexed.checkpoint() < reader_lo {
                            indexed.values.clear();
                            metrics
                                .total_collector_skipped_checkpoints
                                .with_label_values(&[H::NAME])
                                .inc();
                        }

                        recv_cps += 1;
                        recv_rows += indexed.len();
                        pending_rows += indexed.len();
                        pending.insert(indexed.checkpoint(), indexed.into());

                        if pending_rows >= max_pending_rows {
                            break;
                        }

                        match rx.try_recv() {
                            Ok(next) => indexed = next,
                            Err(_) => break,
                        }
                    }

                    metrics
                        .total_collector_rows_received
                        .with_label_values(&[H::NAME])
                        .inc_by(recv_rows as u64);
                    metrics
                        .total_collector_checkpoints_received
                        .with_label_values(&[H::NAME])
                        .inc_by(recv_cps as u64);

                    floor.set_pending(true);
                }
                // docs::/#collector

                () = floor.delay_elapsed(), if !batch_due => {}
            }
        }

        while !pending.is_empty() {
            let batched_rows = gather_batch(
                &*handler,
                &mut pending,
                &mut pending_rows,
                max_watermark_updates,
                &checkpoint_lag_reporter,
                &metrics,
            );
            if tx.send(batched_rows).await.is_err() {
                info!(
                    pipeline = H::NAME,
                    "Committer closed channel, stopping collector"
                );
                return Ok(());
            }
        }

        info!(
            pipeline = H::NAME,
            "Processor closed channel, pending rows empty, stopping collector",
        );

        Ok(())
    })
}

/// Cut the next batch out of `pending`, taking checkpoints in order until the handler reports the
/// batch as ready or it holds `max_watermark_updates` watermarks.
fn gather_batch<H: Handler>(
    handler: &H,
    pending: &mut BTreeMap<u64, PendingCheckpoint<H>>,
    pending_rows: &mut usize,
    max_watermark_updates: usize,
    checkpoint_lag_reporter: &CheckpointLagMetricReporter,
    metrics: &IndexerMetrics,
) -> BatchedRows<H> {
    let guard = metrics
        .collector_gather_latency
        .with_label_values(&[H::NAME])
        .start_timer();

    let mut batch = H::Batch::default();
    let mut watermark = Vec::new();
    let mut batch_len = 0;

    while let Some(mut entry) = pending.first_entry() {
        if watermark.len() >= max_watermark_updates {
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
            break;
        }
    }

    let elapsed = guard.stop_and_record();
    debug!(
        pipeline = H::NAME,
        elapsed_ms = elapsed * 1000.0,
        rows = batch_len,
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

    *pending_rows -= batch_len;

    BatchedRows {
        batch,
        batch_len,
        watermark,
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use async_trait::async_trait;
    use tokio::sync::mpsc;

    use crate::metrics::tests::test_metrics;
    use crate::mocks::store::FallibleMockConnection;
    use crate::mocks::store::FallibleMockStore;
    use crate::pipeline::Processor;
    use crate::pipeline::concurrent::BatchStatus;
    use crate::types::full_checkpoint_content::Checkpoint;

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

        async fn process(&self, _checkpoint: &Arc<Checkpoint>) -> anyhow::Result<Vec<Self::Value>> {
            Ok(vec![])
        }
    }

    #[async_trait]
    impl Handler for TestHandler {
        type Store = FallibleMockStore;
        type Batch = Vec<Entry>;

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
            _conn: &mut FallibleMockConnection<'a>,
        ) -> anyhow::Result<usize> {
            tokio::time::sleep(Duration::from_millis(1000)).await;
            Ok(0)
        }
    }

    /// Start a collector whose committer channel holds `collector_channel_size` batches.
    fn spawn_collector(
        config: CommitterConfig,
        collector_channel_size: usize,
    ) -> (
        mpsc::Sender<IndexedCheckpoint<TestHandler>>,
        mpsc::Receiver<BatchedRows<TestHandler>>,
        Service,
    ) {
        let (processor_tx, processor_rx) = mpsc::channel(10);
        let (collector_tx, collector_rx) = mpsc::channel(collector_channel_size);
        let main_reader_lo = Arc::new(SetOnce::new_with(Some(AtomicU64::new(0))));

        let service = collector::<TestHandler>(
            Arc::new(TestHandler),
            config,
            processor_rx,
            collector_tx,
            main_reader_lo,
            test_metrics(),
            TestHandler::MAX_PENDING_ROWS,
            TestHandler::MAX_WATERMARK_UPDATES,
        );

        (processor_tx, collector_rx, service)
    }

    /// Configuration that holds rows until `min_batch_rows` are pending, or for at most
    /// `max_batch_delay_ms`.
    fn min_batch_config(min_batch_rows: usize, max_batch_delay_ms: u64) -> CommitterConfig {
        CommitterConfig {
            min_batch_rows,
            max_batch_delay_ms,
            ..CommitterConfig::default()
        }
    }

    /// Wait for a timeout on the channel, expecting this operation to timeout.
    async fn expect_timeout<H: Handler>(
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
    async fn recv_with_timeout<H: Handler>(
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
        let (processor_tx, mut collector_rx, _collector) =
            spawn_collector(CommitterConfig::default(), 10);

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
    }

    #[tokio::test]
    async fn test_collector_shutdown() {
        let (processor_tx, mut collector_rx, mut collector) =
            spawn_collector(CommitterConfig::default(), 10);

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
            TestHandler::MAX_PENDING_ROWS,
            TestHandler::MAX_WATERMARK_UPDATES,
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

    /// A checkpoint is cut into a batch as soon as the committer has room, and checkpoints that
    /// arrive while it has none are gathered into the next batch together.
    #[tokio::test(start_paused = true)]
    async fn test_collector_writes_as_soon_as_the_committer_has_room() {
        let (processor_tx, mut collector_rx, _collector) =
            spawn_collector(CommitterConfig::default(), 1);

        processor_tx
            .send(IndexedCheckpoint::new(0, 1, 10, 1000, vec![Entry]))
            .await
            .unwrap();
        tokio::time::sleep(Duration::from_millis(10)).await;

        // The first batch now fills the committer's channel, so these have to wait.
        for cp in 2..=4 {
            processor_tx
                .send(IndexedCheckpoint::new(0, cp, 10, 1000, vec![Entry]))
                .await
                .unwrap();
        }
        tokio::time::sleep(Duration::from_millis(10)).await;

        let batch = recv_with_timeout(&mut collector_rx, Duration::from_secs(1)).await;
        assert_eq!(batch.batch_len, 1);
        assert_eq!(batch.watermark.len(), 1);

        let batch = recv_with_timeout(&mut collector_rx, Duration::from_secs(1)).await;
        assert_eq!(batch.batch_len, 3);
        assert_eq!(batch.watermark.len(), 3);
    }

    /// Checkpoints without rows still reach the committer, so the watermark can advance past them.
    #[tokio::test(start_paused = true)]
    async fn test_collector_writes_checkpoints_without_rows() {
        let (processor_tx, mut collector_rx, _collector) =
            spawn_collector(CommitterConfig::default(), 10);

        processor_tx
            .send(IndexedCheckpoint::new(0, 1, 10, 1000, vec![]))
            .await
            .unwrap();

        let batch = recv_with_timeout(&mut collector_rx, Duration::from_secs(1)).await;
        assert_eq!(batch.batch_len, 0);
        assert_eq!(batch.watermark.len(), 1);
    }

    #[tokio::test(start_paused = true)]
    async fn test_collector_holds_rows_until_min_batch_rows() {
        let (processor_tx, mut collector_rx, _collector) =
            spawn_collector(min_batch_config(10, 60_000), 10);

        processor_tx
            .send(IndexedCheckpoint::new(0, 1, 10, 1000, vec![Entry; 9]))
            .await
            .unwrap();
        expect_timeout(&mut collector_rx, Duration::from_secs(1)).await;

        processor_tx
            .send(IndexedCheckpoint::new(0, 2, 20, 2000, vec![Entry]))
            .await
            .unwrap();

        let batch = recv_with_timeout(&mut collector_rx, Duration::from_millis(100)).await;
        assert_eq!(batch.batch_len, 10);
        assert_eq!(batch.watermark.len(), 2);
    }

    #[tokio::test(start_paused = true)]
    async fn test_collector_writes_rows_below_min_batch_rows_after_max_batch_delay() {
        let (processor_tx, mut collector_rx, _collector) =
            spawn_collector(min_batch_config(10, 3000), 10);

        processor_tx
            .send(IndexedCheckpoint::new(0, 1, 10, 1000, vec![Entry; 9]))
            .await
            .unwrap();
        expect_timeout(&mut collector_rx, Duration::from_secs(2)).await;

        let batch = recv_with_timeout(&mut collector_rx, Duration::from_secs(2)).await;
        assert_eq!(batch.batch_len, 9);
    }

    #[tokio::test(start_paused = true)]
    async fn test_collector_writes_rows_below_min_batch_rows_on_shutdown() {
        let (processor_tx, mut collector_rx, mut collector) =
            spawn_collector(min_batch_config(100, 60_000), 10);

        processor_tx
            .send(IndexedCheckpoint::new(0, 1, 10, 1000, vec![Entry; 5]))
            .await
            .unwrap();
        drop(processor_tx);

        let batch = recv_with_timeout(&mut collector_rx, Duration::from_secs(1)).await;
        assert_eq!(batch.batch_len, 5);

        tokio::time::timeout(Duration::from_secs(1), collector.join())
            .await
            .expect("collector shutdown timeout")
            .expect("collector shutdown failed");
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
            CommitterConfig::default(),
            processor_rx,
            collector_tx,
            main_reader_lo.clone(),
            test_metrics(),
            TestHandler::MAX_PENDING_ROWS,
            TestHandler::MAX_WATERMARK_UPDATES,
        );

        let test_data = IndexedCheckpoint::new(0, 1, 10, 1000, vec![Entry; 11]);
        processor_tx.send(test_data).await.unwrap();

        // Advance time significantly - collector should still be blocked waiting for
        // main_reader_lo.
        tokio::time::advance(Duration::from_secs(100)).await;

        assert!(collector_rx.try_recv().is_err());

        // Now initialize the main reader lo to 0, unblocking the collector.
        main_reader_lo.set(AtomicU64::new(0)).ok();

        tokio::time::advance(Duration::from_secs(1)).await;

        let batch = recv_with_timeout(&mut collector_rx, Duration::from_secs(2)).await;

        assert_eq!(batch.batch_len, 11);

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
            CommitterConfig::default(),
            processor_rx,
            collector_tx,
            main_reader_lo.clone(),
            metrics.clone(),
            TestHandler::MAX_PENDING_ROWS,
            TestHandler::MAX_WATERMARK_UPDATES,
        );

        let rows_per_checkpoint = 11;

        let test_data: Vec<_> = [1, 5, 2, 6, 4, 3]
            .into_iter()
            .map(|cp| IndexedCheckpoint::new(0, cp, 10, 1000, vec![Entry; rows_per_checkpoint]))
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
        assert_eq!(batch.batch_len, rows_per_checkpoint * 2);

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
            TestHandler::MAX_PENDING_ROWS,
            TestHandler::MAX_WATERMARK_UPDATES,
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
            .map(|cp| IndexedCheckpoint::new(0, cp, 10, 1000, vec![Entry; 11]))
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
