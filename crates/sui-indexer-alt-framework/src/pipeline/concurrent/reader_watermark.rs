// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

use sui_pg_db::Db;
use tokio::{task::JoinHandle, time::interval};
use tokio_util::sync::CancellationToken;
use tracing::{debug, info, warn};

use crate::{
    metrics::IndexerMetrics,
    models::watermarks::{ReaderWatermark, StoredWatermark},
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
    db: Db,
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
                    let Ok(mut conn) = db.connect().await else {
                        warn!(pipeline = H::NAME, "Reader watermark task failed to get connection for DB");
                        continue;
                    };

                    let current = match StoredWatermark::get(&mut conn, H::NAME).await {
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

                    let Ok(updated) = ReaderWatermark::new(H::NAME, new_reader_lo).update(&mut conn).await else {
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
