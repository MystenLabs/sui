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
    pub input_objects: Vec<u8>,
    // pub changed_objects: Vec<Option<Vec<u8>>>,
    pub changed_objects: Vec<u8>,
    // pub senders: Vec<Option<Vec<u8>>>,
    pub senders: Vec<u8>,
    // pub payers: Vec<Option<Vec<u8>>>,
    pub payers: Vec<u8>,
    // pub recipients: Vec<Option<String>>,
    pub recipients: Vec<u8>,
    // pub packages: Vec<Option<Vec<u8>>>,
    pub packages: Vec<u8>,
    // pub package_modules: Vec<Option<String>>,
    pub package_modules: Vec<u8>,
    // pub package_module_functions: Vec<Option<String>>,
    pub package_module_functions: Vec<u8>,
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

        let flattened_input_objects: Vec<u8> = input_objects
            .into_iter()
            .filter_map(|opt| opt)
            .flatten()
            .collect();
        let flattened_changed_objects: Vec<u8> = changed_objects
            .into_iter()
            .filter_map(|opt| opt)
            .flatten()
            .collect();
        let flattened_packages: Vec<u8> = packages
            .into_iter()
            .filter_map(|opt| opt)
            .flatten()
            .collect();
        let flattened_package_modules: Vec<u8> = package_modules
            .into_iter()
            .filter_map(|opt| opt)
            .map(|s| s.into_bytes())
            .flatten()
            .collect();
        let flattened_package_module_functions: Vec<u8> = package_module_functions
            .into_iter()
            .filter_map(|opt| opt)
            .map(|s| s.into_bytes())
            .flatten()
            .collect();
        let flattened_payers: Vec<u8> =
            payers.into_iter().filter_map(|opt| opt).flatten().collect();
        let flattened_senders: Vec<u8> = senders
            .into_iter()
            .filter_map(|opt| opt)
            .flatten()
            .collect();
        let flattened_recipients: Vec<u8> = recipients
            .into_iter()
            .filter_map(|opt| opt)
            .flatten()
            .collect();

        StoredTxIndex {
            tx_sequence_number: tx.tx_sequence_number as i64,
            checkpoint_sequence_number: tx.checkpoint_sequence_number as i64,
            transaction_digest: tx.transaction_digest.to_string(),
            input_objects: flattened_input_objects,
            changed_objects: flattened_changed_objects,
            payers: flattened_payers,
            senders: flattened_senders,
            recipients: flattened_recipients,
            packages: flattened_packages,
            package_modules: flattened_package_modules,
            package_module_functions: flattened_package_module_functions,
        }
    }
}
