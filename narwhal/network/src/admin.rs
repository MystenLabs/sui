// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use axum::routing::post;
use axum::{extract::Extension, http::StatusCode, routing::get, Json, Router};
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use sui_metrics::spawn_monitored_task;
use tokio::sync::watch;
use tokio::task::JoinHandle;
use tracing::info;
use types::metered_channel::Sender;
use types::ReconfigureNotification;

pub fn start_admin_server(
    port: u16,
    network: anemo::Network,
    mut rx_reconfigure: watch::Receiver<ReconfigureNotification>,
    tx_state_handler: Option<Sender<ReconfigureNotification>>,
) -> Vec<JoinHandle<()>> {
    let mut router = Router::new()
        .route("/peers", get(get_peers))
        .route("/known_peers", get(get_known_peers));

    // Primaries will have this service enabled
    if let Some(tx_state_handler) = tx_state_handler {
        let r = Router::new()
            .route("/reconfigure", post(reconfigure))
            .layer(Extension(tx_state_handler));
        router = router.merge(r);
    }

    router = router.layer(Extension(network));

    let socket_address = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), port);
    info!(
        address =% socket_address,
        "starting admin server"
    );

    let handle = axum_server::Handle::new();
    let shutdown_handle = handle.clone();

    let mut handles = Vec::new();
    // Spawn a task to shutdown server.
    handles.push(spawn_monitored_task!(async move {
        while (rx_reconfigure.changed().await).is_ok() {
            let message = rx_reconfigure.borrow().clone();
            if let ReconfigureNotification::Shutdown = message {
                handle.clone().shutdown();
                return;
            }
        }
    }));

    handles.push(spawn_monitored_task!(async move {
        axum_server::bind(socket_address)
            .handle(shutdown_handle)
            .serve(router.into_make_service())
            .await
            .unwrap();
    }));

    handles
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

async fn reconfigure(
    Extension(tx_state_handler): Extension<Sender<ReconfigureNotification>>,
    Json(reconfigure_notification): Json<ReconfigureNotification>,
) -> StatusCode {
    let _ = tx_state_handler.send(reconfigure_notification).await;
    StatusCode::OK
}
