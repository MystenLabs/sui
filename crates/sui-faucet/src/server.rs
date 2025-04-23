// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::types::*;
use crate::{AppState, FaucetConfig, FaucetError, FaucetRequest};
use axum::{
    error_handling::HandleErrorLayer,
    http::StatusCode,
    response::IntoResponse,
    routing::{get, post},
    BoxError, Extension, Json, Router,
};
use http::Method;
use std::{
    borrow::Cow,
    net::{IpAddr, SocketAddr},
    path::PathBuf,
    sync::Arc,
    time::Duration,
};
use sui_config::SUI_CLIENT_CONFIG;
use sui_sdk::wallet_context::WalletContext;
use tower::ServiceBuilder;
use tower_http::cors::{Any, CorsLayer};
use tracing::info;

/// basic handler that responds with a static string
async fn health() -> &'static str {
    "OK"
}

async fn request_local_gas(
    Extension(state): Extension<Arc<AppState>>,
    Json(payload): Json<FaucetRequest>,
    // ) -> &'static str {
) -> impl IntoResponse {
    let FaucetRequest::FixedAmountRequest(request) = payload;
    info!("Local request for address: {}", request.recipient);
    let request = state
        .faucet
        .local_request_execute_tx(request.recipient)
        .await;

    if let Err(e) = request {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(FaucetResponse {
                status: RequestStatus::Failure(e),
                coins_sent: None,
            }),
        );
    }

    let Ok(coins) = request else {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(FaucetResponse {
                status: RequestStatus::Failure(FaucetError::internal(format!(
                    "Failed to execute transaction: {}",
                    request.unwrap_err()
                ))),
                coins_sent: None,
            }),
        );
    };

    (
        StatusCode::OK,
        Json(FaucetResponse {
            status: RequestStatus::Success,
            coins_sent: Some(coins),
        }),
    )
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

/// Start a faucet that is run locally. This should only be used for starting a local network, and
/// not for devnet/testnet deployments!
pub async fn start_faucet(app_state: Arc<AppState>) -> Result<(), anyhow::Error> {
    let cors = CorsLayer::new()
        .allow_methods(vec![Method::GET, Method::POST])
        .allow_headers(Any)
        .allow_origin(Any);
    let FaucetConfig { port, host_ip, .. } = app_state.config;

    info!("Starting faucet in local mode");
    let app = Router::new()
        .route("/", get(health))
        .route("/v2/gas", post(request_local_gas))
        .route("/v1/gas", post(request_local_gas))
        .route("/gas", post(request_local_gas))
        .layer(
            ServiceBuilder::new()
                .layer(HandleErrorLayer::new(handle_error))
                .load_shed()
                .layer(Extension(app_state.clone()))
                .layer(cors)
                .into_inner(),
        );

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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::LocalFaucet;
    use serde_json::json;
    use sui_sdk::types::base_types::SuiAddress;
    use test_cluster::TestClusterBuilder;

    #[tokio::test]
    async fn test_v2_gas_endpoint() {
        // Setup test cluster and faucet
        let cluster = TestClusterBuilder::new().build().await;
        let port = 9090;
        let config = FaucetConfig {
            host_ip: "127.0.0.1".parse().unwrap(),
            port,
            ..Default::default()
        };
        let local_faucet = LocalFaucet::new(cluster.wallet, config.clone())
            .await
            .unwrap();

        let app_state = Arc::new(AppState {
            faucet: local_faucet,
            config,
        });

        // Spawn the faucet in a background task
        let handle = tokio::spawn(async move {
            start_faucet(app_state)
                .await
                .expect("Failed to start faucet");
        });

        // Give the server a moment to start
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

        let client = reqwest::Client::new();

        // Test successful request
        let recipient = SuiAddress::random_for_testing_only();
        let req = FaucetRequest::new_fixed_amount_request(recipient);
        let response = client
            .post(format!("http://127.0.0.1:{port}/v2/gas",))
            .json(&req)
            .send()
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        let faucet_response = response.json::<FaucetResponse>().await.unwrap();

        // Verify the transaction was successful
        assert!(faucet_response.coins_sent.is_some());

        // Test invalid request
        let response = client
            .post(format!("http://127.0.0.1:{port}/v2/gas",))
            .json(&json!({
                "recipient": recipient.to_string(),
            }))
            .send()
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
        handle.abort();
    }
}
