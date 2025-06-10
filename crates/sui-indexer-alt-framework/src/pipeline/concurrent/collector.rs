// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{collections::BTreeMap, sync::Arc};

use tokio::{
    sync::mpsc,
    task::JoinHandle,
    time::{interval, MissedTickBehavior},
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
    metrics: Arc<IndexerMetrics>,
    cancel: CancellationToken,
) -> JoinHandle<()> {
    tokio::spawn(async move {
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

                Some(indexed) = rx.recv(), if pending_rows < H::MAX_PENDING_ROWS => {
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
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use sui_pg_db::{Connection, Db};
    use tokio::sync::mpsc;

    use crate::{
        metrics::tests::test_metrics,
        pipeline::{concurrent::max_chunk_rows, Processor},
        types::full_checkpoint_content::CheckpointData,
        FieldCount,
    };

    use super::*;

    #[derive(Clone)]
    struct Entry;

    impl FieldCount for Entry {
        // Fake a large number of fields to test max_chunk_rows.
        const FIELD_COUNT: usize = 32;
    }

    struct TestHandler;
    impl Processor for TestHandler {
        type Value = Entry;
        const NAME: &'static str = "test_handler";
        const FANOUT: usize = 1;

        fn process(&self, _checkpoint: &Arc<CheckpointData>) -> anyhow::Result<Vec<Self::Value>> {
            Ok(vec![])
        }
    }

    #[async_trait::async_trait]
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

    /// Wait for a timeout on the channel, expecting this operation to timeout.
    async fn expect_timeout(rx: &mut mpsc::Receiver<BatchedRows<TestHandler>>, duration: Duration) {
        match tokio::time::timeout(duration, rx.recv()).await {
            Err(_) => (), // Expected timeout - test passes
            Ok(_) => panic!("Expected timeout but received data instead"),
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

        let batch1 = collector_rx.recv().await.unwrap();
        assert_eq!(batch1.len(), max_chunk_rows);

        let batch2 = collector_rx.recv().await.unwrap();
        assert_eq!(batch2.len(), 1);

        let batch3 = collector_rx.recv().await.unwrap();
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
            test_metrics(),
            cancel.clone(),
        );

        processor_tx
            .send(IndexedCheckpoint::new(0, 1, 10, 1000, vec![Entry, Entry]))
            .await
            .unwrap();

        tokio::time::sleep(Duration::from_millis(200)).await;

        let batch = collector_rx.recv().await.unwrap();
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
            test_metrics(),
            cancel.clone(),
        );

        let start_time = std::time::Instant::now();

        // The collector starts with an immediate poll tick, creating an empty batch
        let initial_batch = collector_rx.recv().await.unwrap();
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
        let eager_batch = collector_rx.recv().await.unwrap();
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
            test_metrics(),
            cancel.clone(),
        );

        // The collector starts with an immediate poll tick, creating an empty batch
        let initial_batch = collector_rx.recv().await.unwrap();
        assert_eq!(initial_batch.len(), 0);
        // The collector will then just wait for the next poll as there is no new data yet.
        expect_timeout(&mut collector_rx, Duration::from_secs(1)).await;

        let start_time = std::time::Instant::now();

        // Send exactly MIN_EAGER_ROWS in one checkpoint
        let exact_threshold =
            IndexedCheckpoint::new(0, 1, 10, 1000, vec![Entry; TestHandler::MIN_EAGER_ROWS]);
        processor_tx.send(exact_threshold).await.unwrap();

        // Should trigger immediately since pending_rows >= MIN_EAGER_ROWS.
        let batch = collector_rx.recv().await.unwrap();
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
            test_metrics(),
            cancel.clone(),
        );

        // Consume initial empty batch
        let initial_batch = collector_rx.recv().await.unwrap();
        assert_eq!(initial_batch.len(), 0);

        // Send MIN_EAGER_ROWS - 1 entries (below threshold)
        let below_threshold =
            IndexedCheckpoint::new(0, 1, 10, 1000, vec![Entry; TestHandler::MIN_EAGER_ROWS - 1]);
        processor_tx.send(below_threshold).await.unwrap();

        // Try to receive with timeout - should timeout since we're below threshold
        expect_timeout(&mut collector_rx, Duration::from_secs(1)).await;

        // Should eventually get batch when timer triggers
        let timer_batch = collector_rx.recv().await.unwrap();
        assert_eq!(timer_batch.len(), TestHandler::MIN_EAGER_ROWS - 1);

        cancel.cancel();
    }
}
