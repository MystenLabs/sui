// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{collections::HashMap, sync::Arc, time::Duration};

use futures::{Stream, future::try_join_all, stream};
use tokio::{
    sync::{mpsc, watch},
    task::JoinHandle,
};
use tokio_stream::StreamExt;
use tokio_util::sync::CancellationToken;
use tracing::{debug, error, info, warn};

use super::{IngestionConfig, ingestion_client::IngestionClient};
use crate::{
    ingestion::{error::Error, streaming_client::CheckpointStreamingClient},
    metrics::IngestionMetrics,
    task::TrySpawnStreamExt,
    types::full_checkpoint_content::Checkpoint,
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
    mut streaming_client: Option<S>,
    config: IngestionConfig,
    client: IngestionClient,
    mut commit_hi_rx: mpsc::UnboundedReceiver<(&'static str, u64)>,
    subscribers: Vec<mpsc::Sender<Arc<Checkpoint>>>,
    metrics: Arc<IngestionMetrics>,
    cancel: CancellationToken,
) -> JoinHandle<()>
where
    R: std::ops::RangeBounds<u64> + Send + 'static,
    S: CheckpointStreamingClient + Send + 'static,
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
            // If u64::MAX is provided as an inclusive bound, the saturating_add
            // here will prevent overflow but the broadcaster will actually
            // only ingest up to u64::MAX - 1, since the range is [start..end).
            // This isn't an issue in practice since we won't see that many checkpoints
            // in our lifetime anyway.
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
        let initial_ingest_hi = initial_commit_hi.map(|min_hi| min_hi + buffer_size);
        let (ingest_hi_watch_tx, ingest_hi_watch_rx) = watch::channel(initial_ingest_hi);

        // If the first attempt at streaming connection fails, we back off for an initial number
        // of checkpoints to process using ingestion. This value doubles on each subsequent failure.
        let mut streaming_backoff_batch_size = config.streaming_backoff_initial_batch_size as u64;

        // Initialize the overall checkpoint_hi watermark to start_cp.
        // This value is updated every outer loop iteration after both streaming and broadcasting complete.
        let mut checkpoint_hi = start_cp;

        'outer: while checkpoint_hi < end_cp {
            // Set up the streaming task for the current range [checkpoint_hi, end_cp). This function
            // will return a handle to the streaming task and the end cp of the ingestion task, calculated
            // based on 1) if streaming is used, 2) streaming connection success status, and 3) the network
            // latest checkpoint we get from a success streaming connection.
            // The ingestion task fill up the gap from checkpoint_hi to ingestion_end (exclusive) while the streaming
            // task covers from ingestion_end to end_cp.
            let (streaming_handle, ingestion_end) = setup_streaming_task(
                &mut streaming_client,
                checkpoint_hi,
                end_cp,
                &mut streaming_backoff_batch_size,
                &config,
                &subscribers,
                &ingest_hi_watch_rx,
                &metrics,
                &cancel,
            )
            .await;

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

            let mut join_future = futures::future::join(streaming_handle, ingestion_handle);

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
                            let new_ingest_hi = Some(min_hi + buffer_size);
                            // Update the watch channel, which will notify all waiting tasks
                            let _ = ingest_hi_watch_tx.send(new_ingest_hi);
                        }
                    }
                    // docs::/#regulator

                    // Handle both streaming and ingestion completion
                    (streaming_result, ingestion_result) = &mut join_future => {
                        // Check ingestion result, cancel on any error
                        match ingestion_result {
                            Ok(Ok(())) => {} // Success, continue
                            Ok(Err(e)) => {
                                error!("Ingestion task failed: {}", e);
                                cancel.cancel();
                                break 'outer;
                            }
                            Err(e) => {
                                error!("Ingestion task panicked: {}", e);
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

        info!("Broadcaster finished");
    })
}

/// Fetch and broadcasts checkpoints from a range [start..end) to subscribers.
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
    subscribers: Arc<Vec<mpsc::Sender<Arc<Checkpoint>>>>,
    cancel: CancellationToken,
) -> Result<(), Error> {
    stream::iter(start..end)
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
                // None means no backpressure limit. If we get Some(hi) we wait until cp < hi.
                // wait_for only errors if the sender is dropped (main broadcaster shut down) so
                // we treat an error returned here as cancellation too.
                if tokio::select! {
                    result = ingest_hi_rx.wait_for(|hi| hi.is_none_or(|h| cp < h)) => result.is_err(),
                    _ = cancel.cancelled() => true
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
                            debug!(checkpoint = cp, "Broadcasted checkpoint");
                            Ok(())
                        } else {
                            // An error is returned meaning some subscriber channel has closed,
                            // which we consider a cancellation signal for the entire ingestion.
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

/// Sets up either a noop or real streaming task based on network state and proximity to
/// the current checkpoint_hi, and returns a streaming task handle and the `ingestion_end`
/// telling the main task that ingestion should be used up to this point.
async fn setup_streaming_task<S>(
    streaming_client: &mut Option<S>,
    checkpoint_hi: u64,
    end_cp: u64,
    streaming_backoff_batch_size: &mut u64,
    config: &IngestionConfig,
    subscribers: &Arc<Vec<mpsc::Sender<Arc<Checkpoint>>>>,
    ingest_hi_watch_rx: &watch::Receiver<Option<u64>>,
    metrics: &Arc<IngestionMetrics>,
    cancel: &CancellationToken,
) -> (JoinHandle<u64>, u64)
where
    S: CheckpointStreamingClient,
{
    // No streaming client configured so we ingest all the way to end_cp.
    let Some(streaming_client) = streaming_client else {
        return (noop_streaming_task(end_cp), end_cp);
    };

    let backoff_batch_size = *streaming_backoff_batch_size;

    // Convenient closure to handle streaming fallback logic due to connection or peek failure.
    let mut handle_streaming_fallback = |reason: &str| {
        let ingestion_end = (checkpoint_hi + backoff_batch_size).min(end_cp);
        warn!(
            checkpoint_hi,
            ingestion_end, "{reason}, falling back to ingestion"
        );
        metrics.total_streaming_connection_failures.inc();
        *streaming_backoff_batch_size =
            (backoff_batch_size * 2).min(config.streaming_backoff_max_batch_size as u64);
        (noop_streaming_task(ingestion_end), ingestion_end)
    };

    let stream = match streaming_client.connect().await {
        Ok(stream) => stream,
        Err(e) => {
            return handle_streaming_fallback(&format!("Streaming connection failed: {}", e));
        }
    };

    // Wrap the stream with a statement timeout to prevent hanging indefinitely.
    let timeout_stream = stream
        .timeout(config.streaming_statement_timeout())
        .map(|result| match result {
            Ok(inner) => inner,
            Err(_) => Err(Error::StreamingError(anyhow::anyhow!("Stream timeout"))),
        });

    let mut peekable_stream = Box::pin(timeout_stream).peekable();

    let peeked_checkpoint = match peekable_stream.peek().await {
        Some(Ok(checkpoint)) => checkpoint,
        Some(Err(e)) => {
            return handle_streaming_fallback(&format!("Failed to peek latest checkpoint: {}", e));
        }
        None => return handle_streaming_fallback("Stream ended during peek"),
    };

    // We have successfully connected and peeked, reset backoff batch size.
    *streaming_backoff_batch_size = config.streaming_backoff_initial_batch_size as u64;

    let network_latest_cp = *peeked_checkpoint.summary.sequence_number();

    let ingestion_end = network_latest_cp.min(end_cp);

    if network_latest_cp > checkpoint_hi + config.checkpoint_buffer_size as u64 {
        info!(
            network_latest_cp,
            checkpoint_hi, "Outside buffer size, delaying streaming start"
        );
        return (noop_streaming_task(ingestion_end), ingestion_end);
    }

    info!(
        network_latest_cp,
        checkpoint_hi, "Within buffer size, starting streaming"
    );

    let streaming_handle = tokio::spawn(stream_and_broadcast_range(
        network_latest_cp.max(checkpoint_hi),
        end_cp,
        peekable_stream,
        subscribers.clone(),
        ingest_hi_watch_rx.clone(),
        metrics.clone(),
        cancel.clone(),
    ));

    (streaming_handle, ingestion_end)
}

/// Streams and broadcasts checkpoints from a range [start, end) to subscribers.
/// This task is ingest_hi-aware, for each checkpoint this task will wait until
/// `checkpoint_hi < ingest_hi` before advancing to the next checkpoint.
/// If we encounter any streaming error or out-of-order checkpoint greater than
/// the current checkpoint_hi, we stop streaming and return checkpoint_hi so that
/// the main loop can reconnect and fill in the gap using ingestion.
async fn stream_and_broadcast_range(
    mut lo: u64,
    hi: u64,
    mut stream: impl Stream<Item = Result<Checkpoint, Error>> + std::marker::Unpin,
    subscribers: Arc<Vec<mpsc::Sender<Arc<Checkpoint>>>>,
    mut ingest_hi_rx: watch::Receiver<Option<u64>>,
    metrics: Arc<IngestionMetrics>,
    cancel: CancellationToken,
) -> u64 {
    while lo < hi {
        tokio::select! {
            _ = cancel.cancelled() => {
                info!(lo, "Shutdown received, stopping streaming");
                break;
            }
            item = stream.next() => {
                match item {
                    Some(Ok(checkpoint)) => {
                        let sequence_number = *checkpoint.summary.sequence_number();

                        if sequence_number < lo {
                            debug!(checkpoint = sequence_number, lo, "Skipping already processed checkpoint");
                            continue;
                        }

                        if sequence_number > lo {
                            warn!(checkpoint = sequence_number, lo, "Out-of-order checkpoint");
                            // Return to main loop to fill up the gap.
                            break;
                        }

                        assert_eq!(sequence_number, lo);

                        // Wait until ingest_hi allows processing this checkpoint.
                        if tokio::select! {
                            result = ingest_hi_rx.wait_for(|hi| hi.is_none_or(|h| lo < h)) => result.is_err(),
                            _ = cancel.cancelled() => true,
                        } {
                            break;
                        }

                        // Send checkpoint to all subscribers and break on any error.
                        if send_checkpoint(Arc::new(checkpoint), &subscribers).await.is_err() {
                            break;
                        }

                        info!(checkpoint = lo, "Streamed checkpoint");
                        metrics.total_streamed_checkpoints.inc();
                        metrics.latest_streamed_checkpoint.set(lo as i64);
                        lo += 1;
                    }
                    Some(Err(e)) => {
                        warn!(lo, "Streaming error: {}", e);
                        break;
                    }
                    None => {
                        warn!(lo, "Streaming ended unexpectedly");
                        break;
                    }
                }
            }
        }
    }

    // We exit the loop either due to cancellation, error or completion of the range,
    // in all cases we disconnect the stream and return the current watermark.
    metrics.total_stream_disconnections.inc();
    lo
}

/// Send a checkpoint to all subscribers.
/// Returns an error if any subscriber's channel is closed.
async fn send_checkpoint(
    checkpoint: Arc<Checkpoint>,
    subscribers: &[mpsc::Sender<Arc<Checkpoint>>],
) -> Result<Vec<()>, mpsc::error::SendError<Arc<Checkpoint>>> {
    let futures = subscribers.iter().map(|s| s.send(checkpoint.clone()));
    try_join_all(futures).await
}

// A noop streaming task that just returns the provided checkpoint_hi, used to simplify
// join logic when streaming is not used.
fn noop_streaming_task(checkpoint_hi: u64) -> JoinHandle<u64> {
    tokio::spawn(async move { checkpoint_hi })
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;
    use std::fmt::Debug;
    use std::sync::Arc;
    use std::time::Duration;
    use tokio::time::{error::Elapsed, timeout};

    use super::*;
    use crate::ingestion::ingestion_client::FetchData;
    use crate::ingestion::streaming_client::test_utils::MockStreamingClient;
    use crate::ingestion::{IngestionConfig, test_utils::test_checkpoint_data};
    use crate::metrics::tests::test_ingestion_metrics;

    /// Create a mock IngestionClient for tests
    fn mock_client(metrics: Arc<IngestionMetrics>) -> IngestionClient {
        use crate::ingestion::ingestion_client::{FetchError, IngestionClientTrait};
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
            streaming_backoff_initial_batch_size: 2,
            streaming_backoff_max_batch_size: 16,
            streaming_connection_timeout_ms: 100,
            streaming_statement_timeout_ms: 100,
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

    /// Receive `count` checkpoints from the channel and return their sequence numbers as a Vec.
    /// Maintains order, useful for verifying sequential delivery (e.g., from streaming).
    async fn recv_vec(rx: &mut mpsc::Receiver<Arc<Checkpoint>>, count: usize) -> Vec<u64> {
        let mut result = Vec::with_capacity(count);
        for _ in 0..count {
            let checkpoint = expect_recv(rx).await.unwrap();
            result.push(*checkpoint.summary.sequence_number());
        }
        result
    }

    /// Receive `count` checkpoints from the channel and return their sequence numbers as a BTreeSet.
    /// Useful for verifying unordered delivery (e.g., from concurrent ingestion).
    async fn recv_set(rx: &mut mpsc::Receiver<Arc<Checkpoint>>, count: usize) -> BTreeSet<u64> {
        let mut result = BTreeSet::new();
        for _ in 0..count {
            let checkpoint = expect_recv(rx).await.unwrap();
            let inserted = result.insert(*checkpoint.summary.sequence_number());
            assert!(
                inserted,
                "Received duplicate checkpoint {}",
                checkpoint.summary.sequence_number()
            );
        }
        result
    }

    #[tokio::test]
    async fn finite_list_of_checkpoints() {
        let (_, hi_rx) = mpsc::unbounded_channel();
        let (subscriber_tx, mut subscriber_rx) = mpsc::channel(1);
        let cancel = CancellationToken::new();

        let cps = 0..5;
        let metrics = test_ingestion_metrics();
        let h_broadcaster = broadcaster::<_, MockStreamingClient>(
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

        assert_eq!(
            recv_set(&mut subscriber_rx, 5).await,
            BTreeSet::from_iter(0..5)
        );

        cancel.cancel();
        h_broadcaster.await.unwrap();
    }

    #[tokio::test]
    async fn shutdown_on_sender_closed() {
        let (_, hi_rx) = mpsc::unbounded_channel();
        let (subscriber_tx, mut subscriber_rx) = mpsc::channel(1);
        let cancel = CancellationToken::new();

        let metrics = test_ingestion_metrics();
        let h_broadcaster = broadcaster::<_, MockStreamingClient>(
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

        assert_eq!(
            recv_set(&mut subscriber_rx, 5).await,
            BTreeSet::from_iter(0..5)
        );

        drop(subscriber_rx);
        h_broadcaster.await.unwrap();
    }

    #[tokio::test]
    async fn shutdown_on_cancel() {
        let (_, hi_rx) = mpsc::unbounded_channel();
        let (subscriber_tx, mut subscriber_rx) = mpsc::channel(1);
        let cancel = CancellationToken::new();

        let metrics = test_ingestion_metrics();
        let h_broadcaster = broadcaster::<_, MockStreamingClient>(
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

        assert_eq!(
            recv_set(&mut subscriber_rx, 5).await,
            BTreeSet::from_iter(0..5)
        );

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

        let metrics = test_ingestion_metrics();
        let h_broadcaster = broadcaster::<_, MockStreamingClient>(
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

        assert_eq!(
            recv_set(&mut subscriber_rx, 4).await,
            BTreeSet::from_iter(0..4)
        );

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

        let metrics = test_ingestion_metrics();
        let h_broadcaster = broadcaster::<_, MockStreamingClient>(
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

        assert_eq!(
            recv_set(&mut subscriber_rx, 4).await,
            BTreeSet::from_iter(0..4)
        );

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

        let metrics = test_ingestion_metrics();
        let h_broadcaster = broadcaster::<_, MockStreamingClient>(
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

        assert_eq!(
            recv_set(&mut subscriber_rx, 2).await,
            BTreeSet::from_iter(0..2)
        );

        // Regulator stopped because of watermark, but resumes when that watermark is updated.
        expect_timeout(&mut subscriber_rx).await;
        hi_tx.send(("test", 4)).unwrap();

        assert_eq!(
            recv_set(&mut subscriber_rx, 2).await,
            BTreeSet::from_iter(2..4)
        );

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
        let metrics = test_ingestion_metrics();
        let h_broadcaster = broadcaster::<_, MockStreamingClient>(
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

        assert_eq!(
            recv_set(&mut subscriber_rx, 2).await,
            BTreeSet::from_iter(0..2)
        );

        // Watermark stopped because of a's watermark.
        expect_timeout(&mut subscriber_rx).await;

        // Updating b's watermark doesn't make a difference.
        hi_tx.send(("b", 4)).unwrap();
        expect_timeout(&mut subscriber_rx).await;

        // But updating a's watermark does.
        hi_tx.send(("a", 3)).unwrap();
        let checkpoint = expect_recv(&mut subscriber_rx).await.unwrap();
        assert_eq!(*checkpoint.summary.sequence_number(), 2);

        // ...by one checkpoint.
        expect_timeout(&mut subscriber_rx).await;

        // And we can make more progress by updating it again.
        hi_tx.send(("a", 4)).unwrap();
        let checkpoint = expect_recv(&mut subscriber_rx).await.unwrap();
        assert_eq!(*checkpoint.summary.sequence_number(), 3);

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

        let metrics = test_ingestion_metrics();
        let h_broadcaster = broadcaster::<_, MockStreamingClient>(
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
        assert_eq!(
            recv_set(&mut subscriber_rx1, 3).await,
            BTreeSet::from_iter(0..3)
        );
        assert_eq!(
            recv_set(&mut subscriber_rx2, 3).await,
            BTreeSet::from_iter(0..3)
        );

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

        let metrics = test_ingestion_metrics();
        let h_broadcaster = broadcaster::<_, MockStreamingClient>(
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
        assert_eq!(
            recv_set(&mut subscriber_rx, 5).await,
            BTreeSet::from_iter(1000..1005)
        );

        // Should halt at watermark
        expect_timeout(&mut subscriber_rx).await;

        // Update watermark to allow completion
        hi_tx.send(("test", 1010)).unwrap();

        assert_eq!(
            recv_set(&mut subscriber_rx, 5).await,
            BTreeSet::from_iter(1005..1010)
        );

        cancel.cancel();
        h_broadcaster.await.unwrap();
    }

    // =============== Streaming Tests ==================

    // =============== Part 1: Basic Streaming ==================

    #[tokio::test]
    async fn streaming_only() {
        let (_, hi_rx) = mpsc::unbounded_channel();
        let (subscriber_tx, mut subscriber_rx) = mpsc::channel(10);
        let cancel = CancellationToken::new();

        // Create a mock streaming service with checkpoints 0..5
        let streaming_client = MockStreamingClient::new(0..5, None);

        let metrics = test_ingestion_metrics();
        let h_broadcaster = broadcaster(
            0..5, // Bounded range
            None,
            Some(streaming_client),
            test_config(),
            mock_client(metrics.clone()),
            hi_rx,
            vec![subscriber_tx],
            metrics.clone(),
            cancel.clone(),
        );

        // Should receive all checkpoints from the stream in order
        assert_eq!(recv_vec(&mut subscriber_rx, 5).await, Vec::from_iter(0..5));

        // We should get all checkpoints from streaming.
        assert_eq!(metrics.total_streamed_checkpoints.get(), 5);
        assert_eq!(metrics.total_ingested_checkpoints.get(), 0);
        assert_eq!(metrics.latest_streamed_checkpoint.get(), 4);

        cancel.cancel();
        h_broadcaster.await.unwrap();
    }

    #[tokio::test]
    async fn streaming_with_transition() {
        let (_, hi_rx) = mpsc::unbounded_channel();
        let (subscriber_tx, mut subscriber_rx) = mpsc::channel(100);
        let cancel = CancellationToken::new();

        // Create a mock streaming service that starts at checkpoint 50
        // This simulates streaming being ahead of ingestion
        let streaming_client = MockStreamingClient::new(49..60, None);

        let metrics = test_ingestion_metrics();
        let h_broadcaster = broadcaster(
            0..60,
            None,
            Some(streaming_client),
            test_config(),
            mock_client(metrics.clone()),
            hi_rx,
            vec![subscriber_tx],
            metrics.clone(),
            cancel.clone(),
        );

        assert_eq!(
            recv_set(&mut subscriber_rx, 60).await,
            BTreeSet::from_iter(0..60)
        );

        // Verify both ingestion and streaming were used
        assert_eq!(metrics.total_ingested_checkpoints.get(), 50); // [0..50)
        assert_eq!(metrics.total_streamed_checkpoints.get(), 10); // [50..60)
        assert_eq!(metrics.latest_streamed_checkpoint.get(), 59);

        cancel.cancel();
        h_broadcaster.await.unwrap();
    }

    // =============== Part 2: Edge Cases ==================

    #[tokio::test]
    async fn streaming_beyond_end_checkpoint() {
        // Test scenario where streaming service starts beyond the requested end checkpoint.
        let (_, hi_rx) = mpsc::unbounded_channel();
        let (subscriber_tx, mut subscriber_rx) = mpsc::channel(30);
        let cancel = CancellationToken::new();

        // Streaming starts at checkpoint 100, but we only want 0..30.
        let streaming_client = MockStreamingClient::new(100..110, None);

        let metrics = test_ingestion_metrics();
        let h_broadcaster = broadcaster(
            0..30,
            None,
            Some(streaming_client),
            test_config(),
            mock_client(metrics.clone()),
            hi_rx,
            vec![subscriber_tx],
            metrics.clone(),
            cancel.clone(),
        );

        // Should use only ingestion since streaming is beyond end_cp
        assert_eq!(
            recv_set(&mut subscriber_rx, 30).await,
            BTreeSet::from_iter(0..30)
        );

        // Verify no streaming was used (all from ingestion)
        assert_eq!(metrics.total_streamed_checkpoints.get(), 0);
        assert_eq!(metrics.total_ingested_checkpoints.get(), 30);

        cancel.cancel();
        h_broadcaster.await.unwrap();
    }

    #[tokio::test]
    async fn streaming_before_start_checkpoint() {
        // Test scenario where streaming starts before the requested start checkpoint.
        let (_, hi_rx) = mpsc::unbounded_channel();
        let (subscriber_tx, mut subscriber_rx) = mpsc::channel(30);
        let cancel = CancellationToken::new();

        // Streaming starts at checkpoint 0 but indexing starts at 30.
        let streaming_client = MockStreamingClient::new(0..100, None);

        let metrics = test_ingestion_metrics();
        let h_broadcaster = broadcaster(
            30..100,
            None,
            Some(streaming_client),
            test_config(),
            mock_client(metrics.clone()),
            hi_rx,
            vec![subscriber_tx],
            metrics.clone(),
            cancel.clone(),
        );

        assert_eq!(
            recv_vec(&mut subscriber_rx, 70).await,
            Vec::from_iter(30..100)
        );

        // Verify only streaming was used (all from streaming)
        assert_eq!(metrics.total_streamed_checkpoints.get(), 70);
        assert_eq!(metrics.total_ingested_checkpoints.get(), 0);
        assert_eq!(metrics.latest_streamed_checkpoint.get(), 99);

        cancel.cancel();
        h_broadcaster.await.unwrap();
    }

    #[tokio::test]
    async fn streaming_behind_watermark_skips_duplicates() {
        // Test scenario where streaming service provides checkpoints behind the current watermark,
        // which should be skipped.
        let (_, hi_rx) = mpsc::unbounded_channel();
        let (subscriber_tx, mut subscriber_rx) = mpsc::channel(50);
        let cancel = CancellationToken::new();

        // Create streaming client that returns some checkpoints behind the watermark
        let mut streaming_client = MockStreamingClient::new(0..15, None);
        // Insert duplicate/old checkpoints that should be skipped
        streaming_client.insert_checkpoint(3); // Behind watermark
        streaming_client.insert_checkpoint(4); // Behind watermark
        streaming_client.insert_checkpoint_range(15..20);

        let metrics = test_ingestion_metrics();
        let h_broadcaster = broadcaster(
            0..20,
            None,
            Some(streaming_client),
            test_config(),
            mock_client(metrics.clone()),
            hi_rx,
            vec![subscriber_tx],
            metrics.clone(),
            cancel.clone(),
        );

        // Should receive all checkpoints exactly once (no duplicates) from streaming.
        assert_eq!(
            recv_vec(&mut subscriber_rx, 20).await,
            Vec::from_iter(0..20)
        );

        assert_eq!(metrics.total_streamed_checkpoints.get(), 20);
        assert_eq!(metrics.total_ingested_checkpoints.get(), 0);
        assert_eq!(metrics.latest_streamed_checkpoint.get(), 19);

        cancel.cancel();
        h_broadcaster.await.unwrap();
    }

    #[tokio::test]
    async fn streaming_ahead_of_watermark_recovery() {
        // Test scenario where streaming service has a gap ahead of the watermark,
        // requiring fallback to ingestion to fill the gap.
        let (_, hi_rx) = mpsc::unbounded_channel();
        let (subscriber_tx, mut subscriber_rx) = mpsc::channel(50);
        let cancel = CancellationToken::new();

        // Create streaming client that has a gap (checkpoint ahead of expected watermark)
        let mut streaming_client = MockStreamingClient::new(0..3, None);
        streaming_client.insert_checkpoint_range(6..10); // Gap: skips checkpoints 3 - 5

        let metrics = test_ingestion_metrics();
        let h_broadcaster = broadcaster(
            0..10,
            None,
            Some(streaming_client),
            test_config(),
            mock_client(metrics.clone()),
            hi_rx,
            vec![subscriber_tx],
            metrics.clone(),
            cancel.clone(),
        );

        // Should receive first three checkpoints from streaming in order
        assert_eq!(recv_vec(&mut subscriber_rx, 3).await, Vec::from_iter(0..3));

        // Then should fallback to ingestion for 3-6, and streaming continues for 7-9.
        // Streaming continues from 7 because 6 was consumed already during the last streaming loop.
        assert_eq!(
            recv_set(&mut subscriber_rx, 7).await,
            BTreeSet::from_iter(3..10)
        );

        assert_eq!(metrics.total_streamed_checkpoints.get(), 6);
        assert_eq!(metrics.total_ingested_checkpoints.get(), 4);
        assert_eq!(metrics.latest_streamed_checkpoint.get(), 9);

        cancel.cancel();
        h_broadcaster.await.unwrap();
    }

    #[tokio::test]
    async fn streaming_with_backpressure() {
        // Test scenario where streaming is regulated by watermark backpressure.

        let (hi_tx, hi_rx) = mpsc::unbounded_channel();
        let (subscriber_tx, mut subscriber_rx) = mpsc::channel(30);
        let cancel = CancellationToken::new();

        let streaming_client = MockStreamingClient::new(0..20, None);

        let config = IngestionConfig {
            checkpoint_buffer_size: 5,
            ..test_config()
        };

        let metrics = test_ingestion_metrics();
        let h_broadcaster = broadcaster(
            0..20,
            Some(5), // initial watermark to trigger backpressure
            Some(streaming_client),
            config,
            mock_client(metrics.clone()),
            hi_rx,
            vec![subscriber_tx],
            metrics.clone(),
            cancel.clone(),
        );

        // Should receive first 10 checkpoints (0..10) from streaming
        assert_eq!(
            recv_vec(&mut subscriber_rx, 10).await,
            Vec::from_iter(0..10)
        );
        assert_eq!(metrics.latest_streamed_checkpoint.get(), 9);

        // Should halt due to backpressure
        expect_timeout(&mut subscriber_rx).await;

        // Update watermark to make progress
        hi_tx.send(("test", 15)).unwrap();

        // Should receive remaining checkpoints
        assert_eq!(
            recv_vec(&mut subscriber_rx, 10).await,
            Vec::from_iter(10..20)
        );
        assert_eq!(metrics.latest_streamed_checkpoint.get(), 19);

        assert_eq!(metrics.total_streamed_checkpoints.get(), 20);

        cancel.cancel();
        h_broadcaster.await.unwrap();
    }

    // =============== Part 3: Streaming Errors ==================

    #[tokio::test]
    async fn streaming_error_during_streaming() {
        let (_, hi_rx) = mpsc::unbounded_channel();
        let (subscriber_tx, mut subscriber_rx) = mpsc::channel(20);
        let cancel = CancellationToken::new();

        // Create streaming client with error injected mid-stream
        let mut streaming_client = MockStreamingClient::new(0..5, None);
        streaming_client.insert_error(); // Error after 5 checkpoints
        streaming_client.insert_checkpoint_range(10..15);

        let metrics = test_ingestion_metrics();
        let h_broadcaster = broadcaster(
            0..15,
            None,
            Some(streaming_client),
            test_config(),
            mock_client(metrics.clone()),
            hi_rx,
            vec![subscriber_tx],
            metrics.clone(),
            cancel.clone(),
        );

        // Should receive first 5 checkpoints from streaming in order
        assert_eq!(recv_vec(&mut subscriber_rx, 5).await, Vec::from_iter(0..5));

        // After error, should fallback and complete via ingestion/retry (order not guaranteed)
        assert_eq!(
            recv_set(&mut subscriber_rx, 10).await,
            BTreeSet::from_iter(5..15)
        );

        // Verify streaming was used initially
        assert_eq!(metrics.total_streamed_checkpoints.get(), 10);
        // Then ingestion was used to recover the missing checkpoints.
        assert_eq!(metrics.total_ingested_checkpoints.get(), 5);
        // The last checkpoint should come from streaming after recovery.
        assert_eq!(metrics.latest_streamed_checkpoint.get(), 14);

        cancel.cancel();
        h_broadcaster.await.unwrap();
    }

    #[tokio::test]
    async fn streaming_multiple_errors_with_recovery() {
        let (_, hi_rx) = mpsc::unbounded_channel();
        let (subscriber_tx, mut subscriber_rx) = mpsc::channel(50);
        let cancel = CancellationToken::new();

        // Create streaming client with multiple errors injected
        let mut streaming_client = MockStreamingClient::new(0..5, None);
        streaming_client.insert_error(); // Error at checkpoint 5
        streaming_client.insert_checkpoint_range(5..10);
        streaming_client.insert_error(); // Error at checkpoint 10
        streaming_client.insert_checkpoint_range(10..20);

        let metrics = test_ingestion_metrics();
        let h_broadcaster = broadcaster(
            0..20,
            None,
            Some(streaming_client),
            test_config(),
            mock_client(metrics.clone()),
            hi_rx,
            vec![subscriber_tx],
            metrics.clone(),
            cancel.clone(),
        );

        // Should eventually receive all checkpoints despite errors from streaming.
        assert_eq!(
            recv_vec(&mut subscriber_rx, 20).await,
            Vec::from_iter(0..20)
        );

        assert_eq!(metrics.total_streamed_checkpoints.get(), 20);
        assert_eq!(metrics.latest_streamed_checkpoint.get(), 19);
        assert_eq!(metrics.total_ingested_checkpoints.get(), 0);
        assert_eq!(metrics.total_stream_disconnections.get(), 3); // 2 errors + 1 completion

        cancel.cancel();
        h_broadcaster.await.unwrap();
    }

    #[tokio::test]
    async fn streaming_start_failure_fallback_to_ingestion() {
        let (_, hi_rx) = mpsc::unbounded_channel();
        let (subscriber_tx, mut subscriber_rx) = mpsc::channel(20);
        let cancel = CancellationToken::new();

        // Streaming service that fails to start
        let streaming_service = MockStreamingClient::new(0..20, None).fail_connection_times(1);

        let metrics = test_ingestion_metrics();
        let config = IngestionConfig {
            streaming_backoff_initial_batch_size: 5,
            ..test_config()
        };
        let h_broadcaster = broadcaster(
            0..20,
            None,
            Some(streaming_service),
            config,
            mock_client(metrics.clone()),
            hi_rx,
            vec![subscriber_tx],
            metrics.clone(),
            cancel.clone(),
        );

        // Should fallback to ingestion for initial batch size checkpoints
        assert_eq!(
            recv_set(&mut subscriber_rx, 5).await,
            BTreeSet::from_iter(0..5)
        );

        // After the interval, it should complete the remaining checkpoints from streaming
        assert_eq!(
            recv_vec(&mut subscriber_rx, 15).await,
            Vec::from_iter(5..20)
        );

        assert_eq!(metrics.total_ingested_checkpoints.get(), 5);
        assert_eq!(metrics.total_streamed_checkpoints.get(), 15);

        cancel.cancel();
        h_broadcaster.await.unwrap();
    }

    #[tokio::test]
    async fn streaming_peek_failure_fallback_to_ingestion() {
        let (_, hi_rx) = mpsc::unbounded_channel();
        let (subscriber_tx, mut subscriber_rx) = mpsc::channel(20);
        let cancel = CancellationToken::new();

        // Streaming service where peek fails on first attempt
        let mut streaming_client = MockStreamingClient::new(vec![], None);
        streaming_client.insert_error(); // Fail peek
        streaming_client.insert_checkpoint_range(0..20);

        let metrics = test_ingestion_metrics();
        let config = IngestionConfig {
            streaming_backoff_initial_batch_size: 5,
            ..test_config()
        };
        let h_broadcaster = broadcaster(
            0..20,
            None,
            Some(streaming_client),
            config,
            mock_client(metrics.clone()),
            hi_rx,
            vec![subscriber_tx],
            metrics.clone(),
            cancel.clone(),
        );

        // Should fallback to ingestion for first 10 checkpoints
        assert_eq!(
            recv_set(&mut subscriber_rx, 5).await,
            BTreeSet::from_iter(0..5)
        );

        // Then stream the remaining
        assert_eq!(
            recv_vec(&mut subscriber_rx, 15).await,
            Vec::from_iter(5..20)
        );

        // Verify both were used
        assert_eq!(metrics.total_ingested_checkpoints.get(), 5);
        assert_eq!(metrics.total_streamed_checkpoints.get(), 15);

        cancel.cancel();
        h_broadcaster.await.unwrap();
    }

    #[tokio::test]
    async fn streaming_connection_retry_with_backoff() {
        let (_, hi_rx) = mpsc::unbounded_channel();
        let (subscriber_tx, mut subscriber_rx) = mpsc::channel(50);
        let cancel = CancellationToken::new();

        // Streaming client where connection always fails (never recovers)
        let streaming_client =
            MockStreamingClient::new(0..50, None).fail_connection_times(usize::MAX);

        let metrics = test_ingestion_metrics();

        let h_broadcaster = broadcaster(
            0..50,
            None,
            Some(streaming_client),
            test_config(),
            mock_client(metrics.clone()),
            hi_rx,
            vec![subscriber_tx],
            metrics.clone(),
            cancel.clone(),
        );

        // Should fallback to ingestion for all checkpoints
        assert_eq!(
            recv_set(&mut subscriber_rx, 50).await,
            BTreeSet::from_iter(0..50)
        );

        // Verify failure counter incremented 6 times with batche sizes 2 -> 4 -> 8 -> 16 -> 16 -> 4 (completing the last 4).
        assert_eq!(metrics.total_streaming_connection_failures.get(), 6);

        // Verify only ingestion was used (streaming never succeeded)
        assert_eq!(metrics.total_ingested_checkpoints.get(), 50);
        assert_eq!(metrics.total_streamed_checkpoints.get(), 0);

        cancel.cancel();
        h_broadcaster.await.unwrap();
    }

    #[tokio::test]
    async fn streaming_connection_failure_backoff_reset() {
        // Test that after a successful streaming connection, the backoff state resets.

        let (_, hi_rx) = mpsc::unbounded_channel();
        let (subscriber_tx, mut subscriber_rx) = mpsc::channel(50);
        let cancel = CancellationToken::new();

        let mut streaming_client = MockStreamingClient::new(0..40, None).fail_connection_times(4);
        streaming_client.insert_error(); // First error to get back to main loop
        streaming_client.insert_error(); // Then fail peek
        streaming_client.insert_checkpoint_range(40..50); // Complete the rest

        let metrics = test_ingestion_metrics();

        let h_broadcaster = broadcaster(
            0..50,
            None,
            Some(streaming_client),
            test_config(),
            mock_client(metrics.clone()),
            hi_rx,
            vec![subscriber_tx],
            metrics.clone(),
            cancel.clone(),
        );

        // Should fallback to ingestion for first 2 + 4 + 8 + 16 = 30 checkpoints
        assert_eq!(
            recv_set(&mut subscriber_rx, 30).await,
            BTreeSet::from_iter(0..30)
        );

        // Then should stream 30-40 before peek fails
        assert_eq!(
            recv_vec(&mut subscriber_rx, 10).await,
            Vec::from_iter(30..40)
        );

        // Then fallback to ingestion for the next 2 checkpoints, since backoff should have reset
        assert_eq!(
            recv_set(&mut subscriber_rx, 2).await,
            BTreeSet::from_iter(40..42)
        );

        // Finally stream the last 8 checkpoints
        assert_eq!(
            recv_vec(&mut subscriber_rx, 8).await,
            Vec::from_iter(42..50)
        );

        // Verify failure counter incremented 5 times
        assert_eq!(metrics.total_streaming_connection_failures.get(), 5);

        // Ingestion was used for 2 + 4 + 8 + 16 + 2 = 32 checkpoints
        assert_eq!(metrics.total_ingested_checkpoints.get(), 32);
        assert_eq!(metrics.total_streamed_checkpoints.get(), 18);

        cancel.cancel();
        h_broadcaster.await.unwrap();
    }

    // =============== Part 4: Streaming timeouts ==================

    #[tokio::test]
    async fn streaming_connection_timeout_fallback_to_ingestion() {
        let (_, hi_rx) = mpsc::unbounded_channel();
        let (subscriber_tx, mut subscriber_rx) = mpsc::channel(20);
        let cancel = CancellationToken::new();

        // Streaming service that times out on connection
        let streaming_service = MockStreamingClient::new(0..20, Some(Duration::from_millis(150)))
            .fail_connection_with_timeout(1);

        let metrics = test_ingestion_metrics();
        let config = IngestionConfig {
            streaming_backoff_initial_batch_size: 5,
            ..test_config()
        };
        let h_broadcaster = broadcaster(
            0..20,
            None,
            Some(streaming_service),
            config,
            mock_client(metrics.clone()),
            hi_rx,
            vec![subscriber_tx],
            metrics.clone(),
            cancel.clone(),
        );

        // Should fallback to ingestion for initial batch size checkpoints
        assert_eq!(
            recv_set(&mut subscriber_rx, 5).await,
            BTreeSet::from_iter(0..5)
        );

        // After the timeout, it should complete the remaining checkpoints from streaming
        assert_eq!(
            recv_vec(&mut subscriber_rx, 15).await,
            Vec::from_iter(5..20)
        );

        assert_eq!(metrics.total_ingested_checkpoints.get(), 5);
        assert_eq!(metrics.total_streamed_checkpoints.get(), 15);
        assert_eq!(metrics.total_streaming_connection_failures.get(), 1);

        cancel.cancel();
        h_broadcaster.await.unwrap();
    }

    #[tokio::test]
    async fn streaming_peek_timeout_fallback_to_ingestion() {
        let (_, hi_rx) = mpsc::unbounded_channel();
        let (subscriber_tx, mut subscriber_rx) = mpsc::channel(20);
        let cancel = CancellationToken::new();

        // Streaming service where peek times out on first attempt
        let mut streaming_client =
            MockStreamingClient::new(vec![], Some(Duration::from_millis(150)));
        streaming_client.insert_timeout(); // Timeout during peek
        streaming_client.insert_checkpoint_range(0..20);

        let metrics = test_ingestion_metrics();
        let config = IngestionConfig {
            streaming_backoff_initial_batch_size: 5,
            ..test_config()
        };
        let h_broadcaster = broadcaster(
            0..20,
            None,
            Some(streaming_client),
            config,
            mock_client(metrics.clone()),
            hi_rx,
            vec![subscriber_tx],
            metrics.clone(),
            cancel.clone(),
        );

        // Should fallback to ingestion for first batch
        assert_eq!(
            recv_set(&mut subscriber_rx, 5).await,
            BTreeSet::from_iter(0..5)
        );

        // Then stream the remaining
        assert_eq!(
            recv_vec(&mut subscriber_rx, 15).await,
            Vec::from_iter(5..20)
        );

        // Verify both were used
        assert_eq!(metrics.total_ingested_checkpoints.get(), 5);
        assert_eq!(metrics.total_streamed_checkpoints.get(), 15);

        cancel.cancel();
        h_broadcaster.await.unwrap();
    }

    #[tokio::test]
    async fn streaming_timeout_during_streaming() {
        let (_, hi_rx) = mpsc::unbounded_channel();
        let (subscriber_tx, mut subscriber_rx) = mpsc::channel(20);
        let cancel = CancellationToken::new();

        // Create streaming client with timeout injected mid-stream
        let mut streaming_client = MockStreamingClient::new(0..5, Some(Duration::from_millis(150)));
        streaming_client.insert_timeout(); // Timeout after 5 checkpoints
        streaming_client.insert_checkpoint_range(10..15);

        let metrics = test_ingestion_metrics();
        let h_broadcaster = broadcaster(
            0..15,
            None,
            Some(streaming_client),
            test_config(),
            mock_client(metrics.clone()),
            hi_rx,
            vec![subscriber_tx],
            metrics.clone(),
            cancel.clone(),
        );

        // Should receive first 5 checkpoints from streaming in order
        assert_eq!(recv_vec(&mut subscriber_rx, 5).await, Vec::from_iter(0..5));

        // After timeout, should fallback and complete via ingestion/retry (order not guaranteed)
        assert_eq!(
            recv_set(&mut subscriber_rx, 10).await,
            BTreeSet::from_iter(5..15)
        );

        // Verify streaming was used initially and later recovered
        assert_eq!(metrics.total_streamed_checkpoints.get(), 10);
        // Then ingestion was used to recover the missing checkpoints
        assert_eq!(metrics.total_ingested_checkpoints.get(), 5);
        // The last checkpoint should come from streaming after recovery
        assert_eq!(metrics.latest_streamed_checkpoint.get(), 14);

        cancel.cancel();
        h_broadcaster.await.unwrap();
    }
}
