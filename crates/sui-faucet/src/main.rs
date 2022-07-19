// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use axum::{
    error_handling::HandleErrorLayer,
    http::StatusCode,
    response::IntoResponse,
    routing::{get, post},
    BoxError, Extension, Json, Router,
};
use clap::Parser;
use http::Method;
use std::{
    borrow::Cow,
    net::{IpAddr, Ipv4Addr, SocketAddr},
    sync::Arc,
    time::Duration,
};
use sui::client_commands::{SuiClientCommands, WalletContext};
use sui_config::{sui_config_dir, SUI_CLIENT_CONFIG};
use sui_faucet::{Faucet, FaucetRequest, FaucetResponse, SimpleFaucet};
use tower::ServiceBuilder;
use tower_http::cors::{Any, CorsLayer};
use tracing::{info, warn};

// TODO: Increase this once we use multiple gas objects
const CONCURRENCY_LIMIT: usize = 1;

#[derive(Parser)]
#[clap(
    name = "Sui Faucet",
    about = "Faucet for requesting test tokens on Sui",
    rename_all = "kebab-case"
)]
struct FaucetConfig {
    #[clap(long, default_value_t = 5003)]
    port: u16,

    #[clap(long, default_value = "127.0.0.1")]
    host_ip: Ipv4Addr,

    #[clap(long, default_value_t = 50000)]
    amount: u64,

    #[clap(long, default_value_t = 5)]
    num_coins: usize,

    #[clap(long, default_value_t = 10)]
    request_buffer_size: usize,

    #[clap(long, default_value_t = 120)]
    timeout_in_seconds: u64,
}

struct AppState<F = SimpleFaucet> {
    faucet: F,
    config: FaucetConfig,
    // TODO: add counter
}

#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {
    // initialize tracing
    let _guard = telemetry_subscribers::TelemetryConfig::new(env!("CARGO_BIN_NAME"))
        .with_env()
        .init();

    let context = create_wallet_context().await?;

    let config: FaucetConfig = FaucetConfig::parse();

    let FaucetConfig {
        host_ip,
        port,
        request_buffer_size,
        timeout_in_seconds,
        ..
    } = config;

    let app_state = Arc::new(AppState {
        faucet: SimpleFaucet::new(context).await.unwrap(),
        config,
    });

    // TODO: restrict access if needed
    let cors = CorsLayer::new()
        .allow_methods(vec![Method::GET, Method::POST])
        .allow_headers(Any)
        .allow_origin(Any);

    let app = Router::new()
        .route("/", get(health))
        .route("/gas", post(request_gas))
        .layer(
            ServiceBuilder::new()
                .layer(HandleErrorLayer::new(handle_error))
                .layer(cors)
                .buffer(request_buffer_size)
                .concurrency_limit(CONCURRENCY_LIMIT)
                .timeout(Duration::from_secs(timeout_in_seconds))
                .layer(Extension(app_state))
                .into_inner(),
        );

    let addr = SocketAddr::new(IpAddr::V4(host_ip), port);
    info!("listening on {}", addr);
    axum::Server::bind(&addr)
        .serve(app.into_make_service())
        .await?;
    Ok(())
}

/// basic handler that responds with a static string
async fn health() -> &'static str {
    "OK"
}

/// handler for all the request_gas requests
async fn request_gas(
    Json(payload): Json<FaucetRequest>,
    Extension(state): Extension<Arc<AppState>>,
) -> impl IntoResponse {
    let result = match payload {
        FaucetRequest::FixedAmountRequest(requests) => {
            state
                .faucet
                .send(
                    requests.recipient,
                    &vec![state.config.amount; state.config.num_coins],
                )
                .await
        }
    };
    match result {
        Ok(v) => (StatusCode::CREATED, Json(FaucetResponse::from(v))),
        Err(v) => {
            warn!("Failed to request gas: {:?}", v);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(FaucetResponse::from(v)),
            )
        }
    }
}

async fn create_wallet_context() -> Result<WalletContext, anyhow::Error> {
    // Create Wallet context.
    let wallet_conf = sui_config_dir()?.join(SUI_CLIENT_CONFIG);
    info!("Initialize wallet from config path: {:?}", wallet_conf);
    let mut context = WalletContext::new(&wallet_conf)?;
    let address = context
        .config
        .accounts
        .first()
        .cloned()
        .ok_or_else(|| anyhow::anyhow!("Empty wallet context!"))?;

    // Sync client to retrieve objects from the network.
    SuiClientCommands::SyncClientState {
        address: Some(address),
    }
    .execute(&mut context)
    .await
    .map_err(|err| anyhow::anyhow!("Fail to sync client state: {}", err))?;
    Ok(context)
}

async fn handle_error(error: BoxError) -> impl IntoResponse {
    if error.is::<tower::timeout::error::Elapsed>() {
        return (StatusCode::REQUEST_TIMEOUT, Cow::from("request timed out"));
    }

    if error.is::<tower::load_shed::error::Overloaded>() {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            Cow::from("service is overloaded, try again later"),
        );
    }

    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Cow::from(format!("Unhandled internal error: {}", error)),
    )
}
