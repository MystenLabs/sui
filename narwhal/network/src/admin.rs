// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use axum::{extract::Extension, http::StatusCode, routing::get, Json, Router};
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use tokio::sync::watch;
use tracing::info;
use types::ReconfigureNotification;

pub fn start_admin_server(
    port: u16,
    network: anemo::Network,
    mut rx_reconfigure: watch::Receiver<ReconfigureNotification>,
) {
    let app = Router::new()
        .route("/peers", get(get_peers))
        .route("/known_peers", get(get_known_peers))
        .layer(Extension(network));

    let socket_address = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), port);
    info!(
        address =% socket_address,
        "starting admin server"
    );

    let handle = axum_server::Handle::new();
    let shutdown_handle = handle.clone();

    // Spawn a task to shutdown server.
    tokio::spawn(async move {
        while (rx_reconfigure.changed().await).is_ok() {
            let message = rx_reconfigure.borrow().clone();
            if let ReconfigureNotification::Shutdown = message {
                handle.clone().shutdown();
                return;
            }
        }
    });

    tokio::spawn(async move {
        axum_server::bind(socket_address)
            .handle(shutdown_handle)
            .serve(app.into_make_service())
            .await
            .unwrap();
    });
}

async fn get_peers(
    Extension(network): Extension<anemo::Network>,
) -> (StatusCode, Json<Vec<String>>) {
    (
        StatusCode::OK,
        Json(network.peers().iter().map(|x| x.to_string()).collect()),
    )
}

async fn get_known_peers(
    Extension(network): Extension<anemo::Network>,
) -> (StatusCode, Json<Vec<String>>) {
    (
        StatusCode::OK,
        Json(
            network
                .known_peers()
                .get_all()
                .iter()
                .map(|x| format!("{}: {:?}", x.peer_id, x.address))
                .collect(),
        ),
    )
}
