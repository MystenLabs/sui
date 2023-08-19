// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use axum::response::sse::Event;
use diesel::prelude::*;

use sui_json_rpc_types::{
    BalanceChange, ObjectChange, SuiTransactionBlockDataAPI, SuiTransactionBlockEffectsAPI,
};
use sui_types::digests::TransactionDigest;
use sui_types::effects::{TransactionEffects, TransactionEvents};
use sui_types::transaction::TransactionData;

use crate::errors::IndexerError;
use crate::schema::transactions;
use crate::types::TemporaryTransactionBlockResponseStore;

#[derive(Copy, Clone, Debug)]
pub enum TransactionKind {
    SystemTransaction = 0,
    ProgrammableTransaction = 1,
}

#[derive(Clone, Debug, Queryable, Insertable, QueryableByName)]
#[diesel(table_name = transactions)]
pub struct StoredTransaction {
    pub tx_sequence_number: i64,
    pub transaction_digest: Vec<u8>,
    pub raw_transaction: Vec<u8>,
    pub raw_effects: Vec<u8>,
    pub checkpoint_sequence_number: i64,
    pub timestamp_ms: i64,
    pub object_changes: Vec<Vec<u8>>,
    pub balance_changes: Vec<Vec<u8>>,
    pub events: Vec<Vec<u8>>,
    pub transaction_kind: i16,
}

#[derive(Debug)]
pub struct IndexedTransaction {
    pub tx_sequence_number: u64,
    pub tx_digest: TransactionDigest,
    pub transaction: TransactionData,
    pub effects: TransactionEffects,
    pub checkpoint_sequence_number: u64,
    pub timestamp_ms: u64,
    pub object_changes: Vec<sui_json_rpc_types::ObjectChange>,
    pub balance_change: Vec<sui_json_rpc_types::BalanceChange>,
    pub events: Vec<sui_types::event::Event>,
    pub transaction_kind: TransactionKind,
    pub successful_tx_num: u64,
}

impl From<&IndexedTransaction> for StoredTransaction {
    fn from(tx: &IndexedTransaction) -> Self {
        StoredTransaction {
            tx_sequence_number: tx.tx_sequence_number as i64,
            transaction_digest: tx.tx_digest.clone().into_inner().to_vec(),
            raw_transaction: bcs::to_bytes(&tx.transaction).unwrap(),
            raw_effects: bcs::to_bytes(&tx.effects).unwrap(),
            checkpoint_sequence_number: tx.checkpoint_sequence_number as i64,
            object_changes: tx
                .object_changes
                .iter()
                .map(|oc| bcs::to_bytes(&oc).unwrap())
                .collect(),
            balance_changes: tx
                .balance_change
                .iter()
                .map(|bc| bcs::to_bytes(&bc).unwrap())
                .collect(),
            events: tx
                .events
                .iter()
                .map(|e| bcs::to_bytes(&e).unwrap())
                .collect(),
            transaction_kind: tx.transaction_kind as i16,
            timestamp_ms: tx.timestamp_ms as i64,
        }
    }
}

// impl TryFrom<TemporaryTransactionBlockResponseStore> for Transaction {
//     type Error = IndexerError;

//     fn try_from(tx_resp: TemporaryTransactionBlockResponseStore) -> Result<Self, Self::Error> {
//         let TemporaryTransactionBlockResponseStore {
//             tx_sequence_number,
//             digest,
//             transaction,
//             raw_transaction,
//             effects,
//             events,
//             object_changes,
//             balance_changes,
//             timestamp_ms,
//             checkpoint,
//         } = tx_resp;

//         let transaction_kind = if transaction.data.transaction().is_system_transaction() {
//             TransactionKind::SystemTransaction
//         } else {
//             TransactionKind::ProgrammableTransaction
//         };
//         Ok(Transaction {
//             tx_sequence_number,
//             transaction_digest: digest.into_inner(),
//             raw_transaction,
//             raw_effects: bcs::to_bytes(&effects),
//             checkpoint_sequence_number: checkpoint as i64,
//             timestamp_ms: timestamp_ms.map(|ts| ts as i64),
//             object_changes: object_changes.into_iter().map(|oc| bcs::to_bytes(&oc)).collect(),
//             balance_changes: balance_changes.into_iter().map(|bc| bcs::to_bytes(&bc)).collect(),
//             events: events.into_iter().map(|e| bcs::to_bytes(&e)).collect(),
//             transaction_kind: transaction_kind as i16,
//         })
//     }
// }
