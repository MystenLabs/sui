// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

pub use balance_changes::*;
pub use object_changes::*;
pub use sui_checkpoint::*;
pub use sui_coin::*;
pub use sui_event::*;
pub use sui_extended::*;
pub use sui_governance::*;
pub use sui_move::*;
pub use sui_object::*;
pub use sui_protocol::*;
pub use sui_transaction::*;
use sui_types::base_types::ObjectID;
use sui_types::dynamic_field::DynamicFieldInfo;

#[cfg(test)]
#[path = "unit_tests/rpc_types_tests.rs"]
mod rpc_types_tests;

mod balance_changes;
mod object_changes;
mod sui_checkpoint;
mod sui_coin;
mod sui_event;
mod sui_extended;
mod sui_governance;
mod sui_move;
mod sui_object;
mod sui_protocol;
mod sui_transaction;

pub type DynamicFieldPage = Page<DynamicFieldInfo, ObjectID>;
/// `next_cursor` points to the last item in the page;
/// Reading with `next_cursor` will start from the next item after `next_cursor` if
/// `next_cursor` is `Some`, otherwise it will start from the first item.
#[derive(Clone, Debug, JsonSchema, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Page<T, C> {
    pub data: Vec<T>,
    pub next_cursor: Option<C>,
    pub has_next_page: bool,
}
