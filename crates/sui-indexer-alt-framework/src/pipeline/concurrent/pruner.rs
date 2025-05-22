// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{collections::BTreeMap, sync::Arc};

use futures::stream::FuturesUnordered;
use futures::StreamExt;
use tokio::{
    sync::Semaphore,
    task::JoinHandle,
    time::{interval, MissedTickBehavior},
};
use tokio_util::sync::CancellationToken;
use tracing::{debug, error, info, warn};

use crate::{
    metrics::IndexerMetrics,
    pipeline::logging::{LoggerWatermark, WatermarkLogger},
    store::{Connection, Store},
};

use super::{Handler, PrunerConfig};

#[derive(Default)]
struct PendingRanges {
    /// Maps from `from` to `to_exclusive` for all the ranges that are ready to be pruned.
    ranges: BTreeMap<u64, u64>,
    /// The last range that has been scheduled for pruning.
    last_scheduled_range: Option<(u64, u64)>,
}

impl PendingRanges {
    /// Schedule a new range to be pruned.
    /// Using the last scheduled range to avoid double pruning of the same range.
    /// This is important because double pruning will not always work since pruning
    /// may not be idempotent for some pipelines.
    /// For instance, if handler holds processed data needed for pruning,
    /// the pruning step may remove those data once done.
    fn schedule(&mut self, mut from: u64, to_exclusive: u64) {
        let last_scheduled_range = self.last_scheduled_range.unwrap_or((0, 0));
        // If the end of the last scheduled range is greater than the end of the new range,
        // it means the entire new range was already scheduled before.
        if to_exclusive <= last_scheduled_range.1 {
            return;
        }
        // Otherwise, we make sure the new range starts after the last scheduled range.
        from = from.max(last_scheduled_range.1);
        self.ranges.insert(from, to_exclusive);
        self.last_scheduled_range = Some((from, to_exclusive));
    }

    fn len(&self) -> usize {
        self.ranges.len()
    }

    fn iter(&self) -> impl Iterator<Item = (u64, u64)> + '_ {
        self.ranges
            .iter()
            .map(|(from, to_exclusive)| (*from, *to_exclusive))
    }

    /// Remove the range from the pending_prune_ranges.
    fn remove(&mut self, from: &u64) {
        self.ranges.remove(from).unwrap();
    }

    /// Returns the current pruner_hi watermark, i.e. the first checkpoint that has not yet been pruned.
    /// This will be the first key in the pending_prune_ranges map.
    /// If the map is empty, then it is the last checkpoint that has been scheduled for pruning.
    fn get_pruner_hi(&self) -> u64 {
        self.ranges.keys().next().cloned().unwrap_or(
            self.last_scheduled_range
                .map(|(_, t)| t)
                // get_pruner_hi will generally not be called until we have scheduled something.
                // But return 0 just in case we called it earlier.
                .unwrap_or_default(),
        )
    }
}

/// The pruner task is responsible for deleting old data from the database. It will periodically
/// check the `watermarks` table to see if there is any data that should be pruned between the
/// `pruner_hi` (inclusive), and `reader_lo` (exclusive) checkpoints. This task will also provide a
/// mapping of the pruned checkpoints to their corresponding epoch and tx, which the handler can
/// then use to delete the corresponding data from the database.
///
/// To ensure that the pruner does not interfere with reads that are still in flight, it respects
/// the watermark's `pruner_timestamp`, which records the time that `reader_lo` was last updated.
/// The task will not prune data until at least `config.delay()` has passed since `pruner_timestamp`
/// to give in-flight reads time to land.
///
/// The task regularly traces its progress, outputting at a higher log level every
/// [LOUD_WATERMARK_UPDATE_INTERVAL]-many checkpoints.
///
/// The task will shutdown if the `cancel` token is signalled. If the `config` is `None`, the task
/// will shutdown immediately.
pub(super) fn pruner<H: Handler + Send + Sync + 'static>(
    handler: Arc<H>,
    config: Option<PrunerConfig>,
    store: H::Store,
    metrics: Arc<IndexerMetrics>,
    cancel: CancellationToken,
) -> JoinHandle<()> {
    tokio::spawn(async move {
        let Some(config) = config else {
            info!(pipeline = H::NAME, "Skipping pruner task");
            return;
        };

        info!(
            pipeline = H::NAME,
            "Starting pruner with config: {:?}", config
        );

        // The pruner can pause for a while, waiting for the delay imposed by the
        // `pruner_timestamp` to expire. In that case, the period between ticks should not be
        // compressed to make up for missed ticks.
        let mut poll = interval(config.interval());
        poll.set_missed_tick_behavior(MissedTickBehavior::Delay);

        // The pruner task will periodically output a log message at a higher log level to
        // demonstrate that it is making progress.
        let mut logger = WatermarkLogger::new("pruner", LoggerWatermark::default());

        // Maintains the list of chunks that are ready to be pruned but not yet pruned.
        // This map can contain ranges that were attempted to be pruned in previous iterations,
        // but failed due to errors.
        let mut pending_prune_ranges = PendingRanges::default();

        loop {
            // (1) Get the latest pruning bounds from the database.
            let mut watermark = tokio::select! {
                _ = cancel.cancelled() => {
                    info!(pipeline = H::NAME, "Shutdown received");
                    break;
                }

                _ = poll.tick() => {
                    let guard = metrics
                        .watermark_pruner_read_latency
                        .with_label_values(&[H::NAME])
                        .start_timer();

                    let Ok(mut conn) = store.connect().await else {
                        warn!(pipeline = H::NAME, "Pruner failed to connect, while fetching watermark");
                        continue;
                    };

                    match conn.pruner_watermark(H::NAME, config.delay()).await {
                        Ok(Some(current)) => {
                            guard.stop_and_record();
                            current
                        }

                        Ok(None) => {
                            guard.stop_and_record();
                            warn!(pipeline = H::NAME, "No watermark for pipeline, skipping");
                            continue;
                        }

                        Err(e) => {
                            guard.stop_and_record();
                            warn!(pipeline = H::NAME, "Failed to get watermark: {e}");
                            continue;
                        }
                    }
                }
            };

            // (2) Wait until this information can be acted upon.
            if let Some(wait_for) = watermark.wait_for() {
                debug!(pipeline = H::NAME, ?wait_for, "Waiting to prune");
                tokio::select! {
                    _ = tokio::time::sleep(wait_for) => {}
                    _ = cancel.cancelled() => {
                        info!(pipeline = H::NAME, "Shutdown received");
                        break;
                    }
                }
            }

            // Tracks the current highest `pruner_hi` not yet written to db. This is updated as
            // chunks complete.
            let mut highest_pruned = watermark.pruner_hi;
            // Tracks the `pruner_hi` that has been written to the db.
            let mut highest_watermarked = watermark.pruner_hi;

            // (3) Collect all the new chunks that are ready to be pruned.
            // This will also advance the watermark.
            while let Some((from, to_exclusive)) = watermark.next_chunk(config.max_chunk_size) {
                pending_prune_ranges.schedule(from, to_exclusive);
            }

            debug!(
                pipeline = H::NAME,
                "Number of chunks to prune: {}",
                pending_prune_ranges.len()
            );

            // (3) Prune chunk by chunk to avoid the task waiting on a long-running database
            // transaction, between tests for cancellation.
            // Spawn all tasks in parallel, but limit the number of concurrent tasks.
            let semaphore = Arc::new(Semaphore::new(config.prune_concurrency as usize));
            let mut tasks = FuturesUnordered::new();
            for (from, to_exclusive) in pending_prune_ranges.iter() {
                let semaphore = semaphore.clone();
                let cancel = cancel.child_token();
                let metrics = metrics.clone();
                let handler = handler.clone();

                let db = store.clone();

                tasks.push(tokio::spawn(async move {
                    let _permit = tokio::select! {
                        permit = semaphore.acquire() => {
                            permit.unwrap()
                        }
                        _ = cancel.cancelled() => {
                            return ((from, to_exclusive), Err(anyhow::anyhow!("Cancelled")));
                        }
                    };
                    let result = prune_task_impl(metrics, db, handler, from, to_exclusive).await;
                    ((from, to_exclusive), result)
                }));
            }

            // (4) Wait for all tasks to finish. For each task, if it succeeds, remove the range
            // from the pending_prune_ranges. Otherwise the range will remain in the map and will be
            // retried in the next iteration. Update the highest_pruned watermark if the task
            // succeeds in metrics and in db, to minimize redundant pruner work if the pipeline is
            // restarted.
            while let Some(r) = tasks.next().await {
                let ((from, to_exclusive), result) = r.unwrap();
                match result {
                    Ok(()) => {
                        pending_prune_ranges.remove(&from);
                        let pruner_hi = pending_prune_ranges.get_pruner_hi();
                        highest_pruned = highest_pruned.max(pruner_hi);
                    }
                    Err(e) => {
                        error!(
                            pipeline = H::NAME,
                            "Failed to prune data for range: {from} to {to_exclusive}: {e}"
                        );
                    }
                }

                if highest_pruned > highest_watermarked {
                    metrics
                        .watermark_pruner_hi
                        .with_label_values(&[H::NAME])
                        .set(highest_pruned as i64);

                    let guard = metrics
                        .watermark_pruner_write_latency
                        .with_label_values(&[H::NAME])
                        .start_timer();

                    let Ok(mut conn) = store.connect().await else {
                        warn!(
                            pipeline = H::NAME,
                            "Pruner failed to connect while updating watermark"
                        );
                        continue;
                    };

                    match conn.set_pruner_watermark(H::NAME, highest_pruned).await {
                        Err(e) => {
                            let elapsed = guard.stop_and_record();
                            error!(
                                pipeline = H::NAME,
                                elapsed_ms = elapsed * 1000.0,
                                "Failed to update pruner watermark: {e}"
                            )
                        }
                        Ok(true) => {
                            highest_watermarked = highest_pruned;
                            let elapsed = guard.stop_and_record();
                            logger.log::<H>(
                                LoggerWatermark::checkpoint(highest_watermarked),
                                elapsed,
                            );

                            metrics
                                .watermark_pruner_hi_in_db
                                .with_label_values(&[H::NAME])
                                .set(highest_watermarked as i64);
                        }
                        Ok(false) => {}
                    }
                }
            }
        }

        info!(pipeline = H::NAME, "Stopping pruner");
    })
}

async fn prune_task_impl<H: Handler + Send + Sync + 'static>(
    metrics: Arc<IndexerMetrics>,
    db: H::Store,
    handler: Arc<H>,
    from: u64,
    to_exclusive: u64,
) -> Result<(), anyhow::Error> {
    metrics
        .total_pruner_chunks_attempted
        .with_label_values(&[H::NAME])
        .inc();

    let guard = metrics
        .pruner_delete_latency
        .with_label_values(&[H::NAME])
        .start_timer();

    let mut conn = db.connect().await?;

    debug!(pipeline = H::NAME, "Pruning from {from} to {to_exclusive}");

    let affected = match handler.prune(from, to_exclusive, &mut conn).await {
        Ok(affected) => {
            guard.stop_and_record();
            affected
        }

        Err(e) => {
            guard.stop_and_record();
            return Err(e);
        }
    };

    metrics
        .total_pruner_chunks_deleted
        .with_label_values(&[H::NAME])
        .inc();

    metrics
        .total_pruner_rows_deleted
        .with_label_values(&[H::NAME])
        .inc_by(affected as u64);

    Ok(())
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::sync::{Arc, Mutex};
    use std::time::{Duration, SystemTime, UNIX_EPOCH};

    use async_trait::async_trait;
    use prometheus::Registry;
    use sui_indexer_alt_framework_store_traits::{
        CommitterWatermark, PrunerWatermark, ReaderWatermark, Store,
    };
    use sui_types::full_checkpoint_content::CheckpointData;

    use crate::{pipeline::Processor, store::Connection, FieldCount};

    use super::*;

    #[derive(Clone, Default)]
    pub struct MockWatermark {
        epoch_hi_inclusive: u64,
        checkpoint_hi_inclusive: u64,
        tx_hi: u64,
        timestamp_ms_hi_inclusive: u64,
        reader_lo: u64,
        pruner_timestamp: u64,
        pruner_hi: u64,
    }

    #[derive(Clone)]
    pub struct MockStore {
        pub watermarks: Arc<Mutex<MockWatermark>>,
        /// Map of checkpoint sequence number to list of transaction sequence numbers.
        pub data: Arc<Mutex<HashMap<u64, Vec<u64>>>>,
    }

    #[derive(Clone)]
    pub struct MockConnection<'c>(pub &'c MockStore);

    #[async_trait]
    impl Connection for MockConnection<'_> {
        async fn committer_watermark(
            &mut self,
            _pipeline: &'static str,
        ) -> Result<Option<CommitterWatermark>, anyhow::Error> {
            let watermarks = self.0.watermarks.lock().unwrap();
            Ok(Some(CommitterWatermark {
                epoch_hi_inclusive: watermarks.epoch_hi_inclusive,
                checkpoint_hi_inclusive: watermarks.checkpoint_hi_inclusive,
                tx_hi: watermarks.tx_hi,
                timestamp_ms_hi_inclusive: watermarks.timestamp_ms_hi_inclusive,
            }))
        }

        async fn reader_watermark(
            &mut self,
            _pipeline: &'static str,
        ) -> Result<Option<ReaderWatermark>, anyhow::Error> {
            let watermarks = self.0.watermarks.lock().unwrap();
            Ok(Some(ReaderWatermark {
                checkpoint_hi_inclusive: watermarks.checkpoint_hi_inclusive,
                reader_lo: watermarks.reader_lo,
            }))
        }

        async fn pruner_watermark(
            &mut self,
            _pipeline: &'static str,
            delay: Duration,
        ) -> Result<Option<PrunerWatermark>, anyhow::Error> {
            let watermarks = self.0.watermarks.lock().unwrap();
            let elapsed_ms = watermarks.pruner_timestamp as i64
                - SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap()
                    .as_millis() as i64;
            let wait_for_ms = delay.as_millis() as i64 + elapsed_ms;
            Ok(Some(PrunerWatermark {
                pruner_hi: watermarks.pruner_hi,
                reader_lo: watermarks.reader_lo,
                wait_for_ms,
            }))
        }

        async fn set_committer_watermark(
            &mut self,
            _pipeline: &'static str,
            watermark: CommitterWatermark,
        ) -> anyhow::Result<bool> {
            let mut watermarks = self.0.watermarks.lock().unwrap();
            watermarks.epoch_hi_inclusive = watermark.epoch_hi_inclusive;
            watermarks.checkpoint_hi_inclusive = watermark.checkpoint_hi_inclusive;
            watermarks.tx_hi = watermark.tx_hi;
            watermarks.timestamp_ms_hi_inclusive = watermark.timestamp_ms_hi_inclusive;
            Ok(true)
        }

        async fn set_reader_watermark(
            &mut self,
            _pipeline: &'static str,
            reader_lo: u64,
        ) -> anyhow::Result<bool> {
            let mut watermarks = self.0.watermarks.lock().unwrap();
            watermarks.reader_lo = reader_lo;
            watermarks.pruner_timestamp = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_millis() as u64;
            Ok(true)
        }

        async fn set_pruner_watermark(
            &mut self,
            _pipeline: &'static str,
            pruner_hi: u64,
        ) -> anyhow::Result<bool> {
            let mut watermarks = self.0.watermarks.lock().unwrap();
            watermarks.pruner_hi = pruner_hi;
            Ok(true)
        }
    }

    #[async_trait]
    impl Store for MockStore {
        type Connection<'c> = MockConnection<'c>;

        async fn connect<'c>(&'c self) -> Result<Self::Connection<'c>, anyhow::Error> {
            Ok(MockConnection(self))
        }
    }

    #[derive(FieldCount)]
    pub struct StoredData {
        pub cp_sequence_number: u64,
        pub tx_sequence_numbers: Vec<u64>,
    }

    pub struct DataPipeline;

    impl Processor for DataPipeline {
        const NAME: &'static str = "data";

        type Value = StoredData;

        fn process(&self, checkpoint: &Arc<CheckpointData>) -> anyhow::Result<Vec<Self::Value>> {
            let start_tx = checkpoint.checkpoint_summary.network_total_transactions as usize
                - checkpoint.transactions.len();
            let tx_sequence_numbers = checkpoint
                .transactions
                .iter()
                .enumerate()
                .map(|(i, _)| (start_tx + i) as u64)
                .collect();
            let value = StoredData {
                cp_sequence_number: checkpoint.checkpoint_summary.sequence_number,
                tx_sequence_numbers,
            };

            Ok(vec![value])
        }
    }

    #[async_trait]
    impl Handler for DataPipeline {
        type Store = MockStore;

        async fn commit<'a>(
            values: &[Self::Value],
            conn: &mut MockConnection<'a>,
        ) -> anyhow::Result<usize> {
            let mut data = conn.0.data.lock().unwrap();
            for value in values {
                data.insert(value.cp_sequence_number, value.tx_sequence_numbers.clone());
            }
            Ok(values.len())
        }

        async fn prune<'a>(
            &self,
            from: u64,
            to_exclusive: u64,
            conn: &mut MockConnection<'a>,
        ) -> anyhow::Result<usize> {
            let mut data = conn.0.data.lock().unwrap();
            for cp_sequence_number in from..to_exclusive {
                data.remove(&cp_sequence_number);
            }
            Ok((to_exclusive - from) as usize)
        }
    }

    #[tokio::test]
    async fn test_pruner() {
        let handler = Arc::new(DataPipeline);
        let pruner_config = PrunerConfig {
            interval_ms: 10,
            delay_ms: 2000,
            retention: 1,
            max_chunk_size: 100,
            prune_concurrency: 1,
        };
        let registry = Registry::new_custom(Some("test".to_string()), None).unwrap();
        let metrics = IndexerMetrics::new(&registry);
        let cancel = CancellationToken::new();

        // Update data
        let test_data = HashMap::from([(1, vec![1, 2, 3]), (2, vec![4, 5, 6]), (3, vec![7, 8, 9])]);
        // Update committer watermark
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64;

        let watermark = MockWatermark {
            epoch_hi_inclusive: 0,
            checkpoint_hi_inclusive: 3,
            tx_hi: 9,
            timestamp_ms_hi_inclusive: timestamp,
            reader_lo: 3,
            pruner_timestamp: timestamp,
            pruner_hi: 0,
        };
        let store = MockStore {
            watermarks: Arc::new(Mutex::new(watermark)),
            data: Arc::new(Mutex::new(test_data.clone())),
        };

        // Start the pruner
        let store_clone = store.clone();
        let cancel_clone = cancel.clone();
        let pruner_handle = tokio::spawn(async move {
            pruner(
                handler,
                Some(pruner_config),
                store_clone,
                metrics,
                cancel_clone,
            )
            .await
        });

        // Wait a short time within delay_ms
        tokio::time::sleep(Duration::from_millis(200)).await;
        {
            let data = store.data.lock().unwrap();
            assert!(
                data.contains_key(&1),
                "Checkpoint 1 shouldn't be pruned before delay"
            );
            assert!(
                data.contains_key(&2),
                "Checkpoint 2 shouldn't be pruned before delay"
            );
            assert!(
                data.contains_key(&3),
                "Checkpoint 3 shouldn't be pruned before delay"
            );
        }

        // Wait for the delay to expire
        tokio::time::sleep(Duration::from_millis(2000)).await;

        // Now checkpoint 1 should be pruned
        {
            let data = store.data.lock().unwrap();
            assert!(
                !data.contains_key(&1),
                "Checkpoint 1 should be pruned after delay"
            );

            // Checkpoint 3 should never be pruned (it's the reader_lo)
            assert!(data.contains_key(&3), "Checkpoint 3 should be preserved");

            // Check that the pruner_hi was updated past 1
            let watermark = store.watermarks.lock().unwrap();
            assert!(
                watermark.pruner_hi > 1,
                "Pruner watermark should be updated"
            );
        }

        // Clean up
        cancel.cancel();
        let _ = tokio::time::timeout(Duration::from_millis(1000), pruner_handle).await;
    }

    #[tokio::test]
    async fn test_pruner_timestamp_in_the_past() {
        let handler = Arc::new(DataPipeline);
        let pruner_config = PrunerConfig {
            interval_ms: 10,
            delay_ms: 20_000,
            retention: 1,
            max_chunk_size: 100,
            prune_concurrency: 1,
        };
        let registry = Registry::new_custom(Some("test".to_string()), None).unwrap();
        let metrics = IndexerMetrics::new(&registry);
        let cancel = CancellationToken::new();

        // Update data
        let test_data = HashMap::from([(1, vec![1, 2, 3]), (2, vec![4, 5, 6]), (3, vec![7, 8, 9])]);
        // Update committer watermark
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64;

        let watermark = MockWatermark {
            epoch_hi_inclusive: 0,
            checkpoint_hi_inclusive: 3,
            tx_hi: 9,
            timestamp_ms_hi_inclusive: timestamp,
            reader_lo: 3,
            pruner_timestamp: 0,
            pruner_hi: 0,
        };
        let store = MockStore {
            watermarks: Arc::new(Mutex::new(watermark)),
            data: Arc::new(Mutex::new(test_data.clone())),
        };

        // Start the pruner
        let store_clone = store.clone();
        let cancel_clone = cancel.clone();
        let pruner_handle = tokio::spawn(async move {
            pruner(
                handler,
                Some(pruner_config),
                store_clone,
                metrics,
                cancel_clone,
            )
            .await
        });

        // Because the `pruner_timestamp` is in the past, even with the delay_ms it should be pruned
        // close to immediately. To be safe, sleep for 1000ms before checking, which is well under
        // the delay_ms of 20_000 ms.
        tokio::time::sleep(Duration::from_millis(500)).await;

        {
            let data = store.data.lock().unwrap();
            assert!(!data.contains_key(&1), "Checkpoint 1 should be pruned");

            assert!(!data.contains_key(&2), "Checkpoint 2 should be pruned");

            // Checkpoint 3 should never be pruned (it's the reader_lo)
            assert!(data.contains_key(&3), "Checkpoint 3 should be preserved");

            // Check that the pruner_hi was updated past 1
            let watermark = store.watermarks.lock().unwrap();
            assert!(
                watermark.pruner_hi > 1,
                "Pruner watermark should be updated"
            );
        }

        // Clean up
        cancel.cancel();
        let _ = tokio::time::timeout(Duration::from_millis(1000), pruner_handle).await;
    }
}
