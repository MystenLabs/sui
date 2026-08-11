// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

use sui_indexer_alt_framework_store_traits::CommitterWatermark;
use tokio::sync::mpsc;
use tracing::error;
use tracing::info;

use crate::bigtable::client::BigTableClient;

use super::BitmapIndexMetrics;
use super::COMMIT_RETRY_BACKOFF;

/// Generation task → BigTable-writer request. The writer always prefers the
/// newest request it has seen; older successful writes are safe but stale
/// failed retries yield to newer checkpoints.
pub(super) struct Commit {
    pub(super) watermark: CommitterWatermark,
    /// Replay floor for the bucket containing `watermark.tx_hi`. Persisted
    /// alongside the watermark so `init_watermark` on the next restart can
    /// clamp back and rebuild the active bucket.
    /// `0` when no bucket transition has been observed yet.
    pub(super) bucket_start_cp: u64,
    /// Used only for `watermark_lag_ms` once this request is durable.
    pub(super) framework_commit_time: std::time::Instant,
}

/// Dedicated task that owns committer-watermark writes. It coalesces pending
/// requests to the newest checkpoint before each write attempt and retries
/// failures with backoff.
pub(super) struct WatermarkWriter {
    pub(super) pipeline: &'static str,
    pub(super) client: BigTableClient,
    pub(super) commit_rx: mpsc::Receiver<Commit>,
    pub(super) metrics: Arc<BitmapIndexMetrics>,
}

impl WatermarkWriter {
    pub(super) async fn run(mut self) {
        info!(self.pipeline, "Bitmap watermark commit loop started");

        let mut pending = None;
        loop {
            let req = match pending.take() {
                Some(req) => req,
                None => {
                    let Some(req) = self.commit_rx.recv().await else {
                        break;
                    };
                    req
                }
            };
            let req = drain_to_latest(&mut self.commit_rx, req);

            if let Err(e) = self.write_watermark(&req).await {
                error!(self.pipeline, %e, "set_committer_watermark_cells failed; retrying");
                pending = Some(req);
                tokio::time::sleep(COMMIT_RETRY_BACKOFF).await;
                continue;
            }

            self.metrics
                .watermark_lag_ms
                .observe(req.framework_commit_time.elapsed().as_secs_f64() * 1000.0);
        }

        info!(self.pipeline, "Bitmap watermark commit loop exiting");
    }

    async fn write_watermark(&mut self, req: &Commit) -> anyhow::Result<()> {
        self.client
            .set_committer_watermark_cells(self.pipeline, &req.watermark, Some(req.bucket_start_cp))
            .await?;
        // CAS result is ignored: if `chi` didn't advance, the row is already at
        // or ahead of this write (idempotent replay / stale retry), which is fine.
        Ok(())
    }
}

/// Drain any pending commits from `commit_rx` and return the one with the
/// highest checkpoint among `req` and the drained values. Older requests are
/// safe to skip: the watermark write is monotone (CAS-guarded) and the writer
/// only needs to advance to the newest known checkpoint.
fn drain_to_latest(commit_rx: &mut mpsc::Receiver<Commit>, mut req: Commit) -> Commit {
    while let Ok(next) = commit_rx.try_recv() {
        if next.watermark.checkpoint_hi_inclusive > req.watermark.checkpoint_hi_inclusive {
            req = next;
        }
    }
    req
}

#[cfg(test)]
mod tests {
    use std::time::Instant;

    use sui_indexer_alt_framework_store_traits::CommitterWatermark;

    use super::*;

    fn commit_with_bucket_start(cp: u64, bucket_start_cp: u64) -> Commit {
        Commit {
            watermark: CommitterWatermark {
                epoch_hi_inclusive: 0,
                checkpoint_hi_inclusive: cp,
                tx_hi: cp * 10,
                timestamp_ms_hi_inclusive: cp * 1_000,
            },
            bucket_start_cp,
            framework_commit_time: Instant::now(),
        }
    }

    fn commit(cp: u64) -> Commit {
        commit_with_bucket_start(cp, 0)
    }

    /// `drain_to_latest` must pick the highest-cp pending request and leave
    /// the channel empty. Older requests can be safely skipped because the
    /// watermark write is CAS-guarded and monotone.
    #[tokio::test]
    async fn drain_to_latest_picks_highest_pending_cp() {
        let (tx, mut rx) = mpsc::channel(8);
        // Note: the worker has already consumed the *first* request before
        // calling drain_to_latest — that's the `req` argument. Simulate with
        // cp=1 as the already-consumed request, then push 3 and 2.
        tx.send(commit(3)).await.unwrap();
        tx.send(commit(2)).await.unwrap();

        let chosen = drain_to_latest(&mut rx, commit(1));

        assert_eq!(chosen.watermark.checkpoint_hi_inclusive, 3);
        assert!(rx.try_recv().is_err(), "channel must be drained");
    }

    /// Empty channel must leave the input request unchanged.
    #[tokio::test]
    async fn drain_to_latest_empty_channel_returns_input() {
        let (_tx, mut rx) = mpsc::channel::<Commit>(1);
        let chosen = drain_to_latest(&mut rx, commit(7));
        assert_eq!(chosen.watermark.checkpoint_hi_inclusive, 7);
    }

    /// Already-newer input must be preserved when the queue contains older
    /// requests (the worker may have already coalesced past them).
    #[tokio::test]
    async fn drain_to_latest_keeps_input_when_pending_are_older() {
        let (tx, mut rx) = mpsc::channel(8);
        tx.send(commit(2)).await.unwrap();
        tx.send(commit(1)).await.unwrap();

        let chosen = drain_to_latest(&mut rx, commit(5));

        assert_eq!(chosen.watermark.checkpoint_hi_inclusive, 5);
    }

    /// `bucket_start_cp` must travel with the chosen `Commit`. Coalescing picks
    /// the highest-cp request and must not mix in another request's
    /// `bucket_start_cp`, since the persisted `b` cell has to remain paired
    /// with its watermark for the post-restart clamp to work.
    #[tokio::test]
    async fn drain_to_latest_pairs_bucket_start_cp_with_chosen_cp() {
        let (tx, mut rx) = mpsc::channel(8);
        tx.send(commit_with_bucket_start(2, 200)).await.unwrap();
        tx.send(commit_with_bucket_start(5, 500)).await.unwrap();
        tx.send(commit_with_bucket_start(3, 300)).await.unwrap();

        let chosen = drain_to_latest(&mut rx, commit_with_bucket_start(1, 100));

        assert_eq!(chosen.watermark.checkpoint_hi_inclusive, 5);
        assert_eq!(chosen.bucket_start_cp, 500);
    }
}
