// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::gas_pool::gas_pool_core::GasPool;
use crate::metrics::GasPoolMetrics;
use crate::read_auth_env;
#[cfg(test)]
use crate::rpc::client::GasPoolRpcClient;
use crate::rpc::rpc_types::{
    ExecuteTxRequest, ExecuteTxResponse, ReserveGasRequest, ReserveGasResponse,
};
use axum::headers::authorization::Bearer;
use axum::headers::Authorization;
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::routing::{get, post};
use axum::{Extension, Json, Router, TypedHeader};
use fastcrypto::encoding::Base64;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::Arc;
use std::time::Duration;
use sui_types::crypto::ToFromBytes;
use sui_types::signature::GenericSignature;
use sui_types::transaction::TransactionData;
use tokio::task::JoinHandle;
use tracing::{debug, info};

pub struct GasPoolServer {
    pub handle: JoinHandle<()>,
    pub rpc_port: u16,
}

impl GasPoolServer {
    pub async fn new(
        station: Arc<GasPool>,
        host_ip: Ipv4Addr,
        rpc_port: u16,
        metrics: Arc<GasPoolMetrics>,
    ) -> Self {
        let state = ServerState::new(station, metrics);
        let app = Router::new()
            .route("/", get(health))
            .route("/v1/reserve_gas", post(reserve_gas))
            .route("/v1/execute_tx", post(execute_tx))
            .layer(Extension(state));
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
    pub fn get_local_client(&self) -> GasPoolRpcClient {
        GasPoolRpcClient::new(format!("http://localhost:{}", self.rpc_port))
    }
}

#[derive(Clone)]
struct ServerState {
    gas_station: Arc<GasPool>,
    secret: Arc<String>,
    metrics: Arc<GasPoolMetrics>,
}

impl ServerState {
    fn new(gas_station: Arc<GasPool>, metrics: Arc<GasPoolMetrics>) -> Self {
        let secret = Arc::new(read_auth_env());
        Self {
            gas_station,
            secret,
            metrics,
        }
    }
}

async fn health() -> &'static str {
    info!("Received health request");
    "OK"
}

async fn reserve_gas(
    TypedHeader(authorization): TypedHeader<Authorization<Bearer>>,
    Extension(server): Extension<ServerState>,
    Json(payload): Json<ReserveGasRequest>,
) -> impl IntoResponse {
    server.metrics.num_total_reserve_gas_requests.inc();
    if authorization.token() != server.secret.as_str() {
        return (
            StatusCode::UNAUTHORIZED,
            Json(ReserveGasResponse::new_err(anyhow::anyhow!(
                "Invalid authorization token"
            ))),
        );
    }
    server.metrics.num_authorized_reserve_gas_requests.inc();
    debug!("Received v1 reserve_gas request: {:?}", payload);
    let ReserveGasRequest {
        gas_budget,
        request_sponsor,
        reserve_duration_secs,
    } = payload;
    server
        .metrics
        .target_gas_budget_per_request
        .observe(gas_budget);
    server
        .metrics
        .reserve_duration_per_request
        .observe(reserve_duration_secs);
    match server
        .gas_station
        .reserve_gas(
            request_sponsor,
            gas_budget,
            Duration::from_secs(reserve_duration_secs),
        )
        .await
    {
        Ok((sponsor, gas_coins)) => {
            server.metrics.num_successful_reserve_gas_requests.inc();
            server
                .metrics
                .reserved_gas_coin_count_per_request
                .observe(gas_coins.len() as u64);
            let response = ReserveGasResponse::new_ok(sponsor, gas_coins);
            (StatusCode::OK, Json(response))
        }
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ReserveGasResponse::new_err(err)),
        ),
    }
}

async fn execute_tx(
    TypedHeader(authorization): TypedHeader<Authorization<Bearer>>,
    Extension(server): Extension<ServerState>,
    Json(payload): Json<ExecuteTxRequest>,
) -> impl IntoResponse {
    server.metrics.num_total_execute_tx_requests.inc();
    if authorization.token() != server.secret.as_ref() {
        return (
            StatusCode::UNAUTHORIZED,
            Json(ExecuteTxResponse::new_err(anyhow::anyhow!(
                "Invalid authorization token"
            ))),
        );
    }
    server.metrics.num_authorized_execute_tx_requests.inc();
    debug!("Received v1 execute_tx request: {:?}", payload);
    let ExecuteTxRequest { tx_bytes, user_sig } = payload;
    let Ok((tx_data, user_sig)) = convert_tx_and_sig(tx_bytes, user_sig) else {
        return (
            StatusCode::BAD_REQUEST,
            Json(ExecuteTxResponse::new_err(anyhow::anyhow!(
                "Invalid bcs bytes for TransactionData"
            ))),
        );
    };
    // TODO: Should we check user signature?
    match server
        .gas_station
        .execute_transaction(tx_data, user_sig)
        .await
    {
        Ok(effects) => {
            server.metrics.num_successful_execute_tx_requests.inc();
            (StatusCode::OK, Json(ExecuteTxResponse::new_ok(effects)))
        }
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ExecuteTxResponse::new_err(err)),
        ),
    }
}

fn convert_tx_and_sig(
    tx_bytes: Base64,
    user_sig: Base64,
) -> anyhow::Result<(TransactionData, GenericSignature)> {
    let tx = bcs::from_bytes(
        &tx_bytes
            .to_vec()
            .map_err(|_| anyhow::anyhow!("Failed to convert tx_bytes to vector"))?,
    )?;
    let user_sig = GenericSignature::from_bytes(
        &user_sig
            .to_vec()
            .map_err(|_| anyhow::anyhow!("Failed to convert user_sig to vector"))?,
    )?;
    Ok((tx, user_sig))
}
