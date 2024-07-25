// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::schema::{
    progress_store, sui_error_transactions, sui_progress_store, token_transfer, token_transfer_data,
};
use diesel::data_types::PgTimestamp;
use diesel::{Identifiable, Insertable, Queryable, Selectable};

#[derive(Queryable, Selectable, Insertable, Identifiable, Debug)]
#[diesel(table_name = progress_store, primary_key(task_name))]
pub struct ProgressStore {
    pub task_name: String,
    pub checkpoint: i64,
    pub target_checkpoint: i64,
    pub timestamp: Option<PgTimestamp>,
}

#[derive(Queryable, Selectable, Insertable, Identifiable, Debug)]
#[diesel(table_name = sui_progress_store, primary_key(txn_digest))]
pub struct SuiProgressStore {
    pub id: i32, // Dummy value
    pub txn_digest: Vec<u8>,
}

#[derive(Queryable, Selectable, Insertable, Identifiable, Debug)]
#[diesel(table_name = token_transfer, primary_key(chain_id, nonce))]
pub struct TokenTransfer {
    pub chain_id: i32,
    pub nonce: i64,
    pub status: String,
    pub block_height: i64,
    pub timestamp_ms: i64,
    pub txn_hash: Vec<u8>,
    pub txn_sender: Vec<u8>,
    pub gas_usage: i64,
    pub data_source: String,
}

#[derive(Queryable, Selectable, Insertable, Identifiable, Debug)]
#[diesel(table_name = token_transfer_data, primary_key(chain_id, nonce))]
pub struct TokenTransferData {
    pub chain_id: i32,
    pub nonce: i64,
    pub block_height: i64,
    pub timestamp_ms: i64,
    pub txn_hash: Vec<u8>,
    pub sender_address: Vec<u8>,
    pub destination_chain: i32,
    pub recipient_address: Vec<u8>,
    pub token_id: i32,
    pub amount: i64,
}

#[derive(Queryable, Selectable, Insertable, Identifiable, Debug)]
#[diesel(table_name = sui_error_transactions, primary_key(txn_digest))]
pub struct SuiErrorTransactions {
    pub txn_digest: Vec<u8>,
    pub sender_address: Vec<u8>,
    pub timestamp_ms: i64,
    pub failure_status: String,
    pub cmd_idx: Option<i64>,
}
