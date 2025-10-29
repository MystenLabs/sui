// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{collections::HashMap, sync::Arc, time::Duration};

use futures::{future::try_join_all, stream};
use tokio::{
    sync::{mpsc, watch},
    task::JoinHandle,
};
use tokio_util::sync::CancellationToken;
use tracing::{debug, error, info};

use super::{IngestionConfig, client::IngestionClient};
use crate::{
    ingestion::error::Error, task::TrySpawnStreamExt,
    types::full_checkpoint_content::CheckpointData,
};

/// Broadcaster task that manages checkpoint flow and spawns broadcast tasks for ranges.
///
/// This task:
/// 1. Maintains an ingest_hi based on subscriber feedback
/// 2. Spawns a broadcaster task for the requested checkpoint range. Right now the entire requested range is given to
///    each broadcaster, but with streaming support the end of the range could be different.
/// 3. The broadcast_range task waits on the watch channel when it hits the ingest_hi limit
///
/// The task will shut down if the `cancel` token is signalled, or if the `checkpoints` range completes.
pub(super) fn broadcaster<R>(
    checkpoints: R,
    initial_commit_hi: Option<u64>,
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

        // Spawn the broadcaster task.
        let mut broadcaster_handle = tokio::spawn(broadcast_range(
            start_cp,
            end_cp,
            retry_interval,
            concurrency,
            ingest_hi_watch_rx.clone(),
            client.clone(),
            subscribers.clone(),
            cancel.clone(),
        ));

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
                        // Update the watch channel, which will notify all waiting tasks
                        let _ = ingest_hi_watch_tx.send(Some(new_ingest_hi));
                        debug!(ingest_hi = new_ingest_hi);
                    }
                }
                // docs::/#regulator

                // Handle broadcaster task completion
                result = &mut broadcaster_handle => {
                    match result {
                        Ok(Ok(())) => {
                            info!("Broadcaster completed successfully");
                            break;
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

        info!("Broadcaster finished");
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
                            debug!(checkpoint = cp, "Broadcasted checkpoint");
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
    use prometheus::Registry;
    use std::fmt::Debug;
    use std::sync::Arc;
    use std::time::Duration;
    use tokio::time::{error::Elapsed, timeout};

    use super::*;
    use crate::ingestion::client::FetchData;
    use crate::ingestion::{IngestionConfig, test_utils::test_checkpoint_data};
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

    #[tokio::test]
    async fn finite_list_of_checkpoints() {
        let (_, hi_rx) = mpsc::unbounded_channel();
        let (subscriber_tx, mut subscriber_rx) = mpsc::channel(1);
        let cancel = CancellationToken::new();

        let cps = 0..5;
        let h_broadcaster = broadcaster(
            cps,
            None,
            test_config(),
            mock_client(),
            hi_rx,
            vec![subscriber_tx],
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

        let h_broadcaster = broadcaster(
            0..,
            None,
            test_config(),
            mock_client(),
            hi_rx,
            vec![subscriber_tx],
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

        let h_broadcaster = broadcaster(
            0..,
            None,
            test_config(),
            mock_client(),
            hi_rx,
            vec![subscriber_tx],
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

        let h_broadcaster = broadcaster(
            0..,
            Some(4),
            config,
            mock_client(),
            hi_rx,
            vec![subscriber_tx],
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

        let h_broadcaster = broadcaster(
            0..,
            Some(2),
            config,
            mock_client(),
            hi_rx,
            vec![subscriber_tx],
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

        let h_broadcaster = broadcaster(
            0..,
            Some(2),
            config,
            mock_client(),
            hi_rx,
            vec![subscriber_tx],
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
        let h_broadcaster = broadcaster(
            cps,
            Some(2),
            config,
            mock_client(),
            hi_rx,
            vec![subscriber_tx],
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

        let h_broadcaster = broadcaster(
            0..,
            None,
            test_config(),
            mock_client(),
            hi_rx,
            vec![subscriber_tx1, subscriber_tx2],
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

        let h_broadcaster = broadcaster(
            1000..1010,
            Some(1005),
            config,
            mock_client(),
            hi_rx,
            vec![subscriber_tx],
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
}
