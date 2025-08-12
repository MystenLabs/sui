// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use tokio::{
    sync::watch,
    task::JoinHandle,
    time::{MissedTickBehavior, interval},
};
use tokio_util::sync::CancellationToken;
use tracing::{info, warn};

use crate::store::{Connection, Store};

use super::{Handler, PrunerConfig};

/// Starts a task for a tasked pipeline to track the main reader lo.
pub(super) fn main_reader_lo<H: Handler + 'static>(
    reader_lo_tx: watch::Sender<Option<u64>>,
    config: Option<PrunerConfig>,
    cancel: CancellationToken,
    store: H::Store,
) -> JoinHandle<()> {
    tokio::spawn(async move {
        // Only start the task if channel is not already initialized.
        if reader_lo_tx.borrow().is_some() {
            info!(pipeline = H::NAME, "Skipping main reader lo task");
            return;
        };

        // Keep the channel alive and set to 0, but stop the task as we don't need to track main
        // `reader_lo`.
        let Some(config) = config else {
            // Set channel to 0 to indicate no pruning.
            reader_lo_tx.send(Some(0)).ok();
            info!(pipeline = H::NAME, "Skipping main reader lo task");
            return;
        };

        // Set the interval to half the provided interval to ensure we refresh the watermark read frequently enough.
        let mut reader_interval = interval(config.interval() / 2);
        // If we miss ticks, skip them to ensure we have the latest watermark.
        reader_interval.set_missed_tick_behavior(MissedTickBehavior::Skip);

        loop {
            tokio::select! {
                _ = cancel.cancelled() => {
                    info!(pipeline = H::NAME, "Shutdown received");
                    break;
                }

                // Periodic refresh of the main reader watermark.
                _ = reader_interval.tick() => {
                    match store.connect().await {
                        Ok(mut conn) => {
                            match conn.reader_watermark(H::NAME).await {
                                Ok(watermark_opt) => {
                                    // If the reader watermark is not found, we assume that pruning
                                    // is not enabled, and checkpoints >= 0 are valid.
                                    if reader_lo_tx.send(Some(watermark_opt.map_or(0, |wm| wm.reader_lo))).is_err() {
                                        info!(pipeline = H::NAME, "Main reader lo receiver dropped, shutting down task");
                                        break;
                                    }
                                }
                                Err(e) => {
                                    warn!(pipeline = H::NAME, "Failed to get reader watermark: {e}");
                                }
                            }
                        },
                        Err(e) => {
                            warn!(pipeline = H::NAME, "Failed to connect to store: {e}");
                        }
                    }
                }
            }
        }
    })
}
