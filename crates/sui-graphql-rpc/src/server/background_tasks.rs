// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::data::Db;
use crate::metrics::Metrics;
use crate::types::epoch::Epoch;
use async_graphql::ServerError;
use std::sync::Arc;
use tokio::sync::Notify;
use tokio_util::sync::CancellationToken;
use tracing::{error, info};

/// Starts an infinite loop that periodically checks for an epoch change. If an epoch change is
/// detected, emits a signal and updates the latest epoch value.
pub(crate) async fn check_epoch_boundary(
    db: &Db,
    metrics: Metrics,
    sleep_duration: tokio::time::Duration,
    cancellation_token: CancellationToken,
    notify: Arc<Notify>,
) {
    let mut current_epoch = match Epoch::query_latest_at(db, None).await {
        Ok(epoch) => match epoch {
            Some(epoch) => epoch,
            None => {
                error!("No epoch found in the database");
                metrics.inc_errors(&[ServerError::new(
                    "No epoch found in the database".to_string(),
                    None,
                )]);
                return;
            }
        },
        Err(e) => {
            error!("{}", e);
            metrics.inc_errors(&[ServerError::new(e.to_string(), None)]);
            return;
        }
    };

    loop {
        tokio::select! {
            _ = cancellation_token.cancelled() => {
                info!("Shutdown signal received, terminating epoch boundary task");
                return;
            },
            _ = tokio::time::sleep(sleep_duration) => {
                let new_epoch = match Epoch::query_latest_at(db, None).await {
                    Ok(epoch) => match epoch {
                        Some(epoch) => epoch,
                        None => {
                            error!("No epoch found in the database");
                            metrics.inc_errors(&[ServerError::new(
                                "No epoch found in the database".to_string(),
                                None,
                            )]);
                            return;
                        }
                    },
                    Err(e) => {
                        error!("{}", e);
                        metrics.inc_errors(&[ServerError::new(e.to_string(), None)]);
                        return;
                    }
                };

                if current_epoch.stored.epoch < new_epoch.stored.epoch {
                    info!("Epoch boundary detected: {}", new_epoch.stored.epoch);
                    current_epoch = new_epoch;
                    notify.notify_waiters();
                }
            }
        }
    }
}

/// Simple implementation for a listener that waits for an epoch boundary signal.
pub(crate) async fn epoch_boundary_listener(
    cancellation_token: CancellationToken,
    notification: Arc<Notify>,
) {
    loop {
        tokio::select! {
        _ = cancellation_token.cancelled() => {
            info!("Shutdown signal received, terminating epoch boundary task");
            return;
        },
        _ = notification.notified() => {
                info!("Epoch boundary signal received");
            }
        }
    }
}
