// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use diesel::data_types::PgTimestamp;
use diesel::{Identifiable, Insertable, Queryable, Selectable};

use sui_indexer_builder::{Task, LIVE_TASK_TARGET_CHECKPOINT};

use crate::schema::{
    balances, flashloans, order_fills, order_updates, pool_prices, progress_store, proposals,
    rebates, stakes, sui_error_transactions, trade_params_update, votes,
};

#[derive(Queryable, Selectable, Insertable, Identifiable, Debug)]
#[diesel(table_name = order_updates, primary_key(digest))]
pub struct OrderUpdate {
    pub digest: String,
    pub sender: String,
    pub checkpoint: i64,
    pub package: String,
    pub status: String,
    pub pool_id: String,
    pub order_id: String, // u128
    pub client_order_id: i64,
    pub price: i64,
    pub is_bid: bool,
    pub original_quantity: i64,
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
    pub package: String,
    pub pool_id: String,
    pub maker_order_id: String, // u128
    pub taker_order_id: String, // u128
    pub maker_client_order_id: i64,
    pub taker_client_order_id: i64,
    pub price: i64,
    pub taker_fee: i64,
    pub maker_fee: i64,
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
    pub package: String,
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
    pub package: String,
    pub target_pool: String,
    pub reference_pool: String,
    pub conversion_rate: i64,
}

#[derive(Queryable, Selectable, Insertable, Identifiable, Debug)]
#[diesel(table_name = balances, primary_key(digest))]
pub struct Balances {
    pub digest: String,
    pub sender: String,
    pub checkpoint: i64,
    pub package: String,
    pub balance_manager_id: String,
    pub asset: String,
    pub amount: i64,
    pub deposit: bool,
}

#[derive(Queryable, Selectable, Insertable, Identifiable, Debug)]
#[diesel(table_name = proposals, primary_key(digest))]
pub struct Proposals {
    pub digest: String,
    pub sender: String,
    pub checkpoint: i64,
    pub package: String,
    pub balance_manager_id: String,
    pub epoch: i64,
    pub taker_fee: i64,
    pub maker_fee: i64,
    pub stake_required: i64,
}

#[derive(Queryable, Selectable, Insertable, Identifiable, Debug)]
#[diesel(table_name = rebates, primary_key(digest))]
pub struct Rebates {
    pub digest: String,
    pub sender: String,
    pub checkpoint: i64,
    pub package: String,
    pub pool_id: String,
    pub balance_manager_id: String,
    pub epoch: i64,
    pub claim_amount: i64,
}

#[derive(Queryable, Selectable, Insertable, Identifiable, Debug)]
#[diesel(table_name = stakes, primary_key(digest))]
pub struct Stakes {
    pub digest: String,
    pub sender: String,
    pub checkpoint: i64,
    pub package: String,
    pub pool_id: String,
    pub balance_manager_id: String,
    pub epoch: i64,
    pub amount: i64,
    pub stake: bool,
}

#[derive(Queryable, Selectable, Insertable, Identifiable, Debug)]
#[diesel(table_name = trade_params_update, primary_key(digest))]
pub struct TradeParamsUpdate {
    pub digest: String,
    pub sender: String,
    pub checkpoint: i64,
    pub package: String,
    pub pool_id: String,
    pub taker_fee: i64,
    pub maker_fee: i64,
    pub stake_required: i64,
}

#[derive(Queryable, Selectable, Insertable, Identifiable, Debug)]
#[diesel(table_name = votes, primary_key(digest))]
pub struct Votes {
    pub digest: String,
    pub sender: String,
    pub checkpoint: i64,
    pub package: String,
    pub pool_id: String,
    pub balance_manager_id: String,
    pub epoch: i64,
    pub from_proposal_id: Option<String>,
    pub to_proposal_id: String,
    pub stake: i64,
}

#[derive(Queryable, Selectable, Insertable, Identifiable, Debug)]
#[diesel(table_name = sui_error_transactions, primary_key(txn_digest))]
pub struct SuiErrorTransactions {
    pub txn_digest: String,
    pub sender_address: String,
    pub timestamp_ms: i64,
    pub failure_status: String,
    pub package: String,
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
            start_checkpoint: value.checkpoint as u64,
            target_checkpoint: value.target_checkpoint as u64,
            // Ok to unwrap, timestamp is defaulted to now() in database
            timestamp: value.timestamp.expect("Timestamp not set").0 as u64,
            is_live_task: value.target_checkpoint == LIVE_TASK_TARGET_CHECKPOINT,
        }
    }
}
