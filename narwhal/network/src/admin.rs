// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use axum::{extract::Extension, http::StatusCode, routing::get, Json, Router};
use std::net::{IpAddr, Ipv4Addr, SocketAddr, TcpListener};
use std::time::Duration;
use tokio::sync::mpsc;
use tokio::task::JoinHandle;
use tokio::time::sleep;
use tracing::{error, info};
use types::{ConditionalBroadcastReceiver, ReconfigureNotification};

pub fn start_admin_server(
    port: u16,
    network: anemo::Network,
    mut tr_shutdown: ConditionalBroadcastReceiver,
    tx_state_handler: Option<mpsc::Sender<ReconfigureNotification>>,
) -> Vec<JoinHandle<()>> {
    let mut router = Router::new()
        .route("/peers", get(get_peers))
        .route("/known_peers", get(get_known_peers));

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
    handles.push(tokio::spawn(async move {
        _ = tr_shutdown.receiver.recv().await;
        handle.clone().shutdown();
    }));

    handles.push(tokio::spawn(async move {
        // retry a few times before quitting
        let mut total_retries = 10;

        loop {
            total_retries -= 1;

            match TcpListener::bind(socket_address) {
                Ok(listener) => {
                    axum_server::from_tcp(listener)
                        .handle(shutdown_handle)
                        .serve(router.into_make_service())
                        .await
                        .unwrap_or_else(|err| {
                            panic!("Failed to boot admin {}: {err}", socket_address)
                        });

                    return;
                }
                Err(err) => {
                    if total_retries == 0 {
                        error!("{}", err);
                        panic!("Failed to boot admin {}: {}", socket_address, err);
                    }

                    error!("{}", err);

                    // just sleep for a bit before retrying in case the port
                    // has not been de-allocated
                    sleep(Duration::from_secs(1)).await;

                    continue;
                }
            }
        }
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
    Extension(tx_state_handler): Extension<mpsc::Sender<ReconfigureNotification>>,
    Json(reconfigure_notification): Json<ReconfigureNotification>,
) -> StatusCode {
    let _ = tx_state_handler.send(reconfigure_notification).await;
    StatusCode::OK
}
