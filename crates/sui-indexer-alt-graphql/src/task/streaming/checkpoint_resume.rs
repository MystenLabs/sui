// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Per-subscriber catch-up scan for resumable subscriptions.
//!
//! [`scan_checkpoints`] yields past `ProcessedCheckpoint`s via LedgerService until the
//! remaining gap to the live tip is small enough for the broadcast channel to bridge, then
//! exits. The caller bridges the remaining gap by consuming the live broadcast.
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
/// concurrently. Exits once the remaining gap to the network tip is small enough for the
/// broadcast channel to bridge; the caller then takes over by consuming the live broadcast
/// (the bridge between scan end and the first live item is covered by the caller subscribing
/// before this scan starts).
pub(super) fn scan_checkpoints<F: CheckpointFetcher + 'static>(
    fetcher: Arc<F>,
    broadcast: Arc<SubscriptionBroadcast>,
    start_after: u64,
    config: &SubscriptionConfig,
) -> impl Stream<Item = Result<Arc<ProcessedCheckpoint>, RpcError>> + 'static {
    let transition_threshold = config.resume_transition_threshold();
    let concurrency = config.per_subscriber_scan_max_concurrent_fetches;
    let throttle_interval = Duration::from_secs(1) / config.per_subscriber_scan_max_qps.max(1);

    // Stream of sequence numbers to scan. Stops once the gap to the live tip drops below
    // `transition_threshold`, handing off to the live broadcast that the caller is already
    // consuming.
    let seq_stream = stream::unfold(
        (start_after, broadcast),
        move |(last, broadcast)| async move {
            let tip = broadcast.network_tip();
            if tip.saturating_sub(last) <= transition_threshold {
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
                    let proto = fetch_one_with_retry(fetcher.as_ref(), &mask, seq).await?;
                    let processed = process_checkpoint(proto)?;
                    Ok::<_, RpcError>(Arc::new(processed))
                }
            })
            .buffered(concurrency),
        throttle_interval,
    )
}
