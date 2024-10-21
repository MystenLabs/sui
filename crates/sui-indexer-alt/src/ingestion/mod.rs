// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{sync::Arc, time::Duration};

use backoff::backoff::Constant;
use client::IngestionClient;
use futures::{future::try_join_all, stream, StreamExt, TryStreamExt};
use mysten_metrics::spawn_monitored_task;
use sui_types::full_checkpoint_content::CheckpointData;
use tokio::{sync::mpsc, task::JoinHandle};
use tokio_util::sync::CancellationToken;
use tracing::{debug, error, info};
use url::Url;

use crate::ingestion::error::{Error, Result};
use crate::metrics::IndexerMetrics;

mod client;
pub mod error;

pub struct IngestionService {
    config: IngestionConfig,
    client: IngestionClient,
    metrics: Arc<IndexerMetrics>,
    subscribers: Vec<mpsc::Sender<Arc<CheckpointData>>>,
    cancel: CancellationToken,
}

#[derive(clap::Args, Debug, Clone)]
pub struct IngestionConfig {
    /// Remote Store to fetch checkpoints from.
    #[arg(long)]
    remote_store_url: Url,

    /// Maximum size of checkpoint backlog across all workers downstream of the ingestion service.
    #[arg(long, default_value_t = 5000)]
    buffer_size: usize,

    /// Maximum number of checkpoint to attempt to fetch concurrently.
    #[arg(long, default_value_t = 200)]
    concurrency: usize,

    /// Polling interval to retry fetching checkpoints that do not exist.
    #[arg(
        long,
        default_value = "200",
        value_name = "MILLISECONDS",
        value_parser = |s: &str| s.parse().map(Duration::from_millis)
    )]
    retry_interval: Duration,
}

impl IngestionService {
    pub fn new(
        config: IngestionConfig,
        metrics: Arc<IndexerMetrics>,
        cancel: CancellationToken,
    ) -> Result<Self> {
        Ok(Self {
            client: IngestionClient::new(config.remote_store_url.clone(), metrics.clone())?,
            subscribers: Vec::new(),
            config,
            metrics,
            cancel,
        })
    }

    /// Add a new subscription to the ingestion service. Note that the service is susceptible to
    /// the "slow receiver" problem: If one receiver is slower to process checkpoints than the
    /// checkpoint ingestion rate, it will eventually hold up all receivers.
    pub fn subscribe(&mut self) -> mpsc::Receiver<Arc<CheckpointData>> {
        let (sender, receiver) = mpsc::channel(self.config.buffer_size);
        self.subscribers.push(sender);
        receiver
    }

    /// Start the ingestion service as a background task, consuming it in the process.
    ///
    /// Checkpoints are fetched concurrently from the `checkpoints` iterator, and pushed to
    /// subscribers' channels (potentially out-of-order). Subscribers can communicate with the
    /// ingestion service via their channels in the following ways:
    ///
    /// - If a subscriber is lagging (not receiving checkpoints fast enough), it will eventually
    ///   provide back-pressure to the ingestion service, which will stop fetching new checkpoints.
    /// - If a subscriber closes its channel, the ingestion service will intepret that as a signal
    ///   to shutdown as well.
    ///
    /// If ingestion reaches the leading edge of the network, it will encounter checkpoints that do
    /// not exist yet. These will be retried repeatedly on a fixed `retry_interval` until they
    /// become available.
    pub async fn run<I>(self, checkpoints: I) -> Result<JoinHandle<()>>
    where
        I: IntoIterator<Item = u64> + Send + Sync + 'static,
        I::IntoIter: Send + Sync + 'static,
    {
        let IngestionService {
            config,
            client,
            metrics,
            subscribers,
            cancel,
        } = self;

        if subscribers.is_empty() {
            return Err(Error::NoSubscribers);
        }

        Ok(spawn_monitored_task!(async move {
            info!("Starting ingestion service");

            match stream::iter(checkpoints)
                .map(Ok)
                .try_for_each_concurrent(/* limit */ config.concurrency, |cp| {
                    let client = client.clone();
                    let metrics = metrics.clone();
                    let subscribers = subscribers.clone();

                    // One clone is for the supervisor to signal a cancel if it detects a
                    // subscriber that wants to wind down ingestion, and the other is to pass to
                    // each worker to detect cancellation.
                    let supervisor_cancel = cancel.clone();
                    let cancel = cancel.clone();

                    // Repeatedly retry if the checkpoint is not found, assuming that we are at the
                    // tip of the network and it will become available soon.
                    let backoff = Constant::new(config.retry_interval);
                    let fetch = move || {
                        let client = client.clone();
                        let metrics = metrics.clone();
                        let cancel = cancel.clone();

                        async move {
                            use backoff::Error as BE;
                            if cancel.is_cancelled() {
                                return Err(BE::permanent(Error::Cancelled));
                            }

                            client.fetch(cp, &cancel).await.map_err(|e| match e {
                                Error::NotFound(checkpoint) => {
                                    debug!(checkpoint, "Checkpoint not found, retrying...");
                                    metrics.total_ingested_not_found_retries.inc();
                                    BE::transient(e)
                                }
                                e => BE::permanent(e),
                            })
                        }
                    };

                    async move {
                        let checkpoint = backoff::future::retry(backoff, fetch).await?;
                        let futures = subscribers.iter().map(|s| s.send(checkpoint.clone()));

                        if try_join_all(futures).await.is_err() {
                            info!("Subscription dropped, signalling shutdown");
                            supervisor_cancel.cancel();
                            Err(Error::Cancelled)
                        } else {
                            Ok(())
                        }
                    }
                })
                .await
            {
                Ok(()) => {
                    info!("Checkpoints done, stopping ingestion service");
                    // drop(subscribers);
                }

                Err(Error::Cancelled) => {
                    info!("Shutdown received, stopping ingestion service");
                }

                Err(e) => {
                    error!("Ingestion service failed: {}", e);
                    cancel.cancel();
                }
            }
        }))
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use reqwest::StatusCode;
    use wiremock::{MockServer, Request};

    use crate::ingestion::client::tests::{respond_with, status, test_checkpoint_data};
    use crate::metrics::tests::test_metrics;

    use super::*;

    async fn test_ingestion(
        uri: String,
        buffer_size: usize,
        concurrency: usize,
        cancel: CancellationToken,
    ) -> IngestionService {
        IngestionService::new(
            IngestionConfig {
                remote_store_url: Url::parse(&uri).unwrap(),
                buffer_size,
                concurrency,
                retry_interval: Duration::from_millis(200),
            },
            Arc::new(test_metrics()),
            cancel,
        )
        .unwrap()
    }

    async fn test_subscriber(
        stop_after: usize,
        mut rx: mpsc::Receiver<Arc<CheckpointData>>,
        cancel: CancellationToken,
    ) -> JoinHandle<Vec<u64>> {
        spawn_monitored_task!(async move {
            let mut seqs = vec![];
            for _ in 0..stop_after {
                tokio::select! {
                    _ = cancel.cancelled() => break,
                    Some(checkpoint) = rx.recv() => {
                        seqs.push(checkpoint.checkpoint_summary.sequence_number);
                    }
                }
            }

            rx.close();
            seqs
        })
    }

    /// If the ingestion service has no subscribers, it will fail fast (before fetching any
    /// checkpoints).
    #[tokio::test]
    async fn fail_on_no_subscribers() {
        telemetry_subscribers::init_for_testing();

        // The mock server will repeatedly return 404, so if the service does try to fetch a
        // checkpoint, it will be stuck repeatedly retrying.
        let server = MockServer::start().await;
        respond_with(&server, status(StatusCode::NOT_FOUND)).await;

        let cancel = CancellationToken::new();
        let ingestion_service = test_ingestion(server.uri(), 1, 1, cancel.clone()).await;

        let err = ingestion_service.run(0..).await.unwrap_err();
        assert!(matches!(err, Error::NoSubscribers));
    }

    /// The subscriber has no effective limit, and the mock server will always return checkpoint
    /// information, but the ingestion service can still be stopped using the cancellation token.
    #[tokio::test]
    async fn shutdown_on_cancel() {
        telemetry_subscribers::init_for_testing();

        let server = MockServer::start().await;
        respond_with(
            &server,
            status(StatusCode::OK).set_body_bytes(test_checkpoint_data(42)),
        )
        .await;

        let cancel = CancellationToken::new();
        let mut ingestion_service = test_ingestion(server.uri(), 1, 1, cancel.clone()).await;

        let rx = ingestion_service.subscribe();
        let subscriber_handle = test_subscriber(usize::MAX, rx, cancel.clone()).await;
        let ingestion_handle = ingestion_service.run(0..).await.unwrap();

        cancel.cancel();
        subscriber_handle.await.unwrap();
        ingestion_handle.await.unwrap();
    }

    /// The subscriber will stop after receiving a single checkpoint, and this will trigger the
    /// ingestion service to stop as well, even if there are more checkpoints to fetch.
    #[tokio::test]
    async fn shutdown_on_subscriber_drop() {
        telemetry_subscribers::init_for_testing();

        let server = MockServer::start().await;
        respond_with(
            &server,
            status(StatusCode::OK).set_body_bytes(test_checkpoint_data(42)),
        )
        .await;

        let cancel = CancellationToken::new();
        let mut ingestion_service = test_ingestion(server.uri(), 1, 1, cancel.clone()).await;

        let rx = ingestion_service.subscribe();
        let subscriber_handle = test_subscriber(1, rx, cancel.clone()).await;
        let ingestion_handle = ingestion_service.run(0..).await.unwrap();

        cancel.cancelled().await;
        subscriber_handle.await.unwrap();
        ingestion_handle.await.unwrap();
    }

    /// If fetching the checkpoint throws an unexpected error, the whole pipeline will be shut
    /// down.
    #[tokio::test]
    async fn shutdown_on_unexpected_error() {
        telemetry_subscribers::init_for_testing();

        let server = MockServer::start().await;
        respond_with(&server, status(StatusCode::IM_A_TEAPOT)).await;

        let cancel = CancellationToken::new();
        let mut ingestion_service = test_ingestion(server.uri(), 1, 1, cancel.clone()).await;

        let rx = ingestion_service.subscribe();
        let subscriber_handle = test_subscriber(usize::MAX, rx, cancel.clone()).await;
        let ingestion_handle = ingestion_service.run(0..).await.unwrap();

        cancel.cancelled().await;
        subscriber_handle.await.unwrap();
        ingestion_handle.await.unwrap();
    }

    /// The service will retry fetching a checkpoint that does not exist, in this test, the 4th
    /// checkpoint will return 404 a couple of times, before eventually succeeding.
    #[tokio::test]
    async fn retry_on_not_found() {
        telemetry_subscribers::init_for_testing();

        let server = MockServer::start().await;
        let times: Mutex<u64> = Mutex::new(0);
        respond_with(&server, move |_: &Request| {
            let mut times = times.lock().unwrap();
            *times += 1;
            match *times {
                1..4 => status(StatusCode::OK).set_body_bytes(test_checkpoint_data(*times)),
                4..6 => status(StatusCode::NOT_FOUND),
                _ => status(StatusCode::OK).set_body_bytes(test_checkpoint_data(*times)),
            }
        })
        .await;

        let cancel = CancellationToken::new();
        let mut ingestion_service = test_ingestion(server.uri(), 1, 1, cancel.clone()).await;

        let rx = ingestion_service.subscribe();
        let subscriber_handle = test_subscriber(5, rx, cancel.clone()).await;
        let ingestion_handle = ingestion_service.run(0..).await.unwrap();

        cancel.cancelled().await;
        let seqs = subscriber_handle.await.unwrap();
        ingestion_handle.await.unwrap();

        assert_eq!(seqs, vec![1, 2, 3, 6, 7]);
    }

    /// Similar to the previous test, but now it's a transient error that causes the retry.
    #[tokio::test]
    async fn retry_on_transient_error() {
        telemetry_subscribers::init_for_testing();

        let server = MockServer::start().await;
        let times: Mutex<u64> = Mutex::new(0);
        respond_with(&server, move |_: &Request| {
            let mut times = times.lock().unwrap();
            *times += 1;
            match *times {
                1..4 => status(StatusCode::OK).set_body_bytes(test_checkpoint_data(*times)),
                4..6 => status(StatusCode::REQUEST_TIMEOUT),
                _ => status(StatusCode::OK).set_body_bytes(test_checkpoint_data(*times)),
            }
        })
        .await;

        let cancel = CancellationToken::new();
        let mut ingestion_service = test_ingestion(server.uri(), 1, 1, cancel.clone()).await;

        let rx = ingestion_service.subscribe();
        let subscriber_handle = test_subscriber(5, rx, cancel.clone()).await;
        let ingestion_handle = ingestion_service.run(0..).await.unwrap();

        cancel.cancelled().await;
        let seqs = subscriber_handle.await.unwrap();
        ingestion_handle.await.unwrap();

        assert_eq!(seqs, vec![1, 2, 3, 6, 7]);
    }

    /// One subscriber is going to stop processing checkpoints, so even though the service can keep
    /// fetching checkpoints, it will stop short because of the slow receiver. Other subscribers
    /// can keep processing checkpoints that were buffered for the slow one.
    #[tokio::test]
    async fn back_pressure_and_buffering() {
        telemetry_subscribers::init_for_testing();

        let server = MockServer::start().await;
        let times: Mutex<u64> = Mutex::new(0);
        respond_with(&server, move |_: &Request| {
            let mut times = times.lock().unwrap();
            *times += 1;
            status(StatusCode::OK).set_body_bytes(test_checkpoint_data(*times))
        })
        .await;

        let cancel = CancellationToken::new();
        let mut ingestion_service =
            test_ingestion(server.uri(), /* buffer */ 3, 1, cancel.clone()).await;

        // This subscriber will take its sweet time processing checkpoints.
        let mut laggard = ingestion_service.subscribe();
        async fn unblock(laggard: &mut mpsc::Receiver<Arc<CheckpointData>>) -> u64 {
            let checkpoint = laggard.recv().await.unwrap();
            checkpoint.checkpoint_summary.sequence_number
        }

        let rx = ingestion_service.subscribe();
        let subscriber_handle = test_subscriber(5, rx, cancel.clone()).await;
        let ingestion_handle = ingestion_service.run(0..).await.unwrap();

        // At this point, the service will have been able to pass 3 checkpoints to the non-lagging
        // subscriber, while the laggard's buffer fills up. Now the laggard will pull two
        // checkpoints, which will allow the rest of the pipeline to progress enough for the live
        // subscriber to receive its quota.
        assert_eq!(unblock(&mut laggard).await, 1);
        assert_eq!(unblock(&mut laggard).await, 2);

        cancel.cancelled().await;
        let seqs = subscriber_handle.await.unwrap();
        ingestion_handle.await.unwrap();

        assert_eq!(seqs, vec![1, 2, 3, 4, 5]);
    }
}
