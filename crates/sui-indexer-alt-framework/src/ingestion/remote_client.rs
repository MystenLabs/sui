// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::ingestion::client::{FetchData, FetchError, FetchResult, IngestionClientTrait};
use crate::ingestion::Result as IngestionResult;
use reqwest::{Client, StatusCode};
use tracing::{debug, error};
use url::Url;

#[derive(thiserror::Error, Debug, Eq, PartialEq)]
pub enum HttpError {
    #[error("HTTP error with status code: {0}")]
    Http(StatusCode),
}

fn status_code_to_error(code: StatusCode) -> anyhow::Error {
    HttpError::Http(code).into()
}

pub(crate) struct RemoteIngestionClient {
    url: Url,
    client: Client,
}

impl RemoteIngestionClient {
    pub(crate) fn new(url: Url) -> IngestionResult<Self> {
        Ok(Self {
            url,
            client: Client::builder().build()?,
        })
    }
}

#[async_trait::async_trait]
impl IngestionClientTrait for RemoteIngestionClient {
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
        // SAFETY: The path being joined is statically known to be valid.
        let url = self
            .url
            .join(&format!("/{checkpoint}.chk"))
            .expect("Unexpected invalid URL");

        let response = self
            .client
            .get(url)
            .send()
            .await
            .map_err(|e| FetchError::Transient {
                reason: "request",
                error: e.into(),
            })?;

        match response.status() {
            code if code.is_success() => {
                // Failure to extract all the bytes from the payload, or to deserialize the
                // checkpoint from them is considered a transient error -- the store being
                // fetched from needs to be corrected, and ingestion will keep retrying it
                // until it is.
                response
                    .bytes()
                    .await
                    .map_err(|e| FetchError::Transient {
                        reason: "bytes",
                        error: e.into(),
                    })
                    .map(FetchData::Raw)
            }

            // Treat 404s as a special case so we can match on this error type.
            code @ StatusCode::NOT_FOUND => {
                debug!(checkpoint, %code, "Checkpoint not found");
                Err(FetchError::NotFound)
            }

            // Timeouts are a client error but they are usually transient.
            code @ StatusCode::REQUEST_TIMEOUT => Err(FetchError::Transient {
                reason: "timeout",
                error: status_code_to_error(code),
            }),

            // Rate limiting is also a client error, but the backoff will eventually widen the
            // interval appropriately.
            code @ StatusCode::TOO_MANY_REQUESTS => Err(FetchError::Transient {
                reason: "too_many_requests",
                error: status_code_to_error(code),
            }),

            // Assume that if the server is facing difficulties, it will recover eventually.
            code if code.is_server_error() => Err(FetchError::Transient {
                reason: "server_error",
                error: status_code_to_error(code),
            }),

            // Still retry on other unsuccessful codes, but the reason is unclear.
            code => Err(FetchError::Transient {
                reason: "unknown",
                error: status_code_to_error(code),
            }),
        }
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::ingestion::client::IngestionClient;
    use crate::ingestion::error::Error;
    use crate::ingestion::test_utils::test_checkpoint_data;
    use crate::metrics::tests::test_metrics;
    use axum::http::StatusCode;
    use std::sync::Mutex;
    use wiremock::{
        matchers::{method, path_regex},
        Mock, MockServer, Request, Respond, ResponseTemplate,
    };

    pub(crate) async fn respond_with(server: &MockServer, response: impl Respond + 'static) {
        Mock::given(method("GET"))
            .and(path_regex(r"/\d+.chk"))
            .respond_with(response)
            .mount(server)
            .await;
    }

    pub(crate) fn status(code: StatusCode) -> ResponseTemplate {
        ResponseTemplate::new(code.as_u16())
    }

    fn remote_test_client(uri: String) -> IngestionClient {
        IngestionClient::new_remote(Url::parse(&uri).unwrap(), test_metrics()).unwrap()
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
                // The first request will trigger a redirect to 0.chk no matter what the original
                // request was for -- triggering a request error.
                (1, _) => status(StatusCode::MOVED_PERMANENTLY).append_header("Location", "/0.chk"),

                // Set-up checkpoint 0 as an infinite redirect loop.
                (_, "/0.chk") => {
                    status(StatusCode::MOVED_PERMANENTLY).append_header("Location", r.url.as_str())
                }

                // Subsequently, requests will succeed.
                _ => status(StatusCode::OK).set_body_bytes(test_checkpoint_data(42)),
            }
        })
        .await;

        let client = remote_test_client(server.uri());
        let checkpoint = client.fetch(42).await.unwrap();

        assert_eq!(42, checkpoint.checkpoint_summary.sequence_number)
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

        assert_eq!(42, checkpoint.checkpoint_summary.sequence_number)
    }

    /// Treat deserialization failure as another kind of transient error -- all checkpoint data
    /// that is fetched should be valid (deserializable as a `CheckpointData`).
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

        assert_eq!(42, checkpoint.checkpoint_summary.sequence_number)
    }
}
