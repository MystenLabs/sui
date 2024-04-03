// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::sync::atomic::{AtomicU64, Ordering::Relaxed};
use std::sync::Arc;

use async_graphql::ServerError;
use tokio::sync::watch::{self, error::RecvError};
use tokio_util::sync::CancellationToken;
use tracing::{error, info};

use crate::metrics::Metrics;
use crate::{consistency::Watermark, data::Db};

/// Watermark used by graphql queries to ensure cross-query consistency and flag epoch-boundary
/// changes.
#[derive(Clone)]
pub(crate) struct ServiceWatermark {
    /// The checkpoint upper-bound for the query.
    pub checkpoint: Arc<AtomicU64>,
    /// The current epoch.
    pub epoch: Arc<AtomicU64>,
}

/// Struct that holds the sender and receiver for the epoch boundary signal. The receiver is kept to
/// ensure that at least one receiver stays alive to safely send and unwrap.
pub(crate) struct WatchSender {
    pub tx: watch::Sender<u64>,
    pub rx: watch::Receiver<u64>,
}

/// Starts an infinite loop that periodically updates the `checkpoint_viewed_at` high watermark.
pub(crate) async fn update_watermark(
    db: &Db,
    service_watermark: ServiceWatermark,
    metrics: Metrics,
    sleep_ms: tokio::time::Duration,
    cancellation_token: CancellationToken,
    watch: WatchSender,
) {
    let _rx = watch.rx;
    loop {
        tokio::select! {
            _ = cancellation_token.cancelled() => {
                info!("Shutdown signal received, terminating watermark update task");
                return;
            },
            _ = tokio::time::sleep(sleep_ms) => {
                let Watermark { checkpoint, epoch } = match Watermark::query(db).await {
                    Ok(Some(watermark)) => watermark,
                    Ok(None) => continue,
                    Err(e) => {
                        error!("{}", e);
                        metrics.inc_errors(&[ServerError::new(e.to_string(), None)]);
                        continue;
                    }
                };
                service_watermark.checkpoint.store(checkpoint, Relaxed);
                if epoch > service_watermark.epoch.load(Relaxed) {
                    watch.tx.send(epoch).unwrap();
                    service_watermark.epoch.store(epoch, Relaxed);
                }
            }
        }
    }
}

/// Simple implementation for a listener that waits for an epoch boundary signal.
pub(crate) async fn epoch_boundary_listener(
    cancellation_token: CancellationToken,
    mut listener: watch::Receiver<u64>,
) {
    loop {
        tokio::select! {
            _ = cancellation_token.cancelled() => {
                info!("Shutdown signal received, terminating epoch boundary task");
                return;
            },
            epoch = wait_for_epoch(&mut listener) => {
                match epoch {
                    Ok(epoch) => {
                        info!("Received epoch boundary signal: {}", epoch);
                    }
                    Err(e) => {
                        error!("{}", e);
                    }
                }
            }
        }
    }
}

async fn wait_for_epoch(receiver: &mut watch::Receiver<u64>) -> Result<u64, RecvError> {
    let _ = receiver.changed().await;
    let epoch = *receiver.borrow_and_update();
    return Ok(epoch);
}
