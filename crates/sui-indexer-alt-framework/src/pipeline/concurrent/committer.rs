// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{sync::Arc, time::Duration};

use backoff::ExponentialBackoff;
use tokio::{sync::mpsc, task::JoinHandle};
use tokio_stream::wrappers::ReceiverStream;
use tokio_util::sync::CancellationToken;
use tracing::{debug, error, info, warn};

use crate::{
    metrics::{CheckpointLagMetricReporter, IndexerMetrics},
    pipeline::{Break, CommitterConfig, WatermarkPart},
    store::Store,
    task::TrySpawnStreamExt,
};

use super::{BatchedRows, Handler};

/// If the committer needs to retry a commit, it will wait this long initially.
const INITIAL_RETRY_INTERVAL: Duration = Duration::from_millis(100);

/// If the committer needs to retry a commit, it will wait at most this long between retries.
const MAX_RETRY_INTERVAL: Duration = Duration::from_secs(1);

/// The committer task is responsible for writing batches of rows to the database. It receives
/// batches on `rx` and writes them out to the `db` concurrently (`config.write_concurrency`
/// controls the degree of fan-out).
///
/// The writing of each batch will be repeatedly retried on an exponential back-off until it
/// succeeds. Once the write succeeds, the [WatermarkPart]s for that batch are sent on `tx` to the
/// watermark task, as long as `skip_watermark` is not true.
///
/// This task will shutdown via its `cancel`lation token, or if its receiver or sender channels are
/// closed.
pub(super) fn committer<H: Handler + 'static>(
    config: CommitterConfig,
    skip_watermark: bool,
    rx: mpsc::Receiver<BatchedRows<H>>,
    tx: mpsc::Sender<Vec<WatermarkPart>>,
    db: H::Store,
    metrics: Arc<IndexerMetrics>,
    cancel: CancellationToken,
) -> JoinHandle<()> {
    tokio::spawn(async move {
        info!(pipeline = H::NAME, "Starting committer");
        let checkpoint_lag_reporter = CheckpointLagMetricReporter::new_for_pipeline::<H>(
            &metrics.partially_committed_checkpoint_timestamp_lag,
            &metrics.latest_partially_committed_checkpoint_timestamp_lag_ms,
            &metrics.latest_partially_committed_checkpoint,
        );

        match ReceiverStream::new(rx)
            .try_for_each_spawned(
                config.write_concurrency,
                |BatchedRows { values, watermark }| {
                    let values = Arc::new(values);
                    let tx = tx.clone();
                    let db = db.clone();
                    let metrics = metrics.clone();
                    let cancel = cancel.clone();
                    let checkpoint_lag_reporter = checkpoint_lag_reporter.clone();

                    // Repeatedly try to get a connection to the DB and write the batch. Use an
                    // exponential backoff in case the failure is due to contention over the DB
                    // connection pool.
                    let backoff = ExponentialBackoff {
                        initial_interval: INITIAL_RETRY_INTERVAL,
                        current_interval: INITIAL_RETRY_INTERVAL,
                        max_interval: MAX_RETRY_INTERVAL,
                        max_elapsed_time: None,
                        ..Default::default()
                    };

                    let highest_checkpoint = watermark.iter().map(|w| w.checkpoint()).max();
                    let highest_checkpoint_timestamp =
                        watermark.iter().map(|w| w.timestamp_ms()).max();

                    use backoff::Error as BE;
                    let commit = move || {
                        let values = values.clone();
                        let db = db.clone();
                        let metrics = metrics.clone();
                        let checkpoint_lag_reporter = checkpoint_lag_reporter.clone();
                        async move {
                            if values.is_empty() {
                                return Ok(());
                            }

                            metrics
                                .total_committer_batches_attempted
                                .with_label_values(&[H::NAME])
                                .inc();

                            let guard = metrics
                                .committer_commit_latency
                                .with_label_values(&[H::NAME])
                                .start_timer();

                            let mut conn = db.connect().await.map_err(|e| {
                                warn!(
                                    pipeline = H::NAME,
                                    "Committed failed to get connection for DB"
                                );

                                metrics
                                    .total_committer_batches_failed
                                    .with_label_values(&[H::NAME])
                                    .inc();

                                BE::transient(Break::Err(e))
                            })?;

                            let affected = H::commit(values.as_slice(), &mut conn).await;
                            let elapsed = guard.stop_and_record();

                            match affected {
                                Ok(affected) => {
                                    debug!(
                                        pipeline = H::NAME,
                                        elapsed_ms = elapsed * 1000.0,
                                        affected,
                                        committed = values.len(),
                                        "Wrote batch",
                                    );

                                    checkpoint_lag_reporter.report_lag(
                                        // unwrap is safe because we would have returned if values is empty.
                                        highest_checkpoint.unwrap(),
                                        highest_checkpoint_timestamp.unwrap(),
                                    );

                                    metrics
                                        .total_committer_batches_succeeded
                                        .with_label_values(&[H::NAME])
                                        .inc();

                                    metrics
                                        .total_committer_rows_committed
                                        .with_label_values(&[H::NAME])
                                        .inc_by(values.len() as u64);

                                    metrics
                                        .total_committer_rows_affected
                                        .with_label_values(&[H::NAME])
                                        .inc_by(affected as u64);

                                    metrics
                                        .committer_tx_rows
                                        .with_label_values(&[H::NAME])
                                        .observe(affected as f64);

                                    Ok(())
                                }

                                Err(e) => {
                                    warn!(
                                        pipeline = H::NAME,
                                        elapsed_ms = elapsed * 1000.0,
                                        committed = values.len(),
                                        "Error writing batch: {e}",
                                    );

                                    metrics
                                        .total_committer_batches_failed
                                        .with_label_values(&[H::NAME])
                                        .inc();

                                    Err(BE::transient(Break::Err(e)))
                                }
                            }
                        }
                    };

                    async move {
                        tokio::select! {
                            _ = cancel.cancelled() => {
                                return Err(Break::Cancel);
                            }

                            // Double check that the commit actually went through, (this backoff should
                            // not produce any permanent errors, but if it does, we need to shutdown
                            // the pipeline).
                            commit = backoff::future::retry(backoff, commit) => {
                                let () = commit?;
                            }
                        };

                        if !skip_watermark && tx.send(watermark).await.is_err() {
                            info!(pipeline = H::NAME, "Watermark closed channel");
                            return Err(Break::Cancel);
                        }

                        Ok(())
                    }
                },
            )
            .await
        {
            Ok(()) => {
                info!(pipeline = H::NAME, "Batches done, stopping committer");
            }

            Err(Break::Cancel) => {
                info!(pipeline = H::NAME, "Shutdown received, stopping committer");
            }

            Err(Break::Err(e)) => {
                error!(pipeline = H::NAME, "Error from committer: {e}");
                cancel.cancel();
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use std::sync::{
        Arc, Mutex,
        atomic::{AtomicUsize, Ordering},
    };

    use anyhow::ensure;
    use async_trait::async_trait;
    use sui_types::full_checkpoint_content::CheckpointData;
    use tokio::sync::mpsc;
    use tokio_util::sync::CancellationToken;

    use crate::{
        FieldCount,
        metrics::IndexerMetrics,
        mocks::store::*,
        pipeline::{
            Processor, WatermarkPart,
            concurrent::{BatchedRows, Handler},
        },
        store::CommitterWatermark,
    };

    use super::*;

    #[derive(Clone, FieldCount, Default)]
    pub struct StoredData {
        pub cp_sequence_number: u64,
        pub tx_sequence_numbers: Vec<u64>,
        /// Tracks remaining commit failures for testing retry logic.
        /// The committer spawns concurrent tasks that call H::commit,
        /// so this needs to be thread-safe (hence Arc<AtomicUsize>).
        pub commit_failure_remaining: Arc<AtomicUsize>,
        pub commit_delay_ms: u64,
    }

    pub struct DataPipeline;

    #[async_trait]
    impl Processor for DataPipeline {
        const NAME: &'static str = "data";

        type Value = StoredData;

        async fn process(
            &self,
            _checkpoint: &Arc<CheckpointData>,
        ) -> anyhow::Result<Vec<Self::Value>> {
            Ok(vec![])
        }
    }

    #[async_trait]
    impl Handler for DataPipeline {
        type Store = MockStore;

        async fn commit<'a>(
            values: &[StoredData],
            conn: &mut MockConnection<'a>,
        ) -> anyhow::Result<usize> {
            for value in values {
                // If there's a delay, sleep for that duration
                if value.commit_delay_ms > 0 {
                    tokio::time::sleep(Duration::from_millis(value.commit_delay_ms)).await;
                }

                // If there are remaining failures, fail the commit and decrement the counter
                {
                    let remaining = value
                        .commit_failure_remaining
                        .fetch_sub(1, Ordering::Relaxed);
                    ensure!(
                        remaining == 0,
                        "Commit failed, remaining failures: {}",
                        remaining - 1
                    );
                }

                conn.0
                    .commit_data(
                        DataPipeline::NAME,
                        value.cp_sequence_number,
                        value.tx_sequence_numbers.clone(),
                    )
                    .await?;
            }
            Ok(values.len())
        }
    }

    struct TestSetup {
        store: MockStore,
        batch_tx: mpsc::Sender<BatchedRows<DataPipeline>>,
        watermark_rx: mpsc::Receiver<Vec<WatermarkPart>>,
        committer_handle: JoinHandle<()>,
    }

    /// Creates and spawns a committer task with the provided mock store, along with
    /// all necessary channels and configuration. The committer runs in the background
    /// and can be interacted with through the returned channels.
    ///
    /// # Arguments
    /// * `store` - The mock store to use for testing
    /// * `skip_watermark` - Whether to skip sending watermarks to the watermark channel
    async fn setup_test(store: MockStore, skip_watermark: bool) -> TestSetup {
        let config = CommitterConfig::default();
        let metrics = IndexerMetrics::new(None, &Default::default());
        let cancel = CancellationToken::new();

        let (batch_tx, batch_rx) = mpsc::channel::<BatchedRows<DataPipeline>>(10);
        let (watermark_tx, watermark_rx) = mpsc::channel(10);

        let store_clone = store.clone();
        let committer_handle = tokio::spawn(async move {
            let _ = committer(
                config,
                skip_watermark,
                batch_rx,
                watermark_tx,
                store_clone,
                metrics,
                cancel,
            )
            .await;
        });

        TestSetup {
            store,
            batch_tx,
            watermark_rx,
            committer_handle,
        }
    }

    #[tokio::test]
    async fn test_concurrent_batch_processing() {
        let mut setup = setup_test(MockStore::default(), false).await;

        // Send batches
        let batch1 = BatchedRows {
            values: vec![
                StoredData {
                    cp_sequence_number: 1,
                    tx_sequence_numbers: vec![1, 2, 3],
                    ..Default::default()
                },
                StoredData {
                    cp_sequence_number: 2,
                    tx_sequence_numbers: vec![4, 5, 6],
                    ..Default::default()
                },
            ],
            watermark: vec![
                WatermarkPart {
                    watermark: CommitterWatermark {
                        epoch_hi_inclusive: 0,
                        checkpoint_hi_inclusive: 1,
                        tx_hi: 3,
                        timestamp_ms_hi_inclusive: 1000,
                    },
                    batch_rows: 1,
                    total_rows: 1, // Total rows from checkpoint 1
                },
                WatermarkPart {
                    watermark: CommitterWatermark {
                        epoch_hi_inclusive: 0,
                        checkpoint_hi_inclusive: 2,
                        tx_hi: 6,
                        timestamp_ms_hi_inclusive: 2000,
                    },
                    batch_rows: 1,
                    total_rows: 1, // Total rows from checkpoint 2
                },
            ],
        };

        let batch2 = BatchedRows {
            values: vec![StoredData {
                cp_sequence_number: 3,
                tx_sequence_numbers: vec![7, 8, 9],
                ..Default::default()
            }],
            watermark: vec![WatermarkPart {
                watermark: CommitterWatermark {
                    epoch_hi_inclusive: 0,
                    checkpoint_hi_inclusive: 3,
                    tx_hi: 9,
                    timestamp_ms_hi_inclusive: 3000,
                },
                batch_rows: 1,
                total_rows: 1, // Total rows from checkpoint 3
            }],
        };

        setup.batch_tx.send(batch1).await.unwrap();
        setup.batch_tx.send(batch2).await.unwrap();

        // Verify watermarks. Blocking until committer has processed.
        let watermark1 = setup.watermark_rx.recv().await.unwrap();
        let watermark2 = setup.watermark_rx.recv().await.unwrap();
        assert_eq!(watermark1.len(), 2);
        assert_eq!(watermark2.len(), 1);

        // Verify data was committed
        {
            let data = setup.store.data.get(DataPipeline::NAME).unwrap();
            assert_eq!(data.len(), 3);
            assert_eq!(data.get(&1).unwrap().value(), &vec![1, 2, 3]);
            assert_eq!(data.get(&2).unwrap().value(), &vec![4, 5, 6]);
            assert_eq!(data.get(&3).unwrap().value(), &vec![7, 8, 9]);
        }

        // Clean up
        drop(setup.batch_tx);
        let _ = setup.committer_handle.await;
    }

    #[tokio::test]
    async fn test_commit_with_retries_for_commit_failure() {
        let mut setup = setup_test(MockStore::default(), false).await;

        // Create a batch with a single item that will fail once before succeeding
        let batch = BatchedRows {
            values: vec![StoredData {
                cp_sequence_number: 1,
                tx_sequence_numbers: vec![1, 2, 3],
                commit_failure_remaining: Arc::new(AtomicUsize::new(1)),
                commit_delay_ms: 1_000, // Long commit delay for testing state between retry
            }],
            watermark: vec![WatermarkPart {
                watermark: CommitterWatermark {
                    epoch_hi_inclusive: 0,
                    checkpoint_hi_inclusive: 1,
                    tx_hi: 3,
                    timestamp_ms_hi_inclusive: 1000,
                },
                batch_rows: 1,
                total_rows: 1,
            }],
        };

        // Send the batch
        setup.batch_tx.send(batch).await.unwrap();

        // Wait for the first attempt to fail and before the retry succeeds
        tokio::time::sleep(Duration::from_millis(1_500)).await;

        // Verify state before retry succeeds
        {
            let data = setup.store.data.get(DataPipeline::NAME);
            assert!(
                data.is_none(),
                "Data should not be committed before retry succeeds"
            );
        }
        assert!(
            setup.watermark_rx.try_recv().is_err(),
            "No watermark should be received before retry succeeds"
        );

        // Wait for the retry to succeed
        tokio::time::sleep(Duration::from_millis(1_500)).await;

        // Verify state after retry succeeds
        {
            let data = setup.store.data.get(DataPipeline::NAME).unwrap();

            assert_eq!(data.get(&1).unwrap().value(), &vec![1, 2, 3]);
        }
        let watermark = setup.watermark_rx.recv().await.unwrap();
        assert_eq!(watermark.len(), 1);

        // Clean up
        drop(setup.batch_tx);
        let _ = setup.committer_handle.await;
    }

    #[tokio::test]
    async fn test_commit_with_retries_for_connection_failure() {
        // Create a batch with a single item
        let store = MockStore {
            connection_failure: Arc::new(Mutex::new(ConnectionFailure {
                connection_failure_attempts: 1,
                connection_delay_ms: 1_000, // Long connection delay for testing state between retry
                ..Default::default()
            })),
            ..Default::default()
        };
        let mut setup = setup_test(store, false).await;

        let batch = BatchedRows {
            values: vec![StoredData {
                cp_sequence_number: 1,
                tx_sequence_numbers: vec![1, 2, 3],
                ..Default::default()
            }],
            watermark: vec![WatermarkPart {
                watermark: CommitterWatermark {
                    epoch_hi_inclusive: 0,
                    checkpoint_hi_inclusive: 1,
                    tx_hi: 3,
                    timestamp_ms_hi_inclusive: 1000,
                },
                batch_rows: 1,
                total_rows: 1,
            }],
        };

        // Send the batch
        setup.batch_tx.send(batch).await.unwrap();

        // Wait for the first attempt to fail and before the retry succeeds
        tokio::time::sleep(Duration::from_millis(1_500)).await;

        // Verify state before retry succeeds
        {
            let data = setup.store.data.get(DataPipeline::NAME);
            assert!(
                data.is_none(),
                "Data should not be committed before retry succeeds"
            );
        }
        assert!(
            setup.watermark_rx.try_recv().is_err(),
            "No watermark should be received before retry succeeds"
        );

        // Wait for the retry to succeed
        tokio::time::sleep(Duration::from_millis(1_500)).await;

        // Verify state after retry succeeds
        {
            let data = setup.store.data.get(DataPipeline::NAME).unwrap();
            assert_eq!(data.get(&1).unwrap().value(), &vec![1, 2, 3]);
        }
        let watermark = setup.watermark_rx.recv().await.unwrap();
        assert_eq!(watermark.len(), 1);

        // Clean up
        drop(setup.batch_tx);
        let _ = setup.committer_handle.await;
    }

    #[tokio::test]
    async fn test_empty_batch_handling() {
        let mut setup = setup_test(MockStore::default(), false).await;

        let empty_batch = BatchedRows {
            values: vec![], // Empty values
            watermark: vec![WatermarkPart {
                watermark: CommitterWatermark {
                    epoch_hi_inclusive: 0,
                    checkpoint_hi_inclusive: 1,
                    tx_hi: 0,
                    timestamp_ms_hi_inclusive: 1000,
                },
                batch_rows: 0,
                total_rows: 0,
            }],
        };

        // Send the empty batch
        setup.batch_tx.send(empty_batch).await.unwrap();

        // Verify watermark is still sent (empty batches should still produce watermarks)
        let watermark = setup.watermark_rx.recv().await.unwrap();
        assert_eq!(watermark.len(), 1);
        assert_eq!(watermark[0].batch_rows, 0);
        assert_eq!(watermark[0].total_rows, 0);

        // Verify no data was committed (since batch was empty)
        {
            let data = setup.store.data.get(DataPipeline::NAME);
            assert!(
                data.is_none(),
                "No data should be committed for empty batch"
            );
        }

        // Clean up
        drop(setup.batch_tx);
        let _ = setup.committer_handle.await;
    }

    #[tokio::test]
    async fn test_skip_watermark_mode() {
        let mut setup = setup_test(MockStore::default(), true).await;

        let batch = BatchedRows {
            values: vec![StoredData {
                cp_sequence_number: 1,
                tx_sequence_numbers: vec![1, 2, 3],
                ..Default::default()
            }],
            watermark: vec![WatermarkPart {
                watermark: CommitterWatermark {
                    epoch_hi_inclusive: 0,
                    checkpoint_hi_inclusive: 1,
                    tx_hi: 3,
                    timestamp_ms_hi_inclusive: 1000,
                },
                batch_rows: 1,
                total_rows: 1,
            }],
        };

        // Send the batch
        setup.batch_tx.send(batch).await.unwrap();

        // Wait for processing
        tokio::time::sleep(Duration::from_millis(200)).await;

        // Verify data was committed
        {
            let data = setup.store.data.get(DataPipeline::NAME).unwrap();
            assert_eq!(data.get(&1).unwrap().value(), &vec![1, 2, 3]);
        }

        // Verify no watermark was sent (skip_watermark mode)
        assert!(
            setup.watermark_rx.try_recv().is_err(),
            "No watermark should be sent in skip_watermark mode"
        );

        // Clean up
        drop(setup.batch_tx);
        let _ = setup.committer_handle.await;
    }

    #[tokio::test]
    async fn test_watermark_channel_closed() {
        let setup = setup_test(MockStore::default(), false).await;

        let batch = BatchedRows {
            values: vec![StoredData {
                cp_sequence_number: 1,
                tx_sequence_numbers: vec![1, 2, 3],
                ..Default::default()
            }],
            watermark: vec![WatermarkPart {
                watermark: CommitterWatermark {
                    epoch_hi_inclusive: 0,
                    checkpoint_hi_inclusive: 1,
                    tx_hi: 3,
                    timestamp_ms_hi_inclusive: 1000,
                },
                batch_rows: 1,
                total_rows: 1,
            }],
        };

        // Send the batch
        setup.batch_tx.send(batch).await.unwrap();

        // Wait for processing.
        tokio::time::sleep(Duration::from_millis(100)).await;

        // Close the watermark channel by dropping the receiver
        drop(setup.watermark_rx);

        // Wait a bit more for the committer to handle the channel closure
        tokio::time::sleep(Duration::from_millis(200)).await;

        // Verify data was still committed despite watermark channel closure
        {
            let data = setup.store.data.get(DataPipeline::NAME).unwrap();
            assert_eq!(data.get(&1).unwrap().value(), &vec![1, 2, 3]);
        }

        // Close the batch channel to allow the committer to terminate
        drop(setup.batch_tx);

        // Verify the committer task has terminated due to watermark channel closure
        // The task should exit gracefully when it can't send watermarks (returns Break::Cancel)
        let result = setup.committer_handle.await;
        assert!(
            result.is_ok(),
            "Committer should terminate gracefully when watermark channel is closed"
        );
    }
}
