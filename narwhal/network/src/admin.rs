// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use axum::routing::post;
use axum::{extract::Extension, http::StatusCode, routing::get, Json, Router};
use mysten_metrics::{spawn_logged_monitored_task, spawn_monitored_task};
use std::net::{IpAddr, Ipv4Addr, SocketAddr, TcpListener};
use std::time::Duration;
use tokio::task::JoinHandle;
use tokio::time::sleep;
use tracing::{info, warn};
use types::metered_channel::Sender;
use types::{ConditionalBroadcastReceiver, ReconfigureNotification};

pub fn start_admin_server(
    port: u16,
    network: anemo::Network,
    mut tr_shutdown: ConditionalBroadcastReceiver,
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
        "Starting admin server at {socket_address}"
    );

    let handle = axum_server::Handle::new();
    let shutdown_handle = handle.clone();

    let mut handles = Vec::new();
    // Spawn a task to shutdown server.
    handles.push(spawn_monitored_task!(async move {
        _ = tr_shutdown.receiver.recv().await;
        handle.clone().shutdown();
    }));

    handles.push(spawn_logged_monitored_task!(
        async move {
            let mut i = 0;
            let listener = loop {
                i += 1;
                match TcpListener::bind(socket_address) {
                    Ok(listener) => break listener,
                    Err(e) => {
                        if i == 10 {
                            panic!("Failed to bind to {socket_address}: {e}");
                        } else {
                            warn!("Failed to bind to {socket_address}: {e}. Retrying ...");
                            sleep(Duration::from_secs(1)).await;
                        }
                    }
                };
            };
            axum_server::from_tcp(listener)
                .handle(shutdown_handle)
                .serve(router.into_make_service())
                .await
                .unwrap();
        },
        "AdminServerTask"
    ));

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
