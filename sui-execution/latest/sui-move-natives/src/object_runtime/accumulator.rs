// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use move_core_types::{account_address::AccountAddress, language_storage::StructTag};
use move_vm_types::{loaded_data::runtime_types::Type, values::Value};
use sui_types::accumulator_event::AccumulatorAction;

pub enum MoveAccumulatorAction {
    Merge,
    Split,
}

impl MoveAccumulatorAction {
    pub fn into_sui_accumulator_action(self) -> AccumulatorAction {
        match self {
            MoveAccumulatorAction::Merge => AccumulatorAction::Merge,
            MoveAccumulatorAction::Split => AccumulatorAction::Split,
        }
    }
}

pub enum MoveAccumulatorValue {
    MoveValue(Type, StructTag, Value),
    // commit the nth event emitted by the transaction to an event stream
    EventRef(u64),
}

pub struct MoveAccumulatorEvent {
    pub action: MoveAccumulatorAction,
    pub target: AccountAddress,
    pub value: MoveAccumulatorValue,
}
