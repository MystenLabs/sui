// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use axum::extract::{Query, State};
use std::time::Duration;
use std::time::SystemTime;

use crate::Result;
use crate::RpcService;

/// The largest gap, in checkpoints, between the latest checkpoint and the
/// highest indexed checkpoint still considered healthy.
///
/// The embedded indexer follows the tip asynchronously, so the highest indexed
/// checkpoint always trails the reported tip by a little (roughly the indexer's
/// snapshot window). A gap larger than this means the index is still
/// backfilling -- e.g. the ledger-history cohort after a restore -- and cannot
/// serve complete reads even though the raw tip is readable.
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
    /// only considered healthy once its indexes have caught up to within
    /// `MAX_HEALTHY_INDEX_LAG` checkpoints of the latest checkpoint. When
    /// indexing is disabled this check is skipped.
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

        // When indexing is enabled, the node is only healthy once its indexes
        // have kept up with the latest checkpoint. The embedded indexer follows
        // the tip asynchronously and its ledger-history cohort backfills
        // independently after a restore, so a lagging index cannot serve
        // complete reads even though the raw tip is readable. A node without an
        // index surface (indexing disabled) skips this check.
        if let Some(indexes) = self.reader.inner().indexes() {
            let highest_indexed = indexes.get_highest_indexed_checkpoint_seq_number()?;

            if !index_caught_up(
                latest.sequence_number,
                highest_indexed,
                MAX_HEALTHY_INDEX_LAG,
            ) {
                return Err(anyhow::anyhow!(
                    "the rpc index is not caught up to within {MAX_HEALTHY_INDEX_LAG} \
                     checkpoints of the latest checkpoint"
                )
                .into());
            }
        }

        Ok(())
    }
}

/// Whether the highest indexed checkpoint is close enough to the latest
/// checkpoint to be considered healthy.
///
/// `highest_indexed` is `None` when the index has not committed any checkpoint
/// yet, which is never healthy. A frontier at or past the tip (which should not
/// happen, as the tip is itself index-bounded) saturates to a zero lag.
fn index_caught_up(latest_seq: u64, highest_indexed: Option<u64>, max_lag: u64) -> bool {
    match highest_indexed {
        Some(indexed) => latest_seq.saturating_sub(indexed) <= max_lag,
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

    // The index has not committed any checkpoint yet: never healthy.
    #[test]
    fn not_caught_up_when_unindexed() {
        assert!(!index_caught_up(100, None, MAX_HEALTHY_INDEX_LAG));
        assert!(!index_caught_up(0, None, MAX_HEALTHY_INDEX_LAG));
    }

    // Within (or at) the allowed lag: healthy.
    #[test]
    fn caught_up_within_lag() {
        assert!(index_caught_up(100, Some(100), 60)); // no lag
        assert!(index_caught_up(100, Some(40), 60)); // exactly at the bound
        assert!(index_caught_up(100, Some(41), 60)); // inside the bound
    }

    // Beyond the allowed lag (e.g. a ledger-history backfill): unhealthy.
    #[test]
    fn not_caught_up_beyond_lag() {
        assert!(!index_caught_up(100, Some(39), 60)); // one past the bound
        assert!(!index_caught_up(1_000, Some(0), 60)); // backfilling from genesis
    }

    // A frontier at or past the tip saturates to zero lag rather than
    // underflowing.
    #[test]
    fn caught_up_when_index_at_or_past_tip() {
        assert!(index_caught_up(100, Some(200), 60));
    }
}
