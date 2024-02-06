// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use axum::{http::StatusCode, routing::get, Router};
use std::net::SocketAddr;
use std::sync::Arc;

use crate::{
    error::BridgeError,
    server::handler::{BridgeRequestHandler, BridgeRequestHandlerTrait},
    types::SignedBridgeAction,
};
use axum::{
    extract::{Path, State},
    Json,
};

pub mod handler;

#[cfg(test)]
pub(crate) mod mock_handler;

pub const APPLICATION_JSON: &str = "application/json";

// Important: the paths need to match the ones in bridge_client.rs
pub const ETH_TO_SUI_TX_PATH: &str = "/sign/bridge_tx/eth/sui/:tx_hash/:event_index";
pub const SUI_TO_ETH_TX_PATH: &str = "/sign/bridge_tx/sui/eth/:tx_digest/:event_index";
pub const COMMITTEE_BLOCKLIST_UPDATE_PATH: &str =
    "/sign/update_committee_blocklist/:chain_id/:nonce/:type/:keys";
pub const EMERGENCY_BUTTON_PATH: &str = "/sign/emergency_button/:chain_id/:nonce/:type";

pub async fn run_server(socket_address: &SocketAddr, handler: BridgeRequestHandler) {
    axum::Server::bind(socket_address)
        .serve(make_router(Arc::new(handler)).into_make_service())
        .await
        .unwrap();
}

pub(crate) fn make_router(
    handler: Arc<impl BridgeRequestHandlerTrait + Sync + Send + 'static>,
) -> Router {
    Router::new()
        .route("/", get(health_check))
        .route(ETH_TO_SUI_TX_PATH, get(handle_eth_tx_hash))
        .route(SUI_TO_ETH_TX_PATH, get(handle_sui_tx_digest))
        // TODO: handle COMMITTEE_BLOCKLIST_UPDATE_PATH
        // TODO: handle EMERGENCY_BUTTON_PATH
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
        Self::Generic(err.into().to_string())
    }
}

async fn health_check() -> StatusCode {
    StatusCode::OK
}

async fn handle_eth_tx_hash(
    Path((tx_hash_hex, event_idx)): Path<(String, u16)>,
    State(handler): State<Arc<impl BridgeRequestHandlerTrait + Sync + Send>>,
) -> Result<Json<SignedBridgeAction>, BridgeError> {
    let sig = handler.handle_eth_tx_hash(tx_hash_hex, event_idx).await?;
    Ok(sig)
}

async fn handle_sui_tx_digest(
    Path((tx_digest_base58, event_idx)): Path<(String, u16)>,
    State(handler): State<Arc<impl BridgeRequestHandlerTrait + Sync + Send>>,
) -> Result<Json<SignedBridgeAction>, BridgeError> {
    let sig: Json<
        sui_types::message_envelope::Envelope<
            crate::types::BridgeAction,
            crate::crypto::BridgeAuthoritySignInfo,
        >,
    > = handler
        .handle_sui_tx_digest(tx_digest_base58, event_idx)
        .await?;
    Ok(sig)
}
