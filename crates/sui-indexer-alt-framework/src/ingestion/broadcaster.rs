// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{collections::HashMap, sync::Arc, time::Duration};

use futures::{future::try_join_all, stream};
use tokio::{
    sync::{mpsc, watch},
    task::JoinHandle,
};
use tokio_util::sync::CancellationToken;
use tracing::{error, info};

use super::{IngestionConfig, client::IngestionClient};
use crate::{
    ingestion::{error::Error, streaming_service::StreamingService},
    metrics::IndexerMetrics,
    task::TrySpawnStreamExt,
    types::full_checkpoint_content::CheckpointData,
};

// TODO: Make these configurable via IngestionConfig.
/// If streaming fails to start, we'll use ingestion for this many more checkpoints before trying
/// to start streaming again.
const INGESTION_CHECK_INTERVAL: u64 = 10;

/// If the gap between the start of streaming and the current position of ingestion is less than this
/// threshold, we'll enter the transition mode where we ingest and stream at the same time before
/// filling up the gap, then switch to streaming.
const TRANSITION_THRESHOLD: u64 = 100;

/// Broadcaster task that manages checkpoint flow and spawns broadcast tasks for ranges.
///
/// This task:
/// 1. Maintains an ingest_hi based on subscriber feedback
/// 2. Spawns a broadcaster task for the requested checkpoint range. Right now the entire requested range is given to
///    each broadcaster, but with streaming support the end of the range could be different.
/// 3. The broadcast_range task waits on the watch channel when it hits the ingest_hi limit
///
/// The task will shut down if the `cancel` token is signalled, or if the `checkpoints` range completes.
pub(super) fn broadcaster<R, S>(
    checkpoints: R,
    initial_commit_hi: Option<u64>,
    mut streaming_service: Option<S>,
    config: IngestionConfig,
    client: IngestionClient,
    mut ingest_hi_rx: mpsc::UnboundedReceiver<(&'static str, u64)>,
    subscribers: Vec<mpsc::Sender<Arc<CheckpointData>>>,
    metrics: Arc<IndexerMetrics>,
    cancel: CancellationToken,
) -> JoinHandle<()>
where
    R: std::ops::RangeBounds<u64> + Send + 'static,
    S: StreamingService + Send + 'static,
{
    let retry_interval = config.retry_interval();
    let concurrency = config.ingest_concurrency;

    tokio::spawn(async move {
        info!("Starting broadcaster");

        // Extract start and end from the range bounds
        let start_cp = match checkpoints.start_bound() {
            std::ops::Bound::Included(&n) => n,
            std::ops::Bound::Excluded(&n) => n + 1,
            std::ops::Bound::Unbounded => 0,
        };

        let end_cp = match checkpoints.end_bound() {
            std::ops::Bound::Included(&n) => n,
            std::ops::Bound::Excluded(&n) => n.saturating_sub(1),
            std::ops::Bound::Unbounded => u64::MAX,
        };

        let buffer_size = config.checkpoint_buffer_size;

        let subscribers = Arc::new(subscribers);

        // Track subscriber watermarks
        let mut subscribers_hi = HashMap::<&'static str, u64>::new();

        // Try to receive initial subscriber feedback before spawning the broadcaster.
        // This ensures we don't miss early watermark updates.
        if let Ok((name, hi)) = ingest_hi_rx.try_recv() {
            subscribers_hi.insert(name, hi);
        }

        // Initialize ingest_hi watch channel.
        // Start with None (no backpressure) or Some if we have been provided aninitial bound.
        let initial_ingest_hi = initial_commit_hi.map(|min_hi| min_hi + buffer_size as u64);
        let (ingest_hi_watch_tx, ingest_hi_watch_rx) = watch::channel(initial_ingest_hi);

        let mut watermark = start_cp;

        'outer: loop {
            if watermark > end_cp {
                info!("Reached end of requested checkpoint range, stopping broadcaster");
                break;
            }

            let (mut broadcaster_handle, mut ingestion_range) = (None, None);
            let mut is_streaming = false;

            if let Some(streaming_service) = &mut streaming_service {
                if let Err(e) = streaming_service.start_streaming().await {
                    error!("Failed to start streaming service with error {}, falling back to ingestion only for {} checkpoints", e, INGESTION_CHECK_INTERVAL);
                    let ingestion_end = (watermark + INGESTION_CHECK_INTERVAL - 1).min(end_cp);
                    ingestion_range = Some((watermark, ingestion_end));
                    watermark = ingestion_end.saturating_add(1);
                } else {
                    match streaming_service.peek_next_checkpoint().await {
                        Ok(streamed_cp) => {
                            if streamed_cp >= end_cp {
                                info!("Network is too far ahead at {} beyond end_cp {}, ingesting only", streamed_cp, end_cp);
                                ingestion_range = Some((watermark, end_cp));
                                watermark = end_cp.saturating_add(1);
                            } else if streamed_cp <= watermark {
                                // We are already caught up, start only streaming from the watermark
                                info!("Caught up, starting streaming from watermark {}", watermark);
                                is_streaming = true;
                            } else if streamed_cp <= watermark + TRANSITION_THRESHOLD {
                                // We are within the transition threshold, so ingest and stream at the same time.
                                info!("Streaming within transition threshold at {}, with watermark {}, ingesting and streaming", streamed_cp, watermark);
                                ingestion_range = Some((watermark, streamed_cp.saturating_sub(1)));
                                is_streaming = true;
                                watermark = streamed_cp;
                            } else {
                                // We are beyond the transition threshold, so ingest up to the streamed_cp.
                                info!("Behind streaming by more than transition threshold at {}, with watermark {}, ingesting only until caught up", streamed_cp, watermark);
                                ingestion_range = Some((watermark, streamed_cp.saturating_sub(1)));
                                watermark = streamed_cp;
                            }
                        }
                        Err(e) => {
                            error!("Failed to peek next checkpoint from streaming service with error {}, falling back to ingestion only for {} checkpoints", e, INGESTION_CHECK_INTERVAL);
                            let ingestion_end =
                                (watermark + INGESTION_CHECK_INTERVAL - 1).min(end_cp);
                            ingestion_range = Some((watermark, ingestion_end));
                            watermark = ingestion_end.saturating_add(1);
                        }
                    }
                }
            } else {
                // No streaming service, so just ingest the entire range.
                ingestion_range = Some((watermark, end_cp));
                watermark = end_cp.saturating_add(1);
            }

            println!(
                "ingestion_range: {:?}, is_streaming: {}",
                ingestion_range, is_streaming
            );

            if let Some((broadcaster_start, broadcaster_end)) = ingestion_range {
                info!(
                    broadcaster_start,
                    broadcaster_end, "Starting broadcaster for checkpoint range"
                );

                // Spawn a broadcaster task for this range.
                // It will exit when the range is complete or if it is cancelled.
                broadcaster_handle = Some(tokio::spawn(broadcast_range(
                    broadcaster_start,
                    broadcaster_end,
                    retry_interval,
                    concurrency,
                    ingest_hi_watch_rx.clone(),
                    client.clone(),
                    subscribers.clone(),
                    cancel.clone(),
                )));
            }

            assert!(
                broadcaster_handle.is_some() || is_streaming,
                "Either broadcaster_handle or is_streaming must be set"
            );

            while broadcaster_handle.is_some() || is_streaming {
                tokio::select! {
                    _ = cancel.cancelled() => {
                        info!("Shutdown received, stopping ingestion");
                        break 'outer;
                    }

                    // Subscriber watermark update
                    // docs::#regulator (see docs/content/guides/developer/advanced/custom-indexer.mdx)
                    Some((name, hi)) = ingest_hi_rx.recv() => {
                        subscribers_hi.insert(name, hi);

                        if let Some(min_hi) = subscribers_hi.values().copied().min() {
                            ingest_hi = Some(min_hi + buffer_size as u64);
                            // Update the watch channel, which will notify all waiting tasks
                            let _ = ingest_hi_watch_tx.send(ingest_hi);
                            info!(ingest_hi, "Updated ingest_hi");
                        }
                    }
                    // docs::/#regulator

                    // Handle streaming of checkpoints
                    checkpoint_result = async {
                        // SAFETY: unwrap is safe because is_streaming guards against streaming_service being None.
                        streaming_service.as_mut().unwrap().next_checkpoint().await
                    }, if is_streaming => {
                        match checkpoint_result {
                            Ok(checkpoint) => {
                                let sequence_number = *checkpoint.checkpoint_summary.sequence_number();
                                info!(checkpoint = sequence_number, "Received streamed checkpoint");

                                // We reached the end of the requested range, stop streaming.
                                // We `continue` instead of `break` to allow broadcaster_handle to complete.
                                if sequence_number > end_cp {
                                    info!("Reached end of requested checkpoint range, stopping streaming");
                                    is_streaming = false;
                                    continue;
                                }

                                // We somehow got a checkpoint beyond the current watermark, which should not happen.
                                // We'll restart the streaming service to recover in the next outer loop iteration.
                                if sequence_number > watermark {
                                    info!(checkpoint = sequence_number, watermark, "Streamed checkpoint beyond watermark unexpectedly, stopping streaming");
                                    is_streaming = false;
                                    continue;
                                }

                                // We got a checkpoint we've already processed, so just skip it.
                                if sequence_number < watermark {
                                    // We are still catching up, so skip this checkpoint.
                                    info!(checkpoint = sequence_number, watermark, "Skipping already processed checkpoint");
                                    continue;
                                }

                                assert_eq!(sequence_number, watermark, "Watermark should match streamed checkpoint at this point");

                                if ingest_hi.is_some_and(|hi| sequence_number > hi) {
                                    // We hit the ingest_hi limit, so pause streaming until it advances.
                                    info!(checkpoint = sequence_number, ingest_hi, "Hit ingest_hi limit, pausing streaming");
                                    is_streaming = false;
                                    continue;
                                }

                                // Send checkpoint to all subscribers.
                                if let Err(e) = send_checkpoint(Arc::new(checkpoint), &subscribers).await {
                                    error!("Failed to send streamed checkpoint to subscribers: {}", e);
                                    // Treat this as a cancellation signal for the entire ingestion.
                                    cancel.cancel();
                                    break 'outer;
                                } else {
                                    info!(checkpoint = sequence_number, "Broadcasted streamed checkpoint");
                                    metrics.total_streamed_checkpoints.inc();
                                    watermark = sequence_number.saturating_add(1);
                                }
                            }
                            Err(e) => {
                                error!("Streaming service error: {}, pausing streaming", e);
                                is_streaming = false;
                            }
                        }
                    }

                    // Handle broadcaster task completion
                    result = async {
                        // SAFETY: unwrap is safe because we check is_some in the condition
                        broadcaster_handle.as_mut().unwrap().await
                    }, if broadcaster_handle.is_some() => {
                        match result {
                            Ok(Ok(())) => {
                                info!("Broadcaster completed successfully");
                                broadcaster_handle = None;
                                continue;
                            }
                            Ok(Err(Error::Cancelled)) => {
                                info!("Broadcaster was cancelled, finishing everything");
                                break 'outer;
                            }
                            Ok(Err(e)) => {
                                error!("Broadcaster failed: {}", e);
                                cancel.cancel();
                                break 'outer;
                            }
                            Err(e) => {
                                error!("Broadcaster task panicked: {}", e);
                                cancel.cancel();
                                break 'outer;
                            }
                        }
                    }
                }
            }
        }
    })
}

/// Fetch and broadcasts checkpoints from a range to subscribers.
/// This task is ingest_hi-aware and will wait if it encounters a checkpoint
/// beyond the current ingest_hi, resuming when ingest_hi advances to currently
/// ingesting checkpoints.
async fn broadcast_range(
    start: u64,
    end: u64,
    retry_interval: Duration,
    ingest_concurrency: usize,
    ingest_hi_rx: watch::Receiver<Option<u64>>,
    client: IngestionClient,
    subscribers: Arc<Vec<mpsc::Sender<Arc<CheckpointData>>>>,
    cancel: CancellationToken,
) -> Result<(), Error> {
    stream::iter(start..=end)
        .try_for_each_spawned(ingest_concurrency, |cp| {
            let mut ingest_hi_rx = ingest_hi_rx.clone();
            let client = client.clone();
            let subscribers = subscribers.clone();

            // One clone is for the supervisor to signal a cancel if it detects a
            // subscriber that wants to wind down ingestion, and the other is to pass to
            // each worker to detect cancellation.
            // let supervisor_cancel = cancel.clone();
            let cancel = cancel.clone();

            async move {
                // docs::#bound (see docs/content/guides/developer/advanced/custom-indexer.mdx)
                // Wait until ingest_hi allows processing this checkpoint.
                // None means no backpressure limit. If we get Some(hi) we wait until cp <= hi.
                // wait_for only errors if the sender is dropped (main broadcaster shut down) so
                // we treat an error returned here as cancellation too.
                if tokio::select! {
                    result = ingest_hi_rx.wait_for(|hi| hi.is_none_or(|h| cp < h)) => result.is_err(),
                    _ = cancel.cancelled() => true,
                } {
                    return Err(Error::Cancelled);
                }
                // docs::/#bound

                // Fetch the checkpoint or stop if cancelled.
                let checkpoint = tokio::select! {
                    cp = client.wait_for(cp, retry_interval) => cp?,
                    _ = cancel.cancelled() => {
                        return Err(Error::Cancelled);
                    }
                };

                // Send checkpoint to all subscribers.
                tokio::select! {
                    result = send_checkpoint(checkpoint, &subscribers) => {
                        if result.is_ok() {
                            info!(checkpoint = cp, "Broadcasted checkpoint");
                            Ok(())
                        } else {
                            // An error is returned meaning some subscriber channel has closed, which we consider
                            // a cancellation signal for the entire ingestion.
                            cancel.cancel();
                            Err(Error::Cancelled)
                        }
                    },
                    _ = cancel.cancelled() => Err(Error::Cancelled),
                }
            }
        })
        .await
}

/// Send a checkpoint to all subscribers.
/// Returns an error if any subscriber's channel is closed.
async fn send_checkpoint(
    checkpoint: Arc<CheckpointData>,
    subscribers: &[mpsc::Sender<Arc<CheckpointData>>],
) -> Result<Vec<()>, mpsc::error::SendError<Arc<CheckpointData>>> {
    let futures = subscribers.iter().map(|s| s.send(checkpoint.clone()));
    try_join_all(futures).await
}

#[cfg(test)]
mod tests {
    use std::fmt::Debug;
    use std::sync::Arc;
    use std::time::Duration;
    use tokio::time::{error::Elapsed, timeout};

    use super::*;
    use crate::ingestion::client::FetchData;
    use crate::ingestion::streaming_service::test_utils::MockStreamingService;
    use crate::ingestion::{test_utils::test_checkpoint_data, IngestionConfig};
    use crate::metrics::tests::test_metrics;

    /// Create a mock IngestionClient for tests
    fn mock_client(metrics: Arc<IndexerMetrics>) -> IngestionClient {
        use crate::ingestion::client::{FetchError, IngestionClientTrait};
        use async_trait::async_trait;

        struct MockClient;

        #[async_trait]
        impl IngestionClientTrait for MockClient {
            async fn fetch(&self, checkpoint: u64) -> Result<FetchData, FetchError> {
                // Return mock checkpoint data for any checkpoint number
                let bytes = test_checkpoint_data(checkpoint);
                Ok(FetchData::Raw(bytes.into()))
            }
        }

        IngestionClient::new_impl(Arc::new(MockClient), metrics)
    }

    /// Create a test config
    fn test_config() -> IngestionConfig {
        IngestionConfig {
            checkpoint_buffer_size: 5,
            ingest_concurrency: 2,
            retry_interval_ms: 100,
        }
    }

    /// Wait up to a second for a response on the channel, and return it, expecting this operation
    /// to succeed.
    async fn expect_recv<T>(rx: &mut mpsc::Receiver<T>) -> Option<T> {
        timeout(Duration::from_secs(1), rx.recv()).await.unwrap()
    }

    /// Wait up to a second for a response on the channel, but expecting this operation to timeout.
    async fn expect_timeout<T: Debug>(rx: &mut mpsc::Receiver<T>) -> Elapsed {
        timeout(Duration::from_secs(1), rx.recv())
            .await
            .unwrap_err()
    }

    /// Verify that a channel receives checkpoints in the given range exactly once each, in any order.
    /// We need this because with ingest concurrency the order is not guaranteed.
    async fn expect_checkpoints_in_range<R>(rx: &mut mpsc::Receiver<Arc<CheckpointData>>, range: R)
    where
        R: Iterator<Item = u64>,
    {
        use std::collections::HashSet;

        let expected: HashSet<u64> = range.collect();
        let mut received: HashSet<u64> = HashSet::new();

        for _ in 0..expected.len() {
            let checkpoint = expect_recv(rx).await.unwrap();
            let seq = *checkpoint.checkpoint_summary.sequence_number();
            assert!(
                expected.contains(&seq),
                "Received unexpected checkpoint {seq}",
            );
            assert!(received.insert(seq), "Received duplicate checkpoint {seq}",);
        }
    }

    /// Verify that a channel receives checkpoints in sequential order without gaps.
    /// Use this when checkpoints must arrive in strict order (e.g., from streaming).
    async fn expect_checkpoints_in_order<R>(rx: &mut mpsc::Receiver<Arc<CheckpointData>>, range: R)
    where
        R: Iterator<Item = u64>,
    {
        let expected: Vec<u64> = range.collect();

        for (i, &expected_seq) in expected.iter().enumerate() {
            let checkpoint = expect_recv(rx).await.unwrap();
            let seq = *checkpoint.checkpoint_summary.sequence_number();
            assert_eq!(
                seq, expected_seq,
                "Expected checkpoint {} at position {}, but got {}",
                expected_seq, i, seq
            );
        }
    }

    #[tokio::test]
    async fn finite_list_of_checkpoints() {
        let (_, hi_rx) = mpsc::unbounded_channel();
        let (subscriber_tx, mut subscriber_rx) = mpsc::channel(1);
        let cancel = CancellationToken::new();

        let cps = 0..5;
        let metrics = test_metrics();
        let h_broadcaster = broadcaster::<_, MockStreamingService>(
            cps,
            None,
            None,
            test_config(),
            mock_client(metrics.clone()),
            hi_rx,
            vec![subscriber_tx],
            metrics,
            cancel.clone(),
        );

        expect_checkpoints_in_range(&mut subscriber_rx, 0..5).await;

        cancel.cancel();
        h_broadcaster.await.unwrap();
    }

    #[tokio::test]
    async fn shutdown_on_sender_closed() {
        let (_, hi_rx) = mpsc::unbounded_channel();
        let (subscriber_tx, mut subscriber_rx) = mpsc::channel(1);
        let cancel = CancellationToken::new();

        let metrics = test_metrics();
        let h_broadcaster = broadcaster::<_, MockStreamingService>(
            0..,
            None,
            None,
            test_config(),
            mock_client(metrics.clone()),
            hi_rx,
            vec![subscriber_tx],
            metrics,
            cancel.clone(),
        );

        expect_checkpoints_in_range(&mut subscriber_rx, 0..5).await;

        drop(subscriber_rx);
        h_broadcaster.await.unwrap();
    }

    #[tokio::test]
    async fn shutdown_on_cancel() {
        let (_, hi_rx) = mpsc::unbounded_channel();
        let (subscriber_tx, mut subscriber_rx) = mpsc::channel(1);
        let cancel = CancellationToken::new();

        let metrics = test_metrics();
        let h_broadcaster = broadcaster::<_, MockStreamingService>(
            0..,
            None,
            None,
            test_config(),
            mock_client(metrics.clone()),
            hi_rx,
            vec![subscriber_tx],
            metrics,
            cancel.clone(),
        );

        expect_checkpoints_in_range(&mut subscriber_rx, 0..5).await;

        cancel.cancel();
        h_broadcaster.await.unwrap();
    }

    #[tokio::test]
    async fn halted() {
        let (_, hi_rx) = mpsc::unbounded_channel();
        let (subscriber_tx, mut subscriber_rx) = mpsc::channel(1);
        let cancel = CancellationToken::new();

        let mut config = test_config();
        config.checkpoint_buffer_size = 0; // No buffer

        let metrics = test_metrics();
        let h_broadcaster = broadcaster::<_, MockStreamingService>(
            0..,
            Some(4),
            None,
            config,
            mock_client(metrics.clone()),
            hi_rx,
            vec![subscriber_tx],
            metrics,
            cancel.clone(),
        );
        expect_checkpoints_in_range(&mut subscriber_rx, 0..4).await;
        // Regulator stopped because of watermark.
        expect_timeout(&mut subscriber_rx).await;

        cancel.cancel();
        h_broadcaster.await.unwrap();
    }

    #[tokio::test]
    async fn halted_buffered() {
        let (_, hi_rx) = mpsc::unbounded_channel();
        let (subscriber_tx, mut subscriber_rx) = mpsc::channel(1);
        let cancel = CancellationToken::new();

        let mut config = test_config();
        config.checkpoint_buffer_size = 2; // Buffer of 2

        let metrics = test_metrics();
        let h_broadcaster = broadcaster::<_, MockStreamingService>(
            0..,
            Some(2),
            None,
            config,
            mock_client(metrics.clone()),
            hi_rx,
            vec![subscriber_tx],
            metrics,
            cancel.clone(),
        );

        expect_checkpoints_in_range(&mut subscriber_rx, 0..4).await;

        // Regulator stopped because of watermark (plus buffering).
        expect_timeout(&mut subscriber_rx).await;

        cancel.cancel();
        h_broadcaster.await.unwrap();
    }

    #[tokio::test]
    async fn resumption() {
        let (hi_tx, hi_rx) = mpsc::unbounded_channel();
        let (subscriber_tx, mut subscriber_rx) = mpsc::channel(1);
        let cancel = CancellationToken::new();

        let mut config = test_config();
        config.checkpoint_buffer_size = 0; // No buffer

        let metrics = test_metrics();
        let h_broadcaster = broadcaster::<_, MockStreamingService>(
            0..,
            Some(2),
            None,
            config,
            mock_client(metrics.clone()),
            hi_rx,
            vec![subscriber_tx],
            metrics,
            cancel.clone(),
        );

        expect_checkpoints_in_range(&mut subscriber_rx, 0..2).await;

        // Regulator stopped because of watermark, but resumes when that watermark is updated.
        expect_timeout(&mut subscriber_rx).await;
        hi_tx.send(("test", 4)).unwrap();

        expect_checkpoints_in_range(&mut subscriber_rx, 2..4).await;

        // Halted again.
        expect_timeout(&mut subscriber_rx).await;

        cancel.cancel();
        h_broadcaster.await.unwrap();
    }

    #[tokio::test]
    async fn multiple_subscribers() {
        let (hi_tx, hi_rx) = mpsc::unbounded_channel();
        let (subscriber_tx, mut subscriber_rx) = mpsc::channel(1);
        let cancel = CancellationToken::new();

        hi_tx.send(("a", 2)).unwrap();
        hi_tx.send(("b", 3)).unwrap();

        let mut config = test_config();
        config.checkpoint_buffer_size = 0; // No buffer

        let cps = 0..10;
        let metrics = test_metrics();
        let h_broadcaster = broadcaster::<_, MockStreamingService>(
            cps,
            Some(2),
            None,
            config,
            mock_client(metrics.clone()),
            hi_rx,
            vec![subscriber_tx],
            metrics,
            cancel.clone(),
        );

        expect_checkpoints_in_range(&mut subscriber_rx, 0..2).await;

        // Watermark stopped because of a's watermark.
        expect_timeout(&mut subscriber_rx).await;

        // Updating b's watermark doesn't make a difference.
        hi_tx.send(("b", 4)).unwrap();
        expect_timeout(&mut subscriber_rx).await;

        // But updating a's watermark does.
        hi_tx.send(("a", 3)).unwrap();
        let checkpoint = expect_recv(&mut subscriber_rx).await.unwrap();
        assert_eq!(*checkpoint.checkpoint_summary.sequence_number(), 2);

        // ...by one checkpoint.
        expect_timeout(&mut subscriber_rx).await;

        // And we can make more progress by updating it again.
        hi_tx.send(("a", 4)).unwrap();
        let checkpoint = expect_recv(&mut subscriber_rx).await.unwrap();
        assert_eq!(*checkpoint.checkpoint_summary.sequence_number(), 3);

        // But another update to "a" will now not make a difference, because "b" is still behind.
        hi_tx.send(("a", 5)).unwrap();
        expect_timeout(&mut subscriber_rx).await;

        cancel.cancel();
        h_broadcaster.await.unwrap();
    }

    #[tokio::test]
    async fn multiple_physical_subscribers() {
        let (_, hi_rx) = mpsc::unbounded_channel();
        let (subscriber_tx1, mut subscriber_rx1) = mpsc::channel(1);
        let (subscriber_tx2, mut subscriber_rx2) = mpsc::channel(1);
        let cancel = CancellationToken::new();

        let metrics = test_metrics();
        let h_broadcaster = broadcaster::<_, MockStreamingService>(
            0..,
            None,
            None,
            test_config(),
            mock_client(metrics.clone()),
            hi_rx,
            vec![subscriber_tx1, subscriber_tx2],
            metrics,
            cancel.clone(),
        );

        // Both subscribers should receive checkpoints
        expect_checkpoints_in_range(&mut subscriber_rx1, 0..3).await;
        expect_checkpoints_in_range(&mut subscriber_rx2, 0..3).await;

        // Drop one subscriber - this should cause the broadcaster to shut down
        drop(subscriber_rx1);

        // The broadcaster should shut down gracefully
        h_broadcaster.await.unwrap();
    }

    #[tokio::test]
    async fn start_from_non_zero() {
        let (hi_tx, hi_rx) = mpsc::unbounded_channel();
        let (subscriber_tx, mut subscriber_rx) = mpsc::channel(1);
        let cancel = CancellationToken::new();

        // Set watermark before starting
        hi_tx.send(("test", 1005)).unwrap();

        let mut config = test_config();
        config.checkpoint_buffer_size = 0; // No buffer

        let metrics = test_metrics();
        let h_broadcaster = broadcaster::<_, MockStreamingService>(
            1000..1010,
            Some(1005),
            None,
            config,
            mock_client(metrics.clone()),
            hi_rx,
            vec![subscriber_tx],
            metrics,
            cancel.clone(),
        );

        // Should receive checkpoints starting from 1000
        expect_checkpoints_in_range(&mut subscriber_rx, 1000..1005).await;

        // Should halt at watermark
        expect_timeout(&mut subscriber_rx).await;

        // Update watermark to allow completion
        hi_tx.send(("test", 1010)).unwrap();

        expect_checkpoints_in_range(&mut subscriber_rx, 1005..1010).await;

        cancel.cancel();
        h_broadcaster.await.unwrap();
    }

    #[tokio::test]
    async fn streaming_only() {
        telemetry_subscribers::init_for_testing();
        let (_, hi_rx) = mpsc::unbounded_channel();
        let (subscriber_tx, mut subscriber_rx) = mpsc::channel(10);
        let cancel = CancellationToken::new();

        // Create a mock streaming service with checkpoints 0..5
        let streaming_service = MockStreamingService::new(0..5);

        let metrics = test_metrics();
        let h_broadcaster = broadcaster(
            0..5, // Bounded range
            None,
            Some(streaming_service),
            test_config(),
            mock_client(metrics.clone()),
            hi_rx,
            vec![subscriber_tx],
            metrics.clone(),
            cancel.clone(),
        );

        // Should receive all checkpoints from the stream in order
        expect_checkpoints_in_order(&mut subscriber_rx, 0..5).await;

        // We should get all checkpoints from streaming.
        assert_eq!(metrics.total_streamed_checkpoints.get(), 5);
        assert_eq!(metrics.total_ingested_checkpoints.get(), 0);

        cancel.cancel();
        h_broadcaster.await.unwrap();
    }

    #[tokio::test]
    async fn streaming_with_transition() {
        telemetry_subscribers::init_for_testing();
        let (_, hi_rx) = mpsc::unbounded_channel();
        let (subscriber_tx, mut subscriber_rx) = mpsc::channel(100);
        let cancel = CancellationToken::new();

        // Create a mock streaming service that starts at checkpoint 50
        // This simulates streaming being ahead of ingestion
        let streaming_service = MockStreamingService::new(50..60);

        let metrics = test_metrics();
        let h_broadcaster = broadcaster(
            0..60,
            None,
            Some(streaming_service),
            test_config(),
            mock_client(metrics.clone()),
            hi_rx,
            vec![subscriber_tx],
            metrics.clone(),
            cancel.clone(),
        );

        expect_checkpoints_in_range(&mut subscriber_rx, 0..60).await;

        // Verify both ingestion and streaming were used
        assert_eq!(metrics.total_ingested_checkpoints.get(), 50);
        assert_eq!(metrics.total_streamed_checkpoints.get(), 10);

        cancel.cancel();
        h_broadcaster.await.unwrap();
    }

    #[tokio::test]
    async fn streaming_start_failure_fallback_to_ingestion() {
        telemetry_subscribers::init_for_testing();
        let (_, hi_rx) = mpsc::unbounded_channel();
        let (subscriber_tx, mut subscriber_rx) = mpsc::channel(20);
        let cancel = CancellationToken::new();

        // Streaming service that fails to start
        let streaming_service = MockStreamingService::new(10..20).fail_start_streaming_times(1);

        let metrics = test_metrics();
        let h_broadcaster = broadcaster(
            0..20,
            None,
            Some(streaming_service),
            test_config(),
            mock_client(metrics.clone()),
            hi_rx,
            vec![subscriber_tx],
            metrics.clone(),
            cancel.clone(),
        );

        // Should fallback to ingestion for INGESTION_CHECK_INTERVAL (10) checkpoints
        expect_checkpoints_in_range(&mut subscriber_rx, 0..10).await;

        // After the interval, it should complete the remaining checkpoints from streaming
        expect_checkpoints_in_order(&mut subscriber_rx, 10..20).await;

        assert_eq!(metrics.total_ingested_checkpoints.get(), 10);
        assert_eq!(metrics.total_streamed_checkpoints.get(), 10);

        cancel.cancel();
        h_broadcaster.await.unwrap();
    }

    #[tokio::test]
    async fn streaming_peek_failure_fallback_to_ingestion() {
        telemetry_subscribers::init_for_testing();
        let (_, hi_rx) = mpsc::unbounded_channel();
        let (subscriber_tx, mut subscriber_rx) = mpsc::channel(20);
        let cancel = CancellationToken::new();

        // Streaming service where peek fails
        let streaming_service = MockStreamingService::new(10..20).fail_peek_once();

        let metrics = test_metrics();
        let h_broadcaster = broadcaster(
            0..20,
            None,
            Some(streaming_service),
            test_config(),
            mock_client(metrics.clone()),
            hi_rx,
            vec![subscriber_tx],
            metrics.clone(),
            cancel.clone(),
        );

        // Should fallback to ingestion for first 10 checkpoints
        expect_checkpoints_in_range(&mut subscriber_rx, 0..10).await;

        // Then stream the remaining
        expect_checkpoints_in_order(&mut subscriber_rx, 10..20).await;

        // Verify both were used
        assert_eq!(metrics.total_ingested_checkpoints.get(), 10);
        assert_eq!(metrics.total_streamed_checkpoints.get(), 10);

        cancel.cancel();
        h_broadcaster.await.unwrap();
    }

    #[tokio::test]
    async fn streaming_beyond_end_checkpoint() {
        telemetry_subscribers::init_for_testing();
        let (_, hi_rx) = mpsc::unbounded_channel();
        let (subscriber_tx, mut subscriber_rx) = mpsc::channel(30);
        let cancel = CancellationToken::new();

        // Streaming service starts at checkpoint 100, but we only want 0..30.
        let streaming_service = MockStreamingService::new(100..110);

        let metrics = test_metrics();
        let h_broadcaster = broadcaster(
            0..30,
            None,
            Some(streaming_service),
            test_config(),
            mock_client(metrics.clone()),
            hi_rx,
            vec![subscriber_tx],
            metrics.clone(),
            cancel.clone(),
        );

        // Should use only ingestion since streaming is beyond end_cp
        expect_checkpoints_in_range(&mut subscriber_rx, 0..30).await;

        // Verify no streaming was used (all from ingestion)
        assert_eq!(metrics.total_streamed_checkpoints.get(), 0);
        assert_eq!(metrics.total_ingested_checkpoints.get(), 30);

        cancel.cancel();
        h_broadcaster.await.unwrap();
    }

    #[tokio::test]
    async fn streaming_error_during_streaming() {
        telemetry_subscribers::init_for_testing();
        let (_, hi_rx) = mpsc::unbounded_channel();
        let (subscriber_tx, mut subscriber_rx) = mpsc::channel(20);
        let cancel = CancellationToken::new();

        // Create streaming service with error injected mid-stream
        let mut streaming_service = MockStreamingService::new(0..5);
        streaming_service.insert_error(); // Error after 5 checkpoints
        streaming_service.insert_checkpoint_range(10..15);

        let metrics = test_metrics();
        let h_broadcaster = broadcaster(
            0..15,
            None,
            Some(streaming_service),
            test_config(),
            mock_client(metrics.clone()),
            hi_rx,
            vec![subscriber_tx],
            metrics.clone(),
            cancel.clone(),
        );

        // Should receive first 5 checkpoints from streaming in order
        expect_checkpoints_in_order(&mut subscriber_rx, 0..5).await;

        // After error, should fallback and complete via ingestion/retry (order not guaranteed)
        expect_checkpoints_in_range(&mut subscriber_rx, 5..15).await;

        // Verify streaming was used initially
        assert_eq!(metrics.total_streamed_checkpoints.get(), 10);
        assert_eq!(metrics.total_ingested_checkpoints.get(), 5);

        cancel.cancel();
        h_broadcaster.await.unwrap();
    }

    #[tokio::test]
    async fn streaming_failure_then_retry_success() {
        telemetry_subscribers::init_for_testing();
        let (_, hi_rx) = mpsc::unbounded_channel();
        let (subscriber_tx, mut subscriber_rx) = mpsc::channel(50);
        let cancel = CancellationToken::new();

        // Fail first start attempt, succeed on retry
        let streaming_service = MockStreamingService::new(10..25).fail_start_streaming_times(1);

        let metrics = test_metrics();
        let h_broadcaster = broadcaster(
            0..25,
            None,
            Some(streaming_service),
            test_config(),
            mock_client(metrics.clone()),
            hi_rx,
            vec![subscriber_tx],
            metrics.clone(),
            cancel.clone(),
        );

        expect_checkpoints_in_range(&mut subscriber_rx, 0..10).await;
        expect_checkpoints_in_order(&mut subscriber_rx, 10..25).await;

        // Verify both ingestion and streaming were used
        assert_eq!(metrics.total_ingested_checkpoints.get(), 10);
        assert_eq!(metrics.total_streamed_checkpoints.get(), 15);

        cancel.cancel();
        h_broadcaster.await.unwrap();
    }

    #[tokio::test]
    async fn streaming_multiple_errors_with_recovery() {
        telemetry_subscribers::init_for_testing();
        let (_, hi_rx) = mpsc::unbounded_channel();
        let (subscriber_tx, mut subscriber_rx) = mpsc::channel(50);
        let cancel = CancellationToken::new();

        // Create streaming with multiple errors injected
        let mut streaming_service = MockStreamingService::new(0..5);
        streaming_service.insert_error(); // Error at checkpoint 5
        streaming_service.insert_checkpoint_range(5..10);
        streaming_service.insert_error(); // Error at checkpoint 10
        streaming_service.insert_checkpoint_range(10..20);

        let metrics = test_metrics();
        let h_broadcaster = broadcaster(
            0..20,
            None,
            Some(streaming_service),
            test_config(),
            mock_client(metrics.clone()),
            hi_rx,
            vec![subscriber_tx],
            metrics.clone(),
            cancel.clone(),
        );

        // Should eventually receive all checkpoints despite errors from streaming.
        expect_checkpoints_in_order(&mut subscriber_rx, 0..20).await;

        assert_eq!(metrics.total_streamed_checkpoints.get(), 20);
        assert_eq!(metrics.total_ingested_checkpoints.get(), 0);

        cancel.cancel();
        h_broadcaster.await.unwrap();
    }

    #[tokio::test]
    async fn streaming_empty_then_recovery() {
        telemetry_subscribers::init_for_testing();
        let (_, hi_rx) = mpsc::unbounded_channel();
        let (subscriber_tx, mut subscriber_rx) = mpsc::channel(20);
        let cancel = CancellationToken::new();

        // Streaming service with no checkpoints
        let streaming_service = MockStreamingService::new(0..0);

        let metrics = test_metrics();
        let h_broadcaster = broadcaster(
            0..15,
            None,
            Some(streaming_service),
            test_config(),
            mock_client(metrics.clone()),
            hi_rx,
            vec![subscriber_tx],
            metrics.clone(),
            cancel.clone(),
        );

        // Should fallback to ingestion and complete
        expect_checkpoints_in_range(&mut subscriber_rx, 0..15).await;

        // Verify only ingestion was used
        assert_eq!(metrics.total_ingested_checkpoints.get(), 15);
        assert_eq!(metrics.total_streamed_checkpoints.get(), 0);

        cancel.cancel();
        h_broadcaster.await.unwrap();
    }

    #[tokio::test]
    async fn streaming_behind_watermark_skips_duplicates() {
        telemetry_subscribers::init_for_testing();
        let (_, hi_rx) = mpsc::unbounded_channel();
        let (subscriber_tx, mut subscriber_rx) = mpsc::channel(50);
        let cancel = CancellationToken::new();

        // Create streaming service that returns some checkpoints behind the watermark
        let mut streaming_service = MockStreamingService::new(0..15);
        // Insert duplicate/old checkpoints that should be skipped
        streaming_service.insert_checkpoint(3); // Behind watermark
        streaming_service.insert_checkpoint(4); // Behind watermark
        streaming_service.insert_checkpoint_range(15..20);

        let metrics = test_metrics();
        let h_broadcaster = broadcaster(
            0..20,
            None,
            Some(streaming_service),
            test_config(),
            mock_client(metrics.clone()),
            hi_rx,
            vec![subscriber_tx],
            metrics.clone(),
            cancel.clone(),
        );

        // Should receive all checkpoints exactly once (no duplicates) from streaming.
        expect_checkpoints_in_order(&mut subscriber_rx, 0..20).await;

        assert_eq!(metrics.total_streamed_checkpoints.get(), 20);
        assert_eq!(metrics.total_ingested_checkpoints.get(), 0);

        cancel.cancel();
        h_broadcaster.await.unwrap();
    }

    #[tokio::test]
    async fn streaming_ahead_of_watermark_recovery() {
        telemetry_subscribers::init_for_testing();
        let (_, hi_rx) = mpsc::unbounded_channel();
        let (subscriber_tx, mut subscriber_rx) = mpsc::channel(50);
        let cancel = CancellationToken::new();

        // Create streaming that has a gap (checkpoint ahead of expected watermark)
        let mut streaming_service = MockStreamingService::new(0..3);
        streaming_service.insert_checkpoint_range(6..10); // Gap: skips checkpoints 3 - 5

        let metrics = test_metrics();
        let h_broadcaster = broadcaster(
            0..10,
            None,
            Some(streaming_service),
            test_config(),
            mock_client(metrics.clone()),
            hi_rx,
            vec![subscriber_tx],
            metrics.clone(),
            cancel.clone(),
        );

        // Should receive first three checkpoints from streaming in order
        expect_checkpoints_in_order(&mut subscriber_rx, 0..3).await;

        // Then should fallback to ingestion for 3-5, and streaming continues for 6-9
        expect_checkpoints_in_range(&mut subscriber_rx, 3..10).await;

        assert_eq!(metrics.total_streamed_checkpoints.get(), 6);
        assert_eq!(metrics.total_ingested_checkpoints.get(), 4);

        cancel.cancel();
        h_broadcaster.await.unwrap();
    }

    #[tokio::test]
    async fn streaming_at_transition_threshold_boundary() {
        telemetry_subscribers::init_for_testing();
        let (_, hi_rx) = mpsc::unbounded_channel();
        let (subscriber_tx, mut subscriber_rx) = mpsc::channel(150);
        let cancel = CancellationToken::new();

        // TRANSITION_THRESHOLD is 100
        // Test streaming at exactly watermark + 100
        let streaming_service = MockStreamingService::new(100..110);

        let metrics = test_metrics();
        let h_broadcaster = broadcaster(
            0..110,
            100,
            Some(streaming_service),
            test_config(),
            mock_client(metrics.clone()),
            hi_rx,
            vec![subscriber_tx],
            metrics.clone(),
            cancel.clone(),
        );

        // Should handle transition correctly
        expect_checkpoints_in_range(&mut subscriber_rx, 0..110).await;

        assert_eq!(metrics.total_streamed_checkpoints.get(), 10);
        assert_eq!(metrics.total_ingested_checkpoints.get(), 100);

        cancel.cancel();
        h_broadcaster.await.unwrap();
    }

    #[tokio::test]
    async fn streaming_with_backpressure() {
        telemetry_subscribers::init_for_testing();
        let (hi_tx, hi_rx) = mpsc::unbounded_channel();
        let (subscriber_tx, mut subscriber_rx) = mpsc::channel(30);
        let cancel = CancellationToken::new();

        let streaming_service = MockStreamingService::new(0..20);

        let mut config = test_config();
        config.checkpoint_buffer_size = 0; // No buffer

        let metrics = test_metrics();
        let h_broadcaster = broadcaster(
            0..20,
            Some(10), // initial watermark to trigger backpressure
            Some(streaming_service),
            config,
            mock_client(metrics.clone()),
            hi_rx,
            vec![subscriber_tx],
            metrics.clone(),
            cancel.clone(),
        );

        // Should receive first 11 checkpoints (0..=10) from streaming
        expect_checkpoints_in_order(&mut subscriber_rx, 0..=10).await;

        // Should halt due to backpressure
        expect_timeout(&mut subscriber_rx).await;

        // Update watermark to make progress
        hi_tx.send(("test", 20)).unwrap();

        // Should receive remaining checkpoints
        expect_checkpoints_in_range(&mut subscriber_rx, 11..20).await;

        assert_eq!(metrics.total_streamed_checkpoints.get(), 18);
        assert_eq!(metrics.total_ingested_checkpoints.get(), 2);

        cancel.cancel();
        h_broadcaster.await.unwrap();
    }
}
