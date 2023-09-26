// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::{schema_v2::tx_indices, types_v2::TxIndex};
use diesel::prelude::*;

#[derive(Queryable, Insertable, Debug, Clone, Default)]
#[diesel(table_name = tx_indices)]
pub struct StoredTxIndex {
    pub tx_sequence_number: i64,
    pub checkpoint_sequence_number: i64,
    pub transaction_digest: Vec<u8>,
    pub input_objects: Vec<Option<Vec<u8>>>,
    pub changed_objects: Vec<Option<Vec<u8>>>,
    pub senders: Vec<Option<Vec<u8>>>,
    pub payers: Vec<Option<Vec<u8>>>,
    pub recipients: Vec<Option<Vec<u8>>>,
    pub packages: Vec<Option<Vec<u8>>>,
    pub package_modules: Vec<Option<String>>,
    pub package_module_functions: Vec<Option<String>>,
}

impl From<TxIndex> for StoredTxIndex {
    fn from(tx: TxIndex) -> Self {
        StoredTxIndex {
            tx_sequence_number: tx.tx_sequence_number as i64,
            checkpoint_sequence_number: tx.checkpoint_sequence_number as i64,
            transaction_digest: tx.transaction_digest.into_inner().to_vec(),
            input_objects: tx
                .input_objects
                .iter()
                .map(|o| Some(bcs::to_bytes(&o).unwrap()))
                .collect(),
            changed_objects: tx
                .changed_objects
                .iter()
                .map(|o| Some(bcs::to_bytes(&o).unwrap()))
                .collect(),
            payers: tx.payers.iter().map(|s| Some(s.to_vec())).collect(),
            senders: tx.senders.iter().map(|s| Some(s.to_vec())).collect(),
            recipients: tx.recipients.iter().map(|r| Some(r.to_vec())).collect(),
            packages: tx
                .move_calls
                .iter()
                .map(|(p, _m, _f)| Some(p.to_vec()))
                .collect(),
            package_modules: tx
                .move_calls
                .iter()
                .map(|(p, m, _f)| Some(format!("{}::{}", p, m)))
                .collect(),
            package_module_functions: tx
                .move_calls
                .iter()
                .map(|(p, m, f)| Some(format!("{}::{}::{}", p, m, f)))
                .collect(),
        }
    }
}
