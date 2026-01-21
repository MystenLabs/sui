// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

use bytes::Bytes;
use object_store::Error as ObjectStoreError;
use object_store::ObjectStore;
use object_store::ObjectStoreExt;
use object_store::path::Path as ObjectPath;
use serde::de::DeserializeOwned;
use tracing::debug;
<<<<<<< HEAD:crates/sui-indexer-alt-framework/src/ingestion/store_client.rs
use tracing::error;
=======
use url::Url;
>>>>>>> 2dfe41f986 (Fix as per clippy suggestions):crates/sui-indexer-alt-framework/src/ingestion/remote_client.rs

use crate::ingestion::ingestion_client::FetchData;
use crate::ingestion::ingestion_client::FetchError;
use crate::ingestion::ingestion_client::FetchResult;
use crate::ingestion::ingestion_client::IngestionClientTrait;

pub struct StoreIngestionClient {
    store: Arc<dyn ObjectStore>,
}

impl StoreIngestionClient {
    pub fn new(store: Arc<dyn ObjectStore>) -> Self {
        Self { store }
    }

    /// Fetch metadata mapping epoch IDs to the sequence numbers of their last checkpoints.
    /// The response is a JSON-encoded array of checkpoint sequence numbers.
    pub async fn end_of_epoch_checkpoints<T: DeserializeOwned>(&self) -> anyhow::Result<T> {
        let bytes = self.bytes(ObjectPath::from("epochs.json")).await?;
        Ok(serde_json::from_slice(&bytes)?)
    }

    /// Fetch the bytes for a checkpoint by its sequence number.
    /// The response is the serialized representation of a checkpoint, as raw bytes.
    pub async fn checkpoint(&self, checkpoint: u64) -> object_store::Result<Bytes> {
        self.bytes(ObjectPath::from(format!("{checkpoint}.binpb.zst")))
            .await
    }

    async fn bytes(&self, path: ObjectPath) -> object_store::Result<Bytes> {
        let result = self.store.get(&path).await?;
        result.bytes().await
    }
}

#[async_trait::async_trait]
impl IngestionClientTrait for StoreIngestionClient {
    /// Fetch a checkpoint from the remote store.
    ///
    /// Transient errors include:
    ///
    /// - failures to issue a request, (network errors, redirect issues, etc)
    /// - request timeouts,
    /// - rate limiting,
    /// - server errors (5xx),
    /// - issues getting a full response.
    async fn fetch(&self, checkpoint: u64) -> FetchResult {
        match self.checkpoint(checkpoint).await {
            Ok(bytes) => Ok(FetchData::Raw(bytes)),
            Err(ObjectStoreError::NotFound { .. }) => {
                debug!(checkpoint, "Checkpoint not found");
                Err(FetchError::NotFound)
            }
            Err(error) => {
                error!(checkpoint, "Failed to fetch checkpoint: {error}");
                Err(FetchError::Transient {
                    reason: "object_store",
                    error: error.into(),
                })
            }
        }
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

    #[tokio::test]
    async fn fail_on_not_found() {
        let server = MockServer::start().await;
        respond_with(&server, status(StatusCode::NOT_FOUND)).await;

        let client = remote_test_client(server.uri());
        let error = client.fetch(42).await.unwrap_err();

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
                // The first request will trigger a redirect to 0.binpb.zst no matter what the
                // original request was for -- triggering a request error.
                (1, _) => {
                    status(StatusCode::MOVED_PERMANENTLY).append_header("Location", "/0.binpb.zst")
                }

                // Set-up checkpoint 0 as an infinite redirect loop.
                (_, "/0.binpb.zst") => {
                    status(StatusCode::MOVED_PERMANENTLY).append_header("Location", r.url.as_str())
                }

                // Subsequently, requests will succeed.
                _ => status(StatusCode::OK).set_body_bytes(test_checkpoint_data(42)),
            }
        })
        .await;

        let client = remote_test_client(server.uri());
        let checkpoint = client.fetch(42).await.unwrap();

        assert_eq!(42, checkpoint.summary.sequence_number)
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

        let client = remote_test_client(server.uri());
        let checkpoint = client.fetch(42).await.unwrap();

        assert_eq!(42, checkpoint.summary.sequence_number)
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

        let client = remote_test_client(server.uri());
        let checkpoint = client.fetch(42).await.unwrap();

        assert_eq!(42, checkpoint.summary.sequence_number)
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
                    std::thread::sleep(std::time::Duration::from_secs(4));
                    status(StatusCode::OK).set_body_bytes(test_checkpoint_data(42))
                }
                _ => {
                    // Respond immediately on retry attempts
                    status(StatusCode::OK).set_body_bytes(test_checkpoint_data(42))
                }
            }
        })
        .await;

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
        let checkpoint = ingestion_client.fetch(42).await.unwrap();
        assert_eq!(42, checkpoint.summary.sequence_number);

        // Verify that the server received exactly 2 requests (1 timeout + 1 successful retry)
        let final_count = times.load(Ordering::Relaxed);
        assert_eq!(final_count, 2);
    }
}
