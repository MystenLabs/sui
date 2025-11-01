// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{collections::BTreeMap, sync::Arc};

use tokio::{
    sync::{mpsc, watch},
    task::JoinHandle,
    time::{MissedTickBehavior, interval},
};
use tokio_util::sync::CancellationToken;
use tracing::{debug, info};

use crate::{
    metrics::{CheckpointLagMetricReporter, IndexerMetrics},
    pipeline::{CommitterConfig, IndexedCheckpoint, WatermarkPart},
};

use super::{BatchedRows, Handler};

/// Processed values that are waiting to be written to the database. This is an internal type used
/// by the concurrent collector to hold data it is waiting to send to the committer.
struct PendingCheckpoint<H: Handler> {
    /// Values to be inserted into the database from this checkpoint
    values: Vec<H::Value>,
    /// The watermark associated with this checkpoint and the part of it that is left to commit
    watermark: WatermarkPart,
}

/// Helper enum for filtering checkpoints based on the main pipeline's reader watermark. The
/// `Disabled` variant is a regular pipeline that will batch all checkpoint data without filtering.
/// The `Enabled` variant tracks the main pipeline reader watermark and enables dropping checkpoints
/// below that value.
enum MainReaderFilter {
    Enabled(watch::Receiver<Option<u64>>),
    Disabled,
}

impl MainReaderFilter {
    /// Wait until the watch channel is initialized, returning `None` if there is a watch channel,
    /// but the sender is closed.
    async fn init(rx_opt: Option<watch::Receiver<Option<u64>>>) -> Option<Self> {
        match rx_opt {
            None => return Some(Self::Disabled),
            Some(mut rx) => {
                if rx.wait_for(|v| v.is_some()).await.is_err() {
                    return None;
                }
                Some(MainReaderFilter::Enabled(rx))
            }
        }
    }

    /// If the given checkpoint is less than the main reader lo, return true to indicate it should
    /// be skipped.
    fn should_skip(&self, checkpoint: u64) -> bool {
        match self {
            MainReaderFilter::Disabled => false,
            MainReaderFilter::Enabled(rx) => {
                // SAFETY: We ensured during initialization that this value is `Some`.
                checkpoint
                    < rx.borrow()
                        .expect("main_reader_lo should not revert to None after initialization")
            }
        }
    }

    /// Wait for the main reader watermark to change. The `Disabled` variant never resolves. The
    /// `Enabled` variant blocks until the channel receives a new value, and then updates its cached
    /// value.
    async fn wait_for_change(&mut self) -> Result<(), ()> {
        match self {
            MainReaderFilter::Disabled => std::future::pending().await,
            MainReaderFilter::Enabled(rx) => {
                rx.changed().await.map_err(|_| ())?;
                Ok(())
            }
        }
    }
}

impl<H: Handler> PendingCheckpoint<H> {
    /// Whether there are values left to commit from this indexed checkpoint.
    fn is_empty(&self) -> bool {
        let empty = self.values.is_empty();
        debug_assert!(!empty || self.watermark.batch_rows == 0);
        empty
    }

    /// Adds data from this indexed checkpoint to the `batch`, honoring the handler's bounds on
    /// chunk size.
    fn batch_into(&mut self, batch: &mut BatchedRows<H>) {
        let max_chunk_rows = super::max_chunk_rows::<H>();
        if batch.values.len() + self.values.len() > max_chunk_rows {
            let mut for_batch = self.values.split_off(max_chunk_rows - batch.values.len());

            std::mem::swap(&mut self.values, &mut for_batch);
            batch.watermark.push(self.watermark.take(for_batch.len()));
            batch.values.extend(for_batch);
        } else {
            batch.watermark.push(self.watermark.take(self.values.len()));
            batch.values.extend(std::mem::take(&mut self.values));
        }
    }
}

impl<H: Handler> From<IndexedCheckpoint<H>> for PendingCheckpoint<H> {
    fn from(indexed: IndexedCheckpoint<H>) -> Self {
        Self {
            watermark: WatermarkPart {
                watermark: indexed.watermark,
                batch_rows: indexed.values.len(),
                total_rows: indexed.values.len(),
            },
            values: indexed.values,
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
/// This task will shutdown if canceled via the `cancel` token, or if any of its channels are
/// closed.
pub(super) fn collector<H: Handler + 'static>(
    config: CommitterConfig,
    mut rx: mpsc::Receiver<IndexedCheckpoint<H>>,
    tx: mpsc::Sender<BatchedRows<H>>,
    main_reader_lo_rx: Option<watch::Receiver<Option<u64>>>,
    metrics: Arc<IndexerMetrics>,
    cancel: CancellationToken,
) -> JoinHandle<()> {
    tokio::spawn(async move {
        // If the channel exists, the collector needs to block until the main reader lo value is
        // initialized.
        let mut main_reader_filter = match MainReaderFilter::init(main_reader_lo_rx).await {
            Some(filter) => filter,
            None => {
                info!(
                    pipeline = H::NAME,
                    "Shutdown received before main reader lo initialized"
                );
                return;
            }
        };

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
                _ = cancel.cancelled() => {
                    info!(pipeline = H::NAME, "Shutdown received, stopping collector");
                    break;
                }

                // Check that the main_reader_lo channel is still open.
                changed_result = main_reader_filter.wait_for_change() => {
                    if changed_result.is_err() {
                        info!(
                            pipeline = H::NAME,
                            "Shutting down collector as main reader lo watch closed",
                        );
                        break;
                    }
                }

                // Time to create another batch and push it to the committer.
                _ = poll.tick() => {
                    let guard = metrics
                        .collector_gather_latency
                        .with_label_values(&[H::NAME])
                        .start_timer();

                    let mut batch = BatchedRows::new();
                    while !batch.is_full() {
                        let Some(mut entry) = pending.first_entry() else {
                            break;
                        };

                        let indexed = entry.get_mut();
                        indexed.batch_into(&mut batch);
                        if indexed.is_empty() {
                            checkpoint_lag_reporter.report_lag(
                                indexed.watermark.checkpoint(),
                                indexed.watermark.timestamp_ms(),
                            );
                            entry.remove();
                        }
                    }

                    pending_rows -= batch.len();
                    let elapsed = guard.stop_and_record();
                    debug!(
                        pipeline = H::NAME,
                        elapsed_ms = elapsed * 1000.0,
                        rows = batch.len(),
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
                        .observe(batch.len() as f64);

                    if tx.send(batch).await.is_err() {
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
                    if main_reader_filter.should_skip(indexed.checkpoint()) {
                        indexed.values.clear();
                        metrics.collector_skipped_checkpoints
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
    })
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use async_trait::async_trait;
    use sui_pg_db::{Connection, Db};
    use tokio::sync::mpsc;

    use crate::{
        FieldCount,
        metrics::tests::test_metrics,
        mocks::store::{MockConnection, MockStore},
        pipeline::{Processor, concurrent::max_chunk_rows},
        types::full_checkpoint_content::CheckpointData,
    };

    use super::*;

    #[derive(Clone)]
    struct Entry;

    #[derive(Clone, FieldCount)]
    struct SequenceNumber(u64);

    struct TestHandler;

    struct MainReaderLoTestHandler;

    impl FieldCount for Entry {
        // Fake a large number of fields to test max_chunk_rows.
        const FIELD_COUNT: usize = 32;
    }

    #[async_trait]
    impl Processor for TestHandler {
        type Value = Entry;
        const NAME: &'static str = "test_handler";
        const FANOUT: usize = 1;

        async fn process(
            &self,
            _checkpoint: &Arc<CheckpointData>,
        ) -> anyhow::Result<Vec<Self::Value>> {
            Ok(vec![])
        }
    }

    #[async_trait]
    impl Handler for TestHandler {
        type Store = Db;

        const MIN_EAGER_ROWS: usize = 10;
        const MAX_PENDING_ROWS: usize = 10000;
        async fn commit<'a>(
            _values: &[Self::Value],
            _conn: &mut Connection<'a>,
        ) -> anyhow::Result<usize> {
            tokio::time::sleep(Duration::from_millis(1000)).await;
            Ok(0)
        }
    }

    #[async_trait]
    impl Processor for MainReaderLoTestHandler {
        type Value = SequenceNumber;
        const NAME: &'static str = "main_reader_lo_test_handler";
        const FANOUT: usize = 1;

        async fn process(
            &self,
            checkpoint: &Arc<CheckpointData>,
        ) -> anyhow::Result<Vec<Self::Value>> {
            Ok(vec![SequenceNumber(
                checkpoint.checkpoint_summary.sequence_number,
            )])
        }
    }

    #[async_trait]
    impl Handler for MainReaderLoTestHandler {
        type Store = MockStore;

        const MIN_EAGER_ROWS: usize = 10;
        const MAX_PENDING_ROWS: usize = 10000;
        async fn commit<'a>(
            values: &[Self::Value],
            conn: &mut MockConnection<'a>,
        ) -> anyhow::Result<usize> {
            for value in values {
                conn.0
                    .commit_data(Self::NAME, value.0, vec![value.0])
                    .await?;
            }
            Ok(values.len())
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
        let cancel = CancellationToken::new();

        let _collector = collector::<TestHandler>(
            CommitterConfig::default(),
            processor_rx,
            collector_tx,
            None,
            test_metrics(),
            cancel.clone(),
        );

        let max_chunk_rows = max_chunk_rows::<TestHandler>();
        let part1_length = max_chunk_rows / 2;
        let part2_length = max_chunk_rows - part1_length - 1;

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
        assert_eq!(batch1.len(), max_chunk_rows);

        let batch2 = recv_with_timeout(&mut collector_rx, Duration::from_secs(1)).await;
        assert_eq!(batch2.len(), 1);

        let batch3 = recv_with_timeout(&mut collector_rx, Duration::from_secs(1)).await;
        assert_eq!(batch3.len(), 0);

        cancel.cancel();
    }

    #[tokio::test]
    async fn test_collector_shutdown() {
        let (processor_tx, processor_rx) = mpsc::channel(10);
        let (collector_tx, mut collector_rx) = mpsc::channel(10);
        let cancel = CancellationToken::new();

        let collector = collector::<TestHandler>(
            CommitterConfig::default(),
            processor_rx,
            collector_tx,
            None,
            test_metrics(),
            cancel.clone(),
        );

        processor_tx
            .send(IndexedCheckpoint::new(0, 1, 10, 1000, vec![Entry, Entry]))
            .await
            .unwrap();

        tokio::time::sleep(Duration::from_millis(200)).await;

        let batch = recv_with_timeout(&mut collector_rx, Duration::from_secs(1)).await;
        assert_eq!(batch.len(), 2);

        // Drop processor sender to simulate shutdown
        drop(processor_tx);

        // After a short delay, collector should shut down
        let _ = tokio::time::timeout(Duration::from_millis(500), collector)
            .await
            .expect("collector did not shutdown");

        cancel.cancel();
    }

    #[tokio::test]
    async fn test_collector_respects_max_pending() {
        let processor_channel_size = 5; // unit is checkpoint
        let collector_channel_size = 2; // unit is batch, aka rows / MAX_CHUNK_ROWS
        let (processor_tx, processor_rx) = mpsc::channel(processor_channel_size);
        let (collector_tx, _collector_rx) = mpsc::channel(collector_channel_size);

        let metrics = test_metrics();
        let cancel = CancellationToken::new();

        let _collector = collector::<TestHandler>(
            CommitterConfig::default(),
            processor_rx,
            collector_tx,
            None,
            metrics.clone(),
            cancel.clone(),
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
                    + max_chunk_rows::<TestHandler>() * collector_channel_size
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

        cancel.cancel();
    }

    #[tokio::test]
    async fn test_collector_accumulates_across_checkpoints_until_eager_threshold() {
        let (processor_tx, processor_rx) = mpsc::channel(10);
        let (collector_tx, mut collector_rx) = mpsc::channel(10);
        let cancel = CancellationToken::new();

        // Set a very long collect interval (60 seconds) to ensure timing doesn't trigger batching
        let config = CommitterConfig {
            collect_interval_ms: 60_000,
            ..CommitterConfig::default()
        };
        let _collector = collector::<TestHandler>(
            config,
            processor_rx,
            collector_tx,
            None,
            test_metrics(),
            cancel.clone(),
        );

        let start_time = std::time::Instant::now();

        // The collector starts with an immediate poll tick, creating an empty batch
        let initial_batch = recv_with_timeout(&mut collector_rx, Duration::from_secs(1)).await;
        assert_eq!(initial_batch.len(), 0);

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
        assert_eq!(eager_batch.len(), TestHandler::MIN_EAGER_ROWS);

        // Verify batch was created quickly (much less than 60 seconds)
        let elapsed = start_time.elapsed();
        assert!(elapsed < Duration::from_secs(10));

        cancel.cancel();
    }

    #[tokio::test]
    async fn test_immediate_batch_on_min_eager_rows() {
        let (processor_tx, processor_rx) = mpsc::channel(10);
        let (collector_tx, mut collector_rx) = mpsc::channel(10);
        let cancel = CancellationToken::new();

        // Set a very long collect interval (60 seconds) to ensure timing doesn't trigger batching
        let config = CommitterConfig {
            collect_interval_ms: 60_000,
            ..CommitterConfig::default()
        };
        let _collector = collector::<TestHandler>(
            config,
            processor_rx,
            collector_tx,
            None,
            test_metrics(),
            cancel.clone(),
        );

        // The collector starts with an immediate poll tick, creating an empty batch
        let initial_batch = recv_with_timeout(&mut collector_rx, Duration::from_secs(1)).await;
        assert_eq!(initial_batch.len(), 0);
        // The collector will then just wait for the next poll as there is no new data yet.
        expect_timeout(&mut collector_rx, Duration::from_secs(1)).await;

        let start_time = std::time::Instant::now();

        // Send exactly MIN_EAGER_ROWS in one checkpoint
        let exact_threshold =
            IndexedCheckpoint::new(0, 1, 10, 1000, vec![Entry; TestHandler::MIN_EAGER_ROWS]);
        processor_tx.send(exact_threshold).await.unwrap();

        // Should trigger immediately since pending_rows >= MIN_EAGER_ROWS.
        let batch = recv_with_timeout(&mut collector_rx, Duration::from_secs(1)).await;
        assert_eq!(batch.len(), TestHandler::MIN_EAGER_ROWS);

        // Verify batch was created quickly (much less than 60 seconds)
        let elapsed = start_time.elapsed();
        assert!(elapsed < Duration::from_secs(10));

        cancel.cancel();
    }

    #[tokio::test]
    async fn test_collector_waits_for_timer_when_below_eager_threshold() {
        let (processor_tx, processor_rx) = mpsc::channel(10);
        let (collector_tx, mut collector_rx) = mpsc::channel(10);
        let cancel = CancellationToken::new();

        // Set a reasonable collect interval for this test (3 seconds).
        let config = CommitterConfig {
            collect_interval_ms: 3000,
            ..CommitterConfig::default()
        };
        let _collector = collector::<TestHandler>(
            config,
            processor_rx,
            collector_tx,
            None,
            test_metrics(),
            cancel.clone(),
        );

        // Consume initial empty batch
        let initial_batch = recv_with_timeout(&mut collector_rx, Duration::from_secs(1)).await;
        assert_eq!(initial_batch.len(), 0);

        // Send MIN_EAGER_ROWS - 1 entries (below threshold)
        let below_threshold =
            IndexedCheckpoint::new(0, 1, 10, 1000, vec![Entry; TestHandler::MIN_EAGER_ROWS - 1]);
        processor_tx.send(below_threshold).await.unwrap();

        // Try to receive with timeout - should timeout since we're below threshold
        expect_timeout(&mut collector_rx, Duration::from_secs(1)).await;

        // Should eventually get batch when timer triggers
        let timer_batch = recv_with_timeout(&mut collector_rx, Duration::from_secs(4)).await;
        assert_eq!(timer_batch.len(), TestHandler::MIN_EAGER_ROWS - 1);

        cancel.cancel();
    }

    /// If the `main_reader_lo_rx` channel is Some, the collector must wait for it to initialize the
    /// `main_reader_lo` value before entering the main loop.
    #[tokio::test(start_paused = true)]
    async fn test_collector_waits_for_main_reader_lo_initialization() {
        let (processor_tx, processor_rx) = mpsc::channel(10);
        let (collector_tx, mut collector_rx) = mpsc::channel(10);
        let cancel = CancellationToken::new();
        let (main_reader_lo_tx, main_reader_lo_rx) = watch::channel(None);

        let _collector = collector::<MainReaderLoTestHandler>(
            CommitterConfig {
                // Collect interval logger than time to advance to ensure timing doesn't trigger
                // batching.
                collect_interval_ms: 200_000,
                ..CommitterConfig::default()
            },
            processor_rx,
            collector_tx,
            Some(main_reader_lo_rx),
            test_metrics(),
            cancel.clone(),
        );

        // Send enough data to trigger batching.
        let test_data = IndexedCheckpoint::new(
            0,
            1,
            10,
            1000,
            vec![SequenceNumber(1); MainReaderLoTestHandler::MIN_EAGER_ROWS + 1],
        );
        processor_tx.send(test_data).await.unwrap();

        // Advance time significantly - collector should still be blocked waiting for
        // main_reader_lo.
        tokio::time::advance(Duration::from_secs(100)).await;

        assert!(collector_rx.try_recv().is_err());

        // Now initialize the main reader lo to 0, unblocking the collector.
        main_reader_lo_tx.send(Some(0)).unwrap();

        tokio::time::advance(Duration::from_secs(1)).await;

        let batch = recv_with_timeout(&mut collector_rx, Duration::from_secs(2)).await;

        assert_eq!(batch.len(), MainReaderLoTestHandler::MIN_EAGER_ROWS + 1);

        cancel.cancel();
    }

    /// During initialization, if the `main_reader_lo_rx` channel closes before sending a value, the
    /// collector should shut down.
    #[tokio::test]
    async fn test_collector_shuts_down_when_main_reader_lo_channel_closes_on_initialization() {
        let (processor_tx, processor_rx) = mpsc::channel(10);
        let (collector_tx, mut collector_rx) = mpsc::channel(10);
        let cancel = CancellationToken::new();
        let (main_reader_lo_tx, main_reader_lo_rx) = watch::channel(None);

        let collector = collector::<MainReaderLoTestHandler>(
            CommitterConfig {
                // Collect interval logger than time to advance to ensure timing doesn't trigger
                // batching.
                collect_interval_ms: 200_000,
                ..CommitterConfig::default()
            },
            processor_rx,
            collector_tx,
            Some(main_reader_lo_rx),
            test_metrics(),
            cancel.clone(),
        );

        // Send enough data to trigger batching.
        let test_data = IndexedCheckpoint::new(
            0,
            1,
            10,
            1000,
            vec![SequenceNumber(1); MainReaderLoTestHandler::MIN_EAGER_ROWS + 1],
        );
        processor_tx.send(test_data).await.unwrap();

        assert!(collector_rx.try_recv().is_err());

        // Close the sender channel.
        drop(main_reader_lo_tx);

        // Collector should shut down shortly after.
        let result = collector.await;
        assert!(result.is_ok());

        // After shutdown, we still should not have received any batch.
        assert!(collector_rx.try_recv().is_err());
    }

    // During a run, if the `main_reader_lo_rx` channel closes, the collector should shut down.
    #[tokio::test(start_paused = true)]
    async fn test_collector_shuts_down_when_main_reader_lo_channel_closes_in_main_loop() {
        let (processor_tx, processor_rx) = mpsc::channel(10);
        let (collector_tx, mut collector_rx) = mpsc::channel(10);
        let cancel = CancellationToken::new();
        let (main_reader_lo_tx, main_reader_lo_rx) = watch::channel(None);

        let collector = collector::<MainReaderLoTestHandler>(
            CommitterConfig {
                // Collect interval logger than time to advance to ensure timing doesn't trigger
                // batching.
                collect_interval_ms: 200_000,
                ..CommitterConfig::default()
            },
            processor_rx,
            collector_tx,
            Some(main_reader_lo_rx),
            test_metrics(),
            cancel.clone(),
        );

        // Send enough data to trigger batching, and validate that we are able to get the first batch.
        let test_data = IndexedCheckpoint::new(
            0,
            1,
            10,
            1000,
            vec![SequenceNumber(1); MainReaderLoTestHandler::MIN_EAGER_ROWS + 1],
        );
        processor_tx.send(test_data).await.unwrap();
        main_reader_lo_tx.send(Some(0)).unwrap();
        tokio::time::advance(Duration::from_secs(1)).await;
        let batch = recv_with_timeout(&mut collector_rx, Duration::from_secs(2)).await;

        assert_eq!(batch.len(), MainReaderLoTestHandler::MIN_EAGER_ROWS + 1);

        // On the next batch, test that we eventually shut down when the channel closes.
        let test_data = IndexedCheckpoint::new(
            0,
            2,
            20,
            2000,
            vec![SequenceNumber(2); MainReaderLoTestHandler::MIN_EAGER_ROWS + 1],
        );
        processor_tx.send(test_data).await.unwrap();
        main_reader_lo_tx.send(Some(1)).unwrap();
        tokio::time::advance(Duration::from_secs(1)).await;
        drop(main_reader_lo_tx);

        // Collector should shut down shortly after.
        let result = collector.await;

        assert!(result.is_ok());
    }

    /// When receiving checkpoints, if they are below the main reader lo, they should be dropped
    /// immediately.
    #[tokio::test]
    async fn test_collector_drops_checkpoints_immediately_if_le_main_reader_lo() {
        let (processor_tx, processor_rx) = mpsc::channel(10);
        let (collector_tx, mut collector_rx) = mpsc::channel(10);
        let cancel = CancellationToken::new();
        let (_main_reader_lo_tx, main_reader_lo_rx) = watch::channel(Some(5u64));
        let metrics = test_metrics();

        let _collector = collector::<MainReaderLoTestHandler>(
            CommitterConfig {
                // Collect interval logger than time to advance to ensure timing doesn't trigger
                // batching.
                collect_interval_ms: 200_000,
                ..CommitterConfig::default()
            },
            processor_rx,
            collector_tx,
            Some(main_reader_lo_rx),
            metrics.clone(),
            cancel.clone(),
        );

        let eager_rows_plus_one = MainReaderLoTestHandler::MIN_EAGER_ROWS + 1;

        let test_data: Vec<_> = [1, 5, 2, 6, 4]
            .into_iter()
            .map(|cp| {
                IndexedCheckpoint::new(
                    0,
                    cp,
                    10,
                    1000,
                    vec![SequenceNumber(cp); eager_rows_plus_one],
                )
            })
            .collect();
        for data in test_data {
            processor_tx.send(data).await.unwrap();
        }
        let batch = recv_with_timeout(&mut collector_rx, Duration::from_secs(2)).await;

        // Make sure that we are advancing watermarks.
        assert_eq!(batch.watermark.len(), 5);
        // And reporting the checkpoints as received.
        assert_eq!(
            metrics
                .total_collector_checkpoints_received
                .with_label_values(&[MainReaderLoTestHandler::NAME])
                .get(),
            5
        );
        // But the collector should filter out three checkpoints: (1, 2, 4)
        assert_eq!(
            metrics
                .collector_skipped_checkpoints
                .with_label_values(&[MainReaderLoTestHandler::NAME])
                .get(),
            3
        );
        // And that we only have values from two checkpoints (5, 6)
        assert_eq!(batch.len(), eager_rows_plus_one * 2);

        cancel.cancel();
    }

    /// Because a checkpoint may be partially batched before the main reader lo advances past it,
    /// the collector must ensure that it fully writes out the checkpoint. Otherwise, this will
    /// essentially stall the commit_watermark task indefinitely as the latter waits for the
    /// remaining checkpoint parts.
    #[tokio::test(start_paused = true)]
    async fn test_collector_does_not_drop_partially_batched_checkpoints_when_eventually_le_main_reader_lo()
     {
        let (processor_tx, processor_rx) = mpsc::channel(10);
        let (collector_tx, mut collector_rx) = mpsc::channel(10);
        let cancel = CancellationToken::new();
        let (main_reader_lo_tx, main_reader_lo_rx) = watch::channel(Some(0u64));
        let metrics = test_metrics();

        let _collector = collector::<MainReaderLoTestHandler>(
            CommitterConfig::default(),
            processor_rx,
            collector_tx,
            Some(main_reader_lo_rx),
            metrics.clone(),
            cancel.clone(),
        );

        let more_than_max_chunk_rows = max_chunk_rows::<MainReaderLoTestHandler>() + 10;

        let test_data = IndexedCheckpoint::new(
            0,
            1,
            10,
            1000,
            vec![SequenceNumber(1); more_than_max_chunk_rows],
        );
        processor_tx.send(test_data).await.unwrap();
        tokio::time::advance(Duration::from_secs(1)).await;
        let batch = recv_with_timeout(&mut collector_rx, Duration::from_secs(2)).await;

        // There are still 10 rows left to be sent in the next batch.
        assert_eq!(batch.len(), max_chunk_rows::<MainReaderLoTestHandler>());

        // Send indexed checkpoints 2 through 5 inclusive, but also bump the main reader lo to 4.
        let test_data: Vec<_> = (2..=5)
            .map(|cp| {
                IndexedCheckpoint::new(
                    0,
                    cp,
                    10,
                    1000,
                    vec![SequenceNumber(cp); MainReaderLoTestHandler::MIN_EAGER_ROWS + 1],
                )
            })
            .collect();
        for data in test_data {
            processor_tx.send(data).await.unwrap();
        }
        main_reader_lo_tx.send(Some(4)).unwrap();
        tokio::time::advance(Duration::from_secs(10)).await;

        let batch = recv_with_timeout(&mut collector_rx, Duration::from_secs(2)).await;

        // The next batch should still be the remaining 10 rows from checkpoint 1.
        assert_eq!(batch.len(), 10);
        assert_eq!(batch.watermark[0].watermark.checkpoint_hi_inclusive, 1);

        recv_with_timeout(&mut collector_rx, Duration::from_secs(2)).await;

        assert_eq!(
            metrics
                .collector_skipped_checkpoints
                .with_label_values(&[MainReaderLoTestHandler::NAME])
                .get(),
            2
        );
        assert_eq!(
            metrics
                .total_collector_checkpoints_received
                .with_label_values(&[MainReaderLoTestHandler::NAME])
                .get(),
            5
        );

        cancel.cancel();
    }
}
