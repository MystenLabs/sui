// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;
use std::time::Duration;

use backoff::ExponentialBackoff;
use serde::Deserialize;
use serde::Serialize;
use sui_futures::service::Service;
use sui_futures::stream::Break;
use sui_futures::stream::ConcurrencyConfig;
use sui_futures::stream::TrySpawnStreamExt;
use sui_types::full_checkpoint_content::Checkpoint;
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;
use tracing::debug;
use tracing::error;
use tracing::info;

use async_trait::async_trait;

use crate::metrics::CheckpointLagMetricReporter;
use crate::metrics::IndexerMetrics;
use crate::pipeline::IndexedCheckpoint;

/// If the processor needs to retry processing a checkpoint, it will wait this long initially.
const INITIAL_RETRY_INTERVAL: Duration = Duration::from_millis(100);

/// If the processor needs to retry processing a checkpoint, it will wait at most this long between retries.
const MAX_RETRY_INTERVAL: Duration = Duration::from_secs(5);

fn default_fill_high() -> f64 {
    0.85
}

fn default_fill_low() -> f64 {
    0.6
}

/// Serde-friendly concurrency configuration for the processor.
///
/// `Fixed(n)` gives constant concurrency. `Adaptive { .. }` adjusts the gauge based on
/// downstream channel fill fraction. The `#[serde(untagged)]` attribute allows backward-compatible
/// deserialization: a bare integer like `10` deserializes as `Fixed(10)`.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(untagged)]
pub enum ProcessorConcurrencyConfig {
    Fixed(usize),
    Adaptive {
        initial: usize,
        min: usize,
        max: usize,
        #[serde(default = "default_fill_high")]
        fill_high: f64,
        #[serde(default = "default_fill_low")]
        fill_low: f64,
    },
}

impl ProcessorConcurrencyConfig {
    pub fn initial(&self) -> usize {
        match self {
            Self::Fixed(n) => *n,
            Self::Adaptive { initial, .. } => *initial,
        }
    }

    pub fn max(&self) -> usize {
        match self {
            Self::Fixed(n) => *n,
            Self::Adaptive { max, .. } => *max,
        }
    }

    pub fn is_adaptive(&self) -> bool {
        matches!(self, Self::Adaptive { .. })
    }
}

impl From<ProcessorConcurrencyConfig> for ConcurrencyConfig {
    fn from(config: ProcessorConcurrencyConfig) -> Self {
        match config {
            ProcessorConcurrencyConfig::Fixed(n) => ConcurrencyConfig::fixed(n),
            ProcessorConcurrencyConfig::Adaptive {
                initial,
                min,
                max,
                fill_high,
                fill_low,
            } => ConcurrencyConfig {
                initial,
                min,
                max,
                fill_high,
                fill_low,
            },
        }
    }
}

/// Implementors of this trait are responsible for transforming checkpoint into rows for their
/// table. The `FANOUT` associated value controls how many concurrent workers will be used to
/// process checkpoint information.
#[async_trait]
pub trait Processor: Send + Sync + 'static {
    /// Used to identify the pipeline in logs and metrics.
    const NAME: &'static str;

    /// How much concurrency to use when processing checkpoint data.
    const FANOUT: usize = 10;

    /// The type of value being inserted by the handler.
    type Value: Send + Sync + 'static;

    /// The processing logic for turning a checkpoint into rows of the table.
    ///
    /// All errors returned from this method are treated as transient and will be retried
    /// indefinitely with exponential backoff.
    ///
    /// If you encounter a permanent error that will never succeed on retry (e.g., invalid data
    /// format, unsupported protocol version), you should panic! This stops the indexer and alerts
    /// operators that manual intervention is required. Do not return permanent errors as they will
    /// cause infinite retries and block the pipeline.
    ///
    /// For transient errors (e.g., network issues, rate limiting), simply return the error and
    /// let the framework retry automatically.
    async fn process(&self, checkpoint: &Arc<Checkpoint>) -> anyhow::Result<Vec<Self::Value>>;
}

/// The processor task is responsible for taking checkpoint data and breaking it down into rows
/// ready to commit. It spins up a supervisor that waits on the `rx` channel for checkpoints, and
/// distributes them among workers whose concurrency is governed by `concurrency`.
///
/// Each worker processes a checkpoint into rows and sends them on to the committer using the `tx`
/// channel.
pub(super) fn processor<P: Processor>(
    processor: Arc<P>,
    rx: mpsc::Receiver<Arc<Checkpoint>>,
    tx: mpsc::Sender<IndexedCheckpoint<P>>,
    metrics: Arc<IndexerMetrics>,
    concurrency: ProcessorConcurrencyConfig,
) -> Service {
    Service::new().spawn_aborting(async move {
        info!(pipeline = P::NAME, "Starting processor");
        let checkpoint_lag_reporter = CheckpointLagMetricReporter::new_for_pipeline::<P>(
            &metrics.processed_checkpoint_timestamp_lag,
            &metrics.latest_processed_checkpoint_timestamp_lag_ms,
            &metrics.latest_processed_checkpoint,
        );

        let report_metrics = metrics.clone();
        match ReceiverStream::new(rx)
            .try_for_each_map_spawned(
                concurrency.into(),
                |checkpoint| {
                    let metrics = metrics.clone();
                    let checkpoint_lag_reporter = checkpoint_lag_reporter.clone();
                    let processor = processor.clone();

                    async move {
                        metrics
                            .total_handler_checkpoints_received
                            .with_label_values(&[P::NAME])
                            .inc();

                        let guard = metrics
                            .handler_checkpoint_latency
                            .with_label_values(&[P::NAME])
                            .start_timer();

                        // Retry processing with exponential backoff
                        let backoff = ExponentialBackoff {
                            initial_interval: INITIAL_RETRY_INTERVAL,
                            current_interval: INITIAL_RETRY_INTERVAL,
                            max_interval: MAX_RETRY_INTERVAL,
                            max_elapsed_time: None,
                            ..Default::default()
                        };

                        let values = backoff::future::retry(backoff, || async {
                            processor
                                .process(&checkpoint)
                                .await
                                .map_err(backoff::Error::transient)
                        })
                        .await?;

                        let elapsed = guard.stop_and_record();

                        let epoch = checkpoint.summary.epoch;
                        let cp_sequence_number = checkpoint.summary.sequence_number;
                        let tx_hi = checkpoint.summary.network_total_transactions;
                        let timestamp_ms = checkpoint.summary.timestamp_ms;

                        debug!(
                            pipeline = P::NAME,
                            checkpoint = cp_sequence_number,
                            elapsed_ms = elapsed * 1000.0,
                            "Processed checkpoint",
                        );

                        checkpoint_lag_reporter.report_lag(cp_sequence_number, timestamp_ms);

                        metrics
                            .total_handler_checkpoints_processed
                            .with_label_values(&[P::NAME])
                            .inc();

                        metrics
                            .total_handler_rows_created
                            .with_label_values(&[P::NAME])
                            .inc_by(values.len() as u64);

                        Ok(IndexedCheckpoint::new(
                            epoch,
                            cp_sequence_number,
                            tx_hi,
                            timestamp_ms,
                            values,
                        ))
                    }
                },
                tx,
                move |gauge, inflight| {
                    report_metrics
                        .processor_concurrency_limit
                        .with_label_values(&[P::NAME])
                        .set(gauge as i64);
                    report_metrics
                        .processor_concurrency_inflight
                        .with_label_values(&[P::NAME])
                        .set(inflight as i64);
                },
            )
            .await
        {
            Ok(()) => {
                info!(pipeline = P::NAME, "Checkpoints done, stopping processor");
            }

            Err(Break::Break) => {
                info!(pipeline = P::NAME, "Channel closed, stopping processor");
            }

            Err(Break::Err(e)) => {
                error!(pipeline = P::NAME, "Error from handler: {e}");
                return Err(e.context(format!("Error from processor {}", P::NAME)));
            }
        };

        Ok(())
    })
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::sync::atomic::AtomicU32;
    use std::sync::atomic::Ordering;
    use std::time::Duration;

    use anyhow::ensure;
    use sui_types::test_checkpoint_data_builder::TestCheckpointBuilder;
    use tokio::sync::mpsc;
    use tokio::time::timeout;

    use crate::metrics::IndexerMetrics;

    use super::*;

    pub struct StoredData {
        pub value: u64,
    }

    pub struct DataPipeline;

    #[async_trait]
    impl Processor for DataPipeline {
        const NAME: &'static str = "data";

        type Value = StoredData;

        async fn process(&self, checkpoint: &Arc<Checkpoint>) -> anyhow::Result<Vec<Self::Value>> {
            Ok(vec![
                StoredData {
                    value: checkpoint.summary.sequence_number * 10 + 1,
                },
                StoredData {
                    value: checkpoint.summary.sequence_number * 10 + 2,
                },
            ])
        }
    }

    #[tokio::test]
    async fn test_processor_process_checkpoints() {
        // Build two checkpoints using the test builder
        let checkpoint1: Arc<Checkpoint> = Arc::new(
            TestCheckpointBuilder::new(1)
                .with_epoch(2)
                .with_network_total_transactions(5)
                .with_timestamp_ms(1000000001)
                .build_checkpoint(),
        );
        let checkpoint2: Arc<Checkpoint> = Arc::new(
            TestCheckpointBuilder::new(2)
                .with_epoch(2)
                .with_network_total_transactions(10)
                .with_timestamp_ms(1000000002)
                .build_checkpoint(),
        );

        // Set up the processor, channels, and metrics
        let processor = Arc::new(DataPipeline);
        let (data_tx, data_rx) = mpsc::channel(2);
        let (indexed_tx, mut indexed_rx) = mpsc::channel(2);
        let metrics = IndexerMetrics::new(None, &Default::default());

        // Spawn the processor task
        let _svc = super::processor(
            processor,
            data_rx,
            indexed_tx,
            metrics,
            ProcessorConcurrencyConfig::Fixed(DataPipeline::FANOUT),
        );

        // Send both checkpoints
        data_tx.send(checkpoint1.clone()).await.unwrap();
        data_tx.send(checkpoint2.clone()).await.unwrap();

        // Receive and verify first checkpoint
        let indexed1 = indexed_rx
            .recv()
            .await
            .expect("Should receive first IndexedCheckpoint");
        assert_eq!(indexed1.watermark.checkpoint_hi_inclusive, 1);
        assert_eq!(indexed1.watermark.epoch_hi_inclusive, 2);
        assert_eq!(indexed1.watermark.tx_hi, 5);
        assert_eq!(indexed1.watermark.timestamp_ms_hi_inclusive, 1000000001);
        assert_eq!(indexed1.values.len(), 2);
        assert_eq!(indexed1.values[0].value, 11); // 1 * 10 + 1
        assert_eq!(indexed1.values[1].value, 12); // 1 * 10 + 2

        // Receive and verify second checkpoint
        let indexed2 = indexed_rx
            .recv()
            .await
            .expect("Should receive second IndexedCheckpoint");
        assert_eq!(indexed2.watermark.checkpoint_hi_inclusive, 2);
        assert_eq!(indexed2.watermark.epoch_hi_inclusive, 2);
        assert_eq!(indexed2.watermark.tx_hi, 10);
        assert_eq!(indexed2.watermark.timestamp_ms_hi_inclusive, 1000000002);
        assert_eq!(indexed2.values.len(), 2);
        assert_eq!(indexed2.values[0].value, 21); // 2 * 10 + 1
        assert_eq!(indexed2.values[1].value, 22); // 2 * 10 + 2

        let timeout_result = timeout(Duration::from_secs(1), indexed_rx.recv()).await;
        assert!(
            timeout_result.is_err(),
            "Should timeout waiting for more checkpoints"
        );
    }

    #[tokio::test]
    async fn test_processor_does_not_process_checkpoint_after_cancellation() {
        // Build two checkpoints using the test builder
        let checkpoint1: Arc<Checkpoint> =
            Arc::new(TestCheckpointBuilder::new(1).build_checkpoint());
        let checkpoint2: Arc<Checkpoint> =
            Arc::new(TestCheckpointBuilder::new(2).build_checkpoint());

        // Set up the processor, channels, and metrics
        let processor = Arc::new(DataPipeline);
        let (data_tx, data_rx) = mpsc::channel(2);
        let (indexed_tx, mut indexed_rx) = mpsc::channel(2);
        let metrics = IndexerMetrics::new(None, &Default::default());

        // Spawn the processor task
        let svc = super::processor(
            processor,
            data_rx,
            indexed_tx,
            metrics,
            ProcessorConcurrencyConfig::Fixed(DataPipeline::FANOUT),
        );

        // Send first checkpoint.
        data_tx.send(checkpoint1.clone()).await.unwrap();

        // Receive and verify first checkpoint
        let indexed1 = indexed_rx
            .recv()
            .await
            .expect("Should receive first IndexedCheckpoint");
        assert_eq!(indexed1.watermark.checkpoint_hi_inclusive, 1);

        // Shutdown the processor
        svc.shutdown().await.unwrap();

        // Sending second checkpoint after shutdown should fail, because the data_rx channel is
        // closed.
        data_tx.send(checkpoint2.clone()).await.unwrap_err();

        // Indexed channel is closed, and indexed_rx receives the last None result.
        let next_result = indexed_rx.recv().await;
        assert!(
            next_result.is_none(),
            "Channel should be closed after shutdown"
        );
    }

    #[tokio::test]
    async fn test_processor_error_retry_behavior() {
        struct RetryTestPipeline {
            attempt_count: Arc<AtomicU32>,
        }

        #[async_trait]
        impl Processor for RetryTestPipeline {
            const NAME: &'static str = "retry_test";
            type Value = StoredData;
            async fn process(
                &self,
                checkpoint: &Arc<Checkpoint>,
            ) -> anyhow::Result<Vec<Self::Value>> {
                if checkpoint.summary.sequence_number == 1 {
                    Ok(vec![])
                } else {
                    let attempt = self.attempt_count.fetch_add(1, Ordering::Relaxed) + 1;
                    ensure!(attempt > 2, "Transient error - attempt {attempt}");
                    Ok(vec![])
                }
            }
        }

        // Set up test data
        let checkpoint1: Arc<Checkpoint> =
            Arc::new(TestCheckpointBuilder::new(1).build_checkpoint());
        let checkpoint2: Arc<Checkpoint> =
            Arc::new(TestCheckpointBuilder::new(2).build_checkpoint());

        let attempt_count = Arc::new(AtomicU32::new(0));
        let processor = Arc::new(RetryTestPipeline {
            attempt_count: attempt_count.clone(),
        });

        let (data_tx, data_rx) = mpsc::channel(2);
        let (indexed_tx, mut indexed_rx) = mpsc::channel(2);

        let metrics = IndexerMetrics::new(None, &Default::default());

        // Spawn the processor task
        let _svc = super::processor(
            processor,
            data_rx,
            indexed_tx,
            metrics,
            ProcessorConcurrencyConfig::Fixed(DataPipeline::FANOUT),
        );

        // Send and verify first checkpoint (should succeed immediately)
        data_tx.send(checkpoint1.clone()).await.unwrap();
        let indexed1 = indexed_rx
            .recv()
            .await
            .expect("Should receive first IndexedCheckpoint");
        assert_eq!(indexed1.watermark.checkpoint_hi_inclusive, 1);

        // Send second checkpoint (should fail twice, then succeed on 3rd attempt)
        data_tx.send(checkpoint2.clone()).await.unwrap();

        let indexed2 = indexed_rx
            .recv()
            .await
            .expect("Should receive second IndexedCheckpoint after retries");
        assert_eq!(indexed2.watermark.checkpoint_hi_inclusive, 2);

        // Verify that exactly 3 attempts were made (2 failures + 1 success)
        assert_eq!(attempt_count.load(Ordering::Relaxed), 3);
    }

    // By default, Rust's async tests run on the single-threaded runtime.
    // We need multi_thread here because our test uses std::thread::sleep which blocks the worker thread.
    // The multi-threaded runtime allows other worker threads to continue processing while one is blocked.
    #[tokio::test(flavor = "multi_thread", worker_threads = 8)]
    async fn test_processor_concurrency() {
        // Create a processor that simulates work by sleeping
        struct SlowProcessor;
        #[async_trait]
        impl Processor for SlowProcessor {
            const NAME: &'static str = "slow";
            const FANOUT: usize = 3; // Small fanout for testing
            type Value = StoredData;

            async fn process(
                &self,
                checkpoint: &Arc<Checkpoint>,
            ) -> anyhow::Result<Vec<Self::Value>> {
                // Simulate work by sleeping
                std::thread::sleep(std::time::Duration::from_millis(500));
                Ok(vec![StoredData {
                    value: checkpoint.summary.sequence_number,
                }])
            }
        }

        // Set up test data
        let checkpoints: Vec<Arc<Checkpoint>> = (0..5)
            .map(|i| Arc::new(TestCheckpointBuilder::new(i).build_checkpoint()))
            .collect();

        // Set up channels and metrics
        let processor = Arc::new(SlowProcessor);
        let (data_tx, data_rx) = mpsc::channel(10);
        let (indexed_tx, mut indexed_rx) = mpsc::channel(10);
        let metrics = IndexerMetrics::new(None, &Default::default());

        // Spawn processor task
        let _svc = super::processor(
            processor,
            data_rx,
            indexed_tx,
            metrics,
            ProcessorConcurrencyConfig::Fixed(SlowProcessor::FANOUT),
        );

        // Send all checkpoints and measure time
        let start = std::time::Instant::now();
        for checkpoint in checkpoints {
            data_tx.send(checkpoint).await.unwrap();
        }
        drop(data_tx);

        // Receive all results
        let mut received = Vec::new();
        while let Some(indexed) = indexed_rx.recv().await {
            received.push(indexed);
        }

        // Verify concurrency: total time should be less than sequential processing
        // With FANOUT=3, 5 checkpoints should take ~1000ms (500ms * 2 (batches)) instead of 2500ms (500ms * 5).
        // Adding small 200ms for some processing overhead.
        let elapsed = start.elapsed();
        assert!(elapsed < std::time::Duration::from_millis(1200));

        // Verify results
        assert_eq!(received.len(), 5);
    }

    #[test]
    fn serde_fixed_roundtrip() {
        let config = ProcessorConcurrencyConfig::Fixed(10);
        let json = serde_json::to_string(&config).unwrap();
        let parsed: ProcessorConcurrencyConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, config);
    }

    #[test]
    fn serde_adaptive_roundtrip() {
        let config = ProcessorConcurrencyConfig::Adaptive {
            initial: 10,
            min: 2,
            max: 50,
            fill_high: 0.9,
            fill_low: 0.5,
        };
        let json = serde_json::to_string(&config).unwrap();
        let parsed: ProcessorConcurrencyConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, config);
    }

    #[test]
    fn serde_bare_integer_deserializes_as_fixed() {
        let parsed: ProcessorConcurrencyConfig = serde_json::from_str("10").unwrap();
        assert_eq!(parsed, ProcessorConcurrencyConfig::Fixed(10));
    }

    #[test]
    fn serde_adaptive_defaults() {
        let json = r#"{"initial": 5, "min": 1, "max": 20}"#;
        let parsed: ProcessorConcurrencyConfig = serde_json::from_str(json).unwrap();
        assert_eq!(
            parsed,
            ProcessorConcurrencyConfig::Adaptive {
                initial: 5,
                min: 1,
                max: 20,
                fill_high: 0.85,
                fill_low: 0.6,
            }
        );
    }

    #[test]
    fn concurrency_config_from_fixed() {
        let config: ConcurrencyConfig = ProcessorConcurrencyConfig::Fixed(10).into();
        assert_eq!(config.initial, 10);
        assert_eq!(config.min, 10);
        assert_eq!(config.max, 10);
        assert!(!config.is_adaptive());
    }

    #[test]
    fn concurrency_config_from_adaptive() {
        let config: ConcurrencyConfig = ProcessorConcurrencyConfig::Adaptive {
            initial: 10,
            min: 2,
            max: 50,
            fill_high: 0.9,
            fill_low: 0.5,
        }
        .into();
        assert_eq!(config.initial, 10);
        assert_eq!(config.min, 2);
        assert_eq!(config.max, 50);
        assert!(config.is_adaptive());
    }
}
