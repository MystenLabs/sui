// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// use crate::schema::{changed_objects, input_objects, move_calls, recipients};
use crate::schema::tx_indices;
use diesel::prelude::*;
use sui_types::{
    base_types::{ObjectID, SuiAddress},
    digests::TransactionDigest,
    transaction::ProgrammableMoveCall,
};
#[derive(Queryable, Insertable, Debug, Clone, Default)]
#[diesel(table_name = tx_indices)]
pub struct StoredTxIndex {
    pub tx_sequence_number: i64,
    pub transaction_digest: Vec<u8>,
    pub input_objects: Vec<Vec<u8>>,
    pub changed_objects: Vec<Vec<u8>>,
    pub senders: Vec<Vec<u8>>,
    pub recipients: Vec<Vec<u8>>,
    pub packages: Vec<Vec<u8>>,
    pub package_modules: Vec<String>,
    pub package_module_functions: Vec<String>,
}

#[derive(Debug)]
pub struct TxIndex {
    pub tx_sequence_number: u64,
    pub transaction_digest: TransactionDigest,
    pub input_objects: Vec<ObjectID>,
    pub changed_objects: Vec<ObjectID>,
    pub senders: Vec<SuiAddress>,
    pub recipients: Vec<SuiAddress>,
    pub move_calls: Vec<(ObjectID, String, String)>,
}

// #[derive(Queryable, Insertable, Debug, Clone, Default)]
// #[diesel(table_name = move_calls)]
// pub struct MoveCall {
//     pub id: Option<i64>,
//     pub transaction_digest: String,
//     pub checkpoint_sequence_number: i64,
//     pub epoch: i64,
//     pub sender: String,
//     pub move_package: String,
//     pub move_module: String,
//     pub move_function: String,
// }

// #[derive(Queryable, Insertable, Debug, Clone, Default)]
// #[diesel(table_name = recipients)]
// pub struct Recipient {
//     pub id: Option<i64>,
//     pub transaction_digest: String,
//     pub checkpoint_sequence_number: i64,
//     pub epoch: i64,
//     pub sender: String,
//     pub recipient: String,
// }

// #[derive(Queryable, Insertable, Debug, Clone, Default)]
// #[diesel(table_name = changed_objects)]
// pub struct ChangedObject {
//     pub id: Option<i64>,
//     pub transaction_digest: String,
//     pub checkpoint_sequence_number: i64,
//     pub epoch: i64,
//     pub object_id: String,
//     // object_change_type could be `mutated`, `created` or `unwrapped`.
//     pub object_change_type: String,
//     pub object_version: i64,
// }
