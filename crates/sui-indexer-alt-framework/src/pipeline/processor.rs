// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

use sui_types::full_checkpoint_content::CheckpointData;
use tokio::{sync::mpsc, task::JoinHandle};
use tokio_stream::wrappers::ReceiverStream;
use tokio_util::sync::CancellationToken;
use tracing::{debug, error, info};

use crate::{
    metrics::{CheckpointLagMetricReporter, IndexerMetrics},
    pipeline::Break,
    task::TrySpawnStreamExt,
};

use super::IndexedCheckpoint;

/// Implementors of this trait are responsible for transforming checkpoint into rows for their
/// table. The `FANOUT` associated value controls how many concurrent workers will be used to
/// process checkpoint information.
pub trait Processor {
    /// Used to identify the pipeline in logs and metrics.
    const NAME: &'static str;

    /// How much concurrency to use when processing checkpoint data.
    const FANOUT: usize = 10;

    /// The type of value being inserted by the handler.
    type Value: Send + Sync + 'static;

    /// The processing logic for turning a checkpoint into rows of the table.
    fn process(&self, checkpoint: &Arc<CheckpointData>) -> anyhow::Result<Vec<Self::Value>>;
}

/// The processor task is responsible for taking checkpoint data and breaking it down into rows
/// ready to commit. It spins up a supervisor that waits on the `rx` channel for checkpoints, and
/// distributes them among `H::FANOUT` workers.
///
/// Each worker processes a checkpoint into rows and sends them on to the committer using the `tx`
/// channel.
///
/// The task will shutdown if the `cancel` token is cancelled, or if any of the workers encounters
/// an error -- there is no retry logic at this level.
pub(super) fn processor<P: Processor + Send + Sync + 'static>(
    processor: Arc<P>,
    rx: mpsc::Receiver<Arc<CheckpointData>>,
    tx: mpsc::Sender<IndexedCheckpoint<P>>,
    metrics: Arc<IndexerMetrics>,
    cancel: CancellationToken,
) -> JoinHandle<()> {
    tokio::spawn(async move {
        info!(pipeline = P::NAME, "Starting processor");
        let checkpoint_lag_reporter = CheckpointLagMetricReporter::new_for_pipeline::<P>(
            &metrics.processed_checkpoint_timestamp_lag,
            &metrics.latest_processed_checkpoint_timestamp_lag_ms,
            &metrics.latest_processed_checkpoint,
        );

        match ReceiverStream::new(rx)
            .try_for_each_spawned(P::FANOUT, |checkpoint| {
                let tx = tx.clone();
                let metrics = metrics.clone();
                let cancel = cancel.clone();
                let checkpoint_lag_reporter = checkpoint_lag_reporter.clone();
                let processor = processor.clone();

                async move {
                    if cancel.is_cancelled() {
                        return Err(Break::Cancel);
                    }

                    metrics
                        .total_handler_checkpoints_received
                        .with_label_values(&[P::NAME])
                        .inc();

                    let guard = metrics
                        .handler_checkpoint_latency
                        .with_label_values(&[P::NAME])
                        .start_timer();

                    let values = processor.process(&checkpoint)?;
                    let elapsed = guard.stop_and_record();

                    let epoch = checkpoint.checkpoint_summary.epoch;
                    let cp_sequence_number = checkpoint.checkpoint_summary.sequence_number;
                    let tx_hi = checkpoint.checkpoint_summary.network_total_transactions;
                    let timestamp_ms = checkpoint.checkpoint_summary.timestamp_ms;

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

                    tx.send(IndexedCheckpoint::new(
                        epoch,
                        cp_sequence_number,
                        tx_hi,
                        timestamp_ms,
                        values,
                    ))
                    .await
                    .map_err(|_| Break::Cancel)?;

                    Ok(())
                }
            })
            .await
        {
            Ok(()) => {
                info!(pipeline = P::NAME, "Checkpoints done, stopping processor");
            }

            Err(Break::Cancel) => {
                info!(pipeline = P::NAME, "Shutdown received, stopping processor");
            }

            Err(Break::Err(e)) => {
                error!(pipeline = P::NAME, "Error from handler: {e}");
                cancel.cancel();
            }
        };
    })
}

#[cfg(test)]
mod tests {
    use crate::metrics::IndexerMetrics;
    use std::{sync::Arc, time::Duration};
    use sui_types::test_checkpoint_data_builder::TestCheckpointDataBuilder;
    use tokio::{sync::mpsc, time::timeout};
    use tokio_util::sync::CancellationToken;

    use super::*;

    pub struct StoredData {
        pub value: u64,
    }

    pub struct DataPipeline;

    impl Processor for DataPipeline {
        const NAME: &'static str = "data";

        type Value = StoredData;

        fn process(&self, checkpoint: &Arc<CheckpointData>) -> anyhow::Result<Vec<Self::Value>> {
            Ok(vec![
                StoredData {
                    value: checkpoint.checkpoint_summary.sequence_number * 10 + 1,
                },
                StoredData {
                    value: checkpoint.checkpoint_summary.sequence_number * 10 + 2,
                },
            ])
        }
    }

    #[tokio::test]
    async fn test_processor_process_checkpoints() {
        // Build two checkpoints using the test builder
        let checkpoint1 = Arc::new(
            TestCheckpointDataBuilder::new(1)
                .with_epoch(2)
                .with_network_total_transactions(5)
                .with_timestamp_ms(1000000001)
                .build_checkpoint(),
        );
        let checkpoint2 = Arc::new(
            TestCheckpointDataBuilder::new(2)
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
        let cancel = CancellationToken::new();

        // Spawn the processor task
        let handle = super::processor(processor, data_rx, indexed_tx, metrics, cancel.clone());

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

        // Clean up
        drop(data_tx);
        let _ = handle.await;
    }

    #[tokio::test]
    async fn test_processor_does_not_process_checkpoint_after_cancellation() {
        // Build two checkpoints using the test builder
        let checkpoint1 = Arc::new(TestCheckpointDataBuilder::new(1).build_checkpoint());
        let checkpoint2 = Arc::new(TestCheckpointDataBuilder::new(2).build_checkpoint());

        // Set up the processor, channels, and metrics
        let processor = Arc::new(DataPipeline);
        let (data_tx, data_rx) = mpsc::channel(2);
        let (indexed_tx, mut indexed_rx) = mpsc::channel(2);
        let metrics = IndexerMetrics::new(None, &Default::default());
        let cancel = CancellationToken::new();

        // Spawn the processor task
        let handle = super::processor(processor, data_rx, indexed_tx, metrics, cancel.clone());

        // Send first checkpoint.
        data_tx.send(checkpoint1.clone()).await.unwrap();

        // Receive and verify first checkpoint
        let indexed1 = indexed_rx
            .recv()
            .await
            .expect("Should receive first IndexedCheckpoint");
        assert_eq!(indexed1.watermark.checkpoint_hi_inclusive, 1);

        // Cancel the processor
        cancel.cancel();

        // Send second checkpoint after cancellation
        data_tx.send(checkpoint2.clone()).await.unwrap();

        // Indexed channel is closed, and indexed_rx receives the last None result.
        let next_result = indexed_rx.recv().await;
        assert!(
            next_result.is_none(),
            "Channel should be closed after cancellation"
        );

        // Clean up
        let _ = handle.await;
    }

    #[tokio::test]
    async fn test_processor_error_failed_to_process_checkpoint() {
        // Create a pipeline that succeeds for checkpoint 1 but fails for others
        struct ErrorPipeline;
        impl Processor for ErrorPipeline {
            const NAME: &'static str = "error";
            type Value = StoredData;
            fn process(
                &self,
                checkpoint: &Arc<CheckpointData>,
            ) -> anyhow::Result<Vec<Self::Value>> {
                if checkpoint.checkpoint_summary.sequence_number == 1 {
                    Ok(vec![])
                } else {
                    anyhow::bail!("Test error");
                }
            }
        }

        // Set up test data
        let checkpoint1 = Arc::new(TestCheckpointDataBuilder::new(1).build_checkpoint());
        let checkpoint2 = Arc::new(TestCheckpointDataBuilder::new(2).build_checkpoint());

        // Set up the processor, channels, and metrics
        let processor = Arc::new(ErrorPipeline);
        let (data_tx, data_rx) = mpsc::channel(1);
        let (indexed_tx, mut indexed_rx) = mpsc::channel(1);
        let metrics = IndexerMetrics::new(None, &Default::default());
        let cancel = CancellationToken::new();

        // Spawn the processor task
        let handle = super::processor(processor, data_rx, indexed_tx, metrics, cancel.clone());

        // Send and verify first checkpoint (should succeed)
        data_tx.send(checkpoint1.clone()).await.unwrap();
        let indexed1 = indexed_rx
            .recv()
            .await
            .expect("Should receive first IndexedCheckpoint");
        assert_eq!(indexed1.watermark.checkpoint_hi_inclusive, 1);

        // Send second checkpoint (should fail and cause processor to stop)
        data_tx.send(checkpoint2.clone()).await.unwrap();

        // Verify that the channel is closed after the error
        let next_result = indexed_rx.recv().await;
        assert!(
            next_result.is_none(),
            "Channel should be closed after processing error"
        );

        // Clean up
        let _ = handle.await;
    }

    // By default, Rust's async tests run on the single-threaded runtime.
    // We need multi_thread here because our test uses std::thread::sleep which blocks the worker thread.
    // The multi-threaded runtime allows other worker threads to continue processing while one is blocked.
    #[tokio::test(flavor = "multi_thread", worker_threads = 8)]
    async fn test_processor_concurrency() {
        // Create a processor that simulates work by sleeping
        struct SlowProcessor;
        impl Processor for SlowProcessor {
            const NAME: &'static str = "slow";
            const FANOUT: usize = 3; // Small fanout for testing
            type Value = StoredData;

            fn process(
                &self,
                checkpoint: &Arc<CheckpointData>,
            ) -> anyhow::Result<Vec<Self::Value>> {
                // Simulate work by sleeping
                std::thread::sleep(std::time::Duration::from_millis(500));
                Ok(vec![StoredData {
                    value: checkpoint.checkpoint_summary.sequence_number,
                }])
            }
        }

        // Set up test data
        let checkpoints: Vec<_> = (0..5)
            .map(|i| Arc::new(TestCheckpointDataBuilder::new(i).build_checkpoint()))
            .collect();

        // Set up channels and metrics
        let processor = Arc::new(SlowProcessor);
        let (data_tx, data_rx) = mpsc::channel(10);
        let (indexed_tx, mut indexed_rx) = mpsc::channel(10);
        let metrics = IndexerMetrics::new(None, &Default::default());
        let cancel = CancellationToken::new();

        // Spawn processor task
        let handle = super::processor(processor, data_rx, indexed_tx, metrics, cancel.clone());

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

        // Clean up
        let _ = handle.await;
    }
}
