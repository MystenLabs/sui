// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::schema::{kv_transactions, tx_affected_objects, tx_balance_changes};
use diesel::prelude::*;
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

#[derive(Insertable, Debug, Clone, FieldCount)]
#[diesel(table_name = kv_transactions)]
pub struct StoredTransaction {
    pub tx_digest: Vec<u8>,
    pub cp_sequence_number: i64,
    pub timestamp_ms: i64,
    pub raw_transaction: Vec<u8>,
    pub raw_effects: Vec<u8>,
    pub events: Vec<u8>,
}

#[derive(Insertable, Debug, Clone, FieldCount)]
#[diesel(table_name = tx_affected_objects)]
pub struct StoredTxAffectedObject {
    pub tx_sequence_number: i64,
    pub affected: Vec<u8>,
    pub sender: Vec<u8>,
}

#[derive(Insertable, Debug, Clone, FieldCount)]
#[diesel(table_name = tx_balance_changes)]
pub struct StoredTxBalanceChange {
    pub tx_sequence_number: i64,
    pub balance_changes: Vec<u8>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_stored_transaction_field_count() {
        assert_eq!(StoredTransaction::field_count(), 6);
    }

    #[test]
    fn test_stored_tx_affected_object_field_count() {
        assert_eq!(StoredTxAffectedObject::field_count(), 3);
    }

    #[test]
    fn test_stored_tx_balance_change_field_count() {
        assert_eq!(StoredTxBalanceChange::field_count(), 2);
    }
}
