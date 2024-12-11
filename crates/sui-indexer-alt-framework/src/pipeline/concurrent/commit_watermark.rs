// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{
    cmp::Ordering,
    collections::{btree_map::Entry, BTreeMap},
    sync::Arc,
};

use sui_pg_db::Db;
use tokio::{
    sync::mpsc,
    task::JoinHandle,
    time::{interval, MissedTickBehavior},
};
use tokio_util::sync::CancellationToken;
use tracing::{debug, error, info, warn};

use crate::{
    metrics::IndexerMetrics,
    pipeline::{logging::WatermarkLogger, CommitterConfig, WatermarkPart, WARN_PENDING_WATERMARKS},
    watermarks::CommitterWatermark,
};

use super::Handler;

/// The watermark task is responsible for keeping track of a pipeline's out-of-order commits and
/// updating its row in the `watermarks` table when a continuous run of checkpoints have landed
/// since the last watermark update.
///
/// It receives watermark "parts" that detail the proportion of each checkpoint's data that has
/// been written out by the committer and periodically (on a configurable interval) checks if the
/// watermark for the pipeline can be pushed forward. The watermark can be pushed forward if there
/// is one or more complete (all data for that checkpoint written out) watermarks spanning
/// contiguously from the current high watermark into the future.
///
/// If it detects that more than [WARN_PENDING_WATERMARKS] watermarks have built up, it will issue
/// a warning, as this could be the indication of a memory leak, and the caller probably intended
/// to run the indexer with watermarking disabled (e.g. if they are running a backfill).
///
/// The task regularly traces its progress, outputting at a higher log level every
/// [LOUD_WATERMARK_UPDATE_INTERVAL]-many checkpoints.
///
/// The task will shutdown if the `cancel` token is signalled, or if the `rx` channel closes and
/// the watermark cannot be progressed. If `skip_watermark` is set, the task will shutdown
/// immediately.
pub(super) fn commit_watermark<H: Handler + 'static>(
    initial_watermark: Option<CommitterWatermark<'static>>,
    config: CommitterConfig,
    skip_watermark: bool,
    mut rx: mpsc::Receiver<Vec<WatermarkPart>>,
    db: Db,
    metrics: Arc<IndexerMetrics>,
    cancel: CancellationToken,
) -> JoinHandle<()> {
    tokio::spawn(async move {
        if skip_watermark {
            info!(pipeline = H::NAME, "Skipping commit watermark task");
            return;
        }

        let mut poll = interval(config.watermark_interval());
        poll.set_missed_tick_behavior(MissedTickBehavior::Delay);

        // To correctly update the watermark, the task tracks the watermark it last tried to write
        // and the watermark parts for any checkpoints that have been written since then
        // ("pre-committed"). After each batch is written, the task will try to progress the
        // watermark as much as possible without going over any holes in the sequence of
        // checkpoints (entirely missing watermarks, or incomplete watermarks).
        let mut precommitted: BTreeMap<u64, WatermarkPart> = BTreeMap::new();
        let (mut watermark, mut next_checkpoint) = if let Some(watermark) = initial_watermark {
            let next = watermark.checkpoint_hi_inclusive + 1;
            (watermark, next)
        } else {
            (CommitterWatermark::initial(H::NAME.into()), 0)
        };

        // The watermark task will periodically output a log message at a higher log level to
        // demonstrate that the pipeline is making progress.
        let mut logger = WatermarkLogger::new("concurrent_committer", &watermark);

        info!(pipeline = H::NAME, ?watermark, "Starting commit watermark");

        loop {
            tokio::select! {
                _ = cancel.cancelled() => {
                    info!(pipeline = H::NAME, "Shutdown received");
                    break;
                }

                _ = poll.tick() => {
                    if precommitted.len() > WARN_PENDING_WATERMARKS {
                        warn!(
                            pipeline = H::NAME,
                            pending = precommitted.len(),
                            "Pipeline has a large number of pending commit watermarks",
                        );
                    }

                    let Ok(mut conn) = db.connect().await else {
                        warn!(pipeline = H::NAME, "Commit watermark task failed to get connection for DB");
                        continue;
                    };

                    // Check if the pipeline's watermark needs to be updated
                    let guard = metrics
                        .watermark_gather_latency
                        .with_label_values(&[H::NAME])
                        .start_timer();

                    let mut watermark_needs_update = false;
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
                                watermark = pending.remove().watermark;
                                watermark_needs_update = true;
                                next_checkpoint += 1;
                            }

                            // Next pending checkpoint is in the past. Out of order watermarks can
                            // be encountered when a pipeline is starting up, because ingestion
                            // must start at the lowest checkpoint across all pipelines, or because
                            // of a backfill, where the initial checkpoint has been overridden.
                            Ordering::Greater => {
                                // Track how many we see to make sure it doesn't grow without
                                // bound.
                                metrics
                                    .total_watermarks_out_of_order
                                    .with_label_values(&[H::NAME])
                                    .inc();

                                pending.remove();
                            }
                        }
                    }

                    let elapsed = guard.stop_and_record();

                    metrics
                        .watermark_epoch
                        .with_label_values(&[H::NAME])
                        .set(watermark.epoch_hi_inclusive);

                    metrics
                        .watermark_checkpoint
                        .with_label_values(&[H::NAME])
                        .set(watermark.checkpoint_hi_inclusive);

                    metrics
                        .watermark_transaction
                        .with_label_values(&[H::NAME])
                        .set(watermark.tx_hi);

                    metrics
                        .watermark_timestamp_ms
                        .with_label_values(&[H::NAME])
                        .set(watermark.timestamp_ms_hi_inclusive);

                    debug!(
                        pipeline = H::NAME,
                        elapsed_ms = elapsed * 1000.0,
                        watermark = watermark.checkpoint_hi_inclusive,
                        timestamp = %watermark.timestamp(),
                        pending = precommitted.len(),
                        "Gathered watermarks",
                    );

                    if watermark_needs_update {
                        let guard = metrics
                            .watermark_commit_latency
                            .with_label_values(&[H::NAME])
                            .start_timer();

                        // TODO: If initial_watermark is empty, when we update watermark
                        // for the first time, we should also update the low watermark.
                        match watermark.update(&mut conn).await {
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

                                metrics
                                    .watermark_epoch_in_db
                                    .with_label_values(&[H::NAME])
                                    .set(watermark.epoch_hi_inclusive);

                                metrics
                                    .watermark_checkpoint_in_db
                                    .with_label_values(&[H::NAME])
                                    .set(watermark.checkpoint_hi_inclusive);

                                metrics
                                    .watermark_transaction_in_db
                                    .with_label_values(&[H::NAME])
                                    .set(watermark.tx_hi);

                                metrics
                                    .watermark_timestamp_in_db_ms
                                    .with_label_values(&[H::NAME])
                                    .set(watermark.timestamp_ms_hi_inclusive);
                            }
                            Ok(false) => {}
                        }
                    }

                    if rx.is_closed() && rx.is_empty() {
                        info!(pipeline = H::NAME, "Committer closed channel");
                        break;
                    }
                }

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
                }
            }
        }

        info!(
            pipeline = H::NAME,
            ?watermark,
            "Stopping committer watermark task"
        );
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{pipeline::Processor, MIGRATIONS};
    use prometheus::Registry;
    use std::time::Duration;
    use sui_field_count::FieldCount;
    use sui_pg_db::{self as db, temp::TempDb, Db, DbArgs};
    use tokio::sync::mpsc;

    struct TestHandler;

    struct Entry;

    impl FieldCount for Entry {
        const FIELD_COUNT: usize = 1;
    }

    impl Processor for TestHandler {
        const NAME: &'static str = "test";

        type Value = Entry;

        fn process(
            &self,
            _checkpoint: &Arc<sui_types::full_checkpoint_content::CheckpointData>,
        ) -> anyhow::Result<Vec<Self::Value>> {
            Ok(vec![])
        }
    }

    #[async_trait::async_trait]
    impl Handler for TestHandler {
        async fn commit(
            _values: &[Self::Value],
            _conn: &mut db::Connection<'_>,
        ) -> anyhow::Result<usize> {
            Ok(0)
        }
    }

    async fn test_db() -> (TempDb, Db) {
        let temp_db = TempDb::new().unwrap();
        let db_config = DbArgs::new(temp_db.database().url().clone());
        let db = Db::new(db_config).await.unwrap();
        db.run_migrations(MIGRATIONS).await.unwrap();
        (temp_db, db)
    }

    // Create a new commit watermark with the given checkpoint.
    // For the testing done here, only checkpoint matters so all other fields are arbitrary.
    async fn new_commit_watermark(checkpoint: i64) -> CommitterWatermark<'static> {
        CommitterWatermark {
            pipeline: "test".into(),
            epoch_hi_inclusive: 1,
            checkpoint_hi_inclusive: checkpoint,
            tx_hi: 10,
            timestamp_ms_hi_inclusive: 100,
        }
    }

    #[tokio::test]
    async fn test_commit_watermark_basic() {
        let (_temp_db, db) = test_db().await;
        let registry = Registry::new_custom(Some("indexer_alt".to_string()), None).unwrap();
        let metrics = Arc::new(IndexerMetrics::new(&registry));
        let cancel = CancellationToken::new();
        let (tx, rx) = mpsc::channel(100);

        let initial_watermark = new_commit_watermark(5).await;

        let handle = commit_watermark::<TestHandler>(
            Some(initial_watermark),
            CommitterConfig::default(),
            false,
            rx,
            db.clone(),
            metrics,
            cancel.clone(),
        );

        // Send some watermark parts
        let part = WatermarkPart {
            watermark: new_commit_watermark(6).await,
            batch_rows: 0,
            total_rows: 0,
        };
        tx.send(vec![part]).await.unwrap();

        // Let the task process the watermark
        tokio::time::sleep(Duration::from_millis(500)).await;

        let watermark =
            CommitterWatermark::read_highest_watermark(&mut db.connect().await.unwrap(), "test")
                .await
                .unwrap()
                .unwrap();
        assert_eq!(watermark.checkpoint_hi_inclusive, 6);

        // Cancel and wait for shutdown
        cancel.cancel();
        handle.await.unwrap();
    }

    #[tokio::test]
    async fn test_commit_watermark_skip() {
        let (_temp_db, db) = test_db().await;
        let registry = Registry::new_custom(Some("indexer_alt".to_string()), None).unwrap();
        let metrics = Arc::new(IndexerMetrics::new(&registry));
        let cancel = CancellationToken::new();
        let (_tx, rx) = mpsc::channel(100);

        let handle = commit_watermark::<TestHandler>(
            None,
            CommitterConfig::default(),
            true,
            rx,
            db,
            metrics,
            cancel,
        );

        // Task should exit immediately when skip_watermark is true
        handle.await.unwrap();
    }

    #[tokio::test]
    async fn test_commit_watermark_out_of_order() {
        let (_temp_db, db) = test_db().await;
        let registry = Registry::new_custom(Some("indexer_alt".to_string()), None).unwrap();
        let metrics = Arc::new(IndexerMetrics::new(&registry));
        let cancel = CancellationToken::new();
        let (tx, rx) = mpsc::channel(100);

        let handle = commit_watermark::<TestHandler>(
            None,
            CommitterConfig::default(),
            false,
            rx,
            db.clone(),
            metrics.clone(),
            cancel.clone(),
        );

        // Send a correct watermark part first
        let part1 = WatermarkPart {
            watermark: new_commit_watermark(0).await,
            batch_rows: 10,
            total_rows: 10,
        };
        tx.send(vec![part1]).await.unwrap();
        // Let the task process the watermark
        tokio::time::sleep(Duration::from_millis(50)).await;
        let watermark =
            CommitterWatermark::read_highest_watermark(&mut db.connect().await.unwrap(), "test")
                .await
                .unwrap()
                .unwrap();
        assert_eq!(watermark.checkpoint_hi_inclusive, 0);

        // Send an out of order watermark part
        let part2 = WatermarkPart {
            watermark: new_commit_watermark(2).await,
            batch_rows: 10,
            total_rows: 10,
        };
        tx.send(vec![part2]).await.unwrap();

        // Let the task process the watermark
        tokio::time::sleep(Duration::from_millis(50)).await;
        // Verify watermark not updated since checkpoint 1 has not been received yet.
        let watermark =
            CommitterWatermark::read_highest_watermark(&mut db.connect().await.unwrap(), "test")
                .await
                .unwrap()
                .unwrap();
        assert_eq!(watermark.checkpoint_hi_inclusive, 0);

        // Send the correct watermark part for checkpoint 1
        let part1 = WatermarkPart {
            watermark: new_commit_watermark(1).await,
            batch_rows: 10,
            total_rows: 10,
        };
        tx.send(vec![part1]).await.unwrap();

        // Let the task process the watermark
        tokio::time::sleep(Duration::from_millis(500)).await;
        // Now we should write both checkponts 1 and 2, and the watermark should be updated to checkpoint 2
        let watermark =
            CommitterWatermark::read_highest_watermark(&mut db.connect().await.unwrap(), "test")
                .await
                .unwrap()
                .unwrap();
        assert_eq!(watermark.checkpoint_hi_inclusive, 2);

        // Now try sending watermark for checkpoint 1 again.
        let part1_again = WatermarkPart {
            watermark: new_commit_watermark(1).await,
            batch_rows: 10,
            total_rows: 10,
        };
        tx.send(vec![part1_again]).await.unwrap();
        tokio::time::sleep(Duration::from_millis(500)).await;

        // This should increment the out of order metric but not change the watermark in the db.
        assert_eq!(
            metrics
                .total_watermarks_out_of_order
                .with_label_values(&["test"])
                .get(),
            1
        );
        let watermark =
            CommitterWatermark::read_highest_watermark(&mut db.connect().await.unwrap(), "test")
                .await
                .unwrap()
                .unwrap();
        assert_eq!(watermark.checkpoint_hi_inclusive, 2);

        cancel.cancel();
        handle.await.unwrap();
    }

    #[tokio::test]
    async fn test_commit_watermark_multiple_parts() {
        // Test that the watermark is updated correctly when one checkpoint has multiple parts.
        let (_temp_db, db) = test_db().await;
        let registry = Registry::new_custom(Some("indexer_alt".to_string()), None).unwrap();
        let metrics = Arc::new(IndexerMetrics::new(&registry));
        let cancel = CancellationToken::new();

        let (tx, rx) = mpsc::channel(100);

        let initial_watermark = new_commit_watermark(5).await;

        let handle = commit_watermark::<TestHandler>(
            Some(initial_watermark),
            CommitterConfig::default(),
            false,
            rx,
            db.clone(),
            metrics.clone(),
            cancel.clone(),
        );

        // Send first and sencond part of checkpoint 6
        let part1 = WatermarkPart {
            watermark: new_commit_watermark(6).await,
            batch_rows: 5,
            total_rows: 10,
        };
        let part2 = WatermarkPart {
            watermark: new_commit_watermark(6).await,
            batch_rows: 3,
            total_rows: 10,
        };
        tx.send(vec![part1, part2]).await.unwrap();

        // Now send a complete watermark for checkpoint 1
        let part3 = WatermarkPart {
            watermark: new_commit_watermark(7).await,
            batch_rows: 10,
            total_rows: 10,
        };
        tx.send(vec![part3]).await.unwrap();

        // Let the task process the first part
        tokio::time::sleep(Duration::from_millis(500)).await;

        // Verify watermark not updated since checkpoint incomplete
        let watermark =
            CommitterWatermark::read_highest_watermark(&mut db.connect().await.unwrap(), "test")
                .await
                .unwrap();
        assert!(watermark.is_none());
        // Now send the last part of checkpoint 6
        let part4 = WatermarkPart {
            watermark: new_commit_watermark(6).await,
            batch_rows: 2,
            total_rows: 10,
        };
        tx.send(vec![part4]).await.unwrap();

        // Let the task process the second part
        tokio::time::sleep(Duration::from_millis(500)).await;

        // With the last part of checkpoint 6, the watermark should be updated to checkpoint 7
        let watermark =
            CommitterWatermark::read_highest_watermark(&mut db.connect().await.unwrap(), "test")
                .await
                .unwrap()
                .unwrap();
        assert_eq!(watermark.checkpoint_hi_inclusive, 7);

        cancel.cancel();
        handle.await.unwrap();
    }
}
