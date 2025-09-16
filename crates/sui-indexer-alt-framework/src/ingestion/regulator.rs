// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::HashMap;
use std::ops::{Bound, RangeBounds};
use std::sync::Arc;

use futures::future::try_join_all;
use tokio::{sync::mpsc, task::JoinHandle};
use tokio_util::sync::CancellationToken;
use tracing::{error, info, warn};

use crate::ingestion::streaming_service::StreamingService;
use crate::metrics::IndexerMetrics;
use crate::types::full_checkpoint_content::CheckpointData;

// TODO: Make these configurable via IngestionConfig.
/// If streaming fails to start, we'll use ingestion for this many more checkpoints before trying
/// to start streaming again.
const INGESTION_CHECK_INTERVAL: u64 = 10;

/// If the gap between the start of streaming and the current position of ingestion is less than this
/// threshold, we'll enter the transition mode where we ingest and stream at the same time before
/// filling up the gap, then switch to streaming.
const TRANSITION_THRESHOLD: u64 = 100;

/// The state of the regulator.
/// - Ingest: We are fetching checkpoints only from ingestion client. `current` is the next checkpoint to ingest,
///   `hi_exclusive` is the upper bound (exclusive) of the checkpoints we want to ingest. `next_start` is the next
///   checkpoint to start streaming from, if we transition to streaming. `next_start` will only be set if we fail
///   to stream in the middle of the transition from ingestion to streaming and there is a gap to fill.
/// - Transition: We are getting checkpoints from both ingestion client and streaming service. `ingest_current` is the next
///   checkpoint to ingest, `stream_start` is the first checkpoint we have broadcasted from streaming service, `stream_current`
///   is the next checkpoint to broadcast from streaming service.
/// - Stream: We are getting checkpoints only from streaming service. `current` is the next checkpoint to broadcast.
#[derive(Debug)]
enum State {
    // next_start will only be set if streaming has failed during Transition state and there is a gap to fill.
    Ingest {
        current: u64,
        hi_exclusive: u64,
        next_start: Option<u64>,
    },
    Transition {
        ingest_current: u64,
        stream_start: u64,
        stream_current: u64,
    },
    Stream {
        current: u64,
    },
}

/// The regulator task is responsible for writing out checkpoint sequence numbers from the
/// `checkpoints` iterator to `checkpoint_tx`, bounded by the high watermark dictated by
/// subscribers. Optionally, it can also use a `streaming_service` to receive checkpoints
/// directly from a streaming source and send checkpoint data to `subscribers` if the checkpoint
/// progress is caught up with the network. It will falling back to ingesting checkpoints if the
/// streaming service fails or if there are gaps in the checkpoint sequence.
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
    tokio::spawn(async move {
        // The maximum checkpoint that all subscribers have acknowledged, plus the buffer size.
        // We can only make progress up to this checkpoint.
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

        // If streaming service is provided, start in streaming mode, otherwise start in ingestion mode.
        let mut state = if let Some(service) = streaming_service.as_mut() {
            // Initialize the streaming service
            if let Err(e) = service.start_streaming().await {
                error!(
                    "Failed to start streaming service: {}, starting in ingest mode",
                    e
                );
                State::Ingest {
                    current: start_checkpoint,
                    hi_exclusive: start_checkpoint + INGESTION_CHECK_INTERVAL,
                    next_start: None,
                }
            } else {
                info!("Initialized streaming service");
                State::Stream {
                    current: start_checkpoint,
                }
            }
        } else {
            State::Ingest {
                current: start_checkpoint,
                hi_exclusive: end_checkpoint_exclusive,
                next_start: None,
            }
        };

        loop {
            // Check if we are done against the end checkpoint and also perform state
            // transitions from ingesting to streaming if needed.
            match state {
                State::Ingest {
                    current,
                    hi_exclusive,
                    next_start,
                } => {
                    if current >= end_checkpoint_exclusive {
                        break;
                    } else if current == hi_exclusive && streaming_service.is_some() {
                        // Re-initialize stream since we have caught up to the last checkpoint streamed.
                        if let Err(e) = streaming_service.as_mut().unwrap().start_streaming().await
                        {
                            warn!("Failed to restart streaming service: {}", e);
                            let new_hi_exclusive = hi_exclusive + INGESTION_CHECK_INTERVAL;
                            let new_next_start = next_start
                                .map(|next_start| std::cmp::max(next_start, new_hi_exclusive));
                            state = State::Ingest {
                                current,
                                hi_exclusive: new_hi_exclusive,
                                next_start: new_next_start,
                            };
                        } else {
                            info!(
                                "Switching from ingest to streaming mode at checkpoint {}",
                                current
                            );
                            state = State::Stream {
                                current: next_start.unwrap_or(current),
                            };
                        }
                    }
                }
                State::Transition {
                    ingest_current,
                    stream_start,
                    stream_current,
                } => {
                    if ingest_current >= end_checkpoint_exclusive {
                        break;
                    }
                    if stream_current >= end_checkpoint_exclusive {
                        // No more streaming needed, switch to ingest only until the end.
                        state = State::Ingest {
                            current: ingest_current,
                            hi_exclusive: end_checkpoint_exclusive,
                            next_start: None,
                        };
                    } else if ingest_current == stream_start {
                        // The gap has been filled, switch to just streaming.
                        state = State::Stream {
                            current: stream_current,
                        };
                    }
                }
                State::Stream { current } => {
                    if current >= end_checkpoint_exclusive {
                        break;
                    }
                }
            }

            // Determine if we should attempt ingestion
            let should_ingest = match state {
                State::Ingest { current, .. } => ingest_max.is_none_or(|max| current <= max),
                State::Transition { ingest_current, .. } => {
                    ingest_max.is_none_or(|max| ingest_current <= max)
                }
                State::Stream { .. } => false,
            };

            // Determine if we should attempt streaming
            let should_stream = matches!(state, State::Stream { .. } | State::Transition { .. });

            tokio::select! {
                _ = cancel.cancelled() => {
                    info!("Shutdown received, stopping regulator");
                    break;
                }

                // docs::#regulator (see docs/content/guides/developer/advanced/custom-indexer.mdx)
                Some((name, hi)) = ingest_hi_rx.recv() => {
                    subscribers_hi.insert(name, hi);
                    ingest_max = subscribers_hi.values().copied().min().map(|hi| hi + buffer_size as u64);
                }

                // docs::/#regulator
                // docs::#bound (see docs/content/guides/developer/advanced/custom-indexer.mdx)
                res = async {
                    let checkpoint_to_send = match state {
                        State::Ingest { current, .. } => current,
                        State::Transition { ingest_current, .. } => ingest_current,
                        _ => unreachable!("should_ingest guards against this")
                    };
                    println!("Sent checkpoint {:?} via ingestion", state);
                    checkpoint_tx.send(checkpoint_to_send).await
                }, if should_ingest => {
                    if res.is_ok() {
                        match state {
                            State::Ingest { current, hi_exclusive, next_start } => {
                                state = State::Ingest { current: current + 1, hi_exclusive, next_start };
                            }
                            State::Transition { ingest_current, stream_start, stream_current } => {
                                state = State::Transition { ingest_current: ingest_current + 1, stream_start, stream_current };
                                info!("just incremented ingest_current to {}, stream_start is {}, stream_current is {}", ingest_current + 1, stream_start, stream_current);
                            }
                            _ => unreachable!()
                        }
                    } else {
                        info!("Checkpoint channel closed, stopping regulator");
                        break;
                    }
                }

                checkpoint_result = async {
                    // SAFETY: unwrap is safe because should_stream guards against streaming_service being None.
                    streaming_service.as_mut().unwrap().next_checkpoint().await
                }, if should_stream => {

                    let current = match state {
                        State::Stream { current } => current,
                        State::Transition { stream_current, .. } => stream_current,
                        _ => unreachable!()
                    };

                    match checkpoint_result {
                        Ok(checkpoint_data) => {
                            let sequence_number = checkpoint_data.checkpoint_summary.sequence_number;
                            let checkpoint_arc = Arc::new(checkpoint_data);

                            info!("Received checkpoint {} from subscription", sequence_number);

                            let should_broadcast = state_transition_given_streamed_cp(sequence_number, &mut state, current, ingest_max);

                            if !should_broadcast {
                                // We are not broadcasting this checkpoint, so skip the rest of the loop.
                                continue;
                            }

                            // Broadcast checkpoint to all subscribers
                            info!("Broadcasting streamed checkpoint {} to {} subscribers", sequence_number, subscribers.len());

                            let futures = subscribers.iter().map(|s| s.send(checkpoint_arc.clone()));
                            if try_join_all(futures).await.is_err() {
                                info!("Subscription dropped, stopping regulator");
                                break;
                            }

                            state = match state {
                                State::Stream { current: _ } => State::Stream { current: sequence_number + 1 },
                                State::Transition { ingest_current, stream_start, stream_current: _ } =>
                                    State::Transition { ingest_current, stream_start, stream_current: sequence_number + 1 },
                                _ => unreachable!()
                            };

                            // Increment the metric for streamed checkpoints
                            metrics.total_streamed_checkpoints.inc();
                        }
                        Err(e) => {
                            warn!("Checkpoint stream error: {}", e);

                            // Switch to ingest mode and check back after INGESTION_CHECK_INTERVAL many checkpoints.
                            state = match state {
                                State::Stream { current } => State::Ingest { current, hi_exclusive: current + INGESTION_CHECK_INTERVAL, next_start: None },
                                State::Transition { ingest_current, stream_start, stream_current } =>
                                    State::Ingest { current: ingest_current, hi_exclusive: stream_start, next_start: Some(stream_current) },
                                _ => unreachable!()
                            };
                        }
                    }
                }
                // docs::/#bound
            }
        }
    })
}

/// Given a checkpoint received from the streaming service, and the current state of the regulator,
/// perform the state transition logic, returning true if we should continue and broadcast this checkpoint.
fn state_transition_given_streamed_cp(
    streamed_cp: u64,
    state: &mut State,
    current: u64,
    ingest_max: Option<u64>,
) -> bool {
    if ingest_max.is_some_and(|max| streamed_cp > max)
    // The sequential pipelines are not ready for this one yet
    {
        info!(
            "switch to ingest mode with parameters {}, {}",
            current, streamed_cp
        );
        *state = match state {
            State::Stream { current, .. } => State::Ingest {
                current: *current,
                hi_exclusive: streamed_cp,
                next_start: None,
            },
            State::Transition {
                ingest_current,
                stream_start,
                stream_current,
            } => State::Ingest {
                current: *ingest_current,
                hi_exclusive: *stream_start,
                next_start: Some(*stream_current),
            },
            _ => unreachable!(),
        };
        return false;
    }

    if streamed_cp < current {
        info!(
            "Checkpoint {} is less than current {}, ignoring it",
            streamed_cp, current
        );
        return false;
    }

    assert!(
        streamed_cp >= current,
        "Checkpoint {} is less than the expected current {}",
        streamed_cp,
        current
    );

    if streamed_cp == current {
        // The cp we got from stream is the one we want. Nothing to do, just continue streaming.
        true
    } else if streamed_cp <= current + TRANSITION_THRESHOLD {
        // If the gap is small enough, we can enter transition mode.
        match state {
            State::Stream { current } => {
                info!(
                    "switch from stream to transition mode with parameters {}, {}",
                    *current, streamed_cp
                );
                *state = State::Transition {
                    ingest_current: *current,
                    stream_start: streamed_cp,
                    stream_current: streamed_cp,
                };
            }
            State::Transition { .. } => {
                // We are already in transition, nothing happens.
            }
            _ => unreachable!(),
        }
        true
    } else {
        assert!(
            streamed_cp > current + TRANSITION_THRESHOLD,
            "streamed_cp {} should be greater than current {} + TRANSITION_THRESHOLD {}",
            streamed_cp,
            current,
            TRANSITION_THRESHOLD
        );
        // Gap is too large, switch to ingest mode to fill it.
        match state {
            State::Stream { current } => {
                info!(
                    "switch from stream to ingest mode with parameters {}, {}",
                    *current, streamed_cp
                );
                *state = State::Ingest {
                    current: *current,
                    hi_exclusive: streamed_cp,
                    next_start: None,
                };
            }
            State::Transition {
                ingest_current,
                stream_start,
                stream_current,
            } => {
                info!(
                    "switch from transition to ingest mode with parameters {}, {}",
                    *ingest_current, *stream_start
                );
                *state = State::Ingest {
                    current: *ingest_current,
                    hi_exclusive: *stream_start,
                    next_start: Some(*stream_current),
                };
            }
            _ => unreachable!(),
        }
        false
    }
}

#[cfg(test)]
mod tests {
    use crate::ingestion::streaming_service::test_utils::MockStreamingService;
    use crate::metrics::tests::test_metrics;
    use std::time::Duration;
    use tokio::time::{error::Elapsed, timeout};

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

        let h_regulator = regulator::<MockStreamingService, _>(
            None,
            0..5,
            0,
            hi_rx,
            cp_tx,
            vec![],
            metrics,
            cancel.clone(),
        );

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

        let h_regulator = regulator::<MockStreamingService, _>(
            None,
            0..100,
            0,
            hi_rx,
            cp_tx,
            vec![],
            metrics,
            cancel.clone(),
        );

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

        let h_regulator = regulator::<MockStreamingService, _>(
            None,
            0..100,
            0,
            hi_rx,
            cp_tx,
            vec![],
            metrics,
            cancel.clone(),
        );

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

        let h_regulator = regulator::<MockStreamingService, _>(
            None,
            0..100,
            0,
            hi_rx,
            cp_tx,
            vec![],
            metrics,
            cancel.clone(),
        );

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

        let h_regulator = regulator::<MockStreamingService, _>(
            None,
            0..100,
            2,
            hi_rx,
            cp_tx,
            vec![],
            metrics,
            cancel.clone(),
        );

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

        let h_regulator = regulator::<MockStreamingService, _>(
            None,
            0..100,
            0,
            hi_rx,
            cp_tx,
            vec![],
            metrics,
            cancel.clone(),
        );

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

        let h_regulator = regulator::<MockStreamingService, _>(
            None,
            0..10,
            0,
            hi_rx,
            cp_tx,
            vec![],
            metrics,
            cancel.clone(),
        );

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
        // Since stream starts at global_lo (10), should go directly to Stream
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
            cancel.clone(),
        );

        // Should NOT receive anything on cp_rx (ingestion channel) since we go directly to Stream
        expect_timeout(&mut cp_rx).await;

        // But should receive checkpoints via subscription channel
        for i in 10..15 {
            assert_eq!(
                i,
                expect_recv(&mut sub_rx)
                    .await
                    .unwrap()
                    .checkpoint_summary
                    .sequence_number
            );
        }

        cancel.cancel();
        h_regulator.await.unwrap();
    }

    #[tokio::test]
    async fn transition_state_to_stream() {
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
        // This creates a gap, so should go Transition -> Stream
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
            cancel.clone(),
        );

        // Wait a bit to ensure we have got everything in the channels.
        tokio::time::sleep(Duration::from_millis(500)).await;

        // Should receive checkpoints 10-15 via ingestion (Ingest state)
        for i in 10..15 {
            assert_eq!(Some(i), expect_recv(&mut cp_rx).await);
        }

        // Should NOT receive anything else from ingestion.
        expect_timeout(&mut cp_rx).await;

        // After ingesting 10-15, should transition to Stream state
        // and receive checkpoints 16-20 via subscriber channel
        for i in 15..20 {
            assert_eq!(
                i,
                expect_recv(&mut sub_rx)
                    .await
                    .unwrap()
                    .checkpoint_summary
                    .sequence_number
            );
        }

        cancel.cancel();
        h_regulator.await.unwrap();
    }

    #[tokio::test]
    async fn stream_to_transition_with_gap() {
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
        // Should go Stream -> Transition -> Stream
        let mut streaming_service = MockStreamingService::new(vec![10, 11, 12]);
        streaming_service.insert_checkpoint(15); // Gap here - missing 13, 14
        streaming_service.insert_checkpoint_range(16..100); // Continue streaming

        let metrics = test_metrics();
        let h_regulator = regulator(
            Some(streaming_service),
            10..100,
            0,
            hi_rx,
            cp_tx,
            subscribers,
            metrics,
            cancel.clone(),
        );

        // First 3 checkpoints should be streamed directly (Stream state)
        for i in 10..13 {
            assert_eq!(
                i,
                expect_recv(&mut sub_rx)
                    .await
                    .unwrap()
                    .checkpoint_summary
                    .sequence_number
            );
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
            assert_eq!(
                i,
                expect_recv(&mut sub_rx)
                    .await
                    .unwrap()
                    .checkpoint_summary
                    .sequence_number
            );
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
            cancel.clone(),
        );

        // Stream first few checkpoints normally
        for i in 10..13 {
            assert_eq!(
                i,
                expect_recv(&mut sub_rx)
                    .await
                    .unwrap()
                    .checkpoint_summary
                    .sequence_number
            );
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
            assert_eq!(
                i,
                expect_recv(&mut sub_rx)
                    .await
                    .unwrap()
                    .checkpoint_summary
                    .sequence_number
            );
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
            cancel.clone(),
        );

        // First sequence streams normally
        for i in 10..13 {
            assert_eq!(
                i,
                expect_recv(&mut sub_rx)
                    .await
                    .unwrap()
                    .checkpoint_summary
                    .sequence_number
            );
        }

        // First gap: should ingest 13-14
        for i in 13..15 {
            assert_eq!(Some(i), expect_recv(&mut cp_rx).await);
        }

        // Resume streaming 15-17
        for i in 15..18 {
            assert_eq!(
                i,
                expect_recv(&mut sub_rx)
                    .await
                    .unwrap()
                    .checkpoint_summary
                    .sequence_number
            );
        }

        // Second gap: should ingest 18-19
        for i in 18..20 {
            assert_eq!(Some(i), expect_recv(&mut cp_rx).await);
        }

        // Resume streaming 20-22
        for i in 20..23 {
            assert_eq!(
                i,
                expect_recv(&mut sub_rx)
                    .await
                    .unwrap()
                    .checkpoint_summary
                    .sequence_number
            );
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
            cancel.clone(),
        );

        // First few checkpoints stream normally
        for i in 10..13 {
            assert_eq!(
                i,
                expect_recv(&mut sub_rx)
                    .await
                    .unwrap()
                    .checkpoint_summary
                    .sequence_number
            );
        }

        for i in 13..13 + INGESTION_CHECK_INTERVAL {
            assert_eq!(Some(i), expect_recv(&mut cp_rx).await);
        }
        expect_timeout(&mut cp_rx).await;

        // After the error, streaming should restart and continue
        // The regulator should call start_streaming() again and continue from 13
        for i in 13 + INGESTION_CHECK_INTERVAL..30 {
            assert_eq!(
                i,
                expect_recv(&mut sub_rx)
                    .await
                    .unwrap()
                    .checkpoint_summary
                    .sequence_number
            );
        }

        cancel.cancel();
        h_regulator.await.unwrap();
    }

    #[tokio::test]
    async fn stream_error_during_transition() {
        // Initialize tracing for test output
        let _ = tracing_subscriber::fmt()
            .with_test_writer()
            .with_env_filter("info")
            .try_init();

        let (_, hi_rx) = mpsc::unbounded_channel();
        let (cp_tx, mut cp_rx) = mpsc::channel(50);
        let cancel = CancellationToken::new();

        // Create subscriber to receive broadcast checkpoints
        let (sub_tx, mut sub_rx) = mpsc::channel(50);
        let subscribers = vec![sub_tx];

        // Start with streaming checkpoint 10, 11, 12
        // Then jump to 25 (creates gap, enters transition mode)
        // Then error occurs during transition
        // Should fallback to ingestion with next_start set
        let mut streaming_service = MockStreamingService::new(vec![10, 11, 12]);
        streaming_service.insert_checkpoint_range(25..30); // This will trigger transition mode
        streaming_service.insert_error(); // Error during transition
                                          // After error recovery, continue with checkpoints
        streaming_service.insert_checkpoint_range(40..100);

        let metrics = test_metrics();
        let h_regulator = regulator(
            Some(streaming_service),
            10..100,
            0,
            hi_rx,
            cp_tx,
            subscribers,
            metrics,
            cancel.clone(),
        );

        // First 3 checkpoints should stream normally (Stream state)
        for i in 10..13 {
            assert_eq!(
                i,
                expect_recv(&mut sub_rx)
                    .await
                    .unwrap()
                    .checkpoint_summary
                    .sequence_number
            );
        }

        // Checkpoint 25-30 arrives, gap detected, enters Transition mode
        // Should receive  from streaming while also starting to ingest 13-24
        for i in 25..30 {
            assert_eq!(
                i,
                expect_recv(&mut sub_rx)
                    .await
                    .unwrap()
                    .checkpoint_summary
                    .sequence_number
            );
        }

        // Meanwhile, ingestion should start filling the gap
        for i in 13..25 {
            assert_eq!(Some(i), expect_recv(&mut cp_rx).await);
        }

        // Now error occurs during transition, should switch to Ingest mode
        // State becomes: Ingest { current: ingest_current, hi_exclusive: 25 (stream_start), next_start: Some(30) (stream_current) }
        // Since current == hi_exclusive, it will try to restart streaming, but next
        // streaming checkpoint is 40, so will ingest 30-39
        for i in 30..40 {
            assert_eq!(Some(i), expect_recv(&mut cp_rx).await);
        }

        // Receives 40 onwards from streaming
        for i in 40..50 {
            assert_eq!(
                i,
                expect_recv(&mut sub_rx)
                    .await
                    .unwrap()
                    .checkpoint_summary
                    .sequence_number
            );
        }

        cancel.cancel();
        h_regulator.await.unwrap();
    }
}
