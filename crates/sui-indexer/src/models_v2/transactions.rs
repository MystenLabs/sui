// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use diesel::prelude::*;

use crate::schema_v2::transactions;
use crate::types_v2::IndexedTransaction;

#[derive(Clone, Debug, Queryable, Insertable, QueryableByName)]
#[diesel(table_name = transactions)]
pub struct StoredTransaction {
    pub tx_sequence_number: i64,
    pub transaction_digest: Vec<u8>,
    pub raw_transaction: Vec<u8>,
    pub raw_effects: Vec<u8>,
    pub checkpoint_sequence_number: i64,
    pub timestamp_ms: i64,
    pub object_changes: Vec<Option<Vec<u8>>>,
    pub balance_changes: Vec<Option<Vec<u8>>>,
    pub events: Vec<Option<Vec<u8>>>,
    pub transaction_kind: i16,
}

impl From<&IndexedTransaction> for StoredTransaction {
    fn from(tx: &IndexedTransaction) -> Self {
        StoredTransaction {
            tx_sequence_number: tx.tx_sequence_number as i64,
            transaction_digest: tx.tx_digest.into_inner().to_vec(),
            raw_transaction: bcs::to_bytes(&tx.sender_signed_data).unwrap(),
            raw_effects: bcs::to_bytes(&tx.effects).unwrap(),
            checkpoint_sequence_number: tx.checkpoint_sequence_number as i64,
            object_changes: vec![],
            balance_changes: vec![],
            events: vec![],
            // object_changes: tx
            //     .object_changes
            //     .iter()
            //     .map(|oc| Some(bcs::to_bytes(&oc).unwrap()))
            //     .collect(),
            // balance_changes: tx
            //     .balance_change
            //     .iter()
            //     .map(|bc| Some(bcs::to_bytes(&bc).unwrap()))
            //     .collect(),
            // events: tx
            //     .events
            //     .iter()
            //     .map(|e| Some(bcs::to_bytes(&e).unwrap()))
            //     .collect(),
            transaction_kind: tx.transaction_kind.clone() as i16,
            timestamp_ms: tx.timestamp_ms as i64,
        }
    }
}
