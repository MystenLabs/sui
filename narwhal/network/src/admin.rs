// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use axum::{extract::Extension, http::StatusCode, routing::get, Json, Router};
use mysten_metrics::spawn_logged_monitored_task;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::time::Duration;
use tokio::net::TcpListener;
use tokio::task::JoinHandle;
use tokio::time::sleep;
use tracing::{error, info};
use types::ConditionalBroadcastReceiver;

pub fn start_admin_server(
    port: u16,
    network: anemo::Network,
    mut tr_shutdown: ConditionalBroadcastReceiver,
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

    let mut handles = Vec::new();

    handles.push(spawn_logged_monitored_task!(
        async move {
            // retry a few times before quitting
            let mut total_retries = 10;

            loop {
                total_retries -= 1;

                match TcpListener::bind(socket_address).await {
                    Ok(listener) => {
                        axum::serve(listener, router)
                            .with_graceful_shutdown(async move {
                                _ = tr_shutdown.receiver.recv().await;
                            })
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
