// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::{
    error::DeepBookError,
    models::{OrderFill, Pools},
    schema,
    sui_deepbook_indexer::PgDeepbookPersistent,
};
use axum::{
    debug_handler,
    extract::{Path, State},
    http::StatusCode,
    routing::get,
    Json, Router,
};
use bigdecimal::BigDecimal;
use bigdecimal::ToPrimitive;
use diesel::dsl::sum;
use diesel::prelude::*;
use diesel::{ExpressionMethods, SelectableHelper};
use diesel_async::RunQueryDsl;
use std::net::SocketAddr;
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::{net::TcpListener, task::JoinHandle};
use tracing::info;

pub const GET_POOLS_PATH: &str = "/get_pools";
pub const GET_24HR_VOLUME_PATH: &str = "/get_24hr_volume/:pool_id";
pub const GET_24HR_VOLUME_BY_BALANCE_MANAGER_ID: &str =
    "/get_24hr_volume_by_balance_manager_id/:pool_id/:balance_manager_id";

pub fn run_server(socket_address: SocketAddr, state: PgDeepbookPersistent) -> JoinHandle<()> {
    tokio::spawn(async move {
        let listener = TcpListener::bind(socket_address).await.unwrap();
        axum::serve(listener, make_router(state)).await.unwrap();
    })
}

pub(crate) fn make_router(state: PgDeepbookPersistent) -> Router {
    Router::new()
        .route("/", get(health_check))
        .route(GET_POOLS_PATH, get(get_pools))
        .route(GET_24HR_VOLUME_PATH, get(get_24hr_volume))
        .route(
            GET_24HR_VOLUME_BY_BALANCE_MANAGER_ID,
            get(get_24hr_volume_by_balance_manager_id),
        )
        .with_state(state)
}

impl axum::response::IntoResponse for DeepBookError {
    // TODO: distinguish client error.
    fn into_response(self) -> axum::response::Response {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Something went wrong: {:?}", self),
        )
            .into_response()
    }
}

impl<E> From<E> for DeepBookError
where
    E: Into<anyhow::Error>,
{
    fn from(err: E) -> Self {
        Self::InternalError(err.into().to_string())
    }
}

async fn health_check() -> StatusCode {
    StatusCode::OK
}

/// Get all pools stored in database
#[debug_handler]
async fn get_pools(
    State(state): State<PgDeepbookPersistent>,
) -> Result<Json<Vec<Pools>>, DeepBookError> {
    let connection = &mut state.pool.get().await?;
    let results = schema::pools::table
        .select(Pools::as_select())
        .load(connection)
        .await?;

    Ok(Json(results))
}

async fn get_24hr_volume(
    Path(pool_id): Path<String>,
    State(state): State<PgDeepbookPersistent>,
) -> Result<Json<i64>, DeepBookError> {
    let connection = &mut state.pool.get().await?;
    let unix_ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_millis() as i64;
    let day_ago = unix_ts - 24 * 60 * 60 * 1000;
    info!("day_ago: {}", day_ago);
    let total_vol: BigDecimal = schema::order_fills::table
        .select(sum(schema::order_fills::base_quantity).nullable())
        .filter(schema::order_fills::pool_id.eq(pool_id))
        .filter(schema::order_fills::onchain_timestamp.gt(day_ago))
        .first::<Option<BigDecimal>>(connection)
        .await?
        .unwrap_or_default();

    let total_vol = total_vol.to_i64().unwrap_or_default();

    Ok(Json(total_vol))
}

async fn get_24hr_volume_by_balance_manager_id(
    Path((pool_id, balance_manager_id)): Path<(String, String)>,
    State(state): State<PgDeepbookPersistent>,
) -> Result<Json<Vec<i64>>, DeepBookError> {
    let connection = &mut state.pool.get().await?;
    let unix_ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_millis() as i64;
    let day_ago = unix_ts - 24 * 60 * 60 * 1000;
    let results: Vec<OrderFill> = schema::order_fills::table
        .select(OrderFill::as_select())
        .filter(schema::order_fills::pool_id.eq(pool_id))
        .filter(schema::order_fills::onchain_timestamp.gt(day_ago))
        .load(connection)
        .await?;

    let mut maker_vol = 0;
    let mut taker_vol = 0;
    for order_fill in results {
        if order_fill.maker_balance_manager_id == balance_manager_id {
            maker_vol += order_fill.base_quantity;
        };
        if order_fill.taker_balance_manager_id == balance_manager_id {
            taker_vol += order_fill.base_quantity;
        };
    }

    Ok(Json(vec![maker_vol, taker_vol]))
}
