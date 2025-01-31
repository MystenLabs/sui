// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::{
    AppState, BatchFaucetResponse, BatchStatusFaucetResponse, FaucetConfig, FaucetError,
    FaucetRequest, FaucetResponse, FixedAmountRequest, RequestMetricsLayer,
};
use axum::{
    error_handling::HandleErrorLayer,
    extract::{ConnectInfo, Host, Path},
    http::{header::HeaderMap, StatusCode},
    response::{IntoResponse, Redirect, Response},
    routing::{get, post},
    BoxError, Extension, Json, Router,
};
use http::Method;
use mysten_metrics::spawn_monitored_task;
use prometheus::Registry;
use std::{
    borrow::Cow,
    net::{IpAddr, SocketAddr},
    path::PathBuf,
    sync::Arc,
    time::{Duration, Instant},
};
use sui_config::SUI_CLIENT_CONFIG;
use sui_sdk::wallet_context::WalletContext;
use tower::ServiceBuilder;
use tower_governor::{
    governor::GovernorConfigBuilder, key_extractor::GlobalKeyExtractor, GovernorLayer,
};
use tower_http::cors::{Any, CorsLayer};
use tracing::{error, info, warn};
use uuid::Uuid;

use crate::faucet::Faucet;
use dashmap::{mapref::entry::Entry, DashMap};
use serde::Deserialize;

use anyhow::ensure;
use once_cell::sync::Lazy;

const DEFAULT_FAUCET_WEB_APP_URL: &str = "https://faucet.sui.io";

static FAUCET_WEB_APP_URL: Lazy<String> = Lazy::new(|| {
    std::env::var("FAUCET_WEB_APP_URL")
        .ok()
        .unwrap_or_else(|| DEFAULT_FAUCET_WEB_APP_URL.to_string())
});

static CLOUDFLARE_TURNSTILE_URL: Lazy<Option<String>> =
    Lazy::new(|| std::env::var("CLOUDFLARE_TURNSTILE_URL").ok());

static TURNSTILE_SECRET_KEY: Lazy<Option<String>> =
    Lazy::new(|| std::env::var("TURNSTILE_SECRET_KEY").ok());

static DISCORD_BOT_PWD: Lazy<String> =
    Lazy::new(|| std::env::var("DISCORD_BOT_PWD").unwrap_or_else(|_| "".to_string()));

/// Keep track of every IP address' requests.
#[derive(Debug)]
struct RequestsManager {
    data: Arc<DashMap<IpAddr, RequestInfo>>,
    reset_time_interval: Duration,
    max_requests_per_ip: u64,
    cloudflare_turnstile_url: String,
    turnstile_secret_key: String,
}

/// Request's metadata
#[derive(Debug, Clone)]
struct RequestInfo {
    /// When the first request from this IP address was made. In case of resetting the IP addresses
    /// metadata, this field will be updated with the new current time.
    timestamp: Instant,
    requests_used: u64,
}

/// Struct to deserialize token verification response from Cloudflare
#[derive(Deserialize, Debug)]
struct TurnstileValidationResponse {
    success: bool,
    #[serde(rename = "error-codes")]
    error_codes: Vec<String>,
}

impl RequestsManager {
    /// Initialize a new RequestsManager
    fn new(
        max_requests_per_ip: u64,
        reset_time_interval_secs: Duration,
        cloudflare_turnstile_url: String,
        turnstile_secret_key: String,
    ) -> Self {
        Self {
            data: Arc::new(DashMap::new()),
            reset_time_interval: reset_time_interval_secs,
            max_requests_per_ip,
            cloudflare_turnstile_url,
            turnstile_secret_key,
        }
    }

    /// Validates a turnstile token
    /// - against Cloudflare turnstile's server to ensure token was issued by turnstile
    /// - against the IP address' request count
    async fn validate_turnstile_token(
        &self,
        addr: SocketAddr,
        token: &str,
    ) -> Result<(), (StatusCode, FaucetError)> {
        let ip = addr.ip();
        let req = reqwest::Client::new();
        let params = [
            ("secret", self.turnstile_secret_key.as_str()),
            ("response", token),
            ("remoteip", &ip.to_string()),
        ];

        // Make the POST request
        let resp = match req
            .post(&self.cloudflare_turnstile_url)
            .form(&params)
            .send()
            .await
        {
            Ok(resp) => resp,
            Err(e) => {
                error!("Cloudflare turnstile request failed: {:?}", e);
                return Err((
                    StatusCode::INTERNAL_SERVER_ERROR,
                    FaucetError::Internal(e.to_string()),
                ));
            }
        };

        // Check if the request was successful.
        if !resp.status().is_success() {
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                FaucetError::Internal("Verification failed".to_string()),
            ));
        }

        let body = match resp.json::<TurnstileValidationResponse>().await {
            Ok(body) => body,
            Err(e) => {
                error!("Failed to parse token validation response: {:?}", e);
                return Err((
                    StatusCode::INTERNAL_SERVER_ERROR,
                    FaucetError::Internal(e.to_string()),
                ));
            }
        };

        if !body.success {
            return Err((
                StatusCode::BAD_REQUEST,
                FaucetError::Internal(format!("Token verification failed: {:?}", body.error_codes)),
            ));
        }

        match self.data.entry(ip) {
            Entry::Vacant(entry) => {
                entry.insert(RequestInfo {
                    timestamp: Instant::now(),
                    requests_used: 1,
                });
            }

            Entry::Occupied(mut entry) => {
                let token = entry.get_mut();
                let elapsed = token.timestamp.elapsed();

                if elapsed >= self.reset_time_interval {
                    token.timestamp = Instant::now();
                    token.requests_used = 1;
                } else if token.requests_used >= self.max_requests_per_ip {
                    return Err((
                        StatusCode::TOO_MANY_REQUESTS,
                        FaucetError::TooManyRequests(format!(
                            "You can request a new token in {}",
                            secs_to_human_readable((self.reset_time_interval - elapsed).as_secs())
                        )),
                    ));
                } else {
                    token.requests_used += 1;
                }
            }
        }

        Ok(())
    }

    /// This function iterates through the stored IPs and removes those IP addresses which are now
    /// eligible to make new requests.
    fn cleanup_expired_tokens(&self) {
        // keep only those IP addresses that are still under time limit.
        self.data
            .retain(|_, info| info.timestamp.elapsed() < self.reset_time_interval);
    }
}

pub async fn start_faucet(
    app_state: Arc<AppState>,
    concurrency_limit: usize,
    prometheus_registry: &Registry,
) -> Result<(), anyhow::Error> {
    let (cloudflare_turnstile_url, turnstile_secret_key) = if app_state.config.authenticated {
        ensure!(TURNSTILE_SECRET_KEY.is_some() && CLOUDFLARE_TURNSTILE_URL.is_some(),
                "Both CLOUDFLARE_TURNSTILE_URL and TURNSTILE_SECRET_KEY env vars must be set for testnet deployment (--authenticated flag was set)");

        (
            CLOUDFLARE_TURNSTILE_URL.as_ref().unwrap().to_string(),
            TURNSTILE_SECRET_KEY.as_ref().unwrap().to_string(),
        )
    } else {
        ("".to_string(), "".to_string())
    };

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
        replenish_quota_interval_ms,
        reset_time_interval_secs,
        rate_limiter_cleanup_interval_secs,
        max_requests_per_ip,
        ..
    } = app_state.config;

    let token_manager = Arc::new(RequestsManager::new(
        max_requests_per_ip,
        Duration::from_secs(reset_time_interval_secs),
        cloudflare_turnstile_url,
        turnstile_secret_key,
    ));

    let governor_cfg = Arc::new(
        GovernorConfigBuilder::default()
            .const_per_millisecond(replenish_quota_interval_ms)
            .burst_size(max_request_per_second as u32)
            .key_extractor(GlobalKeyExtractor)
            .finish()
            .unwrap(),
    );

    // these routes have a more aggressive rate limit to reduce the number of reqs per second as
    // per the governor config above.
    let global_limited_routes = Router::new()
        .route("/gas", post(request_gas))
        .route("/v1/gas", post(batch_request_gas))
        .layer(GovernorLayer {
            config: governor_cfg.clone(),
        });

    // This has its own rate limiter via the RequestManager
    let faucet_web_routes = Router::new().route("/v1/faucet_web_gas", post(batch_faucet_web_gas));
    // Routes with no rate limit
    let unrestricted_routes = Router::new()
        .route("/", get(redirect))
        .route("/health", get(health))
        .route("/v1/faucet_discord", post(batch_faucet_discord))
        .route("/v1/status/:task_id", get(request_status));

    // Combine all routes
    let app = Router::new()
        .merge(global_limited_routes)
        .merge(unrestricted_routes)
        .merge(faucet_web_routes)
        .layer(
            ServiceBuilder::new()
                .layer(HandleErrorLayer::new(handle_error))
                .layer(RequestMetricsLayer::new(prometheus_registry))
                .load_shed()
                .buffer(request_buffer_size)
                .concurrency_limit(concurrency_limit)
                .layer(Extension(app_state.clone()))
                .layer(Extension(token_manager.clone()))
                .layer(cors)
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
            tokio::time::sleep(Duration::from_secs(rate_limiter_cleanup_interval_secs)).await;
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

/// Redirect to faucet.sui.io/?network if it's testnet/devnet network. For local network, keep the
/// previous behavior to return health status.
async fn redirect(Host(host): Host) -> Response {
    let url = FAUCET_WEB_APP_URL.to_string();
    if host.contains("testnet") {
        let redirect = Redirect::to(&format!("{url}/?network=testnet"));
        redirect.into_response()
    } else if host.contains("devnet") {
        let redirect = Redirect::to(&format!("{url}/?network=devnet"));
        redirect.into_response()
    } else {
        health().await.into_response()
    }
}

/// A route for requests coming from the discord bot.
async fn batch_faucet_discord(
    headers: HeaderMap,
    Extension(state): Extension<Arc<AppState>>,
    Json(payload): Json<FaucetRequest>,
) -> impl IntoResponse {
    if state.config.authenticated {
        let Some(agent_value) = headers
            .get(reqwest::header::USER_AGENT)
            .and_then(|v| v.to_str().ok())
        else {
            return (
                StatusCode::BAD_REQUEST,
                Json(BatchFaucetResponse::from(FaucetError::InvalidUserAgent(
                    "Invalid user agent for this route".to_string(),
                ))),
            );
        };

        if agent_value != *DISCORD_BOT_PWD {
            return (
                StatusCode::BAD_REQUEST,
                Json(BatchFaucetResponse::from(FaucetError::InvalidUserAgent(
                    "Invalid user agent for this route".to_string(),
                ))),
            );
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

    batch_request_spawn_task(request, state).await
}

/// Handler for requests coming from the frontend faucet web app.
async fn batch_faucet_web_gas(
    headers: HeaderMap,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    Extension(token_manager): Extension<Arc<RequestsManager>>,
    Extension(state): Extension<Arc<AppState>>,
    Json(payload): Json<FaucetRequest>,
) -> impl IntoResponse {
    if state.config.authenticated {
        let Some(token) = headers
            .get("X-Turnstile-Token")
            .and_then(|v| v.to_str().ok())
        else {
            return (
                StatusCode::BAD_REQUEST,
                Json(BatchFaucetResponse::from(
                    FaucetError::MissingTurnstileTokenHeader,
                )),
            );
        };

        let validation = token_manager.validate_turnstile_token(addr, token).await;

        if let Err((status_code, faucet_error)) = validation {
            return (status_code, Json(BatchFaucetResponse::from(faucet_error)));
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

    batch_request_spawn_task(request, state).await
}

// helper method
async fn batch_request_spawn_task(
    request: FixedAmountRequest,
    state: Arc<AppState>,
) -> (StatusCode, Json<BatchFaucetResponse>) {
    let result = spawn_monitored_task!(async move {
        state
            .faucet
            .batch_send(
                Uuid::new_v4(),
                request.recipient,
                &vec![state.config.amount; state.config.num_coins],
            )
            .await
    })
    .await
    .unwrap();
    match result {
        Ok(v) => (StatusCode::ACCEPTED, Json(BatchFaucetResponse::from(v))),
        Err(v) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(BatchFaucetResponse::from(v)),
        ),
    }
}

/// handler for batch_request_gas requests
async fn batch_request_gas(
    Extension(state): Extension<Arc<AppState>>,
    Json(payload): Json<FaucetRequest>,
) -> impl IntoResponse {
    let id = Uuid::new_v4();
    // ID for traceability
    info!(uuid = ?id, "Got new gas request.");

    let FaucetRequest::FixedAmountRequest(request) = payload else {
        return (
            StatusCode::BAD_REQUEST,
            Json(BatchFaucetResponse::from(FaucetError::Internal(
                "Input Error.".to_string(),
            ))),
        );
    };

    if state.config.batch_enabled {
        batch_request_spawn_task(request, state).await
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
    Extension(state): Extension<Arc<AppState>>,
    Json(payload): Json<FaucetRequest>,
) -> impl IntoResponse {
    // ID for traceability
    let id = Uuid::new_v4();
    info!(uuid = ?id, "Got new gas request.");

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
fn secs_to_human_readable(seconds: u64) -> String {
    let hours = seconds / 3600;
    let minutes = (seconds % 3600) / 60;
    let seconds = seconds % 60;

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
    use std::time::Duration;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    const MAX_REQUESTS_PER_IP: u64 = 3;
    const RESET_TIME_INTERVAL: Duration = Duration::from_secs(5);

    async fn setup_mock_cloudflare() -> MockServer {
        let mock_server = MockServer::start().await;

        Mock::given(method("POST"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(json!({ "success": true, "error-codes": [] })),
            )
            .mount(&mock_server)
            .await;

        mock_server
    }

    #[tokio::test]
    async fn test_token_validation_and_limits() {
        // Start mock server
        let mock_server = setup_mock_cloudflare().await;
        let manager = RequestsManager::new(
            MAX_REQUESTS_PER_IP,
            RESET_TIME_INTERVAL,
            mock_server.uri(),
            "test_secret".to_string(),
        );
        let ip = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 8080);
        let token = "test_token";

        // First request should succeed
        let result = manager.validate_turnstile_token(ip, token).await;
        assert!(result.is_ok());

        // Use up remaining requests
        for _ in 1..manager.max_requests_per_ip {
            let result = manager.validate_turnstile_token(ip, token).await;
            assert!(result.is_ok());
        }

        // Next request should fail due to limit
        let result = manager.validate_turnstile_token(ip, token).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_token_reset_after_interval() {
        let mock_server = setup_mock_cloudflare().await;
        let manager = RequestsManager::new(
            MAX_REQUESTS_PER_IP,
            RESET_TIME_INTERVAL,
            mock_server.uri(),
            "test_secret".to_string(),
        );

        let ip = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 8080);
        let token = "test_token";

        // Use up all requests
        for _ in 0..manager.max_requests_per_ip {
            let result = manager.validate_turnstile_token(ip, token).await;
            assert!(result.is_ok());
        }

        // Try one more, it should fail
        let result = manager.validate_turnstile_token(ip, token).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().0 == StatusCode::TOO_MANY_REQUESTS);
        assert!(!manager.data.is_empty());

        tokio::time::sleep(RESET_TIME_INTERVAL + Duration::from_secs(3)).await;
        // Trigger cleanup
        manager.cleanup_expired_tokens();

        // Should be able to make new requests
        let result = manager.validate_turnstile_token(ip, token).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_invalid_token_response() {
        let mock_server = MockServer::start().await;

        // Setup mock for invalid token
        Mock::given(method("POST"))
            .and(path("/siteverify"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "success": false,
                "error-codes": ["invalid-input-response"]
            })))
            .mount(&mock_server)
            .await;

        let manager = RequestsManager::new(
            MAX_REQUESTS_PER_IP,
            RESET_TIME_INTERVAL,
            mock_server.uri(),
            "test_secret".to_string(),
        );
        let ip = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 8080);
        let token = "invalid_token";

        let result = manager.validate_turnstile_token(ip, token).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_concurrent_ip_requests() {
        let mock_server = setup_mock_cloudflare().await;
        let manager = Arc::new(RequestsManager::new(
            MAX_REQUESTS_PER_IP,
            RESET_TIME_INTERVAL,
            mock_server.uri(),
            "test_secret".to_string(),
        ));

        // Create 10 different IP addresses
        let ips: Vec<SocketAddr> = (0..10)
            .map(|i| SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, i as u8)), 8080))
            .collect();

        let token = "test_token";

        // Spawn tasks for each IP to make requests concurrently
        let mut handles = vec![];

        for (idx, &ip) in ips.iter().enumerate() {
            let manager = manager.clone();
            let handle = tokio::spawn(async move {
                // Add some random delay to simulate real-world conditions
                tokio::time::sleep(Duration::from_millis(idx as u64 * 50)).await;

                let mut results = vec![];
                // Each IP tries to make MAX_REQUESTS_PER_IP + 1 requests
                for _ in 0..=MAX_REQUESTS_PER_IP {
                    let result = manager.validate_turnstile_token(ip, token).await;
                    results.push(result);
                }
                (ip, results)
            });
            handles.push(handle);
        }

        // Wait for all tasks to complete and check results
        let all_results = futures::future::join_all(handles).await;

        for result in all_results {
            let (ip, results) = result.unwrap();

            // First MAX_REQUESTS_PER_IP requests should succeed
            for (idx, _) in results
                .iter()
                .enumerate()
                .take(MAX_REQUESTS_PER_IP as usize)
            {
                assert!(
                    results[idx].is_ok(),
                    "Request {} for IP {} should succeed",
                    idx,
                    ip
                );
            }

            // The last request (MAX_REQUESTS_PER_IP + 1) should fail
            assert!(
                results[MAX_REQUESTS_PER_IP as usize].is_err(),
                "Request {} for IP {} should fail",
                MAX_REQUESTS_PER_IP,
                ip
            );
        }

        // Verify the data in the DashMap
        assert_eq!(manager.data.len(), 10, "Should have 10 IPs in the map");

        for info in manager.data.iter() {
            assert_eq!(
                info.requests_used, MAX_REQUESTS_PER_IP,
                "Each IP should have used exactly MAX_REQUESTS_PER_IP requests"
            );
        }
    }

    #[test]
    fn test_secs_to_human_readable() {
        // Test seconds only
        assert_eq!(secs_to_human_readable(45), "45s");
        assert_eq!(secs_to_human_readable(1), "1s");

        // Test minutes and seconds
        assert_eq!(secs_to_human_readable(65), "1m 5s");
        assert_eq!(secs_to_human_readable(3599), "59m 59s");

        // Test hours, minutes, and seconds
        assert_eq!(secs_to_human_readable(3600), "1h 0m 0s");
        assert_eq!(secs_to_human_readable(3661), "1h 1m 1s");
        assert_eq!(secs_to_human_readable(7384), "2h 3m 4s");

        // Test edge case
        assert_eq!(secs_to_human_readable(0), "0s");
    }
}
