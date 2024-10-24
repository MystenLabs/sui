// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::schema::{
    kv_transactions, tx_affected_addresses, tx_affected_objects, tx_balance_changes, tx_calls_fun,
    tx_digests, tx_kinds,
};
use diesel::prelude::*;
use serde::{Deserialize, Serialize};
use sui_types::object::Owner;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum BalanceChange {
    V1 {
        /// Owner whose balance changed
        owner: Owner,

        /// Type of the Coin (just the one-time witness type).
        coin_type: String,

        /// The amount the balance changed by. A negative amount means the net flow of value is
        /// from the owner, and a positive amount means the net flow of value is to the owner.
        amount: i128,
    },
}

#[derive(Insertable, Debug, Clone)]
#[diesel(table_name = kv_transactions)]
pub struct StoredTransaction {
    pub tx_digest: Vec<u8>,
    pub cp_sequence_number: i64,
    pub timestamp_ms: i64,
    pub raw_transaction: Vec<u8>,
    pub raw_effects: Vec<u8>,
    pub events: Vec<u8>,
}

#[derive(Insertable, Debug, Clone)]
#[diesel(table_name = tx_affected_addresses)]
pub struct StoredTxAffectedAddress {
    pub tx_sequence_number: i64,
    /// Address affected by the transaction, including the sender, the gas payer
    /// and any recipients of objects.
    pub affected: Vec<u8>,
    pub sender: Vec<u8>,
}

#[derive(Insertable, Debug, Clone)]
#[diesel(table_name = tx_affected_objects)]
pub struct StoredTxAffectedObject {
    pub tx_sequence_number: i64,
    /// Object affected by the transaction, including deleted, wrapped, mutated,
    /// and created objects.
    pub affected: Vec<u8>,
    pub sender: Vec<u8>,
}

#[derive(Insertable, Debug, Clone)]
#[diesel(table_name = tx_balance_changes)]
pub struct StoredTxBalanceChange {
    pub tx_sequence_number: i64,
    pub balance_changes: Vec<u8>,
}

#[derive(Insertable, Debug, Clone)]
#[diesel(table_name = tx_calls_fun)]
pub struct StoredTxCallsFun {
    pub tx_sequence_number: i64,
    pub package: Vec<u8>,
    pub module: String,
    pub func: String,
    pub sender: Vec<u8>,
}

#[derive(Insertable, Debug, Clone)]
#[diesel(table_name = tx_digests)]
pub struct StoredTxDigest {
    pub tx_digest: Vec<u8>,
    pub tx_sequence_number: i64,
}

#[derive(Debug, Clone)]
#[repr(i16)]
pub enum TxKind {
    SystemTransaction = 0,
    ProgrammableTransaction = 1,
}

#[derive(Insertable, Debug, Clone)]
#[diesel(table_name = tx_kinds)]
pub struct StoredTxKind {
    pub tx_sequence_number: i64,
    pub tx_kind: i16,
}
