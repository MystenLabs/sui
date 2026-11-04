// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! The minimum batch a collector waits for before it cuts its pending rows into a batch (see
//! [`CommitterConfig::min_batch_rows`] and [`CommitterConfig::max_batch_delay_ms`]).

use std::future;
use std::time::Duration;

use tokio::time::Instant;
use tokio::time::sleep_until;

use crate::pipeline::CommitterConfig;

/// Tracks whether a collector's pending rows have reached the minimum batch: at least
/// `min_batch_rows` rows are pending, or rows have been pending for `max_batch_delay`.
pub(super) struct BatchFloor {
    min_rows: usize,
    max_delay: Duration,

    /// When rows started to be pending, or `None` while nothing is pending. Cutting a batch that
    /// leaves rows behind does not move it, so no row waits longer than `max_delay`.
    pending_since: Option<Instant>,
}

impl BatchFloor {
    pub(super) fn new(config: &CommitterConfig) -> Self {
        Self {
            min_rows: config.min_batch_rows,
            max_delay: config.max_batch_delay(),
            pending_since: None,
        }
    }

    /// Record whether anything is pending, after the collector's pending set changed.
    pub(super) fn set_pending(&mut self, anything_pending: bool) {
        if !anything_pending {
            self.pending_since = None;
        } else if self.pending_since.is_none() {
            self.pending_since = Some(Instant::now());
        }
    }

    /// Whether the pending rows, `pending_rows` of them, may be cut into a batch now.
    pub(super) fn is_reached(&self, pending_rows: usize) -> bool {
        let Some(since) = self.pending_since else {
            return false;
        };

        pending_rows >= self.min_rows || Instant::now() >= since + self.max_delay
    }

    /// Resolves once rows have been pending for `max_delay`. Never resolves while nothing is
    /// pending.
    pub(super) async fn delay_elapsed(&self) {
        match self.pending_since {
            Some(since) => sleep_until(since + self.max_delay).await,
            None => future::pending().await,
        }
    }
}
