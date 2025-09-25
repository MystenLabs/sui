// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use futures::future::try_join_all;
use futures::stream;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::{sync::mpsc, task::JoinHandle};
use tokio_util::sync::CancellationToken;
use tracing::{error, info};

use crate::{
    ingestion::error::Error, task::TrySpawnStreamExt,
    types::full_checkpoint_content::CheckpointData,
};

use super::{client::IngestionClient, IngestionConfig};

/// Combined regulator and broadcaster task that manages checkpoint flow and distribution.
///
/// This task:
/// 1. Regulates checkpoint flow based on subscriber watermarks (back-pressure)
/// 2. Fetches checkpoint data from the client
/// 3. Broadcasts fetched data to all subscribers
///
/// The task will shut down if the `cancel` token is signalled, or if the `checkpoints` iterator
/// runs out.
pub(super) fn regulated_broadcaster<I>(
    checkpoints: I,
    config: IngestionConfig,
    client: IngestionClient,
    ingest_hi_rx: mpsc::UnboundedReceiver<(String, u64)>,
    subscribers: Vec<mpsc::Sender<Arc<CheckpointData>>>,
    cancel: CancellationToken,
) -> JoinHandle<()>
where
    I: IntoIterator<Item = u64> + Send + Sync + 'static,
    I::IntoIter: Send + Sync + 'static,
{
    tokio::spawn(async move {
        info!("Starting regulated broadcaster");

        let buffer_size = config.checkpoint_buffer_size;
        let retry_interval = config.retry_interval();
        let checkpoints = checkpoints.into_iter().peekable();

        let ingest_hi = None::<u64>;
        let subscribers_hi = HashMap::<String, u64>::new();

        // Create checkpoint stream with watermark regulation
        let checkpoint_stream = stream::unfold(
            (
                checkpoints,
                ingest_hi_rx,
                ingest_hi,
                subscribers_hi,
                cancel.clone(),
            ),
            move |(
                mut checkpoints,
                mut ingest_hi_rx,
                mut ingest_hi,
                mut subscribers_hi,
                cancel,
            )| async move {
                // Get next checkpoint or exit if done
                let cp = loop {
                    if cancel.is_cancelled() {
                        return None;
                    }

                    let &cp = checkpoints.peek()?;

                    // Process any pending watermark updates
                    while let Ok((name, hi)) = ingest_hi_rx.try_recv() {
                        subscribers_hi.insert(name, hi);
                        ingest_hi = subscribers_hi
                            .values()
                            .copied()
                            .min()
                            .map(|hi| hi + buffer_size as u64);
                    }

                    if ingest_hi.is_none_or(|hi| cp <= hi) {
                        break cp;
                    }

                    // If we get to here, it means we are backpressured so we wait for watermark update before proceeding
                    tokio::select! {
                        _ = cancel.cancelled() => {
                            return None;
                        }
                        Some((name, hi)) = ingest_hi_rx.recv() => {
                            subscribers_hi.insert(name, hi);
                            ingest_hi = subscribers_hi.values().copied().min().map(|hi| hi + buffer_size as u64);
                        }
                    }
                };

                checkpoints.next();
                Some((
                    cp,
                    (checkpoints, ingest_hi_rx, ingest_hi, subscribers_hi, cancel),
                ))
            },
        );

        // Fetch and broadcast each checkpoint from the regulated stream.
        match checkpoint_stream
            .try_for_each_spawned(config.ingest_concurrency, |cp| {
                let client = client.clone();
                let subscribers = subscribers.clone();
                let supervisor_cancel = cancel.clone();
                let cancel = cancel.clone();

                async move {
                    let checkpoint = tokio::select! {
                        cp = client.wait_for(cp, retry_interval) => cp?,
                        _ = cancel.cancelled() => {
                            return Err(Error::Cancelled);
                        }
                    };

                    // Make the send operation cancellable as well.
                    let send_fut = async {
                        let futures = subscribers.iter().map(|s| s.send(checkpoint.clone()));
                        try_join_all(futures).await
                    };

                    tokio::select! {
                        result = send_fut => {
                            if result.is_err() {
                                info!("Subscription dropped, signalling shutdown");
                                supervisor_cancel.cancel();
                                Err(Error::Cancelled)
                            } else {
                                Ok(())
                            }
                        }
                        _ = cancel.cancelled() => {
                            Err(Error::Cancelled)
                        }
                    }
                }
            })
            .await
        {
            Ok(()) => {
                info!("Checkpoints done, stopping ingestion");
            }
            Err(Error::Cancelled) => {
                info!("Shutdown received, stopping ingestion");
            }
            Err(e) => {
                error!("Ingestion failed: {}", e);
                cancel.cancel();
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use std::fmt::Debug;
    use std::sync::Arc;
    use std::time::Duration;
    use tokio::time::{error::Elapsed, timeout};
    // use sui_types::storage::blob::Blob;

    use super::*;
    use crate::ingestion::client::FetchData;
    use crate::ingestion::{test_utils::test_checkpoint_data, IngestionConfig};

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

        IngestionClient::new_for_testing(Arc::new(MockClient))
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
        let h_regulator = regulated_broadcaster(
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
        h_regulator.await.unwrap();
    }

    #[tokio::test]
    async fn shutdown_on_sender_closed() {
        let (_, hi_rx) = mpsc::unbounded_channel();
        let (subscriber_tx, mut subscriber_rx) = mpsc::channel(1);
        let cancel = CancellationToken::new();

        let h_regulator = regulated_broadcaster(
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
        h_regulator.await.unwrap();
    }

    #[tokio::test]
    async fn shutdown_on_cancel() {
        let (_, hi_rx) = mpsc::unbounded_channel();
        let (subscriber_tx, mut subscriber_rx) = mpsc::channel(1);
        let cancel = CancellationToken::new();

        let h_regulator = regulated_broadcaster(
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
        h_regulator.await.unwrap();
    }

    #[tokio::test]
    async fn halted() {
        let (hi_tx, hi_rx) = mpsc::unbounded_channel();
        let (subscriber_tx, mut subscriber_rx) = mpsc::channel(1);
        let cancel = CancellationToken::new();

        hi_tx.send(("test".to_string(), 4)).unwrap();

        let mut config = test_config();
        config.checkpoint_buffer_size = 0; // No buffer

        let h_regulator = regulated_broadcaster(
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
        h_regulator.await.unwrap();
    }

    #[tokio::test]
    async fn halted_buffered() {
        let (hi_tx, hi_rx) = mpsc::unbounded_channel();
        let (subscriber_tx, mut subscriber_rx) = mpsc::channel(1);
        let cancel = CancellationToken::new();

        hi_tx.send(("test".to_string(), 2)).unwrap();

        let mut config = test_config();
        config.checkpoint_buffer_size = 2; // Buffer of 2

        let h_regulator = regulated_broadcaster(
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
        h_regulator.await.unwrap();
    }

    #[tokio::test]
    async fn resumption() {
        let (hi_tx, hi_rx) = mpsc::unbounded_channel();
        let (subscriber_tx, mut subscriber_rx) = mpsc::channel(1);
        let cancel = CancellationToken::new();

        hi_tx.send(("test".to_string(), 2)).unwrap();

        let mut config = test_config();
        config.checkpoint_buffer_size = 0; // No buffer

        let h_regulator = regulated_broadcaster(
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
        hi_tx.send(("test".to_string(), 4)).unwrap();

        for i in 3..=4 {
            let checkpoint = expect_recv(&mut subscriber_rx).await.unwrap();
            assert_eq!(*checkpoint.checkpoint_summary.sequence_number(), i);
        }

        // Halted again.
        expect_timeout(&mut subscriber_rx).await;

        cancel.cancel();
        h_regulator.await.unwrap();
    }

    #[tokio::test]
    async fn multiple_subscribers() {
        let (hi_tx, hi_rx) = mpsc::unbounded_channel();
        let (subscriber_tx, mut subscriber_rx) = mpsc::channel(1);
        let cancel = CancellationToken::new();

        hi_tx.send(("a".to_string(), 2)).unwrap();
        hi_tx.send(("b".to_string(), 3)).unwrap();

        let mut config = test_config();
        config.checkpoint_buffer_size = 0; // No buffer

        let cps = 0..10;
        let h_regulator = regulated_broadcaster(
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
        hi_tx.send(("b".to_string(), 4)).unwrap();
        expect_timeout(&mut subscriber_rx).await;

        // But updating a's watermark does.
        hi_tx.send(("a".to_string(), 3)).unwrap();
        let checkpoint = expect_recv(&mut subscriber_rx).await.unwrap();
        assert_eq!(*checkpoint.checkpoint_summary.sequence_number(), 3);

        // ...by one checkpoint.
        expect_timeout(&mut subscriber_rx).await;

        // And we can make more progress by updating it again.
        hi_tx.send(("a".to_string(), 4)).unwrap();
        let checkpoint = expect_recv(&mut subscriber_rx).await.unwrap();
        assert_eq!(*checkpoint.checkpoint_summary.sequence_number(), 4);

        // But another update to "a" will now not make a difference, because "b" is still behind.
        hi_tx.send(("a".to_string(), 5)).unwrap();
        expect_timeout(&mut subscriber_rx).await;

        cancel.cancel();
        h_regulator.await.unwrap();
    }
}
