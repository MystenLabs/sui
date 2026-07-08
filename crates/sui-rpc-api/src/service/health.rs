// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use axum::extract::{Query, State};
use std::time::Duration;
use std::time::SystemTime;

use crate::Result;
use crate::RpcService;

/// The largest gap, in checkpoints, between the latest executed checkpoint and
/// the highest checkpoint the live-object index has committed while still
/// considered healthy.
///
/// The embedded indexer follows the tip asynchronously, so the live-object
/// index always trails the executed tip by a little (roughly the indexer's
/// snapshot window). A gap larger than this means the live index has fallen
/// behind -- e.g. the indexer has stalled -- and cannot serve current
/// live-object reads. The ledger-history cohort backfills separately after a
/// restore and is deliberately excluded from this gap, so a node is healthy as
/// soon as its live-object reads are caught up.
const MAX_HEALTHY_INDEX_LAG: u64 = 60;

impl RpcService {
    /// Perform a simple health check on the service.
    ///
    /// The threshold, or delta, between the server's system time and the
    /// timestamp in the most recently executed checkpoint for which the server
    /// is considered to be healthy. If not provided, the server's tip is not
    /// subject to a staleness check.
    ///
    /// Independent of the threshold, when indexing is enabled the server is
    /// only considered healthy once its live-object indexes have caught up to
    /// within `MAX_HEALTHY_INDEX_LAG` checkpoints of the latest executed
    /// checkpoint. When indexing is disabled this check is skipped.
    pub fn health_check(&self, threshold_seconds: Option<u32>) -> Result<()> {
        let latest = self.reader.inner().get_latest_checkpoint()?;

        // If we have a provided threshold, check that it's close to the current
        // time.
        if let Some(threshold_seconds) = threshold_seconds {
            let latest_chain_time = latest.timestamp();

            let threshold = SystemTime::now() - Duration::from_secs(threshold_seconds as u64);

            if latest_chain_time < threshold {
                return Err(anyhow::anyhow!(
                    "The latest checkpoint timestamp is less than the provided threshold"
                )
                .into());
            }
        }

        // When indexing is enabled, the node is only healthy once its
        // live-object indexes (owned objects, types, balances) have kept up
        // with the executed tip. Those indexes are restored to the tip and
        // follow it, so a healthy node's live frontier trails execution by at
        // most the indexer's snapshot window. The ledger-history cohort
        // backfills independently after a restore and is deliberately excluded:
        // gating on it would report a node unhealthy for the whole backfill
        // even though its live-object reads are already caught up. The executed
        // tip is read unbounded (rather than via `get_latest_checkpoint`, which
        // is itself bounded to the live frontier) so a stalled live indexer,
        // whose frontier falls behind ongoing execution, is still detected. A
        // node without an index surface (indexing disabled) skips this check.
        if let Some(indexes) = self.reader.inner().indexes() {
            let executed = self
                .reader
                .inner()
                .get_highest_executed_checkpoint_seq_number()?;
            let highest_live_indexed = indexes.get_highest_live_indexed_checkpoint_seq_number()?;

            if !index_caught_up(executed, highest_live_indexed, MAX_HEALTHY_INDEX_LAG) {
                return Err(anyhow::anyhow!(
                    "the live-object index is not caught up to within {MAX_HEALTHY_INDEX_LAG} \
                     checkpoints of the latest executed checkpoint"
                )
                .into());
            }
        }

        Ok(())
    }
}

/// Whether the highest live-indexed checkpoint is close enough to the executed
/// tip to be considered healthy.
///
/// `highest_live_indexed` is `None` when the live-object index has not committed
/// any checkpoint yet, which is never healthy. The live frontier never runs
/// ahead of execution (it indexes executed checkpoints), but an equal frontier
/// saturates to a zero lag rather than underflowing.
fn index_caught_up(executed_seq: u64, highest_live_indexed: Option<u64>, max_lag: u64) -> bool {
    match highest_live_indexed {
        Some(indexed) => executed_seq.saturating_sub(indexed) <= max_lag,
        None => false,
    }
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct Threshold {
    /// The threshold, or delta, between the server's system time and the timestamp in the most
    /// recently executed checkpoint for which the server is considered to be healthy.
    ///
    /// If not provided, the server will be considered healthy if it can simply fetch the latest
    /// checkpoint from its store and, when indexing is enabled, its indexes have caught up to it.
    pub threshold_seconds: Option<u32>,
}

pub async fn health(
    Query(Threshold { threshold_seconds }): Query<Threshold>,
    State(state): State<RpcService>,
) -> impl axum::response::IntoResponse {
    match state.health_check(threshold_seconds) {
        Ok(()) => (axum::http::StatusCode::OK, "up"),
        Err(_) => (axum::http::StatusCode::SERVICE_UNAVAILABLE, "down"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // The live-object index has not committed any checkpoint yet: never
    // healthy.
    #[test]
    fn not_caught_up_when_unindexed() {
        assert!(!index_caught_up(100, None, MAX_HEALTHY_INDEX_LAG));
        assert!(!index_caught_up(0, None, MAX_HEALTHY_INDEX_LAG));
    }

    // The live frontier is within (or at) the allowed lag of the executed tip:
    // healthy.
    #[test]
    fn caught_up_within_lag() {
        assert!(index_caught_up(100, Some(100), 60)); // no lag
        assert!(index_caught_up(100, Some(40), 60)); // exactly at the bound
        assert!(index_caught_up(100, Some(41), 60)); // inside the bound
    }

    // The live frontier trails the executed tip by more than the allowed lag
    // (e.g. the live indexer stalled while execution advanced): unhealthy. A
    // lagging ledger-history backfill does not reach this path -- it is not part
    // of the live frontier.
    #[test]
    fn not_caught_up_beyond_lag() {
        assert!(!index_caught_up(100, Some(39), 60)); // one past the bound
        assert!(!index_caught_up(1_000, Some(0), 60)); // live index far behind
    }

    // A live frontier level with the executed tip saturates to zero lag rather
    // than underflowing.
    #[test]
    fn caught_up_when_index_at_tip() {
        assert!(index_caught_up(100, Some(100), 60));
        assert!(index_caught_up(100, Some(200), 60));
    }
}
