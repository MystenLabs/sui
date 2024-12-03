// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::{
    error::DeepBookError,
    models::{OrderFillSummary, Pools},
    schema::{self},
    sui_deepbook_indexer::PgDeepbookPersistent,
};
use anyhow::anyhow;
use axum::{
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
use serde_json::Value;
use std::time::{SystemTime, UNIX_EPOCH};
use std::{collections::HashMap, net::SocketAddr};
use tokio::{net::TcpListener, task::JoinHandle};

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

pub const SUI_MAINNET_URL: &str = "https://fullnode.mainnet.sui.io:443";
pub const GET_POOLS_PATH: &str = "/get_pools";
pub const GET_HISTORICAL_VOLUME_BY_BALANCE_MANAGER_ID_WITH_INTERVAL: &str =
    "/get_historical_volume_by_balance_manager_id_with_interval/:pool_ids/:balance_manager_id";
pub const GET_HISTORICAL_VOLUME_BY_BALANCE_MANAGER_ID: &str =
    "/get_historical_volume_by_balance_manager_id/:pool_ids/:balance_manager_id";
pub const GET_HISTORICAL_VOLUME_PATH: &str = "/get_historical_volume/:pool_ids";
pub const GET_ALL_HISTORICAL_VOLUME_PATH: &str = "/get_all_historical_volume";
pub const LEVEL2_PATH: &str = "/orderbook/:pool_name";
pub const DEEPBOOK_PACKAGE_ID: &str =
    "0x2c8d603bc51326b8c13cef9dd07031a408a48dddb541963357661df5d3204809";

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
            GET_ALL_HISTORICAL_VOLUME_PATH,
            get(get_all_historical_volume),
        )
        .route(
            GET_HISTORICAL_VOLUME_BY_BALANCE_MANAGER_ID_WITH_INTERVAL,
            get(get_historical_volume_by_balance_manager_id_with_interval),
        )
        .route(
            GET_HISTORICAL_VOLUME_BY_BALANCE_MANAGER_ID,
            get(get_historical_volume_by_balance_manager_id),
        )
        .route(LEVEL2_PATH, get(get_level2_ticks_from_mid))
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

    Ok(Json(normalize_pool_addresses(volume_by_pool)))
}

async fn get_all_historical_volume(
    Query(params): Query<HashMap<String, String>>,
    State(state): State<PgDeepbookPersistent>,
) -> Result<Json<HashMap<String, u64>>, DeepBookError> {
    // Step 1: Clone `state.pool` into a separate variable to ensure it lives long enough
    let pool = state.pool.clone();

    // Step 2: Use the cloned pool to get a connection
    let connection = &mut pool.get().await?;

    // Step 3: Fetch all pool IDs
    let pools: Vec<Pools> = schema::pools::table
        .select(Pools::as_select())
        .load(connection)
        .await?;

    // Extract all pool IDs
    let pool_ids: String = pools
        .into_iter()
        .map(|pool| pool.pool_id)
        .collect::<Vec<String>>()
        .join(",");

    // Step 4: Call `get_historical_volume` with the pool IDs and query parameters
    get_historical_volume(Path(pool_ids), Query(params), State(state)).await
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

async fn get_level2_ticks_from_mid(
    Path(pool_name): Path<String>,
    Query(params): Query<HashMap<String, String>>,
) -> Result<Json<HashMap<String, Value>>, DeepBookError> {
    let depth = params
        .get("depth")
        .map(|v| v.parse::<u64>())
        .transpose()
        .map_err(|_| anyhow!("Depth must be a non-negative integer"))?
        .map(|depth| if depth == 0 { 200 } else { depth });

    if let Some(depth) = depth {
        if depth == 1 {
            return Err(anyhow!(
                "Depth cannot be 1. Use a value greater than 1 or 0 for the entire orderbook"
            )
            .into());
        }
    }

    let level = params
        .get("level")
        .map(|v| v.parse::<u64>())
        .transpose()
        .map_err(|_| anyhow!("Level must be an integer between 1 and 2"))?;

    if let Some(level) = level {
        if !(1..=2).contains(&level) {
            return Err(anyhow!("Level must be 1 or 2").into());
        }
    }

    let ticks_from_mid = match (depth, level) {
        (Some(_), Some(1)) => 1u64, // Depth + Level 1 → Best bid and ask
        (Some(depth), Some(2)) | (Some(depth), None) => depth / 2, // Depth + Level 2 → Use depth
        (None, Some(1)) => 1u64,    // Only Level 1 → Best bid and ask
        (None, Some(2)) | (None, None) => 100u64, // Level 2 or default → 100 ticks
        _ => 100u64,                // Fallback to default
    };

    let sui_client = SuiClientBuilder::default().build(SUI_MAINNET_URL).await?;
    let mut ptb = ProgrammableTransactionBuilder::new();

    let pool_name_map = get_pool_name_mapping();
    let pool_id = pool_name_map
        .iter()
        .find(|(_, name)| name == &&pool_name)
        .map(|(address, _)| address)
        .ok_or_else(|| anyhow!("Pool name '{}' not found", pool_name))?;

    let pool_address = ObjectID::from_hex_literal(pool_id)?;

    let pool_object: SuiObjectResponse = sui_client
        .read_api()
        .get_object_with_options(pool_address, SuiObjectDataOptions::full_content())
        .await?;
    let pool_data: &SuiObjectData = pool_object.data.as_ref().ok_or(anyhow!(
        "Missing data in pool object response for '{}'",
        pool_name
    ))?;
    let pool_object_ref: ObjectRef = (pool_data.object_id, pool_data.version, pool_data.digest);

    let pool_input = CallArg::Object(ObjectArg::ImmOrOwnedObject(pool_object_ref));
    ptb.input(pool_input)?;

    let input_argument = CallArg::Pure(bcs::to_bytes(&ticks_from_mid).unwrap());
    ptb.input(input_argument)?;

    let sui_clock_object_id = ObjectID::from_hex_literal(
        "0x0000000000000000000000000000000000000000000000000000000000000006",
    )?;
    let sui_clock_object: SuiObjectResponse = sui_client
        .read_api()
        .get_object_with_options(sui_clock_object_id, SuiObjectDataOptions::full_content())
        .await?;
    let clock_data: &SuiObjectData = sui_clock_object
        .data
        .as_ref()
        .ok_or(anyhow!("Missing data in clock object response"))?;

    let sui_clock_object_ref: ObjectRef =
        (clock_data.object_id, clock_data.version, clock_data.digest);

    let clock_input = CallArg::Object(ObjectArg::ImmOrOwnedObject(sui_clock_object_ref));
    ptb.input(clock_input)?;

    let pool_full_name = pool_name_map
        .get(pool_id)
        .ok_or_else(|| anyhow!("Pool ID not found"))?;
    let (base_asset, quote_asset) = parse_pool_name(pool_full_name)?;

    let asset_info_map = get_asset_info_mapping();
    let (base_coin_type, base_decimals) = asset_info_map
        .get(&base_asset)
        .ok_or_else(|| anyhow!("Base asset info not found"))?;
    let (quote_coin_type, quote_decimals) = asset_info_map
        .get(&quote_asset)
        .ok_or_else(|| anyhow!("Quote asset info not found"))?;

    let base_coin_type = parse_type_input(base_coin_type)?;
    let quote_coin_type = parse_type_input(quote_coin_type)?;

    let package = ObjectID::from_hex_literal(DEEPBOOK_PACKAGE_ID).map_err(|e| anyhow!(e))?;
    let module = "pool".to_string();
    let function = "get_level2_ticks_from_mid".to_string();

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

    let mut binding = result.results.unwrap();
    let bid_prices = &binding.first_mut().unwrap().return_values.get(0).unwrap().0;
    let bid_parsed_prices: Vec<u64> = bcs::from_bytes(bid_prices).unwrap();
    let bid_quantities = &binding.first_mut().unwrap().return_values.get(1).unwrap().0;
    let bid_parsed_quantities: Vec<u64> = bcs::from_bytes(bid_quantities).unwrap();

    let ask_prices = &binding.first_mut().unwrap().return_values.get(2).unwrap().0;
    let ask_parsed_prices: Vec<u64> = bcs::from_bytes(ask_prices).unwrap();
    let ask_quantities = &binding.first_mut().unwrap().return_values.get(3).unwrap().0;
    let ask_parsed_quantities: Vec<u64> = bcs::from_bytes(ask_quantities).unwrap();

    let mut result = HashMap::new();

    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_millis() as i64;
    result.insert("timestamp".to_string(), Value::from(timestamp.to_string()));

    let bids: Vec<Value> = bid_parsed_prices
        .into_iter()
        .zip(bid_parsed_quantities.into_iter())
        .take(ticks_from_mid as usize)
        .map(|(price, quantity)| {
            let price_factor =
                10u64.pow((9 - *base_decimals + *quote_decimals).try_into().unwrap());
            let quantity_factor = 10u64.pow((*base_decimals).try_into().unwrap());
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
            let price_factor =
                10u64.pow((9 - *base_decimals + *quote_decimals).try_into().unwrap());
            let quantity_factor = 10u64.pow((*base_decimals).try_into().unwrap());
            Value::Array(vec![
                Value::from((price as f64 / price_factor as f64).to_string()),
                Value::from((quantity as f64 / quantity_factor as f64).to_string()),
            ])
        })
        .collect();
    result.insert("asks".to_string(), Value::Array(asks));

    Ok(Json(result))
}

/// Helper function to normalize pool addresses
fn normalize_pool_addresses(raw_response: HashMap<String, u64>) -> HashMap<String, u64> {
    let pool_map = get_pool_name_mapping();

    raw_response
        .into_iter()
        .map(|(address, volume)| {
            let pool_name = pool_map
                .get(&address)
                .unwrap_or(&"Unknown Pool".to_string())
                .to_string();
            (pool_name, volume)
        })
        .collect()
}

/// This function can return what's in the pool table when stable
fn get_pool_name_mapping() -> HashMap<String, String> {
    [
        (
            "0xb663828d6217467c8a1838a03793da896cbe745b150ebd57d82f814ca579fc22",
            "DEEP_SUI",
        ),
        (
            "0xf948981b806057580f91622417534f491da5f61aeaf33d0ed8e69fd5691c95ce",
            "DEEP_USDC",
        ),
        (
            "0xe05dafb5133bcffb8d59f4e12465dc0e9faeaa05e3e342a08fe135800e3e4407",
            "SUI_USDC",
        ),
        (
            "0x1109352b9112717bd2a7c3eb9a416fff1ba6951760f5bdd5424cf5e4e5b3e65c",
            "BWETH_USDC",
        ),
        (
            "0xa0b9ebefb38c963fd115f52d71fa64501b79d1adcb5270563f92ce0442376545",
            "WUSDC_USDC",
        ),
        (
            "0x4e2ca3988246e1d50b9bf209abb9c1cbfec65bd95afdacc620a36c67bdb8452f",
            "WUSDT_USDC",
        ),
        (
            "0x27c4fdb3b846aa3ae4a65ef5127a309aa3c1f466671471a806d8912a18b253e8",
            "NS_SUI",
        ),
        (
            "0x0c0fdd4008740d81a8a7d4281322aee71a1b62c449eb5b142656753d89ebc060",
            "NS_USDC",
        ),
        (
            "0xe8e56f377ab5a261449b92ac42c8ddaacd5671e9fec2179d7933dd1a91200eec",
            "TYPUS_SUI",
        ),
        (
            "0x183df694ebc852a5f90a959f0f563b82ac9691e42357e9a9fe961d71a1b809c8",
            "SUI_AUSD",
        ),
        (
            "0x5661fc7f88fbeb8cb881150a810758cf13700bb4e1f31274a244581b37c303c3",
            "AUSD_USDC",
        ),
    ]
    .iter()
    .map(|&(id, name)| (id.to_string(), name.to_string()))
    .collect()
}

/// This function can return what's in the pool table when stable
fn get_asset_info_mapping() -> HashMap<String, (String, u64)> {
    [
        (
            "SUI",
            (
                "0x0000000000000000000000000000000000000000000000000000000000000002::sui::SUI",
                9,
            ),
        ),
        (
            "DEEP",
            (
                "0xdeeb7a4662eec9f2f3def03fb937a663dddaa2e215b8078a284d026b7946c270::deep::DEEP",
                6,
            ),
        ),
        (
            "USDC",
            (
                "0xdba34672e30cb065b1f93e3ab55318768fd6fef66c15942c9f7cb846e2f900e7::usdc::USDC",
                6,
            ),
        ),
        (
            "BETH",
            (
                "0xd0e89b2af5e4910726fbcd8b8dd37bb79b29e5f83f7491bca830e94f7f226d29::eth::ETH",
                8,
            ),
        ),
        (
            "WUSDC",
            (
                "0x5d4b302506645c37ff133b98c4b50a5ae14841659738d6d733d59d0d217a93bf::coin::COIN",
                6,
            ),
        ),
        (
            "WUSDT",
            (
                "0xc060006111016b8a020ad5b33834984a437aaa7d3c74c18e09a95d48aceab08c::coin::COIN",
                6,
            ),
        ),
        (
            "NS",
            (
                "0x5145494a5f5100e645e4b0aa950fa6b68f614e8c59e17bc5ded3495123a79178::ns::NS",
                6,
            ),
        ),
        (
            "TYPUS",
            (
                "0xf82dc05634970553615eef6112a1ac4fb7bf10272bf6cbe0f80ef44a6c489385::typus::TYPUS",
                9,
            ),
        ),
        (
            "AUSD",
            (
                "0x2053d08c1e2bd02791056171aab0fd12bd7cd7efad2ab8f6b9c8902f14df2ff2::ausd::AUSD",
                6,
            ),
        ),
    ]
    .iter()
    .map(|&(name, (type_str, decimals))| (name.to_string(), (type_str.to_string(), decimals)))
    .collect()
}

fn parse_pool_name(pool_name: &str) -> Result<(String, String), anyhow::Error> {
    let parts: Vec<&str> = pool_name.split('_').collect();
    if parts.len() != 2 {
        return Err(anyhow::anyhow!("Invalid pool name format"));
    }
    Ok((parts[0].to_string(), parts[1].to_string()))
}

fn parse_type_input(type_str: &str) -> Result<TypeInput, DeepBookError> {
    let type_tag = TypeTag::from_str(type_str)?;
    Ok(TypeInput::from(type_tag))
}
