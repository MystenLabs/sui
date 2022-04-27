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
use std::{
    borrow::Cow,
    net::{IpAddr, Ipv4Addr, SocketAddr},
    sync::Arc,
    time::Duration,
};
use sui::{
    sui_config_dir,
    wallet_commands::{WalletCommands, WalletContext},
    SUI_WALLET_CONFIG,
};
use sui_faucet::{Faucet, FaucetRequest, FaucetResponse, SimpleFaucet};
use tower::ServiceBuilder;
use tracing::info;

const DEFAULT_SERVER_PORT: &str = "5003";
const DEFAULT_SERVER_ADDR_IPV4: &str = "127.0.0.1";

const DEFAULT_AMOUNT: u64 = 20;
const DEFAULT_NUM_COINS: usize = 5;
const REQUEST_BUFFER_SIZE: usize = 10;
// TODO: Increase this once we use multiple gas objects
const CONCURRENCY_LIMIT: usize = 1;
const TIMEOUT_IN_SECONDS: u64 = 120;

#[derive(Parser)]
#[clap(
    name = "Sui Faucet",
    about = "Faucet for requesting test tokens on Sui",
    rename_all = "kebab-case"
)]
struct FaucetConfig {
    #[clap(long, default_value = DEFAULT_SERVER_PORT)]
    port: u16,

    #[clap(long, default_value = DEFAULT_SERVER_ADDR_IPV4)]
    host: Ipv4Addr,
}

struct AppState<F = SimpleFaucet> {
    faucet: F,
    // TODO: add counter
}

#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {
    // initialize tracing
    tracing_subscriber::fmt::init();

    let context = create_wallet_context().await?;

    let config: FaucetConfig = FaucetConfig::parse();

    let app_state = Arc::new(AppState {
        faucet: SimpleFaucet::new(context).await.unwrap(),
    });

    let app = Router::new()
        .route("/", get(health))
        .route("/gas", post(request_gas))
        .layer(
            ServiceBuilder::new()
                .layer(HandleErrorLayer::new(handle_error))
                .buffer(REQUEST_BUFFER_SIZE)
                .concurrency_limit(CONCURRENCY_LIMIT)
                .timeout(Duration::from_secs(TIMEOUT_IN_SECONDS))
                .layer(Extension(app_state))
                .into_inner(),
        );

    let addr = SocketAddr::new(IpAddr::V4(config.host), config.port);
    info!("listening on {}", addr);
    axum::Server::bind(&addr)
        .serve(app.into_make_service())
        .await
        .unwrap();
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
                .send(requests.recipient, &[DEFAULT_AMOUNT; DEFAULT_NUM_COINS])
                .await
        }
    };
    match result {
        Ok(v) => (StatusCode::CREATED, Json(FaucetResponse::from(v))),
        Err(v) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(FaucetResponse::from(v)),
        ),
    }
}

async fn create_wallet_context() -> Result<WalletContext, anyhow::Error> {
    // Create Wallet context.
    let wallet_conf = sui_config_dir()?.join(SUI_WALLET_CONFIG);
    info!("Initialize wallet from config path: {:?}", wallet_conf);
    let mut context = WalletContext::new(&wallet_conf)?;
    let address = context.config.accounts.first().cloned().unwrap();

    // Sync client to retrieve objects from the network.
    WalletCommands::SyncClientState {
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
