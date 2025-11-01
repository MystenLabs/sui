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
    reader_lo_tx: Option<watch::Sender<Option<u64>>>,
    config: Option<PrunerConfig>,
    cancel: CancellationToken,
    store: H::Store,
) -> JoinHandle<()> {
    tokio::spawn(async move {
        let Some(reader_lo_tx) = reader_lo_tx else {
            info!(pipeline = H::NAME, "Skipping main reader lo task");
            return;
        };

        let Some(config) = config else {
            info!(pipeline = H::NAME, "Skipping main reader lo task");
            return;
        };

        let mut reader_interval = interval(config.interval() / 2);
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
