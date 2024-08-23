// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::fwmap::Firewall;
use axum::{
    extract::DefaultBodyLimit,
    http::StatusCode,
    routing::{get, post},
    Extension, Json, Router,
};
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;
use std::sync::{Arc, RwLock};
use std::time::Duration;
use tokio::signal;
use tokio::signal::unix::{signal as nix_signal, SignalKind};
use tokio_util::sync::CancellationToken;
use tower::ServiceBuilder;
use tower_http::{
    trace::{DefaultOnResponse, TraceLayer},
    LatencyUnit,
};
use tracing::{error, info, Level};

pub struct ServerConfig {
    pub ctx: CancellationToken,
    pub listener: std::net::TcpListener,
    pub router: Router,
}

pub async fn serve(c: ServerConfig) -> std::io::Result<()> {
    // setup our graceful shutdown
    let handle = axum_server::Handle::new();
    // Spawn a task to gracefully shutdown server.
    tokio::spawn(shutdown_signal(c.ctx, handle.clone()));
    axum_server::Server::from_tcp(c.listener)
        .handle(handle)
        .serve(c.router.into_make_service_with_connect_info::<SocketAddr>())
        .await
}

/// Configure our graceful shutdown scenarios
pub async fn shutdown_signal(ctx: CancellationToken, h: axum_server::Handle) {
    // Listen for the SIGTERM signal
    let mut sigterm =
        nix_signal(SignalKind::terminate()).expect("Failed to create SIGTERM signal handler");
    tokio::select! {
        _ = signal::ctrl_c() => {},
        _ = sigterm.recv() => {},
        _ = ctx.cancelled() => {},
    }
    // ensure context is cancelled everywhere
    ctx.cancel();
    let grace = crate::var!("GRACEFUL_EXIT_DURATION_SEC", 30, u64);
    info!(
        "starting graceful shutdown, grace period {} seconds...",
        &grace
    );
    h.graceful_shutdown(Some(Duration::from_secs(grace)))
}

/// App will configure our routes.
pub fn app(firewall: Arc<RwLock<Firewall>>) -> Router {
    // build our application with a route and our sender mpsc
    Router::new()
        .route("/list_addresses", get(list_addresses))
        .route("/block_addresses", post(block_addresses))
        .route_layer(DefaultBodyLimit::max(crate::var!(
            "MAX_BODY_SIZE",
            1024 * 1024 * 5,
            usize
        )))
        .layer(Extension(firewall))
        .layer(
            ServiceBuilder::new().layer(
                TraceLayer::new_for_http().on_response(
                    DefaultOnResponse::new()
                        .level(Level::INFO)
                        .latency_unit(LatencyUnit::Seconds),
                ),
            ),
        )
}

#[derive(Serialize, Deserialize, Debug)]
pub struct BlockAddresses {
    pub addresses: Vec<BlockAddress>,
}
#[derive(Serialize, Deserialize, Debug)]
pub struct BlockAddress {
    pub source_address: String,
    pub destination_port: u16,
    pub ttl: u64,
}

// handlers

async fn block_addresses(
    Extension(fw): Extension<Arc<RwLock<Firewall>>>,
    Json(request): Json<BlockAddresses>,
) -> (StatusCode, &'static str) {
    let mut fw_guard = fw.write().unwrap();
    if let Err(e) = fw_guard.block_addresses(request.addresses) {
        error!("unable to block requested address; {}", e);
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            "unknown error encountered",
        );
    }
    (StatusCode::CREATED, "created")
}

async fn list_addresses(
    Extension(fw): Extension<Arc<RwLock<Firewall>>>,
) -> (StatusCode, Json<BlockAddresses>) {
    let fw_guard = fw.read().unwrap();
    let addresses = match fw_guard.list_addresses() {
        Ok(v) => v,
        Err(e) => {
            error!("unable to block requested address; {}", e);
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(BlockAddresses { addresses: vec![] }),
            );
        }
    };
    (StatusCode::OK, Json(BlockAddresses { addresses }))
}
