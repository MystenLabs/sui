// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::data::{Db, DbConnection, QueryExecutor};
use crate::error::Error;
use crate::metrics::Metrics;
use crate::types::chain_identifier::ChainIdentifier;
use async_graphql::ServerError;
use diesel::{
    query_dsl::positional_order_dsl::PositionalOrderDsl, CombineDsl, ExpressionMethods,
    OptionalExtension, QueryDsl,
};
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
    /// The inclusive checkpoint upper-bound for the query.
    pub hi_cp: u64,
    /// The timestamp of the inclusive upper-bound checkpoint for the query.
    pub hi_cp_timestamp_ms: u64,
    /// The current epoch.
    pub epoch: u64,
    /// Smallest queryable checkpoint - checkpoints below this value are pruned.
    pub lo_cp: u64,
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
                    let Watermark { lo_cp, lo_tx, hi_cp, hi_cp_timestamp_ms, epoch } = match Watermark::query(&self.db).await {
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
            epoch: w.epoch,
            lo_cp: w.lo_cp,
            lo_tx: w.lo_tx,
        }
    }

    #[allow(clippy::type_complexity)]
    pub(crate) async fn query(db: &Db) -> Result<Option<Watermark>, Error> {
        use checkpoints::dsl;
        let Some(result): Option<Vec<(i64, i64, i64, Option<i64>)>> = db
            .execute(move |conn| {
                async {
                    conn.results(move || {
                        let min_cp = dsl::checkpoints
                            .select((
                                dsl::sequence_number,
                                dsl::timestamp_ms,
                                dsl::epoch,
                                dsl::min_tx_sequence_number,
                            ))
                            .order_by(dsl::sequence_number.asc())
                            .limit(1);

                        let max_cp = dsl::checkpoints
                            .select((
                                dsl::sequence_number,
                                dsl::timestamp_ms,
                                dsl::epoch,
                                dsl::min_tx_sequence_number,
                            ))
                            .order_by(dsl::sequence_number.desc())
                            .limit(1);

                        // Order by sequence_number, which is in the 1st position
                        min_cp.union_all(max_cp).positional_order_by(1)
                    })
                    .await
                    .optional()
                }
                .scope_boxed()
            })
            .await
            .map_err(|e| Error::Internal(format!("Failed to fetch watermark data: {e}")))?
        else {
            // An empty response from the db is valid when indexer has not written any checkpoints
            // to the db yet.
            return Ok(None);
        };

        let (lo_cp, lo_tx) = if let Some((cp, _, _, Some(tx))) = result.first() {
            (cp, tx)
        } else {
            return Err(Error::Internal(
                "Expected entry for tx lower bound and min_tx_sequence_number to be non-null"
                    .to_string(),
            ));
        };

        let (hi_cp, hi_cp_timestamp_ms, epoch) =
            if let Some((cp, timestamp_ms, epoch, _)) = result.last() {
                (cp, timestamp_ms, epoch)
            } else {
                return Err(Error::Internal(
                    "Expected entry for tx upper bound".to_string(),
                ));
            };

        Ok(Some(Watermark {
            hi_cp: *hi_cp as u64,
            hi_cp_timestamp_ms: *hi_cp_timestamp_ms as u64,
            epoch: *epoch as u64,
            lo_cp: *lo_cp as u64,
            lo_tx: *lo_tx as u64,
        }))
    }
}

impl ChainIdentifierLock {
    pub(crate) async fn read(&self) -> ChainIdentifier {
        let w = self.0.read().await;
        w.0.into()
    }
}
