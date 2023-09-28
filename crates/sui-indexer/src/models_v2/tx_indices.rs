// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::{schema_v2::tx_indices, types_v2::TxIndex};
use diesel::prelude::*;

#[derive(Queryable, Insertable, Debug, Clone, Default)]
#[diesel(table_name = tx_indices)]
pub struct StoredTxIndex {
    pub tx_sequence_number: i64,
    pub checkpoint_sequence_number: i64,
    pub transaction_digest: String,
    // pub input_objects: Vec<Option<Vec<u8>>>,
    pub input_objects: serde_json::Value,
    // pub changed_objects: Vec<Option<Vec<u8>>>,
    pub changed_objects: serde_json::Value,
    // pub senders: Vec<Option<Vec<u8>>>,
    pub senders: serde_json::Value,
    // pub payers: Vec<Option<Vec<u8>>>,
    pub payers: serde_json::Value,
    // pub recipients: Vec<Option<String>>,
    pub recipients: serde_json::Value,
    // pub packages: Vec<Option<Vec<u8>>>,
    pub packages: serde_json::Value,
    // pub package_modules: Vec<Option<String>>,
    pub package_modules: serde_json::Value,
    // pub package_module_functions: Vec<Option<String>>,
    pub package_module_functions: serde_json::Value,
}

impl From<TxIndex> for StoredTxIndex {
    fn from(tx: TxIndex) -> Self {
        let input_objects: Vec<Option<Vec<u8>>> = tx
            .input_objects
            .iter()
            .map(|o| Some(bcs::to_bytes(&o).unwrap()))
            .collect();
        let changed_objects: Vec<Option<Vec<u8>>> = tx
            .changed_objects
            .iter()
            .map(|o| Some(bcs::to_bytes(&o).unwrap()))
            .collect();

        let packages: Vec<Option<Vec<u8>>> = tx
            .move_calls
            .iter()
            .map(|(p, _m, _f)| Some(p.to_vec()))
            .collect();
        let package_modules: Vec<Option<String>> = tx
            .move_calls
            .iter()
            .map(|(p, m, _f)| Some(format!("{}::{}", p, m)))
            .collect();
        let package_module_functions: Vec<Option<String>> = tx
            .move_calls
            .iter()
            .map(|(p, m, f)| Some(format!("{}::{}::{}", p, m, f)))
            .collect();

        let payers: Vec<Option<Vec<u8>>> = tx.payers.iter().map(|s| Some(s.to_vec())).collect();
        let senders: Vec<Option<Vec<u8>>> = tx.senders.iter().map(|s| Some(s.to_vec())).collect();
        let recipients: Vec<Option<Vec<u8>>> =
            tx.recipients.iter().map(|r| Some(r.to_vec())).collect();

        StoredTxIndex {
            tx_sequence_number: tx.tx_sequence_number as i64,
            checkpoint_sequence_number: tx.checkpoint_sequence_number as i64,
            transaction_digest: tx.transaction_digest.to_string(),
            input_objects: serde_json::json!(input_objects),
            changed_objects: serde_json::json!(changed_objects),
            payers: serde_json::json!(payers),
            senders: serde_json::json!(senders),
            recipients: serde_json::json!(recipients),
            packages: serde_json::json!(packages),
            package_modules: serde_json::json!(package_modules),
            package_module_functions: serde_json::json!(package_module_functions),
        }
    }
}
