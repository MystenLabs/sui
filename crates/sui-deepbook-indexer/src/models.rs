// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use bigdecimal::BigDecimal;
use diesel::data_types::PgTimestamp;
use diesel::{Identifiable, Insertable, Queryable, Selectable};

use sui_indexer_builder::Task;

use crate::schema::{
    flashloans, order_fills, order_updates, pool_prices, progress_store, sui_error_transactions,
};

#[derive(Queryable, Selectable, Insertable, Identifiable, Debug)]
#[diesel(table_name = order_updates, primary_key(digest))]
pub struct OrderUpdate {
    pub digest: String,
    pub sender: String,
    pub checkpoint: i64,
    pub status: String,
    pub pool_id: String,
    pub order_id: BigDecimal,
    pub client_order_id: i64,
    pub price: i64,
    pub is_bid: bool,
    pub quantity: i64,
    pub onchain_timestamp: i64,
    pub trader: String,
    pub balance_manager_id: String,
}

#[derive(Queryable, Selectable, Insertable, Identifiable, Debug)]
#[diesel(table_name = order_fills, primary_key(digest))]
pub struct OrderFill {
    pub digest: String,
    pub sender: String,
    pub checkpoint: i64,
    pub pool_id: String,
    pub maker_order_id: BigDecimal,
    pub taker_order_id: BigDecimal,
    pub maker_client_order_id: i64,
    pub taker_client_order_id: i64,
    pub price: i64,
    pub taker_is_bid: bool,
    pub base_quantity: i64,
    pub quote_quantity: i64,
    pub maker_balance_manager_id: String,
    pub taker_balance_manager_id: String,
    pub onchain_timestamp: i64,
}

#[derive(Queryable, Selectable, Insertable, Identifiable, Debug)]
#[diesel(table_name = flashloans, primary_key(digest))]
pub struct Flashloan {
    pub digest: String,
    pub sender: String,
    pub checkpoint: i64,
    pub pool_id: String,
    pub borrow_quantity: i64,
    pub borrow: bool,
    pub type_name: String,
}

#[derive(Queryable, Selectable, Insertable, Identifiable, Debug)]
#[diesel(table_name = pool_prices, primary_key(digest))]
pub struct PoolPrice {
    pub digest: String,
    pub sender: String,
    pub checkpoint: i64,
    pub target_pool: String,
    pub reference_pool: String,
    pub conversion_rate: i64,
}

#[derive(Queryable, Selectable, Insertable, Identifiable, Debug)]
#[diesel(table_name = sui_error_transactions, primary_key(txn_digest))]
pub struct SuiErrorTransactions {
    pub txn_digest: String,
    pub sender_address: String,
    pub timestamp_ms: i64,
    pub failure_status: String,
    pub cmd_idx: Option<i64>,
}

#[derive(Queryable, Selectable, Insertable, Identifiable, Debug)]
#[diesel(table_name = progress_store, primary_key(task_name))]
pub struct ProgressStore {
    pub task_name: String,
    pub checkpoint: i64,
    pub target_checkpoint: i64,
    pub timestamp: Option<PgTimestamp>,
}

impl From<ProgressStore> for Task {
    fn from(value: ProgressStore) -> Self {
        Self {
            task_name: value.task_name,
            checkpoint: value.checkpoint as u64,
            target_checkpoint: value.target_checkpoint as u64,
            // Ok to unwrap, timestamp is defaulted to now() in database
            timestamp: value.timestamp.expect("Timestamp not set").0 as u64,
        }
    }
}
