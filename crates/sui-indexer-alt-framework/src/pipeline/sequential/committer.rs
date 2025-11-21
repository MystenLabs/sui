// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{cmp::Ordering, collections::BTreeMap, sync::Arc};

use scoped_futures::ScopedFutureExt;
use tokio::{
    sync::mpsc,
    task::JoinHandle,
    time::{MissedTickBehavior, interval},
};
use tokio_util::sync::CancellationToken;
use tracing::{debug, info, warn};

use crate::{
    metrics::{CheckpointLagMetricReporter, IndexerMetrics},
    pipeline::{IndexedCheckpoint, WARN_PENDING_WATERMARKS, logging::WatermarkLogger},
    store::{Connection, TransactionalStore},
};

use super::{Handler, SequentialConfig};

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
///
/// The committer can be configured to lag behind the ingestion service by a fixed number of
/// checkpoints (configured by `checkpoint_lag`). A value of `0` means no lag.
///
/// Upon successful write, the task sends its new watermark back to the ingestion service, to
/// unblock its regulator.
///
/// The task can be shutdown using its `cancel` token or if either of its channels are closed.
pub(super) fn committer<H>(
    handler: Arc<H>,
    config: SequentialConfig,
    mut next_checkpoint: u64,
    mut rx: mpsc::Receiver<IndexedCheckpoint<H>>,
    tx: mpsc::UnboundedSender<(&'static str, u64)>,
    store: H::Store,
    metrics: Arc<IndexerMetrics>,
    cancel: CancellationToken,
) -> JoinHandle<()>
where
    H: Handler + Send + Sync + 'static,
    H::Store: TransactionalStore + 'static,
{
    tokio::spawn(async move {
        // The `poll` interval controls the maximum time to wait between commits, regardless of the
        // amount of data available.
        let mut poll = interval(config.committer.collect_interval());
        poll.set_missed_tick_behavior(MissedTickBehavior::Delay);

        let checkpoint_lag = config.checkpoint_lag;

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
                _ = cancel.cancelled() => {
                    info!(pipeline = H::NAME, "Shutdown received");
                    break;
                }

                _ = poll.tick() => {
                    if batch_checkpoints == 0
                        && rx.is_closed()
                        && rx.is_empty()
                        && !can_process_pending(next_checkpoint, checkpoint_lag, &pending)
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

                    // Push data into the next batch as long as it's from contiguous checkpoints,
                    // outside of the checkpoint lag and we haven't gathered information from too
                    // many checkpoints already.
                    //
                    // We don't worry about overall size because the handler may have optimized
                    // writes by combining rows, but we will limit the number of checkpoints we try
                    // and batch together as a way to impose some limit on the size of the batch
                    // (and therefore the length of the write transaction).
                    // docs::#batch  (see docs/content/guides/developer/advanced/custom-indexer.mdx)
                    while batch_checkpoints < H::MAX_BATCH_CHECKPOINTS {
                        if !can_process_pending(next_checkpoint, checkpoint_lag, &pending) {
                            break;
                        }

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

                    // docs::#send (see docs/content/guides/developer/advanced/custom-indexer.mdx)
                    // Ignore the result -- the ingestion service will close this channel
                    // once it is done, but there may still be checkpoints buffered that need
                    // processing.
                    let _ = tx.send((H::NAME, watermark.checkpoint_hi_inclusive + 1));
                    // docs::/#send

                    let _ = std::mem::take(&mut batch);
                    pending_rows -= batch_rows;
                    batch_checkpoints = 0;
                    batch_rows = 0;
                    attempt = 0;

                    // If we could make more progress immediately, then schedule more work without
                    // waiting.
                    if can_process_pending(next_checkpoint, checkpoint_lag, &pending) {
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
                    if pending_rows < H::MIN_EAGER_ROWS {
                        continue;
                    }

                    if batch_checkpoints > 0
                        || can_process_pending(next_checkpoint, checkpoint_lag, &pending)
                    {
                        poll.reset_immediately();
                    }
                }
            }
        }

        info!(pipeline = H::NAME, "Stopping committer");
    })
}

// Tests whether the first checkpoint in the `pending` buffer can be processed immediately, which
// is subject to the following conditions:
//
// - It is at or before the `next_checkpoint` expected by the committer.
// - It is at least `checkpoint_lag` checkpoints before the last checkpoint in the buffer.
fn can_process_pending<T>(
    next_checkpoint: u64,
    checkpoint_lag: u64,
    pending: &BTreeMap<u64, T>,
) -> bool {
    let Some((&first, _)) = pending.first_key_value() else {
        return false;
    };

    let Some((&last, _)) = pending.last_key_value() else {
        return false;
    };

    first <= next_checkpoint && first + checkpoint_lag <= last
}

#[cfg(test)]
mod tests {
    use crate::{
        mocks::store::{MockConnection, MockStore},
        pipeline::{CommitterConfig, Processor},
    };

    use super::*;
    use async_trait::async_trait;
    use prometheus::Registry;
    use std::{sync::Arc, time::Duration};
    use sui_types::full_checkpoint_content::Checkpoint;
    use tokio::sync::mpsc;
    use tokio_util::sync::CancellationToken;

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
        commit_hi_rx: mpsc::UnboundedReceiver<(&'static str, u64)>,
        committer_handle: JoinHandle<()>,
    }

    /// Emulates adding a sequential pipeline to the indexer. The next_checkpoint is the checkpoint
    /// for the indexer to ingest from.
    fn setup_test(next_checkpoint: u64, config: SequentialConfig, store: MockStore) -> TestSetup {
        let metrics = IndexerMetrics::new(None, &Registry::default());
        let cancel = CancellationToken::new();

        let (checkpoint_tx, checkpoint_rx) = mpsc::channel(10);
        #[allow(clippy::disallowed_methods)]
        let (commit_hi_tx, commit_hi_rx) = mpsc::unbounded_channel();

        let store_clone = store.clone();
        let handler = Arc::new(TestHandler);
        let committer_handle = committer(
            handler,
            config,
            next_checkpoint,
            checkpoint_rx,
            commit_hi_tx,
            store_clone,
            metrics,
            cancel,
        );

        TestSetup {
            store,
            checkpoint_tx,
            commit_hi_rx,
            committer_handle,
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
        let config = SequentialConfig {
            committer: CommitterConfig::default(),
            checkpoint_lag: 0, // Zero checkpoint lag to process new batch instantly
        };
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
            assert_eq!(watermark.checkpoint_hi_inclusive, 2);
            assert_eq!(watermark.tx_hi, 2);
        }

        // Verify commit_hi was sent to ingestion
        let commit_hi = setup.commit_hi_rx.recv().await.unwrap();
        assert_eq!(commit_hi.0, "test", "Pipeline name should be 'test'");
        assert_eq!(commit_hi.1, 3, "commit_hi should be 3 (checkpoint 2 + 1)");

        // Clean up
        drop(setup.checkpoint_tx);
        let _ = setup.committer_handle.await;
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
            assert_eq!(watermark.checkpoint_hi_inclusive, 7);
            assert_eq!(watermark.tx_hi, 7);
        }

        // Clean up
        drop(setup.checkpoint_tx);
        let _ = setup.committer_handle.await;
    }

    #[tokio::test]
    async fn test_committer_processes_out_of_order_checkpoints() {
        let config = SequentialConfig {
            committer: CommitterConfig::default(),
            checkpoint_lag: 0, // Zero checkpoint lag to process new batch instantly
        };
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
            assert_eq!(watermark.checkpoint_hi_inclusive, 2);
            assert_eq!(watermark.tx_hi, 2);
        }

        // Verify commit_hi was sent to ingestion
        let commit_hi = setup.commit_hi_rx.recv().await.unwrap();
        assert_eq!(commit_hi.0, "test", "Pipeline name should be 'test'");
        assert_eq!(commit_hi.1, 3, "commit_hi should be 3 (checkpoint 2 + 1)");

        // Clean up
        drop(setup.checkpoint_tx);
        let _ = setup.committer_handle.await;
    }

    #[tokio::test]
    async fn test_committer_commit_up_to_max_batch_checkpoints() {
        let config = SequentialConfig {
            committer: CommitterConfig::default(),
            checkpoint_lag: 0, // Zero checkpoint lag to process new batch instantly
        };
        let mut setup = setup_test(0, config, MockStore::default());

        // Send checkpoints up to MAX_BATCH_CHECKPOINTS
        for i in 0..4 {
            send_checkpoint(&mut setup, i).await;
        }

        // Wait for processing
        tokio::time::sleep(Duration::from_millis(200)).await;

        // Verify commit_hi values are sent for each batch
        let commit_hi1 = setup.commit_hi_rx.recv().await.unwrap();
        assert_eq!(
            commit_hi1.1, 3,
            "First commit_hi should be 3 (checkpoint 2 + 1, highest processed of first batch)"
        );

        let commit_hi2 = setup.commit_hi_rx.recv().await.unwrap();
        assert_eq!(
            commit_hi2.1, 4,
            "Second commit_hi should be 4 (checkpoint 3 + 1, highest processed of second batch)"
        );

        // Verify data is written in order across batches
        assert_eq!(setup.store.get_sequential_data(), vec![0, 1, 2, 3]);

        // Clean up
        drop(setup.checkpoint_tx);
        let _ = setup.committer_handle.await;
    }

    #[tokio::test]
    async fn test_committer_does_not_commit_until_checkpoint_lag() {
        let config = SequentialConfig {
            committer: CommitterConfig::default(),
            checkpoint_lag: 1, // Only commit checkpoints that are at least 1 behind
        };
        let mut setup = setup_test(0, config, MockStore::default());

        // Send checkpoints 0-2
        for i in 0..3 {
            send_checkpoint(&mut setup, i).await;
        }

        // Wait for processing
        tokio::time::sleep(Duration::from_millis(200)).await;

        // Verify only checkpoints 0 and 1 are written (since checkpoint 2 is not lagged enough)
        assert_eq!(setup.store.get_sequential_data(), vec![0, 1]);
        let commit_hi = setup.commit_hi_rx.recv().await.unwrap();
        assert_eq!(commit_hi.1, 2, "commit_hi should be 2 (checkpoint 1 + 1)");

        // Send checkpoint 3 to exceed the checkpoint_lag for checkpoint 2
        send_checkpoint(&mut setup, 3).await;

        // Wait for next polling processing
        tokio::time::sleep(Duration::from_millis(1000)).await;

        // Verify checkpoint 2 is now written
        assert_eq!(setup.store.get_sequential_data(), vec![0, 1, 2]);
        let commit_hi = setup.commit_hi_rx.recv().await.unwrap();
        assert_eq!(commit_hi.1, 3, "commit_hi should be 3 (checkpoint 2 + 1)");

        // Clean up
        drop(setup.checkpoint_tx);
        let _ = setup.committer_handle.await;
    }

    #[tokio::test]
    async fn test_committer_commits_eagerly() {
        let config = SequentialConfig {
            committer: CommitterConfig {
                collect_interval_ms: 4_000, // Long polling to test eager commit
                ..Default::default()
            },
            checkpoint_lag: 0, // Zero checkpoint lag to not block the eager logic
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

        // Clean up
        drop(setup.checkpoint_tx);
        let _ = setup.committer_handle.await;
    }

    #[tokio::test]
    async fn test_committer_cannot_commit_eagerly_due_to_checkpoint_lag() {
        let config = SequentialConfig {
            committer: CommitterConfig {
                collect_interval_ms: 4_000, // Long polling to test eager commit
                ..Default::default()
            },
            checkpoint_lag: 4, // High checkpoint lag to block eager commits
        };
        let mut setup = setup_test(0, config, MockStore::default());

        // Wait for initial poll to be over
        tokio::time::sleep(Duration::from_millis(200)).await;

        // Send checkpoints 0-3
        for i in 0..4 {
            send_checkpoint(&mut setup, i).await;
        }

        // Wait for processing
        tokio::time::sleep(Duration::from_millis(200)).await;

        // Verify no checkpoints are written due to checkpoint lag
        assert_eq!(setup.store.get_sequential_data(), Vec::<u64>::new());

        // Send checkpoint 4 to exceed checkpoint lag
        send_checkpoint(&mut setup, 4).await;

        // Wait for processing
        tokio::time::sleep(Duration::from_millis(200)).await;

        // Verify only checkpoint 0 is written (since it's the only one that satisfies checkpoint_lag)
        assert_eq!(setup.store.get_sequential_data(), vec![0]);

        // Clean up
        drop(setup.checkpoint_tx);
        let _ = setup.committer_handle.await;
    }

    #[tokio::test]
    async fn test_committer_retries_on_transaction_failure() {
        let config = SequentialConfig {
            committer: CommitterConfig {
                collect_interval_ms: 1_000, // Long polling to test retry logic
                ..Default::default()
            },
            checkpoint_lag: 0,
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

        // Verify commit_hi is updated
        let commit_hi = setup.commit_hi_rx.recv().await.unwrap();
        assert_eq!(commit_hi.0, "test", "Pipeline name should be 'test'");
        assert_eq!(
            commit_hi.1, 11,
            "commit_hi should be 11 (checkpoint 10 + 1)"
        );

        // Clean up
        drop(setup.checkpoint_tx);
        let _ = setup.committer_handle.await;
    }
}
