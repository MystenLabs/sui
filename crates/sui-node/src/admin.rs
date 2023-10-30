// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::SuiNode;
use axum::{
    extract::{Query, State},
    http::StatusCode,
    routing::{get, post},
    Router,
};
use humantime::parse_duration;
use serde::Deserialize;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::Arc;
use sui_types::error::SuiError;
use telemetry_subscribers::TracingHandle;
use tracing::info;

// Example commands:
//
// Set buffer stake for current epoch 2 to 1500 basis points:
//
//   $ curl -X POST 'http://127.0.0.1:1337/set-override-buffer-stake?buffer_bps=1500&epoch=2'
//
// Clear buffer stake override for current epoch 2, use
// ProtocolConfig::buffer_stake_for_protocol_upgrade_bps:
//
//   $ curl -X POST 'http://127.0.0.1:1337/clear-override-buffer-stake?epoch=2'
//
// Vote to close epoch 2 early
//
//   $ curl -X POST 'http://127.0.0.1:1337/force-close-epoch?epoch=2'
//
// View current all capabilities from all authorities that have been received by this node:
//
//   $ curl 'http://127.0.0.1:1337/capabilities'
//
// View the node config (private keys will be masked):
//
//   $ curl 'http://127.0.0.1:1337/node-config'
//
// Set a time-limited tracing config. After the duration expires, tracing will be disabled
// automatically.
//
//   $ curl -X POST 'http://127.0.0.1:1337/enable-tracing?filter=info&duration=10s'
//
// Reset tracing to the TRACE_FILTER env var.
//
//   $ curl -X POST 'http://127.0.0.1:1337/reset-tracing'

const LOGGING_ROUTE: &str = "/logging";
const TRACING_ROUTE: &str = "/enable-tracing";
const TRACING_RESET_ROUTE: &str = "/reset-tracing";
const SET_BUFFER_STAKE_ROUTE: &str = "/set-override-buffer-stake";
const CLEAR_BUFFER_STAKE_ROUTE: &str = "/clear-override-buffer-stake";
const FORCE_CLOSE_EPOCH: &str = "/force-close-epoch";
const CAPABILITIES: &str = "/capabilities";
const NODE_CONFIG: &str = "/node-config";

struct AppState {
    node: Arc<SuiNode>,
    tracing_handle: TracingHandle,
}

pub async fn run_admin_server(node: Arc<SuiNode>, port: u16, tracing_handle: TracingHandle) {
    let filter = tracing_handle.get_log().unwrap();

    let app_state = AppState {
        node,
        tracing_handle,
    };

    let app = Router::new()
        .route(LOGGING_ROUTE, get(get_filter))
        .route(CAPABILITIES, get(capabilities))
        .route(NODE_CONFIG, get(node_config))
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
        .route(TRACING_ROUTE, post(enable_tracing))
        .route(TRACING_RESET_ROUTE, post(reset_tracing))
        .with_state(Arc::new(app_state));

    let socket_address = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), port);
    info!(
        filter =% filter,
        address =% socket_address,
        "starting admin server"
    );

    axum::Server::bind(&socket_address)
        .serve(app.into_make_service())
        .await
        .unwrap()
}

#[derive(Deserialize)]
struct EnableTracing {
    // These params change the filter, and reset it after the duration expires.
    filter: Option<String>,
    duration: Option<String>,

    // Change the trace output file (if file output was enabled at program start)
    trace_file: Option<String>,

    // Change the tracing sample rate
    sample_rate: Option<f64>,
}

async fn enable_tracing(
    State(state): State<Arc<AppState>>,
    query: Query<EnableTracing>,
) -> (StatusCode, String) {
    let Query(EnableTracing {
        filter,
        duration,
        trace_file,
        sample_rate,
    }) = query;

    let mut response = Vec::new();

    if let Some(sample_rate) = sample_rate {
        state.tracing_handle.update_sampling_rate(sample_rate);
        response.push(format!("sample rate set to {:?}", sample_rate));
    }

    if let Some(trace_file) = trace_file {
        if let Err(err) = state.tracing_handle.update_trace_file(&trace_file) {
            response.push(format!("can't update trace file: {:?}", err));
            return (StatusCode::BAD_REQUEST, response.join("\n"));
        } else {
            response.push(format!("trace file set to {:?}", trace_file));
        }
    }

    let Some(filter) = filter else {
        return (StatusCode::OK, response.join("\n"));
    };

    // Duration is required if filter is set
    let Some(duration) = duration else {
        response.push("can't update filter: missing duration".into());
        return (StatusCode::BAD_REQUEST, response.join("\n"));
    };

    let Ok(duration) = parse_duration(&duration) else {
        response.push("can't update filter: invalid duration".into());
        return (StatusCode::BAD_REQUEST, response.join("\n"));
    };

    match state.tracing_handle.update_trace_filter(&filter, duration) {
        Ok(()) => {
            response.push(format!("filter set to {:?}", filter));
            response.push(format!("filter will be reset after {:?}", duration));
            (StatusCode::OK, response.join("\n"))
        }
        Err(err) => {
            response.push(format!("can't update filter: {:?}", err));
            (StatusCode::BAD_REQUEST, response.join("\n"))
        }
    }
}

async fn reset_tracing(State(state): State<Arc<AppState>>) -> (StatusCode, String) {
    state.tracing_handle.reset_trace();
    (
        StatusCode::OK,
        "tracing filter reset to TRACE_FILTER env var".into(),
    )
}

async fn get_filter(State(state): State<Arc<AppState>>) -> (StatusCode, String) {
    match state.tracing_handle.get_log() {
        Ok(filter) => (StatusCode::OK, filter),
        Err(err) => (StatusCode::INTERNAL_SERVER_ERROR, err.to_string()),
    }
}

async fn set_filter(
    State(state): State<Arc<AppState>>,
    new_filter: String,
) -> (StatusCode, String) {
    match state.tracing_handle.update_log(&new_filter) {
        Ok(()) => {
            info!(filter =% new_filter, "Log filter updated");
            (StatusCode::OK, "".into())
        }
        Err(err) => (StatusCode::BAD_REQUEST, err.to_string()),
    }
}

async fn capabilities(State(state): State<Arc<AppState>>) -> (StatusCode, String) {
    let epoch_store = state.node.state().load_epoch_store_one_call_per_task();
    let capabilities = epoch_store.get_capabilities();

    let mut output = String::new();
    for capability in &capabilities {
        output.push_str(&format!("{:?}\n", capability));
    }

    (StatusCode::OK, output)
}

async fn node_config(State(state): State<Arc<AppState>>) -> (StatusCode, String) {
    let node_config = &state.node.config;

    // Note private keys will be masked
    (StatusCode::OK, format!("{:#?}\n", node_config))
}

#[derive(Deserialize)]
struct Epoch {
    epoch: u64,
}

async fn clear_override_protocol_upgrade_buffer_stake(
    State(state): State<Arc<AppState>>,
    epoch: Query<Epoch>,
) -> (StatusCode, String) {
    let Query(Epoch { epoch }) = epoch;

    match state
        .node
        .clear_override_protocol_upgrade_buffer_stake(epoch)
    {
        Ok(()) => (
            StatusCode::OK,
            "protocol upgrade buffer stake cleared\n".to_string(),
        ),
        Err(err) => (StatusCode::INTERNAL_SERVER_ERROR, err.to_string()),
    }
}

#[derive(Deserialize)]
struct SetBufferStake {
    buffer_bps: u64,
    epoch: u64,
}

async fn set_override_protocol_upgrade_buffer_stake(
    State(state): State<Arc<AppState>>,
    buffer_state: Query<SetBufferStake>,
) -> (StatusCode, String) {
    let Query(SetBufferStake { buffer_bps, epoch }) = buffer_state;

    match state
        .node
        .set_override_protocol_upgrade_buffer_stake(epoch, buffer_bps)
    {
        Ok(()) => (
            StatusCode::OK,
            format!("protocol upgrade buffer stake set to '{}'\n", buffer_bps),
        ),
        Err(err) => (StatusCode::INTERNAL_SERVER_ERROR, err.to_string()),
    }
}

async fn force_close_epoch(
    State(state): State<Arc<AppState>>,
    epoch: Query<Epoch>,
) -> (StatusCode, String) {
    let Query(Epoch {
        epoch: expected_epoch,
    }) = epoch;
    let epoch_store = state.node.state().load_epoch_store_one_call_per_task();
    let actual_epoch = epoch_store.epoch();
    if actual_epoch != expected_epoch {
        let err = SuiError::WrongEpoch {
            expected_epoch,
            actual_epoch,
        };
        return (StatusCode::INTERNAL_SERVER_ERROR, err.to_string());
    }

    match state.node.close_epoch(&epoch_store).await {
        Ok(()) => (
            StatusCode::OK,
            "close_epoch() called successfully\n".to_string(),
        ),
        Err(err) => (StatusCode::INTERNAL_SERVER_ERROR, err.to_string()),
    }
}
