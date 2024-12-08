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
    metrics::IndexerMetrics,
    pipeline::{CommitterConfig, Indexed, WatermarkPart},
};

use super::{Batched, Handler};

/// Processed values that are waiting to be written to the database. This is an internal type used
/// by the concurrent collector to hold data it is waiting to send to the committer.
struct Pending<H: Handler> {
    /// Values to be inserted into the database from this checkpoint
    values: Vec<H::Value>,
    /// The watermark associated with this checkpoint and the part of it that is left to commit
    watermark: WatermarkPart,
}

impl<H: Handler> Pending<H> {
    /// Whether there are values left to commit from this indexed checkpoint.
    fn is_empty(&self) -> bool {
        debug_assert!(self.watermark.batch_rows == 0);
        self.values.is_empty()
    }

    /// Adds data from this indexed checkpoint to the `batch`, honoring the handler's bounds on
    /// chunk size.
    fn batch_into(&mut self, batch: &mut Batched<H>) {
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

impl<H: Handler> From<Indexed<H>> for Pending<H> {
    fn from(indexed: Indexed<H>) -> Self {
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
    mut rx: mpsc::Receiver<Indexed<H>>,
    tx: mpsc::Sender<Batched<H>>,
    metrics: Arc<IndexerMetrics>,
    cancel: CancellationToken,
) -> JoinHandle<()> {
    tokio::spawn(async move {
        // The `poll` interval controls the maximum time to wait between collecting batches,
        // regardless of number of rows pending.
        let mut poll = interval(config.collect_interval());
        poll.set_missed_tick_behavior(MissedTickBehavior::Delay);

        // Data for checkpoints that have been received but not yet ready to be sent to committer due to lag constraint.
        let mut received: BTreeMap<u64, Indexed<H>> = BTreeMap::new();
        let checkpoint_lag = checkpoint_lag.unwrap_or_default();

        // Data for checkpoints that are ready to be sent but haven't been written yet.
        let mut pending: BTreeMap<u64, Pending<H>> = BTreeMap::new();
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

                    let mut batch = Batched::new();
                    while !batch.is_full() {
                        let Some(mut entry) = pending.first_entry() else {
                            break;
                        };

                        let indexed = entry.get_mut();
                        indexed.batch_into(&mut batch);
                        if indexed.is_empty() {
                            entry.remove();
                        }
                    }

                    pending_rows -= batch.len();
                    let elapsed = guard.stop_and_record();
                    debug!(
                        pipeline = H::NAME,
                        elapsed_ms = elapsed * 1000.0,
                        rows = batch.len(),
                        pending = pending_rows,
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
    received: &mut BTreeMap<u64, Indexed<H>>,
    pending: &mut BTreeMap<u64, Pending<H>>,
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
    use sui_types::full_checkpoint_content::CheckpointData;

    use crate::{db, pipeline::Processor};

    use super::*;

    #[derive(FieldCount)]
    struct Entry;

    struct TestHandler;
    impl Processor for TestHandler {
        type Value = Entry;
        const NAME: &'static str = "test";

        fn process(&self, _: &Arc<CheckpointData>) -> anyhow::Result<Vec<Self::Value>> {
            Ok(vec![])
        }
    }

    #[async_trait::async_trait]
    impl Handler for TestHandler {
        const MAX_PENDING_ROWS: usize = 1000;
        const MIN_EAGER_ROWS: usize = 100;

        async fn commit(_: &[Self::Value], _: &mut db::Connection<'_>) -> anyhow::Result<usize> {
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
            received.insert(i, Indexed::new(0, i, 0, 0, vec![Entry, Entry, Entry]));
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
        pending.insert(10, Pending::from(Indexed::new(0, 10, 0, 0, vec![Entry])));

        // Add checkpoints 1-5 to received
        for i in 1..=5 {
            received.insert(i, Indexed::new(0, i, 0, 0, vec![Entry]));
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
            received.insert(i, Indexed::new(0, i, 0, 0, vec![Entry]));
        }

        // With lag of 5 and tip at 10, no checkpoints can move
        let moved = move_ready_checkpoints::<TestHandler>(&mut received, &mut pending, 5);

        assert_eq!(moved, 0);
        assert_eq!(received.len(), 3);
        assert!(pending.is_empty());
    }
}
