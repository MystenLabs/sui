// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
#![allow(dead_code)]

use crate::{ParquetSchema, ParquetValue};
use serde::Serialize;
use strum_macros::Display;
use sui_analytics_indexer_derive::SerializeParquet;
// use std::collections::BTreeSet;

//
// Table entries for the analytics database.
// Each entry is a row in the database.
//

// Checkpoint information.
#[derive(Serialize, Clone, SerializeParquet)]
pub(crate) struct CheckpointEntry {
    // indexes
    pub(crate) checkpoint_digest: String,
    pub(crate) sequence_number: u64,
    pub(crate) epoch: u64,
    pub(crate) timestamp_ms: u64,

    pub(crate) previous_checkpoint_digest: Option<String>,
    pub(crate) end_of_epoch: bool,
    // gas stats
    pub(crate) total_gas_cost: i64,
    pub(crate) computation_cost: u64,
    pub(crate) storage_cost: u64,
    pub(crate) storage_rebate: u64,
    pub(crate) non_refundable_storage_fee: u64,
    // transaction stats
    pub(crate) total_transaction_blocks: u64,
    pub(crate) total_transactions: u64,
    pub(crate) total_successful_transaction_blocks: u64,
    pub(crate) total_successful_transactions: u64,

    pub(crate) network_total_transaction: u64,
    pub(crate) validator_signature: String,
}

// Transaction information.
#[derive(Serialize, Clone, SerializeParquet)]
pub(crate) struct TransactionEntry {
    // main indexes
    pub(crate) transaction_digest: String,
    pub(crate) checkpoint: u64,
    pub(crate) epoch: u64,
    pub(crate) timestamp_ms: u64,
    // transaction info
    pub(crate) sender: String,
    pub(crate) transaction_kind: String,
    pub(crate) is_system_txn: bool,
    pub(crate) is_sponsored_tx: bool,
    pub(crate) transaction_count: u64,
    pub(crate) execution_success: bool,
    // object info
    pub(crate) input: u64,
    pub(crate) shared_input: u64,
    pub(crate) gas_coins: u64,
    // objects are broken up in created, mutated and deleted.
    // No wrap or unwrap information is provided
    pub(crate) created: u64,
    pub(crate) mutated: u64,
    pub(crate) deleted: u64,
    // PTB info
    pub(crate) transfers: u64,
    pub(crate) split_coins: u64,
    pub(crate) merge_coins: u64,
    pub(crate) publish: u64,
    pub(crate) upgrade: u64,
    // move_vec or default for future commands
    pub(crate) others: u64,
    pub(crate) move_calls: u64,
    // pub(crate) packages: BTreeSet<String>,
    // commas separated list of packages used by the transaction.
    // Use as a simple way to query for transactions that use a specific package.
    pub(crate) packages: String,
    // gas info
    pub(crate) gas_owner: String,
    pub(crate) gas_object_id: String,
    pub(crate) gas_object_sequence: u64,
    pub(crate) gas_object_digest: String,
    pub(crate) gas_budget: u64,
    pub(crate) total_gas_cost: i64,
    pub(crate) computation_cost: u64,
    pub(crate) storage_cost: u64,
    pub(crate) storage_rebate: u64,
    pub(crate) non_refundable_storage_fee: u64,
    pub(crate) gas_price: u64,
    // raw transaction bytes
    // pub(crate) raw_transaction: Vec<u8>,
    // We represent them in base64 encoding so they work with the csv.
    // TODO: review and possibly move back to Vec<u8>
    pub(crate) raw_transaction: String,
}

// Event information.
// Events identity is via `transaction_digest` and `event_index`.
#[derive(Serialize, Clone, SerializeParquet)]
pub(crate) struct EventEntry {
    // indexes
    pub(crate) transaction_digest: String,
    pub(crate) event_index: u64,
    pub(crate) checkpoint: u64,
    pub(crate) epoch: u64,
    pub(crate) timestamp_ms: u64,
    // sender
    pub(crate) sender: String,
    // event type
    pub(crate) package: String,
    pub(crate) module: String,
    pub(crate) event_type: String,
    // raw event bytes
    // pub(crate) bcs: Vec<u8>,
    // We represent them in base64 encoding so they work with the csv.
    // TODO: review and possibly move back to Vec<u8>
    pub(crate) bcs: String,
}

// Used in the transaction object table to identify the type of input object.
#[derive(Serialize, Clone, Display)]
pub enum InputObjectKind {
    Input,
    SharedInput,
    GasCoin,
}

// Used in the object table to identify the status of object, its result in the last transaction
// effect.
#[derive(Serialize, Clone, Display)]
pub enum ObjectStatus {
    Created,
    Mutated,
    Deleted,
}

// Object owner information.
#[derive(Serialize, Clone, Display)]
pub enum OwnerType {
    AddressOwner,
    ObjectOwner,
    Shared,
    Immutable,
}

// Object information.
// A row in the live object table.
#[derive(Serialize, Clone, SerializeParquet)]
pub(crate) struct ObjectEntry {
    // indexes
    pub(crate) object_id: String,
    pub(crate) version: u64,
    pub(crate) digest: String,
    pub(crate) type_: Option<String>, // None is for packages
    pub(crate) checkpoint: u64,
    pub(crate) epoch: u64,
    pub(crate) timestamp_ms: u64,
    // owner info
    pub(crate) owner_type: OwnerType,
    pub(crate) owner_address: Option<String>,
    // object info
    pub(crate) object_status: ObjectStatus,
    pub(crate) initial_shared_version: Option<u64>,
    pub(crate) previous_transaction: String,
    pub(crate) has_public_transfer: bool,
    pub(crate) storage_rebate: u64,
    // raw object bytes
    // pub(crate) bcs: Vec<u8>,
    // We represent them in base64 encoding so they work with the csv.
    // TODO: review and possibly move back to Vec<u8>
    pub(crate) bcs: String,
}

// Objects used and manipulated in a transaction.
// Both input object and objects in effects are reported here with the proper
// input kind (for input objects) and status (for objets in effects).
// An object may appear twice as an input and output object. In that case, the
// version will be different.
#[derive(Serialize, Clone, SerializeParquet)]
pub(crate) struct TransactionObjectEntry {
    // indexes
    pub(crate) object_id: String,
    pub(crate) version: Option<u64>,
    pub(crate) transaction_digest: String,
    pub(crate) checkpoint: u64,
    pub(crate) epoch: u64,
    pub(crate) timestamp_ms: u64,
    // input/output information
    pub(crate) input_kind: Option<InputObjectKind>,
    pub(crate) object_status: Option<ObjectStatus>,
}

// A Move call expressed as a package, module and function.
#[derive(Serialize, Clone, SerializeParquet)]
pub(crate) struct MoveCallEntry {
    // indexes
    pub(crate) transaction_digest: String,
    pub(crate) checkpoint: u64,
    pub(crate) epoch: u64,
    pub(crate) timestamp_ms: u64,
    // move call info
    pub(crate) package: String,
    pub(crate) module: String,
    pub(crate) function: String,
}

// A Move package. Pacakge id and MovePackage object bytes
#[derive(Serialize, Clone, SerializeParquet)]
pub(crate) struct MovePackageEntry {
    // indexes
    pub(crate) package_id: String,
    pub(crate) checkpoint: u64,
    pub(crate) epoch: u64,
    pub(crate) timestamp_ms: u64,
    // raw package bytes
    // pub(crate) bcs: Vec<u8>,
    // We represent them in base64 encoding so they work with the csv.
    // TODO: review and possibly move back to Vec<u8>
    pub(crate) bcs: String,
}
