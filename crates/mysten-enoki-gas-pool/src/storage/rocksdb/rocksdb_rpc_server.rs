// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[cfg(test)]
use crate::metrics::StoragePoolMetrics;
use crate::read_auth_env;
#[cfg(test)]
use crate::storage::rocksdb::rocksdb_rpc_client::RocksDbRpcClient;
use crate::storage::rocksdb::rocksdb_rpc_types::{
    ReserveGasStorageRequest, ReserveGasStorageResponse, UpdateGasStorageRequest,
    UpdateGasStorageResponse,
};
use crate::storage::rocksdb::RocksDBStorage;
use crate::storage::Storage;
#[cfg(test)]
use crate::AUTH_ENV_NAME;
use axum::headers::authorization::Bearer;
use axum::headers::Authorization;
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::routing::{get, post};
use axum::{Extension, Json, Router, TypedHeader};
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::Arc;
#[cfg(test)]
use sui_config::local_ip_utils::{get_available_port, localhost_for_testing};
#[cfg(test)]
use sui_types::base_types::SuiAddress;
use tokio::task::JoinHandle;
use tracing::{debug, info};

pub struct RocksDbServer {
    pub handle: JoinHandle<()>,
    pub rpc_port: u16,
}

impl RocksDbServer {
    pub async fn new(storage: Arc<RocksDBStorage>, host_ip: Ipv4Addr, rpc_port: u16) -> Self {
        let state = ServerState::new(storage);
        let mut app = Router::new()
            .route("/", get(health))
            .route("/v1/reserve_gas_coins", post(reserve_gas_coins))
            .route("/v1/update_gas_coins", post(update_gas_coins));
        #[cfg(test)]
        {
            app = app
                .route(
                    "/v1/get_available_coin_count",
                    post(get_available_coin_count),
                )
                .route(
                    "/v1/get_total_available_coin_balance",
                    post(get_total_available_coin_balance),
                )
                .route("/v1/get_reserved_coin_count", post(get_reserved_coin_count));
        }
        app = app.layer(Extension(state));
        let address = SocketAddr::new(IpAddr::V4(host_ip), rpc_port);
        let handle = tokio::spawn(async move {
            info!("listening on {}", address);
            axum::Server::bind(&address)
                .serve(app.into_make_service())
                .await
                .unwrap();
        });
        Self { handle, rpc_port }
    }

    #[cfg(test)]
    pub fn get_local_client(&self) -> RocksDbRpcClient {
        RocksDbRpcClient::new(format!("http://localhost:{}", self.rpc_port))
    }

    #[cfg(test)]
    pub async fn start_storage_server_for_testing() -> Arc<dyn Storage> {
        let localhost = localhost_for_testing();
        std::env::set_var(AUTH_ENV_NAME, "some secret");
        let storage_rpc_server = RocksDbServer::new(
            Arc::new(RocksDBStorage::new(
                tempfile::tempdir().unwrap().path(),
                StoragePoolMetrics::new_for_testing(),
            )),
            localhost.parse().unwrap(),
            get_available_port(&localhost),
        )
        .await;
        Arc::new(storage_rpc_server.get_local_client())
    }
}

#[derive(Clone)]
struct ServerState {
    storage: Arc<RocksDBStorage>,
    secret: Arc<String>,
}

impl ServerState {
    fn new(storage: Arc<RocksDBStorage>) -> Self {
        let secret = Arc::new(read_auth_env());
        Self { storage, secret }
    }
}

async fn health() -> &'static str {
    info!("Received health request");
    "OK"
}

async fn reserve_gas_coins(
    TypedHeader(authorization): TypedHeader<Authorization<Bearer>>,
    Extension(server): Extension<ServerState>,
    Json(payload): Json<ReserveGasStorageRequest>,
) -> impl IntoResponse {
    server
        .storage
        .metrics
        .num_total_storage_reserve_gas_coins_requests
        .inc();
    debug!("Received v1 reserve_gas_coins request: {:?}", payload);
    if authorization.token() != server.secret.as_str() {
        return (
            StatusCode::UNAUTHORIZED,
            Json(ReserveGasStorageResponse::new_err(anyhow::anyhow!(
                "Invalid authorization token"
            ))),
        );
    }
    server
        .storage
        .metrics
        .num_authorized_storage_reserve_gas_coins_requests
        .inc();
    let ReserveGasStorageRequest {
        gas_budget,
        request_sponsor,
    } = payload;
    match server
        .storage
        .reserve_gas_coins(request_sponsor, gas_budget)
        .await
    {
        Ok(gas_coins) => {
            server
                .storage
                .metrics
                .num_successful_storage_reserve_gas_coins_requests
                .inc();
            let response = ReserveGasStorageResponse::new_ok(gas_coins);
            (StatusCode::OK, Json(response))
        }
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ReserveGasStorageResponse::new_err(err)),
        ),
    }
}

async fn update_gas_coins(
    TypedHeader(authorization): TypedHeader<Authorization<Bearer>>,
    Extension(server): Extension<ServerState>,
    Json(payload): Json<UpdateGasStorageRequest>,
) -> impl IntoResponse {
    server
        .storage
        .metrics
        .num_total_storage_update_gas_coins_requests
        .inc();
    debug!("Received v1 update_gas_coins request: {:?}", payload);
    if authorization.token() != server.secret.as_ref() {
        return (
            StatusCode::UNAUTHORIZED,
            Json(UpdateGasStorageResponse::new_err(anyhow::anyhow!(
                "Invalid authorization token"
            ))),
        );
    }
    server
        .storage
        .metrics
        .num_authorized_storage_update_gas_coins_requests
        .inc();
    let UpdateGasStorageRequest {
        sponsor_address,
        released_gas_coins,
        deleted_gas_coins,
    } = payload;
    let released_gas_coins = released_gas_coins
        .into_iter()
        .map(|c| c.into())
        .collect::<Vec<_>>();
    match server
        .storage
        .update_gas_coins(sponsor_address, released_gas_coins, deleted_gas_coins)
        .await
    {
        Ok(()) => {
            server
                .storage
                .metrics
                .num_successful_storage_update_gas_coins_requests
                .inc();
            (StatusCode::OK, Json(UpdateGasStorageResponse::new_ok()))
        }
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(UpdateGasStorageResponse::new_err(err)),
        ),
    }
}

#[cfg(test)]
async fn get_available_coin_count(
    Extension(server): Extension<ServerState>,
    Json(sponsor_address): Json<SuiAddress>,
) -> impl IntoResponse {
    debug!(
        "Received v1 get_available_coin_count request: {:?}",
        sponsor_address
    );
    (
        StatusCode::OK,
        Json(
            server
                .storage
                .get_available_coin_count(sponsor_address)
                .await,
        ),
    )
}

#[cfg(test)]
async fn get_total_available_coin_balance(
    Extension(server): Extension<ServerState>,
    Json(sponsor_address): Json<SuiAddress>,
) -> impl IntoResponse {
    debug!(
        "Received v1 get_total_available_coin_balance request: {:?}",
        sponsor_address
    );
    (
        StatusCode::OK,
        Json(
            server
                .storage
                .get_total_available_coin_balance(sponsor_address)
                .await,
        ),
    )
}

#[cfg(test)]
async fn get_reserved_coin_count(
    Extension(server): Extension<ServerState>,
    Json(()): Json<()>,
) -> impl IntoResponse {
    debug!("Received v1 get_reserved_coin_count request");
    (
        StatusCode::OK,
        Json(server.storage.get_reserved_coin_count().await),
    )
}
