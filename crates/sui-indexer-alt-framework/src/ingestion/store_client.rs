// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

use anyhow::Context;
use bytes::Bytes;
use object_store::Error;
use object_store::ObjectStore;
use object_store::ObjectStoreExt;
use object_store::RetryConfig;
use object_store::path::Path as ObjectPath;
use prometheus::IntCounter;
use serde::de::DeserializeOwned;
use sui_types::digests::ChainIdentifier;

use crate::ingestion::decode;
use crate::ingestion::ingestion_client::CheckpointError;
use crate::ingestion::ingestion_client::CheckpointResult;
use crate::ingestion::ingestion_client::IngestionClientTrait;
use crate::types::full_checkpoint_content::Checkpoint;

// from sui-indexer-alt-object-store
pub(crate) const WATERMARK_PATH: &str = "_metadata/watermark/checkpoint_blob.json";

pub struct StoreIngestionClient {
    store: Arc<dyn ObjectStore>,
    /// Counter incremented (in the [`IngestionClientTrait`] impl) by the size in bytes of each
    /// fetched checkpoint payload. `None` for callers that only use this client for one-shot
    /// metadata fetches (e.g. `end_of_epoch_checkpoints`) and don't need a metric.
    total_ingested_bytes: Option<IntCounter>,
}

#[derive(serde::Deserialize, serde::Serialize)]
pub(crate) struct ObjectStoreWatermark {
    pub checkpoint_hi_inclusive: u64,
}

impl StoreIngestionClient {
    pub fn new(store: Arc<dyn ObjectStore>, total_ingested_bytes: Option<IntCounter>) -> Self {
        Self {
            store,
            total_ingested_bytes,
        }
    }

    /// Fetch metadata mapping epoch IDs to the sequence numbers of their last checkpoints.
    /// The response is a JSON-encoded array of checkpoint sequence numbers.
    pub async fn end_of_epoch_checkpoints<T: DeserializeOwned>(&self) -> anyhow::Result<T> {
        let bytes = self.bytes(ObjectPath::from("epochs.json")).await?;
        Ok(serde_json::from_slice(&bytes)?)
    }

    /// Fetch and decode checkpoint data by sequence number.
    pub async fn checkpoint(&self, checkpoint: u64) -> anyhow::Result<Checkpoint> {
        let bytes = self.checkpoint_bytes(checkpoint).await?;
        Ok(decode::checkpoint(&bytes)?)
    }

    async fn checkpoint_bytes(&self, checkpoint: u64) -> object_store::Result<Bytes> {
        self.bytes(ObjectPath::from(format!("{checkpoint}.binpb.zst")))
            .await
    }

    async fn bytes(&self, path: ObjectPath) -> object_store::Result<Bytes> {
        let result = self.store.get(&path).await?;
        result.bytes().await
    }

    async fn watermark_checkpoint_hi_inclusive(&self) -> anyhow::Result<Option<u64>> {
        let bytes = match self.bytes(ObjectPath::from(WATERMARK_PATH)).await {
            Ok(bytes) => bytes,
            Err(Error::NotFound { .. }) => return Ok(None),
            Err(e) => return Err(e).context(format!("error reading {WATERMARK_PATH}")),
        };

        let watermark: ObjectStoreWatermark =
            serde_json::from_slice(&bytes).context(format!("error parsing {WATERMARK_PATH}"))?;

        Ok(Some(watermark.checkpoint_hi_inclusive))
    }
}

#[async_trait::async_trait]
impl IngestionClientTrait for StoreIngestionClient {
    async fn chain_id(&self) -> anyhow::Result<ChainIdentifier> {
        let checkpoint = self.checkpoint(0).await?;
        Ok((*checkpoint.summary.digest()).into())
    }

    /// Fetch a checkpoint from the remote store.
    ///
    /// Transient errors include:
    ///
    /// - failures to issue a request, (network errors, redirect issues, etc)
    /// - request timeouts,
    /// - rate limiting,
    /// - server errors (5xx),
    /// - issues getting a full response.
    async fn checkpoint(&self, checkpoint: u64) -> CheckpointResult {
        let bytes = self
            .checkpoint_bytes(checkpoint)
            .await
            .map_err(|e| match e {
                Error::NotFound { .. } => CheckpointError::NotFound,
                e => CheckpointError::Fetch(e.into()),
            })?;

        if let Some(counter) = &self.total_ingested_bytes {
            counter.inc_by(bytes.len() as u64);
        }
        decode::checkpoint(&bytes).map_err(CheckpointError::Decode)
    }

    async fn latest_checkpoint_number(&self) -> anyhow::Result<u64> {
        self.watermark_checkpoint_hi_inclusive()
            .await
            .map(|cp| cp.unwrap_or(0))
    }
}

/// Disable object_store's internal retries so that transient errors (429s, 5xx) propagate
/// immediately to the framework's own retry logic.
pub(super) fn retry_config() -> RetryConfig {
    RetryConfig {
        max_retries: 0,
        ..Default::default()
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use axum::http::StatusCode;
    use object_store::ClientOptions;
    use object_store::http::HttpBuilder;
    use std::sync::Mutex;
    use std::sync::atomic::AtomicUsize;
    use std::sync::atomic::Ordering;
    use std::time::Duration;
    use wiremock::Mock;
    use wiremock::MockServer;
    use wiremock::Request;
    use wiremock::Respond;
    use wiremock::ResponseTemplate;
    use wiremock::matchers::method;
    use wiremock::matchers::path;
    use wiremock::matchers::path_regex;

    use crate::ingestion::error::Error;
    use crate::ingestion::ingestion_client::IngestionClient;
    use crate::ingestion::test_utils::test_checkpoint_data;
    use crate::metrics::tests::test_ingestion_metrics;

    use super::*;

    pub(crate) async fn respond_with(server: &MockServer, response: impl Respond + 'static) {
        Mock::given(method("GET"))
            .and(path_regex(r"/\d+\.binpb\.zst"))
            .respond_with(response)
            .mount(server)
            .await;
    }

    /// Mount a high-priority mock for checkpoint 0 used by `StoreIngestionClient::chain_id()`.
    pub(crate) async fn respond_with_chain_id(server: &MockServer) {
        Mock::given(method("GET"))
            .and(path("/0.binpb.zst"))
            .respond_with(status(StatusCode::OK).set_body_bytes(test_checkpoint_data(0)))
            .with_priority(1)
            .mount(server)
            .await;
    }

    /// Returns the expected chain_id produced by `StoreIngestionClient::chain_id()` when
    /// `respond_with_chain_id` is mounted.
    pub(crate) fn expected_chain_id() -> ChainIdentifier {
        let bytes = test_checkpoint_data(0);
        let checkpoint = decode::checkpoint(&bytes).unwrap();
        (*checkpoint.summary.digest()).into()
    }

    pub(crate) fn status(code: StatusCode) -> ResponseTemplate {
        ResponseTemplate::new(code.as_u16())
    }

    fn remote_test_client(uri: String) -> IngestionClient {
        let store = HttpBuilder::new()
            .with_url(uri)
            .with_client_options(ClientOptions::default().with_allow_http(true))
            .build()
            .map(Arc::new)
            .unwrap();
        IngestionClient::with_store(store, test_ingestion_metrics()).unwrap()
    }

    async fn test_latest_checkpoint_number(watermark: ResponseTemplate) -> anyhow::Result<u64> {
        let server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path(WATERMARK_PATH))
            .respond_with(watermark)
            .mount(&server)
            .await;

        let store = HttpBuilder::new()
            .with_url(server.uri())
            .with_client_options(ClientOptions::default().with_allow_http(true))
            .build()
            .map(Arc::new)
            .unwrap();
        let client = StoreIngestionClient::new(store, None);

        IngestionClientTrait::latest_checkpoint_number(&client).await
    }

    #[tokio::test]
    async fn test_latest_checkpoint_no_watermark() {
        assert_eq!(
            test_latest_checkpoint_number(status(StatusCode::NOT_FOUND))
                .await
                .unwrap(),
            0
        )
    }

    #[tokio::test]
    async fn test_latest_checkpoint_corrupt_watermark() {
        assert!(
            test_latest_checkpoint_number(status(StatusCode::OK).set_body_string("<"))
                .await
                .is_err()
        )
    }

    #[tokio::test]
    async fn test_latest_checkpoint_from_watermark() {
        let body = serde_json::json!({"checkpoint_hi_inclusive": 1}).to_string();
        assert_eq!(
            test_latest_checkpoint_number(status(StatusCode::OK).set_body_string(body))
                .await
                .unwrap(),
            1
        )
    }

    #[tokio::test]
    async fn fail_on_not_found() {
        let server = MockServer::start().await;
        respond_with(&server, status(StatusCode::NOT_FOUND)).await;

        let client = remote_test_client(server.uri());
        let error = client.checkpoint(42).await.unwrap_err();

        assert!(matches!(error, Error::NotFound(42)));
    }

    /// Assume that failures to send the request to the remote store are due to temporary
    /// connectivity issues, and retry them.
    #[tokio::test]
    async fn retry_on_request_error() {
        let server = MockServer::start().await;

        let times: Mutex<u64> = Mutex::new(0);
        respond_with(&server, move |r: &Request| {
            let mut times = times.lock().unwrap();
            *times += 1;
            match (*times, r.url.path()) {
                // The first request will trigger a redirect to 999999.binpb.zst no matter what
                // the original request was for -- triggering a request error.
                (1, _) => status(StatusCode::MOVED_PERMANENTLY)
                    .append_header("Location", "/999999.binpb.zst"),

                // Set-up checkpoint 999999 as an infinite redirect loop.
                (_, "/999999.binpb.zst") => {
                    status(StatusCode::MOVED_PERMANENTLY).append_header("Location", r.url.as_str())
                }

                // Subsequently, requests will succeed.
                _ => status(StatusCode::OK).set_body_bytes(test_checkpoint_data(42)),
            }
        })
        .await;
        respond_with_chain_id(&server).await;

        let client = remote_test_client(server.uri());
        let envelope = client.checkpoint(42).await.unwrap();

        assert_eq!(42, envelope.checkpoint.summary.sequence_number);
        assert_eq!(envelope.chain_id, expected_chain_id());
    }

    /// Assume that certain errors will recover by themselves, and keep retrying with an
    /// exponential back-off. These errors include: 5xx (server) errors, 408 (timeout), and 429
    /// (rate limiting).
    #[tokio::test]
    async fn retry_on_transient_server_error() {
        let server = MockServer::start().await;
        let times: Mutex<u64> = Mutex::new(0);
        respond_with(&server, move |_: &Request| {
            let mut times = times.lock().unwrap();
            *times += 1;
            match *times {
                1 => status(StatusCode::INTERNAL_SERVER_ERROR),
                2 => status(StatusCode::REQUEST_TIMEOUT),
                3 => status(StatusCode::TOO_MANY_REQUESTS),
                _ => status(StatusCode::OK).set_body_bytes(test_checkpoint_data(42)),
            }
        })
        .await;
        respond_with_chain_id(&server).await;

        let client = remote_test_client(server.uri());
        let envelope = client.checkpoint(42).await.unwrap();

        assert_eq!(42, envelope.checkpoint.summary.sequence_number);
        assert_eq!(envelope.chain_id, expected_chain_id());
    }

    /// Treat deserialization failure as another kind of transient error -- all checkpoint data
    /// that is fetched should be valid (deserializable as a `Checkpoint`).
    #[tokio::test]
    async fn retry_on_deserialization_error() {
        let server = MockServer::start().await;
        let times: Mutex<u64> = Mutex::new(0);
        respond_with(&server, move |_: &Request| {
            let mut times = times.lock().unwrap();
            *times += 1;
            if *times < 3 {
                status(StatusCode::OK).set_body_bytes(vec![])
            } else {
                status(StatusCode::OK).set_body_bytes(test_checkpoint_data(42))
            }
        })
        .await;
        respond_with_chain_id(&server).await;

        let client = remote_test_client(server.uri());
        let envelope = client.checkpoint(42).await.unwrap();

        assert_eq!(42, envelope.checkpoint.summary.sequence_number);
        assert_eq!(envelope.chain_id, expected_chain_id());
    }

    /// Test that timeout errors are retried as transient errors.
    /// The first request will timeout, the second will succeed.
    #[tokio::test]
    async fn retry_on_timeout() {
        let server = MockServer::start().await;
        let times: Arc<AtomicUsize> = Arc::new(AtomicUsize::new(0));
        let times_clone = times.clone();

        // First request will delay longer than timeout, second will succeed immediately
        respond_with(&server, move |_: &Request| {
            match times_clone.fetch_add(1, Ordering::Relaxed) {
                0 => {
                    // Delay longer than our test timeout (2 seconds)
                    std::thread::sleep(Duration::from_secs(4));
                    status(StatusCode::OK).set_body_bytes(test_checkpoint_data(42))
                }
                _ => {
                    // Respond immediately on retry attempts
                    status(StatusCode::OK).set_body_bytes(test_checkpoint_data(42))
                }
            }
        })
        .await;
        respond_with_chain_id(&server).await;

        let options = ClientOptions::default()
            .with_allow_http(true)
            .with_timeout(Duration::from_secs(2));
        let store = HttpBuilder::new()
            .with_url(server.uri())
            .with_client_options(options)
            .build()
            .map(Arc::new)
            .unwrap();
        let ingestion_client =
            IngestionClient::with_store(store, test_ingestion_metrics()).unwrap();

        // This should timeout once, then succeed on retry
        let envelope = ingestion_client.checkpoint(42).await.unwrap();
        assert_eq!(42, envelope.checkpoint.summary.sequence_number);
        assert_eq!(envelope.chain_id, expected_chain_id());

        // Verify that the server received exactly 2 requests (1 timeout + 1 successful retry)
        // The chain_id request for checkpoint 0 is handled by a separate mock.
        let final_count = times.load(Ordering::Relaxed);
        assert_eq!(final_count, 2);
    }
}
