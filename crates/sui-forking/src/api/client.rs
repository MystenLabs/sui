// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Typed HTTP control client for a running `sui-forking` server.

use serde::Serialize;
use serde::de::DeserializeOwned;
use sui_types::base_types::SuiAddress;
use url::Url;

use crate::api::error::ClientError;
use crate::api::types::{AdvanceClockRequest, ApiResponse, ExecuteTxResponse, ForkingStatus};

/// Typed client for the local `sui-forking` control API.
#[derive(Clone, Debug)]
pub struct ForkingClient {
    base_url: Url,
    http: reqwest::Client,
}

impl ForkingClient {
    /// Create a client targeting the given control API base URL.
    ///
    /// # Examples
    ///
    /// ```rust
    /// # use sui_forking::ForkingClient;
    /// # use url::Url;
    /// # fn demo() -> Result<(), url::ParseError> {
    /// let client = ForkingClient::new(Url::parse("http://127.0.0.1:9001")?);
    /// # let _ = client;
    /// # Ok(())
    /// # }
    /// ```
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

    /// Return the configured base URL.
    pub fn base_url(&self) -> &Url {
        &self.base_url
    }

    /// Fetch the runtime status.
    ///
    /// # Errors
    ///
    /// Returns [`ClientError`] if the request fails or the server returns an error payload.
    pub async fn status(&self) -> Result<ForkingStatus, ClientError> {
        self.send_without_body::<ForkingStatus>(reqwest::Method::GET, "status")
            .await
    }

    /// Advance the local checkpoint by one.
    ///
    /// # Errors
    ///
    /// Returns [`ClientError`] if the request fails or the server returns an error payload.
    pub async fn advance_checkpoint(&self) -> Result<(), ClientError> {
        let _: String = self
            .send_without_body(reqwest::Method::POST, "advance-checkpoint")
            .await?;
        Ok(())
    }

    /// Advance the local clock by `ms`.
    ///
    /// # Errors
    ///
    /// Returns [`ClientError`] if the request fails or the server returns an error payload.
    pub async fn advance_clock(&self, ms: u64) -> Result<(), ClientError> {
        let request = AdvanceClockRequest { ms };
        let _: String = self.send_with_body("advance-clock", &request).await?;
        Ok(())
    }

    /// Advance the local epoch by one.
    ///
    /// # Errors
    ///
    /// Returns [`ClientError`] if the request fails or the server returns an error payload.
    pub async fn advance_epoch(&self) -> Result<(), ClientError> {
        let _: String = self
            .send_without_body(reqwest::Method::POST, "advance-epoch")
            .await?;
        Ok(())
    }

    /// Execute a faucet transfer.
    ///
    /// # Errors
    ///
    /// Returns [`ClientError`] if the request fails or the server returns an error payload.
    pub async fn faucet(
        &self,
        address: SuiAddress,
        amount: u64,
    ) -> Result<ExecuteTxResponse, ClientError> {
        #[derive(Serialize)]
        struct FaucetRequest {
            address: SuiAddress,
            amount: u64,
        }

        self.send_with_body("faucet", &FaucetRequest { address, amount })
            .await
    }

    async fn send_without_body<T>(
        &self,
        method: reqwest::Method,
        endpoint: &'static str,
    ) -> Result<T, ClientError>
    where
        T: DeserializeOwned,
    {
        let url = self.endpoint_url(endpoint)?;
        let response = self
            .http
            .request(method, url.clone())
            .send()
            .await
            .map_err(|source| ClientError::Transport {
                url: url.clone(),
                source,
            })?;
        self.decode_response(endpoint, url, response).await
    }

    async fn send_with_body<T, B>(&self, endpoint: &'static str, body: &B) -> Result<T, ClientError>
    where
        T: DeserializeOwned,
        B: Serialize + ?Sized,
    {
        let url = self.endpoint_url(endpoint)?;
        let response = self
            .http
            .post(url.clone())
            .json(body)
            .send()
            .await
            .map_err(|source| ClientError::Transport {
                url: url.clone(),
                source,
            })?;
        self.decode_response(endpoint, url, response).await
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
            return Err(http_status_error(url, status, body));
        }

        let envelope: ApiResponse<T> =
            response
                .json()
                .await
                .map_err(|source| ClientError::Decode {
                    url: url.clone(),
                    source,
                })?;
        extract_envelope_data(endpoint, envelope)
    }

    fn endpoint_url(&self, endpoint: &'static str) -> Result<Url, ClientError> {
        self.base_url
            .join(endpoint)
            .map_err(|source| ClientError::UrlJoin {
                path: endpoint.to_string(),
                source,
            })
    }
}

fn http_status_error(url: Url, status: reqwest::StatusCode, body: String) -> ClientError {
    ClientError::HttpStatus { url, status, body }
}

fn extract_envelope_data<T>(
    endpoint: &'static str,
    envelope: ApiResponse<T>,
) -> Result<T, ClientError> {
    if !envelope.success {
        return Err(ClientError::Api {
            message: envelope
                .error
                .unwrap_or_else(|| format!("endpoint '{endpoint}' reported failure")),
        });
    }

    envelope.data.ok_or(ClientError::MissingData { endpoint })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_success() {
        let envelope = ApiResponse {
            success: true,
            data: Some(ForkingStatus {
                checkpoint: 7,
                epoch: 3,
                clock_timestamp_ms: 1_000_000,
            }),
            error: None,
        };
        let status = extract_envelope_data("status", envelope).expect("status payload");
        assert_eq!(status.checkpoint, 7);
        assert_eq!(status.epoch, 3);
    }

    #[test]
    fn maps_http_failures() {
        let err = http_status_error(
            Url::parse("http://127.0.0.1:9001").expect("url"),
            reqwest::StatusCode::INTERNAL_SERVER_ERROR,
            "boom".to_string(),
        );
        assert!(matches!(err, ClientError::HttpStatus { .. }));
    }

    #[test]
    fn maps_api_failures() {
        let envelope: ApiResponse<String> = ApiResponse {
            success: false,
            data: None,
            error: Some("cannot advance epoch".to_string()),
        };
        let err = extract_envelope_data("advance-epoch", envelope).expect_err("api failure");
        assert!(matches!(err, ClientError::Api { .. }));
    }
}
