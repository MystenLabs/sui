// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Duration;

use backoff::ExponentialBackoff;
use sui_futures::service::Service;
use tokio::sync::mpsc;
use tracing::error;
use tracing::info;
use tracing::warn;

use crate::metrics::IndexerMetrics;
use crate::pipeline::concurrent::Handler;
use crate::pipeline::concurrent::PrunerConfig;
use crate::pipeline::logging::LoggerWatermark;
use crate::pipeline::logging::WatermarkLogger;
use crate::store::ConcurrentConnection;
use crate::store::Store;

/// If reading the initial `pruner_hi` fails, retry starting at this interval.
const INITIAL_READ_RETRY_INTERVAL: Duration = Duration::from_millis(100);

/// Cap the retry interval at this value while reading the initial `pruner_hi`.
const MAX_READ_RETRY_INTERVAL: Duration = Duration::from_secs(1);

/// The prune watermark task is responsible for advancing the pipeline's `pruner_hi` watermark in
/// the database. It receives successfully pruned ranges `(from, to_exclusive)` from the `pruner`
/// task over an mpsc channel, buffers out-of-order completions in a `BTreeMap`, and advances
/// `pruner_hi` through the contiguous prefix of completed ranges.
///
/// Splitting watermark updates into their own task mirrors the `committer` + `commit_watermark`
/// pattern: the pruner task focuses on distributing work, while this task focuses on persisting
/// progress. At startup, the task reads the current DB `pruner_hi` to establish the baseline for
/// contiguity — the first received chunk may have `from > pruner_hi` if an earlier chunk failed
/// and is still being retried, so lazy-initialization from the first message is not safe.
///
/// If `config` is `None`, the task will shutdown immediately.
pub(super) fn prune_watermark<H: Handler>(
    config: Option<PrunerConfig>,
    mut rx: mpsc::Receiver<(u64, u64)>,
    store: H::Store,
    metrics: Arc<IndexerMetrics>,
) -> Service {
    Service::new().spawn_aborting(async move {
        if config.is_none() {
            info!(pipeline = H::NAME, "Skipping prune watermark task");
            return Ok(());
        };

        let initial_pruner_hi = read_initial_pruner_hi::<H>(&store).await;

        info!(
            pipeline = H::NAME,
            initial_pruner_hi, "Starting prune watermark task"
        );

        // Buffer completed ranges that are not yet contiguous with `highest_pruned`.
        let mut precommitted: BTreeMap<u64, u64> = BTreeMap::new();
        let mut highest_pruned: u64 = initial_pruner_hi;
        let mut highest_watermarked: u64 = initial_pruner_hi;

        let mut logger = WatermarkLogger::new("pruner");

        while let Some((from, to_exclusive)) = rx.recv().await {
            precommitted.insert(from, to_exclusive);

            while let Some(entry) = precommitted.first_entry() {
                if *entry.key() != highest_pruned {
                    break;
                }
                highest_pruned = entry.remove();
            }

            metrics
                .watermark_pruner_hi
                .with_label_values(&[H::NAME])
                .set(highest_pruned as i64);

            if highest_pruned > highest_watermarked {
                match write_pruner_watermark::<H>(&store, highest_pruned, &metrics).await {
                    Ok(elapsed) => {
                        highest_watermarked = highest_pruned;
                        logger.log::<H>(LoggerWatermark::checkpoint(highest_pruned), elapsed);
                        metrics
                            .watermark_pruner_hi_in_db
                            .with_label_values(&[H::NAME])
                            .set(highest_pruned as i64);
                    }
                    Err(()) => {
                        // Failure already logged; the next received range will retry.
                    }
                }
            }
        }

        info!(pipeline = H::NAME, "Stopping prune watermark task");
        Ok(())
    })
}

/// Read the initial `pruner_hi` from the DB, retrying with exponential backoff on connection or
/// query failures. If the pipeline has no watermark row yet, returns 0 — in that case the pruner
/// task will also find no watermark and produce no messages, so the value is unused until a
/// watermark is written.
async fn read_initial_pruner_hi<H: Handler>(store: &H::Store) -> u64 {
    let backoff = ExponentialBackoff {
        initial_interval: INITIAL_READ_RETRY_INTERVAL,
        current_interval: INITIAL_READ_RETRY_INTERVAL,
        max_interval: MAX_READ_RETRY_INTERVAL,
        max_elapsed_time: None,
        ..Default::default()
    };

    use backoff::Error as BE;
    let read = || async {
        let mut conn = store.connect().await.map_err(|e| {
            warn!(
                pipeline = H::NAME,
                "Failed to connect to read initial pruner watermark, retrying: {e}"
            );
            BE::transient(())
        })?;
        // Pass `Duration::ZERO` because we only need the current `pruner_hi`; the `delay` argument
        // exists to let the production pruner task wait until in-flight reads have completed.
        conn.pruner_watermark(H::NAME, Duration::ZERO)
            .await
            .map(|w| w.map_or(0, |w| w.pruner_hi))
            .map_err(|e| {
                warn!(
                    pipeline = H::NAME,
                    "Failed to read initial pruner watermark, retrying: {e}"
                );
                BE::transient(())
            })
    };

    // `max_elapsed_time: None` means the operation never terminates with a permanent error, so the
    // `Err` arm is unreachable — `unwrap_or(0)` is a safe fallback for the type system.
    backoff::future::retry(backoff, read).await.unwrap_or(0)
}

/// Write `pruner_hi` to the database. Returns the elapsed write time on success (so the caller can
/// pass it to the logger) and `Err(())` on connection or DB error.
async fn write_pruner_watermark<H: Handler>(
    store: &H::Store,
    pruner_hi: u64,
    metrics: &IndexerMetrics,
) -> Result<f64, ()> {
    let guard = metrics
        .watermark_pruner_write_latency
        .with_label_values(&[H::NAME])
        .start_timer();

    let Ok(mut conn) = store.connect().await else {
        let elapsed = guard.stop_and_record();
        warn!(
            pipeline = H::NAME,
            elapsed_ms = elapsed * 1000.0,
            "Prune watermark task failed to connect while updating watermark"
        );
        return Err(());
    };

    match conn.set_pruner_watermark(H::NAME, pruner_hi).await {
        Ok(_) => Ok(guard.stop_and_record()),
        Err(e) => {
            let elapsed = guard.stop_and_record();
            error!(
                pipeline = H::NAME,
                elapsed_ms = elapsed * 1000.0,
                "Failed to update pruner watermark: {e}"
            );
            Err(())
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::time::Duration;
    use std::time::SystemTime;
    use std::time::UNIX_EPOCH;

    use async_trait::async_trait;
    use prometheus::Registry;
    use sui_types::full_checkpoint_content::Checkpoint;
    use tokio::sync::mpsc;

    use crate::FieldCount;
    use crate::metrics::IndexerMetrics;
    use crate::mocks::store::*;
    use crate::pipeline::Processor;
    use crate::pipeline::concurrent::BatchStatus;
    use crate::pipeline::concurrent::Handler;
    use crate::pipeline::concurrent::PrunerConfig;

    use super::*;

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
            _batch: &Self::Batch,
            _conn: &mut MockConnection<'a>,
        ) -> anyhow::Result<usize> {
            Ok(0)
        }
    }

    struct TestSetup {
        store: MockStore,
        tx: mpsc::Sender<(u64, u64)>,
        #[allow(unused)]
        service: Service,
    }

    fn default_watermark() -> MockWatermark {
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64;
        MockWatermark {
            epoch_hi_inclusive: 0,
            checkpoint_hi_inclusive: Some(0),
            tx_hi: 0,
            timestamp_ms_hi_inclusive: timestamp,
            reader_lo: 0,
            pruner_timestamp: timestamp,
            pruner_hi: 0,
            chain_id: None,
        }
    }

    fn setup_test(config: Option<PrunerConfig>, store: MockStore) -> TestSetup {
        let (tx, rx) = mpsc::channel(16);
        let registry = Registry::new_custom(Some("test".to_string()), None).unwrap();
        let metrics = IndexerMetrics::new(None, &registry);
        let store_clone = store.clone();
        let service = prune_watermark::<DataPipeline>(config, rx, store_clone, metrics);
        TestSetup { store, tx, service }
    }

    async fn wait_for_pruner_hi(store: &MockStore, expected: u64, timeout: Duration) {
        let start = tokio::time::Instant::now();
        loop {
            if let Some(w) = store.watermark(DataPipeline::NAME)
                && w.pruner_hi == expected
            {
                return;
            }
            if start.elapsed() > timeout {
                let actual = store
                    .watermark(DataPipeline::NAME)
                    .map(|w| w.pruner_hi)
                    .unwrap_or(u64::MAX);
                panic!("Timed out waiting for pruner_hi={expected}; last observed {actual}");
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    }

    #[tokio::test]
    async fn test_basic_watermark_progression() {
        let store = MockStore::new().with_watermark(DataPipeline::NAME, default_watermark());
        let setup = setup_test(Some(PrunerConfig::default()), store);

        for range in [(0u64, 5u64), (5, 10), (10, 15)] {
            setup.tx.send(range).await.unwrap();
        }

        wait_for_pruner_hi(&setup.store, 15, Duration::from_secs(1)).await;
    }

    #[tokio::test]
    async fn test_out_of_order_ranges() {
        let store = MockStore::new().with_watermark(DataPipeline::NAME, default_watermark());
        let setup = setup_test(Some(PrunerConfig::default()), store);

        // First message establishes the baseline at `from = 0`, so (10, 15) gets buffered.
        setup.tx.send((0, 5)).await.unwrap();
        setup.tx.send((10, 15)).await.unwrap();

        // Wait for the (0, 5) write to land before we assert the stall.
        wait_for_pruner_hi(&setup.store, 5, Duration::from_secs(1)).await;

        tokio::time::sleep(Duration::from_millis(50)).await;
        assert_eq!(
            setup.store.watermark(DataPipeline::NAME).unwrap().pruner_hi,
            5,
            "Gap between (0, 5) and (10, 15) should stall pruner_hi at 5"
        );

        // Fill the gap. Both (5, 10) and the buffered (10, 15) should flush.
        setup.tx.send((5, 10)).await.unwrap();
        wait_for_pruner_hi(&setup.store, 15, Duration::from_secs(1)).await;
    }

    #[tokio::test]
    async fn test_watermark_advances_despite_connection_flakiness() {
        // Exercise the retry loops around DB access (initial read + per-message write). With a
        // handful of transient connection failures, the watermark should still eventually land.
        let store = MockStore::new()
            .with_watermark(DataPipeline::NAME, default_watermark())
            .with_connection_failures(3);
        let setup = setup_test(Some(PrunerConfig::default()), store);

        setup.tx.send((0, 5)).await.unwrap();
        setup.tx.send((5, 10)).await.unwrap();
        wait_for_pruner_hi(&setup.store, 10, Duration::from_secs(2)).await;
    }

    #[tokio::test]
    async fn test_no_config_exits_immediately() {
        let store = MockStore::new().with_watermark(DataPipeline::NAME, default_watermark());
        let setup = setup_test(None, store);

        // Sending into the channel succeeds (mpsc buffer), but the task has already exited so no
        // progression happens. Give the runtime a moment and assert pruner_hi is unchanged.
        setup.tx.send((0, 5)).await.unwrap();
        tokio::time::sleep(Duration::from_millis(50)).await;
        assert_eq!(
            setup.store.watermark(DataPipeline::NAME).unwrap().pruner_hi,
            0
        );
    }

    #[tokio::test]
    async fn test_non_zero_initial_baseline_buffers_leading_gap() {
        // The pruner task schedules chunks starting from DB `pruner_hi`, but a chunk at the front
        // of the sequence may still be retrying while later chunks succeed. The watermark task
        // must initialize its baseline from DB — not from the first received `from` — so those
        // later completions buffer until the gap is filled.
        let mut watermark = default_watermark();
        watermark.pruner_hi = 2;
        let store = MockStore::new().with_watermark(DataPipeline::NAME, watermark);
        let setup = setup_test(Some(PrunerConfig::default()), store);

        setup.tx.send((5, 10)).await.unwrap();
        tokio::time::sleep(Duration::from_millis(100)).await;
        assert_eq!(
            setup.store.watermark(DataPipeline::NAME).unwrap().pruner_hi,
            2,
            "Leading gap [2, 5) should block advancement"
        );

        setup.tx.send((2, 5)).await.unwrap();
        wait_for_pruner_hi(&setup.store, 10, Duration::from_secs(1)).await;
    }

    #[tokio::test]
    async fn test_missing_watermark_row_defaults_baseline_to_zero() {
        // If the pipeline is initialized but has not yet indexed any checkpoints,
        // `pruner_watermark` returns `Ok(None)` and the task starts from 0.
        let mut watermark = default_watermark();
        watermark.checkpoint_hi_inclusive = None;
        let store = MockStore::new().with_watermark(DataPipeline::NAME, watermark);
        let setup = setup_test(Some(PrunerConfig::default()), store);

        setup.tx.send((0, 5)).await.unwrap();
        wait_for_pruner_hi(&setup.store, 5, Duration::from_secs(1)).await;
    }

    #[tokio::test]
    async fn test_channel_close_shutdown() {
        let store = MockStore::new().with_watermark(DataPipeline::NAME, default_watermark());
        let setup = setup_test(Some(PrunerConfig::default()), store);

        setup.tx.send((0, 5)).await.unwrap();
        wait_for_pruner_hi(&setup.store, 5, Duration::from_secs(1)).await;

        drop(setup.tx);
        // The task must terminate cleanly once the channel is closed.
        setup.service.shutdown().await.unwrap();
    }
}
