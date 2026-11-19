// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use axum::extract::{Query, State};
use std::time::Duration;
use std::time::SystemTime;
use sui_types::storage::error::Result as StorageResult;

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
/// restore and is excluded from this gap, so by default a node is healthy as
/// soon as its live-object reads are caught up; callers that also need
/// history caught up opt in with `max_history_lag_seconds` (see [`Threshold`]).
const MAX_HEALTHY_INDEX_LAG: u64 = 60;

impl RpcService {
    /// Perform a simple health check on the service.
    ///
    /// `threshold_seconds` is the threshold, or delta, between the server's
    /// system time and the timestamp in the most recently executed checkpoint
    /// for which the server is considered to be healthy. If not provided, the
    /// server's tip is not subject to a staleness check.
    ///
    /// Independent of the threshold, when indexing is enabled the server is
    /// only considered healthy once its live-object indexes have caught up to
    /// within `MAX_HEALTHY_INDEX_LAG` checkpoints of the latest executed
    /// checkpoint.
    ///
    /// `max_history_lag_seconds`, when provided, additionally requires every
    /// index pipeline -- including the ledger-history cohort, which backfills
    /// separately after a restore -- to have caught up to within that many
    /// seconds of chain time of the live-object index: the chain timestamps
    /// of the checkpoints their watermarks have reached are compared, so the
    /// bound means the same on every network and at every checkpoint rate. A
    /// history index with no watermark yet is never healthy, and neither is a
    /// node without rpc indexes: its reported tip is its executed checkpoint,
    /// which can run ahead of what indexed replicas have reached.
    ///
    /// When indexing is disabled and `max_history_lag_seconds` is not
    /// provided, the index checks are skipped.
    pub fn health_check(
        &self,
        threshold_seconds: Option<u32>,
        max_history_lag_seconds: Option<u32>,
    ) -> Result<()> {
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
        // backfills independently after a restore and is only checked when the
        // caller passes `max_history_lag_seconds`: gating on it by default
        // would report a node unhealthy for the whole backfill even though its
        // live-object reads are already caught up. The executed tip is read
        // unbounded (rather than via `get_latest_checkpoint`, which is itself
        // bounded to the live frontier) so a stalled live indexer, whose
        // frontier falls behind ongoing execution, is still detected.
        //
        // The reported tip is bounded to the live frontier, so a replica
        // whose history trails it advertises checkpoints its ledger-history
        // reads cannot serve yet. Measure the history gap against that
        // frontier rather than the executed tip, so the live index's own
        // (separately bounded) lag behind execution is not counted twice. A
        // node without an index surface (indexing disabled) reports its
        // executed checkpoint as the tip, so it fails the history check
        // whenever `max_history_lag_seconds` is set and skips the index checks
        // otherwise.
        let history_ok = if let Some(indexes) = self.reader.inner().indexes() {
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

            history_caught_up(
                max_history_lag_seconds,
                Some(IndexTimestamps {
                    read_live_indexed_ms: &|| {
                        indexes.get_highest_live_indexed_checkpoint_timestamp_ms()
                    },
                    read_all_indexed_ms: &|| indexes.get_highest_indexed_checkpoint_timestamp_ms(),
                }),
            )?
        } else {
            history_caught_up(max_history_lag_seconds, None)?
        };

        if !history_ok {
            return Err(anyhow::anyhow!(
                "`max_history_lag_seconds` is set and the ledger-history indexes are absent or \
                 not caught up to within that many seconds of the live-object index"
            )
            .into());
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

/// The rpc index surface as the history check sees it: the chain timestamps,
/// in milliseconds, of the checkpoints its watermarks have reached.
struct IndexTimestamps<'a> {
    /// Reads the live-object frontier's timestamp; `None` before the live
    /// cohort's first commit.
    read_live_indexed_ms: &'a dyn Fn() -> StorageResult<Option<u64>>,
    /// Reads the timestamp of the lowest watermark across all pipelines (live
    /// and ledger-history); `None` while any pipeline has no watermark yet.
    read_all_indexed_ms: &'a dyn Fn() -> StorageResult<Option<u64>>,
}

/// Whether every index pipeline (live and ledger-history) has caught up to
/// within `max_lag_seconds` of chain time of the live-object frontier.
///
/// With no `max_lag_seconds` the history cohort is not checked: the result is
/// `true` and nothing is read. Otherwise `indexes` is `None` for a node without
/// rpc indexes, which is never healthy. The all-pipelines minimum includes the
/// live pipelines, but it is read after the live frontier, so a live cohort
/// that advanced in between can leave it slightly above; that counts as zero
/// lag. A `None` minimum (in practice a history pipeline after a restore,
/// before its seed) is never healthy, and neither is a missing live frontier.
///
/// A timestamp of 0 means the watermark was written without one, not a real
/// chain time: the embedded restore anchors the live cohort with only its
/// checkpoint set, so until the live indexer commits its first checkpoint both
/// timestamps read 0 while history may still sit at its seed far below. A 0 on
/// either side is therefore never healthy either.
fn history_caught_up(
    max_lag_seconds: Option<u32>,
    indexes: Option<IndexTimestamps<'_>>,
) -> StorageResult<bool> {
    let Some(max_lag_seconds) = max_lag_seconds else {
        return Ok(true);
    };
    let Some(indexes) = indexes else {
        return Ok(false);
    };
    let max_lag_ms = u64::from(max_lag_seconds) * 1_000;
    let live = (indexes.read_live_indexed_ms)()?;
    let all = (indexes.read_all_indexed_ms)()?;
    Ok(match (live, all) {
        (Some(live), Some(all)) if live > 0 && all > 0 => live.saturating_sub(all) <= max_lag_ms,
        _ => false,
    })
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct Threshold {
    /// The threshold, or delta, between the server's system time and the timestamp in the most
    /// recently executed checkpoint for which the server is considered to be healthy.
    ///
    /// If not provided, the server will be considered healthy if it can simply fetch the latest
    /// checkpoint from its store and, when indexing is enabled, its live-object indexes have
    /// caught up to it.
    pub threshold_seconds: Option<u32>,

    /// The largest gap, in seconds of chain time, between the live-object index and the slowest
    /// index pipeline (in practice the ledger-history indexes, which backfill separately after a
    /// restore) for which the server is considered to be healthy. The gap is measured between
    /// the timestamps of the checkpoints their watermarks have reached. A history index with no
    /// watermark yet is unhealthy, and so is a node without rpc indexes (indexing disabled),
    /// whose reported tip is its executed checkpoint.
    ///
    /// If not provided, ledger-history progress and the presence of rpc indexes do not affect
    /// health.
    pub max_history_lag_seconds: Option<u32>,
}

pub async fn health(
    Query(Threshold {
        threshold_seconds,
        max_history_lag_seconds,
    }): Query<Threshold>,
    State(state): State<RpcService>,
) -> impl axum::response::IntoResponse {
    match state.health_check(threshold_seconds, max_history_lag_seconds) {
        Ok(()) => (axum::http::StatusCode::OK, "up"),
        Err(_) => (axum::http::StatusCode::SERVICE_UNAVAILABLE, "down"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sui_types::storage::error::Error as StorageError;

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

    /// Evaluate the history check on a node with rpc indexes, with fixed
    /// live-frontier and all-pipelines timestamps in milliseconds.
    fn history_ok(max_lag_seconds: Option<u32>, live_ms: Option<u64>, all_ms: Option<u64>) -> bool {
        let read_live = || Ok(live_ms);
        let read_all = || Ok(all_ms);
        let indexes = IndexTimestamps {
            read_live_indexed_ms: &read_live,
            read_all_indexed_ms: &read_all,
        };
        history_caught_up(max_lag_seconds, Some(indexes)).unwrap()
    }

    // Without `max_history_lag_seconds` the history cohort does not affect
    // health and no index timestamp is read, so neither a mid-backfill history
    // cohort nor a failing read can take the node out of rotation. A node
    // without rpc indexes is likewise unaffected.
    #[test]
    fn history_ignored_when_unset() {
        let read = || -> StorageResult<Option<u64>> {
            panic!("index timestamp read with max_history_lag_seconds unset")
        };
        let indexes = IndexTimestamps {
            read_live_indexed_ms: &read,
            read_all_indexed_ms: &read,
        };
        assert!(history_caught_up(None, Some(indexes)).unwrap());
        assert!(history_caught_up(None, None).unwrap());
    }

    // A node without rpc indexes reports its executed checkpoint as the tip,
    // which can run ahead of what indexed replicas have reached: unhealthy
    // whenever `max_history_lag_seconds` is set, however large.
    #[test]
    fn history_not_caught_up_without_indexes() {
        assert!(!history_caught_up(Some(u32::MAX), None).unwrap());
        assert!(!history_caught_up(Some(0), None).unwrap());
    }

    // Every pipeline is within (or at) the allowed lag of the live frontier:
    // healthy. The bound is in seconds and the timestamps in milliseconds.
    #[test]
    fn history_caught_up_within_lag() {
        assert!(history_ok(Some(10), Some(100_000), Some(100_000))); // no lag
        assert!(history_ok(Some(10), Some(100_000), Some(90_000))); // exactly at the bound
        assert!(history_ok(Some(10), Some(100_000), Some(95_000))); // inside the bound
    }

    // The all-pipelines minimum is read after the live frontier, so the live
    // cohort can advance in between and leave it above the live timestamp:
    // that saturates to zero lag rather than underflowing.
    #[test]
    fn history_caught_up_when_above_live() {
        assert!(history_ok(Some(10), Some(100_000), Some(100_500)));
        assert!(history_ok(Some(0), Some(100_000), Some(100_500)));
    }

    // History is backfilling far behind the live frontier (e.g. after a
    // snapshot restore): unhealthy.
    #[test]
    fn history_not_caught_up_beyond_lag() {
        assert!(!history_ok(Some(10), Some(100_000), Some(89_999))); // one ms past the bound
        assert!(!history_ok(Some(10), Some(1_700_000_000_000), Some(5_000))); // mid-backfill
    }

    // A history pipeline with no watermark yet makes the all-pipelines
    // timestamp `None`, and a live cohort that has not committed makes the
    // live timestamp `None`: unhealthy, however large the allowed lag.
    #[test]
    fn history_not_caught_up_when_uncommitted() {
        assert!(!history_ok(Some(u32::MAX), Some(100_000), None));
        assert!(!history_ok(Some(u32::MAX), None, Some(100_000)));
        assert!(!history_ok(Some(u32::MAX), None, None));
    }

    // Right after the embedded restore the live cohort's watermarks carry
    // only the restore checkpoint, so the live and all-pipelines timestamps
    // both read 0 while history sits at its seed: unhealthy, however large
    // the allowed lag, until the live indexer commits a real timestamp.
    #[test]
    fn history_not_caught_up_without_recorded_timestamps() {
        assert!(!history_ok(Some(u32::MAX), Some(0), Some(0)));
        assert!(!history_ok(Some(u32::MAX), Some(100_000), Some(0)));
        assert!(!history_ok(Some(u32::MAX), Some(0), Some(100_000)));
    }

    // A failure reading either timestamp propagates rather than being read as
    // healthy.
    #[test]
    fn history_read_error_propagates() {
        let ok = || -> StorageResult<Option<u64>> { Ok(Some(100_000)) };
        let fail = || -> StorageResult<Option<u64>> { Err(StorageError::custom("read failed")) };
        let all_fails = IndexTimestamps {
            read_live_indexed_ms: &ok,
            read_all_indexed_ms: &fail,
        };
        assert!(history_caught_up(Some(10), Some(all_fails)).is_err());
        let live_fails = IndexTimestamps {
            read_live_indexed_ms: &fail,
            read_all_indexed_ms: &ok,
        };
        assert!(history_caught_up(Some(10), Some(live_fails)).is_err());
    }
}
