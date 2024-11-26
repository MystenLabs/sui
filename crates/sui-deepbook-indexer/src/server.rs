// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::{
    error::DeepBookError,
    models::{OrderFillSummary, Pools},
    schema::{self},
    sui_deepbook_indexer::PgDeepbookPersistent,
};
use axum::{
    debug_handler,
    extract::{Path, Query, State},
    http::StatusCode,
    routing::get,
    Json, Router,
};
use diesel::dsl::sql;
use diesel::BoolExpressionMethods;
use diesel::QueryDsl;
use diesel::{ExpressionMethods, SelectableHelper};
use diesel_async::RunQueryDsl;
use std::time::{SystemTime, UNIX_EPOCH};
use std::{collections::HashMap, net::SocketAddr};
use tokio::{net::TcpListener, task::JoinHandle};

pub const GET_POOLS_PATH: &str = "/get_pools";
pub const GET_HISTORICAL_VOLUME_BY_BALANCE_MANAGER_ID_WITH_INTERVAL: &str =
    "/get_historical_volume_by_balance_manager_id_with_interval/:pool_ids/:balance_manager_id";
pub const GET_HISTORICAL_VOLUME_BY_BALANCE_MANAGER_ID: &str =
    "/get_historical_volume_by_balance_manager_id/:pool_ids/:balance_manager_id";
pub const GET_HISTORICAL_VOLUME_PATH: &str = "/get_historical_volume/:pool_ids";

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
        .route(GET_HISTORICAL_VOLUME_PATH, get(get_historical_volume))
        .route(
            GET_HISTORICAL_VOLUME_BY_BALANCE_MANAGER_ID_WITH_INTERVAL,
            get(get_historical_volume_by_balance_manager_id_with_interval),
        )
        .route(
            GET_HISTORICAL_VOLUME_BY_BALANCE_MANAGER_ID,
            get(get_historical_volume_by_balance_manager_id),
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

async fn get_historical_volume(
    Path(pool_ids): Path<String>,
    Query(params): Query<HashMap<String, String>>,
    State(state): State<PgDeepbookPersistent>,
) -> Result<Json<HashMap<String, u64>>, DeepBookError> {
    let connection = &mut state.pool.get().await?;

    let pool_ids_list: Vec<String> = pool_ids.split(',').map(|s| s.to_string()).collect();

    // Get start_time and end_time from query parameters (in seconds)
    let end_time = params
        .get("end_time")
        .and_then(|v| v.parse::<i64>().ok())
        .map(|t| t * 1000) // Convert to milliseconds
        .unwrap_or_else(|| {
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_millis() as i64
        });

    let start_time = params
        .get("start_time")
        .and_then(|v| v.parse::<i64>().ok())
        .map(|t| t * 1000) // Convert to milliseconds
        .unwrap_or_else(|| end_time - 24 * 60 * 60 * 1000);

    // Determine whether to query volume in base or quote
    let volume_in_base = params
        .get("volume_in_base")
        .map(|v| v == "true")
        .unwrap_or(true);
    let column_to_query = if volume_in_base {
        sql::<diesel::sql_types::BigInt>("base_quantity")
    } else {
        sql::<diesel::sql_types::BigInt>("quote_quantity")
    };

    let results: Vec<(String, i64)> = schema::order_fills::table
        .select((schema::order_fills::pool_id, column_to_query))
        .filter(schema::order_fills::pool_id.eq_any(pool_ids_list))
        .filter(schema::order_fills::onchain_timestamp.between(start_time, end_time))
        .load(connection)
        .await?;

    // Aggregate volume by pool
    let mut volume_by_pool = HashMap::new();
    for (pool_id, volume) in results {
        *volume_by_pool.entry(pool_id).or_insert(0) += volume as u64;
    }

    Ok(Json(volume_by_pool))
}

async fn get_historical_volume_by_balance_manager_id(
    Path((pool_ids, balance_manager_id)): Path<(String, String)>,
    Query(params): Query<HashMap<String, String>>,
    State(state): State<PgDeepbookPersistent>,
) -> Result<Json<HashMap<String, Vec<i64>>>, DeepBookError> {
    let connection = &mut state.pool.get().await?;
    let pool_ids_list: Vec<String> = pool_ids.split(',').map(|s| s.to_string()).collect();

    // Get start_time and end_time from query parameters (in seconds)
    let end_time = params
        .get("end_time")
        .and_then(|v| v.parse::<i64>().ok())
        .map(|t| t * 1000) // Convert to milliseconds
        .unwrap_or_else(|| {
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_millis() as i64
        });

    let start_time = params
        .get("start_time")
        .and_then(|v| v.parse::<i64>().ok())
        .map(|t| t * 1000) // Convert to milliseconds
        .unwrap_or_else(|| end_time - 24 * 60 * 60 * 1000);

    let volume_in_base = params
        .get("volume_in_base")
        .map(|v| v == "true")
        .unwrap_or(true);
    let column_to_query = if volume_in_base {
        sql::<diesel::sql_types::BigInt>("base_quantity")
    } else {
        sql::<diesel::sql_types::BigInt>("quote_quantity")
    };

    let results: Vec<OrderFillSummary> = schema::order_fills::table
        .select((
            schema::order_fills::pool_id,
            schema::order_fills::maker_balance_manager_id,
            schema::order_fills::taker_balance_manager_id,
            column_to_query,
        ))
        .filter(schema::order_fills::pool_id.eq_any(&pool_ids_list))
        .filter(schema::order_fills::onchain_timestamp.between(start_time, end_time))
        .filter(
            schema::order_fills::maker_balance_manager_id
                .eq(&balance_manager_id)
                .or(schema::order_fills::taker_balance_manager_id.eq(&balance_manager_id)),
        )
        .load(connection)
        .await?;

    let mut volume_by_pool: HashMap<String, Vec<i64>> = HashMap::new();
    for order_fill in results {
        let entry = volume_by_pool
            .entry(order_fill.pool_id.clone())
            .or_insert(vec![0, 0]);
        if order_fill.maker_balance_manager_id == balance_manager_id {
            entry[0] += order_fill.quantity;
        }
        if order_fill.taker_balance_manager_id == balance_manager_id {
            entry[1] += order_fill.quantity;
        }
    }

    Ok(Json(volume_by_pool))
}

async fn get_historical_volume_by_balance_manager_id_with_interval(
    Path((pool_ids, balance_manager_id)): Path<(String, String)>,
    Query(params): Query<HashMap<String, String>>,
    State(state): State<PgDeepbookPersistent>,
) -> Result<Json<HashMap<String, HashMap<String, Vec<i64>>>>, DeepBookError> {
    let connection = &mut state.pool.get().await?;
    let pool_ids_list: Vec<String> = pool_ids.split(',').map(|s| s.to_string()).collect();

    // Parse interval from query parameters (in seconds), default to 1 hour (3600 seconds)
    let interval = params
        .get("interval")
        .and_then(|v| v.parse::<i64>().ok())
        .unwrap_or(3600); // Default interval: 1 hour

    if interval <= 0 {
        return Err(DeepBookError::InternalError(
            "Interval must be greater than 0".to_string(),
        ));
    }

    let interval_ms = interval * 1000;

    // Parse start_time and end_time (in seconds) and convert to milliseconds
    let end_time = params
        .get("end_time")
        .and_then(|v| v.parse::<i64>().ok())
        .map(|t| t * 1000) // Convert to milliseconds
        .unwrap_or_else(|| {
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_millis() as i64
        });

    let start_time = params
        .get("start_time")
        .and_then(|v| v.parse::<i64>().ok())
        .map(|t| t * 1000) // Convert to milliseconds
        .unwrap_or_else(|| end_time - 24 * 60 * 60 * 1000);

    let mut metrics_by_interval: HashMap<String, HashMap<String, Vec<i64>>> = HashMap::new();

    let mut current_start = start_time;
    while current_start + interval_ms <= end_time {
        let current_end = current_start + interval_ms;

        let volume_in_base = params
            .get("volume_in_base")
            .map(|v| v == "true")
            .unwrap_or(true);
        let column_to_query = if volume_in_base {
            sql::<diesel::sql_types::BigInt>("base_quantity")
        } else {
            sql::<diesel::sql_types::BigInt>("quote_quantity")
        };

        let results: Vec<OrderFillSummary> = schema::order_fills::table
            .select((
                schema::order_fills::pool_id,
                schema::order_fills::maker_balance_manager_id,
                schema::order_fills::taker_balance_manager_id,
                column_to_query,
            ))
            .filter(schema::order_fills::pool_id.eq_any(&pool_ids_list))
            .filter(schema::order_fills::onchain_timestamp.between(current_start, current_end))
            .filter(
                schema::order_fills::maker_balance_manager_id
                    .eq(&balance_manager_id)
                    .or(schema::order_fills::taker_balance_manager_id.eq(&balance_manager_id)),
            )
            .load(connection)
            .await?;

        let mut volume_by_pool: HashMap<String, Vec<i64>> = HashMap::new();
        for order_fill in results {
            let entry = volume_by_pool
                .entry(order_fill.pool_id.clone())
                .or_insert(vec![0, 0]);
            if order_fill.maker_balance_manager_id == balance_manager_id {
                entry[0] += order_fill.quantity;
            }
            if order_fill.taker_balance_manager_id == balance_manager_id {
                entry[1] += order_fill.quantity;
            }
        }

        metrics_by_interval.insert(
            format!("[{}, {}]", current_start / 1000, current_end / 1000),
            volume_by_pool,
        );

        current_start = current_end;
    }

    Ok(Json(metrics_by_interval))
}
