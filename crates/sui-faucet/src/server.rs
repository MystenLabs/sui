// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::{
    AppState, BatchFaucetResponse, BatchStatusFaucetResponse, FaucetConfig, FaucetError,
    FaucetRequest, FaucetResponse, RequestMetricsLayer,
};

use axum::{
    error_handling::HandleErrorLayer,
    extract::Path,
    http::StatusCode,
    response::IntoResponse,
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
    time::Duration,
};
use sui_config::SUI_CLIENT_CONFIG;
use sui_sdk::wallet_context::WalletContext;
use tower::{limit::RateLimitLayer, ServiceBuilder};
use tower_http::cors::{Any, CorsLayer};
use tracing::{info, warn};
use uuid::Uuid;

use crate::faucet::Faucet;

pub async fn start_faucet(
    app_state: Arc<AppState>,
    concurrency_limit: usize,
    prometheus_registry: &Registry,
) -> Result<(), anyhow::Error> {
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

    let app = Router::new()
        .route("/", get(health))
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
                .layer(RateLimitLayer::new(
                    max_request_per_second,
                    Duration::from_secs(1),
                ))
                .concurrency_limit(concurrency_limit)
                .layer(Extension(app_state.clone()))
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

    let addr = SocketAddr::new(IpAddr::V4(host_ip), port);
    info!("listening on {}", addr);
    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    axum::serve(listener, app).await?;
    Ok(())
}

/// basic handler that responds with a static string
async fn health() -> &'static str {
    "OK"
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
            StatusCode::INTERNAL_SERVER_ERROR,
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
