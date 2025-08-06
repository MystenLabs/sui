// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::HashMap;
use std::ops::{Bound, RangeBounds};
use std::sync::Arc;

use futures::future::try_join_all;
use tokio::{sync::mpsc, task::JoinHandle};
use tokio_util::sync::CancellationToken;
use tracing::{info, warn};

use crate::ingestion::streaming_service::StreamingService;
use crate::metrics::IndexerMetrics;
use crate::types::full_checkpoint_content::CheckpointData;

/// If streaming fails to start, we'll use ingestion for this many more checkpoints before trying
/// to start streaming again.
const INGESTION_CHECK_INTERVAL: u64 = 10;

/// The regulator task is responsible for writing out checkpoint sequence numbers from the
/// `checkpoints` iterator to `checkpoint_tx`, bounded by the high watermark dictated by
/// subscribers.
///
/// Subscribers can share their high watermarks on `ingest_hi_rx`. The regulator remembers these,
/// and stops serving checkpoints if they are over the minimum subscriber watermark plus the
/// ingestion `buffer_size`.
///
/// This offers a form of back-pressure that is sensitive to ordering, which is useful for
/// subscribers that need to commit information in order: Without it, those subscribers may need to
/// buffer unboundedly many updates from checkpoints while they wait for the checkpoint that they
/// need to commit.
///
/// Note that back-pressure is optional, and will only be applied if a subscriber provides a
/// watermark, at which point it must keep updating the watermark to allow the ingestion service to
/// continue making progress.
///
/// The task will shut down if the `cancel` token is signalled, or if streaming ends.
pub(super) fn regulator<S, R>(
    mut streaming_service: Option<S>,
    checkpoints: R,
    buffer_size: usize,
    mut ingest_hi_rx: mpsc::UnboundedReceiver<(&'static str, u64)>,
    checkpoint_tx: mpsc::Sender<u64>,
    subscribers: Vec<mpsc::Sender<Arc<CheckpointData>>>,
    metrics: Arc<IndexerMetrics>,
    cancel: CancellationToken,
) -> JoinHandle<()>
where
    R: RangeBounds<u64> + Send + Sync + 'static,
    S: StreamingService + Send + 'static,
{

    #[derive(Debug)]
    enum State {
        UnInit,
        Ingest{ current: u64, hi_exclusive: u64 },
        Stream{ current: u64 }
    }

    tokio::spawn(async move {
        let mut state = State::UnInit;
        let mut stream_active = false;

        let mut ingest_max = None;
        let mut subscribers_hi = HashMap::new();

        // Extract start and end bounds from the range
        let start_checkpoint = match checkpoints.start_bound() {
            Bound::Included(&n) => n,
            Bound::Excluded(&n) => n.saturating_add(1),
            Bound::Unbounded => {
                info!("Unbounded start range not supported, stopping regulator");
                return;
            }
        };

        let end_checkpoint_exclusive = match checkpoints.end_bound() {
            Bound::Included(&n) => n.saturating_add(1),
            Bound::Excluded(&n) => n,
            Bound::Unbounded => u64::MAX,
        };


        if let Some(service) = streaming_service.as_mut() {
            stream_active = true;

            // Initialize the streaming service
            if let Err(e) = service.start_streaming().await {
                warn!("Failed to start streaming service: {}", e);
                return;
            }
        } else {
            state = State::Ingest{ current: start_checkpoint,  hi_exclusive: end_checkpoint_exclusive };
        }

        loop {
            if let State::Ingest { current, hi_exclusive } = state {
                if current >= end_checkpoint_exclusive {
                    break;
                } else if current == hi_exclusive && !stream_active && streaming_service.is_some() {
                    // Re-initialize stream when caught up
                    if let Err(e) = streaming_service.as_mut().unwrap().start_streaming().await {
                        warn!("Failed to restart streaming service: {}", e);
                        state = State::Ingest{ current: current, hi_exclusive: hi_exclusive + INGESTION_CHECK_INTERVAL };
                    } else {
                        stream_active = true;
                    }
                }
            }

            tokio::select! {
                _ = cancel.cancelled() => {
                    info!("Shutdown received, stopping regulator");
                    break;
                }

                // docs::#regulator (see docs/content/guides/developer/advanced/custom-indexer.mdx)
                Some((name, hi)) = ingest_hi_rx.recv() => {
                    subscribers_hi.insert(name, hi);
                    ingest_max = subscribers_hi.values().copied().min().map(|hi| hi + buffer_size as u64);
                    println!("ingest max is {:?}", ingest_max);
                }

                checkpoint_result = async {
                    streaming_service.as_mut().unwrap().next_checkpoint().await
                }, if stream_active => {
                    match checkpoint_result {
                        Ok(checkpoint_data) => {
                            let sequence_number = checkpoint_data.checkpoint_summary.sequence_number;
                            let checkpoint_arc = Arc::new(checkpoint_data);

                            println!("Received checkpoint {} from subscription", sequence_number);

                            match state {
                                State::UnInit => {
                                    if start_checkpoint >= sequence_number {
                                        println!("switching state from UnInit to Stream with parameters {}", sequence_number);
                                        state = State::Stream { current: sequence_number };
                                    } else {
                                        println!("switching state from UnInit to Ingest with parameters {}, {}", start_checkpoint, sequence_number);
                                        state = State::Ingest { current: start_checkpoint, hi_exclusive: sequence_number };
                                        stream_active = false;
                                        continue;
                                    }

                                }

                                State::Ingest { current, hi_exclusive } => {
                                    assert_eq!(hi_exclusive, current);
                                    if sequence_number <= hi_exclusive {
                                        info!("switching state from Ingest to Stream with parameters {}", sequence_number);
                                        state = State::Stream { current: hi_exclusive };
                                    } else {
                                        info!("stayed in ingest mode with a new hi_exclusive {}", sequence_number);
                                        state = State::Ingest { current, hi_exclusive: sequence_number };
                                        stream_active = false;
                                        continue;
                                    }
                                }

                                State::Stream { current } => {
                                    if sequence_number > current // the cp we got is not the next one we expect
                                    || ingest_max.map_or(false, |max| sequence_number > max) // or the sequential pipelines are not ready for this one yet
                                    {
                                        info!("switch to ingest mode with parameters {}, {}", current, sequence_number);
                                        state = State::Ingest { current, hi_exclusive: sequence_number };
                                        stream_active = false;
                                        continue;
                                    }
                                }
                            }

                            // When we get to this point we know we are in stream mode and the cp we got is indeed
                            // the expected one, and it can be safely sent.
                            let State::Stream { current } = state else {
                                panic!("Invariant violation: Expected Stream state but got {:?}", state);
                            };

                            if sequence_number >= end_checkpoint_exclusive {
                                // We have reached the last checkpoint we want to index. Stop ingestion.
                                info!("Checkpoints done, stopping regulator");
                                break;
                            }

                            if sequence_number < current {
                                info!("Checkpoint {} is less than current {}, ignoring it", sequence_number, current);
                                continue;
                            }

                            // Broadcast checkpoint to all subscribers
                            println!("Broadcasting checkpoint {} to {} subscribers", sequence_number, subscribers.len());
                            let futures = subscribers.iter().map(|s| s.send(checkpoint_arc.clone()));
                            if try_join_all(futures).await.is_err() {
                                info!("Subscription dropped, stopping regulator");
                                break;
                            }

                            // Increment the metric for streamed checkpoints
                            metrics.total_streamed_checkpoints.inc();

                            state = State::Stream { current: sequence_number + 1 };
                        }
                        Err(e) => {
                            warn!("Checkpoint stream error: {}", e);

                            // Switch to ingest mode
                            stream_active = false;
                            state = match state {
                                State::UnInit => State::Ingest { current: start_checkpoint, hi_exclusive: start_checkpoint + INGESTION_CHECK_INTERVAL },
                                State::Ingest { current, hi_exclusive } => State::Ingest { current, hi_exclusive: hi_exclusive + INGESTION_CHECK_INTERVAL },
                                State::Stream { current } => State::Ingest { current, hi_exclusive: current + INGESTION_CHECK_INTERVAL }
                            }
                        }
                    }
                }

                // docs::/#regulator
                // docs::#bound (see docs/content/guides/developer/advanced/custom-indexer.mdx)
                res = async {
                    match state {
                        State::Ingest { current, .. } if ingest_max.is_none_or(|max| current <= max) => {
                            println!("Sent checkpoint {:?} via ingestion", state);
                            checkpoint_tx.send(current).await
                        }
                        _ => futures::future::pending().await
                    }
                }, if matches!(state, State::Ingest { .. }) && !stream_active => {
                    if res.is_ok() {
                        if let State::Ingest { current, hi_exclusive } = state {
                            state = State::Ingest { current: current + 1, hi_exclusive };
                        }
                    } else {
                        info!("Checkpoint channel closed, stopping regulator");
                        break;
                    }
                }
                // docs::/#bound
            }
        }
    })
}


#[cfg(test)]
mod tests {
    use std::time::Duration;
    use tokio::time::{error::Elapsed, timeout};
    use crate::ingestion::streaming_service::test_utils::MockStreamingService;
    use crate::metrics::tests::test_metrics;

    use super::*;

    /// Wait up to a second for a response on the channel, and return it, expecting this operation
    /// to succeed.
    async fn expect_recv<T>(rx: &mut mpsc::Receiver<T>) -> Option<T> {
        timeout(Duration::from_secs(1), rx.recv()).await.unwrap()
    }

    /// Wait up to a second for a response on the channel, but expecting this operation to timeout.
    async fn expect_timeout<T: std::fmt::Debug>(rx: &mut mpsc::Receiver<T>) -> Elapsed {
        timeout(Duration::from_secs(1), rx.recv())
            .await
            .unwrap_err()
    }

    #[tokio::test]
    async fn finite_list_of_checkpoints() {
        let (_, hi_rx) = mpsc::unbounded_channel();
        let (cp_tx, mut cp_rx) = mpsc::channel(1);
        let cancel = CancellationToken::new();
        let metrics = test_metrics();

        let h_regulator = regulator::<MockStreamingService, _>(None, 0..5, 0, hi_rx, cp_tx, vec![], metrics, cancel.clone());

        for i in 0..5 {
            assert_eq!(Some(i), expect_recv(&mut cp_rx).await);
        }

        h_regulator.await.unwrap();
    }

    #[tokio::test]
    async fn shutdown_on_sender_closed() {
        let (_, hi_rx) = mpsc::unbounded_channel();
        let (cp_tx, mut cp_rx) = mpsc::channel(1);
        let cancel = CancellationToken::new();
        let metrics = test_metrics();

        let h_regulator = regulator::<MockStreamingService, _>(None, 0..100, 0, hi_rx, cp_tx, vec![], metrics, cancel.clone());

        for i in 0..5 {
            assert_eq!(Some(i), expect_recv(&mut cp_rx).await);
        }

        drop(cp_rx);
        h_regulator.await.unwrap();
    }

    #[tokio::test]
    async fn shutdown_on_cancel() {
        let (_, hi_rx) = mpsc::unbounded_channel();
        let (cp_tx, mut cp_rx) = mpsc::channel(1);
        let cancel = CancellationToken::new();
        let metrics = test_metrics();

        let h_regulator = regulator::<MockStreamingService, _>(None, 0..100, 0, hi_rx, cp_tx, vec![], metrics, cancel.clone());

        for i in 0..5 {
            assert_eq!(Some(i), expect_recv(&mut cp_rx).await);
        }

        cancel.cancel();
        h_regulator.await.unwrap();
    }

    #[tokio::test]
    async fn halted() {
        let (hi_tx, hi_rx) = mpsc::unbounded_channel();
        let (cp_tx, mut cp_rx) = mpsc::channel(1);
        let cancel = CancellationToken::new();
        let metrics = test_metrics();

        hi_tx.send(("test", 4)).unwrap();

        let h_regulator = regulator::<MockStreamingService, _>(None, 0..100, 0, hi_rx, cp_tx, vec![], metrics, cancel.clone());

        for _ in 0..=4 {
            expect_recv(&mut cp_rx).await;
        }

        // Regulator stopped because of watermark.
        expect_timeout(&mut cp_rx).await;

        cancel.cancel();
        h_regulator.await.unwrap();
    }

    #[tokio::test]
    async fn halted_buffered() {
        let (hi_tx, hi_rx) = mpsc::unbounded_channel();
        let (cp_tx, mut cp_rx) = mpsc::channel(1);
        let cancel = CancellationToken::new();
        let metrics = test_metrics();

        hi_tx.send(("test", 2)).unwrap();

        let h_regulator = regulator::<MockStreamingService, _>(None, 0..100, 2, hi_rx, cp_tx, vec![], metrics, cancel.clone());

        for i in 0..=4 {
            assert_eq!(Some(i), expect_recv(&mut cp_rx).await);
        }

        // Regulator stopped because of watermark (plus buffering).
        expect_timeout(&mut cp_rx).await;

        cancel.cancel();
        h_regulator.await.unwrap();
    }

    #[tokio::test]
    async fn resumption() {
        let (hi_tx, hi_rx) = mpsc::unbounded_channel();
        let (cp_tx, mut cp_rx) = mpsc::channel(1);
        let cancel = CancellationToken::new();
        let metrics = test_metrics();

        hi_tx.send(("test", 2)).unwrap();

        let h_regulator = regulator::<MockStreamingService, _>(None, 0..100, 0, hi_rx, cp_tx, vec![], metrics, cancel.clone());

        for i in 0..=2 {
            assert_eq!(Some(i), expect_recv(&mut cp_rx).await);
        }

        // Regulator stopped because of watermark, but resumes when that watermark is updated.
        expect_timeout(&mut cp_rx).await;
        hi_tx.send(("test", 4)).unwrap();

        for i in 3..=4 {
            assert_eq!(Some(i), expect_recv(&mut cp_rx).await);
        }

        // Halted again.
        expect_timeout(&mut cp_rx).await;

        cancel.cancel();
        h_regulator.await.unwrap();
    }

    #[tokio::test]
    async fn multiple_subscribers() {
        let (hi_tx, hi_rx) = mpsc::unbounded_channel();
        let (cp_tx, mut cp_rx) = mpsc::channel(1);
        let cancel = CancellationToken::new();
        let metrics = test_metrics();

        hi_tx.send(("a", 2)).unwrap();
        hi_tx.send(("b", 3)).unwrap();

        let h_regulator = regulator::<MockStreamingService, _>(None, 0..10, 0, hi_rx, cp_tx, vec![], metrics, cancel.clone());

        for i in 0..=2 {
            assert_eq!(Some(i), expect_recv(&mut cp_rx).await);
        }

        // Watermark stopped because of a's watermark.
        expect_timeout(&mut cp_rx).await;

        // Updating b's watermark doesn't make a difference.
        hi_tx.send(("b", 4)).unwrap();
        expect_timeout(&mut cp_rx).await;

        // But updating a's watermark does.
        hi_tx.send(("a", 3)).unwrap();
        assert_eq!(Some(3), expect_recv(&mut cp_rx).await);

        // ...by one checkpoint.
        expect_timeout(&mut cp_rx).await;

        // And we can make more progress by updating it again.
        hi_tx.send(("a", 4)).unwrap();
        assert_eq!(Some(4), expect_recv(&mut cp_rx).await);

        // But another update to "a" will now not make a difference, because "b" is still behind.
        hi_tx.send(("a", 5)).unwrap();
        expect_timeout(&mut cp_rx).await;

        cancel.cancel();
        h_regulator.await.unwrap();
    }

    #[tokio::test]
    async fn stream_direct_transition() {
        let (_, hi_rx) = mpsc::unbounded_channel();
        let (cp_tx, mut cp_rx) = mpsc::channel(10);
        let cancel = CancellationToken::new();

        // Create subscriber to receive broadcast checkpoints
        let (sub_tx, mut sub_rx) = mpsc::channel(10);
        let subscribers = vec![sub_tx];

        // Start streaming from checkpoint 10, range is 10..15
        // Since stream starts at global_lo (10), should go directly from UnInit to Stream
        let streaming_service = MockStreamingService::new(10..150);
        let metrics = test_metrics();
        let h_regulator = regulator(
            Some(streaming_service),
            10..150,
            0,
            hi_rx,
            cp_tx,
            subscribers,
            metrics,
            cancel.clone()
        );

        // Should NOT receive anything on cp_rx (ingestion channel) since we go directly to Stream
        expect_timeout(&mut cp_rx).await;


        // But should receive checkpoints via subscriber channel
        for i in 10..15 {
            assert_eq!(i, expect_recv(&mut sub_rx).await.unwrap().checkpoint_summary.sequence_number);
        }

        cancel.cancel();
        h_regulator.await.unwrap();
    }

    #[tokio::test]
    async fn ingest_to_stream_transition() {
        // Initialize tracing for test output
        let _ = tracing_subscriber::fmt()
            .with_test_writer()
            .with_env_filter("info")
            .try_init();

        let (_, hi_rx) = mpsc::unbounded_channel();
        let (cp_tx, mut cp_rx) = mpsc::channel(10);
        let cancel = CancellationToken::new();

        // Create subscriber to receive broadcast checkpoints
        let (sub_tx, mut sub_rx) = mpsc::channel(10);
        let subscribers = vec![sub_tx];

        // Start streaming from checkpoint 15, but request range starts at 10
        // This creates a gap, so should go UnInit -> Ingest -> Stream
        let mut streaming_service = MockStreamingService::new(15..16);
        streaming_service.insert_checkpoint_range(15..200);
        let metrics = test_metrics();
        let h_regulator = regulator(
            Some(streaming_service),
            10..200,
            0,
            hi_rx,
            cp_tx,
            subscribers,
            metrics,
            cancel.clone()
        );

        // Should receive checkpoints 10-15 via ingestion (Ingest state)
        for i in 10..15 {
            assert_eq!(Some(i), expect_recv(&mut cp_rx).await);
        }

        // Should NOT receive anything else from ingestion.
        expect_timeout(&mut cp_rx).await;

        // After ingesting 10-15, should transition to Stream state
        // and receive checkpoints 16-20 via subscriber channel
        for i in 15..20 {
            assert_eq!(i, expect_recv(&mut sub_rx).await.unwrap().checkpoint_summary.sequence_number);
        }

        cancel.cancel();
        h_regulator.await.unwrap();
    }

    #[tokio::test]
    async fn stream_to_ingest_with_gap() {
        // Initialize tracing for test output
        let _ = tracing_subscriber::fmt()
            .with_test_writer()
            .with_env_filter("info")
            .try_init();

        let (_, hi_rx) = mpsc::unbounded_channel();
        let (cp_tx, mut cp_rx) = mpsc::channel(10);
        let cancel = CancellationToken::new();

        // Create subscriber to receive broadcast checkpoints
        let (sub_tx, mut sub_rx) = mpsc::channel(10);
        let subscribers = vec![sub_tx];

        // Start with continuous stream, then create a gap
        // Checkpoints: 10, 11, 12, then jump to 15 (gap of 13, 14)
        let mut streaming_service = MockStreamingService::new(vec![10, 11, 12]);
        streaming_service.insert_checkpoint(15);  // Gap here - missing 13, 14
        streaming_service.insert_checkpoint_range(15..200);  // Continue streaming

        let metrics = test_metrics();
        let h_regulator = regulator(
            Some(streaming_service),
            10..200,
            0,
            hi_rx,
            cp_tx,
            subscribers,
            metrics,
            cancel.clone()
        );

        // First 3 checkpoints should be streamed directly (Stream state)
        for i in 10..13 {
            assert_eq!(i, expect_recv(&mut sub_rx).await.unwrap().checkpoint_summary.sequence_number);
        }

        // When checkpoint 15 arrives (gap detected), should switch to Ingest state
        // Checkpoints 13-14 should come via ingestion
        for i in 13..15 {
            assert_eq!(Some(i), expect_recv(&mut cp_rx).await);
        }

        // No more from ingestion since we switched to streaming
        expect_timeout(&mut cp_rx).await;


        // After filling the gap, should resume streaming from 15
        for i in 15..18 {
            assert_eq!(i, expect_recv(&mut sub_rx).await.unwrap().checkpoint_summary.sequence_number);
        }

        cancel.cancel();
        h_regulator.await.unwrap();
    }

    #[tokio::test]
    async fn stream_to_ingest_due_to_backpressure() {
        // Initialize tracing for test output
        let _ = tracing_subscriber::fmt()
            .with_test_writer()
            .with_env_filter("info")
            .try_init();

        let (hi_tx, hi_rx) = mpsc::unbounded_channel();

        // With watermark at 10 and buffer_size of 2, max is 12
        // Backpressure will be triggered at checkpoint 13.
        hi_tx.send(("pipeline1", 10)).unwrap();

        let (cp_tx, mut cp_rx) = mpsc::channel(10);
        let cancel = CancellationToken::new();

        // Create subscriber to receive broadcast checkpoints
        let (sub_tx, mut sub_rx) = mpsc::channel(10);
        let subscribers = vec![sub_tx];

        let mut streaming_service = MockStreamingService::new(10..15);
        streaming_service.insert_checkpoint(20);
        streaming_service.insert_checkpoint_range(20..100);

        let metrics = test_metrics();
        let h_regulator = regulator(
            Some(streaming_service),
            10..100,
            2, // buffer_size of 2
            hi_rx,
            cp_tx,
            subscribers,
            metrics,
            cancel.clone()
        );

        // Stream first few checkpoints normally
        for i in 10..13 {
            assert_eq!(i, expect_recv(&mut sub_rx).await.unwrap().checkpoint_summary.sequence_number);
        }

        // We should be receiving nothing from either ingestion or streaming.
        expect_timeout(&mut cp_rx).await;
        expect_timeout(&mut sub_rx).await;

        // Update watermark to allow progress
        hi_tx.send(("pipeline1", 23)).unwrap();

        // Should start receiving via ingestion for the next checkpoints.
        for i in 13..20 {
            assert_eq!(Some(i), expect_recv(&mut cp_rx).await);
        }

        expect_timeout(&mut cp_rx).await;

        // Then the next ones should come from streaming
        for i in 20..25 {
            assert_eq!(i, expect_recv(&mut sub_rx).await.unwrap().checkpoint_summary.sequence_number);
        }

        cancel.cancel();
        h_regulator.await.unwrap();
    }

    #[tokio::test]
    async fn multiple_gaps_handling() {
        // Initialize tracing for test output
        let _ = tracing_subscriber::fmt()
            .with_test_writer()
            .with_env_filter("info")
            .try_init();

        let (_, hi_rx) = mpsc::unbounded_channel();
        let (cp_tx, mut cp_rx) = mpsc::channel(10);
        let cancel = CancellationToken::new();

        // Create subscriber to receive broadcast checkpoints
        let (sub_tx, mut sub_rx) = mpsc::channel(10);
        let subscribers = vec![sub_tx];

        // Create stream with multiple gaps:
        // 10-12 (gap) 15-17 (gap) 20-22
        let mut streaming_service = MockStreamingService::new(vec![10, 11, 12]);
        streaming_service.insert_checkpoint(15); // First gap: missing 13, 14
        streaming_service.insert_checkpoint(15);
        streaming_service.insert_checkpoint(16);
        streaming_service.insert_checkpoint(17);
        streaming_service.insert_checkpoint(20); // Second gap: missing 18, 19
        streaming_service.insert_checkpoint(20);
        streaming_service.insert_checkpoint(21);
        streaming_service.insert_checkpoint(22);

        let metrics = test_metrics();
        let h_regulator = regulator(
            Some(streaming_service),
            10..23,
            0,
            hi_rx,
            cp_tx,
            subscribers,
            metrics,
            cancel.clone()
        );

        // First sequence streams normally
        for i in 10..13 {
            assert_eq!(i, expect_recv(&mut sub_rx).await.unwrap().checkpoint_summary.sequence_number);
        }

        // First gap: should ingest 13-14
        for i in 13..15 {
            assert_eq!(Some(i), expect_recv(&mut cp_rx).await);
        }

        // Resume streaming 15-17
        for i in 15..18 {
            assert_eq!(i, expect_recv(&mut sub_rx).await.unwrap().checkpoint_summary.sequence_number);
        }

        // Second gap: should ingest 18-19
        for i in 18..20 {
            assert_eq!(Some(i), expect_recv(&mut cp_rx).await);
        }

        // Resume streaming 20-22
        for i in 20..23 {
            assert_eq!(i, expect_recv(&mut sub_rx).await.unwrap().checkpoint_summary.sequence_number);
        }

        cancel.cancel();
        h_regulator.await.unwrap();
    }

    #[tokio::test]
    async fn stream_error_recovery() {
        // Initialize tracing for test output
        let _ = tracing_subscriber::fmt()
            .with_test_writer()
            .with_env_filter("info")
            .try_init();

        let (_, hi_rx) = mpsc::unbounded_channel();
        let (cp_tx, mut cp_rx) = mpsc::channel(10);
        let cancel = CancellationToken::new();

        // Create subscriber to receive broadcast checkpoints
        let (sub_tx, mut sub_rx) = mpsc::channel(10);
        let subscribers = vec![sub_tx];

        // Stream with an error in the middle
        let mut streaming_service = MockStreamingService::new(vec![10, 11, 12]);
        streaming_service.insert_error(); // This will cause an error
        // After error, streaming service should be restarted and continue
        streaming_service.insert_checkpoint_range(17..100);

        let metrics = test_metrics();
        let h_regulator = regulator(
            Some(streaming_service),
            10..300,
            0,
            hi_rx,
            cp_tx,
            subscribers,
            metrics,
            cancel.clone()
        );

        // First few checkpoints stream normally
        for i in 10..13 {
            assert_eq!(i, expect_recv(&mut sub_rx).await.unwrap().checkpoint_summary.sequence_number);
        }

        for i in 13..13 + INGESTION_CHECK_INTERVAL {
            assert_eq!(Some(i), expect_recv(&mut cp_rx).await);
        }
        expect_timeout(&mut cp_rx).await;

        // After the error, streaming should restart and continue
        // The regulator should call start_streaming() again and continue from 13
        for i in 13 + INGESTION_CHECK_INTERVAL..30 {
            assert_eq!(i, expect_recv(&mut sub_rx).await.unwrap().checkpoint_summary.sequence_number);
        }

        cancel.cancel();
        h_regulator.await.unwrap();
    }
}
