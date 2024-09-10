// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::data::{Db, DbConnection, QueryExecutor};
use crate::error::Error;
use crate::metrics::Metrics;
use crate::types::chain_identifier::ChainIdentifier;
use async_graphql::ServerError;
use diesel::{ExpressionMethods, OptionalExtension, QueryDsl};
use diesel_async::scoped_futures::ScopedFutureExt;
use std::mem;
use std::sync::Arc;
use std::time::Duration;
use sui_indexer::schema::checkpoints;
use tokio::sync::{watch, RwLock};
use tokio::time::Interval;
use tokio_util::sync::CancellationToken;
use tracing::{error, info};

/// Watermark task that periodically updates the current checkpoint, checkpoint timestamp, and
/// epoch values.
pub(crate) struct WatermarkTask {
    /// Thread-safe watermark that avoids writer starvation
    watermark: WatermarkLock,
    chain_identifier: ChainIdentifierLock,
    db: Db,
    metrics: Metrics,
    sleep: Duration,
    cancel: CancellationToken,
    sender: watch::Sender<u64>,
    receiver: watch::Receiver<u64>,
}

#[derive(Clone, Default)]
pub(crate) struct ChainIdentifierLock(pub(crate) Arc<RwLock<ChainIdentifier>>);

pub(crate) type WatermarkLock = Arc<RwLock<Watermark>>;

/// Watermark used by GraphQL queries to ensure cross-query consistency and flag epoch-boundary
/// changes.
#[derive(Clone, Copy, Default)]
pub(crate) struct Watermark {
    /// The checkpoint upper-bound for the query.
    pub checkpoint: u64,
    /// The checkpoint upper-bound timestamp for the query.
    pub checkpoint_timestamp_ms: u64,
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
        let (sender, receiver) = watch::channel(0);

        Self {
            watermark: Default::default(),
            chain_identifier: Default::default(),
            db,
            metrics,
            sleep,
            cancel,
            sender,
            receiver,
        }
    }

    pub(crate) async fn run(&self) {
        let mut interval = tokio::time::interval(self.sleep);
        // We start the task by first finding & setting the chain identifier
        // so that it can be used in all requests.
        self.get_and_cache_chain_identifier(&mut interval).await;

        loop {
            tokio::select! {
                _ = self.cancel.cancelled() => {
                    info!("Shutdown signal received, terminating watermark update task");
                    return;
                },
                _ = interval.tick() => {
                    let Watermark {checkpoint, epoch, checkpoint_timestamp_ms } = match Watermark::query(&self.db).await {
                        Ok(Some(watermark)) => watermark,
                        Ok(None) => continue,
                        Err(e) => {
                            error!("Failed to fetch chain identifier: {}", e);
                            self.metrics.inc_errors(&[ServerError::new(e.to_string(), None)]);
                            continue;
                        }
                    };

                    // Write the watermark as follows to limit how long we hold the lock
                    let prev_epoch = {
                        let mut w = self.watermark.write().await;
                        w.checkpoint = checkpoint;
                        w.checkpoint_timestamp_ms = checkpoint_timestamp_ms;
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

    pub(crate) fn chain_id_lock(&self) -> ChainIdentifierLock {
        self.chain_identifier.clone()
    }

    /// Receiver for subscribing to epoch changes.
    pub(crate) fn epoch_receiver(&self) -> watch::Receiver<u64> {
        self.receiver.clone()
    }

    // Fetch the chain identifier (once) from the database and cache it.
    async fn get_and_cache_chain_identifier(&self, interval: &mut Interval) {
        loop {
            tokio::select! {
                _ = self.cancel.cancelled() => {
                    info!("Shutdown signal received, terminating attempt to get chain identifier");
                    return;
                },
                _ = interval.tick() => {
                    // we only set the chain_identifier once.
                    let chain = match ChainIdentifier::query(&self.db).await  {
                        Ok(Some(chain)) => chain,
                        Ok(None) => continue,
                        Err(e) => {
                            error!("{}", e);
                            self.metrics.inc_errors(&[ServerError::new(e.to_string(), None)]);
                            continue;
                        }
                    };

                    let mut chain_id_lock = self.chain_identifier.0.write().await;
                    *chain_id_lock = chain.into();
                    return;
                }
            }
        }
    }
}

impl Watermark {
    pub(crate) async fn new(lock: WatermarkLock) -> Self {
        let w = lock.read().await;
        Self {
            checkpoint: w.checkpoint,
            checkpoint_timestamp_ms: w.checkpoint_timestamp_ms,
            epoch: w.epoch,
        }
    }

    pub(crate) async fn query(db: &Db) -> Result<Option<Watermark>, Error> {
        use checkpoints::dsl;
        let Some((checkpoint, checkpoint_timestamp_ms, epoch)): Option<(i64, i64, i64)> = db
            .execute(move |conn| {
                async {
                    conn.first(move || {
                        dsl::checkpoints
                            .select((dsl::sequence_number, dsl::timestamp_ms, dsl::epoch))
                            .order_by(dsl::sequence_number.desc())
                    })
                    .await
                    .optional()
                }
                .scope_boxed()
            })
            .await
            .map_err(|e| Error::Internal(format!("Failed to fetch checkpoint: {e}")))?
        else {
            return Ok(None);
        };
        Ok(Some(Watermark {
            checkpoint: checkpoint as u64,
            checkpoint_timestamp_ms: checkpoint_timestamp_ms as u64,
            epoch: epoch as u64,
        }))
    }
}

impl ChainIdentifierLock {
    pub(crate) async fn read(&self) -> ChainIdentifier {
        let w = self.0.read().await;
        w.0.into()
    }
}
