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
    checkpoint_lag: Option<u64>,
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

        // Data for checkpoints that have been received but not yet ready to be sent to committer due to lag constraint.
        let mut received: BTreeMap<u64, IndexedCheckpoint<H>> = BTreeMap::new();
        let checkpoint_lag = checkpoint_lag.unwrap_or_default();

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

                    received.insert(indexed.checkpoint(), indexed);
                    pending_rows += move_ready_checkpoints(&mut received, &mut pending, checkpoint_lag);

                    if pending_rows >= H::MIN_EAGER_ROWS {
                        poll.reset_immediately()
                    }
                }
            }
        }
    })
}

/// Move all checkpoints from `received` that are within the lag range into `pending`.
/// Returns the number of rows moved.
fn move_ready_checkpoints<H: Handler>(
    received: &mut BTreeMap<u64, IndexedCheckpoint<H>>,
    pending: &mut BTreeMap<u64, PendingCheckpoint<H>>,
    checkpoint_lag: u64,
) -> usize {
    let tip = match (received.last_key_value(), pending.last_key_value()) {
        (Some((cp, _)), None) | (None, Some((cp, _))) => *cp,
        (Some((cp1, _)), Some((cp2, _))) => std::cmp::max(*cp1, *cp2),
        (None, None) => return 0,
    };

    let mut moved_rows = 0;
    while let Some(entry) = received.first_entry() {
        let cp = *entry.key();
        if cp + checkpoint_lag > tip {
            break;
        }

        let indexed = entry.remove();
        moved_rows += indexed.len();
        pending.insert(cp, indexed.into());
    }

    moved_rows
}

#[cfg(test)]
mod tests {
    use sui_field_count::FieldCount;
    use sui_pg_db as db;
    use sui_types::full_checkpoint_content::CheckpointData;

    use crate::pipeline::{concurrent::max_chunk_rows, Processor};

    use super::*;

    #[derive(Clone)]
    struct Entry;

    impl FieldCount for Entry {
        // Fake a large number of fields to test max_chunk_rows.
        const FIELD_COUNT: usize = 32;
    }

    use prometheus::Registry;
    use std::time::Duration;
    use tokio::sync::mpsc;

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
        const MAX_PENDING_ROWS: usize = 10000;
        async fn commit(
            _values: &[Self::Value],
            _conn: &mut db::Connection<'_>,
        ) -> anyhow::Result<usize> {
            tokio::time::sleep(Duration::from_millis(1000)).await;
            Ok(0)
        }
    }

    #[test]
    fn test_move_ready_checkpoints_empty() {
        let mut received = BTreeMap::new();
        let mut pending = BTreeMap::new();
        let moved = move_ready_checkpoints::<TestHandler>(&mut received, &mut pending, 10);
        assert_eq!(moved, 0);
        assert!(received.is_empty());
        assert!(pending.is_empty());
    }

    #[test]
    fn test_move_ready_checkpoints_within_lag() {
        let mut received = BTreeMap::new();
        let mut pending = BTreeMap::new();

        // Add checkpoints 1-5 to received
        for i in 1..=5 {
            received.insert(
                i,
                IndexedCheckpoint::new(0, i, 0, 0, vec![Entry, Entry, Entry]),
            );
        }

        // With lag of 2 and tip at 5, only checkpoints 1-3 should move
        let moved = move_ready_checkpoints::<TestHandler>(&mut received, &mut pending, 2);

        assert_eq!(moved, 9); // 3 checkpoints * 3 rows each
        assert_eq!(received.len(), 2); // 4,5 remain
        assert_eq!(pending.len(), 3); // 1,2,3 moved
        assert!(pending.contains_key(&1));
        assert!(pending.contains_key(&2));
        assert!(pending.contains_key(&3));
    }

    #[test]
    fn test_move_ready_checkpoints_tip_from_pending() {
        let mut received = BTreeMap::new();
        let mut pending = BTreeMap::new();

        // Add checkpoint 10 to pending to establish tip
        pending.insert(
            10,
            PendingCheckpoint::from(IndexedCheckpoint::new(0, 10, 0, 0, vec![Entry])),
        );

        // Add checkpoints 1-5 to received
        for i in 1..=5 {
            received.insert(i, IndexedCheckpoint::new(0, i, 0, 0, vec![Entry]));
        }

        // With lag of 3 and tip at 10, checkpoints 1-7 can move
        let moved = move_ready_checkpoints::<TestHandler>(&mut received, &mut pending, 3);

        assert_eq!(moved, 5); // All 5 checkpoints moved, 1 row each
        assert!(received.is_empty());
        assert_eq!(pending.len(), 6); // Original + 5 new
    }

    #[test]
    fn test_move_ready_checkpoints_no_eligible() {
        let mut received = BTreeMap::new();
        let mut pending = BTreeMap::new();

        // Add checkpoints 8-10 to received
        for i in 8..=10 {
            received.insert(i, IndexedCheckpoint::new(0, i, 0, 0, vec![Entry]));
        }

        // With lag of 5 and tip at 10, no checkpoints can move
        let moved = move_ready_checkpoints::<TestHandler>(&mut received, &mut pending, 5);

        assert_eq!(moved, 0);
        assert_eq!(received.len(), 3);
        assert!(pending.is_empty());
    }

    #[tokio::test]
    async fn test_collector_batches_data() {
        let (processor_tx, processor_rx) = mpsc::channel(10);
        let (collector_tx, mut collector_rx) = mpsc::channel(10);
        let metrics = Arc::new(IndexerMetrics::new(&Registry::new()));
        let cancel = CancellationToken::new();

        let _collector = collector::<TestHandler>(
            CommitterConfig::default(),
            None,
            processor_rx,
            collector_tx,
            metrics,
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
        let metrics = Arc::new(IndexerMetrics::new(&Registry::new()));
        let cancel = CancellationToken::new();

        let collector = collector::<TestHandler>(
            CommitterConfig::default(),
            None,
            processor_rx,
            collector_tx,
            metrics,
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

        let metrics = Arc::new(IndexerMetrics::new(&Registry::new()));

        let cancel = CancellationToken::new();

        let _collector = collector::<TestHandler>(
            CommitterConfig::default(),
            None,
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
}
