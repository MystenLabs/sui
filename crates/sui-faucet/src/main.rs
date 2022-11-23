// Copyright (c) Mysten Labs, Inc.
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
use std::env;
use std::{
    borrow::Cow,
    net::{IpAddr, Ipv4Addr, SocketAddr},
    sync::Arc,
    time::Duration,
};
use sui::client_commands::WalletContext;
use sui_config::{sui_config_dir, SUI_CLIENT_CONFIG};
use sui_faucet::{Faucet, FaucetRequest, FaucetResponse, RequestMetricsLayer, SimpleFaucet};
use sui_metrics::spawn_monitored_task;
use tower::{limit::RateLimitLayer, ServiceBuilder};
use tower_http::cors::{Any, CorsLayer};
use tracing::{info, warn};
use uuid::Uuid;

const CONCURRENCY_LIMIT: usize = 30;

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

    #[clap(long, default_value_t = 10)]
    max_request_per_second: u64,

    #[clap(long, default_value_t = 60)]
    wallet_client_timeout_secs: u64,
}

struct AppState<F = SimpleFaucet> {
    faucet: F,
    config: FaucetConfig,
    // TODO: add counter
}

const PROM_PORT_ADDR: &str = "0.0.0.0:9184";

#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {
    // initialize tracing
    let _guard = telemetry_subscribers::TelemetryConfig::new(env!("CARGO_BIN_NAME"))
        .with_env()
        .init();

    let max_concurrency = match env::var("MAX_CONCURRENCY") {
        Ok(val) => val.parse::<usize>().unwrap(),
        _ => CONCURRENCY_LIMIT,
    };
    info!("Max concurrency: {max_concurrency}.");

    let config: FaucetConfig = FaucetConfig::parse();
    let FaucetConfig {
        port,
        host_ip,
        request_buffer_size,
        max_request_per_second,
        wallet_client_timeout_secs,
        ..
    } = config;

    let context = create_wallet_context(wallet_client_timeout_secs).await?;

    let prom_binding = PROM_PORT_ADDR.parse().unwrap();
    info!("Starting Prometheus HTTP endpoint at {}", prom_binding);
    let prometheus_registry = sui_node::metrics::start_prometheus_server(prom_binding);

    let app_state = Arc::new(AppState {
        faucet: SimpleFaucet::new(context, &prometheus_registry)
            .await
            .unwrap(),
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
                .layer(RequestMetricsLayer::new(&prometheus_registry))
                .layer(cors)
                .load_shed()
                .buffer(request_buffer_size)
                .layer(RateLimitLayer::new(
                    max_request_per_second,
                    Duration::from_secs(1),
                ))
                .concurrency_limit(max_concurrency)
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
    // ID for traceability
    let id = Uuid::new_v4();
    info!(uuid = ?id, "Got new gas request.");
    let result = match payload {
        FaucetRequest::FixedAmountRequest(requests) => {
            // We spawn a tokio task for this such that connection drop will not interrupt
            // it and impact the reclycing of coins
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

async fn create_wallet_context(timeout_secs: u64) -> Result<WalletContext, anyhow::Error> {
    let wallet_conf = sui_config_dir()?.join(SUI_CLIENT_CONFIG);
    info!("Initialize wallet from config path: {:?}", wallet_conf);
    WalletContext::new(&wallet_conf, Some(Duration::from_secs(timeout_secs))).await
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
