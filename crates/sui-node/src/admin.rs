// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::SuiNode;
use axum::{
    extract::State,
    http::StatusCode,
    routing::{get, post},
    Router,
};
use mysten_metrics::spawn_monitored_task;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::Arc;
use telemetry_subscribers::FilterHandle;
use tracing::info;

// Example commands:
//
// Set buffer stake to 1500 basis points:
//
//   $ curl -d 1500 -X POST http://127.0.0.1:1337/set-override-buffer-stake
//
// Clear buffer stake override, use ProtocolConfig::buffer_stake_for_protocol_upgrade_bps:
//
//   $ curl -X POST http://127.0.0.1:1337/clear-override-buffer-stake
//
// Vote to the epoch early
//
//   $ curl -X POST http://127.0.0.1:1337/force-close-epoch

const LOGGING_ROUTE: &str = "/logging";
const SET_BUFFER_STAKE_ROUTE: &str = "/set-override-buffer-stake";
const CLEAR_BUFFER_STAKE_ROUTE: &str = "/clear-override-buffer-stake";
const FORCE_CLOSE_EPOCH: &str = "/force-close-epoch";

struct AppState {
    node: Arc<SuiNode>,
    filter_handle: FilterHandle,
}

pub fn start_admin_server(node: Arc<SuiNode>, port: u16, filter_handle: FilterHandle) {
    let filter = filter_handle.get().unwrap();

    let app_state = AppState {
        node,
        filter_handle,
    };

    let app = Router::new()
        .route(LOGGING_ROUTE, get(get_filter))
        .route(LOGGING_ROUTE, post(set_filter))
        .route(
            SET_BUFFER_STAKE_ROUTE,
            post(set_override_protocol_upgrade_buffer_stake),
        )
        .route(
            CLEAR_BUFFER_STAKE_ROUTE,
            post(clear_override_protocol_upgrade_buffer_stake),
        )
        .route(FORCE_CLOSE_EPOCH, post(force_close_epoch))
        .with_state(Arc::new(app_state));

    let socket_address = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), port);
    info!(
        filter =% filter,
        address =% socket_address,
        "starting admin server"
    );

    spawn_monitored_task!(async move {
        axum::Server::bind(&socket_address)
            .serve(app.into_make_service())
            .await
            .unwrap();
    });
}

async fn get_filter(State(state): State<Arc<AppState>>) -> (StatusCode, String) {
    match state.filter_handle.get() {
        Ok(filter) => (StatusCode::OK, filter),
        Err(err) => (StatusCode::INTERNAL_SERVER_ERROR, err.to_string()),
    }
}

async fn set_filter(
    State(state): State<Arc<AppState>>,
    new_filter: String,
) -> (StatusCode, String) {
    match state.filter_handle.update(&new_filter) {
        Ok(()) => {
            info!(filter =% new_filter, "Log filter updated");
            (StatusCode::OK, "".into())
        }
        Err(err) => (StatusCode::BAD_REQUEST, err.to_string()),
    }
}

async fn clear_override_protocol_upgrade_buffer_stake(
    State(state): State<Arc<AppState>>,
    _body: String,
) -> (StatusCode, String) {
    match state.node.clear_override_protocol_upgrade_buffer_stake() {
        Ok(()) => (
            StatusCode::OK,
            "protocol upgrade buffer stake cleared".to_string(),
        ),
        Err(err) => (StatusCode::INTERNAL_SERVER_ERROR, err.to_string()),
    }
}

async fn set_override_protocol_upgrade_buffer_stake(
    State(state): State<Arc<AppState>>,
    buffer_bps: String,
) -> (StatusCode, String) {
    let Ok(buffer_bps) = buffer_bps.parse::<u64>() else {
        return (
            StatusCode::BAD_REQUEST,
            "argument must be a positive integer".to_string(),
        );
    };

    match state
        .node
        .set_override_protocol_upgrade_buffer_stake(buffer_bps)
    {
        Ok(()) => (
            StatusCode::OK,
            format!("protocol upgrade buffer stake set to '{}'", buffer_bps),
        ),
        Err(err) => (StatusCode::INTERNAL_SERVER_ERROR, err.to_string()),
    }
}

async fn force_close_epoch(
    State(state): State<Arc<AppState>>,
    _body: String,
) -> (StatusCode, String) {
    let epoch_store = state.node.state().load_epoch_store_one_call_per_task();

    match state.node.close_epoch(&epoch_store).await {
        Ok(()) => (
            StatusCode::OK,
            "close_epoch() called successfully".to_string(),
        ),
        Err(err) => (StatusCode::INTERNAL_SERVER_ERROR, err.to_string()),
    }
}
