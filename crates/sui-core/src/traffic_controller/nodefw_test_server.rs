// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::traffic_controller::nodefw_client::{BlockAddress, BlockAddresses};
use axum::{
    extract::State,
    http::StatusCode,
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use std::time::{Duration, SystemTime};
use std::{collections::HashMap, net::SocketAddr, sync::Arc};
use tokio::sync::{Mutex, Notify};
use tokio::task::JoinHandle;

#[derive(Clone)]
struct AppState {
    /// BlockAddress -> expiry time
    blocklist: Arc<Mutex<HashMap<BlockAddress, SystemTime>>>,
}

pub struct NodeFwTestServer {
    server_handle: Option<JoinHandle<()>>,
    shutdown_signal: Arc<Notify>,
    state: AppState,
}

impl NodeFwTestServer {
    pub fn new() -> Self {
        Self {
            server_handle: None,
            shutdown_signal: Arc::new(Notify::new()),
            state: AppState {
                blocklist: Arc::new(Mutex::new(HashMap::new())),
            },
        }
    }

    pub async fn start(&mut self, port: u16) {
        let app_state = self.state.clone();
        let app = Router::new()
            .route("/list_addresses", get(Self::list_addresses))
            .route("/block_addresses", post(Self::block_addresses))
            .with_state(app_state.clone());

        let addr = SocketAddr::from(([127, 0, 0, 1], port));

        let handle = tokio::spawn(async move {
            let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
            axum::serve(listener, app).await.unwrap();
        });

        tokio::spawn(Self::periodically_remove_expired_addresses(
            app_state.blocklist.clone(),
        ));

        self.server_handle = Some(handle);
    }

    /// Direct access api for test verification
    pub async fn list_addresses_rpc(&self) -> Vec<BlockAddress> {
        let blocklist = self.state.blocklist.lock().await;
        blocklist.keys().cloned().collect()
    }

    /// Endpoint handler to list addresses
    async fn list_addresses(State(state): State<AppState>) -> impl IntoResponse {
        let blocklist = state.blocklist.lock().await;
        let block_addresses = blocklist.keys().cloned().collect();
        Json(BlockAddresses {
            addresses: block_addresses,
        })
    }

    async fn periodically_remove_expired_addresses(
        blocklist: Arc<Mutex<HashMap<BlockAddress, SystemTime>>>,
    ) {
        loop {
            tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
            let mut blocklist = blocklist.lock().await;
            let now = SystemTime::now();
            blocklist.retain(|_address, expiry| now < *expiry);
        }
    }

    /// Endpoint handler to block addresses
    async fn block_addresses(
        State(state): State<AppState>,
        Json(addresses): Json<BlockAddresses>,
    ) -> impl IntoResponse {
        let mut blocklist = state.blocklist.lock().await;
        for addr in addresses.addresses.iter() {
            blocklist.insert(
                addr.clone(),
                SystemTime::now() + Duration::from_secs(addr.ttl),
            );
        }
        (StatusCode::CREATED, "created")
    }

    pub async fn stop(&self) {
        self.shutdown_signal.notify_one();
    }
}

impl Default for NodeFwTestServer {
    fn default() -> Self {
        Self::new()
    }
}
