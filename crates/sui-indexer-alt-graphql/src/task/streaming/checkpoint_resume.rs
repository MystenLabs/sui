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
/// concurrently. Exits once the scan position catches up to the live tip; the caller then
/// hands off to the live broadcast (the receiver is subscribed by the caller partway through
/// this scan, so it accumulates live items in parallel with the remaining drain).
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
