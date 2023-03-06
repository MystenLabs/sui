// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use sui_types::base_types::ObjectID;
use sui_types::dynamic_field::DynamicFieldInfo;

pub use sui_event::*;
pub use sui_object::*;
pub use sui_transaction::*;

pub use sui_bls::*;
pub use sui_checkpoint::*;
pub use sui_coin::*;
pub use sui_governance::*;
pub use sui_move::*;

#[cfg(test)]
#[path = "unit_tests/rpc_types_tests.rs"]
mod rpc_types_tests;

mod sui_bls;
mod sui_checkpoint;
mod sui_coin;
mod sui_event;
mod sui_governance;
mod sui_move;
mod sui_object;
mod sui_transaction;

pub type DynamicFieldPage = Page<DynamicFieldInfo, ObjectID>;

#[derive(Clone, Debug, JsonSchema, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Page<T, C> {
    pub data: Vec<T>,
    pub next_cursor: Option<C>,
}
