// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeMap;
use std::sync::Arc;

use futures::StreamExt;
use futures::stream::FuturesUnordered;
use sui_futures::service::Service;
use tokio::sync::Semaphore;
use tokio::sync::mpsc;
use tokio::time::MissedTickBehavior;
use tokio::time::interval;
use tracing::debug;
use tracing::error;
use tracing::info;
use tracing::warn;

use crate::metrics::IndexerMetrics;
use crate::pipeline::concurrent::Handler;
use crate::pipeline::concurrent::PrunerConfig;
use crate::store::ConcurrentConnection;
use crate::store::Store;

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
/// After each chunk is successfully pruned, the range is forwarded on `tx` to the
/// [super::prune_watermark] task, which is responsible for advancing `pruner_hi` in the database.
///
/// If the `config` is `None`, the task will shutdown immediately.
pub(super) fn pruner<H: Handler>(
    handler: Arc<H>,
    config: Option<PrunerConfig>,
    store: H::Store,
    tx: mpsc::Sender<(u64, u64)>,
    metrics: Arc<IndexerMetrics>,
) -> Service {
    Service::new().spawn_aborting(async move {
        let Some(config) = config else {
            info!(pipeline = H::NAME, "Skipping pruner task");
            return Ok(());
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

        // Maintains the list of chunks that are ready to be pruned but not yet pruned.
        // This map can contain ranges that were attempted to be pruned in previous iterations,
        // but failed due to errors.
        let mut pending_prune_ranges = PendingRanges::default();

        loop {
            poll.tick().await;

            // (1) Get the latest pruning bounds from the database.
            let mut watermark = {
                let guard = metrics
                    .watermark_pruner_read_latency
                    .with_label_values(&[H::NAME])
                    .start_timer();

                let Ok(mut conn) = store.connect().await else {
                    warn!(
                        pipeline = H::NAME,
                        "Pruner failed to connect, while fetching watermark"
                    );
                    continue;
                };

                match conn.pruner_watermark(H::NAME, config.delay()).await {
                    Ok(Some(current)) => {
                        guard.stop_and_record();
                        current
                    }

                    Ok(None) => {
                        guard.stop_and_record();
                        info!(pipeline = H::NAME, "No watermark for pipeline, skipping");
                        continue;
                    }

                    Err(e) => {
                        guard.stop_and_record();
                        warn!(pipeline = H::NAME, "Failed to get watermark: {e}");
                        continue;
                    }
                }
            };

            // (2) Wait until this information can be acted upon.
            if let Some(wait_for) = watermark.wait_for() {
                debug!(pipeline = H::NAME, ?wait_for, "Waiting to prune");
                tokio::time::sleep(wait_for).await;
            }

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

            // (4) Prune chunk by chunk to avoid the task waiting on a long-running database
            // transaction, between tests for cancellation.
            // Spawn all tasks in parallel, but limit the number of concurrent tasks.
            let semaphore = Arc::new(Semaphore::new(config.prune_concurrency as usize));
            let mut tasks = FuturesUnordered::new();
            for (from, to_exclusive) in pending_prune_ranges.iter() {
                let semaphore = semaphore.clone();
                let metrics = metrics.clone();
                let handler = handler.clone();

                let db = store.clone();

                tasks.push(tokio::spawn(async move {
                    let _permit = semaphore.acquire().await.unwrap();
                    let result = prune_task_impl(metrics, db, handler, from, to_exclusive).await;
                    ((from, to_exclusive), result)
                }));
            }

            // (5) Wait for all tasks to finish. For each task, if it succeeds, remove the range
            // from the pending_prune_ranges and forward it to the prune_watermark task.
            // Otherwise the range will remain in the map and will be retried in the next
            // iteration.
            while let Some(r) = tasks.next().await {
                let ((from, to_exclusive), result) = r.unwrap();
                match result {
                    Ok(()) => {
                        pending_prune_ranges.remove(&from);
                        if tx.send((from, to_exclusive)).await.is_err() {
                            info!(pipeline = H::NAME, "Prune watermark channel closed");
                            return Ok(());
                        }
                    }
                    Err(e) => {
                        error!(
                            pipeline = H::NAME,
                            "Failed to prune data for range: {from} to {to_exclusive}: {e}"
                        );
                    }
                }
            }
        }
    })
}

async fn prune_task_impl<H: Handler>(
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
    use std::sync::Arc;
    use std::time::SystemTime;
    use std::time::UNIX_EPOCH;

    use async_trait::async_trait;
    use prometheus::Registry;
    use sui_types::full_checkpoint_content::Checkpoint;
    use tokio::sync::mpsc;
    use tokio::time::Duration;

    use crate::FieldCount;
    use crate::metrics::IndexerMetrics;
    use crate::mocks::store::*;
    use crate::pipeline::Processor;
    use crate::pipeline::concurrent::BatchStatus;
    use crate::pipeline::concurrent::prune_watermark::prune_watermark;

    use super::*;

    fn spawn_pruner_stack(
        handler: Arc<DataPipeline>,
        config: PrunerConfig,
        store: MockStore,
        metrics: Arc<IndexerMetrics>,
    ) -> (Service, Service) {
        let (tx, rx) = mpsc::channel(16);
        let s_pruner = pruner(
            handler,
            Some(config.clone()),
            store.clone(),
            tx,
            metrics.clone(),
        );
        let s_prune_watermark = prune_watermark::<DataPipeline>(Some(config), rx, store, metrics);
        (s_pruner, s_prune_watermark)
    }

    #[derive(Clone, FieldCount)]
    pub struct StoredData;

    pub struct DataPipeline;

    #[async_trait]
    impl Processor for DataPipeline {
        const NAME: &'static str = "data";

        type Value = StoredData;

        async fn process(&self, _checkpoint: &Arc<Checkpoint>) -> anyhow::Result<Vec<Self::Value>> {
            Ok(vec![])
        }
    }

    #[async_trait]
    impl Handler for DataPipeline {
        type Store = MockStore;
        type Batch = Vec<Self::Value>;

        fn batch(
            &self,
            batch: &mut Self::Batch,
            values: &mut std::vec::IntoIter<Self::Value>,
        ) -> BatchStatus {
            batch.extend(values);
            BatchStatus::Pending
        }

        async fn commit<'a>(
            &self,
            batch: &Self::Batch,
            _conn: &mut MockConnection<'a>,
        ) -> anyhow::Result<usize> {
            Ok(batch.len())
        }

        async fn prune<'a>(
            &self,
            from: u64,
            to_exclusive: u64,
            conn: &mut MockConnection<'a>,
        ) -> anyhow::Result<usize> {
            conn.0.prune_data(DataPipeline::NAME, from, to_exclusive)
        }
    }

    #[test]
    fn test_pending_ranges_basic_scheduling() {
        let mut ranges = PendingRanges::default();

        // Schedule initial range
        ranges.schedule(1, 5);

        // Schedule non-overlapping range
        ranges.schedule(10, 15);

        // Verify ranges are stored correctly
        let scheduled: Vec<_> = ranges.iter().collect();
        assert_eq!(scheduled, vec![(1, 5), (10, 15)]);
    }

    #[test]
    fn test_pending_ranges_double_pruning_prevention() {
        let mut ranges = PendingRanges::default();

        // Schedule initial range
        ranges.schedule(1, 5);

        // Try to schedule overlapping range.
        ranges.schedule(3, 7);

        let scheduled: Vec<_> = ranges.iter().collect();
        assert_eq!(scheduled, vec![(1, 5), (5, 7)]);

        // Try to schedule range that's entirely covered by previous range
        ranges.schedule(2, 4); // Entirely within (1,5), should be ignored
        assert_eq!(ranges.len(), 2); // No change

        let scheduled: Vec<_> = ranges.iter().collect();
        assert_eq!(scheduled, vec![(1, 5), (5, 7)]); // No change
    }

    #[test]
    fn test_pending_ranges_exact_duplicate() {
        let mut ranges = PendingRanges::default();

        // Schedule initial range
        ranges.schedule(1, 5);
        assert_eq!(ranges.len(), 1);

        // Schedule exact same range.
        ranges.schedule(1, 5);
        assert_eq!(ranges.len(), 1); // No change

        let scheduled: Vec<_> = ranges.iter().collect();
        assert_eq!(scheduled, vec![(1, 5)]);
    }

    #[test]
    fn test_pending_ranges_adjacent_ranges() {
        let mut ranges = PendingRanges::default();

        // Schedule initial range
        ranges.schedule(1, 5);

        // Schedule adjacent range
        ranges.schedule(5, 10);

        let scheduled: Vec<_> = ranges.iter().collect();
        assert_eq!(scheduled, vec![(1, 5), (5, 10)]);
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
        let metrics = IndexerMetrics::new(None, &registry);

        // Update data
        let test_data = HashMap::from([(1, vec![1, 2, 3]), (2, vec![4, 5, 6]), (3, vec![7, 8, 9])]);
        // Update committer watermark
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64;

        let watermark = MockWatermark {
            epoch_hi_inclusive: 0,
            checkpoint_hi_inclusive: Some(3),
            tx_hi: 9,
            timestamp_ms_hi_inclusive: timestamp,
            reader_lo: 3,
            pruner_timestamp: timestamp,
            pruner_hi: 0,
            chain_id: None,
        };
        let store = MockStore::new()
            .with_watermark(DataPipeline::NAME, watermark)
            .with_data(DataPipeline::NAME, test_data);

        // Start the pruner + prune_watermark stack
        let _services = spawn_pruner_stack(handler, pruner_config, store.clone(), metrics);

        // Wait a short time within delay_ms
        tokio::time::sleep(Duration::from_millis(200)).await;
        {
            let data = store.data.get(DataPipeline::NAME).unwrap();
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
            let data = store.data.get(DataPipeline::NAME).unwrap();
            assert!(
                !data.contains_key(&1),
                "Checkpoint 1 should be pruned after delay"
            );

            // Checkpoint 3 should never be pruned (it's the reader_lo)
            assert!(data.contains_key(&3), "Checkpoint 3 should be preserved");

            // Check that the pruner_hi was updated past 1
            let watermark = store.watermark(DataPipeline::NAME).unwrap();
            assert!(
                watermark.pruner_hi > 1,
                "Pruner watermark should be updated"
            );
        }
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
        let metrics = IndexerMetrics::new(None, &registry);

        // Update data
        let test_data = HashMap::from([(1, vec![1, 2, 3]), (2, vec![4, 5, 6]), (3, vec![7, 8, 9])]);
        // Update committer watermark
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64;

        let watermark = MockWatermark {
            epoch_hi_inclusive: 0,
            checkpoint_hi_inclusive: Some(3),
            tx_hi: 9,
            timestamp_ms_hi_inclusive: timestamp,
            reader_lo: 3,
            pruner_timestamp: 0,
            pruner_hi: 0,
            chain_id: None,
        };
        let store = MockStore::new()
            .with_watermark(DataPipeline::NAME, watermark)
            .with_data(DataPipeline::NAME, test_data);

        // Start the pruner + prune_watermark stack
        let _services = spawn_pruner_stack(handler, pruner_config, store.clone(), metrics);

        // Because the `pruner_timestamp` is in the past, even with the delay_ms it should be pruned
        // close to immediately. To be safe, sleep for 1000ms before checking, which is well under
        // the delay_ms of 20_000 ms.
        tokio::time::sleep(Duration::from_millis(500)).await;

        {
            let data = store.data.get(DataPipeline::NAME).unwrap();
            assert!(!data.contains_key(&1), "Checkpoint 1 should be pruned");

            assert!(!data.contains_key(&2), "Checkpoint 2 should be pruned");

            // Checkpoint 3 should never be pruned (it's the reader_lo)
            assert!(data.contains_key(&3), "Checkpoint 3 should be preserved");

            // Check that the pruner_hi was updated past 1
            let watermark = store.watermark(DataPipeline::NAME).unwrap();
            assert!(
                watermark.pruner_hi > 1,
                "Pruner watermark should be updated"
            );
        }
    }

    #[tokio::test]
    async fn test_pruner_watermark_update_with_retries() {
        let handler = Arc::new(DataPipeline);
        let pruner_config = PrunerConfig {
            interval_ms: 3_000, // Long interval to test retried attempts of failed range.
            delay_ms: 100,      // Short delay to speed up each interval
            retention: 1,
            max_chunk_size: 1, // Process one checkpoint at a time
            prune_concurrency: 1,
        };
        let registry = Registry::new_custom(Some("test".to_string()), None).unwrap();
        let metrics = IndexerMetrics::new(None, &registry);

        // Set up test data for checkpoints 1-4
        let test_data = HashMap::from([
            (1, vec![1, 2]),
            (2, vec![3, 4]),
            (3, vec![5, 6]),
            (4, vec![7, 8]),
        ]);

        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64;

        let watermark = MockWatermark {
            epoch_hi_inclusive: 0,
            checkpoint_hi_inclusive: Some(4),
            tx_hi: 8,
            timestamp_ms_hi_inclusive: timestamp,
            reader_lo: 4,        // Allow pruning up to checkpoint 4 (exclusive)
            pruner_timestamp: 0, // Past timestamp so delay doesn't block
            pruner_hi: 1,
            chain_id: None,
        };

        // Configure failing behavior: range [1,2) should fail once before succeeding
        let store = MockStore::new()
            .with_watermark(DataPipeline::NAME, watermark)
            .with_data(DataPipeline::NAME, test_data.clone())
            .with_prune_failures(1, 2, 1);

        // Start the pruner + prune_watermark stack
        let _services = spawn_pruner_stack(handler, pruner_config, store.clone(), metrics);

        // Wait for first pruning cycle - ranges [2,3) and [3,4) should succeed, [1,2) should fail
        tokio::time::sleep(Duration::from_millis(500)).await;
        {
            let data = store.data.get(DataPipeline::NAME).unwrap();
            let watermarks = store.watermark(DataPipeline::NAME).unwrap();

            // Verify watermark doesn't advance past the failed range [1,2)
            assert_eq!(
                watermarks.pruner_hi, 1,
                "Pruner watermark should remain at 1 because range [1,2) failed"
            );
            assert!(data.contains_key(&1), "Checkpoint 1 should be preserved");
            assert!(!data.contains_key(&2), "Checkpoint 2 should be pruned");
            assert!(!data.contains_key(&3), "Checkpoint 3 should be pruned");
            assert!(data.contains_key(&4), "Checkpoint 4 should be preserved");
        }

        // Wait for second pruning cycle - range [1,2) should succeed on retry
        tokio::time::sleep(Duration::from_millis(3000)).await;
        {
            let data = store.data.get(DataPipeline::NAME).unwrap();
            let watermarks = store.watermark(DataPipeline::NAME).unwrap();

            // Verify watermark advances after all ranges complete successfully
            assert_eq!(
                watermarks.pruner_hi, 4,
                "Pruner watermark should advance to 4 after all ranges complete"
            );
            assert!(!data.contains_key(&1), "Checkpoint 1 should be pruned");
            assert!(!data.contains_key(&2), "Checkpoint 2 should be pruned");
            assert!(!data.contains_key(&3), "Checkpoint 3 should be pruned");
            assert!(data.contains_key(&4), "Checkpoint 4 should be preserved");
        }
    }
}
