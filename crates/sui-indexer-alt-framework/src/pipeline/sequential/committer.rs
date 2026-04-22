// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::cmp::Ordering;
use std::collections::BTreeMap;
use std::sync::Arc;

use scoped_futures::ScopedFutureExt;
use sui_futures::service::Service;
use tokio::sync::mpsc;
use tokio::time::MissedTickBehavior;
use tokio::time::interval;
use tracing::debug;
use tracing::info;
use tracing::warn;

use crate::metrics::CheckpointLagMetricReporter;
use crate::metrics::IndexerMetrics;
use crate::pipeline::IndexedCheckpoint;
use crate::pipeline::WARN_PENDING_WATERMARKS;
use crate::pipeline::logging::WatermarkLogger;
use crate::pipeline::sequential::Handler;
use crate::pipeline::sequential::SequentialConfig;
use crate::store::Connection;
use crate::store::SequentialStore;

/// The committer task gathers rows into batches and writes them to the database.
///
/// Data arrives out of order, grouped by checkpoint, on `rx`. The task orders them and waits to
/// write them until either a configural polling interval has passed (controlled by
/// `config.collect_interval()`), or `H::BATCH_SIZE` rows have been accumulated and we have
/// received the next expected checkpoint.
///
/// Writes are performed on checkpoint boundaries (more than one checkpoint can be present in a
/// single write), in a single transaction that includes all row updates and an update to the
/// watermark table.
pub(super) fn committer<H: Handler>(
    handler: Arc<H>,
    config: SequentialConfig,
    mut next_checkpoint: u64,
    mut rx: mpsc::Receiver<IndexedCheckpoint<H>>,
    store: H::Store,
    metrics: Arc<IndexerMetrics>,
    min_eager_rows: usize,
    max_batch_checkpoints: usize,
) -> Service {
    Service::new().spawn_aborting(async move {
        // The `poll` interval controls the maximum time to wait between commits, regardless of the
        // amount of data available.
        let mut poll = interval(config.committer.collect_interval());
        poll.set_missed_tick_behavior(MissedTickBehavior::Delay);

        // Buffer to gather the next batch to write. A checkpoint's data is only added to the batch
        // when it is known to come from the next checkpoint after `watermark` (the current tip of
        // the batch), and data from previous checkpoints will be discarded to avoid double writes.
        //
        // The batch may be non-empty at top of a tick of the committer's loop if the previous
        // attempt at a write failed. Attempt is incremented every time a batch write fails, and is
        // reset when it succeeds.
        let mut attempt = 0;
        let mut batch = H::Batch::default();
        let mut batch_rows = 0;
        let mut batch_checkpoints = 0;

        // The task keeps track of the highest (inclusive) checkpoint it has added to the batch
        // through `next_checkpoint`, and whether that batch needs to be written out. By extension
        // it also knows the next checkpoint to expect and add to the batch. In case of db txn
        // failures, we need to know the watermark update that failed, cached to this variable. in
        // case of db txn failures.
        let mut watermark = None;

        // The committer task will periodically output a log message at a higher log level to
        // demonstrate that the pipeline is making progress.
        let mut logger = WatermarkLogger::new("sequential_committer");

        let checkpoint_lag_reporter = CheckpointLagMetricReporter::new_for_pipeline::<H>(
            &metrics.watermarked_checkpoint_timestamp_lag,
            &metrics.latest_watermarked_checkpoint_timestamp_lag_ms,
            &metrics.watermark_checkpoint_in_db,
        );

        // Data for checkpoint that haven't been written yet. Note that `pending_rows` includes
        // rows in `batch`.
        let mut pending: BTreeMap<u64, IndexedCheckpoint<H>> = BTreeMap::new();
        let mut pending_rows = 0;

        info!(pipeline = H::NAME, "Starting committer");

        loop {
            tokio::select! {
                _ = poll.tick() => {
                    if batch_checkpoints == 0
                        && rx.is_closed()
                        && rx.is_empty()
                        && !has_ready_checkpoint(next_checkpoint, &pending)
                    {
                        info!(pipeline = H::NAME, "Process closed channel and no more data to commit");
                        break;
                    }

                    if pending.len() > WARN_PENDING_WATERMARKS {
                        warn!(
                            pipeline = H::NAME,
                            pending = pending.len(),
                            "Pipeline has a large number of pending watermarks",
                        );
                    }

                    let guard = metrics
                        .collector_gather_latency
                        .with_label_values(&[H::NAME])
                        .start_timer();

                    // Push data into the next batch as long as it's from contiguous checkpoints
                    // and we haven't gathered information from too many checkpoints already.
                    //
                    // We don't worry about overall size because the handler may have optimized
                    // writes by combining rows, but we will limit the number of checkpoints we try
                    // and batch together as a way to impose some limit on the size of the batch
                    // (and therefore the length of the write transaction).
                    // docs::#batch  (see docs/content/guides/developer/advanced/custom-indexer.mdx)
                    while batch_checkpoints < max_batch_checkpoints {
                        let Some(entry) = pending.first_entry() else {
                            break;
                        };

                        match next_checkpoint.cmp(entry.key()) {
                            // Next pending checkpoint is from the future.
                            Ordering::Less => break,

                            // This is the next checkpoint -- include it.
                            Ordering::Equal => {
                                let indexed = entry.remove();
                                batch_rows += indexed.len();
                                batch_checkpoints += 1;
                                handler.batch(&mut batch, indexed.values.into_iter());
                                watermark = Some(indexed.watermark);
                                next_checkpoint += 1;
                            }

                            // Next pending checkpoint is in the past, ignore it to avoid double
                            // writes.
                            Ordering::Greater => {
                                metrics
                                    .total_watermarks_out_of_order
                                    .with_label_values(&[H::NAME])
                                    .inc();

                                let indexed = entry.remove();
                                pending_rows -= indexed.len();
                            }
                        }
                    }
                    // docs::/#batch

                    let elapsed = guard.stop_and_record();
                    debug!(
                        pipeline = H::NAME,
                        elapsed_ms = elapsed * 1000.0,
                        rows = batch_rows,
                        pending = pending_rows,
                        "Gathered batch",
                    );

                    // If there is no new data to commit, we can skip the rest of the process. Note
                    // that we cannot use batch_rows for the check, since it is possible that there
                    // are empty checkpoints with no new rows added, but the watermark still needs
                    // to be updated. Conversely, if there is no watermark to be updated, we know
                    // there is no data to write out.
                    if batch_checkpoints == 0 {
                        assert_eq!(batch_rows, 0);
                        continue;
                    }

                    let Some(watermark) = watermark else {
                        continue;
                    };

                    metrics
                        .collector_batch_size
                        .with_label_values(&[H::NAME])
                        .observe(batch_rows as f64);

                    metrics
                        .total_committer_batches_attempted
                        .with_label_values(&[H::NAME])
                        .inc();

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

                    let guard = metrics
                        .committer_commit_latency
                        .with_label_values(&[H::NAME])
                        .start_timer();

                    let affected = store.transaction(|conn| {
                        async {
                            conn.set_committer_watermark(H::NAME, watermark).await?;
                            handler.commit(&batch, conn).await
                        }.scope_boxed()
                    }).await;


                    let elapsed = guard.stop_and_record();

                    let affected = match affected {
                        Ok(affected) => affected,

                        Err(e) => {
                            warn!(
                                pipeline = H::NAME,
                                elapsed_ms = elapsed * 1000.0,
                                attempt,
                                committed = batch_rows,
                                pending = pending_rows,
                                "Error writing batch: {e}",
                            );

                            metrics
                                .total_committer_batches_failed
                                .with_label_values(&[H::NAME])
                                .inc();

                            attempt += 1;
                            continue;
                        }
                    };

                    debug!(
                        pipeline = H::NAME,
                        attempt,
                        affected,
                        committed = batch_rows,
                        pending = pending_rows,
                        "Wrote batch",
                    );

                    logger.log::<H>(&watermark, elapsed);

                    checkpoint_lag_reporter.report_lag(
                        watermark.checkpoint_hi_inclusive,
                        watermark.timestamp_ms_hi_inclusive
                    );

                    metrics
                        .total_committer_batches_succeeded
                        .with_label_values(&[H::NAME])
                        .inc();

                    metrics
                        .total_committer_rows_committed
                        .with_label_values(&[H::NAME])
                        .inc_by(batch_rows as u64);

                    metrics
                        .total_committer_rows_affected
                        .with_label_values(&[H::NAME])
                        .inc_by(affected as u64);

                    metrics
                        .committer_tx_rows
                        .with_label_values(&[H::NAME])
                        .observe(affected as f64);

                    metrics
                        .watermark_epoch_in_db
                        .with_label_values(&[H::NAME])
                        .set(watermark.epoch_hi_inclusive as i64);

                    metrics
                        .watermark_checkpoint_in_db
                        .with_label_values(&[H::NAME])
                        .set(watermark.checkpoint_hi_inclusive as i64);

                    metrics
                        .watermark_transaction_in_db
                        .with_label_values(&[H::NAME])
                        .set(watermark.tx_hi as i64);

                    metrics
                        .watermark_timestamp_in_db_ms
                        .with_label_values(&[H::NAME])
                        .set(watermark.timestamp_ms_hi_inclusive as i64);

                    let _ = std::mem::take(&mut batch);
                    pending_rows -= batch_rows;
                    batch_checkpoints = 0;
                    batch_rows = 0;
                    attempt = 0;

                    // If we could make more progress immediately, then schedule more work without
                    // waiting.
                    if has_ready_checkpoint(next_checkpoint, &pending) {
                        poll.reset_immediately();
                    }
                }

                Some(indexed) = rx.recv() => {
                    // Although there isn't an explicit collector in the sequential pipeline,
                    // keeping this metric consistent with concurrent pipeline is useful
                    // to monitor the backpressure from committer to processor.
                    metrics
                        .total_collector_rows_received
                        .with_label_values(&[H::NAME])
                        .inc_by(indexed.len() as u64);

                    pending_rows += indexed.len();
                    pending.insert(indexed.checkpoint(), indexed);

                    // Once data has been inserted, check if we need to schedule a write before the
                    // next polling interval. This is appropriate if there are a minimum number of
                    // rows to write, and they are already in the batch, or we can process the next
                    // checkpoint to extract them.
                    if pending_rows < min_eager_rows {
                        continue;
                    }

                    if batch_checkpoints > 0 || has_ready_checkpoint(next_checkpoint, &pending) {
                        poll.reset_immediately();
                    }
                }
            }
        }

        info!(pipeline = H::NAME, "Stopping committer");
        Ok(())
    })
}

// Whether the first entry in `pending` is ready to be consumed by the committer — either to be
// included in the next batch (if it matches `next_checkpoint`) or discarded as a stale duplicate
// (if it predates `next_checkpoint`).
fn has_ready_checkpoint<T>(next_checkpoint: u64, pending: &BTreeMap<u64, T>) -> bool {
    pending
        .first_key_value()
        .is_some_and(|(&first, _)| first <= next_checkpoint)
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::time::Duration;

    use async_trait::async_trait;
    use prometheus::Registry;
    use sui_types::full_checkpoint_content::Checkpoint;
    use tokio::sync::mpsc;

    use crate::mocks::store::MockConnection;
    use crate::mocks::store::MockStore;
    use crate::pipeline::CommitterConfig;
    use crate::pipeline::Processor;

    use super::*;

    // Test implementation of Handler
    #[derive(Default)]
    struct TestHandler;

    #[async_trait]
    impl Processor for TestHandler {
        const NAME: &'static str = "test";
        type Value = u64;

        async fn process(&self, _checkpoint: &Arc<Checkpoint>) -> anyhow::Result<Vec<Self::Value>> {
            Ok(vec![])
        }
    }

    #[async_trait]
    impl super::Handler for TestHandler {
        type Store = MockStore;
        type Batch = Vec<u64>;
        const MAX_BATCH_CHECKPOINTS: usize = 3; // Using small max value for testing.
        const MIN_EAGER_ROWS: usize = 4; // Using small eager value for testing.

        fn batch(&self, batch: &mut Self::Batch, values: std::vec::IntoIter<Self::Value>) {
            batch.extend(values);
        }

        async fn commit<'a>(
            &self,
            batch: &Self::Batch,
            conn: &mut MockConnection<'a>,
        ) -> anyhow::Result<usize> {
            if !batch.is_empty() {
                let mut sequential_data = conn.0.sequential_checkpoint_data.lock().unwrap();
                sequential_data.extend(batch.iter().cloned());
            }
            Ok(batch.len())
        }
    }

    struct TestSetup {
        store: MockStore,
        checkpoint_tx: mpsc::Sender<IndexedCheckpoint<TestHandler>>,
        #[allow(unused)]
        committer: Service,
    }

    /// Emulates adding a sequential pipeline to the indexer. The next_checkpoint is the checkpoint
    /// for the indexer to ingest from.
    fn setup_test(next_checkpoint: u64, config: SequentialConfig, store: MockStore) -> TestSetup {
        let metrics = IndexerMetrics::new(None, &Registry::default());

        let min_eager_rows = config
            .min_eager_rows
            .unwrap_or(<TestHandler as super::Handler>::MIN_EAGER_ROWS);
        let max_batch_checkpoints = config
            .max_batch_checkpoints
            .unwrap_or(<TestHandler as super::Handler>::MAX_BATCH_CHECKPOINTS);

        let (checkpoint_tx, checkpoint_rx) = mpsc::channel(10);

        let store_clone = store.clone();
        let handler = Arc::new(TestHandler);
        let committer = committer(
            handler,
            config,
            next_checkpoint,
            checkpoint_rx,
            store_clone,
            metrics,
            min_eager_rows,
            max_batch_checkpoints,
        );

        TestSetup {
            store,
            checkpoint_tx,
            committer,
        }
    }

    async fn send_checkpoint(setup: &mut TestSetup, checkpoint: u64) {
        setup
            .checkpoint_tx
            .send(create_checkpoint(checkpoint))
            .await
            .unwrap();
    }

    fn create_checkpoint(checkpoint: u64) -> IndexedCheckpoint<TestHandler> {
        IndexedCheckpoint::new(
            checkpoint,        // epoch
            checkpoint,        // checkpoint number
            checkpoint,        // tx_hi
            checkpoint * 1000, // timestamp
            vec![checkpoint],  // values
        )
    }

    #[tokio::test]
    async fn test_committer_processes_sequential_checkpoints() {
        let config = SequentialConfig::default();
        let mut setup = setup_test(0, config, MockStore::default());

        // Send checkpoints in order
        for i in 0..3 {
            send_checkpoint(&mut setup, i).await;
        }

        // Wait for processing
        tokio::time::sleep(Duration::from_millis(200)).await;

        // Verify data was written in order
        assert_eq!(setup.store.get_sequential_data(), vec![0, 1, 2]);

        // Verify watermark was updated
        {
            let watermark = setup.store.watermark(TestHandler::NAME).unwrap();
            assert_eq!(watermark.checkpoint_hi_inclusive, Some(2));
            assert_eq!(watermark.tx_hi, 2);
        }

        // Verify commit_hi was sent to ingestion
    }

    /// Configure the MockStore with no watermark, and emulate `first_checkpoint` by passing the
    /// `initial_watermark` into the setup.
    #[tokio::test]
    async fn test_committer_processes_sequential_checkpoints_with_initial_watermark() {
        let config = SequentialConfig::default();
        let mut setup = setup_test(5, config, MockStore::default());

        // Verify watermark hasn't progressed
        let watermark = setup.store.watermark(TestHandler::NAME);
        assert!(watermark.is_none());

        // Send checkpoints in order
        for i in 0..5 {
            send_checkpoint(&mut setup, i).await;
        }

        // Wait for processing
        tokio::time::sleep(Duration::from_millis(1000)).await;

        // Verify watermark hasn't progressed
        let watermark = setup.store.watermark(TestHandler::NAME);
        assert!(watermark.is_none());

        for i in 5..8 {
            send_checkpoint(&mut setup, i).await;
        }

        // Wait for processing
        tokio::time::sleep(Duration::from_millis(1000)).await;

        // Verify data was written in order
        assert_eq!(setup.store.get_sequential_data(), vec![5, 6, 7]);

        // Verify watermark was updated
        {
            let watermark = setup.store.watermark(TestHandler::NAME).unwrap();
            assert_eq!(watermark.checkpoint_hi_inclusive, Some(7));
            assert_eq!(watermark.tx_hi, 7);
        }
    }

    #[tokio::test]
    async fn test_committer_processes_out_of_order_checkpoints() {
        let config = SequentialConfig::default();
        let mut setup = setup_test(0, config, MockStore::default());

        // Send checkpoints out of order
        for i in [1, 0, 2] {
            send_checkpoint(&mut setup, i).await;
        }

        // Wait for processing
        tokio::time::sleep(Duration::from_millis(200)).await;

        // Verify data was written in order despite receiving out of order
        assert_eq!(setup.store.get_sequential_data(), vec![0, 1, 2]);

        // Verify watermark was updated
        {
            let watermark = setup.store.watermark(TestHandler::NAME).unwrap();
            assert_eq!(watermark.checkpoint_hi_inclusive, Some(2));
            assert_eq!(watermark.tx_hi, 2);
        }

        // Verify commit_hi was sent to ingestion
    }

    #[tokio::test]
    async fn test_committer_commit_up_to_max_batch_checkpoints() {
        let config = SequentialConfig::default();
        let mut setup = setup_test(0, config, MockStore::default());

        // Send checkpoints up to MAX_BATCH_CHECKPOINTS
        for i in 0..4 {
            send_checkpoint(&mut setup, i).await;
        }

        // Wait for processing
        tokio::time::sleep(Duration::from_millis(200)).await;

        // Verify data is written in order across batches
        assert_eq!(setup.store.get_sequential_data(), vec![0, 1, 2, 3]);
    }

    #[tokio::test]
    async fn test_committer_commits_eagerly() {
        let config = SequentialConfig {
            committer: CommitterConfig {
                collect_interval_ms: 4_000, // Long polling to test eager commit
                ..Default::default()
            },
            ..Default::default()
        };
        let mut setup = setup_test(0, config, MockStore::default());

        // Wait for initial poll to be over
        tokio::time::sleep(Duration::from_millis(200)).await;

        // Send checkpoints 0-2
        for i in 0..3 {
            send_checkpoint(&mut setup, i).await;
        }

        // Verify no checkpoints are written yet (not enough rows for eager commit)
        assert_eq!(setup.store.get_sequential_data(), Vec::<u64>::new());

        // Send checkpoint 3 to trigger the eager commit (3 + 1 >= MIN_EAGER_ROWS)
        send_checkpoint(&mut setup, 3).await;

        // Wait for processing
        tokio::time::sleep(Duration::from_millis(200)).await;

        // Verify all checkpoints are written
        assert_eq!(setup.store.get_sequential_data(), vec![0, 1, 2, 3]);
    }

    #[tokio::test]
    async fn test_committer_retries_on_transaction_failure() {
        let config = SequentialConfig {
            committer: CommitterConfig {
                collect_interval_ms: 1_000, // Long polling to test retry logic
                ..Default::default()
            },
            ..Default::default()
        };

        // Create store with transaction failure configuration
        let store = MockStore::default().with_transaction_failures(1); // Will fail once before succeeding

        let mut setup = setup_test(10, config, store);

        // Send a checkpoint
        send_checkpoint(&mut setup, 10).await;

        // Wait for initial poll to be over
        tokio::time::sleep(Duration::from_millis(200)).await;

        // Verify no data is written before retries complete
        assert_eq!(setup.store.get_sequential_data(), Vec::<u64>::new());

        // Wait for retries to complete
        tokio::time::sleep(Duration::from_millis(1_200)).await;

        // Verify data is written after retries complete on next polling
        assert_eq!(setup.store.get_sequential_data(), vec![10]);
    }
}
