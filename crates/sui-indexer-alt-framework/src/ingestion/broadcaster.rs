// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use futures::future::try_join_all;
use futures::stream;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use tokio::{sync::mpsc, task::JoinHandle};
use tokio_util::sync::CancellationToken;
use tracing::{error, info};

use crate::{
    ingestion::error::Error, task::TrySpawnStreamExt,
    types::full_checkpoint_content::CheckpointData,
};

use super::{client::IngestionClient, IngestionConfig};

/// Send a checkpoint to all subscribers.
/// Returns an error if any subscriber's channel is closed.
async fn send_checkpoint(
    checkpoint: Arc<CheckpointData>,
    subscribers: &[mpsc::Sender<Arc<CheckpointData>>],
) -> Result<(), Error> {
    let futures = subscribers.iter().map(|s| s.send(checkpoint.clone()));
    try_join_all(futures).await.map_err(|_| Error::Cancelled)?;
    Ok(())
}

/// Fetch and broadcasts checkpoints from a range to subscribers.
/// This task is ingest_hi-aware and will stop if it encounters a checkpoint
/// beyond the current ingest_hi.
async fn broadcast_range(
    start: u64,
    end: u64,
    retry_interval: std::time::Duration,
    ingest_concurrency: usize,
    ingest_hi: Arc<AtomicU64>,
    watermark: Arc<AtomicU64>,
    client: Arc<IngestionClient>,
    subscribers: Arc<Vec<mpsc::Sender<Arc<CheckpointData>>>>,
    cancel: CancellationToken,
) -> Result<(), Error> {
    let checkpoint_stream = stream::iter(start..=end);

    checkpoint_stream
        .try_for_each_spawned(ingest_concurrency, |cp| {
            let watermark = watermark.clone();
            let ingest_hi = ingest_hi.clone();
            let client = client.clone();
            let subscribers = subscribers.clone();

            // One clone is for the supervisor to signal a cancel if it detects a
            // subscriber that wants to wind down ingestion, and the other is to pass to
            // each worker to detect cancellation.
            let supervisor_cancel = cancel.clone();
            let cancel = cancel.clone();

            async move {
                // Check ingest_hi before fetching
                let current_ingest_hi = ingest_hi.load(Ordering::Acquire);

                // docs::#bound (see docs/content/guides/developer/advanced/custom-indexer.mdx)
                if cp > current_ingest_hi {
                    info!(
                        checkpoint = cp,
                        ingest_hi = current_ingest_hi,
                        "Stopping broadcast due to ingest_hi"
                    );
                    // Return Backpressured error to stop the entire broadcast_range task
                    return Err(Error::Backpressured(cp, current_ingest_hi));
                }
                // docs::/#bound

                // Fetch the checkpoint or stop if cancelled.
                let checkpoint = tokio::select! {
                    cp = client.wait_for(cp, retry_interval) => cp?,
                    _ = cancel.cancelled() => {
                        return Err(Error::Cancelled);
                    }
                };

                // Check ingest_hi again before sending (in case it changed during fetch)
                let current_ingest_hi = ingest_hi.load(Ordering::Acquire);
                if cp > current_ingest_hi {
                    info!(
                        checkpoint = cp,
                        ingest_hi = current_ingest_hi,
                        "Stopping broadcast due to ingest_hi after fetch"
                    );
                    return Err(Error::Backpressured(cp, current_ingest_hi));
                }

                // Send to all subscribers, or stop if any subscriber has closed.
                tokio::select! {
                    result = send_checkpoint(checkpoint, &subscribers) => {
                        if result.is_ok() {
                            // Atomically update the watermark. We need to use fetch_max here instead of just
                            // incrementing because multiple tasks may update the watermark out of order.
                            watermark.fetch_max(cp + 1, Ordering::AcqRel);
                            info!(checkpoint = cp, "Broadcasted checkpoint and updated watermark");
                        } else {
                            // If any subscriber has closed, we consider this a cancellation signal for the entire ingestion.
                            supervisor_cancel.cancel();
                        }
                        result
                    },
                    _ = cancel.cancelled() => Err(Error::Cancelled),
                }
            }
        })
        .await
}

/// Broadcaster task that manages checkpoint flow and spawns broadcast tasks for ranges.
///
/// This task:
/// 1. Maintains a ingest_hi based on subscriber feedback
/// 2. Spawns broadcaster tasks for checkpoint ranges. Right now the entire requested range is given to
///    each broadcaster, but with streaming support the end of the range could be different.
/// 3. Monitors broadcaster completion due to backpressure and spawns new ones as needed.
///
/// The task will shut down if the `cancel` token is signalled, or if the `checkpoints` range completes.
pub(super) fn broadcaster<R>(
    checkpoints: R,
    config: IngestionConfig,
    client: IngestionClient,
    mut ingest_hi_rx: mpsc::UnboundedReceiver<(&'static str, u64)>,
    subscribers: Vec<mpsc::Sender<Arc<CheckpointData>>>,
    cancel: CancellationToken,
) -> JoinHandle<()>
where
    R: std::ops::RangeBounds<u64> + Send + 'static,
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

        let client = Arc::new(client);
        let subscribers = Arc::new(subscribers);

        // Initialize shared watermark and ingest_hi states.
        // ingest_hi starts at max to allow for initial progress and may be adjusted
        // downwards based on subscriber feedback.
        let watermark = Arc::new(AtomicU64::new(start_cp));
        let ingest_hi = Arc::new(AtomicU64::new(u64::MAX));

        // Track subscriber watermarks
        let mut subscribers_hi = HashMap::<&'static str, u64>::new();

        // Helper to spawn a new broadcaster if possible.
        let maybe_spawn_new_broadcaster = || {
            let current_watermark = watermark.load(Ordering::Acquire);
            if current_watermark <= ingest_hi.load(Ordering::Acquire) {
                info!(
                    start = current_watermark,
                    end = end_cp,
                    "Spawning broadcaster for range"
                );
                Some(tokio::spawn(broadcast_range(
                    current_watermark,
                    end_cp, // Right now we always use the end of the range but may change later with streaming.
                    retry_interval,
                    concurrency,
                    ingest_hi.clone(),
                    watermark.clone(),
                    client.clone(),
                    subscribers.clone(),
                    cancel.clone(),
                )))
            } else {
                None
            }
        };

        // First start a broadcaster task.
        let mut broadcaster_handle = maybe_spawn_new_broadcaster();

        loop {
            tokio::select! {
                _ = cancel.cancelled() => {
                    info!("Shutdown received, stopping ingestion");
                    break;
                }

                // Subscriber watermark update
                // docs::#regulator (see docs/content/guides/developer/advanced/custom-indexer.mdx)
                Some((name, hi)) = ingest_hi_rx.recv() => {
                    subscribers_hi.insert(name, hi);

                    if let Some(min_hi) = subscribers_hi.values().copied().min() {
                        let new_ingest_hi = min_hi + buffer_size as u64;
                        ingest_hi.store(new_ingest_hi, Ordering::Release);
                        info!(ingest_hi = new_ingest_hi, "Updated ingest_hi");

                        // Try to spawn a new broadcaster if none is running
                        if broadcaster_handle.is_none() {
                            broadcaster_handle = maybe_spawn_new_broadcaster();
                        }
                    }
                }
                // docs::/#regulator

                // Handle broadcaster task completion
                join_result = async {
                    match &mut broadcaster_handle {
                        Some(handle) => Some(handle.await),
                        None => None,
                    }
                }, if broadcaster_handle.is_some() => {
                    if let Some(result) = join_result {
                        broadcaster_handle = None;

                        match result {
                            Ok(Ok(())) => {
                                info!("Broadcaster completed successfully");

                                // Check for completion of the entire range.
                                // In fact this is the only possible successful completion case.
                                let current_watermark = watermark.load(Ordering::Acquire);
                                if end_cp < current_watermark {
                                    info!("All checkpoints processed, shutting down broadcaster");
                                    break;
                                }

                                broadcaster_handle = maybe_spawn_new_broadcaster();
                            }
                            Ok(Err(Error::Backpressured(cp, watermark))) => {
                                info!(checkpoint = cp, watermark = watermark, "Broadcaster stopped due to backpressure");
                                broadcaster_handle = maybe_spawn_new_broadcaster();
                            }
                            Ok(Err(Error::Cancelled)) => {
                                info!("Broadcaster was cancelled");
                                break;
                            }
                            Ok(Err(e)) => {
                                error!("Broadcaster failed: {}", e);
                                cancel.cancel();
                                break;
                            }
                            Err(e) => {
                                error!("Broadcaster task panicked: {}", e);
                                cancel.cancel();
                                break;
                            }
                        }
                    }
                }
            }
        }

        // Wait for any active broadcaster to complete
        if let Some(handle) = broadcaster_handle {
            let _ = handle.await;
        }

        info!("Broadcaster finished");
    })
}

#[cfg(test)]
mod tests {
    use prometheus::Registry;
    use std::fmt::Debug;
    use std::sync::Arc;
    use std::time::Duration;
    use tokio::time::{error::Elapsed, timeout};

    use super::*;
    use crate::ingestion::client::FetchData;
    use crate::ingestion::{test_utils::test_checkpoint_data, IngestionConfig};
    use crate::metrics::IndexerMetrics;

    /// Create a mock IngestionClient for tests
    fn mock_client() -> IngestionClient {
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

        IngestionClient::new_impl(
            Arc::new(MockClient),
            IndexerMetrics::new(None, &Registry::new()),
        )
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

    #[tokio::test]
    async fn finite_list_of_checkpoints() {
        let (_, hi_rx) = mpsc::unbounded_channel();
        let (subscriber_tx, mut subscriber_rx) = mpsc::channel(1);
        let cancel = CancellationToken::new();

        let cps = 0..5;
        let h_broadcaster = broadcaster(
            cps,
            test_config(),
            mock_client(),
            hi_rx,
            vec![subscriber_tx],
            cancel.clone(),
        );

        for i in 0..5 {
            let checkpoint = expect_recv(&mut subscriber_rx).await.unwrap();
            assert_eq!(*checkpoint.checkpoint_summary.sequence_number(), i);
        }

        cancel.cancel();
        h_broadcaster.await.unwrap();
    }

    #[tokio::test]
    async fn shutdown_on_sender_closed() {
        let (_, hi_rx) = mpsc::unbounded_channel();
        let (subscriber_tx, mut subscriber_rx) = mpsc::channel(1);
        let cancel = CancellationToken::new();

        let h_broadcaster = broadcaster(
            0..,
            test_config(),
            mock_client(),
            hi_rx,
            vec![subscriber_tx],
            cancel.clone(),
        );

        for i in 0..5 {
            let checkpoint = expect_recv(&mut subscriber_rx).await.unwrap();
            assert_eq!(*checkpoint.checkpoint_summary.sequence_number(), i);
        }

        drop(subscriber_rx);
        h_broadcaster.await.unwrap();
    }

    #[tokio::test]
    async fn shutdown_on_cancel() {
        let (_, hi_rx) = mpsc::unbounded_channel();
        let (subscriber_tx, mut subscriber_rx) = mpsc::channel(1);
        let cancel = CancellationToken::new();

        let h_broadcaster = broadcaster(
            0..,
            test_config(),
            mock_client(),
            hi_rx,
            vec![subscriber_tx],
            cancel.clone(),
        );

        for i in 0..5 {
            let checkpoint = expect_recv(&mut subscriber_rx).await.unwrap();
            assert_eq!(*checkpoint.checkpoint_summary.sequence_number(), i);
        }

        cancel.cancel();
        h_broadcaster.await.unwrap();
    }

    #[tokio::test]
    async fn halted() {
        let (hi_tx, hi_rx) = mpsc::unbounded_channel();
        let (subscriber_tx, mut subscriber_rx) = mpsc::channel(1);
        let cancel = CancellationToken::new();

        hi_tx.send(("test", 4)).unwrap();

        let mut config = test_config();
        config.checkpoint_buffer_size = 0; // No buffer

        let h_broadcaster = broadcaster(
            0..,
            config,
            mock_client(),
            hi_rx,
            vec![subscriber_tx],
            cancel.clone(),
        );

        for i in 0..=4 {
            let checkpoint = expect_recv(&mut subscriber_rx).await.unwrap();
            assert_eq!(*checkpoint.checkpoint_summary.sequence_number(), i);
        }

        // Regulator stopped because of watermark.
        expect_timeout(&mut subscriber_rx).await;

        cancel.cancel();
        h_broadcaster.await.unwrap();
    }

    #[tokio::test]
    async fn halted_buffered() {
        let (hi_tx, hi_rx) = mpsc::unbounded_channel();
        let (subscriber_tx, mut subscriber_rx) = mpsc::channel(1);
        let cancel = CancellationToken::new();

        hi_tx.send(("test", 2)).unwrap();

        let mut config = test_config();
        config.checkpoint_buffer_size = 2; // Buffer of 2

        let h_broadcaster = broadcaster(
            0..,
            config,
            mock_client(),
            hi_rx,
            vec![subscriber_tx],
            cancel.clone(),
        );

        for i in 0..=4 {
            let checkpoint = expect_recv(&mut subscriber_rx).await.unwrap();
            assert_eq!(*checkpoint.checkpoint_summary.sequence_number(), i);
        }

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

        hi_tx.send(("test", 2)).unwrap();

        let mut config = test_config();
        config.checkpoint_buffer_size = 0; // No buffer

        let h_broadcaster = broadcaster(
            0..,
            config,
            mock_client(),
            hi_rx,
            vec![subscriber_tx],
            cancel.clone(),
        );

        for i in 0..=2 {
            let checkpoint = expect_recv(&mut subscriber_rx).await.unwrap();
            assert_eq!(*checkpoint.checkpoint_summary.sequence_number(), i);
        }

        // Regulator stopped because of watermark, but resumes when that watermark is updated.
        expect_timeout(&mut subscriber_rx).await;
        hi_tx.send(("test", 4)).unwrap();

        for i in 3..=4 {
            let checkpoint = expect_recv(&mut subscriber_rx).await.unwrap();
            assert_eq!(*checkpoint.checkpoint_summary.sequence_number(), i);
        }

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
        let h_broadcaster = broadcaster(
            cps,
            config,
            mock_client(),
            hi_rx,
            vec![subscriber_tx],
            cancel.clone(),
        );

        for i in 0..=2 {
            let checkpoint = expect_recv(&mut subscriber_rx).await.unwrap();
            assert_eq!(*checkpoint.checkpoint_summary.sequence_number(), i);
        }

        // Watermark stopped because of a's watermark.
        expect_timeout(&mut subscriber_rx).await;

        // Updating b's watermark doesn't make a difference.
        hi_tx.send(("b", 4)).unwrap();
        expect_timeout(&mut subscriber_rx).await;

        // But updating a's watermark does.
        hi_tx.send(("a", 3)).unwrap();
        let checkpoint = expect_recv(&mut subscriber_rx).await.unwrap();
        assert_eq!(*checkpoint.checkpoint_summary.sequence_number(), 3);

        // ...by one checkpoint.
        expect_timeout(&mut subscriber_rx).await;

        // And we can make more progress by updating it again.
        hi_tx.send(("a", 4)).unwrap();
        let checkpoint = expect_recv(&mut subscriber_rx).await.unwrap();
        assert_eq!(*checkpoint.checkpoint_summary.sequence_number(), 4);

        // But another update to "a" will now not make a difference, because "b" is still behind.
        hi_tx.send(("a", 5)).unwrap();
        expect_timeout(&mut subscriber_rx).await;

        cancel.cancel();
        h_broadcaster.await.unwrap();
    }
}
