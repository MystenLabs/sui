// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{sync::Arc, time::Duration};

use backoff::ExponentialBackoff;
use reqwest::{Client, StatusCode};
use sui_storage::blob::Blob;
use sui_types::full_checkpoint_content::CheckpointData;
use url::Url;

/// Wait at most this long between retries for transient errors.
const MAX_RETRY_INTERVAL: Duration = Duration::from_secs(60);

pub struct IngestionClient {
    url: Url,
    client: Client,
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
}

impl IngestionClient {
    pub fn new(url: Url) -> Result<Self> {
        Ok(Self {
            url,
            client: Client::builder().build()?,
        })
    }

    /// Fetch a checkpoint from the remote store. Repeatedly retries transient errors with an
    /// exponential backoff (up to [MAX_RETRY_INTERVAL]), but will immediately return
    /// non-transient errors, which include all client errors, except timeouts and rate limiting.
    pub async fn fetch(&self, checkpoint: u64) -> Result<Arc<CheckpointData>> {
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
                    StatusCode::NOT_FOUND => Err(BE::Permanent(Error::NotFound(checkpoint))),

                    // Timeouts are a client error but they are usually transient.
                    code @ StatusCode::REQUEST_TIMEOUT => {
                        Err(BE::transient(Error::HttpError(checkpoint, code)))
                    }

                    // Rate limiting is also a client error, but the backoff will eventually widen the
                    // interval appropriately.
                    code @ StatusCode::TOO_MANY_REQUESTS => {
                        Err(BE::transient(Error::HttpError(checkpoint, code)))
                    }

                    // Assume that if the server is facing difficulties, it will recover eventually.
                    code if code.is_server_error() => {
                        Err(BE::transient(Error::HttpError(checkpoint, code)))
                    }

                    // For everything else, assume it's a permanent error and don't retry.
                    code => Err(BE::Permanent(Error::HttpError(checkpoint, code))),
                }
            }
        };

        // Keep backing off until we are waiting for the max interval, but don't give up.
        let backoff = ExponentialBackoff {
            max_interval: MAX_RETRY_INTERVAL,
            max_elapsed_time: None,
            ..Default::default()
        };

        let bytes = backoff::future::retry(backoff, request)
            .await?
            .bytes()
            .await?;

        Ok(Arc::new(
            Blob::from_bytes(&bytes).map_err(|e| Error::DeserializationError(checkpoint, e))?,
        ))
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use wiremock::{
        matchers::{method, path_regex},
        Mock, MockServer, Request, Respond, ResponseTemplate,
    };

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

    #[tokio::test]
    async fn not_found() {
        let server = MockServer::start().await;
        respond_with(&server, status(StatusCode::NOT_FOUND)).await;

        let client = IngestionClient::new(Url::parse(&server.uri()).unwrap()).unwrap();
        let error = client.fetch(42).await.unwrap_err();

        assert!(matches!(error, Error::NotFound(42)));
    }

    #[tokio::test]
    async fn client_error() {
        let server = MockServer::start().await;
        respond_with(&server, status(StatusCode::IM_A_TEAPOT)).await;

        let client = IngestionClient::new(Url::parse(&server.uri()).unwrap()).unwrap();
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

        let client = IngestionClient::new(Url::parse(&server.uri()).unwrap()).unwrap();
        let error = client.fetch(42).await.unwrap_err();

        assert!(matches!(
            error,
            Error::HttpError(42, StatusCode::IM_A_TEAPOT)
        ));
    }
}
