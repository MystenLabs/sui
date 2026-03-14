// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Client API for interacting with a running `sui-forking` server.

use serde::Serialize;
use serde::de::DeserializeOwned;
use sui_types::base_types::SuiAddress;
use url::Url;

use crate::api::endpoints::{self, Endpoint, EndpointMethod};
use crate::api::error::ClientError;
use crate::api::types::{
    AdvanceClockRequest, ApiResponse, ExecuteTxResponse, FaucetRequest, ForkingStatus,
};

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
    pub async fn status(&self) -> Result<ForkingStatus, ClientError> {
        self.send_without_body::<ForkingStatus>(endpoints::STATUS)
            .await
    }

    /// Advance the local checkpoint by one.
    pub async fn advance_checkpoint(&self) -> Result<(), ClientError> {
        let _: String = self
            .send_without_body(endpoints::ADVANCE_CHECKPOINT)
            .await?;
        Ok(())
    }

    /// Advance the local clock by `ms`.
    pub async fn advance_clock(&self, ms: u64) -> Result<(), ClientError> {
        let request = AdvanceClockRequest { ms };
        let _: String = self
            .send_with_body(endpoints::ADVANCE_CLOCK, &request)
            .await?;
        Ok(())
    }

    /// Advance the local epoch by one.
    pub async fn advance_epoch(&self) -> Result<(), ClientError> {
        let _: String = self.send_without_body(endpoints::ADVANCE_EPOCH).await?;
        Ok(())
    }

    /// Execute a faucet transfer to the given address for the given amount.
    pub async fn faucet(
        &self,
        address: SuiAddress,
        amount: u64,
    ) -> Result<ExecuteTxResponse, ClientError> {
        self.send_with_body(endpoints::FAUCET, &FaucetRequest { address, amount })
            .await
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

    async fn send_with_body<T, B>(&self, endpoint: Endpoint, body: &B) -> Result<T, ClientError>
    where
        T: DeserializeOwned,
        B: Serialize + ?Sized,
    {
        let url = self.endpoint_url(endpoint)?;
        let response = self
            .http
            .request(reqwest_method(endpoint.method), url.clone())
            .json(body)
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
    use std::{str::FromStr, sync::Arc};

    use axum::{
        Json, Router,
        extract::State,
        routing::{get, post},
    };
    use tokio::{net::TcpListener, sync::Mutex};

    use super::*;

    #[derive(Clone, Debug, PartialEq, Eq)]
    enum RecordedRequest {
        Status,
        AdvanceCheckpoint,
        AdvanceClock(u64),
        AdvanceEpoch,
        Faucet { address: SuiAddress, amount: u64 },
    }

    type RequestLog = Arc<Mutex<Vec<RecordedRequest>>>;

    fn ok_envelope<T>(data: T) -> ApiResponse<T> {
        ApiResponse {
            success: true,
            data: Some(data),
            error: None,
        }
    }

    async fn status_handler(State(log): State<RequestLog>) -> Json<ApiResponse<ForkingStatus>> {
        log.lock().await.push(RecordedRequest::Status);
        Json(ok_envelope(ForkingStatus {
            checkpoint: 42,
            epoch: 9,
            clock_timestamp_ms: 1_234_567,
        }))
    }

    async fn advance_checkpoint_handler(
        State(log): State<RequestLog>,
    ) -> Json<ApiResponse<String>> {
        log.lock().await.push(RecordedRequest::AdvanceCheckpoint);
        Json(ok_envelope("advanced".to_string()))
    }

    async fn advance_clock_handler(
        State(log): State<RequestLog>,
        Json(request): Json<AdvanceClockRequest>,
    ) -> Json<ApiResponse<String>> {
        log.lock()
            .await
            .push(RecordedRequest::AdvanceClock(request.ms));
        Json(ok_envelope("clock".to_string()))
    }

    async fn advance_epoch_handler(State(log): State<RequestLog>) -> Json<ApiResponse<String>> {
        log.lock().await.push(RecordedRequest::AdvanceEpoch);
        Json(ok_envelope("epoch".to_string()))
    }

    async fn faucet_handler(
        State(log): State<RequestLog>,
        Json(request): Json<FaucetRequest>,
    ) -> Json<ApiResponse<ExecuteTxResponse>> {
        let FaucetRequest { address, amount } = request;
        log.lock()
            .await
            .push(RecordedRequest::Faucet { address, amount });
        Json(ok_envelope(ExecuteTxResponse {
            effects: "effects-base64".to_string(),
            error: None,
        }))
    }

    async fn spawn_contract_server(log: RequestLog) -> (Url, tokio::task::JoinHandle<()>) {
        let app = Router::new()
            .route(endpoints::STATUS.path, get(status_handler))
            .route(
                endpoints::ADVANCE_CHECKPOINT.path,
                post(advance_checkpoint_handler),
            )
            .route(endpoints::ADVANCE_CLOCK.path, post(advance_clock_handler))
            .route(endpoints::ADVANCE_EPOCH.path, post(advance_epoch_handler))
            .route(endpoints::FAUCET.path, post(faucet_handler))
            .with_state(log);

        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind test listener");
        let addr = listener.local_addr().expect("listener address");
        let server = tokio::spawn(async move {
            axum::serve(listener, app).await.expect("serve test app");
        });

        let url = Url::parse(&format!("http://{addr}")).expect("valid test base url");
        (url, server)
    }

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

    #[tokio::test]
    async fn client_matches_control_api_contract() {
        let log: RequestLog = Arc::new(Mutex::new(Vec::new()));
        let (base_url, server) = spawn_contract_server(log.clone()).await;
        let client = ForkingClient::new(base_url);

        let status = client.status().await.expect("status call");
        assert_eq!(
            status,
            ForkingStatus {
                checkpoint: 42,
                epoch: 9,
                clock_timestamp_ms: 1_234_567,
            }
        );
        client
            .advance_checkpoint()
            .await
            .expect("advance checkpoint call");
        client.advance_clock(777).await.expect("advance clock call");
        client.advance_epoch().await.expect("advance epoch call");

        let faucet_address = SuiAddress::from_str(
            "0x1111111111111111111111111111111111111111111111111111111111111111",
        )
        .expect("valid address");
        let faucet_response = client
            .faucet(faucet_address, 5_000)
            .await
            .expect("faucet call");
        assert_eq!(
            faucet_response,
            ExecuteTxResponse {
                effects: "effects-base64".to_string(),
                error: None,
            }
        );

        let expected_address = SuiAddress::from_str(
            "0x1111111111111111111111111111111111111111111111111111111111111111",
        )
        .expect("valid address");
        assert_eq!(
            &*log.lock().await,
            &[
                RecordedRequest::Status,
                RecordedRequest::AdvanceCheckpoint,
                RecordedRequest::AdvanceClock(777),
                RecordedRequest::AdvanceEpoch,
                RecordedRequest::Faucet {
                    address: expected_address,
                    amount: 5_000,
                },
            ]
        );

        server.abort();
        let _ = server.await;
    }
}
