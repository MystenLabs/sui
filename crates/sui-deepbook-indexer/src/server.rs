// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::{
    error::DeepBookError,
    models::{BalancesSummary, OrderFillSummary, Pools},
    schema::{self},
    sui_deepbook_indexer::PgDeepbookPersistent,
};
use axum::{
    debug_handler,
    extract::{Path, State},
    http::StatusCode,
    routing::get,
    Json, Router,
};
use diesel::BoolExpressionMethods;
use diesel::QueryDsl;
use diesel::{ExpressionMethods, SelectableHelper};
use diesel_async::RunQueryDsl;
use std::time::{SystemTime, UNIX_EPOCH};
use std::{collections::HashMap, net::SocketAddr};
use tokio::{net::TcpListener, task::JoinHandle};

pub const GET_POOLS_PATH: &str = "/get_pools";
pub const GET_24HR_VOLUME_PATH: &str = "/get_24hr_volume/:pool_id";
pub const GET_24HR_VOLUME_BY_BALANCE_MANAGER_ID: &str =
    "/get_24hr_volume_by_balance_manager_id/:pool_id/:balance_manager_id";
pub const GET_NET_DEPOSITS: &str = "/get_net_deposits/:asset_ids/:timestamp";

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
        .route(GET_NET_DEPOSITS, get(get_net_deposits))
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
) -> Result<Json<u64>, DeepBookError> {
    let connection = &mut state.pool.get().await?;
    let unix_ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_millis() as i64;
    let day_ago = unix_ts - 24 * 60 * 60 * 1000;
    let vols: Vec<i64> = schema::order_fills::table
        .select(schema::order_fills::base_quantity)
        .filter(schema::order_fills::pool_id.eq(pool_id))
        .filter(schema::order_fills::onchain_timestamp.gt(day_ago))
        .load(connection)
        .await?;
    Ok(Json(vols.into_iter().map(|v| v as u64).sum()))
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
    let results: Vec<OrderFillSummary> = schema::order_fills::table
        .select((
            schema::order_fills::maker_balance_manager_id,
            schema::order_fills::taker_balance_manager_id,
            schema::order_fills::base_quantity,
        ))
        .filter(schema::order_fills::pool_id.eq(pool_id))
        .filter(schema::order_fills::onchain_timestamp.gt(day_ago))
        .filter(
            schema::order_fills::maker_balance_manager_id
                .eq(&balance_manager_id)
                .or(schema::order_fills::taker_balance_manager_id.eq(&balance_manager_id)),
        )
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

#[debug_handler]
async fn get_net_deposits(
    Path((asset_ids, timestamp)): Path<(String, String)>,
    State(state): State<PgDeepbookPersistent>,
) -> Result<Json<HashMap<String, i64>>, DeepBookError> {
    let connection = &mut state.pool.get().await?;
    let mut query =
        "SELECT asset, SUM(amount)::bigint AS amount, deposit FROM balances WHERE checkpoint_timestamp_ms < "
            .to_string();
    query.push_str(&timestamp);
    query.push_str("000 AND asset in (");
    for asset in asset_ids.split(",") {
        if asset.starts_with("0x") {
            let len = asset.len();
            query.push_str(&format!("'{}',", &asset[2..len]));
        } else {
            query.push_str(&format!("'{}',", asset));
        }
    }
    query.pop();
    query.push_str(") GROUP BY asset, deposit");

    let results: Vec<BalancesSummary> = diesel::sql_query(query).load(connection).await?;
    let mut net_deposits = HashMap::new();
    for result in results {
        let mut asset = result.asset;
        if !asset.starts_with("0x") {
            asset.insert_str(0, "0x");
        }
        let amount = result.amount;
        if result.deposit {
            *net_deposits.entry(asset).or_insert(0) += amount;
        } else {
            *net_deposits.entry(asset).or_insert(0) -= amount;
        }
    }

    Ok(Json(net_deposits))
}
