// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::storage::PgBridgePersistent;
use axum::{
    debug_handler,
    extract::{Query, State},
    http::StatusCode,
    routing::get,
    Json, Router,
};
use diesel::sql_query;
use diesel::sql_types::{Bool, Bytea, Int4, Int8, Text};
use diesel::{prelude::*, sql_types::Nullable};
use diesel_async::RunQueryDsl;
use hex;
use serde::ser::Serializer;
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;
use tokio::{net::TcpListener, task::JoinHandle};

pub const GET_TOKEN_TRANSFERS: &str = "/token_transfers";

pub fn run_server(socket_address: SocketAddr, state: PgBridgePersistent) -> JoinHandle<()> {
    tokio::spawn(async move {
        let listener = TcpListener::bind(socket_address).await.unwrap();
        axum::serve(listener, make_router(state)).await.unwrap();
    })
}

pub(crate) fn make_router(state: PgBridgePersistent) -> Router {
    Router::new()
        .route("/", get(health_check))
        .route(GET_TOKEN_TRANSFERS, get(get_token_transfers))
        .with_state(state)
}

impl axum::response::IntoResponse for BridgeIndexerError {
    // TODO: distinguish client error.
    fn into_response(self) -> axum::response::Response {
        match self {
            BridgeIndexerError::InternalError(message) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Internal server error: {}", message),
            )
                .into_response(),
            BridgeIndexerError::InvalidInput(message) => (
                StatusCode::BAD_REQUEST,
                format!("Invalid input: {}", message),
            )
                .into_response(),
        }
    }
}

async fn health_check() -> StatusCode {
    StatusCode::OK
}

#[derive(Deserialize)]
struct QueryParams {
    page: usize,
    per_page: usize,
    chain_id: i32,
    address: Option<String>,
}

#[debug_handler]
async fn get_token_transfers(
    params: Query<QueryParams>,
    State(state): State<PgBridgePersistent>,
) -> Result<Json<Vec<TokenTransferResult>>, BridgeIndexerError> {
    let connection = &mut state.pool.get().await?;

    // Destructure params
    let QueryParams {
        page,
        per_page,
        chain_id,
        address,
    } = params.0;

    let offset = (page * per_page) as i64;
    let limit = per_page as i64;

    let sql = r#"
        SELECT DISTINCT ON (tt.chain_id, tt.nonce)
            tt.chain_id AS chain_id,
            tt.nonce AS nonce,
            tt.status AS status,
            tt.block_height AS block_height,
            tt.timestamp_ms AS timestamp_ms,
            tt.txn_hash AS txn_hash,
            tt.txn_sender AS txn_sender,
            tt.gas_usage AS gas_usage,
            tt.data_source AS data_source,
            tt.is_finalized AS is_finalized,
            ttd.destination_chain AS destination_chain,
            ttd.recipient_address AS recipient_address,
            ttd.token_id AS token_id,
            ttd.amount AS amount
        FROM token_transfer tt
        JOIN token_transfer_data ttd
            ON tt.chain_id = ttd.chain_id AND tt.nonce = ttd.nonce
        WHERE tt.chain_id = $1
          AND ($2::bytea IS NULL OR tt.txn_sender = $2::bytea)
        ORDER BY tt.chain_id, tt.nonce,
            CASE tt.status
                WHEN 'Claimed' THEN 3
                WHEN 'Approved' THEN 2
                WHEN 'Deposited' THEN 1
                ELSE 0
            END DESC
        LIMIT $3 OFFSET $4;
    "#;

    let txn_sender_bytes = match address {
        Some(ref addr) => {
            let addr_trimmed = addr.trim_start_matches("0x");
            let bytes = hex::decode(addr_trimmed).map_err(|e| {
                BridgeIndexerError::InvalidInput(format!("Invalid address format: {}", e))
            })?;
            Some(bytes)
        }
        None => None,
    };

    let results = sql_query(sql)
        .bind::<Int4, _>(chain_id)
        .bind::<Nullable<Bytea>, _>(txn_sender_bytes)
        .bind::<Int8, _>(limit)
        .bind::<Int8, _>(offset)
        .load::<TokenTransferResult>(connection)
        .await?;

    Ok(Json(results))
}

#[derive(Debug, QueryableByName, Serialize)]
struct TokenTransferResult {
    // Fields from token_transfer (tt)
    #[diesel(sql_type = Int4)]
    #[diesel(column_name = chain_id)]
    chain_id: i32,

    #[diesel(sql_type = Int8)]
    #[diesel(column_name = nonce)]
    nonce: i64,

    #[diesel(sql_type = Text)]
    #[diesel(column_name = status)]
    status: String,

    #[diesel(sql_type = Int8)]
    #[diesel(column_name = block_height)]
    block_height: i64,

    #[diesel(sql_type = Int8)]
    #[diesel(column_name = timestamp_ms)]
    timestamp_ms: i64,

    #[serde(serialize_with = "hex_encode")]
    #[diesel(sql_type = Bytea)]
    #[diesel(column_name = txn_hash)]
    txn_hash: Vec<u8>,

    #[serde(serialize_with = "hex_encode")]
    #[diesel(sql_type = Bytea)]
    #[diesel(column_name = txn_sender)]
    txn_sender: Vec<u8>,

    #[diesel(sql_type = Int8)]
    #[diesel(column_name = gas_usage)]
    gas_usage: i64,

    #[diesel(sql_type = Text)]
    #[diesel(column_name = data_source)]
    data_source: String,

    #[diesel(sql_type = Bool)]
    #[diesel(column_name = is_finalized)]
    is_finalized: bool,

    #[diesel(sql_type = Int4)]
    #[diesel(column_name = destination_chain)]
    destination_chain: i32,

    #[serde(serialize_with = "hex_encode")]
    #[diesel(sql_type = Bytea)]
    #[diesel(column_name = recipient_address)]
    recipient_address: Vec<u8>,

    #[diesel(sql_type = Int4)]
    #[diesel(column_name = token_id)]
    token_id: i32,

    #[diesel(sql_type = Int8)]
    #[diesel(column_name = amount)]
    amount: i64,
}

fn hex_encode<S>(bytes: &Vec<u8>, serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    let hex_string = format!("0x{}", hex::encode(bytes));
    serializer.serialize_str(&hex_string)
}

#[derive(Debug, Clone)]
pub enum BridgeIndexerError {
    InternalError(String),
    InvalidInput(String),
}

impl<E> From<E> for BridgeIndexerError
where
    E: Into<anyhow::Error>,
{
    fn from(err: E) -> Self {
        Self::InternalError(err.into().to_string())
    }
}
