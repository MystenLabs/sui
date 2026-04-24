// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;
use std::time::Duration;

use backoff::ExponentialBackoff;
use scoped_futures::ScopedFutureExt;
use sui_futures::service::Service;
use tokio::sync::mpsc;
use tracing::debug;
use tracing::info;
use tracing::warn;

use crate::metrics::CheckpointLagMetricReporter;
use crate::metrics::IndexerMetrics;
use crate::pipeline::logging::WatermarkLogger;
use crate::pipeline::sequential::Handler;
use crate::pipeline::sequential::collector::BatchedRows;
use crate::store::Connection;
use crate::store::SequentialStore;

const INITIAL_RETRY_INTERVAL: Duration = Duration::from_millis(100);
const MAX_RETRY_INTERVAL: Duration = Duration::from_secs(1);

/// Committer task — receives fully-assembled batches from the collector and commits them in
/// order (one at a time; watermark ordering requires strict serialisation). On commit failure
/// it retries the same batch under exponential backoff. The collector is free to build the
/// next batch in the meantime, bounded by `pipeline_depth`.
pub(super) fn committer<H: Handler>(
    handler: Arc<H>,
    store: H::Store,
    metrics: Arc<IndexerMetrics>,
    mut rx: mpsc::Receiver<BatchedRows<H>>,
) -> Service {
    Service::new().spawn_aborting(async move {
        info!(pipeline = H::NAME, "Starting committer");

        let mut logger = WatermarkLogger::new("sequential_committer");
        let checkpoint_lag_reporter = CheckpointLagMetricReporter::new_for_pipeline::<H>(
            &metrics.watermarked_checkpoint_timestamp_lag,
            &metrics.latest_watermarked_checkpoint_timestamp_lag_ms,
            &metrics.watermark_checkpoint_in_db,
        );

        while let Some(batched) = rx.recv().await {
            let BatchedRows {
                batch,
                watermark,
                batch_rows,
            } = batched;

            let backoff = ExponentialBackoff {
                initial_interval: INITIAL_RETRY_INTERVAL,
                current_interval: INITIAL_RETRY_INTERVAL,
                max_interval: MAX_RETRY_INTERVAL,
                max_elapsed_time: None,
                ..Default::default()
            };

            let commit = || async {
                metrics
                    .total_committer_batches_attempted
                    .with_label_values(&[H::NAME])
                    .inc();

                let guard = metrics
                    .committer_commit_latency
                    .with_label_values(&[H::NAME])
                    .start_timer();

                let result = store
                    .transaction(|conn| {
                        async {
                            conn.set_committer_watermark(H::NAME, watermark).await?;
                            handler.commit(&batch, conn).await
                        }
                        .scope_boxed()
                    })
                    .await;

                let elapsed = guard.stop_and_record();

                match result {
                    Ok(affected) => Ok((affected, elapsed)),
                    Err(e) => {
                        warn!(
                            pipeline = H::NAME,
                            elapsed_ms = elapsed * 1000.0,
                            committed = batch_rows,
                            "Error writing batch: {e}",
                        );
                        metrics
                            .total_committer_batches_failed
                            .with_label_values(&[H::NAME])
                            .inc();
                        Err(backoff::Error::transient(e))
                    }
                }
            };

            let (affected, elapsed) = backoff::future::retry(backoff, commit).await?;

            debug!(
                pipeline = H::NAME,
                affected,
                committed = batch_rows,
                "Wrote batch",
            );
            logger.log::<H>(&watermark, elapsed);

            checkpoint_lag_reporter.report_lag(
                watermark.checkpoint_hi_inclusive,
                watermark.timestamp_ms_hi_inclusive,
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
        }

        info!(pipeline = H::NAME, "Stopping committer");
        Ok(())
    })
}
