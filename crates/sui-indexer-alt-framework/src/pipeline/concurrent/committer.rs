// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{sync::Arc, time::Duration};

use backoff::ExponentialBackoff;
use sui_pg_db::Db;
use tokio::{sync::mpsc, task::JoinHandle};
use tokio_stream::wrappers::ReceiverStream;
use tokio_util::sync::CancellationToken;
use tracing::{debug, error, info, warn};

use crate::{
    metrics::{CheckpointLagMetricReporter, IndexerMetrics},
    pipeline::{Break, CommitterConfig, WatermarkPart},
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
    db: Db,
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
