// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use axum::{http::StatusCode, routing::get, Router};
use std::net::SocketAddr;
use std::sync::Arc;

use crate::error::BridgeError;
use crate::handler::BridgeRequestHandler;
use axum::{
    extract::{Path, State},
    Json,
};

pub const APPLICATION_JSON: &str = "application/json";

pub const ETH_TO_SUI_TX_PATH: &str = "/eth/sui/:tx_hash";
pub const SUI_TO_ETH_TX_PATH: &str = "/sui/eth/:tx_digest";

pub async fn run_server(socket_address: &SocketAddr) {
    axum::Server::bind(socket_address)
        .serve(make_router(Arc::new(BridgeRequestHandler::new())).into_make_service())
        .await
        .unwrap();
}

fn make_router(handler: Arc<BridgeRequestHandler>) -> Router {
    Router::new()
        .route("/", get(health_check))
        .route(ETH_TO_SUI_TX_PATH, get(handle_eth_tx_hash))
        .route(SUI_TO_ETH_TX_PATH, get(handle_sui_tx_digest))
        .with_state(handler)
}

impl axum::response::IntoResponse for BridgeError {
    // TODO: distinguish client error.
    fn into_response(self) -> axum::response::Response {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Something went wrong: {:?}", self),
        )
            .into_response()
    }
}

impl<E> From<E> for BridgeError
where
    E: Into<anyhow::Error>,
{
    fn from(err: E) -> Self {
        Self::Generic(err.into())
    }
}

async fn health_check() -> StatusCode {
    StatusCode::OK
}

async fn handle_eth_tx_hash(
    Path(tx_hash_hex): Path<String>,
    State(handler): State<Arc<BridgeRequestHandler>>,
) -> Result<Json<String>, BridgeError> {
    let sig = handler.handle_eth_tx_hash(tx_hash_hex).await?;
    Ok(sig)
}

async fn handle_sui_tx_digest(
    Path(tx_digest_base58): Path<String>,
    State(handler): State<Arc<BridgeRequestHandler>>,
) -> Result<Json<String>, BridgeError> {
    let sig = handler.handle_sui_tx_digest(tx_digest_base58).await?;
    Ok(sig)
}
