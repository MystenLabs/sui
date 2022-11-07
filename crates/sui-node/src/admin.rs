// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use axum::{
    extract::Extension,
    http::StatusCode,
    routing::{get, post},
    Router,
};
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use sui_metrics::spawn_monitored_task;
use telemetry_subscribers::FilterHandle;
use tracing::info;

const LOGGING_ROUTE: &str = "/logging";

pub fn start_admin_server(port: u16, filter_handle: FilterHandle) {
    let filter = filter_handle.get().unwrap();

    let app = Router::new()
        .route(LOGGING_ROUTE, get(get_filter))
        .route(LOGGING_ROUTE, post(set_filter))
        .layer(Extension(filter_handle));

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

async fn get_filter(Extension(filter_handle): Extension<FilterHandle>) -> (StatusCode, String) {
    match filter_handle.get() {
        Ok(filter) => (StatusCode::OK, filter),
        Err(err) => (StatusCode::INTERNAL_SERVER_ERROR, err.to_string()),
    }
}

async fn set_filter(
    Extension(filter_handle): Extension<FilterHandle>,
    new_filter: String,
) -> (StatusCode, String) {
    match filter_handle.update(&new_filter) {
        Ok(()) => {
            info!(filter =% new_filter, "Log filter updated");
            (StatusCode::OK, "".into())
        }
        Err(err) => (StatusCode::BAD_REQUEST, err.to_string()),
    }
}
