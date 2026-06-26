// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Per-subscriber catch-up scan for resumable subscriptions.
//!
//! [`scan_checkpoints`] yields past `ProcessedCheckpoint`s via LedgerService until it
//! catches up to the live tip, then exits. The caller resubscribes to the broadcast and
//! consumes live items from there.
//!
//! ```text
//!   start_after                                       tip - threshold      network_tip
//!        │                                                  │                  │
//!        ▼                                                  ▼                  ▼
//!   ─────[fetch][fetch][fetch][fetch][fetch][fetch]─────────[──── gap ──────────]
//!         (concurrent, ordered emission, rate-capped)        (caller bridges
//!                                                             via live broadcast)
//! ```
//!
//! The per-subscriber catch-up rate is capped by `per_subscriber_scan_max_qps` so a single
//! backfill cannot monopolise shared kv-rpc throughput. When the subscriber drains slowly,
//! the throttle and `buffered` adapter naturally back-pressure upstream fetches.

use std::sync::Arc;
use std::time::Duration;

use futures::Stream;
use futures::StreamExt;
use futures::stream;

use super::ProcessedCheckpoint;
use super::checkpoint_stream_task::SubscriptionBroadcast;
use super::checkpoint_stream_task::checkpoint_field_mask;
use super::checkpoint_stream_task::process_checkpoint;
use super::gap_recovery::CheckpointFetcher;
use super::gap_recovery::fetch_one_with_retry;
use crate::config::SubscriptionConfig;
use crate::error::RpcError;

/// Yield `ProcessedCheckpoint`s from `start_after + 1` onward by reading LedgerService
/// concurrently, toward the live tip. The caller pins a `handoff` near the tip and stops
/// consuming this stream there (see `subscribe`), so the unfold's `last >= network_tip()` exit is
/// just an outer bound.
pub(super) fn scan_checkpoints<F: CheckpointFetcher + Clone + Send + 'static>(
    fetcher: F,
    broadcast: Arc<SubscriptionBroadcast>,
    start_after: u64,
    config: &SubscriptionConfig,
) -> impl Stream<Item = Result<Arc<ProcessedCheckpoint>, RpcError>> + 'static {
    let concurrency = config.per_subscriber_scan_max_concurrent_fetches;
    let throttle_interval = Duration::from_secs(1) / config.per_subscriber_scan_max_qps.max(1);

    // Stream of sequence numbers to scan. Stops once `last` has caught up to the live tip;
    // the caller then continues from the receiver it subscribed mid-scan.
    let seq_stream = stream::unfold(
        (start_after, broadcast),
        move |(last, broadcast)| async move {
            let tip = broadcast.network_tip();
            if last >= tip {
                None
            } else {
                let next = last + 1;
                Some((next, (next, broadcast)))
            }
        },
    );

    let mask = checkpoint_field_mask();
    // `buffered` runs fetches concurrently but processes each checkpoint one at a time on the
    // polling task. Fine while the throttle is what caps throughput; if processing becomes the
    // bottleneck, spawn each item instead.
    tokio_stream::StreamExt::throttle(
        seq_stream
            .map(move |seq| {
                let fetcher = fetcher.clone();
                let mask = mask.clone();
                async move {
                    let proto = fetch_one_with_retry(&fetcher, &mask, seq).await?;
                    let processed = process_checkpoint(proto)?;
                    Ok::<_, RpcError>(Arc::new(processed))
                }
            })
            .buffered(concurrency),
        throttle_interval,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::task::streaming::test_utils::FetcherBehavior;
    use crate::task::streaming::test_utils::MockFetcher;
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

        let fetcher = MockFetcher::success_for_range(1..=5);
        let stream = scan_checkpoints(fetcher, broadcast, 0, &SubscriptionConfig::default());
        let yielded: Vec<u64> = stream
            .map(|item| item.unwrap().summary.sequence_number)
            .collect()
            .await;
        assert_eq!(yielded, vec![1, 2, 3, 4, 5]);
    }

    #[tokio::test]
    async fn scan_completes_through_transient_fetcher_errors() {
        let (tx, broadcast) = test_broadcast(/* first_live_checkpoint */ 1);
        for seq in 1..=3 {
            let processed = process_checkpoint(make_test_proto_checkpoint(seq)).unwrap();
            tx.send(Arc::new(processed)).ok();
        }
        assert_eq!(broadcast.network_tip(), 3);

        // Seq 2 errors twice before succeeding; scan should still yield 1..=3 in order.
        let fetcher = MockFetcher::from_setup(&[
            (1, FetcherBehavior::Success),
            (2, FetcherBehavior::ErrorThenSuccess(2)),
            (3, FetcherBehavior::Success),
        ]);
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
        // Seq 2 was retried twice before succeeding, so total 3 calls.
        assert_eq!(fetcher.calls_for(2), 3);
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

        // Build the scan stream while tip is 3.
        let fetcher = MockFetcher::success_for_range(1..=7);
        let stream = scan_checkpoints(
            fetcher,
            broadcast.clone(),
            0,
            &SubscriptionConfig::default(),
        );

        // Advance the tip to 7 before consuming. The unfold reads `network_tip()` lazily per
        // iteration, so the scan should yield through 7.
        for seq in 4..=7 {
            let processed = process_checkpoint(make_test_proto_checkpoint(seq)).unwrap();
            tx.send(Arc::new(processed)).ok();
        }
        assert_eq!(broadcast.network_tip(), 7);

        let yielded: Vec<u64> = stream
            .map(|item| item.unwrap().summary.sequence_number)
            .collect()
            .await;
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
        let fetcher = MockFetcher::success_for_range(1..=3);
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
        let fetcher = MockFetcher::success_for_range(1..=5);
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
        let fetcher = MockFetcher::success_for_range(1..=5);
        let stream = scan_checkpoints(fetcher, broadcast, 10, &SubscriptionConfig::default());
        let yielded: Vec<u64> = stream
            .map(|item| item.unwrap().summary.sequence_number)
            .collect()
            .await;
        assert!(yielded.is_empty());
    }
}
