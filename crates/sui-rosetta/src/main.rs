// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::Arc;

use axum::routing::post;
use axum::{Extension, Router};
use once_cell::sync::Lazy;
use tracing::info;

use sui_sdk::SuiClient;

use crate::errors::{Error, ErrorType};
use crate::state::ApiState;
use crate::types::{Currency, NetworkIdentifier, SuiEnv};
use crate::ErrorType::{UnsupportedBlockchain, UnsupportedNetwork};

mod account;
mod actions;
mod block;
mod construction;
mod errors;
mod network;
mod state;
mod types;

pub static SUI: Lazy<Currency> = Lazy::new(|| Currency {
    symbol: "SUI".to_string(),
    decimals: 8,
});

#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {
    // initialize tracing
    let _guard = telemetry_subscribers::TelemetryConfig::new(env!("CARGO_BIN_NAME"))
        .with_env()
        .init();

    let mut state = ApiState::default();
    state.add_env(
        SuiEnv::MainNet,
        SuiClient::new_rpc_client("http://127.0.0.1:9000", None).await?,
    );

    let app = Router::new()
        .route("/account/balance", post(account::balance))
        .route("/account/coins", post(account::coins))
        .route("/block", post(block::block))
        .route("/block/transaction", post(block::transaction))
        .route("/construction/derive", post(construction::derive))
        .route("/construction/payload", post(construction::payload))
        .route("/construction/combine", post(construction::combine))
        .route("/construction/submit", post(construction::submit))
        .route("/construction/preprocess", post(construction::preprocess))
        .route("/construction/hash", post(construction::hash))
        .route("/construction/metadata", post(construction::metadata))
        .route("/construction/parse", post(construction::parse))
        .route("/network/list", post(network::list))
        .route("/network/status", post(network::status))
        .route("/network/options", post(network::options))
        .layer(Extension(Arc::new(state)));

    let addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 9002);
    info!("listening on {}", addr);
    axum::Server::bind(&addr)
        .serve(app.into_make_service())
        .await?;

    Ok(())
}
