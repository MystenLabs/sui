// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::{
    error::DeepBookError,
    models::{BalancesSummary, OrderFillSummary, Pools},
    schema::{self},
    sui_deepbook_indexer::PgDeepbookPersistent,
};
use axum::http::Method;
use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    routing::get,
    Json, Router,
};
use diesel::dsl::{count_star, sql};
use diesel::dsl::{max, min};
use diesel::BoolExpressionMethods;
use diesel::QueryDsl;
use diesel::{ExpressionMethods, SelectableHelper};
use diesel_async::RunQueryDsl;
use serde_json::Value;
use std::time::{SystemTime, UNIX_EPOCH};
use std::{collections::HashMap, net::SocketAddr};
use tokio::{net::TcpListener, task::JoinHandle};
use tower_http::cors::{AllowMethods, Any, CorsLayer};

use futures::future::join_all;
use std::str::FromStr;
use sui_json_rpc_types::{SuiObjectData, SuiObjectDataOptions, SuiObjectResponse};
use sui_sdk::SuiClientBuilder;
use sui_types::{
    base_types::{ObjectID, ObjectRef, SuiAddress},
    programmable_transaction_builder::ProgrammableTransactionBuilder,
    transaction::{Argument, CallArg, Command, ObjectArg, ProgrammableMoveCall, TransactionKind},
    type_input::TypeInput,
    TypeTag,
};
use tokio::join;

pub const SUI_MAINNET_URL: &str = "https://fullnode.mainnet.sui.io:443";
pub const GET_POOLS_PATH: &str = "/get_pools";
pub const GET_HISTORICAL_VOLUME_BY_BALANCE_MANAGER_ID_WITH_INTERVAL: &str =
    "/historical_volume_by_balance_manager_id_with_interval/:pool_names/:balance_manager_id";
pub const GET_HISTORICAL_VOLUME_BY_BALANCE_MANAGER_ID: &str =
    "/historical_volume_by_balance_manager_id/:pool_names/:balance_manager_id";
pub const HISTORICAL_VOLUME_PATH: &str = "/historical_volume/:pool_names";
pub const ALL_HISTORICAL_VOLUME_PATH: &str = "/all_historical_volume";
pub const GET_NET_DEPOSITS: &str = "/get_net_deposits/:asset_ids/:timestamp";
pub const TICKER_PATH: &str = "/ticker";
pub const TRADES_PATH: &str = "/trades/:pool_name";
pub const ORDER_UPDATES_PATH: &str = "/order_updates/:pool_name";
pub const TRADE_COUNT_PATH: &str = "/trade_count";
pub const ASSETS_PATH: &str = "/assets";
pub const SUMMARY_PATH: &str = "/summary";
pub const LEVEL2_PATH: &str = "/orderbook/:pool_name";
pub const LEVEL2_MODULE: &str = "pool";
pub const LEVEL2_FUNCTION: &str = "get_level2_ticks_from_mid";
pub const DEEPBOOK_PACKAGE_ID: &str =
    "0x2c8d603bc51326b8c13cef9dd07031a408a48dddb541963357661df5d3204809";
pub const DEEP_TOKEN_PACKAGE_ID: &str =
    "0xdeeb7a4662eec9f2f3def03fb937a663dddaa2e215b8078a284d026b7946c270";
pub const DEEP_TREASURY_ID: &str =
    "0x032abf8948dda67a271bcc18e776dbbcfb0d58c8d288a700ff0d5521e57a1ffe";
pub const DEEP_SUPPLY_MODULE: &str = "deep";
pub const DEEP_SUPPLY_FUNCTION: &str = "total_supply";
pub const DEEP_SUPPLY_PATH: &str = "/deep_supply";

pub fn run_server(socket_address: SocketAddr, state: PgDeepbookPersistent) -> JoinHandle<()> {
    tokio::spawn(async move {
        let listener = TcpListener::bind(socket_address).await.unwrap();
        axum::serve(listener, make_router(state)).await.unwrap();
    })
}

pub(crate) fn make_router(state: PgDeepbookPersistent) -> Router {
    let cors = CorsLayer::new()
        .allow_methods(AllowMethods::list(vec![Method::GET, Method::OPTIONS]))
        .allow_headers(Any)
        .allow_origin(Any);

    Router::new()
        .route("/", get(health_check))
        .route(GET_POOLS_PATH, get(get_pools))
        .route(HISTORICAL_VOLUME_PATH, get(historical_volume))
        .route(ALL_HISTORICAL_VOLUME_PATH, get(all_historical_volume))
        .route(
            GET_HISTORICAL_VOLUME_BY_BALANCE_MANAGER_ID_WITH_INTERVAL,
            get(get_historical_volume_by_balance_manager_id_with_interval),
        )
        .route(
            GET_HISTORICAL_VOLUME_BY_BALANCE_MANAGER_ID,
            get(get_historical_volume_by_balance_manager_id),
        )
        .route(LEVEL2_PATH, get(orderbook))
        .route(GET_NET_DEPOSITS, get(get_net_deposits))
        .route(TICKER_PATH, get(ticker))
        .route(TRADES_PATH, get(trades))
        .route(TRADE_COUNT_PATH, get(trade_count))
        .route(ORDER_UPDATES_PATH, get(order_updates))
        .route(ASSETS_PATH, get(assets))
        .route(SUMMARY_PATH, get(summary))
        .route(DEEP_SUPPLY_PATH, get(deep_supply))
        .layer(cors)
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

async fn historical_volume(
    Path(pool_names): Path<String>,
    Query(params): Query<HashMap<String, String>>,
    State(state): State<PgDeepbookPersistent>,
) -> Result<Json<HashMap<String, u64>>, DeepBookError> {
    // Fetch all pools to map names to IDs
    let pools: Json<Vec<Pools>> = get_pools(State(state.clone())).await?;
    let pool_name_to_id: HashMap<String, String> = pools
        .0
        .into_iter()
        .map(|pool| (pool.pool_name, pool.pool_id))
        .collect();

    // Map provided pool names to pool IDs
    let pool_ids_list: Vec<String> = pool_names
        .split(',')
        .filter_map(|name| pool_name_to_id.get(name).cloned())
        .collect();

    if pool_ids_list.is_empty() {
        return Err(DeepBookError::InternalError(
            "No valid pool names provided".to_string(),
        ));
    }

    // Parse start_time and end_time from query parameters (in seconds) and convert to milliseconds
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
        .unwrap_or(false);
    let column_to_query = if volume_in_base {
        sql::<diesel::sql_types::BigInt>("base_quantity")
    } else {
        sql::<diesel::sql_types::BigInt>("quote_quantity")
    };

    // Query the database for the historical volume
    let connection = &mut state.pool.get().await?;
    let results: Vec<(String, i64)> = schema::order_fills::table
        .filter(schema::order_fills::checkpoint_timestamp_ms.between(start_time, end_time))
        .filter(schema::order_fills::pool_id.eq_any(pool_ids_list))
        .select((schema::order_fills::pool_id, column_to_query))
        .load(connection)
        .await?;

    // Aggregate volume by pool ID and map back to pool names
    let mut volume_by_pool = HashMap::new();
    for (pool_id, volume) in results {
        if let Some(pool_name) = pool_name_to_id
            .iter()
            .find(|(_, id)| **id == pool_id)
            .map(|(name, _)| name)
        {
            *volume_by_pool.entry(pool_name.clone()).or_insert(0) += volume as u64;
        }
    }

    Ok(Json(volume_by_pool))
}

/// Get all historical volume for all pools
async fn all_historical_volume(
    Query(params): Query<HashMap<String, String>>,
    State(state): State<PgDeepbookPersistent>,
) -> Result<Json<HashMap<String, u64>>, DeepBookError> {
    let pools: Json<Vec<Pools>> = get_pools(State(state.clone())).await?;

    let pool_names: String = pools
        .0
        .into_iter()
        .map(|pool| pool.pool_name)
        .collect::<Vec<String>>()
        .join(",");

    historical_volume(Path(pool_names), Query(params), State(state)).await
}

async fn get_historical_volume_by_balance_manager_id(
    Path((pool_names, balance_manager_id)): Path<(String, String)>,
    Query(params): Query<HashMap<String, String>>,
    State(state): State<PgDeepbookPersistent>,
) -> Result<Json<HashMap<String, Vec<i64>>>, DeepBookError> {
    let connection = &mut state.pool.get().await?;

    let pools: Json<Vec<Pools>> = get_pools(State(state.clone())).await?;
    let pool_name_to_id: HashMap<String, String> = pools
        .0
        .into_iter()
        .map(|pool| (pool.pool_name, pool.pool_id))
        .collect();

    let pool_ids_list: Vec<String> = pool_names
        .split(',')
        .filter_map(|name| pool_name_to_id.get(name).cloned())
        .collect();

    if pool_ids_list.is_empty() {
        return Err(DeepBookError::InternalError(
            "No valid pool names provided".to_string(),
        ));
    }

    // Parse start_time and end_time
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
        .unwrap_or(false);
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
        .filter(schema::order_fills::checkpoint_timestamp_ms.between(start_time, end_time))
        .filter(
            schema::order_fills::maker_balance_manager_id
                .eq(&balance_manager_id)
                .or(schema::order_fills::taker_balance_manager_id.eq(&balance_manager_id)),
        )
        .load(connection)
        .await?;

    let mut volume_by_pool: HashMap<String, Vec<i64>> = HashMap::new();
    for order_fill in results {
        if let Some(pool_name) = pool_name_to_id
            .iter()
            .find(|(_, id)| **id == order_fill.pool_id)
            .map(|(name, _)| name)
        {
            let entry = volume_by_pool
                .entry(pool_name.clone())
                .or_insert(vec![0, 0]);
            if order_fill.maker_balance_manager_id == balance_manager_id {
                entry[0] += order_fill.quantity;
            }
            if order_fill.taker_balance_manager_id == balance_manager_id {
                entry[1] += order_fill.quantity;
            }
        }
    }

    Ok(Json(volume_by_pool))
}

async fn get_historical_volume_by_balance_manager_id_with_interval(
    Path((pool_names, balance_manager_id)): Path<(String, String)>,
    Query(params): Query<HashMap<String, String>>,
    State(state): State<PgDeepbookPersistent>,
) -> Result<Json<HashMap<String, HashMap<String, Vec<i64>>>>, DeepBookError> {
    let connection = &mut state.pool.get().await?;

    let pools: Json<Vec<Pools>> = get_pools(State(state.clone())).await?;
    let pool_name_to_id: HashMap<String, String> = pools
        .0
        .into_iter()
        .map(|pool| (pool.pool_name, pool.pool_id))
        .collect();

    let pool_ids_list: Vec<String> = pool_names
        .split(',')
        .filter_map(|name| pool_name_to_id.get(name).cloned())
        .collect();

    if pool_ids_list.is_empty() {
        return Err(DeepBookError::InternalError(
            "No valid pool names provided".to_string(),
        ));
    }

    // Parse interval
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

    // Parse start_time and end_time
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
            .unwrap_or(false);
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
            .filter(
                schema::order_fills::checkpoint_timestamp_ms.between(current_start, current_end),
            )
            .filter(
                schema::order_fills::maker_balance_manager_id
                    .eq(&balance_manager_id)
                    .or(schema::order_fills::taker_balance_manager_id.eq(&balance_manager_id)),
            )
            .load(connection)
            .await?;

        let mut volume_by_pool: HashMap<String, Vec<i64>> = HashMap::new();
        for order_fill in results {
            if let Some(pool_name) = pool_name_to_id
                .iter()
                .find(|(_, id)| **id == order_fill.pool_id)
                .map(|(name, _)| name)
            {
                let entry = volume_by_pool
                    .entry(pool_name.clone())
                    .or_insert(vec![0, 0]);
                if order_fill.maker_balance_manager_id == balance_manager_id {
                    entry[0] += order_fill.quantity;
                }
                if order_fill.taker_balance_manager_id == balance_manager_id {
                    entry[1] += order_fill.quantity;
                }
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

async fn ticker(
    Query(params): Query<HashMap<String, String>>,
    State(state): State<PgDeepbookPersistent>,
) -> Result<Json<HashMap<String, HashMap<String, Value>>>, DeepBookError> {
    // Fetch base and quote historical volumes
    let base_volumes = fetch_historical_volume(&params, true, &state).await?;
    let quote_volumes = fetch_historical_volume(&params, false, &state).await?;

    // Fetch pools data for metadata
    let pools: Json<Vec<Pools>> = get_pools(State(state.clone())).await?;
    let pool_map: HashMap<String, &Pools> = pools
        .0
        .iter()
        .map(|pool| (pool.pool_id.clone(), pool))
        .collect();

    let end_time = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| DeepBookError::InternalError("System time error".to_string()))?
        .as_millis() as i64;

    // Calculate the start time for 24 hours ago
    let start_time = end_time - (24 * 60 * 60 * 1000);

    // Fetch last prices for all pools in a single query. Only trades in the last 24 hours will count.
    let connection = &mut state.pool.get().await?;
    let last_prices: Vec<(String, i64)> = schema::order_fills::table
        .filter(schema::order_fills::checkpoint_timestamp_ms.between(start_time, end_time))
        .select((schema::order_fills::pool_id, schema::order_fills::price))
        .order_by((
            schema::order_fills::pool_id.asc(),
            schema::order_fills::checkpoint_timestamp_ms.desc(),
        ))
        .distinct_on(schema::order_fills::pool_id)
        .load(connection)
        .await?;

    let last_price_map: HashMap<String, i64> = last_prices.into_iter().collect();

    let mut response = HashMap::new();

    for (pool_id, pool) in &pool_map {
        let pool_name = &pool.pool_name;
        let base_volume = base_volumes.get(pool_name).copied().unwrap_or(0);
        let quote_volume = quote_volumes.get(pool_name).copied().unwrap_or(0);
        let last_price = last_price_map.get(pool_id).copied();

        // Conversion factors based on decimals
        let base_factor = 10u64.pow(pool.base_asset_decimals as u32);
        let quote_factor = 10u64.pow(pool.quote_asset_decimals as u32);
        let price_factor =
            10u64.pow((9 - pool.base_asset_decimals + pool.quote_asset_decimals) as u32);

        response.insert(
            pool_name.clone(),
            HashMap::from([
                (
                    "last_price".to_string(),
                    Value::from(
                        last_price
                            .map(|price| price as f64 / price_factor as f64)
                            .unwrap_or(0.0),
                    ),
                ),
                (
                    "base_volume".to_string(),
                    Value::from(base_volume as f64 / base_factor as f64),
                ),
                (
                    "quote_volume".to_string(),
                    Value::from(quote_volume as f64 / quote_factor as f64),
                ),
                ("isFrozen".to_string(), Value::from(0)), // Fixed to 0 because all pools in pools table are active
            ]),
        );
    }

    Ok(Json(response))
}

async fn fetch_historical_volume(
    params: &HashMap<String, String>,
    volume_in_base: bool,
    state: &PgDeepbookPersistent,
) -> Result<HashMap<String, u64>, DeepBookError> {
    let mut params_with_volume = params.clone();
    params_with_volume.insert("volume_in_base".to_string(), volume_in_base.to_string());

    all_historical_volume(Query(params_with_volume), State(state.clone()))
        .await
        .map(|Json(volumes)| volumes)
}

#[allow(clippy::get_first)]
async fn summary(
    State(state): State<PgDeepbookPersistent>,
) -> Result<Json<Vec<HashMap<String, Value>>>, DeepBookError> {
    // Fetch pools metadata first since it's required for other functions
    let pools: Json<Vec<Pools>> = get_pools(State(state.clone())).await?;
    let pool_metadata: HashMap<String, (String, (i16, i16))> = pools
        .0
        .iter()
        .map(|pool| {
            (
                pool.pool_name.clone(),
                (
                    pool.pool_id.clone(),
                    (pool.base_asset_decimals, pool.quote_asset_decimals),
                ),
            )
        })
        .collect();

    // Prepare pool decimals for scaling
    let pool_decimals: HashMap<String, (i16, i16)> = pool_metadata
        .iter()
        .map(|(_, (pool_id, decimals))| (pool_id.clone(), *decimals))
        .collect();

    // Parallelize fetching ticker, price changes, and high/low prices
    let (ticker_result, price_change_result, high_low_result) = join!(
        ticker(Query(HashMap::new()), State(state.clone())),
        price_change_24h(&pool_metadata, State(state.clone())),
        high_low_prices_24h(&pool_decimals, State(state.clone()))
    );

    let Json(ticker_map) = ticker_result?;
    let price_change_map = price_change_result?;
    let high_low_map = high_low_result?;

    // Prepare futures for orderbook queries
    let orderbook_futures: Vec<_> = ticker_map
        .keys()
        .map(|pool_name| {
            let pool_name_clone = pool_name.clone();
            orderbook(
                Path(pool_name_clone),
                Query(HashMap::from([("level".to_string(), "1".to_string())])),
                State(state.clone()),
            )
        })
        .collect();

    // Run all orderbook queries concurrently
    let orderbook_results = join_all(orderbook_futures).await;

    let mut response = Vec::new();

    for ((pool_name, ticker_info), orderbook_result) in ticker_map.iter().zip(orderbook_results) {
        if let Some((pool_id, _)) = pool_metadata.get(pool_name) {
            // Extract data from the ticker function response
            let last_price = ticker_info
                .get("last_price")
                .and_then(|price| price.as_f64())
                .unwrap_or(0.0);

            let base_volume = ticker_info
                .get("base_volume")
                .and_then(|volume| volume.as_f64())
                .unwrap_or(0.0);

            let quote_volume = ticker_info
                .get("quote_volume")
                .and_then(|volume| volume.as_f64())
                .unwrap_or(0.0);

            // Fetch the 24-hour price change percent
            let price_change_percent = price_change_map.get(pool_name).copied().unwrap_or(0.0);

            // Fetch the highest and lowest prices in the last 24 hours
            let (highest_price, lowest_price) =
                high_low_map.get(pool_id).copied().unwrap_or((0.0, 0.0));

            // Process the parallel orderbook result
            let orderbook_data = orderbook_result.ok().map(|Json(data)| data);

            let highest_bid = orderbook_data
                .as_ref()
                .and_then(|data| data.get("bids"))
                .and_then(|bids| bids.as_array())
                .and_then(|bids| bids.get(0))
                .and_then(|bid| bid.as_array())
                .and_then(|bid| bid.get(0))
                .and_then(|price| price.as_str()?.parse::<f64>().ok())
                .unwrap_or(0.0);

            let lowest_ask = orderbook_data
                .as_ref()
                .and_then(|data| data.get("asks"))
                .and_then(|asks| asks.as_array())
                .and_then(|asks| asks.get(0))
                .and_then(|ask| ask.as_array())
                .and_then(|ask| ask.get(0))
                .and_then(|price| price.as_str()?.parse::<f64>().ok())
                .unwrap_or(0.0);

            let mut summary_data = HashMap::new();
            summary_data.insert(
                "trading_pairs".to_string(),
                Value::String(pool_name.clone()),
            );
            let parts: Vec<&str> = pool_name.split('_').collect();
            let base_currency = parts.get(0).unwrap_or(&"Unknown").to_string();
            let quote_currency = parts.get(1).unwrap_or(&"Unknown").to_string();

            summary_data.insert("base_currency".to_string(), Value::String(base_currency));
            summary_data.insert("quote_currency".to_string(), Value::String(quote_currency));
            summary_data.insert("last_price".to_string(), Value::from(last_price));
            summary_data.insert("base_volume".to_string(), Value::from(base_volume));
            summary_data.insert("quote_volume".to_string(), Value::from(quote_volume));
            summary_data.insert(
                "price_change_percent_24h".to_string(),
                Value::from(price_change_percent),
            );
            summary_data.insert("highest_price_24h".to_string(), Value::from(highest_price));
            summary_data.insert("lowest_price_24h".to_string(), Value::from(lowest_price));
            summary_data.insert("highest_bid".to_string(), Value::from(highest_bid));
            summary_data.insert("lowest_ask".to_string(), Value::from(lowest_ask));

            response.push(summary_data);
        }
    }

    Ok(Json(response))
}

async fn high_low_prices_24h(
    pool_decimals: &HashMap<String, (i16, i16)>,
    State(state): State<PgDeepbookPersistent>,
) -> Result<HashMap<String, (f64, f64)>, DeepBookError> {
    // Get the current timestamp in milliseconds
    let end_time = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| DeepBookError::InternalError("System time error".to_string()))?
        .as_millis() as i64;

    // Calculate the start time for 24 hours ago
    let start_time = end_time - (24 * 60 * 60 * 1000);

    let connection = &mut state.pool.get().await?;

    // Query for trades within the last 24 hours for all pools
    let results: Vec<(String, Option<i64>, Option<i64>)> = schema::order_fills::table
        .filter(schema::order_fills::checkpoint_timestamp_ms.between(start_time, end_time))
        .group_by(schema::order_fills::pool_id)
        .select((
            schema::order_fills::pool_id,
            max(schema::order_fills::price),
            min(schema::order_fills::price),
        ))
        .load(connection)
        .await?;

    // Aggregate the highest and lowest prices for each pool
    let mut price_map: HashMap<String, (f64, f64)> = HashMap::new();

    for (pool_id, max_price_opt, min_price_opt) in results {
        if let Some((base_decimals, quote_decimals)) = pool_decimals.get(&pool_id) {
            let scaling_factor = 10f64.powi((9 - base_decimals + quote_decimals) as i32);

            let max_price_f64 = max_price_opt.unwrap_or(0) as f64 / scaling_factor;
            let min_price_f64 = min_price_opt.unwrap_or(0) as f64 / scaling_factor;

            price_map.insert(pool_id, (max_price_f64, min_price_f64));
        }
    }

    Ok(price_map)
}

async fn price_change_24h(
    pool_metadata: &HashMap<String, (String, (i16, i16))>,
    State(state): State<PgDeepbookPersistent>,
) -> Result<HashMap<String, f64>, DeepBookError> {
    let connection = &mut state.pool.get().await?;

    // Calculate the timestamp for 24 hours ago
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| DeepBookError::InternalError("System time error".to_string()))?
        .as_millis() as i64;

    let timestamp_24h_ago = now - (24 * 60 * 60 * 1000); // 24 hours in milliseconds
    let timestamp_48h_ago = now - (48 * 60 * 60 * 1000); // 24 hours in milliseconds

    let mut response = HashMap::new();

    for (pool_name, (pool_id, (base_decimals, quote_decimals))) in pool_metadata.iter() {
        // Get the latest price <= 24 hours ago. Only trades until 48 hours ago will count.
        let earliest_trade_24h = schema::order_fills::table
            .filter(
                schema::order_fills::checkpoint_timestamp_ms
                    .between(timestamp_48h_ago, timestamp_24h_ago),
            )
            .filter(schema::order_fills::pool_id.eq(pool_id))
            .order_by(schema::order_fills::checkpoint_timestamp_ms.desc())
            .select(schema::order_fills::price)
            .first::<i64>(connection)
            .await;

        // Get the most recent price. Only trades until 24 hours ago will count.
        let most_recent_trade = schema::order_fills::table
            .filter(schema::order_fills::checkpoint_timestamp_ms.between(timestamp_24h_ago, now))
            .filter(schema::order_fills::pool_id.eq(pool_id))
            .order_by(schema::order_fills::checkpoint_timestamp_ms.desc())
            .select(schema::order_fills::price)
            .first::<i64>(connection)
            .await;

        if let (Ok(earliest_price), Ok(most_recent_price)) = (earliest_trade_24h, most_recent_trade)
        {
            let price_factor = 10u64.pow((9 - base_decimals + quote_decimals) as u32);

            // Scale the prices
            let earliest_price_scaled = earliest_price as f64 / price_factor as f64;
            let most_recent_price_scaled = most_recent_price as f64 / price_factor as f64;

            // Calculate price change percentage
            let price_change_percent =
                ((most_recent_price_scaled / earliest_price_scaled) - 1.0) * 100.0;

            response.insert(pool_name.clone(), price_change_percent);
        } else {
            // If there's no price data for 24 hours or recent trades, insert 0.0 as price change
            response.insert(pool_name.clone(), 0.0);
        }
    }

    Ok(response)
}

async fn order_updates(
    Path(pool_name): Path<String>,
    Query(params): Query<HashMap<String, String>>,
    State(state): State<PgDeepbookPersistent>,
) -> Result<Json<Vec<HashMap<String, Value>>>, DeepBookError> {
    let connection = &mut state.pool.get().await?;

    // Fetch pool data with proper error handling
    let (pool_id, base_decimals, quote_decimals) = schema::pools::table
        .filter(schema::pools::pool_name.eq(pool_name.clone()))
        .select((
            schema::pools::pool_id,
            schema::pools::base_asset_decimals,
            schema::pools::quote_asset_decimals,
        ))
        .first::<(String, i16, i16)>(connection)
        .await
        .map_err(|_| DeepBookError::InternalError(format!("Pool '{}' not found", pool_name)))?;

    let base_decimals = base_decimals as u8;
    let quote_decimals = quote_decimals as u8;

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

    let limit = params
        .get("limit")
        .and_then(|v| v.parse::<i64>().ok())
        .unwrap_or(1);

    let mut query = schema::order_updates::table
        .filter(schema::order_updates::checkpoint_timestamp_ms.between(start_time, end_time))
        .filter(schema::order_updates::pool_id.eq(pool_id))
        .order_by(schema::order_updates::checkpoint_timestamp_ms.desc())
        .select((
            schema::order_updates::order_id,
            schema::order_updates::price,
            schema::order_updates::original_quantity,
            schema::order_updates::quantity,
            schema::order_updates::filled_quantity,
            schema::order_updates::checkpoint_timestamp_ms,
            schema::order_updates::is_bid,
            schema::order_updates::balance_manager_id,
            schema::order_updates::status,
        ))
        .limit(limit)
        .into_boxed();

    let balance_manager_filter = params.get("balance_manager_id").cloned();
    if let Some(manager_id) = balance_manager_filter {
        query = query.filter(schema::order_updates::balance_manager_id.eq(manager_id));
    }

    let status_filter = params.get("status").cloned();
    if let Some(status) = status_filter {
        query = query.filter(schema::order_updates::status.eq(status));
    }

    let trades = query
        .load::<(String, i64, i64, i64, i64, i64, bool, String, String)>(connection)
        .await
        .map_err(|_| DeepBookError::InternalError("Error fetching trade details".to_string()))?;

    let base_factor = 10u64.pow(base_decimals as u32);
    let price_factor = 10u64.pow((9 - base_decimals + quote_decimals) as u32);

    let trade_data: Vec<HashMap<String, Value>> = trades
        .into_iter()
        .map(
            |(
                order_id,
                price,
                original_quantity,
                quantity,
                filled_quantity,
                timestamp,
                is_bid,
                balance_manager_id,
                status,
            )| {
                let trade_type = if is_bid { "buy" } else { "sell" };
                HashMap::from([
                    ("order_id".to_string(), Value::from(order_id)),
                    (
                        "price".to_string(),
                        Value::from(price as f64 / price_factor as f64),
                    ),
                    (
                        "original_quantity".to_string(),
                        Value::from(original_quantity as f64 / base_factor as f64),
                    ),
                    (
                        "remaining_quantity".to_string(),
                        Value::from(quantity as f64 / base_factor as f64),
                    ),
                    (
                        "filled_quantity".to_string(),
                        Value::from(filled_quantity as f64 / base_factor as f64),
                    ),
                    ("timestamp".to_string(), Value::from(timestamp as u64)),
                    ("type".to_string(), Value::from(trade_type)),
                    (
                        "balance_manager_id".to_string(),
                        Value::from(balance_manager_id),
                    ),
                    ("status".to_string(), Value::from(status)),
                ])
            },
        )
        .collect();

    Ok(Json(trade_data))
}

async fn trades(
    Path(pool_name): Path<String>,
    Query(params): Query<HashMap<String, String>>,
    State(state): State<PgDeepbookPersistent>,
) -> Result<Json<Vec<HashMap<String, Value>>>, DeepBookError> {
    // Fetch all pools to map names to IDs and decimals
    let connection = &mut state.pool.get().await?;
    let pool_data = schema::pools::table
        .filter(schema::pools::pool_name.eq(pool_name.clone()))
        .select((
            schema::pools::pool_id,
            schema::pools::base_asset_decimals,
            schema::pools::quote_asset_decimals,
        ))
        .first::<(String, i16, i16)>(connection)
        .await
        .map_err(|_| DeepBookError::InternalError(format!("Pool '{}' not found", pool_name)))?;

    // Parse start_time and end_time
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

    // Parse limit (default to 1 if not provided)
    let limit = params
        .get("limit")
        .and_then(|v| v.parse::<i64>().ok())
        .unwrap_or(1);

    // Parse optional filters for balance managers
    let maker_balance_manager_filter = params.get("maker_balance_manager_id").cloned();
    let taker_balance_manager_filter = params.get("taker_balance_manager_id").cloned();

    let (pool_id, base_decimals, quote_decimals) = pool_data;
    let base_decimals = base_decimals as u8;
    let quote_decimals = quote_decimals as u8;

    // Build the query dynamically
    let mut query = schema::order_fills::table
        .filter(schema::order_fills::pool_id.eq(pool_id))
        .filter(schema::order_fills::checkpoint_timestamp_ms.between(start_time, end_time))
        .into_boxed();

    // Apply optional filters if parameters are provided
    if let Some(maker_id) = maker_balance_manager_filter {
        query = query.filter(schema::order_fills::maker_balance_manager_id.eq(maker_id));
    }
    if let Some(taker_id) = taker_balance_manager_filter {
        query = query.filter(schema::order_fills::taker_balance_manager_id.eq(taker_id));
    }

    // Fetch latest trades (sorted by timestamp in descending order) within the time range, applying the limit
    let trades = query
        .order_by(schema::order_fills::checkpoint_timestamp_ms.desc()) // Ensures latest trades come first
        .limit(limit) // Apply limit to get the most recent trades
        .select((
            schema::order_fills::maker_order_id,
            schema::order_fills::taker_order_id,
            schema::order_fills::price,
            schema::order_fills::base_quantity,
            schema::order_fills::quote_quantity,
            schema::order_fills::checkpoint_timestamp_ms,
            schema::order_fills::taker_is_bid,
            schema::order_fills::maker_balance_manager_id,
            schema::order_fills::taker_balance_manager_id,
        ))
        .load::<(String, String, i64, i64, i64, i64, bool, String, String)>(connection)
        .await
        .map_err(|_| {
            DeepBookError::InternalError(format!(
                "No trades found for pool '{}' in the specified time range",
                pool_name
            ))
        })?;

    // Conversion factors for decimals
    let base_factor = 10u64.pow(base_decimals as u32);
    let quote_factor = 10u64.pow(quote_decimals as u32);
    let price_factor = 10u64.pow((9 - base_decimals + quote_decimals) as u32);

    // Map trades to JSON format
    let trade_data: Vec<HashMap<String, Value>> = trades
        .into_iter()
        .map(
            |(
                maker_order_id,
                taker_order_id,
                price,
                base_quantity,
                quote_quantity,
                timestamp,
                taker_is_bid,
                maker_balance_manager_id,
                taker_balance_manager_id,
            )| {
                let trade_id = calculate_trade_id(&maker_order_id, &taker_order_id).unwrap_or(0);
                let trade_type = if taker_is_bid { "buy" } else { "sell" };

                HashMap::from([
                    ("trade_id".to_string(), Value::from(trade_id.to_string())),
                    ("maker_order_id".to_string(), Value::from(maker_order_id)),
                    ("taker_order_id".to_string(), Value::from(taker_order_id)),
                    (
                        "maker_balance_manager_id".to_string(),
                        Value::from(maker_balance_manager_id),
                    ),
                    (
                        "taker_balance_manager_id".to_string(),
                        Value::from(taker_balance_manager_id),
                    ),
                    (
                        "price".to_string(),
                        Value::from(price as f64 / price_factor as f64),
                    ),
                    (
                        "base_volume".to_string(),
                        Value::from(base_quantity as f64 / base_factor as f64),
                    ),
                    (
                        "quote_volume".to_string(),
                        Value::from(quote_quantity as f64 / quote_factor as f64),
                    ),
                    ("timestamp".to_string(), Value::from(timestamp as u64)),
                    ("type".to_string(), Value::from(trade_type)),
                ])
            },
        )
        .collect();

    Ok(Json(trade_data))
}

async fn trade_count(
    Query(params): Query<HashMap<String, String>>,
    State(state): State<PgDeepbookPersistent>,
) -> Result<Json<i64>, DeepBookError> {
    // Parse start_time and end_time
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

    let connection = &mut state.pool.get().await?;
    let result: i64 = schema::order_fills::table
        .select(count_star())
        .filter(schema::order_fills::checkpoint_timestamp_ms.between(start_time, end_time))
        .first(connection)
        .await?;

    Ok(Json(result))
}

fn calculate_trade_id(maker_id: &str, taker_id: &str) -> Result<u128, DeepBookError> {
    // Parse maker_id and taker_id as u128
    let maker_id = maker_id
        .parse::<u128>()
        .map_err(|_| DeepBookError::InternalError("Invalid maker_id".to_string()))?;
    let taker_id = taker_id
        .parse::<u128>()
        .map_err(|_| DeepBookError::InternalError("Invalid taker_id".to_string()))?;

    // Ignore the most significant bit for both IDs
    let maker_id = maker_id & !(1 << 127);
    let taker_id = taker_id & !(1 << 127);

    // Return the sum of the modified IDs as the trade_id
    Ok(maker_id + taker_id)
}

pub async fn assets(
    State(state): State<PgDeepbookPersistent>,
) -> Result<Json<HashMap<String, HashMap<String, Value>>>, DeepBookError> {
    let connection = &mut state.pool.get().await?;
    let assets = schema::assets::table
        .select((
            schema::assets::symbol,
            schema::assets::name,
            schema::assets::ucid,
            schema::assets::package_address_url,
            schema::assets::package_id,
        ))
        .load::<(String, String, Option<i32>, Option<String>, Option<String>)>(connection)
        .await
        .map_err(|err| DeepBookError::InternalError(format!("Failed to query assets: {}", err)))?;

    let mut response = HashMap::new();

    for (symbol, name, ucid, package_address_url, package_id) in assets {
        let mut asset_info = HashMap::new();
        asset_info.insert("name".to_string(), Value::String(name));
        asset_info.insert(
            "can_withdraw".to_string(),
            Value::String("true".to_string()),
        );
        asset_info.insert("can_deposit".to_string(), Value::String("true".to_string()));

        if let Some(ucid) = ucid {
            asset_info.insert(
                "unified_cryptoasset_id".to_string(),
                Value::String(ucid.to_string()),
            );
        }
        if let Some(addresses) = package_address_url {
            asset_info.insert("contractAddressUrl".to_string(), Value::String(addresses));
        }

        if let Some(addresses) = package_id {
            asset_info.insert("contractAddress".to_string(), Value::String(addresses));
        }

        response.insert(symbol, asset_info);
    }

    Ok(Json(response))
}

/// Level2 data for all pools
async fn orderbook(
    Path(pool_name): Path<String>,
    Query(params): Query<HashMap<String, String>>,
    State(state): State<PgDeepbookPersistent>,
) -> Result<Json<HashMap<String, Value>>, DeepBookError> {
    let depth = params
        .get("depth")
        .map(|v| v.parse::<u64>())
        .transpose()
        .map_err(|_| {
            DeepBookError::InternalError("Depth must be a non-negative integer".to_string())
        })?
        .map(|depth| if depth == 0 { 200 } else { depth });

    if let Some(depth) = depth {
        if depth == 1 {
            return Err(DeepBookError::InternalError(
                "Depth cannot be 1. Use a value greater than 1 or 0 for the entire orderbook"
                    .to_string(),
            ));
        }
    }

    let level = params
        .get("level")
        .map(|v| v.parse::<u64>())
        .transpose()
        .map_err(|_| {
            DeepBookError::InternalError("Level must be an integer between 1 and 2".to_string())
        })?;

    if let Some(level) = level {
        if !(1..=2).contains(&level) {
            return Err(DeepBookError::InternalError(
                "Level must be 1 or 2".to_string(),
            ));
        }
    }

    let ticks_from_mid = match (depth, level) {
        (Some(_), Some(1)) => 1u64, // Depth + Level 1 → Best bid and ask
        (Some(depth), Some(2)) | (Some(depth), None) => depth / 2, // Depth + Level 2 → Use depth
        (None, Some(1)) => 1u64,    // Only Level 1 → Best bid and ask
        (None, Some(2)) | (None, None) => 100u64, // Level 2 or default → 100 ticks
        _ => 100u64,                // Fallback to default
    };

    // Fetch the pool data from the `pools` table
    let connection = &mut state.pool.get().await?;
    let pool_data = schema::pools::table
        .filter(schema::pools::pool_name.eq(pool_name.clone()))
        .select((
            schema::pools::pool_id,
            schema::pools::base_asset_id,
            schema::pools::base_asset_decimals,
            schema::pools::quote_asset_id,
            schema::pools::quote_asset_decimals,
        ))
        .first::<(String, String, i16, String, i16)>(connection)
        .await?;

    let (pool_id, base_asset_id, base_decimals, quote_asset_id, quote_decimals) = pool_data;
    let base_decimals = base_decimals as u8;
    let quote_decimals = quote_decimals as u8;

    let pool_address = ObjectID::from_hex_literal(&pool_id)?;

    let sui_client = SuiClientBuilder::default().build(SUI_MAINNET_URL).await?;
    let mut ptb = ProgrammableTransactionBuilder::new();

    let pool_object: SuiObjectResponse = sui_client
        .read_api()
        .get_object_with_options(pool_address, SuiObjectDataOptions::full_content())
        .await?;
    let pool_data: &SuiObjectData =
        pool_object
            .data
            .as_ref()
            .ok_or(DeepBookError::InternalError(format!(
                "Missing data in pool object response for '{}'",
                pool_name
            )))?;
    let pool_object_ref: ObjectRef = (pool_data.object_id, pool_data.version, pool_data.digest);

    let pool_input = CallArg::Object(ObjectArg::ImmOrOwnedObject(pool_object_ref));
    ptb.input(pool_input)?;

    let input_argument = CallArg::Pure(bcs::to_bytes(&ticks_from_mid).map_err(|_| {
        DeepBookError::InternalError("Failed to serialize ticks_from_mid".to_string())
    })?);
    ptb.input(input_argument)?;

    let sui_clock_object_id = ObjectID::from_hex_literal(
        "0x0000000000000000000000000000000000000000000000000000000000000006",
    )?;
    let sui_clock_object: SuiObjectResponse = sui_client
        .read_api()
        .get_object_with_options(sui_clock_object_id, SuiObjectDataOptions::full_content())
        .await?;
    let clock_data: &SuiObjectData =
        sui_clock_object
            .data
            .as_ref()
            .ok_or(DeepBookError::InternalError(
                "Missing data in clock object response".to_string(),
            ))?;

    let sui_clock_object_ref: ObjectRef =
        (clock_data.object_id, clock_data.version, clock_data.digest);

    let clock_input = CallArg::Object(ObjectArg::ImmOrOwnedObject(sui_clock_object_ref));
    ptb.input(clock_input)?;

    let base_coin_type = parse_type_input(&base_asset_id)?;
    let quote_coin_type = parse_type_input(&quote_asset_id)?;

    let package = ObjectID::from_hex_literal(DEEPBOOK_PACKAGE_ID)
        .map_err(|e| DeepBookError::InternalError(format!("Invalid pool ID: {}", e)))?;
    let module = LEVEL2_MODULE.to_string();
    let function = LEVEL2_FUNCTION.to_string();

    ptb.command(Command::MoveCall(Box::new(ProgrammableMoveCall {
        package,
        module,
        function,
        type_arguments: vec![base_coin_type, quote_coin_type],
        arguments: vec![Argument::Input(0), Argument::Input(1), Argument::Input(2)],
    })));

    let builder = ptb.finish();
    let tx = TransactionKind::ProgrammableTransaction(builder);

    let result = sui_client
        .read_api()
        .dev_inspect_transaction_block(SuiAddress::default(), tx, None, None, None)
        .await?;

    let mut binding = result.results.ok_or(DeepBookError::InternalError(
        "No results from dev_inspect_transaction_block".to_string(),
    ))?;
    let bid_prices = &binding
        .first_mut()
        .ok_or(DeepBookError::InternalError(
            "No return values for bid prices".to_string(),
        ))?
        .return_values
        .first_mut()
        .ok_or(DeepBookError::InternalError(
            "No bid price data found".to_string(),
        ))?
        .0;
    let bid_parsed_prices: Vec<u64> = bcs::from_bytes(bid_prices).map_err(|_| {
        DeepBookError::InternalError("Failed to deserialize bid prices".to_string())
    })?;
    let bid_quantities = &binding
        .first_mut()
        .ok_or(DeepBookError::InternalError(
            "No return values for bid quantities".to_string(),
        ))?
        .return_values
        .get(1)
        .ok_or(DeepBookError::InternalError(
            "No bid quantity data found".to_string(),
        ))?
        .0;
    let bid_parsed_quantities: Vec<u64> = bcs::from_bytes(bid_quantities).map_err(|_| {
        DeepBookError::InternalError("Failed to deserialize bid quantities".to_string())
    })?;

    let ask_prices = &binding
        .first_mut()
        .ok_or(DeepBookError::InternalError(
            "No return values for ask prices".to_string(),
        ))?
        .return_values
        .get(2)
        .ok_or(DeepBookError::InternalError(
            "No ask price data found".to_string(),
        ))?
        .0;
    let ask_parsed_prices: Vec<u64> = bcs::from_bytes(ask_prices).map_err(|_| {
        DeepBookError::InternalError("Failed to deserialize ask prices".to_string())
    })?;
    let ask_quantities = &binding
        .first_mut()
        .ok_or(DeepBookError::InternalError(
            "No return values for ask quantities".to_string(),
        ))?
        .return_values
        .get(3)
        .ok_or(DeepBookError::InternalError(
            "No ask quantity data found".to_string(),
        ))?
        .0;
    let ask_parsed_quantities: Vec<u64> = bcs::from_bytes(ask_quantities).map_err(|_| {
        DeepBookError::InternalError("Failed to deserialize ask quantities".to_string())
    })?;

    let mut result = HashMap::new();

    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| DeepBookError::InternalError("System time error".to_string()))?
        .as_millis() as i64;
    result.insert("timestamp".to_string(), Value::from(timestamp.to_string()));

    let bids: Vec<Value> = bid_parsed_prices
        .into_iter()
        .zip(bid_parsed_quantities.into_iter())
        .take(ticks_from_mid as usize)
        .map(|(price, quantity)| {
            let price_factor = 10u64.pow((9 - base_decimals + quote_decimals).into());
            let quantity_factor = 10u64.pow((base_decimals).into());
            Value::Array(vec![
                Value::from((price as f64 / price_factor as f64).to_string()),
                Value::from((quantity as f64 / quantity_factor as f64).to_string()),
            ])
        })
        .collect();
    result.insert("bids".to_string(), Value::Array(bids));

    let asks: Vec<Value> = ask_parsed_prices
        .into_iter()
        .zip(ask_parsed_quantities.into_iter())
        .take(ticks_from_mid as usize)
        .map(|(price, quantity)| {
            let price_factor = 10u64.pow((9 - base_decimals + quote_decimals).into());
            let quantity_factor = 10u64.pow((base_decimals).into());
            Value::Array(vec![
                Value::from((price as f64 / price_factor as f64).to_string()),
                Value::from((quantity as f64 / quantity_factor as f64).to_string()),
            ])
        })
        .collect();
    result.insert("asks".to_string(), Value::Array(asks));

    Ok(Json(result))
}

/// DEEP total supply
async fn deep_supply() -> Result<Json<u64>, DeepBookError> {
    let sui_client = SuiClientBuilder::default().build(SUI_MAINNET_URL).await?;
    let mut ptb = ProgrammableTransactionBuilder::new();

    let deep_treasury_object_id = ObjectID::from_hex_literal(DEEP_TREASURY_ID)?;
    let deep_treasury_object: SuiObjectResponse = sui_client
        .read_api()
        .get_object_with_options(
            deep_treasury_object_id,
            SuiObjectDataOptions::full_content(),
        )
        .await?;
    let deep_treasury_data: &SuiObjectData =
        deep_treasury_object
            .data
            .as_ref()
            .ok_or(DeepBookError::InternalError(
                "Incorrect Treasury ID".to_string(),
            ))?;

    let deep_treasury_ref: ObjectRef = (
        deep_treasury_data.object_id,
        deep_treasury_data.version,
        deep_treasury_data.digest,
    );

    let deep_treasury_input = CallArg::Object(ObjectArg::ImmOrOwnedObject(deep_treasury_ref));
    ptb.input(deep_treasury_input)?;

    let package = ObjectID::from_hex_literal(DEEP_TOKEN_PACKAGE_ID).map_err(|e| {
        DeepBookError::InternalError(format!("Invalid deep token package ID: {}", e))
    })?;
    let module = DEEP_SUPPLY_MODULE.to_string();
    let function = DEEP_SUPPLY_FUNCTION.to_string();

    ptb.command(Command::MoveCall(Box::new(ProgrammableMoveCall {
        package,
        module,
        function,
        type_arguments: vec![],
        arguments: vec![Argument::Input(0)],
    })));

    let builder = ptb.finish();
    let tx = TransactionKind::ProgrammableTransaction(builder);

    let result = sui_client
        .read_api()
        .dev_inspect_transaction_block(SuiAddress::default(), tx, None, None, None)
        .await?;

    let mut binding = result.results.ok_or(DeepBookError::InternalError(
        "No results from dev_inspect_transaction_block".to_string(),
    ))?;

    let total_supply = &binding
        .first_mut()
        .ok_or(DeepBookError::InternalError(
            "No return values for total supply".to_string(),
        ))?
        .return_values
        .first_mut()
        .ok_or(DeepBookError::InternalError(
            "No total supply data found".to_string(),
        ))?
        .0;

    let total_supply_value: u64 = bcs::from_bytes(total_supply).map_err(|_| {
        DeepBookError::InternalError("Failed to deserialize total supply".to_string())
    })?;

    Ok(Json(total_supply_value))
}

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

fn parse_type_input(type_str: &str) -> Result<TypeInput, DeepBookError> {
    let type_tag = TypeTag::from_str(type_str)?;
    Ok(TypeInput::from(type_tag))
}
