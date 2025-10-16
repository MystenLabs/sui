// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

use tokio::{task::JoinHandle, time::interval};
use tokio_util::sync::CancellationToken;
use tracing::{debug, info, warn};

use crate::{
    metrics::IndexerMetrics,
    store::{Connection, Store},
};

use super::{Handler, PrunerConfig};

/// The reader watermark task is responsible for updating the `reader_lo` and `pruner_timestamp`
/// values for a pipeline's row in the watermark table, based on the pruner configuration, and the
/// committer's progress.
///
/// `reader_lo` is the lowest checkpoint that readers are allowed to read from with a guarantee of
/// data availability for this pipeline, and `pruner_timestamp` is the timestamp at which this task
/// last updated that watermark. The timestamp is always fetched from the database (not from the
/// indexer or the reader), to avoid issues with drift between clocks.
///
/// If there is no pruner configuration, this task will immediately exit. Otherwise, the task exits
/// when the provided cancellation token is triggered.
pub(super) fn reader_watermark<H: Handler + 'static>(
    config: Option<PrunerConfig>,
    store: H::Store,
    metrics: Arc<IndexerMetrics>,
    cancel: CancellationToken,
) -> JoinHandle<()> {
    tokio::spawn(async move {
        let Some(config) = config else {
            info!(pipeline = H::NAME, "Skipping reader watermark task");
            return;
        };

        let mut poll = interval(config.interval());

        loop {
            tokio::select! {
                _ = cancel.cancelled() => {
                    info!(pipeline = H::NAME, "Shutdown received");
                    break;
                }

                _ = poll.tick() => {
                    let Ok(mut conn) = store.connect().await else {
                        warn!(pipeline = H::NAME, "Reader watermark task failed to get connection for DB");
                        continue;
                    };

                    let current = match conn.reader_watermark(H::NAME).await {
                        Ok(Some(current)) => current,

                        Ok(None) => {
                            warn!(pipeline = H::NAME, "No watermark for pipeline, skipping");
                            continue;
                        }

                        Err(e) => {
                            warn!(pipeline = H::NAME, "Failed to get current watermark: {e}");
                            continue;
                        }
                    };

                    // Calculate the new reader watermark based on the current high watermark.
                    let new_reader_lo = (current.checkpoint_hi_inclusive as u64 + 1)
                        .saturating_sub(config.retention);

                    if new_reader_lo <= current.reader_lo as u64 {
                        debug!(
                            pipeline = H::NAME,
                            current = current.reader_lo,
                            new = new_reader_lo,
                            "No change to reader watermark",
                        );
                        continue;
                    }

                    metrics
                        .watermark_reader_lo
                        .with_label_values(&[H::NAME])
                        .set(new_reader_lo as i64);

                    let Ok(updated) = conn.set_reader_watermark(H::NAME, new_reader_lo).await else {
                        warn!(pipeline = H::NAME, "Failed to update reader watermark");
                        continue;
                    };

                    if updated {
                        info!(pipeline = H::NAME, new_reader_lo, "Watermark");

                        metrics
                            .watermark_reader_lo_in_db
                            .with_label_values(&[H::NAME])
                            .set(new_reader_lo as i64);
                    }
                }
            }
        }

        info!(pipeline = H::NAME, "Stopping reader watermark task");
    })
}

#[cfg(test)]
mod tests {
    use async_trait::async_trait;
    use std::sync::{Arc, Mutex};
    use sui_pg_db::FieldCount;
    use sui_types::full_checkpoint_content::CheckpointData;
    use tokio::time::Duration;
    use tokio_util::sync::CancellationToken;

    use crate::{metrics::IndexerMetrics, mocks::store::*, pipeline::Processor};

    use super::*;

    // Fixed retention value used across all tests
    const TEST_RETENTION: u64 = 5;
    // Default timeout for test operations
    const TEST_TIMEOUT: Duration = Duration::from_secs(20);

    #[derive(Clone, FieldCount)]
    pub struct StoredData;

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
            _values: &[Self::Value],
            _conn: &mut MockConnection<'a>,
        ) -> anyhow::Result<usize> {
            Ok(0)
        }
    }

    struct TestSetup {
        store: MockStore,
        handle: JoinHandle<()>,
        cancel: CancellationToken,
    }

    async fn setup_test(
        watermark: MockWatermark,
        interval_ms: u64,
        connection_failure_attempts: usize,
        set_reader_watermark_failure_attempts: usize,
    ) -> TestSetup {
        let store = MockStore {
            watermark: Arc::new(Mutex::new(Some(watermark))),
            set_reader_watermark_failure_attempts: Arc::new(Mutex::new(
                set_reader_watermark_failure_attempts,
            )),
            connection_failure: Arc::new(Mutex::new(ConnectionFailure {
                connection_failure_attempts,
                ..Default::default()
            })),
            ..Default::default()
        };

        let config = PrunerConfig {
            interval_ms,
            delay_ms: 100,
            retention: TEST_RETENTION,
            max_chunk_size: 100,
            prune_concurrency: 1,
        };

        let metrics = IndexerMetrics::new(None, &Default::default());
        let cancel = CancellationToken::new();

        let store_clone = store.clone();
        let cancel_clone = cancel.clone();
        let handle =
            reader_watermark::<DataPipeline>(Some(config), store_clone, metrics, cancel_clone);

        TestSetup {
            store,
            handle,
            cancel,
        }
    }

    #[tokio::test]
    async fn test_reader_watermark_updates() {
        let watermark = MockWatermark {
            epoch_hi_inclusive: 0,
            checkpoint_hi_inclusive: 10, // Current high watermark
            tx_hi: 100,
            timestamp_ms_hi_inclusive: 1000,
            reader_lo: 0, // Initial reader_lo
            pruner_timestamp: 0,
            pruner_hi: 0,
        };
        let polling_interval_ms = 100;
        let connection_failure_attempts = 0;
        let set_reader_watermark_failure_attempts = 0;
        let setup = setup_test(
            watermark,
            polling_interval_ms,
            connection_failure_attempts,
            set_reader_watermark_failure_attempts,
        )
        .await;

        // Wait for a few intervals to allow the task to update the watermark
        tokio::time::sleep(Duration::from_millis(200)).await;

        // new reader_lo = checkpoint_hi_inclusive (10) - retention (5) + 1 = 6
        {
            let watermarks = setup.store.watermark().unwrap();
            assert_eq!(watermarks.reader_lo, 6);
        }

        // Clean up
        setup.cancel.cancel();
        let _ = setup.handle.await;
    }

    #[tokio::test]
    async fn test_reader_watermark_does_not_update_smaller_reader_lo() {
        let watermark = MockWatermark {
            epoch_hi_inclusive: 0,
            checkpoint_hi_inclusive: 10, // Current high watermark
            tx_hi: 100,
            timestamp_ms_hi_inclusive: 1000,
            reader_lo: 7, // Initial reader_lo
            pruner_timestamp: 0,
            pruner_hi: 0,
        };
        let polling_interval_ms = 100;
        let connection_failure_attempts = 0;
        let set_reader_watermark_failure_attempts = 0;
        let setup = setup_test(
            watermark,
            polling_interval_ms,
            connection_failure_attempts,
            set_reader_watermark_failure_attempts,
        )
        .await;

        // Wait for a few intervals to allow the task to update the watermark
        tokio::time::sleep(Duration::from_millis(200)).await;

        // new reader_lo = checkpoint_hi_inclusive (10) - retention (5) + 1 = 6,
        // which is smaller than current reader_lo (7). Therefore, the reader_lo was not updated.
        {
            let watermarks = setup.store.watermark().unwrap();
            assert_eq!(
                watermarks.reader_lo, 7,
                "Reader watermark should not be updated when new value is smaller"
            );
        }

        // Clean up
        setup.cancel.cancel();
        let _ = setup.handle.await;
    }

    #[tokio::test]
    async fn test_reader_watermark_retry_update_after_connection_failure() {
        let watermark = MockWatermark {
            epoch_hi_inclusive: 0,
            checkpoint_hi_inclusive: 10, // Current high watermark
            tx_hi: 100,
            timestamp_ms_hi_inclusive: 1000,
            reader_lo: 0, // Initial reader_lo
            pruner_timestamp: 0,
            pruner_hi: 0,
        };
        let polling_interval_ms = 1_000; // Long interval for testing retry
        let connection_failure_attempts = 1;
        let set_reader_watermark_failure_attempts = 0;
        let setup = setup_test(
            watermark,
            polling_interval_ms,
            connection_failure_attempts,
            set_reader_watermark_failure_attempts,
        )
        .await;

        // Wait for first connection attempt (which should fail)
        setup
            .store
            .wait_for_connection_attempts(1, TEST_TIMEOUT)
            .await;

        // Verify state before retry succeeds
        let watermark = setup.store.watermark().unwrap();
        assert_eq!(
            watermark.reader_lo, 0,
            "Reader watermark should not be updated due to DB connection failure"
        );

        // Wait for second connection attempt (which should succeed)
        setup
            .store
            .wait_for_connection_attempts(2, TEST_TIMEOUT)
            .await;

        // Wait a bit more for the watermark update to complete
        tokio::time::sleep(Duration::from_millis(100)).await;

        // Verify state after retry succeeds
        let watermark = setup.store.watermark().unwrap();
        assert_eq!(
            watermark.reader_lo, 6,
            "Reader watermark should be updated after retry succeeds"
        );

        // Clean up
        setup.cancel.cancel();
        let _ = setup.handle.await;
    }

    #[tokio::test]
    async fn test_reader_watermark_retry_update_after_set_watermark_failure() {
        let watermark = MockWatermark {
            epoch_hi_inclusive: 0,
            checkpoint_hi_inclusive: 10, // Current high watermark
            tx_hi: 100,
            timestamp_ms_hi_inclusive: 1000,
            reader_lo: 0, // Initial reader_lo
            pruner_timestamp: 0,
            pruner_hi: 0,
        };
        let polling_interval_ms = 1_000; // Long interval for testing retry
        let connection_failure_attempts = 0;
        let set_reader_watermark_failure_attempts = 1;
        let setup = setup_test(
            watermark,
            polling_interval_ms,
            connection_failure_attempts,
            set_reader_watermark_failure_attempts,
        )
        .await;

        // Wait for first failed attempt
        tokio::time::sleep(Duration::from_millis(200)).await;

        // Verify state before retry succeeds
        {
            let watermarks = setup.store.watermark().unwrap();
            assert_eq!(
                watermarks.reader_lo, 0,
                "Reader watermark should not be updated due to set_reader_watermark failure"
            );
        }

        // Wait for next polling for second attempt
        tokio::time::sleep(Duration::from_millis(1200)).await;

        // Verify state after retry succeeds
        {
            let watermarks = setup.store.watermark().unwrap();
            assert_eq!(watermarks.reader_lo, 6);
        }

        // Clean up
        setup.cancel.cancel();
        let _ = setup.handle.await;
    }
}
