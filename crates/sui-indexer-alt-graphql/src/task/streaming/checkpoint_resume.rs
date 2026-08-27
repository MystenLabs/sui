// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Per-subscriber catch-up scan for resumable subscriptions.
//!
//! [`scan_checkpoints`] yields past `ProcessedCheckpoint`s via the LedgerService `ListCheckpoints`
//! streaming API until it catches up to the live tip, then exits. The caller resubscribes to the
//! broadcast and consumes live items from there.
//!
//! ```text
//!   start_after                                       tip - threshold      network_tip
//!        │                                                  │                  │
//!        ▼                                                  ▼                  ▼
//!   ─────[───── ListCheckpoints pages, sequential ─────]────[──── gap ──────────]
//!         (ascending, rate-capped)                            (caller bridges
//!                                                             via live broadcast)
//! ```
//!
//! `ListCheckpoints` reads a contiguous checkpoint range as one sequential scan, which is
//! substantially cheaper for a deep archival backfill than the equivalent fan-out of per-checkpoint
//! point reads. The per-subscriber catch-up rate is capped by `per_subscriber_scan_max_qps` so a
//! single backfill cannot monopolise shared kv-rpc throughput, and the `throttle` back-pressures
//! upstream paging when the subscriber drains slowly.

use std::sync::Arc;
use std::time::Duration;

use async_stream::stream;
use backoff::ExponentialBackoff;
use bytes::Bytes;
use futures::Stream;
use sui_indexer_alt_reader::alpha_ledger_grpc_reader::AlphaLedgerGrpcReader;
use sui_indexer_alt_reader::alpha_ledger_grpc_reader::StreamPage;
use sui_rpc::proto::sui::rpc::v2 as proto;
use tracing::warn;

use super::ProcessedCheckpoint;
use super::checkpoint_stream_task::SubscriptionBroadcast;
use super::checkpoint_stream_task::checkpoint_field_mask;
use super::checkpoint_stream_task::process_checkpoint;
use crate::config::SubscriptionConfig;
use crate::error::RpcError;

/// Proto checkpoint returned by `ListCheckpoints`.
type ProtoCheckpoint = proto::Checkpoint;

/// Wait before re-requesting when the scan has drained the indexer's current tip but has not yet
/// reached the live tip.
const BACKFILL_POLL_INTERVAL: Duration = Duration::from_millis(100);

/// A source of backfill checkpoint pages: one page of `ProtoCheckpoint`s starting at or after
/// `start_checkpoint`, resuming from `after` (an opaque `ListCheckpoints` resume cursor), at most
/// `limit` items in ascending order. Abstracted as a trait so the scan is unit-testable without a
/// live gRPC endpoint.
pub(crate) trait CheckpointPageFetcher {
    async fn list_checkpoints_page(
        &self,
        start_checkpoint: u64,
        after: Option<Bytes>,
        limit: usize,
    ) -> anyhow::Result<StreamPage<ProtoCheckpoint>>;
}

impl CheckpointPageFetcher for AlphaLedgerGrpcReader {
    async fn list_checkpoints_page(
        &self,
        start_checkpoint: u64,
        after: Option<Bytes>,
        limit: usize,
    ) -> anyhow::Result<StreamPage<ProtoCheckpoint>> {
        let mut options = proto::QueryOptions::default();
        options.limit = Some(limit as u32);
        options.after = after;
        options.ordering = Some(proto::Ordering::Ascending as i32);

        let mut request = proto::ListCheckpointsRequest::default();
        request.read_mask = Some(checkpoint_field_mask());
        request.start_checkpoint = Some(start_checkpoint);
        // Unfiltered: deliver every checkpoint in the range, so the server does one sequential scan
        // rather than an inverted-index lookup.
        request.filter = None;
        request.options = Some(options);

        self.list_checkpoints(request).await
    }
}

/// Retry policy for a transient `ListCheckpoints` page failure. Bounded so a persistently broken
/// upstream surfaces as an error to the subscriber rather than hanging.
fn scan_backoff() -> ExponentialBackoff {
    ExponentialBackoff {
        initial_interval: Duration::from_millis(100),
        max_interval: Duration::from_secs(1),
        max_elapsed_time: Some(Duration::from_secs(30)),
        ..Default::default()
    }
}

/// Yield `ProcessedCheckpoint`s from `start_after + 1` onward by paging `ListCheckpoints`, toward the
/// live tip. The caller pins a `handoff` near the tip and stops consuming this stream there (see
/// `subscribe`), so the `last_yielded >= network_tip()` exit is just an outer bound.
pub(super) fn scan_checkpoints<F: CheckpointPageFetcher + Clone + Send + 'static>(
    fetcher: F,
    broadcast: Arc<SubscriptionBroadcast>,
    start_after: u64,
    config: &SubscriptionConfig,
) -> impl Stream<Item = Result<Arc<ProcessedCheckpoint>, RpcError>> + 'static {
    let page_size = config.per_subscriber_scan_max_concurrent_fetches.max(1);
    let throttle_interval = Duration::from_secs(1) / config.per_subscriber_scan_max_qps.max(1);

    // `start_checkpoint` is a fixed range floor; the `after` cursor drives continuation once paging
    // has begun (mirroring how the transaction/event backfill pins its range and pages by cursor).
    let start_checkpoint = start_after + 1;

    let raw = stream! {
        let mut after: Option<Bytes> = None;
        let mut last_yielded = start_after;

        loop {
            // Outer bound: stop once caught up to the live tip. `subscribe` normally breaks earlier
            // at the handoff, so this mainly bounds a consumer that never pins one.
            if last_yielded >= broadcast.network_tip() {
                return;
            }

            let page = backoff::future::retry(scan_backoff(), || {
                let fetcher = fetcher.clone();
                let after = after.clone();
                async move {
                    fetcher
                        .list_checkpoints_page(start_checkpoint, after, page_size)
                        .await
                        .map_err(|e| {
                            warn!(error = ?e, "ListCheckpoints page failed, retrying");
                            backoff::Error::transient(e)
                        })
                }
            })
            .await;

            let page = match page {
                Ok(page) => page,
                Err(e) => {
                    yield Err(e.into());
                    return;
                }
            };

            if page.items.is_empty() {
                // Nothing new indexed past `last_yielded` yet; wait for the indexer, then re-request.
                tokio::time::sleep(BACKFILL_POLL_INTERVAL).await;
                continue;
            }

            after = page.last_cursor().cloned();
            for item in page.items {
                let processed = match process_checkpoint(item.payload) {
                    Ok(processed) => Arc::new(processed),
                    Err(e) => {
                        yield Err(e.into());
                        return;
                    }
                };
                last_yielded = processed.summary.sequence_number;
                yield Ok(processed);
            }
        }
    };

    tokio_stream::StreamExt::throttle(raw, throttle_interval)
}

#[cfg(test)]
mod tests {
    use futures::StreamExt;

    use super::*;
    use crate::task::streaming::test_utils::MockPageFetcher;
    use crate::task::streaming::test_utils::make_test_proto_checkpoint;
    use crate::task::streaming::test_utils::test_broadcast;

    #[tokio::test]
    async fn scan_exits_when_caught_up_to_tip() {
        let (tx, broadcast) = test_broadcast(/* first_live_checkpoint */ 1);
        for seq in 1..=5 {
            let processed = process_checkpoint(make_test_proto_checkpoint(seq)).unwrap();
            tx.send(Arc::new(processed)).ok();
        }
        assert_eq!(broadcast.network_tip(), 5);

        let fetcher = MockPageFetcher::success_for_range(1..=5);
        let stream = scan_checkpoints(fetcher, broadcast, 0, &SubscriptionConfig::default());
        let yielded: Vec<u64> = stream
            .map(|item| item.unwrap().summary.sequence_number)
            .collect()
            .await;
        assert_eq!(yielded, vec![1, 2, 3, 4, 5]);
    }

    #[tokio::test]
    async fn scan_completes_through_transient_page_errors() {
        let (tx, broadcast) = test_broadcast(/* first_live_checkpoint */ 1);
        for seq in 1..=3 {
            let processed = process_checkpoint(make_test_proto_checkpoint(seq)).unwrap();
            tx.send(Arc::new(processed)).ok();
        }
        assert_eq!(broadcast.network_tip(), 3);

        // The first two page fetches fail transiently; the scan retries and still yields 1..=3.
        let fetcher = MockPageFetcher::success_for_range(1..=3).with_transient_failures(2);
        let stream = scan_checkpoints(
            fetcher.clone(),
            broadcast,
            0,
            &SubscriptionConfig::default(),
        );
        let yielded: Vec<u64> = stream
            .map(|item| item.unwrap().summary.sequence_number)
            .collect()
            .await;
        assert_eq!(yielded, vec![1, 2, 3]);
        // Two failed attempts plus the successful one.
        assert!(
            fetcher.calls() >= 3,
            "expected retries, got {}",
            fetcher.calls()
        );
    }

    #[tokio::test]
    async fn scan_tracks_advancing_tip() {
        let (tx, broadcast) = test_broadcast(/* first_live_checkpoint */ 1);
        // Initial tip = 3.
        for seq in 1..=3 {
            let processed = process_checkpoint(make_test_proto_checkpoint(seq)).unwrap();
            tx.send(Arc::new(processed)).ok();
        }
        assert_eq!(broadcast.network_tip(), 3);

        // One page holds only the first three; the rest index as the tip advances.
        let fetcher = MockPageFetcher::success_for_range(1..=7).with_page_size(3);
        let stream = scan_checkpoints(
            fetcher,
            broadcast.clone(),
            0,
            &SubscriptionConfig::default(),
        );
        tokio::pin!(stream);

        // Drain the first three, then advance the tip to 7 before draining the rest. `network_tip()`
        // is read lazily per iteration, so the scan continues through 7.
        let mut yielded = Vec::new();
        for _ in 0..3 {
            yielded.push(
                stream
                    .next()
                    .await
                    .unwrap()
                    .unwrap()
                    .summary
                    .sequence_number,
            );
        }
        for seq in 4..=7 {
            let processed = process_checkpoint(make_test_proto_checkpoint(seq)).unwrap();
            tx.send(Arc::new(processed)).ok();
        }
        assert_eq!(broadcast.network_tip(), 7);
        while let Some(item) = stream.next().await {
            yielded.push(item.unwrap().summary.sequence_number);
        }
        assert_eq!(yielded, vec![1, 2, 3, 4, 5, 6, 7]);
    }

    #[tokio::test(start_paused = true)]
    async fn scan_respects_qps_cap() {
        let (tx, broadcast) = test_broadcast(/* first_live_checkpoint */ 1);
        for seq in 1..=3 {
            let processed = process_checkpoint(make_test_proto_checkpoint(seq)).unwrap();
            tx.send(Arc::new(processed)).ok();
        }

        // qps = 1 → at most one emission per second; 3 items can't all emit in under 2s.
        let config = SubscriptionConfig {
            per_subscriber_scan_max_qps: 1,
            ..SubscriptionConfig::default()
        };
        let fetcher = MockPageFetcher::success_for_range(1..=3);
        let stream = scan_checkpoints(fetcher, broadcast, 0, &config);
        let start = tokio::time::Instant::now();
        let yielded: Vec<u64> = stream
            .map(|item| item.unwrap().summary.sequence_number)
            .collect()
            .await;
        let elapsed = start.elapsed();

        assert_eq!(yielded, vec![1, 2, 3]);
        assert!(
            elapsed >= Duration::from_secs(2),
            "expected throttle to take >= 2s for 3 items at 1 qps, got {elapsed:?}",
        );
    }

    #[tokio::test]
    async fn scan_skips_when_start_after_at_or_past_tip() {
        let (tx, broadcast) = test_broadcast(/* first_live_checkpoint */ 1);
        for seq in 1..=5 {
            let processed = process_checkpoint(make_test_proto_checkpoint(seq)).unwrap();
            tx.send(Arc::new(processed)).ok();
        }
        assert_eq!(broadcast.network_tip(), 5);

        // start_after = tip → scan yields nothing.
        let fetcher = MockPageFetcher::success_for_range(1..=5);
        let stream = scan_checkpoints(
            fetcher,
            broadcast.clone(),
            5,
            &SubscriptionConfig::default(),
        );
        let yielded: Vec<u64> = stream
            .map(|item| item.unwrap().summary.sequence_number)
            .collect()
            .await;
        assert!(yielded.is_empty());

        // start_after > tip → scan still yields nothing.
        let fetcher = MockPageFetcher::success_for_range(1..=5);
        let stream = scan_checkpoints(fetcher, broadcast, 10, &SubscriptionConfig::default());
        let yielded: Vec<u64> = stream
            .map(|item| item.unwrap().summary.sequence_number)
            .collect()
            .await;
        assert!(yielded.is_empty());
    }
}
