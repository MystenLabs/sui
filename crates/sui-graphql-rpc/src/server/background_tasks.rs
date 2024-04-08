// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::sync::atomic::{AtomicU64, Ordering::Relaxed};
use std::sync::Arc;
use std::time::Duration;

use async_graphql::ServerError;
use tokio::sync::watch;
use tokio_util::sync::CancellationToken;
use tracing::{error, info};

use crate::metrics::Metrics;
use crate::{consistency::Watermark, data::Db};

/// Watermark task that periodically updates the current checkpoint and epoch values.
pub(crate) struct ServiceWatermarkTask {
    /// The checkpoint upper-bound for the query.
    checkpoint: Arc<AtomicU64>,
    /// The current epoch.
    epoch: Arc<AtomicU64>,
    db: Db,
    metrics: Metrics,
    sleep: Duration,
    cancel: CancellationToken,
}

#[derive(Clone)]
pub(crate) struct ServiceWatermark {
    pub checkpoint: Arc<AtomicU64>,
    pub epoch: Arc<AtomicU64>,
}

/// Starts an infinite loop that periodically updates the `checkpoint_viewed_at` high watermark.
impl ServiceWatermarkTask {
    pub(crate) fn new(
        checkpoint: Arc<AtomicU64>,
        epoch: Arc<AtomicU64>,
        db: Db,
        metrics: Metrics,
        sleep: Duration,
        cancel: CancellationToken,
    ) -> Self {
        Self {
            checkpoint,
            epoch,
            db,
            metrics,
            sleep,
            cancel,
        }
    }

    pub(crate) async fn run(&self, tx: watch::Sender<u64>, _rx: watch::Receiver<u64>) {
        loop {
            tokio::select! {
                _ = self.cancel.cancelled() => {
                    info!("Shutdown signal received, terminating watermark update task");
                    return;
                },
                _ = tokio::time::sleep(self.sleep) => {
                    let Watermark { checkpoint, epoch } = match Watermark::query(&self.db).await {
                        Ok(Some(watermark)) => watermark,
                        Ok(None) => continue,
                        Err(e) => {
                            error!("{}", e);
                            self.metrics.inc_errors(&[ServerError::new(e.to_string(), None)]);
                            continue;
                        }
                    };
                    self.checkpoint.store(checkpoint, Relaxed);
                    if epoch > self.epoch.load(Relaxed) {
                        tx.send(epoch).unwrap();
                        self.epoch.store(epoch, Relaxed);
                    }
                }
            }
        }
    }

    pub(crate) fn get_watermark(&self) -> ServiceWatermark {
        ServiceWatermark {
            checkpoint: self.checkpoint.clone(),
            epoch: self.epoch.clone(),
        }
    }
}
