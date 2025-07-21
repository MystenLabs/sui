// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::data::{Db, DbConnection, QueryExecutor};
use crate::error::Error;
use crate::metrics::Metrics;
use crate::types::chain_identifier::ChainIdentifier;
use async_graphql::ServerError;
use diesel::{ExpressionMethods, JoinOnDsl, OptionalExtension, QueryDsl};
use diesel_async::scoped_futures::ScopedFutureExt;
use std::mem;
use std::sync::Arc;
use std::time::Duration;
use sui_indexer::schema::{checkpoints, watermarks};
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
    /// The current epoch.
    pub epoch: u64,
    /// The timestamp of the inclusive upper-bound checkpoint for the query. This is used for the
    /// health check.
    pub hi_cp_timestamp_ms: u64,
    /// The inclusive checkpoint upper-bound for the query.
    pub hi_cp: u64,
    /// The inclusive tx_sequence_number upper-bound for the query.
    pub hi_tx: u64,
    /// Smallest queryable checkpoint - checkpoints below this value are pruned.
    pub lo_cp: u64,
    /// Smallest queryable tx_sequence_number - tx_sequence_numbers below this value are pruned.
    pub lo_tx: u64,
}

/// Starts an infinite loop that periodically updates the watermark.
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
                    let Watermark {epoch, hi_cp_timestamp_ms, hi_cp, hi_tx, lo_cp, lo_tx } = match Watermark::query(&self.db).await {
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
                        w.hi_cp = hi_cp;
                        w.hi_tx = hi_tx;
                        w.hi_cp_timestamp_ms = hi_cp_timestamp_ms;
                        w.lo_cp = lo_cp;
                        w.lo_tx = lo_tx;
                        mem::replace(&mut w.epoch, epoch)
                    };

                    // On epoch boundary, notify subscribers
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
            hi_cp: w.hi_cp,
            hi_cp_timestamp_ms: w.hi_cp_timestamp_ms,
            hi_tx: w.hi_tx,
            epoch: w.epoch,
            lo_cp: w.lo_cp,
            lo_tx: w.lo_tx,
        }
    }

    /// Queries the watermarks table for the `checkpoints` pipeline to determine the available range
    /// of checkpoints and tx_sequence_numbers. We don't query tables directly as pruning may be in
    /// progress, which means the lower bound of data will constantly change. The watermarks table
    /// has a `tx_hi` value, but not a `tx_lo` value, so the query also joins on the `checkpoints`
    /// table to get the `min_tx_sequence_number` for that lower bound.
    #[allow(clippy::type_complexity)]
    pub(crate) async fn query(db: &Db) -> Result<Option<Watermark>, Error> {
        let (reader_lo_to_tx, cp_hi_to_timestamp) = diesel::alias!(
            checkpoints as reader_lo_to_tx,
            checkpoints as cp_hi_to_timestamp
        );

        let Some(result): Option<(i64, i64, i64, i64, i64, Option<i64>)> = db
            .execute(move |conn| {
                async move {
                    conn.result(move || {
                        watermarks::table
                            // Join for reader_lo -> checkpoints (as cp_reader) to get min_tx_sequence_number
                            .inner_join(
                                reader_lo_to_tx.on(watermarks::reader_lo
                                    .eq(reader_lo_to_tx.field(checkpoints::sequence_number))),
                            )
                            // Join for checkpoint_hi_inclusive -> checkpoints (as cp_hi) to get
                            // timestamp_ms of cp_hi
                            .inner_join(
                                cp_hi_to_timestamp.on(watermarks::checkpoint_hi_inclusive
                                    .eq(cp_hi_to_timestamp.field(checkpoints::sequence_number))),
                            )
                            .filter(watermarks::pipeline.eq("checkpoints"))
                            .select((
                                watermarks::epoch_hi_inclusive,
                                cp_hi_to_timestamp.field(checkpoints::timestamp_ms),
                                watermarks::checkpoint_hi_inclusive,
                                watermarks::tx_hi,
                                watermarks::reader_lo,
                                reader_lo_to_tx.field(checkpoints::min_tx_sequence_number),
                            ))
                    })
                    .await
                    .optional()
                }
                .scope_boxed()
            })
            .await
            .map_err(|e| Error::Internal(format!("Failed to fetch watermark data: {e}")))?
        else {
            // An empty response from the db is valid when indexer has not committed data to the db
            // yet.
            return Ok(None);
        };

        if let (epoch, hi_cp_timestamp_ms, hi_cp, hi_tx, lo_cp, Some(lo_tx)) = result {
            Ok(Some(Watermark {
                hi_cp: hi_cp as u64,
                hi_cp_timestamp_ms: hi_cp_timestamp_ms as u64,
                hi_tx: hi_tx as u64,
                epoch: epoch as u64,
                lo_cp: lo_cp as u64,
                lo_tx: lo_tx as u64,
            }))
        } else {
            Err(Error::Internal(
                "Expected entry for tx lower bound and min_tx_sequence_number to be non-null"
                    .to_string(),
            ))
        }
    }
}

impl ChainIdentifierLock {
    pub(crate) async fn read(&self) -> ChainIdentifier {
        let w = self.0.read().await;
        w.0.into()
    }
}
