// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    time::Duration,
};

use tokio::{
    sync::SetOnce,
    task::JoinHandle,
    time::{MissedTickBehavior, interval},
};
use tokio_util::sync::CancellationToken;
use tracing::{info, warn};

use crate::store::{Connection, Store};

use super::Handler;

/// Starts a task for a tasked pipeline to track the main reader lo. The existence of
/// `reader_interval` indicates whether the indexer was tasked, necessitating this task, or not.
pub(super) fn track_main_reader_lo<H: Handler + 'static>(
    reader_lo: Arc<SetOnce<AtomicU64>>,
    reader_interval: Option<Duration>,
    cancel: CancellationToken,
    store: H::Store,
) -> JoinHandle<()> {
    tokio::spawn(async move {
        let Some(reader_interval) = reader_interval else {
            info!(
                pipeline = H::NAME,
                "Not a tasked indexer, skipping main reader lo task"
            );
            reader_lo.set(AtomicU64::new(0)).ok();
            return;
        };

        let mut reader_interval = interval(reader_interval);

        // If we miss ticks, skip them to ensure we have the latest watermark.
        reader_interval.set_missed_tick_behavior(MissedTickBehavior::Skip);

        loop {
            tokio::select! {
                _ = cancel.cancelled() => {
                    info!(pipeline = H::NAME, "Shutdown received");
                    break;
                }

                _ = reader_interval.tick() => {
                    match store.connect().await {
                        Ok(mut conn) => {
                            match conn.reader_watermark(H::NAME).await {
                                Ok(watermark_opt) => {
                                    // If the reader watermark is not present (either because the
                                    // watermark entry does not exist, or the reader watermark is
                                    // not set), we assume that pruning is not enabled, and
                                    // checkpoints >= 0 are valid.
                                    let update = watermark_opt.map_or(0, |wm| wm.reader_lo);

                                    let current = reader_lo.get();

                                    if let Some(can_update) = current {
                                        can_update.store(update, Ordering::Relaxed);
                                    } else {
                                        reader_lo.set(AtomicU64::new(update)).ok();
                                    }
                                }
                                Err(e) => {
                                    warn!(pipeline = H::NAME, "Failed to get reader watermark: {e}");
                                }
                            }
                        }
                        Err(e) => {
                            warn!(pipeline = H::NAME, "Failed to connect to store: {e}");
                        }
                    }
                }
            }
        }
    })
}
