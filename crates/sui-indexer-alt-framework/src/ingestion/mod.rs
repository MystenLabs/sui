// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// Allow use of `unbounded_channel` in `ingestion` -- it is used by the regulator task to receive
// feedback. Traffic through this task should be minimal, but if a bound is applied to it and that
// bound is hit, the indexer could deadlock.
#![allow(clippy::disallowed_methods)]

use std::{path::PathBuf, sync::Arc, time::Duration};

use serde::{Deserialize, Serialize};
use sui_types::full_checkpoint_content::CheckpointData;
use tokio::{sync::mpsc, task::JoinHandle};
use tokio_util::sync::CancellationToken;
use url::Url;

use crate::ingestion::broadcaster::broadcaster;
use crate::ingestion::client::IngestionClient;
use crate::ingestion::error::{Error, Result};
use crate::ingestion::regulator::regulator;
use crate::metrics::IndexerMetrics;

mod broadcaster;
pub mod client;
pub mod error;
mod local_client;
mod regulator;
mod remote_client;
#[cfg(test)]
mod test_utils;

#[derive(clap::Args, Clone, Debug)]
pub struct ClientArgs {
    /// Remote Store to fetch checkpoints from.
    #[clap(long, required = true, group = "source")]
    pub remote_store_url: Option<Url>,

    /// Path to the local ingestion directory.
    /// If both remote_store_url and local_ingestion_path are provided, remote_store_url will be used.
    #[clap(long, required = true, group = "source")]
    pub local_ingestion_path: Option<PathBuf>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct IngestionConfig {
    /// Maximum size of checkpoint backlog across all workers downstream of the ingestion service.
    pub checkpoint_buffer_size: usize,

    /// Maximum number of checkpoints to attempt to fetch concurrently.
    pub ingest_concurrency: usize,

    /// Polling interval to retry fetching checkpoints that do not exist, in milliseconds.
    pub retry_interval_ms: u64,
}

pub(crate) struct IngestionService {
    config: IngestionConfig,
    client: IngestionClient,
    ingest_hi_tx: mpsc::UnboundedSender<(&'static str, u64)>,
    ingest_hi_rx: mpsc::UnboundedReceiver<(&'static str, u64)>,
    subscribers: Vec<mpsc::Sender<Arc<CheckpointData>>>,
    cancel: CancellationToken,
}

impl IngestionConfig {
    pub fn retry_interval(&self) -> Duration {
        Duration::from_millis(self.retry_interval_ms)
    }
}

impl IngestionService {
    /// TODO: If we want to expose this as part of the framework, so people can run just an
    /// ingestion service, we will need to split `IngestionMetrics` out from `IndexerMetrics`.
    pub(crate) fn new(
        args: ClientArgs,
        config: IngestionConfig,
        metrics: Arc<IndexerMetrics>,
        cancel: CancellationToken,
    ) -> Result<Self> {
        // TODO: Potentially support a hybrid mode where we can fetch from both local and remote.
        let client = if let Some(url) = args.remote_store_url.as_ref() {
            IngestionClient::new_remote(url.clone(), metrics.clone())?
        } else if let Some(path) = args.local_ingestion_path.as_ref() {
            IngestionClient::new_local(path.clone(), metrics.clone())
        } else {
            panic!("Either remote_store_url or local_ingestion_path must be provided");
        };

        let subscribers = Vec::new();
        let (ingest_hi_tx, ingest_hi_rx) = mpsc::unbounded_channel();
        Ok(Self {
            config,
            client,
            ingest_hi_tx,
            ingest_hi_rx,
            subscribers,
            cancel,
        })
    }

    /// The client this service uses to fetch checkpoints.
    pub(crate) fn client(&self) -> &IngestionClient {
        &self.client
    }

    /// Add a new subscription to the ingestion service. Note that the service is susceptible to
    /// the "slow receiver" problem: If one receiver is slower to process checkpoints than the
    /// checkpoint ingestion rate, it will eventually hold up all receivers.
    ///
    /// The ingestion service can optionally receive checkpoint high watermarks from its
    /// subscribers. If a subscriber provides a watermark, the ingestion service will commit to not
    /// run ahead of the watermark by more than the config's buffer_size.
    ///
    /// Returns the channel to receive checkpoints from and the channel to accept watermarks from.
    pub(crate) fn subscribe(
        &mut self,
    ) -> (
        mpsc::Receiver<Arc<CheckpointData>>,
        mpsc::UnboundedSender<(&'static str, u64)>,
    ) {
        let (sender, receiver) = mpsc::channel(self.config.checkpoint_buffer_size);
        self.subscribers.push(sender);
        (receiver, self.ingest_hi_tx.clone())
    }

    /// Start the ingestion service as a background task, consuming it in the process.
    ///
    /// Checkpoints are fetched concurrently from the `checkpoints` iterator, and pushed to
    /// subscribers' channels (potentially out-of-order). Subscribers can communicate with the
    /// ingestion service via their channels in the following ways:
    ///
    /// - If a subscriber is lagging (not receiving checkpoints fast enough), it will eventually
    ///   provide back-pressure to the ingestion service, which will stop fetching new checkpoints.
    /// - If a subscriber closes its channel, the ingestion service will interpret that as a signal
    ///   to shutdown as well.
    ///
    /// If ingestion reaches the leading edge of the network, it will encounter checkpoints that do
    /// not exist yet. These will be retried repeatedly on a fixed `retry_interval` until they
    /// become available.
    pub(crate) async fn run<I>(self, checkpoints: I) -> Result<(JoinHandle<()>, JoinHandle<()>)>
    where
        I: IntoIterator<Item = u64> + Send + Sync + 'static,
        I::IntoIter: Send + Sync + 'static,
    {
        let IngestionService {
            config,
            client,
            ingest_hi_tx: _,
            ingest_hi_rx,
            subscribers,
            cancel,
        } = self;

        if subscribers.is_empty() {
            return Err(Error::NoSubscribers);
        }

        let (checkpoint_tx, checkpoint_rx) = mpsc::channel(config.ingest_concurrency);

        let regulator = regulator(
            checkpoints,
            config.checkpoint_buffer_size,
            ingest_hi_rx,
            checkpoint_tx,
            cancel.clone(),
        );

        let broadcaster = broadcaster(config, client, checkpoint_rx, subscribers, cancel.clone());

        Ok((regulator, broadcaster))
    }
}

impl Default for IngestionConfig {
    fn default() -> Self {
        Self {
            checkpoint_buffer_size: 5000,
            ingest_concurrency: 200,
            retry_interval_ms: 200,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use reqwest::StatusCode;
    use wiremock::{MockServer, Request};

    use crate::ingestion::remote_client::tests::{respond_with, status};
    use crate::ingestion::test_utils::test_checkpoint_data;
    use crate::metrics::tests::test_metrics;

    use super::*;

    async fn test_ingestion(
        uri: String,
        checkpoint_buffer_size: usize,
        ingest_concurrency: usize,
        cancel: CancellationToken,
    ) -> IngestionService {
        IngestionService::new(
            ClientArgs {
                remote_store_url: Some(Url::parse(&uri).unwrap()),
                local_ingestion_path: None,
            },
            IngestionConfig {
                checkpoint_buffer_size,
                ingest_concurrency,
                ..Default::default()
            },
            test_metrics(),
            cancel,
        )
        .unwrap()
    }

    async fn test_subscriber(
        stop_after: usize,
        mut rx: mpsc::Receiver<Arc<CheckpointData>>,
        cancel: CancellationToken,
    ) -> JoinHandle<Vec<u64>> {
        tokio::spawn(async move {
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

        let (rx, _) = ingestion_service.subscribe();
        let subscriber = test_subscriber(usize::MAX, rx, cancel.clone()).await;
        let (regulator, broadcaster) = ingestion_service.run(0..).await.unwrap();

        cancel.cancel();
        subscriber.await.unwrap();
        regulator.await.unwrap();
        broadcaster.await.unwrap();
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

        let (rx, _) = ingestion_service.subscribe();
        let subscriber = test_subscriber(1, rx, cancel.clone()).await;
        let (regulator, broadcaster) = ingestion_service.run(0..).await.unwrap();

        cancel.cancelled().await;
        subscriber.await.unwrap();
        regulator.await.unwrap();
        broadcaster.await.unwrap();
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

        let (rx, _) = ingestion_service.subscribe();
        let subscriber = test_subscriber(usize::MAX, rx, cancel.clone()).await;
        let (regulator, broadcaster) = ingestion_service.run(0..).await.unwrap();

        cancel.cancelled().await;
        subscriber.await.unwrap();
        regulator.await.unwrap();
        broadcaster.await.unwrap();
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

        let (rx, _) = ingestion_service.subscribe();
        let subscriber = test_subscriber(5, rx, cancel.clone()).await;
        let (regulator, broadcaster) = ingestion_service.run(0..).await.unwrap();

        cancel.cancelled().await;
        let seqs = subscriber.await.unwrap();
        regulator.await.unwrap();
        broadcaster.await.unwrap();

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

        let (rx, _) = ingestion_service.subscribe();
        let subscriber = test_subscriber(5, rx, cancel.clone()).await;
        let (regulator, broadcaster) = ingestion_service.run(0..).await.unwrap();

        cancel.cancelled().await;
        let seqs = subscriber.await.unwrap();
        regulator.await.unwrap();
        broadcaster.await.unwrap();

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
        let (mut laggard, _) = ingestion_service.subscribe();
        async fn unblock(laggard: &mut mpsc::Receiver<Arc<CheckpointData>>) -> u64 {
            let checkpoint = laggard.recv().await.unwrap();
            checkpoint.checkpoint_summary.sequence_number
        }

        let (rx, _) = ingestion_service.subscribe();
        let subscriber = test_subscriber(5, rx, cancel.clone()).await;
        let (regulator, broadcaster) = ingestion_service.run(0..).await.unwrap();

        // At this point, the service will have been able to pass 3 checkpoints to the non-lagging
        // subscriber, while the laggard's buffer fills up. Now the laggard will pull two
        // checkpoints, which will allow the rest of the pipeline to progress enough for the live
        // subscriber to receive its quota.
        assert_eq!(unblock(&mut laggard).await, 1);
        assert_eq!(unblock(&mut laggard).await, 2);

        cancel.cancelled().await;
        let seqs = subscriber.await.unwrap();
        regulator.await.unwrap();
        broadcaster.await.unwrap();

        assert_eq!(seqs, vec![1, 2, 3, 4, 5]);
    }
}
