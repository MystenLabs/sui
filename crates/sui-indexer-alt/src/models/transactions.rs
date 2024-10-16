// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::schema::{kv_transactions, tx_affected_objects};
use diesel::prelude::*;
use serde::{Deserialize, Serialize};
use sui_types::object::Owner;

/// Even though the balance changes are not a protocol structure, they are stored in the database
/// as a BCS-encoded array. This is mainly to keep sizes down, but when stored in the key-value
/// store, balance changes are likely to be JSON encoded.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredBalanceChange {
    /// Owner whose balance changed
    pub owner: Owner,

    /// Type of the Coin (just the one-time witness type).
    pub coin_type: String,

    /// The amount the balance changed by. A negative amount means the net flow of value is from
    /// the owner, and a positive amount means the net flow of value is to the owner.
    pub amount: i128,
}

#[derive(Insertable, Debug, Clone)]
#[diesel(table_name = kv_transactions)]
pub struct StoredTransaction {
    pub tx_sequence_number: i64,
    pub cp_sequence_number: i64,
    pub timestamp_ms: i64,
    pub raw_transaction: Vec<u8>,
    pub raw_effects: Vec<u8>,
    pub events: Vec<u8>,
    pub balance_changes: Vec<u8>,
}

#[derive(Insertable, Debug, Clone)]
#[diesel(table_name = tx_affected_objects)]
pub struct StoredTxAffectedObjects {
    pub tx_sequence_number: i64,
    pub affected: Vec<u8>,
    pub sender: Vec<u8>,
}
