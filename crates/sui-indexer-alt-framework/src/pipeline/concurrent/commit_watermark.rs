// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{
    cmp::Ordering,
    collections::{btree_map::Entry, BTreeMap},
    sync::Arc,
};

use sui_pg_db::Db;
use tokio::{
    sync::mpsc,
    task::JoinHandle,
    time::{interval, MissedTickBehavior},
};
use tokio_util::sync::CancellationToken;
use tracing::{debug, error, info, warn};

use crate::{
    metrics::{CheckpointLagMetricReporter, IndexerMetrics},
    models::watermarks::CommitterWatermark,
    pipeline::{logging::WatermarkLogger, CommitterConfig, WatermarkPart, WARN_PENDING_WATERMARKS},
};

use super::Handler;

/// The watermark task is responsible for keeping track of a pipeline's out-of-order commits and
/// updating its row in the `watermarks` table when a continuous run of checkpoints have landed
/// since the last watermark update.
///
/// It receives watermark "parts" that detail the proportion of each checkpoint's data that has
/// been written out by the committer and periodically (on a configurable interval) checks if the
/// watermark for the pipeline can be pushed forward. The watermark can be pushed forward if there
/// is one or more complete (all data for that checkpoint written out) watermarks spanning
/// contiguously from the current high watermark into the future.
///
/// If it detects that more than [WARN_PENDING_WATERMARKS] watermarks have built up, it will issue
/// a warning, as this could be the indication of a memory leak, and the caller probably intended
/// to run the indexer with watermarking disabled (e.g. if they are running a backfill).
///
/// The task regularly traces its progress, outputting at a higher log level every
/// [LOUD_WATERMARK_UPDATE_INTERVAL]-many checkpoints.
///
/// The task will shutdown if the `cancel` token is signalled, or if the `rx` channel closes and
/// the watermark cannot be progressed. If `skip_watermark` is set, the task will shutdown
/// immediately.
pub(super) fn commit_watermark<H: Handler + 'static>(
    initial_watermark: Option<CommitterWatermark<'static>>,
    config: CommitterConfig,
    skip_watermark: bool,
    mut rx: mpsc::Receiver<Vec<WatermarkPart>>,
    db: Db,
    metrics: Arc<IndexerMetrics>,
    cancel: CancellationToken,
) -> JoinHandle<()> {
    tokio::spawn(async move {
        if skip_watermark {
            info!(pipeline = H::NAME, "Skipping commit watermark task");
            return;
        }

        let mut poll = interval(config.watermark_interval());
        poll.set_missed_tick_behavior(MissedTickBehavior::Delay);

        // To correctly update the watermark, the task tracks the watermark it last tried to write
        // and the watermark parts for any checkpoints that have been written since then
        // ("pre-committed"). After each batch is written, the task will try to progress the
        // watermark as much as possible without going over any holes in the sequence of
        // checkpoints (entirely missing watermarks, or incomplete watermarks).
        let mut precommitted: BTreeMap<u64, WatermarkPart> = BTreeMap::new();
        let (mut watermark, mut next_checkpoint) = if let Some(watermark) = initial_watermark {
            let next = watermark.checkpoint_hi_inclusive + 1;
            (watermark, next)
        } else {
            (CommitterWatermark::initial(H::NAME.into()), 0)
        };

        // The watermark task will periodically output a log message at a higher log level to
        // demonstrate that the pipeline is making progress.
        let mut logger = WatermarkLogger::new("concurrent_committer", &watermark);

        let checkpoint_lag_reporter = CheckpointLagMetricReporter::new_for_pipeline::<H>(
            &metrics.watermarked_checkpoint_timestamp_lag,
            &metrics.latest_watermarked_checkpoint_timestamp_lag_ms,
            &metrics.watermark_checkpoint_in_db,
        );

        info!(pipeline = H::NAME, ?watermark, "Starting commit watermark");

        loop {
            tokio::select! {
                _ = cancel.cancelled() => {
                    info!(pipeline = H::NAME, "Shutdown received");
                    break;
                }

                _ = poll.tick() => {
                    if precommitted.len() > WARN_PENDING_WATERMARKS {
                        warn!(
                            pipeline = H::NAME,
                            pending = precommitted.len(),
                            "Pipeline has a large number of pending commit watermarks",
                        );
                    }

                    let Ok(mut conn) = db.connect().await else {
                        warn!(pipeline = H::NAME, "Commit watermark task failed to get connection for DB");
                        continue;
                    };

                    // Check if the pipeline's watermark needs to be updated
                    let guard = metrics
                        .watermark_gather_latency
                        .with_label_values(&[H::NAME])
                        .start_timer();

                    let mut watermark_needs_update = false;
                    while let Some(pending) = precommitted.first_entry() {
                        let part = pending.get();

                        // Some rows from the next watermark have not landed yet.
                        if !part.is_complete() {
                            break;
                        }

                        match next_checkpoint.cmp(&part.watermark.checkpoint_hi_inclusive) {
                            // Next pending checkpoint is from the future.
                            Ordering::Less => break,

                            // This is the next checkpoint -- include it.
                            Ordering::Equal => {
                                watermark = pending.remove().watermark;
                                watermark_needs_update = true;
                                next_checkpoint += 1;
                            }

                            // Next pending checkpoint is in the past. Out of order watermarks can
                            // be encountered when a pipeline is starting up, because ingestion
                            // must start at the lowest checkpoint across all pipelines, or because
                            // of a backfill, where the initial checkpoint has been overridden.
                            Ordering::Greater => {
                                // Track how many we see to make sure it doesn't grow without
                                // bound.
                                metrics
                                    .total_watermarks_out_of_order
                                    .with_label_values(&[H::NAME])
                                    .inc();

                                pending.remove();
                            }
                        }
                    }

                    let elapsed = guard.stop_and_record();

                    metrics
                        .watermark_epoch
                        .with_label_values(&[H::NAME])
                        .set(watermark.epoch_hi_inclusive);

                    metrics
                        .watermark_checkpoint
                        .with_label_values(&[H::NAME])
                        .set(watermark.checkpoint_hi_inclusive);

                    metrics
                        .watermark_transaction
                        .with_label_values(&[H::NAME])
                        .set(watermark.tx_hi);

                    metrics
                        .watermark_timestamp_ms
                        .with_label_values(&[H::NAME])
                        .set(watermark.timestamp_ms_hi_inclusive);

                    debug!(
                        pipeline = H::NAME,
                        elapsed_ms = elapsed * 1000.0,
                        watermark = watermark.checkpoint_hi_inclusive,
                        timestamp = %watermark.timestamp(),
                        pending = precommitted.len(),
                        "Gathered watermarks",
                    );

                    if watermark_needs_update {
                        let guard = metrics
                            .watermark_commit_latency
                            .with_label_values(&[H::NAME])
                            .start_timer();

                        // TODO: If initial_watermark is empty, when we update watermark
                        // for the first time, we should also update the low watermark.
                        match watermark.update(&mut conn).await {
                            // If there's an issue updating the watermark, log it but keep going,
                            // it's OK for the watermark to lag from a correctness perspective.
                            Err(e) => {
                                let elapsed = guard.stop_and_record();
                                error!(
                                    pipeline = H::NAME,
                                    elapsed_ms = elapsed * 1000.0,
                                    ?watermark,
                                    "Error updating commit watermark: {e}",
                                );
                            }

                            Ok(true) => {
                                let elapsed = guard.stop_and_record();

                                logger.log::<H>(&watermark, elapsed);

                                checkpoint_lag_reporter.report_lag(
                                    watermark.checkpoint_hi_inclusive as u64,
                                    watermark.timestamp_ms_hi_inclusive as u64,
                                );

                                metrics
                                    .watermark_epoch_in_db
                                    .with_label_values(&[H::NAME])
                                    .set(watermark.epoch_hi_inclusive);

                                metrics
                                    .watermark_transaction_in_db
                                    .with_label_values(&[H::NAME])
                                    .set(watermark.tx_hi);

                                metrics
                                    .watermark_timestamp_in_db_ms
                                    .with_label_values(&[H::NAME])
                                    .set(watermark.timestamp_ms_hi_inclusive);
                            }
                            Ok(false) => {}
                        }
                    }

                    if rx.is_closed() && rx.is_empty() {
                        info!(pipeline = H::NAME, "Committer closed channel");
                        break;
                    }
                }

                Some(parts) = rx.recv() => {
                    for part in parts {
                        match precommitted.entry(part.checkpoint()) {
                            Entry::Vacant(entry) => {
                                entry.insert(part);
                            }

                            Entry::Occupied(mut entry) => {
                                entry.get_mut().add(part);
                            }
                        }
                    }
                }
            }
        }

        info!(
            pipeline = H::NAME,
            ?watermark,
            "Stopping committer watermark task"
        );
    })
}
