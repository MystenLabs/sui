// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use axum::{extract::Extension, http::StatusCode, routing::get, Json, Router};
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use tracing::info;

pub fn start_admin_server(port: u16, network: anemo::Network) {
    let app = Router::new()
        .route("/peers", get(get_peers))
        .route("/known_peers", get(get_known_peers))
        .layer(Extension(network));

    let socket_address = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), port);
    info!(
        address =% socket_address,
        "starting admin server"
    );

    tokio::spawn(async move {
        axum::Server::bind(&socket_address)
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
