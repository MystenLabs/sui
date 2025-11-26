// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    time::Duration,
};

use sui_futures::service::Service;
use tokio::{
    sync::SetOnce,
    time::{MissedTickBehavior, interval},
};
use tracing::{info, warn};

use crate::store::{Connection, Store};

use super::Handler;

/// Starts a task for a tasked pipeline to track the main reader lo. The existence of
/// `reader_interval` indicates whether the indexer was tasked, necessitating this task, or not.
pub(super) fn track_main_reader_lo<H: Handler + 'static>(
    reader_lo: Arc<SetOnce<AtomicU64>>,
    reader_interval: Option<Duration>,
    store: H::Store,
) -> Service {
    Service::new().spawn_aborting(async move {
        let Some(reader_interval) = reader_interval else {
            info!(
                pipeline = H::NAME,
                "Not a tasked indexer, skipping main reader lo task"
            );
            reader_lo.set(AtomicU64::new(0)).ok();
            return Ok(());
        };

        // If we miss ticks, skip them to ensure we have the latest watermark.
        let mut reader_interval = interval(reader_interval);
        reader_interval.set_missed_tick_behavior(MissedTickBehavior::Skip);

        loop {
            reader_interval.tick().await;

            let mut conn = match store.connect().await {
                Ok(conn) => conn,
                Err(e) => {
                    warn!(pipeline = H::NAME, "Failed to connect to store: {e}");
                    continue;
                }
            };

            let watermark = match conn.reader_watermark(H::NAME).await {
                // If the reader watermark is not present (either because the watermark entry does
                // not exist, or the reaer watermark is not set), we assume that pruning is not
                // enabled and checkpoints >= 0 are valid.
                Ok(watermark) => watermark.map_or(0, |wm| wm.reader_lo),
                Err(e) => {
                    warn!(pipeline = H::NAME, "Failed to get reader watermark: {e}");
                    continue;
                }
            };

            if let Some(lo) = reader_lo.get() {
                lo.store(watermark, Ordering::Relaxed);
            } else {
                reader_lo.set(AtomicU64::new(watermark)).ok();
            }
        }
    })
}
