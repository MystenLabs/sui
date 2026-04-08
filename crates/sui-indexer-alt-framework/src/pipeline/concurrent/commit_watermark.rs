// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::cmp::Ordering;
use std::collections::BTreeMap;
use std::collections::btree_map::Entry;
use std::sync::Arc;

use sui_futures::service::Service;
use tokio::sync::SetOnce;
use tokio::sync::mpsc;
use tracing::debug;
use tracing::error;
use tracing::info;
use tracing::warn;

use crate::metrics::CheckpointLagMetricReporter;
use crate::metrics::IndexerMetrics;
use crate::pipeline::CommitterConfig;
use crate::pipeline::WARN_PENDING_WATERMARKS;
use crate::pipeline::WatermarkPart;
use crate::pipeline::concurrent::Direction;
use crate::pipeline::concurrent::Handler;
use crate::pipeline::logging::WatermarkLogger;
use crate::store::CommitterWatermark;
use crate::store::ConcurrentConnection;
use crate::store::Connection;
use crate::store::Store;
use crate::store::pipeline_task;

/// The watermark task is responsible for keeping track of a pipeline's out-of-order commits and
/// updating its row in the `watermarks` table when a continuous run of checkpoints have landed
/// since the last watermark update.
///
/// It receives watermark "parts" that detail the proportion of each checkpoint's data that has been
/// written out by the committer and periodically (on a configurable interval) checks if the
/// watermark for the pipeline can be pushed forward. The watermark can be pushed forward if there
/// is one or more complete (all data for that checkpoint written out) watermarks spanning
/// contiguously from the current high watermark into the future.
///
/// If it detects that more than [WARN_PENDING_WATERMARKS] watermarks have built up, it will issue a
/// warning, as this could be the indication of a memory leak, and the caller probably intended to
/// run the indexer with watermarking disabled (e.g. if they are running a backfill).
///
/// The task regularly traces its progress, outputting at a higher log level every
/// [LOUD_WATERMARK_UPDATE_INTERVAL]-many checkpoints.
///
/// The task will shutdown if the `rx` channel closes and the watermark cannot be progressed.
pub(super) fn commit_watermark<H: Handler>(
    mut next_checkpoint: u64,
    first_checkpoint: u64,
    reader_lo: u64,
    config: CommitterConfig,
    mut rx: mpsc::Receiver<(Direction, Vec<WatermarkPart>)>,
    store: H::Store,
    task: Option<String>,
    backwards_complete: Arc<SetOnce<()>>,
    metrics: Arc<IndexerMetrics>,
) -> Service {
    // SAFETY: on indexer instantiation, we've checked that the pipeline name is valid.
    let pipeline_task = pipeline_task::<H::Store>(H::NAME, task.as_deref()).unwrap();
    Service::new().spawn_aborting(async move {
        // To correctly update the watermark, the task tracks the watermark it last tried to write
        // and the watermark parts for any checkpoints that have been written since then
        // ("pre-committed"). After each batch is written, the task will try to progress the
        // watermark as much as possible without going over any holes in the sequence of
        // checkpoints (entirely missing watermarks, or incomplete watermarks).
        let mut precommitted: BTreeMap<u64, WatermarkPart> = BTreeMap::new();

        // Backwards-direction state. The cursor starts at `reader_lo - 1` (the highest checkpoint
        // *below* `reader_lo` that backwards needs to process) and decrements. `None` means there
        // is no backwards work, or backwards has finished.
        let mut precommitted_backward: BTreeMap<u64, WatermarkPart> = BTreeMap::new();
        let mut next_backwards: Option<u64> = if reader_lo > first_checkpoint {
            Some(reader_lo - 1)
        } else {
            // No backwards work to do — fire the signal immediately so the gated tasks
            // (reader_watermark, pruner) can begin.
            let _ = backwards_complete.set(());
            None
        };
        let mut pending_reader_lo: Option<u64> = None;

        // The watermark task will periodically output a log message at a higher log level to
        // demonstrate that the pipeline is making progress.
        let mut logger = WatermarkLogger::new("concurrent_committer");

        let checkpoint_lag_reporter = CheckpointLagMetricReporter::new_for_pipeline::<H>(
            &metrics.watermarked_checkpoint_timestamp_lag,
            &metrics.latest_watermarked_checkpoint_timestamp_lag_ms,
            &metrics.watermark_checkpoint_in_db,
        );

        info!(
            pipeline = H::NAME,
            next_checkpoint, "Starting commit watermark task"
        );

        let mut next_wake = tokio::time::Instant::now();
        let mut pending_watermark = None;

        loop {
            let mut should_write_db = false;

            tokio::select! {
                () = tokio::time::sleep_until(next_wake) => {
                    // Schedule next wake immediately, so the timer effectively runs in parallel
                    // with the commit logic below.
                    next_wake = config.watermark_interval_with_jitter();
                    should_write_db = true;
                }
                Some((direction, parts)) = rx.recv() => {
                    let target = match direction {
                        Direction::Forward => &mut precommitted,
                        Direction::Backwards => &mut precommitted_backward,
                    };
                    for part in parts {
                        match target.entry(part.checkpoint()) {
                            Entry::Vacant(entry) => {
                                entry.insert(part);
                            }

                            Entry::Occupied(mut entry) => {
                                entry.get_mut().add(part);
                            }
                        }
                    }
                }
            }

            // Advance the watermark through contiguous precommitted entries on every
            // iteration, not just when the DB write timer fires. This ensures commit_hi
            // feedback reaches the broadcaster immediately.
            let guard = metrics
                .watermark_gather_latency
                .with_label_values(&[H::NAME])
                .start_timer();

            while let Some(pending) = precommitted.first_entry() {
                let part = pending.get();

                // Some rows from the next watermark have not landed yet.
                if !part.is_complete() {
                    break;
                }

                match next_checkpoint.cmp(&part.watermark.checkpoint_hi_inclusive) {
                    // Next pending checkpoint is from the future.
                    Ordering::Less => break,

                    // This is the next checkpoint -- include it.
                    Ordering::Equal => {
                        pending_watermark = Some(pending.remove().watermark);
                        next_checkpoint += 1;
                    }

                    // Next pending checkpoint is in the past. Out of order watermarks can
                    // be encountered when a pipeline is starting up, because ingestion
                    // must start at the lowest checkpoint across all pipelines, or because
                    // of a backfill, where the initial checkpoint has been overridden.
                    Ordering::Greater => {
                        // Track how many we see to make sure it doesn't grow without bound.
                        metrics
                            .total_watermarks_out_of_order
                            .with_label_values(&[H::NAME])
                            .inc();

                        pending.remove();
                    }
                }
            }

            // Walk the backwards cursor downward through contiguous complete entries.
            //
            // The cursor starts at `reader_lo - 1` and moves down. After each successful step,
            // `pending_reader_lo` records the new `reader_lo` value to write to the database
            // (`= cursor`, since `cursor` is the lowest-numbered checkpoint we've now persisted).
            // When the cursor reaches `first_checkpoint`, we fire `backwards_complete` so the
            // gated reader_watermark and pruner tasks can begin.
            while let Some(cursor) = next_backwards {
                let Some(pending) = precommitted_backward.last_entry() else {
                    break;
                };

                // Highest pending entry is *not* the cursor — there's a gap, wait.
                if *pending.key() != cursor {
                    break;
                }

                // Some rows from the cursor checkpoint have not landed yet.
                if !pending.get().is_complete() {
                    break;
                }

                pending.remove();
                pending_reader_lo = Some(cursor);

                if cursor == first_checkpoint {
                    info!(
                        pipeline = H::NAME,
                        first_checkpoint, "Backwards range complete"
                    );
                    let _ = backwards_complete.set(());
                    next_backwards = None;
                } else {
                    next_backwards = Some(cursor - 1);
                }
            }

            let elapsed = guard.stop_and_record();

            if let Some(ref watermark) = pending_watermark {
                metrics
                    .watermark_epoch
                    .with_label_values(&[H::NAME])
                    .set(watermark.epoch_hi_inclusive as i64);

                metrics
                    .watermark_checkpoint
                    .with_label_values(&[H::NAME])
                    .set(watermark.checkpoint_hi_inclusive as i64);

                metrics
                    .watermark_transaction
                    .with_label_values(&[H::NAME])
                    .set(watermark.tx_hi as i64);

                metrics
                    .watermark_timestamp_ms
                    .with_label_values(&[H::NAME])
                    .set(watermark.timestamp_ms_hi_inclusive as i64);

                debug!(
                    pipeline = H::NAME,
                    elapsed_ms = elapsed * 1000.0,
                    watermark = watermark.checkpoint_hi_inclusive,
                    timestamp = %watermark.timestamp(),
                    pending = precommitted.len(),
                    "Gathered watermarks",
                );
            }

            if precommitted.len() > WARN_PENDING_WATERMARKS {
                warn!(
                    pipeline = H::NAME,
                    pending = precommitted.len(),
                    "Pipeline has a large number of pending commit watermarks",
                );
            }

            if precommitted_backward.len() > WARN_PENDING_WATERMARKS {
                warn!(
                    pipeline = H::NAME,
                    pending = precommitted_backward.len(),
                    "Pipeline has a large number of pending backwards commit watermarks",
                );
            }

            // DB writes are deferred to the timer interval to avoid excessive DB load.
            if should_write_db
                && let Some(watermark) = pending_watermark.take()
                && write_watermark::<H>(
                    &store,
                    &pipeline_task,
                    &watermark,
                    &mut logger,
                    &checkpoint_lag_reporter,
                    &metrics,
                )
                .await
                .is_err()
            {
                pending_watermark = Some(watermark);
            }

            if should_write_db
                && let Some(reader_lo) = pending_reader_lo.take()
                && write_backwards_reader_lo::<H>(&store, reader_lo, &metrics)
                    .await
                    .is_err()
            {
                pending_reader_lo = Some(reader_lo);
            }

            if rx.is_closed() && rx.is_empty() {
                info!(pipeline = H::NAME, "Committer closed channel");
                break;
            }
        }

        if let Some(watermark) = pending_watermark
            && write_watermark::<H>(
                &store,
                &pipeline_task,
                &watermark,
                &mut logger,
                &checkpoint_lag_reporter,
                &metrics,
            )
            .await
            .is_err()
        {
            warn!(
                pipeline = H::NAME,
                ?watermark,
                "Failed to write final watermark on shutdown, will not retry",
            );
        }

        if let Some(reader_lo) = pending_reader_lo
            && write_backwards_reader_lo::<H>(&store, reader_lo, &metrics)
                .await
                .is_err()
        {
            warn!(
                pipeline = H::NAME,
                reader_lo,
                "Failed to write final backwards reader_lo on shutdown, will not retry",
            );
        }

        info!(pipeline = H::NAME, "Stopping committer watermark task");
        Ok(())
    })
}

/// Lower the pipeline's `reader_lo` in the database to reflect backwards-indexing progress.
/// Returns `Err` so the caller can preserve the value for retry on the next tick.
async fn write_backwards_reader_lo<H: Handler>(
    store: &H::Store,
    reader_lo: u64,
    metrics: &IndexerMetrics,
) -> Result<(), ()> {
    let Ok(mut conn) = store.connect().await else {
        warn!(
            pipeline = H::NAME,
            "Backwards watermark write failed to get connection for DB"
        );
        return Err(());
    };

    match conn.lower_reader_watermark(H::NAME, reader_lo).await {
        Err(e) => {
            error!(
                pipeline = H::NAME,
                reader_lo, "Error lowering reader watermark: {e}",
            );
            Err(())
        }
        Ok(_) => {
            metrics
                .watermark_reader_lo_in_db
                .with_label_values(&[H::NAME])
                .set(reader_lo as i64);
            debug!(pipeline = H::NAME, reader_lo, "Lowered backwards reader_lo");
            Ok(())
        }
    }
}

/// Write the watermark to DB and update metrics. Returns `Err` on failure so the caller can
/// preserve the watermark for retry on the next tick.
async fn write_watermark<H: Handler>(
    store: &H::Store,
    pipeline_task: &str,
    watermark: &CommitterWatermark,
    logger: &mut WatermarkLogger,
    checkpoint_lag_reporter: &CheckpointLagMetricReporter,
    metrics: &IndexerMetrics,
) -> Result<(), ()> {
    let Ok(mut conn) = store.connect().await else {
        warn!(
            pipeline = H::NAME,
            "Commit watermark task failed to get connection for DB"
        );
        return Err(());
    };

    let guard = metrics
        .watermark_commit_latency
        .with_label_values(&[H::NAME])
        .start_timer();

    // TODO: If initial_watermark is empty, when we update watermark
    // for the first time, we should also update the low watermark.
    match conn
        .set_committer_watermark(pipeline_task, *watermark)
        .await
    {
        Err(e) => {
            let elapsed = guard.stop_and_record();
            error!(
                pipeline = H::NAME,
                elapsed_ms = elapsed * 1000.0,
                ?watermark,
                "Error updating commit watermark: {e}",
            );
            Err(())
        }

        Ok(true) => {
            let elapsed = guard.stop_and_record();

            logger.log::<H>(watermark, elapsed);

            checkpoint_lag_reporter.report_lag(
                watermark.checkpoint_hi_inclusive,
                watermark.timestamp_ms_hi_inclusive,
            );

            metrics
                .watermark_epoch_in_db
                .with_label_values(&[H::NAME])
                .set(watermark.epoch_hi_inclusive as i64);

            metrics
                .watermark_transaction_in_db
                .with_label_values(&[H::NAME])
                .set(watermark.tx_hi as i64);

            metrics
                .watermark_timestamp_in_db_ms
                .with_label_values(&[H::NAME])
                .set(watermark.timestamp_ms_hi_inclusive as i64);

            Ok(())
        }
        Ok(false) => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::time::Duration;

    use async_trait::async_trait;
    use sui_types::full_checkpoint_content::Checkpoint;
    use tokio::sync::mpsc;

    use crate::FieldCount;
    use crate::metrics::IndexerMetrics;
    use crate::mocks::store::*;
    use crate::pipeline::CommitterConfig;
    use crate::pipeline::Processor;
    use crate::pipeline::WatermarkPart;
    use crate::pipeline::concurrent::BatchStatus;
    use crate::store::CommitterWatermark;

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
        watermark_tx: mpsc::Sender<(Direction, Vec<WatermarkPart>)>,
        #[allow(unused)]
        commit_watermark: Service,
    }

    fn setup_test<H: Handler<Store = MockStore>>(
        config: CommitterConfig,
        next_checkpoint: u64,
        store: MockStore,
    ) -> TestSetup {
        setup_test_with_backwards::<H>(config, next_checkpoint, 0, 0, store).0
    }

    fn setup_test_with_backwards<H: Handler<Store = MockStore>>(
        config: CommitterConfig,
        next_checkpoint: u64,
        first_checkpoint: u64,
        reader_lo: u64,
        store: MockStore,
    ) -> (TestSetup, Arc<SetOnce<()>>) {
        let (watermark_tx, watermark_rx) = mpsc::channel(100);
        let metrics = IndexerMetrics::new(None, &Default::default());

        let store_clone = store.clone();
        let backwards_complete: Arc<SetOnce<()>> = Arc::new(SetOnce::new());

        let commit_watermark = commit_watermark::<H>(
            next_checkpoint,
            first_checkpoint,
            reader_lo,
            config,
            watermark_rx,
            store_clone,
            None,
            backwards_complete.clone(),
            metrics,
        );

        (
            TestSetup {
                store,
                watermark_tx,
                commit_watermark,
            },
            backwards_complete,
        )
    }

    fn create_watermark_part_for_checkpoint(checkpoint: u64) -> WatermarkPart {
        WatermarkPart {
            watermark: CommitterWatermark {
                checkpoint_hi_inclusive: checkpoint,
                ..Default::default()
            },
            batch_rows: 1,
            total_rows: 1,
        }
    }

    #[tokio::test]
    async fn test_basic_watermark_progression() {
        let config = CommitterConfig::default();
        let setup = setup_test::<DataPipeline>(config, 1, MockStore::default());

        // Send watermark parts in order
        for cp in 1..4 {
            let part = create_watermark_part_for_checkpoint(cp);
            setup.watermark_tx.send((Direction::Forward, vec![part])).await.unwrap();
        }

        // Wait for processing
        tokio::time::sleep(Duration::from_millis(100)).await;

        // Verify watermark progression
        let watermark = setup.store.watermark(DataPipeline::NAME).unwrap();
        assert_eq!(watermark.checkpoint_hi_inclusive, Some(3));
    }

    #[tokio::test]
    async fn test_out_of_order_watermarks() {
        let config = CommitterConfig::default();
        let setup = setup_test::<DataPipeline>(config, 1, MockStore::default());

        // Send watermark parts out of order
        let parts = vec![
            create_watermark_part_for_checkpoint(4),
            create_watermark_part_for_checkpoint(2),
            create_watermark_part_for_checkpoint(1),
        ];
        setup.watermark_tx.send((Direction::Forward, parts)).await.unwrap();

        // Wait for processing
        tokio::time::sleep(Duration::from_millis(100)).await;

        // Verify watermark hasn't progressed past 2
        let watermark = setup.store.watermark(DataPipeline::NAME).unwrap();
        assert_eq!(watermark.checkpoint_hi_inclusive, Some(2));

        // Send checkpoint 3 to fill the gap
        setup
            .watermark_tx
            .send((Direction::Forward, vec![create_watermark_part_for_checkpoint(3)]))
            .await
            .unwrap();

        // Wait for the next polling and processing
        tokio::time::sleep(Duration::from_secs(1)).await;

        // Verify watermark has progressed to 4
        let watermark = setup.store.watermark(DataPipeline::NAME).unwrap();
        assert_eq!(watermark.checkpoint_hi_inclusive, Some(4));
    }

    #[tokio::test]
    async fn test_watermark_with_connection_failure() {
        let config = CommitterConfig {
            watermark_interval_ms: 1_000, // Long polling interval to test connection retry
            ..Default::default()
        };
        let store = MockStore::default().with_connection_failures(1);
        let setup = setup_test::<DataPipeline>(config, 1, store);

        // Send watermark part
        let part = create_watermark_part_for_checkpoint(1);
        setup.watermark_tx.send((Direction::Forward, vec![part])).await.unwrap();

        // Wait for processing
        tokio::time::sleep(Duration::from_millis(200)).await;

        // Verify watermark hasn't progressed
        let watermark = setup.store.watermark(DataPipeline::NAME);
        assert!(watermark.is_none());

        // Wait for next polling and processing
        tokio::time::sleep(Duration::from_millis(1_200)).await;

        // Verify watermark has progressed
        let watermark = setup.store.watermark(DataPipeline::NAME).unwrap();
        assert_eq!(watermark.checkpoint_hi_inclusive, Some(1));
    }

    #[tokio::test]
    async fn test_committer_retries_on_commit_watermark_failure() {
        let config = CommitterConfig {
            watermark_interval_ms: 1_000, // Long polling interval to test connection retry
            ..Default::default()
        };
        // Create store with transaction failure configuration
        let store = MockStore::default().with_commit_watermark_failures(1); // Will fail once before succeeding
        let setup = setup_test::<DataPipeline>(config, 10, store);

        let part = WatermarkPart {
            watermark: CommitterWatermark {
                checkpoint_hi_inclusive: 10,
                ..Default::default()
            },
            batch_rows: 1,
            total_rows: 1,
        };
        setup.watermark_tx.send((Direction::Forward, vec![part])).await.unwrap();

        // Wait for initial poll to be over
        tokio::time::sleep(Duration::from_millis(200)).await;
        let watermark = setup.store.watermark(DataPipeline::NAME);
        assert!(watermark.is_none());

        // Wait for retries to complete
        tokio::time::sleep(Duration::from_millis(1_200)).await;

        // Verify watermark has progressed after retry
        let watermark = setup.store.watermark(DataPipeline::NAME).unwrap();
        assert_eq!(watermark.checkpoint_hi_inclusive, Some(10));
    }

    #[tokio::test]
    async fn test_committer_retries_on_commit_watermark_failure_advances() {
        let config = CommitterConfig {
            watermark_interval_ms: 1_000, // Long polling interval to test connection retry
            ..Default::default()          // Create store with transaction failure configuration
        };
        let store = MockStore::default().with_commit_watermark_failures(1); // Will fail once before succeeding
        let setup = setup_test::<DataPipeline>(config, 10, store);

        let part = WatermarkPart {
            watermark: CommitterWatermark {
                checkpoint_hi_inclusive: 10,
                ..Default::default()
            },
            batch_rows: 1,
            total_rows: 1,
        };
        setup.watermark_tx.send((Direction::Forward, vec![part])).await.unwrap();

        // Wait for initial poll to be over
        tokio::time::sleep(Duration::from_millis(200)).await;
        let watermark = setup.store.watermark(DataPipeline::NAME);
        assert!(watermark.is_none());

        let part = WatermarkPart {
            watermark: CommitterWatermark {
                checkpoint_hi_inclusive: 11,
                ..Default::default()
            },
            batch_rows: 1,
            total_rows: 1,
        };
        setup.watermark_tx.send((Direction::Forward, vec![part])).await.unwrap();

        // Wait for retries to complete
        tokio::time::sleep(Duration::from_millis(1_200)).await;

        let watermark = setup.store.watermark(DataPipeline::NAME).unwrap();
        assert_eq!(watermark.checkpoint_hi_inclusive, Some(11));
    }

    #[tokio::test]
    async fn test_incomplete_watermark() {
        let config = CommitterConfig {
            watermark_interval_ms: 1_000, // Long polling interval to test adding complete part
            ..Default::default()
        };
        let setup = setup_test::<DataPipeline>(config, 1, MockStore::default());

        // Send the first incomplete watermark part
        let part = WatermarkPart {
            watermark: CommitterWatermark {
                checkpoint_hi_inclusive: 1,
                ..Default::default()
            },
            batch_rows: 1,
            total_rows: 3,
        };
        setup.watermark_tx.send((Direction::Forward, vec![part.clone()])).await.unwrap();

        // Wait for processing
        tokio::time::sleep(Duration::from_millis(200)).await;

        // Verify watermark hasn't progressed
        let watermark = setup.store.watermark(DataPipeline::NAME);
        assert!(watermark.is_none());

        // Send the other two parts to complete the watermark
        setup
            .watermark_tx
            .send((Direction::Forward, vec![part.clone(), part.clone()]))
            .await
            .unwrap();

        // Wait for next polling and processing
        tokio::time::sleep(Duration::from_millis(1_200)).await;

        // Verify watermark has progressed
        let watermark = setup.store.watermark(DataPipeline::NAME).unwrap();
        assert_eq!(watermark.checkpoint_hi_inclusive, Some(1));
    }

    #[tokio::test]
    async fn test_no_initial_watermark() {
        let config = CommitterConfig::default();
        let setup = setup_test::<DataPipeline>(config, 0, MockStore::default());

        // Send the checkpoint 1 watermark
        setup
            .watermark_tx
            .send((Direction::Forward, vec![create_watermark_part_for_checkpoint(1)]))
            .await
            .unwrap();

        // Wait for processing
        tokio::time::sleep(Duration::from_millis(200)).await;

        // Verify watermark hasn't progressed
        let watermark = setup.store.watermark(DataPipeline::NAME);
        assert!(watermark.is_none());

        // Send the checkpoint 0 watermark to fill the gap.
        setup
            .watermark_tx
            .send((Direction::Forward, vec![create_watermark_part_for_checkpoint(0)]))
            .await
            .unwrap();

        // Wait for processing
        tokio::time::sleep(Duration::from_millis(1200)).await;

        // Verify watermark has progressed
        let watermark = setup.store.watermark(DataPipeline::NAME).unwrap();
        assert_eq!(watermark.checkpoint_hi_inclusive, Some(1));
    }

    /// Backwards-tagged watermark parts walk the cursor downward through contiguous complete
    /// entries from `reader_lo - 1` to `first_checkpoint`, lowering the persisted `reader_lo`.
    /// Once the cursor reaches `first_checkpoint`, the `backwards_complete` Notify is fired.
    #[tokio::test]
    async fn test_backwards_cursor_advances_and_lowers_reader_lo() {
        let store = MockStore::default().with_watermark(
            DataPipeline::NAME,
            MockWatermark {
                reader_lo: 10,
                ..Default::default()
            },
        );
        let config = CommitterConfig::default();
        // first_checkpoint = 0, reader_lo = 10 → backwards range [0..10).
        let (setup, backwards_complete) =
            setup_test_with_backwards::<DataPipeline>(config, 0, 0, 10, store);

        // Send backwards watermark parts in order from 9 down to 0 (the order they would arrive
        // from a descending broadcaster). We send them all together via separate sends to
        // exercise the cursor advancing through contiguous completed entries.
        for cp in (0..10).rev() {
            setup
                .watermark_tx
                .send((Direction::Backwards, vec![create_watermark_part_for_checkpoint(cp)]))
                .await
                .unwrap();
        }

        // Wait for the watermark task to drain its input and walk the cursor + write the DB.
        tokio::time::sleep(Duration::from_millis(1500)).await;

        // The persisted reader_lo should now be 0 (the lowest backwards checkpoint we processed).
        let watermark = setup.store.watermark(DataPipeline::NAME).unwrap();
        assert_eq!(
            watermark.reader_lo, 0,
            "Backwards cursor should have lowered reader_lo all the way to first_checkpoint",
        );

        // The signal should fire once the cursor reaches first_checkpoint.
        tokio::time::timeout(Duration::from_millis(500), backwards_complete.wait())
            .await
            .expect("backwards_complete should be set");
    }

    /// The backwards cursor must NOT advance through gaps in the pending map. If a non-contiguous
    /// part arrives, the cursor stops and waits for the gap to be filled.
    #[tokio::test]
    async fn test_backwards_cursor_waits_for_contiguous() {
        let store = MockStore::default().with_watermark(
            DataPipeline::NAME,
            MockWatermark {
                reader_lo: 10,
                ..Default::default()
            },
        );
        let config = CommitterConfig::default();
        let (setup, _backwards_complete) =
            setup_test_with_backwards::<DataPipeline>(config, 0, 0, 10, store);

        // Send 9 and 8, then SKIP 7, then send 6 and 5. The cursor should advance to 8 and stop.
        for cp in [9, 8, 6, 5] {
            setup
                .watermark_tx
                .send((Direction::Backwards, vec![create_watermark_part_for_checkpoint(cp)]))
                .await
                .unwrap();
        }

        tokio::time::sleep(Duration::from_millis(1500)).await;

        // reader_lo should be lowered to 8 (the lowest contiguous backwards checkpoint).
        let watermark = setup.store.watermark(DataPipeline::NAME).unwrap();
        assert_eq!(
            watermark.reader_lo, 8,
            "Cursor should have stopped at the gap (missing checkpoint 7)",
        );

        // Now fill the gap.
        setup
            .watermark_tx
            .send((Direction::Backwards, vec![create_watermark_part_for_checkpoint(7)]))
            .await
            .unwrap();

        tokio::time::sleep(Duration::from_millis(1500)).await;

        // Cursor should have walked through 7, 6, 5 → reader_lo = 5.
        let watermark = setup.store.watermark(DataPipeline::NAME).unwrap();
        assert_eq!(watermark.reader_lo, 5);
    }

    /// When `reader_lo == first_checkpoint`, there is no backwards work to do; the Notify must
    /// fire immediately at startup so the gated tasks (reader_watermark, pruner) can begin.
    #[tokio::test]
    async fn test_backwards_complete_notifies_immediately_when_no_work() {
        let store = MockStore::default().with_watermark(
            DataPipeline::NAME,
            MockWatermark {
                reader_lo: 5,
                ..Default::default()
            },
        );
        let config = CommitterConfig::default();
        // first_checkpoint = 5 == reader_lo → no backwards range.
        let (_setup, backwards_complete) =
            setup_test_with_backwards::<DataPipeline>(config, 0, 5, 5, store);

        // The signal should fire essentially immediately.
        tokio::time::timeout(Duration::from_millis(500), backwards_complete.wait())
            .await
            .expect("backwards_complete should be set immediately when no backwards work");
    }
}
