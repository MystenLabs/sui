// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

use mysten_metrics::spawn_monitored_task;
use tokio::{
    task::JoinHandle,
    time::{interval, MissedTickBehavior},
};
use tokio_util::sync::CancellationToken;
use tracing::{debug, error, info, warn};

use crate::{
    db::Db, metrics::IndexerMetrics, models::watermarks::PrunerWatermark,
    pipeline::LOUD_WATERMARK_UPDATE_INTERVAL,
};

use super::{Handler, PrunerConfig};

/// The pruner task is responsible for deleting old data from the database. It will periodically
/// check the `watermarks` table to see if there is any data that should be pruned -- between
/// `pruner_hi` (inclusive), and `reader_lo` (exclusive).
///
/// To ensure that the pruner does not interfere with reads that are still in flight, it respects
/// the watermark's `pruner_timestamp`, which records the time that `reader_lo` was last updated.
/// The task will not prune data until at least `config.delay()` has passed since
/// `pruner_timestamp` to give in-flight reads time to land.
///
/// The task regularly traces its progress, outputting at a higher log level every
/// [LOUD_WATERMARK_UPDATE_INTERVAL]-many checkpoints.
///
/// The task will shutdown if the `cancel` token is signalled. If the `config` is `None`, the task
/// will shutdown immediately.
pub(super) fn pruner<H: Handler + 'static>(
    config: Option<PrunerConfig>,
    db: Db,
    metrics: Arc<IndexerMetrics>,
    cancel: CancellationToken,
) -> JoinHandle<()> {
    spawn_monitored_task!(async move {
        let Some(config) = config else {
            info!(pipeline = H::NAME, "Skipping pruner task");
            return;
        };

        // The pruner can pause for a while, waiting for the delay imposed by the
        // `pruner_timestamp` to expire. In that case, the period between ticks should not be
        // compressed to make up for missed ticks.
        let mut poll = interval(config.interval());
        poll.set_missed_tick_behavior(MissedTickBehavior::Delay);

        // The pruner task will periodically output a log message at a higher log level to
        // demonstrate that it is making progress.
        let mut next_loud_watermark_update = 0;

        'outer: loop {
            // (1) Get the latest pruning bounds from the database.
            let mut watermark = tokio::select! {
                _ = cancel.cancelled() => {
                    info!(pipeline = H::NAME, "Shutdown received");
                    break;
                }

                _ = poll.tick() => {
                    let guard = metrics
                        .watermark_pruner_read_latency
                        .with_label_values(&[H::NAME])
                        .start_timer();

                    let Ok(mut conn) = db.connect().await else {
                        warn!(pipeline = H::NAME, "Pruner failed to connect, while fetching watermark");
                        continue;
                    };

                    match PrunerWatermark::get(&mut conn, H::NAME, config.delay()).await {
                        Ok(Some(current)) => {
                            guard.stop_and_record();
                            current
                        }

                        Ok(None) => {
                            guard.stop_and_record();
                            warn!(pipeline = H::NAME, "No watermark for pipeline, skipping");
                            continue;
                        }

                        Err(e) => {
                            guard.stop_and_record();
                            warn!(pipeline = H::NAME, "Failed to get watermark: {e}");
                            continue;
                        }
                    }
                }
            };

            // (2) Wait until this information can be acted upon.
            if let Some(wait_for) = watermark.wait_for() {
                debug!(pipeline = H::NAME, ?wait_for, "Waiting to prune");
                tokio::select! {
                    _ = tokio::time::sleep(wait_for) => {}
                    _ = cancel.cancelled() => {
                        info!(pipeline = H::NAME, "Shutdown received");
                        break;
                    }
                }
            }

            // (3) Prune chunk by chunk to avoid the task waiting on a long-running database
            // transaction, between tests for cancellation.
            while !watermark.is_empty() {
                if cancel.is_cancelled() {
                    info!(pipeline = H::NAME, "Shutdown received");
                    break 'outer;
                }

                metrics
                    .total_pruner_chunks_attempted
                    .with_label_values(&[H::NAME])
                    .inc();

                let guard = metrics
                    .pruner_delete_latency
                    .with_label_values(&[H::NAME])
                    .start_timer();

                let Ok(mut conn) = db.connect().await else {
                    warn!(
                        pipeline = H::NAME,
                        "Pruner failed to connect, while pruning"
                    );
                    break;
                };

                let (from, to) = watermark.next_chunk(config.max_chunk_size);
                let affected = match H::prune(from, to, &mut conn).await {
                    Ok(affected) => {
                        guard.stop_and_record();
                        watermark.pruner_hi = to as i64;
                        affected
                    }

                    Err(e) => {
                        guard.stop_and_record();
                        error!(pipeline = H::NAME, "Failed to prune data: {e}");
                        break;
                    }
                };

                metrics
                    .total_pruner_chunks_deleted
                    .with_label_values(&[H::NAME])
                    .inc();

                metrics
                    .total_pruner_rows_deleted
                    .with_label_values(&[H::NAME])
                    .inc_by(affected as u64);

                metrics
                    .watermark_pruner_hi
                    .with_label_values(&[H::NAME])
                    .set(watermark.pruner_hi);
            }

            // (4) Update the pruner watermark
            let guard = metrics
                .watermark_pruner_write_latency
                .with_label_values(&[H::NAME])
                .start_timer();

            let Ok(mut conn) = db.connect().await else {
                warn!(
                    pipeline = H::NAME,
                    "Pruner failed to connect, while updating watermark"
                );
                continue;
            };

            match watermark.update(&mut conn).await {
                Err(e) => {
                    let elapsed = guard.stop_and_record();
                    error!(
                        pipeline = H::NAME,
                        elapsed_ms = elapsed * 1000.0,
                        "Failed to update pruner watermark: {e}"
                    )
                }

                Ok(updated) => {
                    let elapsed = guard.stop_and_record();

                    if updated {
                        metrics
                            .watermark_pruner_hi_in_db
                            .with_label_values(&[H::NAME])
                            .set(watermark.pruner_hi);
                    }

                    if watermark.pruner_hi > next_loud_watermark_update {
                        next_loud_watermark_update =
                            watermark.pruner_hi + LOUD_WATERMARK_UPDATE_INTERVAL;

                        info!(
                            pipeline = H::NAME,
                            pruner_hi = watermark.pruner_hi,
                            updated,
                            elapsed_ms = elapsed * 1000.0,
                            "Watermark"
                        );
                    } else {
                        debug!(
                            pipeline = H::NAME,
                            pruner_hi = watermark.pruner_hi,
                            updated,
                            elapsed_ms = elapsed * 1000.0,
                            "Watermark"
                        );
                    }
                }
            }
        }

        info!(pipeline = H::NAME, "Stopping pruner");
    })
}
