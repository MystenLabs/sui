// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::sync::atomic::AtomicU64;
use std::sync::Arc;

use sui_types::full_checkpoint_content::CheckpointData;
use tokio::{sync::mpsc, task::JoinHandle};
use tokio_stream::wrappers::ReceiverStream;
use tokio_util::sync::CancellationToken;
use tracing::{debug, error, info};

use crate::{metrics::IndexerMetrics, pipeline::Break, task::TrySpawnStreamExt};

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
    processor: P,
    rx: mpsc::Receiver<Arc<CheckpointData>>,
    tx: mpsc::Sender<IndexedCheckpoint<P>>,
    metrics: Arc<IndexerMetrics>,
    cancel: CancellationToken,
) -> JoinHandle<()> {
    tokio::spawn(async move {
        info!(pipeline = P::NAME, "Starting processor");
        let latest_processed_checkpoint = Arc::new(AtomicU64::new(0));
        let processor = Arc::new(processor);

        match ReceiverStream::new(rx)
            .try_for_each_spawned(P::FANOUT, |checkpoint| {
                let tx = tx.clone();
                let metrics = metrics.clone();
                let cancel = cancel.clone();
                let latest_processed_checkpoint = latest_processed_checkpoint.clone();
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

                    let lag = chrono::Utc::now().timestamp_millis() - timestamp_ms as i64;
                    metrics
                        .processed_checkpoint_timestamp_lag
                        .with_label_values(&[P::NAME])
                        .observe((lag as f64) / 1000.0);

                    let prev = latest_processed_checkpoint
                        .fetch_max(cp_sequence_number, std::sync::atomic::Ordering::Relaxed);
                    if cp_sequence_number > prev {
                        metrics
                            .latest_processed_checkpoint
                            .with_label_values(&[P::NAME])
                            .set(cp_sequence_number as i64);
                        metrics
                            .latest_processed_checkpoint_timestamp_lag_ms
                            .with_label_values(&[P::NAME])
                            .set(lag);
                    }

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
