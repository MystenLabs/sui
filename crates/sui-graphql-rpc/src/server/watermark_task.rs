// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::data::{Db, DbConnection, QueryExecutor};
use crate::error::Error;
use crate::metrics::Metrics;
use async_graphql::ServerError;
use diesel::{ExpressionMethods, OptionalExtension, QueryDsl};
use std::mem;
use std::sync::Arc;
use std::time::Duration;
use sui_indexer::schema::checkpoints;
use tokio::sync::{watch, RwLock};
use tokio_util::sync::CancellationToken;
use tracing::{error, info};

/// Watermark task that periodically updates the current checkpoint and epoch values.
pub(crate) struct WatermarkTask {
    /// Thread-safe watermark that avoids writer starvation
    watermark: WatermarkLock,
    db: Db,
    metrics: Metrics,
    sleep: Duration,
    cancel: CancellationToken,
    sender: watch::Sender<u64>,
    _receiver: watch::Receiver<u64>,
}

pub(crate) type WatermarkLock = Arc<RwLock<Watermark>>;

/// Watermark used by GraphQL queries to ensure cross-query consistency and flag epoch-boundary
/// changes.
#[derive(Clone, Copy, Default)]
pub(crate) struct Watermark {
    /// The checkpoint upper-bound for the query.
    pub checkpoint: u64,
    /// The current epoch.
    pub epoch: u64,
}

/// Starts an infinite loop that periodically updates the `checkpoint_viewed_at` high watermark.
impl WatermarkTask {
    pub(crate) fn new(
        db: Db,
        metrics: Metrics,
        sleep: Duration,
        cancel: CancellationToken,
    ) -> Self {
        let (sender, _receiver) = watch::channel(0);

        Self {
            watermark: Default::default(),
            db,
            metrics,
            sleep,
            cancel,
            sender,
            _receiver,
        }
    }

    pub(crate) async fn run(&self) {
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

                    // Write the watermark as follows to limit how long we hold the lock
                    let prev_epoch = {
                        let mut w = self.watermark.write().await;
                        w.checkpoint = checkpoint;
                        mem::replace(&mut w.epoch, epoch)
                    };

                    if epoch > prev_epoch {
                        self.sender.send(epoch).unwrap();
                    }
                }
            }
        }
    }

    pub(crate) fn lock(&self) -> WatermarkLock {
        self.watermark.clone()
    }

    pub(crate) fn _receiver(&self) -> watch::Receiver<u64> {
        self._receiver.clone()
    }
}

impl Watermark {
    pub(crate) async fn new(lock: WatermarkLock) -> Self {
        let w = lock.read().await;
        Self {
            checkpoint: w.checkpoint,
            epoch: w.epoch,
        }
    }

    pub(crate) async fn query(db: &Db) -> Result<Option<Watermark>, Error> {
        use checkpoints::dsl;
        let Some((checkpoint, epoch)): Option<(i64, i64)> = db
            .execute(move |conn| {
                conn.first(move || {
                    dsl::checkpoints
                        .select((dsl::sequence_number, dsl::epoch))
                        .order_by(dsl::sequence_number.desc())
                })
                .optional()
            })
            .await
            .map_err(|e| Error::Internal(format!("Failed to fetch checkpoint: {e}")))?
        else {
            return Ok(None);
        };

        Ok(Some(Watermark {
            checkpoint: checkpoint as u64,
            epoch: epoch as u64,
        }))
    }
}
