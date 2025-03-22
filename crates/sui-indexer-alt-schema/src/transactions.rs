// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::schema::{
    kv_transactions, tx_affected_addresses, tx_affected_objects, tx_balance_changes, tx_calls,
    tx_digests, tx_kinds,
};
use diesel::{
    backend::Backend,
    deserialize::{self, FromSqlRow},
    expression::AsExpression,
    prelude::*,
    serialize,
    sql_types::SmallInt,
};
use serde::{Deserialize, Serialize};
use sui_field_count::FieldCount;
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

#[derive(Insertable, Debug, Clone, FieldCount, Queryable)]
#[diesel(table_name = kv_transactions)]
pub struct StoredTransaction {
    pub tx_digest: Vec<u8>,
    pub cp_sequence_number: i64,
    pub timestamp_ms: i64,
    pub raw_transaction: Vec<u8>,
    pub raw_effects: Vec<u8>,
    pub events: Vec<u8>,
    pub user_signatures: Vec<u8>,
}

#[derive(Insertable, Debug, Clone, FieldCount, Queryable)]
#[diesel(table_name = tx_affected_addresses)]
pub struct StoredTxAffectedAddress {
    pub tx_sequence_number: i64,
    /// Address affected by the transaction, including the sender, the gas payer
    /// and any recipients of objects.
    pub affected: Vec<u8>,
    pub sender: Vec<u8>,
}

#[derive(Insertable, Debug, Clone, FieldCount, Queryable)]
#[diesel(table_name = tx_affected_objects)]
pub struct StoredTxAffectedObject {
    pub tx_sequence_number: i64,
    /// Object affected by the transaction, including deleted, wrapped, mutated,
    /// and created objects.
    pub affected: Vec<u8>,
    pub sender: Vec<u8>,
}

#[derive(Insertable, Selectable, Debug, Clone, FieldCount, Queryable)]
#[diesel(table_name = tx_balance_changes)]
pub struct StoredTxBalanceChange {
    pub tx_sequence_number: i64,
    pub balance_changes: Vec<u8>,
}

#[derive(Insertable, Debug, Clone, FieldCount, Queryable)]
#[diesel(table_name = tx_calls)]
pub struct StoredTxCalls {
    pub package: Vec<u8>,
    pub module: String,
    pub function: String,
    pub tx_sequence_number: i64,
    pub sender: Vec<u8>,
}

#[derive(Insertable, Debug, Clone, FieldCount, Queryable)]
#[diesel(table_name = tx_digests)]
pub struct StoredTxDigest {
    pub tx_sequence_number: i64,
    pub tx_digest: Vec<u8>,
}

#[derive(AsExpression, FromSqlRow, Copy, Clone, Debug)]
#[diesel(sql_type = SmallInt)]
#[repr(i16)]
pub enum StoredKind {
    SystemTransaction = 0,
    ProgrammableTransaction = 1,
}

#[derive(Insertable, Debug, Clone, FieldCount, Queryable)]
#[diesel(table_name = tx_kinds)]
pub struct StoredTxKind {
    pub tx_sequence_number: i64,
    pub tx_kind: StoredKind,
}

impl<DB: Backend> serialize::ToSql<SmallInt, DB> for StoredKind
where
    i16: serialize::ToSql<SmallInt, DB>,
{
    fn to_sql<'b>(&'b self, out: &mut serialize::Output<'b, '_, DB>) -> serialize::Result {
        match self {
            StoredKind::SystemTransaction => 0.to_sql(out),
            StoredKind::ProgrammableTransaction => 1.to_sql(out),
        }
    }
}

impl<DB: Backend> deserialize::FromSql<SmallInt, DB> for StoredKind
where
    i16: deserialize::FromSql<SmallInt, DB>,
{
    fn from_sql(raw: DB::RawValue<'_>) -> deserialize::Result<Self> {
        Ok(match i16::from_sql(raw)? {
            0 => StoredKind::SystemTransaction,
            1 => StoredKind::ProgrammableTransaction,
            k => return Err(format!("Unexpected StoredTxKind: {k}").into()),
        })
    }
}
