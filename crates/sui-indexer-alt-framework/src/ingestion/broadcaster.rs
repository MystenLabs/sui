// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{collections::HashMap, sync::Arc, time::Duration};

use futures::{future::try_join_all, stream, Stream, StreamExt};
use tokio::{
    sync::{mpsc, watch},
    task::JoinHandle,
};
use tokio_util::sync::CancellationToken;
use tracing::{error, info, warn};

use super::{IngestionConfig, client::IngestionClient};
use crate::{
    ingestion::{
        error::Error,
        streaming_service::{PeekableStream, StreamingService},
    },
    metrics::IndexerMetrics,
    task::TrySpawnStreamExt,
    types::full_checkpoint_content::CheckpointData,
};

/// Broadcaster task that manages checkpoint flow and spawns broadcast tasks for ranges
/// via either streaming or ingesting, or both.
///
/// This task:
/// 1. Maintains an ingest_hi based on subscriber feedback.
/// 2. Spawns streaming or ingesting tasks for the requested checkpoint range. Depending on
///    the current latest checkpoint available from streaming, it may spawn either or both tasks
///    to cover the requested range. The overall idea is that ingestion covers the range
///    [start, network_latest_cp), while streaming covers [network_latest_cp, end).
/// 3. When both the streaming and ingesting task finish due to failures or range completion, it
///    updates the overall watermark and runs a new loop iteration if the requested range is not complete.
/// 4. Both the ingest_and_broadcast_range and stream_and_broadcast_range tasks wait on the watch
///    channel when they hit the ingest_hi limit.
///
/// The task will shut down if the `cancel` token is signalled, or if the `checkpoints` range completes.
pub(super) fn broadcaster<R, S>(
    checkpoints: R,
    initial_commit_hi: Option<u64>,
    mut streaming_service: Option<S>,
    config: IngestionConfig,
    client: IngestionClient,
    mut commit_hi_rx: mpsc::UnboundedReceiver<(&'static str, u64)>,
    subscribers: Vec<mpsc::Sender<Arc<CheckpointData>>>,
    metrics: Arc<IndexerMetrics>,
    cancel: CancellationToken,
) -> JoinHandle<()>
where
    R: std::ops::RangeBounds<u64> + Send + 'static,
    S: StreamingService + Send + 'static,
{
    tokio::spawn(async move {
        info!("Starting broadcaster");

        // Extract start and end from the range bounds
        let start_cp = match checkpoints.start_bound() {
            std::ops::Bound::Included(&n) => n,
            std::ops::Bound::Excluded(&n) => n + 1,
            std::ops::Bound::Unbounded => 0,
        };

        let end_cp = match checkpoints.end_bound() {
            std::ops::Bound::Included(&n) => n.saturating_add(1),
            std::ops::Bound::Excluded(&n) => n,
            std::ops::Bound::Unbounded => u64::MAX,
        };

        let buffer_size = config.checkpoint_buffer_size as u64;

        let subscribers = Arc::new(subscribers);

        // Track subscriber watermarks
        let mut subscribers_hi = HashMap::<&'static str, u64>::new();

        // Initialize ingest_hi watch channel.
        // Start with None (no backpressure) or Some if we have been provided an initial bound.
        let mut ingest_hi = initial_commit_hi.map(|min_hi| min_hi + buffer_size);
        let (ingest_hi_watch_tx, ingest_hi_watch_rx) = watch::channel(ingest_hi);

        // Backoff state for streaming connection retries.
        // If the first attempt at streaming connection fails, we back off for an initial delay,
        // and keep broadcasting ingestion_batch_size checkpoints via ingestion until the backoff period ends.
        // On each subsequent failure, we double the backoff delay until reaching the max backoff delay.
        let mut streaming_backoff_until = tokio::time::Instant::now();
        let mut streaming_backoff_delay = config.streaming_backoff_initial_delay();
        let max_backoff_delay = config.streaming_backoff_max_delay();
        let ingestion_batch_size = config.ingestion_batch_size as u64;

        // Helper macro to create a dummy streaming handle that just returns
        // the provided checkpoint_hi immediately.
        // We use this to simplify the join logic when streaming is not used.
        macro_rules! dummy_streaming_handle {
            ($checkpoint_hi:expr) => {
                tokio::spawn(async move { $checkpoint_hi })
            };
        }

        // Initialize the overall checkpoint_hi watermark to start_cp.
        // This value is updated every outer loop iteration
        // after both streaming and broadcasting complete.
        let mut checkpoint_hi = start_cp;

        'outer: while checkpoint_hi < end_cp {
            let (streaming_handle, ingestion_end) =
                if let Some(streaming_service) = &mut streaming_service {
                    // Attempt to connect to streaming service if not in backoff period.
                    if tokio::time::Instant::now() < streaming_backoff_until {
                        // Still in backoff period, skip connection attempt
                        info!(
                            delay_secs = streaming_backoff_delay.as_secs(),
                            "In streaming backoff period, skipping connection attempt"
                        );

                        // We broadcast a batch of checkpoints via ingestion before retrying in next loop iteration.
                        let ingestion_end = (checkpoint_hi + ingestion_batch_size).min(end_cp);
                        (dummy_streaming_handle!(ingestion_end), ingestion_end)
                    } else {
                        // Backoff period elapsed, attempt connection
                        async {
                            let mut stream = streaming_service.connect().await?;
                            match stream.peek().await {
                                Some(Ok(checkpoint)) => {
                                    let cp = *checkpoint.checkpoint_summary.sequence_number();
                                    Ok((stream, cp))
                                }
                                _ => Err(Error::StreamingError("Peeking fails".to_string())),
                            }
                        }
                        .await
                        .map(|(stream, streamed_cp)| {
                            info!(streamed_cp, "Connected to streaming service successfully");

                            // Reset backoff state since we connected successfully.
                            streaming_backoff_until = tokio::time::Instant::now();
                            streaming_backoff_delay = config.streaming_backoff_initial_delay();
                            metrics.streaming_connection_failures.set(0);

                            // We ingest up to the streamed_cp because anything beyond that is
                            // either handled by the streaming task (if we decide to spawn a streaming task)
                            // or will be reassessed in the next loop iteration once we finish this batch.
                            let ingestion_end = streamed_cp.min(end_cp);

                            // Decide whether to start streaming now or delay it until we are within buffer size.
                            let streaming_handle = if streamed_cp <= checkpoint_hi + buffer_size {
                                info!(
                                    streamed_cp,
                                    checkpoint_hi, "Within buffer size, starting streaming"
                                );

                                tokio::spawn(stream_and_broadcast_range(
                                    streamed_cp.max(checkpoint_hi), // Need the max here to avoid sending already processed checkpoints
                                    end_cp,
                                    stream,
                                    subscribers.clone(),
                                    ingest_hi_watch_rx.clone(),
                                    metrics.clone(),
                                    cancel.clone(),
                                ))
                            } else {
                                info!(
                                    streamed_cp,
                                    checkpoint_hi, "Outside buffer size, delaying streaming start"
                                );
                                dummy_streaming_handle!(ingestion_end)
                            };
                            (streaming_handle, ingestion_end)
                        })
                        .unwrap_or_else(|_| {
                            // Streaming connection failed so we set backoff timer and double the delay
                            error!(
                                delay_millis = streaming_backoff_delay.as_millis(),
                                "Streaming connection failed, setting backoff timer"
                            );
                            metrics.streaming_connection_failures.inc();

                            streaming_backoff_until =
                                tokio::time::Instant::now() + streaming_backoff_delay;
                            streaming_backoff_delay =
                                (streaming_backoff_delay * 2).min(max_backoff_delay);

                            // We broadcast a batch of checkpoints via ingestion before retrying
                            let ingestion_end = (checkpoint_hi + ingestion_batch_size).min(end_cp);
                            (dummy_streaming_handle!(ingestion_end), ingestion_end)
                        })
                    }
                } else {
                    // No streaming service configured, so we just use ingestion for the entire range.
                    (dummy_streaming_handle!(end_cp), end_cp)
                };

            // Spawn a broadcaster task for this range.
            // It will exit when the range is complete or if it is cancelled.
            let ingestion_handle = tokio::spawn(ingest_and_broadcast_range(
                checkpoint_hi,
                ingestion_end,
                config.retry_interval(),
                config.ingest_concurrency,
                ingest_hi_watch_rx.clone(),
                client.clone(),
                subscribers.clone(),
                cancel.clone(),
            ));

            let join_future = async { tokio::join!(streaming_handle, ingestion_handle) };
            tokio::pin!(join_future);

            loop {
                tokio::select! {
                    _ = cancel.cancelled() => {
                        info!("Shutdown received, stopping ingestion");
                        break 'outer;
                    }

                    // Subscriber watermark update
                    // docs::#regulator (see docs/content/guides/developer/advanced/custom-indexer.mdx)
                    Some((name, hi)) = commit_hi_rx.recv() => {
                        subscribers_hi.insert(name, hi);

                        if let Some(min_hi) = subscribers_hi.values().copied().min() {
                            ingest_hi = Some(min_hi + buffer_size);
                            // Update the watch channel, which will notify all waiting tasks
                            let _ = ingest_hi_watch_tx.send(ingest_hi);
                        }
                    }
                    // docs::/#regulator

                    // Handle both streaming and broadcaster completion
                    (streaming_result, broadcaster_result) = join_future.as_mut() => {
                        // Check broadcaster result, cancel on any error
                        match broadcaster_result {
                            Ok(Ok(())) => {} // Success, continue
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

                        // Update checkpoint_hi from streaming, or cancel on error
                        checkpoint_hi = match streaming_result {
                            Ok(w) => w,
                            Err(e) => {
                                error!("Streaming task panicked: {}", e);
                                cancel.cancel();
                                break 'outer;
                            }
                        };

                        info!(checkpoint_hi, "Both tasks completed, moving on to next range");
                        break;
                    }
                }
            }
        }
    })
}

/// Streams and broadcasts checkpoints from a range [start, end) to subscribers.
/// This task is ingest_hi-aware and will wait if it encounters a checkpoint
/// beyond the current ingest_hi, resuming when ingest_hi advances to currently
/// streaming checkpoint.
/// If we encounter any streaming error or unexpected checkpoint ahead of the current
/// checkpoint_hi, we return the next checkpoint we want to stream.
async fn stream_and_broadcast_range(
    start: u64,
    end: u64,
    mut stream: impl Stream<Item = Result<CheckpointData, Error>> + std::marker::Unpin,
    subscribers: Arc<Vec<mpsc::Sender<Arc<CheckpointData>>>>,
    mut ingest_hi_rx: watch::Receiver<Option<u64>>,
    metrics: Arc<IndexerMetrics>,
    cancel: CancellationToken,
) -> u64 {
    let mut checkpoint_hi = start;
    while checkpoint_hi < end {
        tokio::select! {
            _ = cancel.cancelled() => {
                info!("Shutdown received, stopping streaming");
                return checkpoint_hi;
            }
            item = stream.next() => {
                match item {
                    Some(Ok(checkpoint)) => {
                        let sequence_number = *checkpoint.checkpoint_summary.sequence_number();

                        if sequence_number < checkpoint_hi {
                            // Already processed checkpoint, skip.
                            info!(checkpoint = sequence_number, checkpoint_hi, "Skipping already processed checkpoint");
                            continue;
                        }

                        if sequence_number > checkpoint_hi {
                            // Unexpected checkpoint ahead of current watermark, return to main loop
                            // to fill up the gap.
                            warn!(checkpoint = sequence_number, checkpoint_hi, "Unexpected checkpoint");
                            return checkpoint_hi;
                        }

                        assert_eq!(sequence_number, checkpoint_hi);

                        // Wait until ingest_hi allows processing this checkpoint.
                        if tokio::select! {
                            result = ingest_hi_rx.wait_for(|hi| hi.is_none_or(|h| checkpoint_hi < h)) => result.is_err(),
                            _ = cancel.cancelled() => true,
                        } {
                            return checkpoint_hi;
                        }

                        // Send checkpoint to all subscribers and return on any error.
                        if send_checkpoint(Arc::new(checkpoint), &subscribers).await.is_err() {
                            return checkpoint_hi;
                        }

                        info!(checkpoint = checkpoint_hi, "Streamed checkpoint to subscribers");
                        checkpoint_hi = checkpoint_hi.saturating_add(1);
                        metrics.total_streamed_checkpoints.inc();
                    }
                    Some(Err(e)) => {
                        warn!("Streaming error: {}", e);
                        return checkpoint_hi;
                    }
                    None => {
                        warn!("Streaming ended unexpectedly");
                        return checkpoint_hi;
                    }
                }
            }
        }
    }

    checkpoint_hi
}

/// Ingests and broadcasts checkpoints from a range [start, end) to subscribers.
/// This task is ingest_hi-aware and will wait if it encounters a checkpoint
/// beyond the current ingest_hi, resuming when ingest_hi advances to currently
/// ingesting checkpoints.
async fn ingest_and_broadcast_range(
    start: u64,
    end: u64,
    retry_interval: Duration,
    ingest_concurrency: usize,
    ingest_hi_rx: watch::Receiver<Option<u64>>,
    client: IngestionClient,
    subscribers: Arc<Vec<mpsc::Sender<Arc<CheckpointData>>>>,
    cancel: CancellationToken,
) -> Result<(), Error> {
    info!(start, end, "Starting broadcaster for checkpoint range");

    stream::iter(start..end)
        .try_for_each_spawned(ingest_concurrency, |cp| {
            let mut ingest_hi_rx = ingest_hi_rx.clone();
            let client = client.clone();
            let subscribers = subscribers.clone();

            let cancel = cancel.clone();

            async move {
                // docs::#bound (see docs/content/guides/developer/advanced/custom-indexer.mdx)
                // Wait until ingest_hi allows processing this checkpoint.
                // None means no backpressure limit. If we get Some(hi) we wait until cp < hi.
                // wait_for only errors if the sender is dropped (main broadcaster shut down) so
                // we treat an error returned here as cancellation too.
                if tokio::select! {
                    result = ingest_hi_rx.wait_for(|hi| hi.is_none_or(|h| cp < h)) => result.is_err(),
                    _ = cancel.cancelled() => true,
                } {
                    tracing::error!(checkpoint = cp, "Ingestion cancelled while waiting for ingest_hi");
                    return Err(Error::Cancelled);
                }
                // docs::/#bound

                // Fetch the checkpoint or stop if cancelled.
                let checkpoint = tokio::select! {
                    cp = client.wait_for(cp, retry_interval) => cp?,
                    _ = cancel.cancelled() => {
                        tracing::error!(checkpoint = cp, "Ingestion cancelled while fetching checkpoint");
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
                            tracing::error!(checkpoint = cp, "Failed to broadcast checkpoint, subscriber channel closed");
                            // An error is returned meaning some subscriber channel has closed, which we consider
                            // a cancellation signal for the entire ingestion.
                            // cancel.cancel();
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
            ingestion_batch_size: 10,
            // Setting the delay to 0 so some tests testing failure recovery can run deterministically
            streaming_backoff_initial_delay_ms: 0,
            streaming_backoff_max_delay_ms: 60000,
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
        telemetry_subscribers::init_for_testing();
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

        println!("Updating watermark to 4 yayaya");

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

        // Should fallback to ingestion for ingestion_batch_size (10) checkpoints
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

        // Streaming service where peek fails on first attempt
        let streaming_service = MockStreamingService::new(0..20).fail_peek_times(1);

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
    async fn streaming_before_start_checkpoint() {
        telemetry_subscribers::init_for_testing();
        let (_, hi_rx) = mpsc::unbounded_channel();
        let (subscriber_tx, mut subscriber_rx) = mpsc::channel(30);
        let cancel = CancellationToken::new();

        // Streaming service starts at checkpoint 0 but indexing starts at 30.
        let streaming_service = MockStreamingService::new(0..100);

        let metrics = test_metrics();
        let h_broadcaster = broadcaster(
            30..100,
            None,
            Some(streaming_service),
            test_config(),
            mock_client(metrics.clone()),
            hi_rx,
            vec![subscriber_tx],
            metrics.clone(),
            cancel.clone(),
        );

        expect_checkpoints_in_order(&mut subscriber_rx, 30..100).await;

        // Verify only streaming was used (all from streaming)
        assert_eq!(metrics.total_streamed_checkpoints.get(), 70);
        assert_eq!(metrics.total_ingested_checkpoints.get(), 0);

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
    async fn streaming_connection_retry_with_backoff() {
        telemetry_subscribers::init_for_testing();
        let (_, hi_rx) = mpsc::unbounded_channel();
        let (subscriber_tx, mut subscriber_rx) = mpsc::channel(50);
        let cancel = CancellationToken::new();

        // Streaming service where peek always fails (never recovers)
        let streaming_service = MockStreamingService::new(30..50).fail_peek_times(usize::MAX);

        let metrics = test_metrics();

        // Custom config with a small backoff delay
        let mut config = test_config();
        config.streaming_backoff_initial_delay_ms = 5; // Short delay for test speed

        let h_broadcaster = broadcaster(
            0..,
            None,
            Some(streaming_service),
            config,
            mock_client(metrics.clone()),
            hi_rx,
            vec![subscriber_tx],
            metrics.clone(),
            cancel.clone(),
        );

        tokio::time::sleep(Duration::from_millis(200)).await;

        // Should fallback to ingestion for all checkpoints
        expect_checkpoints_in_range(&mut subscriber_rx, 0..30).await;

        // Verify retry counter incremented at least twice.
        assert!(metrics.streaming_connection_failures.get() >= 2);

        // Verify only ingestion was used (streaming never succeeded)
        assert!(metrics.total_ingested_checkpoints.get() > 0);
        assert_eq!(metrics.total_streamed_checkpoints.get(), 0);

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
        expect_checkpoints_in_order(&mut subscriber_rx, 0..10).await;

        // Should halt due to backpressure
        expect_timeout(&mut subscriber_rx).await;

        // Update watermark to make progress
        hi_tx.send(("test", 20)).unwrap();

        // Should receive remaining checkpoints
        expect_checkpoints_in_order(&mut subscriber_rx, 10..20).await;

        assert_eq!(metrics.total_streamed_checkpoints.get(), 20);

        cancel.cancel();
        h_broadcaster.await.unwrap();
    }
}
