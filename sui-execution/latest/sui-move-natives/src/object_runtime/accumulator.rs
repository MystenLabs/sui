// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use move_core_types::{account_address::AccountAddress, language_storage::StructTag};
use move_vm_types::{loaded_data::runtime_types::Type, values::Value};
use sui_types::{base_types::ObjectID, effects::AccumulatorOperation};

#[derive(Debug)]
pub enum MoveAccumulatorAction {
    Merge,
    Split,
}

impl MoveAccumulatorAction {
    pub fn into_sui_accumulator_action(self) -> AccumulatorOperation {
        match self {
            MoveAccumulatorAction::Merge => AccumulatorOperation::Merge,
            MoveAccumulatorAction::Split => AccumulatorOperation::Split,
        }
    }
}

#[derive(Debug)]
pub enum MoveAccumulatorValue {
    MoveValue(Type, StructTag, Value),
}

#[derive(Debug)]
pub struct MoveAccumulatorEvent {
    // Note: accumulator_id is derived by hashing target and ty, but we include
    // both for simplicity.
    pub accumulator_id: ObjectID,
    pub action: MoveAccumulatorAction,
    pub target_addr: AccountAddress,
    pub target_ty: Type,
    pub value: MoveAccumulatorValue,
}
