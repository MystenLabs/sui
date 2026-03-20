// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Minimal client API for interacting with a running `sui-forking` server.

use serde::de::DeserializeOwned;
use url::Url;

use crate::api::endpoints::{self, Endpoint, EndpointMethod};
use crate::api::error::ClientError;
use crate::api::types::{ApiResponse, ForkingStatus};

#[derive(Clone, Debug)]
pub struct ForkingClient {
    base_url: Url,
    http: reqwest::Client,
}

impl ForkingClient {
    pub fn new(mut base_url: Url) -> Self {
        if base_url.path().is_empty() {
            base_url.set_path("/");
        }
        base_url.set_query(None);
        base_url.set_fragment(None);

        Self {
            base_url,
            http: reqwest::Client::new(),
        }
    }

    pub fn base_url(&self) -> &Url {
        &self.base_url
    }

    pub async fn status(&self) -> Result<ForkingStatus, ClientError> {
        self.send_without_body(endpoints::STATUS).await
    }

    async fn send_without_body<T>(&self, endpoint: Endpoint) -> Result<T, ClientError>
    where
        T: DeserializeOwned,
    {
        let url = self.endpoint_url(endpoint)?;
        let response = self
            .http
            .request(reqwest_method(endpoint.method), url.clone())
            .send()
            .await
            .map_err(|source| ClientError::Transport {
                url: url.clone(),
                source,
            })?;
        self.decode_response(endpoint.path, url, response).await
    }

    async fn decode_response<T>(
        &self,
        endpoint: &'static str,
        url: Url,
        response: reqwest::Response,
    ) -> Result<T, ClientError>
    where
        T: DeserializeOwned,
    {
        let status = response.status();
        if !status.is_success() {
            let body = match response.text().await {
                Ok(text) => text,
                Err(err) => format!("<failed to read response body: {err}>"),
            };
            return Err(ClientError::HttpStatus { url, status, body });
        }

        let envelope: ApiResponse<T> =
            response
                .json()
                .await
                .map_err(|source| ClientError::Decode {
                    url: url.clone(),
                    source,
                })?;

        if !envelope.success {
            return Err(ClientError::Api {
                message: envelope
                    .error
                    .unwrap_or_else(|| format!("endpoint '{endpoint}' reported failure")),
            });
        }

        envelope.data.ok_or(ClientError::MissingData { endpoint })
    }

    fn endpoint_url(&self, endpoint: Endpoint) -> Result<Url, ClientError> {
        self.base_url
            .join(endpoint.client_path())
            .map_err(|source| ClientError::UrlJoin {
                path: endpoint.path.to_string(),
                source,
            })
    }
}

fn reqwest_method(method: EndpointMethod) -> reqwest::Method {
    match method {
        EndpointMethod::Get => reqwest::Method::GET,
        EndpointMethod::Post => reqwest::Method::POST,
    }
}

#[cfg(test)]
mod tests {
    use axum::{Json, Router, routing::get};
    use tokio::net::TcpListener;

    use super::*;

    async fn status_handler() -> Json<ApiResponse<ForkingStatus>> {
        Json(ApiResponse {
            success: true,
            data: Some(ForkingStatus {
                checkpoint: 7,
                epoch: 3,
                clock_timestamp_ms: 1234,
            }),
            error: None,
        })
    }

    #[tokio::test]
    async fn status_calls_minimal_status_endpoint() {
        let app = Router::new().route("/status", get(status_handler));
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
        let addr = listener.local_addr().expect("local addr");

        let server = tokio::spawn(async move {
            axum::serve(listener, app).await.expect("server should run");
        });

        let client = ForkingClient::new(Url::parse(&format!("http://{addr}")).expect("url"));
        let status = client.status().await.expect("status");
        assert_eq!(status.checkpoint, 7);
        assert_eq!(status.epoch, 3);
        assert_eq!(status.clock_timestamp_ms, 1234);

        server.abort();
    }
}
