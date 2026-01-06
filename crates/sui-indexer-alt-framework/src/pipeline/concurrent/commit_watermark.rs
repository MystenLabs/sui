// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{
    cmp::Ordering,
    collections::{BTreeMap, btree_map::Entry},
    sync::Arc,
};

use sui_futures::service::Service;
use tokio::{
    sync::mpsc,
    time::{MissedTickBehavior, interval},
};
use tracing::{debug, error, info, warn};

use crate::{
    metrics::{CheckpointLagMetricReporter, IndexerMetrics},
    pipeline::{CommitterConfig, WARN_PENDING_WATERMARKS, WatermarkPart, logging::WatermarkLogger},
    store::{Connection, Store, pipeline_task},
};

use super::Handler;

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
pub(super) fn commit_watermark<H: Handler + 'static>(
    mut next_checkpoint: u64,
    config: CommitterConfig,
    mut rx: mpsc::Receiver<Vec<WatermarkPart>>,
    store: H::Store,
    task: Option<String>,
    metrics: Arc<IndexerMetrics>,
) -> Service {
    // SAFETY: on indexer instantiation, we've checked that the pipeline name is valid.
    let pipeline_task = pipeline_task::<H::Store>(H::NAME, task.as_deref()).unwrap();
    Service::new().spawn_aborting(async move {
        let mut poll = interval(config.watermark_interval());
        poll.set_missed_tick_behavior(MissedTickBehavior::Delay);

        // To correctly update the watermark, the task tracks the watermark it last tried to write
        // and the watermark parts for any checkpoints that have been written since then
        // ("pre-committed"). After each batch is written, the task will try to progress the
        // watermark as much as possible without going over any holes in the sequence of
        // checkpoints (entirely missing watermarks, or incomplete watermarks).
        let mut precommitted: BTreeMap<u64, WatermarkPart> = BTreeMap::new();

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

        loop {
            tokio::select! {
                _ = poll.tick() => {}
                Some(parts) = rx.recv() => {
                    for part in parts {
                        match precommitted.entry(part.checkpoint()) {
                            Entry::Vacant(entry) => {
                                entry.insert(part);
                            }

                            Entry::Occupied(mut entry) => {
                                entry.get_mut().add(part);
                            }
                        }
                    }

                    continue;
                }
            }

            // The presence of a watermark means that we can update the watermark in db.
            // However, concurrent pipelines do not need every watermark update to succeed.
            let mut watermark = None;

            if precommitted.len() > WARN_PENDING_WATERMARKS {
                warn!(
                    pipeline = H::NAME,
                    pending = precommitted.len(),
                    "Pipeline has a large number of pending commit watermarks",
                );
            }

            let Ok(mut conn) = store.connect().await else {
                warn!(
                    pipeline = H::NAME,
                    "Commit watermark task failed to get connection for DB"
                );
                continue;
            };

            // Check if the pipeline's watermark needs to be updated
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
                        watermark = Some(pending.remove().watermark);
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

            let elapsed = guard.stop_and_record();

            if let Some(watermark) = watermark {
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

                let guard = metrics
                    .watermark_commit_latency
                    .with_label_values(&[H::NAME])
                    .start_timer();

                // TODO: If initial_watermark is empty, when we update watermark
                // for the first time, we should also update the low watermark.
                match conn
                    .set_committer_watermark(&pipeline_task, watermark)
                    .await
                {
                    // If there's an issue updating the watermark, log it but keep going,
                    // it's OK for the watermark to lag from a correctness perspective.
                    Err(e) => {
                        let elapsed = guard.stop_and_record();
                        error!(
                            pipeline = H::NAME,
                            elapsed_ms = elapsed * 1000.0,
                            ?watermark,
                            "Error updating commit watermark: {e}",
                        );
                    }

                    Ok(true) => {
                        let elapsed = guard.stop_and_record();

                        logger.log::<H>(&watermark, elapsed);

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
                    }
                    Ok(false) => {}
                }
            }

            if rx.is_closed() && rx.is_empty() {
                info!(pipeline = H::NAME, "Committer closed channel");
                break;
            }
        }

        info!(pipeline = H::NAME, "Stopping committer watermark task");
        Ok(())
    })
}

#[cfg(test)]
mod tests {
    use std::{sync::Arc, time::Duration};

    use async_trait::async_trait;
    use sui_types::full_checkpoint_content::Checkpoint;
    use tokio::sync::mpsc;

    use crate::{
        FieldCount,
        metrics::IndexerMetrics,
        mocks::store::*,
        pipeline::{CommitterConfig, Processor, WatermarkPart, concurrent::BatchStatus},
        store::CommitterWatermark,
    };

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
        watermark_tx: mpsc::Sender<Vec<WatermarkPart>>,
        #[allow(unused)]
        commit_watermark: Service,
    }

    fn setup_test<H: Handler<Store = MockStore> + 'static>(
        config: CommitterConfig,
        next_checkpoint: u64,
        store: MockStore,
    ) -> TestSetup {
        let (watermark_tx, watermark_rx) = mpsc::channel(100);
        let metrics = IndexerMetrics::new(None, &Default::default());

        let store_clone = store.clone();

        let commit_watermark = commit_watermark::<H>(
            next_checkpoint,
            config,
            watermark_rx,
            store_clone,
            None,
            metrics,
        );

        TestSetup {
            store,
            watermark_tx,
            commit_watermark,
        }
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
            setup.watermark_tx.send(vec![part]).await.unwrap();
        }

        // Wait for processing
        tokio::time::sleep(Duration::from_millis(100)).await;

        // Verify watermark progression
        let watermark = setup.store.watermark(DataPipeline::NAME).unwrap();
        assert_eq!(watermark.checkpoint_hi_inclusive, 3);
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
        setup.watermark_tx.send(parts).await.unwrap();

        // Wait for processing
        tokio::time::sleep(Duration::from_millis(100)).await;

        // Verify watermark hasn't progressed past 2
        let watermark = setup.store.watermark(DataPipeline::NAME).unwrap();
        assert_eq!(watermark.checkpoint_hi_inclusive, 2);

        // Send checkpoint 3 to fill the gap
        setup
            .watermark_tx
            .send(vec![create_watermark_part_for_checkpoint(3)])
            .await
            .unwrap();

        // Wait for the next polling and processing
        tokio::time::sleep(Duration::from_secs(1)).await;

        // Verify watermark has progressed to 4
        let watermark = setup.store.watermark(DataPipeline::NAME).unwrap();
        assert_eq!(watermark.checkpoint_hi_inclusive, 4);
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
        setup.watermark_tx.send(vec![part]).await.unwrap();

        // Wait for processing
        tokio::time::sleep(Duration::from_millis(200)).await;

        // Verify watermark hasn't progressed
        let watermark = setup.store.watermark(DataPipeline::NAME);
        assert!(watermark.is_none());

        // Wait for next polling and processing
        tokio::time::sleep(Duration::from_millis(1_200)).await;

        // Verify watermark has progressed
        let watermark = setup.store.watermark(DataPipeline::NAME).unwrap();
        assert_eq!(watermark.checkpoint_hi_inclusive, 1);
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
        setup.watermark_tx.send(vec![part]).await.unwrap();

        // Wait for initial poll to be over
        tokio::time::sleep(Duration::from_millis(200)).await;
        let watermark = setup.store.watermark(DataPipeline::NAME);
        assert!(watermark.is_none());

        // Wait for retries to complete
        tokio::time::sleep(Duration::from_millis(1_200)).await;

        // Verify watermark is still none
        let watermark = setup.store.watermark(DataPipeline::NAME);
        assert!(watermark.is_none());
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
        setup.watermark_tx.send(vec![part]).await.unwrap();

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
        setup.watermark_tx.send(vec![part]).await.unwrap();

        // Wait for retries to complete
        tokio::time::sleep(Duration::from_millis(1_200)).await;

        let watermark = setup.store.watermark(DataPipeline::NAME).unwrap();
        assert_eq!(watermark.checkpoint_hi_inclusive, 11);
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
        setup.watermark_tx.send(vec![part.clone()]).await.unwrap();

        // Wait for processing
        tokio::time::sleep(Duration::from_millis(200)).await;

        // Verify watermark hasn't progressed
        let watermark = setup.store.watermark(DataPipeline::NAME);
        assert!(watermark.is_none());

        // Send the other two parts to complete the watermark
        setup
            .watermark_tx
            .send(vec![part.clone(), part.clone()])
            .await
            .unwrap();

        // Wait for next polling and processing
        tokio::time::sleep(Duration::from_millis(1_200)).await;

        // Verify watermark has progressed
        let watermark = setup.store.watermark(DataPipeline::NAME).unwrap();
        assert_eq!(watermark.checkpoint_hi_inclusive, 1);
    }

    #[tokio::test]
    async fn test_no_initial_watermark() {
        let config = CommitterConfig::default();
        let setup = setup_test::<DataPipeline>(config, 0, MockStore::default());

        // Send the checkpoint 1 watermark
        setup
            .watermark_tx
            .send(vec![create_watermark_part_for_checkpoint(1)])
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
            .send(vec![create_watermark_part_for_checkpoint(0)])
            .await
            .unwrap();

        // Wait for processing
        tokio::time::sleep(Duration::from_millis(1200)).await;

        // Verify watermark has progressed
        let watermark = setup.store.watermark(DataPipeline::NAME).unwrap();
        assert_eq!(watermark.checkpoint_hi_inclusive, 1);
    }
}
