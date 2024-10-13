// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{sync::Arc, time::Duration};

use backoff::{backoff::Constant, ExponentialBackoff};
use futures::{future::try_join_all, stream, StreamExt, TryStreamExt};
use mysten_metrics::spawn_monitored_task;
use reqwest::{Client, StatusCode};
use sui_storage::blob::Blob;
use sui_types::full_checkpoint_content::CheckpointData;
use tokio::{sync::mpsc, task::JoinHandle};
use tokio_util::sync::CancellationToken;
use tracing::{debug, error, info};
use url::Url;

use crate::metrics::IndexerMetrics;

/// Wait at most this long between retries for transient errors.
const MAX_TRANSIENT_RETRY_INTERVAL: Duration = Duration::from_secs(60);

pub struct IngestionService {
    config: IngestionConfig,
    client: IngestionClient,
    metrics: Arc<IndexerMetrics>,
    subscribers: Vec<mpsc::Sender<Arc<CheckpointData>>>,
    cancel: CancellationToken,
}

#[derive(Clone)]
struct IngestionClient {
    url: Url,
    client: Client,
    /// Wrap the metrics in an `Arc` to keep copies of the client cheap.
    metrics: Arc<IndexerMetrics>,
}

#[derive(clap::Args, Debug, Clone)]
pub struct IngestionConfig {
    /// Remote Store to fetch checkpoints from.
    #[arg(long)]
    remote_store_url: Url,

    /// First checkpoint to start ingestion from.
    #[arg(long, default_value_t = 0)]
    start_checkpoint: u64,

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

type Result<T> = std::result::Result<T, Error>;

#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error("Checkpoint {0} not found")]
    NotFound(u64),

    #[error("Failed to deserialize checkpoint {0}: {1}")]
    DeserializationError(u64, #[source] anyhow::Error),

    #[error("Failed to fetch checkpoint {0}: {1}")]
    HttpError(u64, StatusCode),

    #[error(transparent)]
    ReqwestError(#[from] reqwest::Error),

    #[error("No subscribers for ingestion service")]
    NoSubscribers,
}

impl IngestionService {
    pub fn new(
        config: IngestionConfig,
        metrics: IndexerMetrics,
        cancel: CancellationToken,
    ) -> Result<Self> {
        let metrics = Arc::new(metrics);
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
    /// Checkpoints are fetched concurrently starting with the configured `start_checkpoint`, and
    /// pushed to subscribers' channels (potentially out-of-order). Subscribers can communicate
    /// with the ingestion service via their channels in the following ways:
    ///
    /// - If a subscriber is lagging (not receiving checkpoints fast enough), it will eventually
    ///   provide back-pressure to the ingestion service, which will stop fetching new checkpoints.
    /// - If a subscriber closes its channel, the ingestion service will intepret that as a signal
    ///   to shutdown as well.
    ///
    /// If ingestion reaches the leading edge of the network, it will encounter checkpoints that do
    /// not exist yet. These will be retried repeatedly on a fixed `retry_interval` until they
    /// become available.
    pub async fn run(self) -> Result<JoinHandle<()>> {
        let IngestionService {
            config,
            client,
            metrics,
            subscribers,
            cancel,
        } = self;

        /// Individual iterations of `try_for_each_concurrent` communicate with the supervisor by
        /// returning an `Err` with a `Break` variant.
        enum Break {
            Cancelled,
            Err(u64, Error),
        }

        if subscribers.is_empty() {
            return Err(Error::NoSubscribers);
        }

        Ok(spawn_monitored_task!(async move {
            let start = config.start_checkpoint;
            info!(start, "Starting ingestion service");

            match stream::iter(start..)
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
                                return Err(BE::permanent(Break::Cancelled));
                            }

                            client.fetch(cp).await.map_err(|e| match e {
                                Error::NotFound(checkpoint) => {
                                    debug!(checkpoint, "Checkpoint not found, retrying...");
                                    metrics.total_ingested_not_found_retries.inc();
                                    BE::transient(Break::Err(cp, e))
                                }
                                e => BE::permanent(Break::Err(cp, e)),
                            })
                        }
                    };

                    async move {
                        let checkpoint = backoff::future::retry(backoff, fetch).await?;
                        let futures = subscribers.iter().map(|s| s.send(checkpoint.clone()));

                        if try_join_all(futures).await.is_err() {
                            info!("Subscription dropped, signalling shutdown");
                            supervisor_cancel.cancel();
                            Err(Break::Cancelled)
                        } else {
                            Ok(())
                        }
                    }
                })
                .await
            {
                Ok(()) => {}
                Err(Break::Cancelled) => {
                    info!("Shutdown received, stopping ingestion service");
                }

                Err(Break::Err(checkpoint, e)) => {
                    error!(checkpoint, "Ingestion service failed: {}", e);
                }
            }
        }))
    }
}

impl IngestionClient {
    fn new(url: Url, metrics: Arc<IndexerMetrics>) -> Result<Self> {
        Ok(Self {
            url,
            client: Client::builder().build()?,
            metrics,
        })
    }

    /// Fetch a checkpoint from the remote store. Repeatedly retries transient errors with an
    /// exponential backoff (up to [MAX_RETRY_INTERVAL]), but will immediately return
    /// non-transient errors, which include all client errors, except timeouts and rate limiting.
    async fn fetch(&self, checkpoint: u64) -> Result<Arc<CheckpointData>> {
        // SAFETY: The path being joined is statically known to be valid.
        let url = self
            .url
            .join(&format!("/{checkpoint}.chk"))
            .expect("Unexpected invalid URL");

        let request = move || {
            let url = url.clone();
            async move {
                let response = self
                    .client
                    .get(url)
                    .send()
                    .await
                    .expect("Unexpected error building request");

                use backoff::Error as BE;
                match response.status() {
                    code if code.is_success() => Ok(response),

                    // Treat 404s as a special case so we can match on this error type.
                    code @ StatusCode::NOT_FOUND => {
                        debug!(checkpoint, %code, "Checkpoint not found");
                        Err(BE::permanent(Error::NotFound(checkpoint)))
                    }

                    // Timeouts are a client error but they are usually transient.
                    code @ StatusCode::REQUEST_TIMEOUT => {
                        debug!(checkpoint, %code, "Transient error, retrying...");
                        self.metrics.total_ingested_transient_retries.inc();
                        Err(BE::transient(Error::HttpError(checkpoint, code)))
                    }

                    // Rate limiting is also a client error, but the backoff will eventually widen the
                    // interval appropriately.
                    code @ StatusCode::TOO_MANY_REQUESTS => {
                        debug!(checkpoint, %code, "Transient error, retrying...");
                        self.metrics.total_ingested_transient_retries.inc();
                        Err(BE::transient(Error::HttpError(checkpoint, code)))
                    }

                    // Assume that if the server is facing difficulties, it will recover eventually.
                    code if code.is_server_error() => {
                        debug!(checkpoint, %code, "Transient error, retrying...");
                        self.metrics.total_ingested_transient_retries.inc();
                        Err(BE::transient(Error::HttpError(checkpoint, code)))
                    }

                    // For everything else, assume it's a permanent error and don't retry.
                    code => {
                        debug!(checkpoint, %code, "Permanent error, giving up!");
                        Err(BE::permanent(Error::HttpError(checkpoint, code)))
                    }
                }
            }
        };

        // Keep backing off until we are waiting for the max interval, but don't give up.
        let backoff = ExponentialBackoff {
            max_interval: MAX_TRANSIENT_RETRY_INTERVAL,
            max_elapsed_time: None,
            ..Default::default()
        };

        let guard = self.metrics.ingested_checkpoint_latency.start_timer();

        let bytes = backoff::future::retry(backoff, request)
            .await?
            .bytes()
            .await?;

        let data: CheckpointData =
            Blob::from_bytes(&bytes).map_err(|e| Error::DeserializationError(checkpoint, e))?;

        let elapsed = guard.stop_and_record();
        debug!(
            checkpoint,
            "Fetched checkpoint in {:.03}ms",
            elapsed * 1000.0
        );

        self.metrics.total_ingested_checkpoints.inc();
        self.metrics.total_ingested_bytes.inc_by(bytes.len() as u64);

        self.metrics
            .total_ingested_transactions
            .inc_by(data.transactions.len() as u64);

        self.metrics.total_ingested_events.inc_by(
            data.transactions
                .iter()
                .map(|tx| tx.events.as_ref().map_or(0, |evs| evs.data.len()) as u64)
                .sum(),
        );

        self.metrics.total_ingested_inputs.inc_by(
            data.transactions
                .iter()
                .map(|tx| tx.input_objects.len() as u64)
                .sum(),
        );

        self.metrics.total_ingested_outputs.inc_by(
            data.transactions
                .iter()
                .map(|tx| tx.output_objects.len() as u64)
                .sum(),
        );

        Ok(Arc::new(data))
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use wiremock::{
        matchers::{method, path_regex},
        Mock, MockServer, Request, Respond, ResponseTemplate,
    };

    use crate::metrics::tests::test_metrics;

    use super::*;

    async fn respond_with(server: &MockServer, response: impl Respond + 'static) {
        Mock::given(method("GET"))
            .and(path_regex(r"/\d+.chk"))
            .respond_with(response)
            .mount(server)
            .await;
    }

    fn status(code: StatusCode) -> ResponseTemplate {
        ResponseTemplate::new(code.as_u16())
    }

    fn test_client(uri: String) -> IngestionClient {
        IngestionClient::new(Url::parse(&uri).unwrap(), Arc::new(test_metrics())).unwrap()
    }

    #[tokio::test]
    async fn not_found() {
        let server = MockServer::start().await;
        respond_with(&server, status(StatusCode::NOT_FOUND)).await;

        let client = test_client(server.uri());
        let error = client.fetch(42).await.unwrap_err();

        assert!(matches!(error, Error::NotFound(42)));
    }

    #[tokio::test]
    async fn client_error() {
        let server = MockServer::start().await;
        respond_with(&server, status(StatusCode::IM_A_TEAPOT)).await;

        let client = test_client(server.uri());
        let error = client.fetch(42).await.unwrap_err();

        assert!(matches!(
            error,
            Error::HttpError(42, StatusCode::IM_A_TEAPOT)
        ));
    }

    #[tokio::test]
    async fn transient_server_error() {
        let server = MockServer::start().await;

        let times: Mutex<u64> = Mutex::new(0);
        respond_with(&server, move |_: &Request| {
            let mut times = times.lock().unwrap();
            *times += 1;
            status(match *times {
                1 => StatusCode::INTERNAL_SERVER_ERROR,
                2 => StatusCode::REQUEST_TIMEOUT,
                3 => StatusCode::TOO_MANY_REQUESTS,
                _ => StatusCode::IM_A_TEAPOT,
            })
        })
        .await;

        let client = test_client(server.uri());
        let error = client.fetch(42).await.unwrap_err();

        assert!(matches!(
            error,
            Error::HttpError(42, StatusCode::IM_A_TEAPOT)
        ));
    }
}
