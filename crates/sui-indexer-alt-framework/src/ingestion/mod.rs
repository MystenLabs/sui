// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;
use std::time::Duration;

use prometheus::Registry;
use serde::Deserialize;
use serde::Serialize;
use sui_futures::service::Service;
use tokio::sync::mpsc;
use tracing::warn;

pub use crate::config::ConcurrencyConfig as IngestConcurrencyConfig;
use crate::ingestion::broadcaster::broadcaster;
use crate::ingestion::error::Error;
use crate::ingestion::error::Result;
use crate::ingestion::ingestion_client::CheckpointEnvelope;
use crate::ingestion::ingestion_client::IngestionClient;
use crate::ingestion::ingestion_client::IngestionClientArgs;
use crate::ingestion::ingestion_client::retry_transient_with_slow_monitor;
use crate::ingestion::streaming_client::CheckpointStreamingClient;
use crate::ingestion::streaming_client::GrpcStreamingClient;
use crate::ingestion::streaming_client::StreamingClientArgs;
use crate::metrics::IngestionMetrics;

mod broadcaster;
mod byte_count;
pub(crate) mod decode;
pub mod error;
pub mod ingestion_client;
mod rpc_client;
pub mod store_client;
pub mod streaming_client;
#[cfg(test)]
mod test_utils;

pub(crate) const MAX_GRPC_MESSAGE_SIZE_BYTES: usize = 128 * 1024 * 1024;

/// Combined arguments for both ingestion and streaming clients.
/// This is a convenience wrapper that flattens both argument types.
#[derive(clap::Args, Clone, Debug, Default)]
pub struct ClientArgs {
    #[clap(flatten)]
    pub ingestion: IngestionClientArgs,

    #[clap(flatten)]
    pub streaming: StreamingClientArgs,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct IngestionConfig {
    /// Concurrency control for checkpoint ingestion. A plain integer gives fixed concurrency;
    /// an object with `initial`, `min`, and `max` fields enables adaptive concurrency that adjusts
    /// based on subscriber channel fill fraction.
    pub ingest_concurrency: IngestConcurrencyConfig,

    /// Polling interval to retry fetching checkpoints that do not exist, in milliseconds.
    pub retry_interval_ms: u64,

    /// Initial number of checkpoints to process using ingestion after a streaming connection failure.
    pub streaming_backoff_initial_batch_size: usize,

    /// Maximum number of checkpoints to process using ingestion after repeated streaming connection failures.
    pub streaming_backoff_max_batch_size: usize,

    /// Timeout for streaming connection in milliseconds.
    pub streaming_connection_timeout_ms: u64,

    /// Timeout for streaming statement (peek/next) operations in milliseconds.
    pub streaming_statement_timeout_ms: u64,
}

pub struct IngestionService {
    config: IngestionConfig,
    ingestion_client: IngestionClient,
    streaming_client: Option<GrpcStreamingClient>,
    subscribers: Vec<mpsc::Sender<Arc<CheckpointEnvelope>>>,
    metrics: Arc<IngestionMetrics>,
}

impl IngestionConfig {
    pub fn retry_interval(&self) -> Duration {
        Duration::from_millis(self.retry_interval_ms)
    }

    pub fn streaming_connection_timeout(&self) -> Duration {
        Duration::from_millis(self.streaming_connection_timeout_ms)
    }

    pub fn streaming_statement_timeout(&self) -> Duration {
        Duration::from_millis(self.streaming_statement_timeout_ms)
    }
}

impl IngestionService {
    /// Create a new instance of the ingestion service, responsible for fetching checkpoints and
    /// disseminating them to subscribers.
    ///
    /// - `args` specifies where to fetch checkpoints from.
    /// - `config` specifies the various sizes and time limits for ingestion.
    /// - `metrics_prefix` and `registry` are used to set up metrics for the service.
    ///
    /// After initialization, subscribers can be added using [Self::subscribe_bounded], and the
    /// service is started with [Self::run], given a range of checkpoints to fetch (potentially
    /// unbounded).
    pub fn new(
        args: ClientArgs,
        config: IngestionConfig,
        metrics_prefix: Option<&str>,
        registry: &Registry,
    ) -> Result<Self> {
        let metrics = IngestionMetrics::new(metrics_prefix, registry);
        let ingestion_client = IngestionClient::new(args.ingestion, metrics.clone())?;
        let streaming_client = args.streaming.streaming_url.map(|uri| {
            GrpcStreamingClient::new(
                uri,
                config.streaming_connection_timeout(),
                config.streaming_statement_timeout(),
            )
        });

        Ok(Self {
            config,
            ingestion_client,
            streaming_client,
            subscribers: Vec::new(),
            metrics,
        })
    }

    /// The ingestion client this service uses to fetch checkpoints.
    pub(crate) fn ingestion_client(&self) -> &IngestionClient {
        &self.ingestion_client
    }

    /// Return the latest checkpoint number known to the ingestion service, preferably via the
    /// streaming client, and failing that via the ingestion client.
    pub async fn latest_checkpoint_number(&self) -> anyhow::Result<u64> {
        let streaming_client = self.streaming_client.clone();
        let ingestion_client = self.ingestion_client.clone();
        let future = move || {
            let mut streaming_client = streaming_client.clone();
            let ingestion_client = ingestion_client.clone();
            async move {
                latest_checkpoint_number(&mut streaming_client, &ingestion_client)
                    .await
                    .map_err(|e| backoff::Error::transient(Error::LatestCheckpointError(e)))
            }
        };

        Ok(retry_transient_with_slow_monitor(
            "latest_checkpoint_number",
            future,
            &self.metrics.ingested_latest_checkpoint_latency,
        )
        .await?)
    }

    /// Access to the ingestion metrics.
    pub(crate) fn metrics(&self) -> &Arc<IngestionMetrics> {
        &self.metrics
    }

    /// The ingestion configuration this service was built with.
    pub fn config(&self) -> &IngestionConfig {
        &self.config
    }

    /// Add a new subscription backed by a bounded `mpsc` channel of the given capacity. The
    /// channel itself is the backpressure signal: when this consumer falls behind, the channel
    /// fills and the adaptive ingestion controller cuts fetch concurrency. Send blocks when the
    /// channel is full.
    ///
    /// Callers typically pass `pipeline::IngestionConfig::subscriber_channel_size()`.
    pub fn subscribe_bounded(&mut self, size: usize) -> mpsc::Receiver<Arc<CheckpointEnvelope>> {
        let (tx, rx) = mpsc::channel(size);
        self.subscribers.push(tx);
        rx
    }

    /// Start the ingestion service as a background task, consuming it in the process.
    ///
    /// Checkpoints are fetched concurrently from the `checkpoints` iterator and pushed to
    /// subscribers' channels (potentially out-of-order). Each subscriber's bounded channel
    /// acts as the backpressure signal: when it fills, the adaptive ingestion controller
    /// throttles fetch concurrency. The slowest subscriber gates ingestion for everyone.
    ///
    /// If a subscriber closes its channel, the ingestion service shuts down as well.
    ///
    /// If ingestion reaches the leading edge of the network, it will encounter checkpoints
    /// that do not exist yet. These are retried on a fixed `retry_interval` until they become
    /// available.
    pub async fn run<R>(self, checkpoints: R) -> Result<Service>
    where
        R: std::ops::RangeBounds<u64> + Send + 'static,
    {
        let IngestionService {
            config,
            ingestion_client,
            streaming_client,
            subscribers,
            metrics,
        } = self;

        if subscribers.is_empty() {
            return Err(Error::NoSubscribers);
        }

        Ok(broadcaster(
            checkpoints,
            streaming_client,
            config,
            ingestion_client,
            subscribers,
            metrics,
        ))
    }
}

impl Default for IngestionConfig {
    fn default() -> Self {
        Self {
            ingest_concurrency: IngestConcurrencyConfig::Adaptive {
                initial: 1,
                min: 1,
                max: 500,
                dead_band: None,
            },
            retry_interval_ms: 200,
            streaming_backoff_initial_batch_size: 10, // 10 checkpoints, ~ 2 seconds
            streaming_backoff_max_batch_size: 10000,  // 10000 checkpoints, ~ 40 minutes
            streaming_connection_timeout_ms: 5000,    // 5 seconds
            streaming_statement_timeout_ms: 5000,     // 5 seconds
        }
    }
}

async fn latest_checkpoint_number(
    streaming_client: &mut Option<impl CheckpointStreamingClient + Send>,
    ingestion_client: &IngestionClient,
) -> anyhow::Result<u64> {
    if let Some(streaming_client) = streaming_client.as_mut() {
        match streaming_client.latest_checkpoint_number().await {
            Ok(checkpoint_number) => return Ok(checkpoint_number),
            Err(e) => {
                warn!(
                    operation = "latest_checkpoint_number",
                    "Failed to get latest checkpoint number from streaming client: {e}"
                );
            }
        }
    }

    ingestion_client.latest_checkpoint_number().await
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use axum::http::StatusCode;
    use sui_futures::task::TaskGuard;
    use url::Url;
    use wiremock::MockServer;
    use wiremock::Request;

    use crate::ingestion::ingestion_client::CheckpointResult;
    use crate::ingestion::ingestion_client::IngestionClientTrait;
    use crate::ingestion::store_client::tests::respond_with;
    use crate::ingestion::store_client::tests::respond_with_chain_id;
    use crate::ingestion::store_client::tests::status;
    use crate::ingestion::streaming_client::test_utils::MockStreamingClient;
    use crate::ingestion::test_utils::test_checkpoint_data;
    use crate::metrics::IngestionMetrics;
    use crate::types::digests::ChainIdentifier;

    use super::*;

    const FALLBACK: u64 = 99;

    struct MockLatestCheckpoint(u64);

    #[async_trait::async_trait]
    impl IngestionClientTrait for MockLatestCheckpoint {
        async fn chain_id(&self) -> anyhow::Result<ChainIdentifier> {
            unimplemented!()
        }
        async fn checkpoint(&self, _: u64) -> CheckpointResult {
            unimplemented!()
        }
        async fn latest_checkpoint_number(&self) -> anyhow::Result<u64> {
            Ok(self.0)
        }
    }

    fn mock_ingestion_client(latest_checkpoint: u64) -> IngestionClient {
        let metrics = IngestionMetrics::new(None, &Registry::new());
        IngestionClient::new_impl(Arc::new(MockLatestCheckpoint(latest_checkpoint)), metrics)
    }

    async fn test_ingestion(uri: String, ingest_concurrency: usize) -> IngestionService {
        let registry = Registry::new();
        IngestionService::new(
            ClientArgs {
                ingestion: IngestionClientArgs {
                    remote_store_url: Some(Url::parse(&uri).unwrap()),
                    ..Default::default()
                },
                ..Default::default()
            },
            IngestionConfig {
                ingest_concurrency: IngestConcurrencyConfig::Fixed {
                    value: ingest_concurrency,
                },
                ..Default::default()
            },
            None,
            &registry,
        )
        .unwrap()
    }

    async fn test_subscriber(
        stop_after: usize,
        mut rx: mpsc::Receiver<Arc<CheckpointEnvelope>>,
    ) -> TaskGuard<Vec<u64>> {
        TaskGuard::new(tokio::spawn(async move {
            let mut seqs = vec![];
            for _ in 0..stop_after {
                let Some(checkpoint_envelope) = rx.recv().await else {
                    break;
                };

                seqs.push(checkpoint_envelope.checkpoint.summary.sequence_number);
            }

            seqs
        }))
    }

    /// If the ingestion service has no subscribers, it will fail fast (before fetching any
    /// checkpoints).
    #[tokio::test]
    async fn fail_on_no_subscribers() {
        // The mock server will repeatedly return 404, so if the service does try to fetch a
        // checkpoint, it will be stuck repeatedly retrying.
        let server = MockServer::start().await;
        respond_with(&server, status(StatusCode::NOT_FOUND)).await;

        let ingestion_service = test_ingestion(server.uri(), 1).await;

        let res = ingestion_service.run(0..).await;
        assert!(matches!(res, Err(Error::NoSubscribers)));
    }

    /// The subscriber has no effective limit, and the mock server will always return checkpoint
    /// information, but the ingestion service can still be stopped by shutting it down.
    #[tokio::test]
    async fn shutdown() {
        let server = MockServer::start().await;
        respond_with(
            &server,
            status(StatusCode::OK).set_body_bytes(test_checkpoint_data(42)),
        )
        .await;
        respond_with_chain_id(&server).await;

        let mut ingestion_service = test_ingestion(server.uri(), 1).await;

        let rx = ingestion_service.subscribe_bounded(1);
        let subscriber = test_subscriber(usize::MAX, rx).await;
        let svc = ingestion_service.run(0..).await.unwrap();

        svc.shutdown().await.unwrap();
        subscriber.await.unwrap();
    }

    /// The subscriber will stop after receiving a single checkpoint, and this will trigger the
    /// ingestion service to stop as well, even if there are more checkpoints to fetch.
    #[tokio::test]
    async fn shutdown_on_subscriber_drop() {
        let server = MockServer::start().await;
        respond_with(
            &server,
            status(StatusCode::OK).set_body_bytes(test_checkpoint_data(42)),
        )
        .await;
        respond_with_chain_id(&server).await;

        let mut ingestion_service = test_ingestion(server.uri(), 1).await;

        let rx = ingestion_service.subscribe_bounded(1);
        let subscriber = test_subscriber(1, rx).await;
        let mut svc = ingestion_service.run(0..).await.unwrap();

        drop(subscriber);
        svc.join().await.unwrap();
    }

    /// The service will retry fetching a checkpoint that does not exist, in this test, the 4th
    /// checkpoint will return 404 a couple of times, before eventually succeeding.
    #[tokio::test]
    async fn retry_on_not_found() {
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
        respond_with_chain_id(&server).await;

        let mut ingestion_service = test_ingestion(server.uri(), 1).await;

        let rx = ingestion_service.subscribe_bounded(1);
        let subscriber = test_subscriber(6, rx).await;
        let _svc = ingestion_service.run(0..).await.unwrap();

        let seqs = subscriber.await.unwrap();
        assert_eq!(seqs, vec![0, 1, 2, 3, 6, 7]);
    }

    /// Similar to the previous test, but now it's a transient error that causes the retry.
    #[tokio::test]
    async fn retry_on_transient_error() {
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
        respond_with_chain_id(&server).await;

        let mut ingestion_service = test_ingestion(server.uri(), 1).await;

        let rx = ingestion_service.subscribe_bounded(1);
        let subscriber = test_subscriber(6, rx).await;
        let _svc = ingestion_service.run(0..).await.unwrap();

        let seqs = subscriber.await.unwrap();
        assert_eq!(seqs, vec![0, 1, 2, 3, 6, 7]);
    }

    /// One subscriber is going to stop processing checkpoints, so even though the service can keep
    /// fetching checkpoints, it will stop short because of the slow receiver. Other subscribers
    /// can keep processing checkpoints that were buffered for the slow one.
    #[tokio::test]
    async fn back_pressure_and_buffering() {
        let server = MockServer::start().await;
        let times: Mutex<u64> = Mutex::new(0);
        respond_with(&server, move |_: &Request| {
            let mut times = times.lock().unwrap();
            *times += 1;
            status(StatusCode::OK).set_body_bytes(test_checkpoint_data(*times))
        })
        .await;
        respond_with_chain_id(&server).await;

        let mut ingestion_service = test_ingestion(server.uri(), 1).await;
        let size = 3;

        // This subscriber will take its sweet time processing checkpoints.
        let mut laggard = ingestion_service.subscribe_bounded(size);
        async fn unblock(laggard: &mut mpsc::Receiver<Arc<CheckpointEnvelope>>) -> u64 {
            let checkpoint_envelope = laggard.recv().await.unwrap();
            checkpoint_envelope.checkpoint.summary.sequence_number
        }

        let rx = ingestion_service.subscribe_bounded(size);
        let subscriber = test_subscriber(6, rx).await;
        let _svc = ingestion_service.run(0..).await.unwrap();

        // At this point, the service will have been able to pass 3 checkpoints to the non-lagging
        // subscriber, while the laggard's buffer fills up. Now the laggard will pull two
        // checkpoints, which will allow the rest of the pipeline to progress enough for the live
        // subscriber to receive its quota. Checkpoint 0 is served by the chain_id mock.
        assert_eq!(unblock(&mut laggard).await, 0);
        assert_eq!(unblock(&mut laggard).await, 1);
        assert_eq!(unblock(&mut laggard).await, 2);

        let seqs = subscriber.await.unwrap();
        assert_eq!(seqs, vec![0, 1, 2, 3, 4, 5]);
    }

    #[tokio::test]
    async fn latest_checkpoint_number_no_streaming_client() {
        let client = mock_ingestion_client(FALLBACK);
        let mut streaming: Option<MockStreamingClient> = None;
        let result = latest_checkpoint_number(&mut streaming, &client).await;
        assert_eq!(result.unwrap(), FALLBACK);
    }

    #[tokio::test]
    async fn latest_checkpoint_number_from_stream() {
        let client = mock_ingestion_client(FALLBACK);
        let mut streaming = Some(MockStreamingClient::new([42], None));
        let result = latest_checkpoint_number(&mut streaming, &client).await;
        assert_eq!(result.unwrap(), 42);
    }

    #[tokio::test]
    async fn latest_checkpoint_number_stream_error_falls_back() {
        let client = mock_ingestion_client(FALLBACK);
        let mut mock = MockStreamingClient::new(std::iter::empty::<u64>(), None);
        mock.insert_error();
        let mut streaming = Some(mock);
        let result = latest_checkpoint_number(&mut streaming, &client).await;
        assert_eq!(result.unwrap(), FALLBACK);
    }

    #[tokio::test]
    async fn latest_checkpoint_number_empty_stream_falls_back() {
        let client = mock_ingestion_client(FALLBACK);
        let mut streaming = Some(MockStreamingClient::new(std::iter::empty::<u64>(), None));
        let result = latest_checkpoint_number(&mut streaming, &client).await;
        assert_eq!(result.unwrap(), FALLBACK);
    }

    #[tokio::test]
    async fn latest_checkpoint_number_connection_failure_falls_back() {
        let client = mock_ingestion_client(FALLBACK);
        let mut streaming = Some(
            MockStreamingClient::new(std::iter::empty::<u64>(), None).fail_connection_times(1),
        );
        let result = latest_checkpoint_number(&mut streaming, &client).await;
        assert_eq!(result.unwrap(), FALLBACK);
    }
}
