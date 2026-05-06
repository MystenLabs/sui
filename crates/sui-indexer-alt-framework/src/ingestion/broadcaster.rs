// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::marker::Unpin;
use std::sync::Arc;
use std::time::Duration;

use anyhow::Context;
use anyhow::anyhow;
use futures::Stream;
use futures::TryStreamExt;
use futures::future::try_join_all;
use sui_futures::service::Service;
use sui_futures::stream::Break;
use sui_futures::stream::TrySpawnStreamExt;
use sui_futures::task::TaskGuard;
use tokio::sync::mpsc;
use tokio_stream::StreamExt;
use tracing::debug;
use tracing::info;
use tracing::warn;

use crate::config::ConcurrencyConfig;
use crate::ingestion::IngestionConfig;
use crate::ingestion::error::Error;
use crate::ingestion::ingestion_client::CheckpointEnvelope;
use crate::ingestion::ingestion_client::IngestionClient;
use crate::ingestion::streaming_client::CheckpointStream;
use crate::ingestion::streaming_client::CheckpointStreamingClient;
use crate::metrics::IngestionMetrics;

/// If the network's latest checkpoint (per the streaming client) is more than this many
/// checkpoints ahead of where ingestion currently is, skip streaming and let the ingestion
/// path catch up first.
const STREAMING_CATCHUP_THRESHOLD: u64 = 1_000;

/// Broadcaster task that manages checkpoint flow and spawns broadcast tasks for ranges
/// via either streaming or ingesting, or both.
///
/// This task spawns streaming or ingesting tasks for the requested checkpoint range. Depending
/// on the current latest checkpoint available from streaming, it may spawn either or both tasks
/// to cover the requested range. The overall idea is that ingestion covers the range
/// `[start, network_latest_cp)`, while streaming covers `[network_latest_cp, end)`. When both
/// finish (failure or range completion), the overall watermark is updated and the loop runs
/// again if the requested range is not yet complete.
///
/// Backpressure is **per-subscriber, channel-fill based**: each subscriber's bounded mpsc
/// channel acts as both transport and the backpressure signal. When any subscriber's channel
/// fills, [`TrySpawnStreamExt::try_for_each_broadcast_spawned`]'s adaptive controller cuts
/// ingest concurrency. The task will shut down if the `checkpoints` range completes.
pub(super) fn broadcaster<R, S>(
    checkpoints: R,
    mut streaming_client: Option<S>,
    config: IngestionConfig,
    client: IngestionClient,
    subscribers: Vec<mpsc::Sender<Arc<CheckpointEnvelope>>>,
    metrics: Arc<IngestionMetrics>,
) -> Service
where
    R: std::ops::RangeBounds<u64> + Send + 'static,
    S: CheckpointStreamingClient + Send + 'static,
{
    Service::new().spawn_aborting(async move {
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

        // If the first attempt at streaming connection fails, we back off for an initial number
        // of checkpoints to process using ingestion. This value doubles on each subsequent failure.
        let mut streaming_backoff_batch_size = config.streaming_backoff_initial_batch_size as u64;

        // Initialize the overall checkpoint_hi watermark to start_cp.
        // This value is updated every outer loop iteration after both streaming and broadcasting complete.
        let mut checkpoint_hi = start_cp;

        while checkpoint_hi < end_cp {
            // Set up the streaming task for the current range [checkpoint_hi, end_cp). This function
            // will return a handle to the streaming task and the end cp of the ingestion task, calculated
            // based on 1) if streaming is used, 2) streaming connection success status, and 3) the network
            // latest checkpoint we get from a success streaming connection.
            // The ingestion task fill up the gap from checkpoint_hi to ingestion_end (exclusive) while the streaming
            // task covers from ingestion_end to end_cp.
            let (stream_guard, ingestion_end) = setup_streaming_task(
                &mut streaming_client,
                checkpoint_hi,
                end_cp,
                &mut streaming_backoff_batch_size,
                &config,
                &subscribers,
                &metrics,
            )
            .await;

            // Spawn a broadcaster task for this range.
            // It will exit when the range is complete or if it is cancelled.
            let ingest_guard = ingest_and_broadcast_range(
                checkpoint_hi,
                ingestion_end,
                config.retry_interval(),
                config.ingest_concurrency.clone(),
                client.clone(),
                subscribers.clone(),
                metrics.clone(),
            );

            let (streaming_result, ingestion_result) =
                futures::future::join(stream_guard, ingest_guard).await;

            // Check ingestion result, exit on any error.
            match ingestion_result.context("Ingestion task panicked, stopping broadcaster")? {
                Ok(()) => {}

                // Ingestion stopped because one of its channels was closed. The
                // overall broadcaster should also shutdown.
                Err(Break::Break) => break,

                // Ingestion failed with an error of some kind, surface this as an
                // overall error from the broadcaster.
                Err(Break::Err(e)) => {
                    return Err(anyhow!(e).context("Ingestion task failed, stopping broadcaster"));
                }
            }

            // Update checkpoint_hi from streaming, or shutdown on error
            checkpoint_hi =
                streaming_result.context("Streaming task panicked, stopping broadcaster")?;

            info!(
                checkpoint_hi,
                "Both tasks completed, moving on to next range"
            );
        }

        info!("Checkpoints done, stopping broadcaster");
        Ok(())
    })
}

/// Fetch and broadcast checkpoints from a range [start..end) to subscribers. The adaptive
/// controller reads the max `fill` across subscribers (channel capacity for bounded,
/// `len / soft_limit` for unbounded) and adjusts ingest concurrency to match.
fn ingest_and_broadcast_range(
    start: u64,
    end: u64,
    retry_interval: Duration,
    ingest_concurrency: ConcurrencyConfig,
    client: IngestionClient,
    subscribers: Vec<mpsc::Sender<Arc<CheckpointEnvelope>>>,
    metrics: Arc<IngestionMetrics>,
) -> TaskGuard<Result<(), Break<Error>>> {
    TaskGuard::new(tokio::spawn(async move {
        let report_metrics = metrics.clone();
        futures::stream::iter(start..end)
            .try_for_each_broadcast_spawned(
                ingest_concurrency.into(),
                |cp| {
                    let client = client.clone();
                    async move {
                        // Fetch the checkpoint or stop if cancelled.
                        let checkpoint_envelope = client.wait_for(cp, retry_interval).await?;
                        debug!(checkpoint = cp, "Fetched checkpoint");
                        Ok(Arc::new(checkpoint_envelope))
                    }
                },
                subscribers,
                move |stats| {
                    report_metrics
                        .ingestion_concurrency_limit
                        .set(stats.limit as i64);
                    report_metrics
                        .ingestion_concurrency_inflight
                        .set(stats.inflight as i64);
                },
            )
            .await
    }))
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
    subscribers: &[mpsc::Sender<Arc<CheckpointEnvelope>>],
    metrics: &Arc<IngestionMetrics>,
) -> (TaskGuard<u64>, u64)
where
    S: CheckpointStreamingClient,
{
    // No streaming client configured so we ingest all the way to end_cp.
    let Some(streaming_client) = streaming_client else {
        return (noop_streaming_task(end_cp), end_cp);
    };

    let backoff_batch_size = *streaming_backoff_batch_size;

    // Convenient closure to handle streaming fallback logic due to connection or peek failure.
    let mut fallback = |reason: &str| {
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

    let CheckpointStream {
        mut stream,
        chain_id,
    } = match streaming_client.connect().await {
        Ok(checkpoint_stream) => checkpoint_stream,
        Err(e) => {
            return fallback(&format!("Streaming connection failed: {e}"));
        }
    };

    let checkpoint_envelope = match stream.peek().await {
        Some(Ok(checkpoint)) => CheckpointEnvelope {
            checkpoint: Arc::new(checkpoint.clone()),
            chain_id,
        },
        Some(Err(e)) => {
            return fallback(&format!("Failed to peek latest checkpoint: {e}"));
        }
        None => {
            return fallback("Stream ended during peek");
        }
    };

    // We have successfully connected and peeked, reset backoff batch size.
    *streaming_backoff_batch_size = config.streaming_backoff_initial_batch_size as u64;

    let network_latest_cp = *checkpoint_envelope.checkpoint.summary.sequence_number();
    let ingestion_end = network_latest_cp.min(end_cp);
    if network_latest_cp > checkpoint_hi + STREAMING_CATCHUP_THRESHOLD {
        info!(
            network_latest_cp,
            checkpoint_hi,
            threshold = STREAMING_CATCHUP_THRESHOLD,
            "Network is far ahead, delaying streaming start to let ingestion catch up"
        );
        return (noop_streaming_task(ingestion_end), ingestion_end);
    }

    info!(
        network_latest_cp,
        checkpoint_hi, "Within catchup threshold, starting streaming"
    );

    let envelope_stream = stream.map_ok(move |checkpoint| CheckpointEnvelope {
        checkpoint: Arc::new(checkpoint),
        chain_id,
    });
    let stream_guard = TaskGuard::new(tokio::spawn(stream_and_broadcast_range(
        network_latest_cp.max(checkpoint_hi),
        end_cp,
        envelope_stream,
        subscribers.to_vec(),
        metrics.clone(),
    )));

    (stream_guard, ingestion_end)
}

/// Streams and broadcasts checkpoints from a range [start, end) to subscribers. Each
/// `mpsc::Sender::send` honors that subscriber's channel capacity, so a slow consumer
/// naturally stalls the streaming side. If we encounter any streaming error or out-of-order
/// checkpoint greater than the current `lo`, we stop streaming and return `lo` so the main
/// loop can reconnect and fill in the gap using ingestion.
async fn stream_and_broadcast_range(
    mut lo: u64,
    hi: u64,
    mut stream: impl Stream<Item = Result<CheckpointEnvelope, Error>> + Unpin,
    subscribers: Vec<mpsc::Sender<Arc<CheckpointEnvelope>>>,
    metrics: Arc<IngestionMetrics>,
) -> u64 {
    while lo < hi {
        let Some(item) = stream.next().await else {
            warn!(lo, "Streaming ended unexpectedly");
            break;
        };

        let checkpoint_envelope = match item {
            Ok(checkpoint_envelope) => checkpoint_envelope,
            Err(e) => {
                warn!(lo, "Streaming error: {e}");
                break;
            }
        };

        let sequence_number = *checkpoint_envelope.checkpoint.summary.sequence_number();

        if sequence_number < lo {
            debug!(
                checkpoint = sequence_number,
                lo, "Skipping already processed checkpoint"
            );
            metrics.total_skipped_streamed_checkpoints.inc();
            metrics
                .latest_skipped_streamed_checkpoint
                .set(sequence_number as i64);
            continue;
        }

        if sequence_number > lo {
            warn!(checkpoint = sequence_number, lo, "Out-of-order checkpoint");
            metrics.total_out_of_order_streamed_checkpoints.inc();
            // Return to main loop to fill up the gap.
            break;
        }

        assert_eq!(sequence_number, lo);

        if send_checkpoint(Arc::new(checkpoint_envelope), &subscribers)
            .await
            .is_err()
        {
            break;
        }

        debug!(checkpoint = lo, "Streamed checkpoint");
        metrics.total_streamed_checkpoints.inc();
        metrics.latest_streamed_checkpoint.set(lo as i64);
        lo += 1;
    }

    // We exit the loop either due to cancellation, error or completion of the range,
    // in all cases we disconnect the stream and return the current watermark.
    metrics.total_stream_disconnections.inc();
    lo
}

// A noop streaming task that just returns the provided checkpoint_hi, used to simplify
// join logic when streaming is not used.
fn noop_streaming_task(checkpoint_hi: u64) -> TaskGuard<u64> {
    TaskGuard::new(tokio::spawn(async move { checkpoint_hi }))
}

/// Send a checkpoint to all subscribers. Returns an error if any subscriber's channel is closed.
async fn send_checkpoint(
    checkpoint_envelope: Arc<CheckpointEnvelope>,
    subscribers: &[mpsc::Sender<Arc<CheckpointEnvelope>>],
) -> Result<Vec<()>, mpsc::error::SendError<Arc<CheckpointEnvelope>>> {
    let futures = subscribers
        .iter()
        .map(|s| s.send(checkpoint_envelope.clone()));
    try_join_all(futures).await
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;
    use std::ops::Range;
    use std::sync::Arc;
    use std::time::Duration;

    use tokio::time::timeout;

    use crate::ingestion::IngestionConfig;
    use crate::ingestion::ingestion_client::tests::MockIngestionClient;
    use crate::ingestion::streaming_client::test_utils::MockStreamingClient;
    use crate::metrics::tests::test_ingestion_metrics;

    use super::*;

    /// Create a mock `IngestionClient` that serves synthetic checkpoints for the given
    /// sequence-number range.
    fn mock_client_with_range(
        checkpoints: Range<u64>,
        metrics: Arc<IngestionMetrics>,
    ) -> IngestionClient {
        let mock = MockIngestionClient::default();
        mock.insert_checkpoints(checkpoints);
        IngestionClient::new_impl(Arc::new(mock), metrics)
    }

    /// Create a test config
    fn test_config() -> IngestionConfig {
        IngestionConfig {
            ingest_concurrency: ConcurrencyConfig::Fixed { value: 2 },
            retry_interval_ms: 100,
            streaming_backoff_initial_batch_size: 2,
            streaming_backoff_max_batch_size: 16,
            streaming_connection_timeout_ms: 100,
            streaming_statement_timeout_ms: 100,
        }
    }

    /// Wait up to a second for a response on the stream, and return it, expecting this operation
    /// to succeed.
    async fn expect_recv<S>(rx: &mut S) -> Option<S::Item>
    where
        S: Stream + Unpin,
    {
        timeout(Duration::from_secs(1), rx.next()).await.unwrap()
    }

    /// Receive `count` checkpoints from the stream and return their sequence numbers as a Vec.
    /// Maintains order, useful for verifying sequential delivery (e.g., from streaming).
    async fn recv_vec<S>(rx: &mut S, count: usize) -> Vec<u64>
    where
        S: Stream<Item = Arc<CheckpointEnvelope>> + Unpin,
    {
        let mut result = Vec::with_capacity(count);
        for _ in 0..count {
            let checkpoint_envelope = expect_recv(rx).await.unwrap();
            assert_eq!(
                checkpoint_envelope.chain_id,
                MockStreamingClient::mock_chain_id()
            );
            result.push(*checkpoint_envelope.checkpoint.summary.sequence_number());
        }
        result
    }

    /// Build a subscribers list with a single bounded subscriber of the given capacity. Returns
    /// the senders vec (to pass into `broadcaster(...)`) and the subscriber's stream.
    fn single_subscriber(
        capacity: usize,
    ) -> (
        Vec<mpsc::Sender<Arc<CheckpointEnvelope>>>,
        impl Stream<Item = Arc<CheckpointEnvelope>> + Send + Unpin + 'static,
    ) {
        let (tx, rx) = mpsc::channel(capacity);
        (vec![tx], tokio_stream::wrappers::ReceiverStream::new(rx))
    }

    /// Receive `count` checkpoints from the stream and return their sequence numbers as a BTreeSet.
    /// Useful for verifying unordered delivery (e.g., from concurrent ingestion).
    async fn recv_set<S>(rx: &mut S, count: usize) -> BTreeSet<u64>
    where
        S: Stream<Item = Arc<CheckpointEnvelope>> + Unpin,
    {
        let mut result = BTreeSet::new();
        for _ in 0..count {
            let checkpoint_envelope = expect_recv(rx).await.unwrap();
            assert_eq!(
                checkpoint_envelope.chain_id,
                MockStreamingClient::mock_chain_id()
            );
            let sequence_number = *checkpoint_envelope.checkpoint.summary.sequence_number();
            let inserted = result.insert(sequence_number);
            assert!(
                inserted,
                "Received duplicate checkpoint {}",
                sequence_number
            );
        }
        result
    }

    #[tokio::test]
    async fn finite_list_of_checkpoints() {
        let (subscriber_dest, mut subscriber_rx) = single_subscriber(1);

        let cps = 0..5;
        let metrics = test_ingestion_metrics();
        let mut svc = broadcaster::<_, MockStreamingClient>(
            cps,
            None,
            test_config(),
            mock_client_with_range(0..5, metrics.clone()),
            subscriber_dest,
            metrics,
        );

        assert_eq!(
            recv_set(&mut subscriber_rx, 5).await,
            BTreeSet::from_iter(0..5)
        );

        svc.join().await.unwrap();
    }

    #[tokio::test]
    async fn shutdown_on_sender_closed() {
        let (subscriber_dest, mut subscriber_rx) = single_subscriber(1);

        let metrics = test_ingestion_metrics();
        let mut svc = broadcaster::<_, MockStreamingClient>(
            0..,
            None,
            test_config(),
            mock_client_with_range(0..20, metrics.clone()),
            subscriber_dest,
            metrics,
        );

        assert_eq!(
            recv_set(&mut subscriber_rx, 5).await,
            BTreeSet::from_iter(0..5)
        );

        drop(subscriber_rx);
        svc.join().await.unwrap();
    }

    #[tokio::test]
    async fn shutdown() {
        let (subscriber_dest, mut subscriber_rx) = single_subscriber(1);

        let metrics = test_ingestion_metrics();
        let svc = broadcaster::<_, MockStreamingClient>(
            0..,
            None,
            test_config(),
            mock_client_with_range(0..20, metrics.clone()),
            subscriber_dest,
            metrics,
        );

        assert_eq!(
            recv_set(&mut subscriber_rx, 5).await,
            BTreeSet::from_iter(0..5)
        );

        svc.shutdown().await.unwrap();
    }

    #[tokio::test]
    async fn multiple_physical_subscribers() {
        let (tx1, rx1) = mpsc::channel(1);
        let (tx2, rx2) = mpsc::channel(1);
        let subscribers = vec![tx1, tx2];
        let mut subscriber_rx1 = tokio_stream::wrappers::ReceiverStream::new(rx1);
        let mut subscriber_rx2 = tokio_stream::wrappers::ReceiverStream::new(rx2);

        let metrics = test_ingestion_metrics();
        let mut svc = broadcaster::<_, MockStreamingClient>(
            0..,
            None,
            test_config(),
            mock_client_with_range(0..20, metrics.clone()),
            subscribers,
            metrics,
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
        svc.join().await.unwrap();
    }

    // =============== Streaming Tests ==================

    // =============== Part 1: Basic Streaming ==================

    #[tokio::test]
    async fn streaming_only() {
        let (subscriber_dest, mut subscriber_rx) = single_subscriber(10);

        // Create a mock streaming service with checkpoints 0..5
        let streaming_client = MockStreamingClient::new(0..5, None);

        let metrics = test_ingestion_metrics();
        let mut svc = broadcaster(
            0..5, // Bounded range
            Some(streaming_client),
            test_config(),
            mock_client_with_range(0..5, metrics.clone()),
            subscriber_dest,
            metrics.clone(),
        );

        // Should receive all checkpoints from the stream in order
        assert_eq!(recv_vec(&mut subscriber_rx, 5).await, Vec::from_iter(0..5));

        // We should get all checkpoints from streaming.
        assert_eq!(metrics.total_streamed_checkpoints.get(), 5);
        assert_eq!(metrics.total_ingested_checkpoints.get(), 0);
        assert_eq!(metrics.latest_streamed_checkpoint.get(), 4);

        svc.join().await.unwrap();
    }

    #[tokio::test]
    async fn streaming_with_transition() {
        let (subscriber_dest, mut subscriber_rx) = single_subscriber(100);

        // Create a mock streaming service that starts at checkpoint 50
        // This simulates streaming being ahead of ingestion
        let streaming_client = MockStreamingClient::new(49..60, None);

        let metrics = test_ingestion_metrics();
        let mut svc = broadcaster(
            0..60,
            Some(streaming_client),
            test_config(),
            mock_client_with_range(0..60, metrics.clone()),
            subscriber_dest,
            metrics.clone(),
        );

        assert_eq!(
            recv_set(&mut subscriber_rx, 60).await,
            BTreeSet::from_iter(0..60)
        );

        // Verify both ingestion and streaming were used. The exact split depends on the
        // peek'd network_latest (49) and STREAMING_CATCHUP_THRESHOLD: streaming begins at
        // the peek'd checkpoint, ingestion fills in below it.
        assert_eq!(metrics.total_ingested_checkpoints.get(), 49); // [0..49)
        assert_eq!(metrics.total_streamed_checkpoints.get(), 11); // [49..60)
        assert_eq!(metrics.latest_streamed_checkpoint.get(), 59);

        svc.join().await.unwrap();
    }

    // =============== Part 2: Edge Cases ==================

    #[tokio::test]
    async fn streaming_beyond_end_checkpoint() {
        // Test scenario where streaming service starts beyond the requested end checkpoint.
        let (subscriber_dest, mut subscriber_rx) = single_subscriber(30);

        // Streaming starts at checkpoint 100, but we only want 0..30.
        let streaming_client = MockStreamingClient::new(100..110, None);

        let metrics = test_ingestion_metrics();
        let mut svc = broadcaster(
            0..30,
            Some(streaming_client),
            test_config(),
            mock_client_with_range(0..30, metrics.clone()),
            subscriber_dest,
            metrics.clone(),
        );

        // Should use only ingestion since streaming is beyond end_cp
        assert_eq!(
            recv_set(&mut subscriber_rx, 30).await,
            BTreeSet::from_iter(0..30)
        );

        // Verify no streaming was used (all from ingestion)
        assert_eq!(metrics.total_streamed_checkpoints.get(), 0);
        assert_eq!(metrics.total_ingested_checkpoints.get(), 30);

        svc.join().await.unwrap();
    }

    #[tokio::test]
    async fn streaming_before_start_checkpoint() {
        // Test scenario where streaming starts before the requested start checkpoint.
        let (subscriber_dest, mut subscriber_rx) = single_subscriber(30);

        // Streaming starts at checkpoint 0 but indexing starts at 30.
        let streaming_client = MockStreamingClient::new(0..100, None);

        let metrics = test_ingestion_metrics();
        let mut svc = broadcaster(
            30..100,
            Some(streaming_client),
            test_config(),
            mock_client_with_range(30..100, metrics.clone()),
            subscriber_dest,
            metrics.clone(),
        );

        assert_eq!(
            recv_vec(&mut subscriber_rx, 70).await,
            Vec::from_iter(30..100)
        );

        // Verify only streaming was used (all from streaming)
        assert_eq!(metrics.total_streamed_checkpoints.get(), 70);
        assert_eq!(metrics.total_ingested_checkpoints.get(), 0);
        assert_eq!(metrics.latest_streamed_checkpoint.get(), 99);
        assert_eq!(metrics.total_skipped_streamed_checkpoints.get(), 30);
        assert_eq!(metrics.latest_skipped_streamed_checkpoint.get(), 29);

        svc.join().await.unwrap();
    }

    #[tokio::test]
    async fn streaming_behind_watermark_skips_duplicates() {
        // Test scenario where streaming service provides checkpoints behind the current watermark,
        // which should be skipped.
        let (subscriber_dest, mut subscriber_rx) = single_subscriber(50);

        // Create streaming client that returns some checkpoints behind the watermark
        let mut streaming_client = MockStreamingClient::new(0..15, None);
        // Insert duplicate/old checkpoints that should be skipped
        streaming_client.insert_checkpoint(3); // Behind watermark
        streaming_client.insert_checkpoint(4); // Behind watermark
        streaming_client.insert_checkpoint_range(15..20);

        let metrics = test_ingestion_metrics();
        let mut svc = broadcaster(
            0..20,
            Some(streaming_client),
            test_config(),
            mock_client_with_range(0..20, metrics.clone()),
            subscriber_dest,
            metrics.clone(),
        );

        // Should receive all checkpoints exactly once (no duplicates) from streaming.
        assert_eq!(
            recv_vec(&mut subscriber_rx, 20).await,
            Vec::from_iter(0..20)
        );

        assert_eq!(metrics.total_streamed_checkpoints.get(), 20);
        assert_eq!(metrics.total_ingested_checkpoints.get(), 0);
        assert_eq!(metrics.latest_streamed_checkpoint.get(), 19);
        assert_eq!(metrics.total_skipped_streamed_checkpoints.get(), 2);
        assert_eq!(metrics.latest_skipped_streamed_checkpoint.get(), 4);

        svc.join().await.unwrap();
    }

    #[tokio::test]
    async fn streaming_ahead_of_watermark_recovery() {
        // Test scenario where streaming service has a gap ahead of the watermark,
        // requiring fallback to ingestion to fill the gap.
        let (subscriber_dest, mut subscriber_rx) = single_subscriber(50);

        // Create streaming client that has a gap (checkpoint ahead of expected watermark)
        let mut streaming_client = MockStreamingClient::new(0..3, None);
        streaming_client.insert_checkpoint_range(6..10); // Gap: skips checkpoints 3 - 5

        let metrics = test_ingestion_metrics();
        let mut svc = broadcaster(
            0..10,
            Some(streaming_client),
            test_config(),
            mock_client_with_range(0..10, metrics.clone()),
            subscriber_dest,
            metrics.clone(),
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
        assert_eq!(metrics.total_out_of_order_streamed_checkpoints.get(), 1);

        svc.join().await.unwrap();
    }

    // =============== Part 3: Streaming Errors ==================

    #[tokio::test]
    async fn streaming_error_during_streaming() {
        let (subscriber_dest, mut subscriber_rx) = single_subscriber(20);

        // Create streaming client with error injected mid-stream
        let mut streaming_client = MockStreamingClient::new(0..5, None);
        streaming_client.insert_error(); // Error after 5 checkpoints
        streaming_client.insert_checkpoint_range(10..15);

        let metrics = test_ingestion_metrics();
        let mut svc = broadcaster(
            0..15,
            Some(streaming_client),
            test_config(),
            mock_client_with_range(0..15, metrics.clone()),
            subscriber_dest,
            metrics.clone(),
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

        svc.join().await.unwrap();
    }

    #[tokio::test]
    async fn streaming_multiple_errors_with_recovery() {
        let (subscriber_dest, mut subscriber_rx) = single_subscriber(50);

        // Create streaming client with multiple errors injected
        let mut streaming_client = MockStreamingClient::new(0..5, None);
        streaming_client.insert_error(); // Error at checkpoint 5
        streaming_client.insert_checkpoint_range(5..10);
        streaming_client.insert_error(); // Error at checkpoint 10
        streaming_client.insert_checkpoint_range(10..20);

        let metrics = test_ingestion_metrics();
        let mut svc = broadcaster(
            0..20,
            Some(streaming_client),
            test_config(),
            mock_client_with_range(0..20, metrics.clone()),
            subscriber_dest,
            metrics.clone(),
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

        svc.join().await.unwrap();
    }

    #[tokio::test]
    async fn streaming_start_failure_fallback_to_ingestion() {
        let (subscriber_dest, mut subscriber_rx) = single_subscriber(20);

        // Streaming service that fails to start
        let streaming_service = MockStreamingClient::new(0..20, None).fail_connection_times(1);

        let metrics = test_ingestion_metrics();
        let mut svc = broadcaster(
            0..20,
            Some(streaming_service),
            IngestionConfig {
                streaming_backoff_initial_batch_size: 5,
                ..test_config()
            },
            mock_client_with_range(0..20, metrics.clone()),
            subscriber_dest,
            metrics.clone(),
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

        svc.join().await.unwrap();
    }

    #[tokio::test]
    async fn streaming_peek_failure_fallback_to_ingestion() {
        let (subscriber_dest, mut subscriber_rx) = single_subscriber(20);

        // Streaming service where peek fails on first attempt
        let mut streaming_client = MockStreamingClient::new(vec![], None);
        streaming_client.insert_error(); // Fail peek
        streaming_client.insert_checkpoint_range(0..20);

        let metrics = test_ingestion_metrics();
        let mut svc = broadcaster(
            0..20,
            Some(streaming_client),
            IngestionConfig {
                streaming_backoff_initial_batch_size: 5,
                ..test_config()
            },
            mock_client_with_range(0..20, metrics.clone()),
            subscriber_dest,
            metrics.clone(),
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

        svc.join().await.unwrap();
    }

    #[tokio::test]
    async fn streaming_connection_retry_with_backoff() {
        let (subscriber_dest, mut subscriber_rx) = single_subscriber(50);

        // Streaming client where connection always fails (never recovers)
        let streaming_client =
            MockStreamingClient::new(0..50, None).fail_connection_times(usize::MAX);

        let metrics = test_ingestion_metrics();
        let mut svc = broadcaster(
            0..50,
            Some(streaming_client),
            test_config(),
            mock_client_with_range(0..50, metrics.clone()),
            subscriber_dest,
            metrics.clone(),
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

        svc.join().await.unwrap();
    }

    #[tokio::test]
    async fn streaming_connection_failure_backoff_reset() {
        // Test that after a successful streaming connection, the backoff state resets.
        let (subscriber_dest, mut subscriber_rx) = single_subscriber(50);

        let mut streaming_client = MockStreamingClient::new(0..40, None).fail_connection_times(4);
        streaming_client.insert_error(); // First error to get back to main loop
        streaming_client.insert_error(); // Then fail peek
        streaming_client.insert_checkpoint_range(40..50); // Complete the rest

        let metrics = test_ingestion_metrics();
        let mut svc = broadcaster(
            0..50,
            Some(streaming_client),
            test_config(),
            mock_client_with_range(0..50, metrics.clone()),
            subscriber_dest,
            metrics.clone(),
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

        svc.join().await.unwrap();
    }

    // =============== Part 4: Streaming timeouts ==================

    #[tokio::test]
    async fn streaming_connection_timeout_fallback_to_ingestion() {
        let (subscriber_dest, mut subscriber_rx) = single_subscriber(20);

        // Streaming service that times out on connection
        let streaming_service = MockStreamingClient::new(0..20, Some(Duration::from_millis(150)))
            .fail_connection_with_timeout(1);

        let metrics = test_ingestion_metrics();
        let config = IngestionConfig {
            streaming_backoff_initial_batch_size: 5,
            ..test_config()
        };
        let mut svc = broadcaster(
            0..20,
            Some(streaming_service),
            config,
            mock_client_with_range(0..20, metrics.clone()),
            subscriber_dest,
            metrics.clone(),
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

        svc.join().await.unwrap();
    }

    #[tokio::test]
    async fn streaming_peek_timeout_fallback_to_ingestion() {
        let (subscriber_dest, mut subscriber_rx) = single_subscriber(20);

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
        let mut svc = broadcaster(
            0..20,
            Some(streaming_client),
            config,
            mock_client_with_range(0..20, metrics.clone()),
            subscriber_dest,
            metrics.clone(),
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

        svc.join().await.unwrap();
    }

    #[tokio::test]
    async fn streaming_timeout_during_streaming() {
        let (subscriber_dest, mut subscriber_rx) = single_subscriber(20);

        // Create streaming client with timeout injected mid-stream
        let mut streaming_client = MockStreamingClient::new(0..5, Some(Duration::from_millis(150)));
        streaming_client.insert_timeout(); // Timeout after 5 checkpoints
        streaming_client.insert_checkpoint_range(10..15);

        let metrics = test_ingestion_metrics();
        let mut svc = broadcaster(
            0..15,
            Some(streaming_client),
            test_config(),
            mock_client_with_range(0..15, metrics.clone()),
            subscriber_dest,
            metrics.clone(),
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

        svc.join().await.unwrap();
    }
}
