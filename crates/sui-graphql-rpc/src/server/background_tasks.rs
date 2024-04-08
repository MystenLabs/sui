// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;
use std::time::Duration;

use async_graphql::ServerError;
use tokio::sync::{watch, RwLock};
use tokio_util::sync::CancellationToken;
use tracing::{error, info};

use crate::metrics::Metrics;
use crate::{consistency::Watermark, data::Db};

/// Watermark task that periodically updates the current checkpoint and epoch values.
pub(crate) struct ServiceWatermarkTask {
    /// Thread-safe watermark that avoids writer starvation
    watermark: ServiceWatermark,
    db: Db,
    metrics: Metrics,
    sleep: Duration,
    cancel: CancellationToken,
}

pub(crate) type ServiceWatermark = Arc<RwLock<Watermark>>;

/// Starts an infinite loop that periodically updates the `checkpoint_viewed_at` high watermark.
impl ServiceWatermarkTask {
    pub(crate) fn new(
        db: Db,
        metrics: Metrics,
        sleep: Duration,
        cancel: CancellationToken,
    ) -> Self {
        let watermark = Arc::new(RwLock::new(Watermark::default()));
        Self {
            watermark,
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
                    let mut w = self.watermark.write().await;
                    w.checkpoint = checkpoint;
                    if epoch > w.epoch {
                        w.epoch = epoch;
                        tx.send(epoch).unwrap();
                    }
                }
            }
        }
    }

    pub(crate) fn get_watermark(&self) -> ServiceWatermark {
        self.watermark.clone()
    }
}
