// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::{
    AppState, BatchFaucetResponse, BatchStatusFaucetResponse, FaucetConfig, FaucetError,
    FaucetRequest, FaucetResponse, RequestMetricsLayer,
};
use axum::{
    error_handling::HandleErrorLayer,
    extract::{ConnectInfo, Host, Path},
    http::{header::HeaderMap, StatusCode},
    response::{IntoResponse, Redirect},
    routing::{get, post},
    BoxError, Extension, Json, Router,
};
use http::{header::USER_AGENT, HeaderValue, Method};
use mysten_metrics::spawn_monitored_task;
use prometheus::Registry;
use std::{
    borrow::Cow,
    collections::BTreeMap,
    net::{IpAddr, SocketAddr},
    path::PathBuf,
    sync::Arc,
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use sui_config::SUI_CLIENT_CONFIG;
use sui_sdk::wallet_context::WalletContext;
use tower::ServiceBuilder;
use tower_governor::{
    governor::GovernorConfigBuilder, key_extractor::GlobalKeyExtractor, GovernorLayer,
};
use tower_http::cors::{Any, CorsLayer};
use tracing::{info, warn};
use uuid::Uuid;

use crate::faucet::Faucet;
use serde::Deserialize;
use std::sync::Mutex;

use anyhow::bail;
use lazy_static::lazy_static;
use std::env;

/// Interval to cleanup expired tokens
const CLEANUP_INTERVAL: u64 = 60; // 60 seconds
/// Maximum number of requests per IP address
const MAX_REQUESTS_PER_IP: u32 = 3;
/// Interval to reset the request count for each IP address
const RESET_TIME_INTERVAL_SECS: u64 = 12 * 3600; // 12 hours
const CLOUDFLARE_URL: &str = "https://challenges.cloudflare.com/turnstile/v0/siteverify";
const FAUCET_WEB_APP_URL: &str = "https://faucet.sui.io"; // make this lazy static env?

lazy_static! {
    static ref DISCORD_BOT: Option<String> = env::var("DISCORD_BOT").ok();
}

lazy_static! {
    static ref TURNSTILE_SECRET_KEY: Option<String> = env::var("TURNSTILE_SECRET_KEY").ok();
}

type IPAddr = String;

/// Keep track of every IP address' requests.
#[derive(Debug)]
struct RequestsManager {
    pub data: Mutex<BTreeMap<IPAddr, RequestInfo>>,
    reset_time_interval_secs: u64,
    max_requests_per_ip: u32,
}

/// Request's metadata
#[derive(Debug, Clone)]
struct RequestInfo {
    expires_at: u64,
    requests_used: u32,
    total_requests_allowed: u32,
}

/// Struct to deserialize token verification response from Cloudflare
#[derive(Deserialize, Debug)]
struct TokenVerification {
    success: bool,
    #[serde(rename = "error-codes")]
    error_codes: Vec<String>,
}

impl RequestsManager {
    /// Initialize a new RequestsManager the default values.
    fn new() -> Self {
        Self {
            data: Mutex::new(BTreeMap::new()),
            reset_time_interval_secs: RESET_TIME_INTERVAL_SECS,
            max_requests_per_ip: MAX_REQUESTS_PER_IP,
        }
    }

    #[cfg(test)]
    fn new_with_limits(max_requests_per_ip: u32, reset_time_interval_secs: u64) -> Self {
        Self {
            data: Mutex::new(BTreeMap::new()),
            reset_time_interval_secs,
            max_requests_per_ip,
        }
    }

    /// Get the current timestamp in seconds
    fn current_timestamp_secs() -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("Time went backwards")
            .as_secs()
    }

    /// Checks if the user-agent is present, and if it is the expected discord bot user-agent. This
    /// includes a unique password that the bot has set in the user-agent header.
    fn is_discord_bot(&self, user_agent: Option<&HeaderValue>) -> Result<bool, FaucetError> {
        if let (Some(discord_bot), Some(v)) = (DISCORD_BOT.as_ref(), user_agent) {
            let header = v
                .to_str()
                .map_err(|e| FaucetError::InvalidUserAgent(e.to_string()))?;
            Ok(header == format!("discord-bot-{}", discord_bot))
        } else {
            Ok(false)
        }
    }

    /// Validates a token
    /// - against Cloudflare turnstile's server to ensure token was issued by turnstile
    /// - against the IP address' request count
    async fn validate_token(
        &self,
        url: &str,
        addr: SocketAddr,
        token: &str,
    ) -> Result<(), (StatusCode, FaucetError)> {
        let turnstile_key = TURNSTILE_SECRET_KEY.as_ref().unwrap().as_str();
        let req = reqwest::Client::new();
        let params = [
            ("secret", turnstile_key),
            ("response", token),
            ("remoteip", &addr.ip().to_string()),
        ];

        // Make the POST request
        let response = req.post(url).form(&params).send().await;

        match response {
            Ok(resp) => {
                if resp.status().is_success() {
                    let body = resp.json::<TokenVerification>().await;
                    if let Ok(body) = body {
                        // if body success is false, that means that token verification failed
                        // either because the token is invalid or the token has already been used
                        if !body.success {
                            return Err((
                                StatusCode::BAD_REQUEST,
                                FaucetError::Internal(format!(
                                    "Token verification failed: {:?}",
                                    body.error_codes
                                )),
                            ));
                        }
                    }

                    let current_time = Self::current_timestamp_secs();

                    // Check if the IP address is already in the map
                    let mut locked_data = self.data.lock().unwrap();
                    let token_entry = locked_data.get_mut(&addr.ip().to_string());

                    if let Some(token_entry) = token_entry {
                        // Check IP address expiration time
                        if current_time > token_entry.expires_at {
                            locked_data.remove(token);
                            return Err((
                                StatusCode::BAD_REQUEST,
                                FaucetError::Internal("Token expired".to_string()),
                            ));
                        }

                        // Check request limit
                        if token_entry.requests_used >= token_entry.total_requests_allowed {
                            return Err((
                                StatusCode::TOO_MANY_REQUESTS,
                                FaucetError::TooManyRequests(format!(
                                    "You can request a new token in {}",
                                    secs_to_human_readable(token_entry.expires_at - current_time)
                                )),
                            ));
                        }
                        // Increment request count
                        token_entry.requests_used += 1;
                    } else {
                        // Create new token entry
                        let token_info = RequestInfo {
                            expires_at: current_time + self.reset_time_interval_secs, // 12 hours
                            requests_used: 1,
                            total_requests_allowed: self.max_requests_per_ip,
                        };
                        locked_data.insert(addr.ip().to_string(), token_info);
                    }
                } else {
                    return Err((
                        StatusCode::BAD_REQUEST,
                        FaucetError::Internal(format!("Invalid token")),
                    ));
                }
            }
            Err(e) => {
                return Err((
                    StatusCode::INTERNAL_SERVER_ERROR,
                    FaucetError::Internal(format!("Internal server error: {:?}", e)),
                ));
            }
        }
        Ok(())
    }

    /// This function iterates through the stored IPs and removes those IP addresses which are now
    /// eligible to make new requests.
    fn cleanup_expired_tokens(&self) {
        let current_time = Self::current_timestamp_secs();
        let mut data = self.data.lock().unwrap();

        // keep only those IP addresses that are still under time limit.
        data.retain(|_, info| current_time <= info.expires_at);
    }
}

pub async fn start_faucet(
    app_state: Arc<AppState>,
    concurrency_limit: usize,
    prometheus_registry: &Registry,
) -> Result<(), anyhow::Error> {
    if app_state.config.testnet && (DISCORD_BOT.is_none() || TURNSTILE_SECRET_KEY.is_none()) {
        bail!("Both DISCORD_BOT and TURNSTILE_SECRET_KEY env vars must be set for testnet deployment (--testnet flag was set)");
    }
    // TODO: restrict access if needed
    let cors = CorsLayer::new()
        .allow_methods(vec![Method::GET, Method::POST])
        .allow_headers(Any)
        .allow_origin(Any);

    let FaucetConfig {
        port,
        host_ip,
        request_buffer_size,
        max_request_per_second,
        wal_retry_interval,
        ..
    } = app_state.config;

    let governor_cfg = Arc::new(
        GovernorConfigBuilder::default()
            .burst_size(max_request_per_second as u32)
            .key_extractor(GlobalKeyExtractor)
            .finish()
            .unwrap(),
    );
    let token_manager = Arc::new(RequestsManager::new());

    let app = Router::new()
        .route("/", get(redirect))
        .route("/health", get(health))
        .route("/gas", post(request_gas))
        .route("/v1/gas", post(batch_request_gas))
        .route("/v1/status/:task_id", get(request_status))
        .layer(
            ServiceBuilder::new()
                .layer(HandleErrorLayer::new(handle_error))
                .layer(RequestMetricsLayer::new(prometheus_registry))
                .layer(cors)
                .load_shed()
                .buffer(request_buffer_size)
                .layer(GovernorLayer {
                    config: governor_cfg,
                })
                .concurrency_limit(concurrency_limit)
                .layer(Extension(app_state.clone()))
                .layer(Extension(token_manager.clone()))
                .into_inner(),
        );

    spawn_monitored_task!(async move {
        info!("Starting task to clear WAL.");
        loop {
            // Every config.wal_retry_interval (Default: 300 seconds) we try to clear the wal coins
            tokio::time::sleep(Duration::from_secs(wal_retry_interval)).await;
            app_state.faucet.retry_wal_coins().await.unwrap();
        }
    });

    spawn_monitored_task!(async move {
        info!("Starting task to clear banned ip addresses.");
        loop {
            tokio::time::sleep(Duration::from_secs(CLEANUP_INTERVAL)).await;
            token_manager.cleanup_expired_tokens();
        }
    });

    let addr = SocketAddr::new(IpAddr::V4(host_ip), port);
    info!("listening on {}", addr);
    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .await?;
    Ok(())
}

/// basic handler that responds with a static string
async fn health() -> &'static str {
    "OK"
}

/// Redirect to faucet.sui.io/?network if it's testnet/devnet network
async fn redirect(Host(host): Host) -> impl IntoResponse {
    if host.contains("testnet") {
        let redirect = Redirect::to(&format!("{FAUCET_WEB_APP_URL}/?network=testnet"));
        redirect.into_response()
    } else if host.contains("devnet") {
        let redirect = Redirect::to(&format!("{FAUCET_WEB_APP_URL}/?network=devnet"));
        redirect.into_response()
    } else {
        health().await.into_response()
    }
}

/// handler for batch_request_gas requests
async fn batch_request_gas(
    headers: HeaderMap,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    Extension(token_manager): Extension<Arc<RequestsManager>>,
    Extension(state): Extension<Arc<AppState>>,
    Json(payload): Json<FaucetRequest>,
) -> impl IntoResponse {
    let id = Uuid::new_v4();
    // ID for traceability
    info!(uuid = ?id, "Got new gas request.");

    // If this service is running for testnet and it is not the discord bot, users need to use the
    // WebUI to request tokens and we need to validate the CloudFlare turnstile token here

    if state.config.testnet {
        // Check if the user-agent is present, and if it is the expected discord bot user-agent.
        let is_discord_bot = match token_manager.is_discord_bot(headers.get(USER_AGENT)) {
            Ok(bot) => bot,
            Err(err) => {
                return (
                    StatusCode::BAD_REQUEST,
                    Json(BatchFaucetResponse::from(err)),
                );
            }
        };

        if !is_discord_bot {
            let Some(token) = headers
                .get("X-Turnstile-Token")
                .and_then(|v| v.to_str().ok())
            else {
                return (
                    StatusCode::BAD_REQUEST,
                    Json(BatchFaucetResponse::from(FaucetError::NoToken)),
                );
            };

            let validation = token_manager
                .validate_token(CLOUDFLARE_URL, addr, token)
                .await;
            if let Err((status_code, faucet_error)) = validation {
                return (status_code, Json(BatchFaucetResponse::from(faucet_error)));
            }
        }
    }

    let FaucetRequest::FixedAmountRequest(request) = payload else {
        return (
            StatusCode::BAD_REQUEST,
            Json(BatchFaucetResponse::from(FaucetError::Internal(
                "Input Error.".to_string(),
            ))),
        );
    };

    if state.config.batch_enabled {
        let result = spawn_monitored_task!(async move {
            state
                .faucet
                .batch_send(
                    id,
                    request.recipient,
                    &vec![state.config.amount; state.config.num_coins],
                )
                .await
        })
        .await
        .unwrap();

        match result {
            Ok(v) => {
                info!(uuid =?id, "Request is successfully served");
                (StatusCode::ACCEPTED, Json(BatchFaucetResponse::from(v)))
            }
            Err(v) => {
                warn!(uuid =?id, "Failed to request gas: {:?}", v);
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(BatchFaucetResponse::from(v)),
                )
            }
        }
    } else {
        // TODO (jian): remove this feature gate when batch has proven to be baked long enough
        info!(uuid = ?id, "Falling back to v1 implementation");
        let result = spawn_monitored_task!(async move {
            state
                .faucet
                .send(
                    id,
                    request.recipient,
                    &vec![state.config.amount; state.config.num_coins],
                )
                .await
        })
        .await
        .unwrap();

        match result {
            Ok(_) => {
                info!(uuid =?id, "Request is successfully served");
                (StatusCode::ACCEPTED, Json(BatchFaucetResponse::from(id)))
            }
            Err(v) => {
                warn!(uuid =?id, "Failed to request gas: {:?}", v);
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(BatchFaucetResponse::from(v)),
                )
            }
        }
    }
}

/// handler for batch_get_status requests
async fn request_status(
    Extension(state): Extension<Arc<AppState>>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    match Uuid::parse_str(&id) {
        Ok(task_id) => {
            let result = state.faucet.get_batch_send_status(task_id).await;
            match result {
                Ok(v) => (
                    StatusCode::CREATED,
                    Json(BatchStatusFaucetResponse::from(v)),
                ),
                Err(v) => (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(BatchStatusFaucetResponse::from(v)),
                ),
            }
        }
        Err(e) => (
            StatusCode::BAD_REQUEST,
            Json(BatchStatusFaucetResponse::from(FaucetError::Internal(
                e.to_string(),
            ))),
        ),
    }
}

/// handler for all the request_gas requests
async fn request_gas(
    headers: HeaderMap,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    Extension(token_manager): Extension<Arc<RequestsManager>>,
    Extension(state): Extension<Arc<AppState>>,
    Json(payload): Json<FaucetRequest>,
) -> impl IntoResponse {
    // ID for traceability
    let id = Uuid::new_v4();
    info!(uuid = ?id, "Got new gas request.");

    if state.config.testnet {
        // Check if the user-agent is present, and if it is the expected discord bot user-agent.
        let is_discord_bot = match token_manager.is_discord_bot(headers.get(USER_AGENT)) {
            Ok(bot) => bot,
            Err(err) => {
                return (StatusCode::BAD_REQUEST, Json(FaucetResponse::from(err)));
            }
        };

        if !is_discord_bot {
            let Some(token) = headers
                .get("X-Turnstile-Token")
                .and_then(|v| v.to_str().ok())
            else {
                return (
                    StatusCode::BAD_REQUEST,
                    Json(FaucetResponse::from(FaucetError::NoToken)),
                );
            };

            let validation = token_manager
                .validate_token(CLOUDFLARE_URL, addr, token)
                .await;
            if let Err((status_code, faucet_error)) = validation {
                return (status_code, Json(FaucetResponse::from(faucet_error)));
            }
        }
    }

    let result = match payload {
        FaucetRequest::FixedAmountRequest(requests) => {
            // We spawn a tokio task for this such that connection drop will not interrupt
            // it and impact the recycling of coins
            spawn_monitored_task!(async move {
                state
                    .faucet
                    .send(
                        id,
                        requests.recipient,
                        &vec![state.config.amount; state.config.num_coins],
                    )
                    .await
            })
            .await
            .unwrap()
        }
        _ => {
            return (
                StatusCode::BAD_REQUEST,
                Json(FaucetResponse::from(FaucetError::Internal(
                    "Input Error.".to_string(),
                ))),
            )
        }
    };
    match result {
        Ok(v) => {
            info!(uuid =?id, "Request is successfully served");
            (StatusCode::CREATED, Json(FaucetResponse::from(v)))
        }
        Err(v) => {
            warn!(uuid =?id, "Failed to request gas: {:?}", v);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(FaucetResponse::from(v)),
            )
        }
    }
}

pub fn create_wallet_context(
    timeout_secs: u64,
    config_dir: PathBuf,
) -> Result<WalletContext, anyhow::Error> {
    let wallet_conf = config_dir.join(SUI_CLIENT_CONFIG);
    info!("Initialize wallet from config path: {:?}", wallet_conf);
    WalletContext::new(
        &wallet_conf,
        Some(Duration::from_secs(timeout_secs)),
        Some(1000),
    )
}

async fn handle_error(error: BoxError) -> impl IntoResponse {
    if error.is::<tower::load_shed::error::Overloaded>() {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            Cow::from("service is overloaded, please try again later"),
        );
    }

    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Cow::from(format!("Unhandled internal error: {}", error)),
    )
}

/// Format seconds to human readable format.
fn secs_to_human_readable(total_seconds: u64) -> String {
    let hours = total_seconds / 3600;
    let minutes = (total_seconds % 3600) / 60;
    let seconds = total_seconds % 60;

    if hours > 0 {
        format!("{}h {}m {}s", hours, minutes, seconds)
    } else if minutes > 0 {
        format!("{}m {}s", minutes, seconds)
    } else {
        format!("{}s", seconds)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::net::{IpAddr, Ipv4Addr};
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    async fn setup_mock_cloudflare() -> MockServer {
        std::env::set_var("TURNSTILE_SECRET_KEY", "test_secret");
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(200))
            .mount(&mock_server)
            .await;

        mock_server
    }

    #[tokio::test]
    async fn test_token_validation_and_limits() {
        // Start mock server
        let mock_server = setup_mock_cloudflare().await;
        let manager = RequestsManager::new();
        let ip = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 8080);
        let token = "test_token";

        // First request should succeed
        let result = manager.validate_token(&mock_server.uri(), ip, token).await;
        assert!(result.is_ok());

        // Use up remaining requests
        for _ in 1..manager.max_requests_per_ip {
            let result = manager.validate_token(&mock_server.uri(), ip, token).await;
            assert!(result.is_ok());
        }

        // Next request should fail due to limit
        let result = manager.validate_token(&mock_server.uri(), ip, token).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_token_reset_after_interval() {
        let mock_server = setup_mock_cloudflare().await;
        let reset_time_interval_secs = 5; // seconds for testing
        let manager =
            RequestsManager::new_with_limits(MAX_REQUESTS_PER_IP, reset_time_interval_secs);

        let ip = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 8080);
        let token = "test_token";

        // Use up all requests
        for _ in 0..manager.max_requests_per_ip {
            let result = manager.validate_token(&mock_server.uri(), ip, token).await;
            assert!(result.is_ok());
        }

        // Try one more, it should fail
        let result = manager.validate_token(&mock_server.uri(), ip, token).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().0 == StatusCode::TOO_MANY_REQUESTS);
        assert!(!manager.data.lock().unwrap().is_empty());

        tokio::time::sleep(Duration::from_secs(reset_time_interval_secs + 1)).await;
        // Trigger cleanup
        manager.cleanup_expired_tokens();

        // Should be able to make new requests
        let result = manager.validate_token(&mock_server.uri(), ip, token).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_invalid_token_response() {
        let mock_server = MockServer::start().await;
        std::env::set_var("TURNSTILE_SECRET_KEY", "test_secret");

        // Setup mock for invalid token
        Mock::given(method("POST"))
            .and(path("/siteverify"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "success": false,
                "error-codes": ["invalid-input-response"]
            })))
            .mount(&mock_server)
            .await;

        let manager = RequestsManager::new();
        let ip = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 8080);
        let token = "invalid_token";

        let result = manager.validate_token(&mock_server.uri(), ip, token).await;
        assert!(result.is_err());
    }
}
